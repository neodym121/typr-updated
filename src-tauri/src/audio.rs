use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hound::{WavSpec, WavWriter};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Whisper works on 16 kHz mono audio
pub const TARGET_SAMPLE_RATE: u32 = 16000;

#[derive(Debug, Clone, serde::Serialize)]
pub struct MicDevice {
    pub name: String,
    pub is_default: bool,
}

pub fn list_microphones() -> Vec<MicDevice> {
    let host = cpal::default_host();
    let default_name = host
        .default_input_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_default();

    let mut devices = Vec::new();
    match host.input_devices() {
        Ok(input_devices) => {
            for device in input_devices {
                if let Ok(name) = device.name() {
                    devices.push(MicDevice {
                        is_default: name == default_name,
                        name,
                    });
                }
            }
        }
        Err(e) => log::error!("Failed to list input devices: {}", e),
    }
    devices
}

/// Audio captured by one recording, already mixed to mono and resampled to 16 kHz.
pub struct CapturedAudio {
    pub samples: Vec<f32>,
    pub duration_secs: f32,
    /// Peak absolute amplitude, 0.0..=1.0
    pub peak: f32,
}

/// Wrapper to make cpal::Stream usable across threads.
/// SAFETY: cpal::Stream on macOS (CoreAudio) is thread-safe in practice;
/// we only access it behind a Mutex to start/stop recording.
struct SendStream(#[allow(dead_code)] cpal::Stream);
unsafe impl Send for SendStream {}
unsafe impl Sync for SendStream {}

pub struct AudioRecorder {
    samples: Arc<Mutex<Vec<f32>>>,
    stream: Option<SendStream>,
    source_sample_rate: u32,
    source_channels: u16,
}

impl AudioRecorder {
    pub fn new() -> Self {
        Self {
            samples: Arc::new(Mutex::new(Vec::new())),
            stream: None,
            source_sample_rate: 48000,
            source_channels: 1,
        }
    }

    pub fn start(&mut self, mic_name: &str) -> Result<(), String> {
        // Close any previous stream and drop leftover samples
        self.stream = None;
        self.samples
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();

        let host = cpal::default_host();

        let device = if mic_name.is_empty() || mic_name == "default" {
            host.default_input_device()
                .ok_or("No default input device found")?
        } else {
            match find_input_device(&host, mic_name)? {
                Some(device) => device,
                None => {
                    log::warn!(
                        "Microphone '{}' not found, falling back to the default input device",
                        mic_name
                    );
                    host.default_input_device().ok_or(format!(
                        "Microphone '{}' not found and no default input device is available",
                        mic_name
                    ))?
                }
            }
        };
        let device_name = device.name().unwrap_or_else(|_| "<unknown>".to_string());

        // Use the device's default config instead of forcing 16kHz
        let default_config = device.default_input_config().map_err(|e| {
            format!(
                "Failed to get default input config for '{}': {}",
                device_name, e
            )
        })?;

        let sample_rate = default_config.sample_rate().0;
        let channels = default_config.channels();

        log::info!(
            "Microphone '{}': {} Hz, {} channel(s), {:?}",
            device_name,
            sample_rate,
            channels,
            default_config.sample_format()
        );

        self.source_sample_rate = sample_rate;
        self.source_channels = channels;

        let config = cpal::StreamConfig {
            channels,
            sample_rate: cpal::SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let samples = self.samples.clone();
        let stream = device
            .build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if let Ok(mut buf) = samples.lock() {
                        buf.extend_from_slice(data);
                    }
                },
                |err| {
                    log::error!("Audio stream error: {}", err);
                },
                None,
            )
            .map_err(|e| format!("Failed to open audio stream on '{}': {}", device_name, e))?;

        stream
            .play()
            .map_err(|e| format!("Failed to start audio stream: {}", e))?;
        self.stream = Some(SendStream(stream));
        log::info!("Audio capture started");
        Ok(())
    }

    /// Stops capturing and returns what was recorded.
    pub fn stop(&mut self) -> Result<CapturedAudio, String> {
        // Dropping the stream stops the capture and releases the device
        self.stream = None;

        let raw: Vec<f32> = {
            let mut buf = self
                .samples
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            std::mem::take(&mut *buf)
        };
        log::info!("Audio capture stopped, {} raw samples", raw.len());

        if raw.is_empty() {
            return Err(
                "No audio captured: the microphone delivered no data. Check the selected input device."
                    .to_string(),
            );
        }

        // Convert to mono if multi-channel
        let channels = self.source_channels.max(1) as usize;
        let mono: Vec<f32> = if channels > 1 {
            raw.chunks(channels)
                .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
                .collect()
        } else {
            raw
        };

        let duration_secs = mono.len() as f32 / self.source_sample_rate.max(1) as f32;
        let peak = mono.iter().fold(0.0f32, |acc, s| acc.max(s.abs()));

        // Downsample to 16kHz for Whisper
        let samples = resample(&mono, self.source_sample_rate, TARGET_SAMPLE_RATE);

        log::info!(
            "Recorded {:.2} s, peak level {:.1} dBFS, {} samples at 16 kHz",
            duration_secs,
            to_dbfs(peak),
            samples.len()
        );

        Ok(CapturedAudio {
            samples,
            duration_secs,
            peak,
        })
    }
}

/// Writes 16 kHz mono 16-bit PCM and returns the file size in bytes.
pub fn write_wav(path: &PathBuf, audio: &CapturedAudio) -> Result<u64, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create folder {}: {}", parent.display(), e))?;
    }

    let spec = WavSpec {
        channels: 1,
        sample_rate: TARGET_SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = WavWriter::create(path, spec)
        .map_err(|e| format!("Failed to create WAV file {}: {}", path.display(), e))?;
    for &sample in audio.samples.iter() {
        let amplitude = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        writer
            .write_sample(amplitude)
            .map_err(|e| format!("Failed to write WAV data: {}", e))?;
    }
    writer
        .finalize()
        .map_err(|e| format!("Failed to finalize WAV file: {}", e))?;

    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    log::debug!("WAV saved to {} ({} KB)", path.display(), size / 1024);
    Ok(size)
}

fn find_input_device(host: &cpal::Host, name: &str) -> Result<Option<cpal::Device>, String> {
    let mut devices = host
        .input_devices()
        .map_err(|e| format!("Failed to list input devices: {}", e))?;
    Ok(devices.find(|d| d.name().map(|n| n == name).unwrap_or(false)))
}

fn to_dbfs(amplitude: f32) -> f32 {
    if amplitude <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * amplitude.log10()
    }
}

/// Simple linear interpolation resampler
fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || samples.is_empty() {
        return samples.to_vec();
    }

    let ratio = from_rate as f64 / to_rate as f64;
    let output_len = (samples.len() as f64 / ratio) as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_idx = i as f64 * ratio;
        let idx = src_idx as usize;
        let frac = src_idx - idx as f64;

        let sample = if idx + 1 < samples.len() {
            samples[idx] as f64 * (1.0 - frac) + samples[idx + 1] as f64 * frac
        } else {
            samples[idx.min(samples.len() - 1)] as f64
        };

        output.push(sample as f32);
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resample_downsamples_by_ratio() {
        let input = vec![0.5f32; 48000];
        let output = resample(&input, 48000, 16000);
        assert_eq!(output.len(), 16000);
        assert!(output.iter().all(|&s| (s - 0.5).abs() < 1e-6));
    }

    #[test]
    fn test_resample_empty_input() {
        assert!(resample(&[], 48000, 16000).is_empty());
    }

    #[test]
    fn test_to_dbfs() {
        assert_eq!(to_dbfs(1.0), 0.0);
        assert!(to_dbfs(0.0).is_infinite());
    }
}
