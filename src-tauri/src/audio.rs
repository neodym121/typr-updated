use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hound::{WavSpec, WavWriter};
use std::io::Cursor;
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

/// How a recording is packed for a cloud service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    Wav,
    /// Lossless and about half the size of WAV, so it uploads faster
    Flac,
}

impl AudioFormat {
    pub fn mime(self) -> &'static str {
        match self {
            AudioFormat::Wav => "audio/wav",
            AudioFormat::Flac => "audio/flac",
        }
    }

    pub fn file_name(self) -> &'static str {
        match self {
            AudioFormat::Wav => "audio.wav",
            AudioFormat::Flac => "audio.flac",
        }
    }
}

/// A recording as a file in memory, ready to upload.
pub struct EncodedAudio {
    pub bytes: Vec<u8>,
    pub format: AudioFormat,
}

impl EncodedAudio {
    pub fn size_kb(&self) -> usize {
        self.bytes.len() / 1024
    }
}

/// Packs 16 kHz mono samples as `format`. FLAC falls back to WAV if the
/// encoder fails, so a dictation never fails for that.
pub fn encode(samples: &[f32], format: AudioFormat) -> Result<EncodedAudio, String> {
    let started = std::time::Instant::now();
    let encoded = match format {
        AudioFormat::Flac => match encode_flac(samples) {
            Ok(bytes) => EncodedAudio { bytes, format },
            Err(e) => {
                log::warn!("FLAC encoding failed, sending WAV instead: {}", e);
                EncodedAudio { bytes: encode_wav(samples)?, format: AudioFormat::Wav }
            }
        },
        AudioFormat::Wav => EncodedAudio { bytes: encode_wav(samples)?, format },
    };
    log::debug!(
        "Audio packed as {:?}: {} KB in {} ms",
        encoded.format,
        encoded.size_kb(),
        started.elapsed().as_millis()
    );
    Ok(encoded)
}

fn to_pcm16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
}

/// 16 kHz mono 16-bit PCM WAV.
pub fn encode_wav(samples: &[f32]) -> Result<Vec<u8>, String> {
    let spec = WavSpec {
        channels: 1,
        sample_rate: TARGET_SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut buffer = Cursor::new(Vec::with_capacity(44 + samples.len() * 2));
    let mut writer = WavWriter::new(&mut buffer, spec)
        .map_err(|e| format!("Failed to start the WAV data: {}", e))?;
    for &sample in samples {
        writer
            .write_sample(to_pcm16(sample))
            .map_err(|e| format!("Failed to write WAV data: {}", e))?;
    }
    writer
        .finalize()
        .map_err(|e| format!("Failed to finish the WAV data: {}", e))?;
    Ok(buffer.into_inner())
}

/// 16 kHz mono 16-bit FLAC.
pub fn encode_flac(samples: &[f32]) -> Result<Vec<u8>, String> {
    use flacenc::component::BitRepr;
    use flacenc::error::Verify;

    let pcm: Vec<i32> = samples.iter().map(|&s| to_pcm16(s) as i32).collect();
    let config = flacenc::config::Encoder::default()
        .into_verified()
        .map_err(|(_, e)| format!("Invalid FLAC settings: {:?}", e))?;
    let source = flacenc::source::MemSource::from_samples(&pcm, 1, 16, TARGET_SAMPLE_RATE as usize);
    let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
        .map_err(|e| format!("FLAC encoding failed: {:?}", e))?;
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|e| format!("FLAC writing failed: {:?}", e))?;
    Ok(sink.into_inner())
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

    /// A second of a 440 Hz tone with a little noise, like speech it is not
    /// silent and compresses well
    fn tone() -> Vec<f32> {
        (0..TARGET_SAMPLE_RATE)
            .map(|i| {
                let t = i as f32 / TARGET_SAMPLE_RATE as f32;
                0.3 * (t * 440.0 * std::f32::consts::TAU).sin() + 0.01 * ((i * 7919 % 101) as f32 / 101.0 - 0.5)
            })
            .collect()
    }

    #[test]
    fn test_wav_in_memory() {
        let samples = tone();
        let wav = encode_wav(&samples).unwrap();
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(wav.len(), 44 + samples.len() * 2);
        let reader = hound::WavReader::new(Cursor::new(wav)).unwrap();
        assert_eq!(reader.spec().sample_rate, TARGET_SAMPLE_RATE);
        assert_eq!(reader.len() as usize, samples.len());
    }

    #[test]
    fn test_flac_is_smaller_than_wav() {
        let samples = tone();
        let flac = encode(&samples, AudioFormat::Flac).unwrap();
        assert_eq!(flac.format, AudioFormat::Flac);
        assert_eq!(&flac.bytes[..4], b"fLaC");
        assert!(flac.bytes.len() < encode_wav(&samples).unwrap().len() * 3 / 4);
        assert_eq!(flac.format.mime(), "audio/flac");
    }

    #[test]
    fn test_to_dbfs() {
        assert_eq!(to_dbfs(1.0), 0.0);
        assert!(to_dbfs(0.0).is_infinite());
    }
}
