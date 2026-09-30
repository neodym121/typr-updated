use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};

use crate::audio::{self, AudioRecorder};
use crate::cleanup::cleanup_text;
use crate::paste::paste_text;
use crate::settings::Settings;
use crate::transcribe_groq;
use crate::transcribe_openai;
use crate::transcribe_polza;

/// Shorter recordings are almost always accidental taps; Whisper tends to
/// hallucinate text for them.
const MIN_RECORDING_SECS: f32 = 0.3;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub enum RecordingState {
    Ready,
    Recording,
    Transcribing,
}

fn update_overlay(app: &AppHandle, state: &RecordingState) {
    if let Some(overlay) = app.get_webview_window("overlay") {
        let class = match state {
            RecordingState::Ready => "mic",
            RecordingState::Recording => "mic recording",
            RecordingState::Transcribing => "mic transcribing",
        };
        let js = format!("document.getElementById('mic').className = '{}';", class);
        if let Err(e) = overlay.eval(&js) {
            log::debug!("Failed to update overlay: {}", e);
        }
    }
}

fn publish_state(app: &AppHandle, state: &RecordingState) {
    if let Err(e) = app.emit("recording-state", state.clone()) {
        log::warn!("Failed to emit recording-state: {}", e);
    }
    update_overlay(app, state);
}

/// Shows a failed dictation in the main window and briefly flashes the overlay.
pub fn notify_error(app: &AppHandle, message: &str) {
    if let Err(e) = app.emit("recording-error", message.to_string()) {
        log::warn!("Failed to emit recording-error: {}", e);
    }
    if let Some(overlay) = app.get_webview_window("overlay") {
        let js = "(function(){var m=document.getElementById('mic');if(!m)return;\
                  m.className='mic error';clearTimeout(window.__typrError);\
                  window.__typrError=setTimeout(function(){\
                  if(m.className==='mic error')m.className='mic';},2500);})();";
        let _ = overlay.eval(js);
    }
}

pub struct Recorder {
    state: Arc<Mutex<RecordingState>>,
    audio_recorder: Arc<Mutex<AudioRecorder>>,
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(RecordingState::Ready)),
            audio_recorder: Arc::new(Mutex::new(AudioRecorder::new())),
        }
    }

    pub fn get_state(&self) -> RecordingState {
        self.state.lock().unwrap().clone()
    }

    pub fn start_recording(&self, app: &AppHandle, mic_name: &str) -> Result<(), String> {
        {
            let mut state = self.state.lock().unwrap();
            if *state != RecordingState::Ready {
                return Err(format!("Cannot start recording while {:?}", *state));
            }

            log::info!("Starting recording (microphone: '{}')", mic_name);
            let mut recorder = self.audio_recorder.lock().unwrap();
            recorder.start(mic_name)?;
            *state = RecordingState::Recording;
        }

        publish_state(app, &RecordingState::Recording);
        log::info!("State: Ready → Recording");
        Ok(())
    }

    /// Stops the recording, transcribes it and pastes the result.
    /// The recorder always returns to `Ready`, whether this succeeds or fails.
    pub async fn stop_and_transcribe(
        &self,
        app: &AppHandle,
        settings: &Settings,
        app_dir: &PathBuf,
    ) -> Result<String, String> {
        {
            let mut state = self.state.lock().unwrap();
            if *state != RecordingState::Recording {
                return Err("Not currently recording".to_string());
            }
            *state = RecordingState::Transcribing;
        }
        publish_state(app, &RecordingState::Transcribing);
        log::info!("State: Recording → Transcribing");

        let started = Instant::now();
        let result = self.process_recording(settings, app_dir).await;

        {
            let mut state = self.state.lock().unwrap();
            *state = RecordingState::Ready;
        }
        publish_state(app, &RecordingState::Ready);

        match &result {
            Ok(text) => log::info!(
                "Dictation finished in {} ms ({} characters)",
                started.elapsed().as_millis(),
                text.chars().count()
            ),
            Err(e) => {
                log::error!(
                    "Dictation failed after {} ms: {}",
                    started.elapsed().as_millis(),
                    e
                );
                notify_error(app, e);
            }
        }
        log::info!("State: Transcribing → Ready");

        result
    }

    async fn process_recording(
        &self,
        settings: &Settings,
        app_dir: &PathBuf,
    ) -> Result<String, String> {
        // The guard is a temporary, released at the end of this statement
        let stop_result = self.audio_recorder.lock().unwrap().stop();
        let captured = stop_result?;

        if captured.duration_secs < MIN_RECORDING_SECS {
            return Err(format!(
                "Recording is too short ({:.2} s), nothing to transcribe",
                captured.duration_secs
            ));
        }
        if captured.peak <= 0.0 {
            return Err(
                "The microphone recorded pure silence. Check that the right input device is selected and not muted."
                    .to_string(),
            );
        }

        let temp_path = app_dir.join("temp_recording.wav");
        audio::write_wav(&temp_path, &captured)?;

        log::info!("Transcribing with engine '{}'", settings.engine);
        let transcription = transcribe(settings, &temp_path).await;

        if let Err(e) = std::fs::remove_file(&temp_path) {
            log::debug!("Could not remove {}: {}", temp_path.display(), e);
        }

        let raw_text = transcription?;
        log::debug!("Raw transcription: {:?}", raw_text);

        let cleaned = cleanup_text(&raw_text);
        if cleaned.is_empty() {
            log::warn!("Transcription is empty, nothing to paste");
            return Ok(cleaned);
        }
        log::info!("Transcribed text: {}", cleaned);

        paste_text(&cleaned)?;
        log::info!("Text pasted into the active window");

        Ok(cleaned)
    }
}

async fn transcribe(settings: &Settings, audio_path: &PathBuf) -> Result<String, String> {
    match settings.engine.as_str() {
        "groq" => transcribe_groq::transcribe_groq(&settings.groq_api_key, audio_path).await,
        "openai" | "openai-compatible" => {
            transcribe_openai::transcribe_openai(
                &settings.openai_endpoint,
                &settings.openai_model,
                &settings.openai_api_key,
                audio_path,
            )
            .await
        }
        "polza" => {
            transcribe_polza::transcribe_polza(
                &settings.polza_api_key,
                &settings.polza_model,
                &settings.polza_provider,
                audio_path,
            )
            .await
        }
        other => Err(format!("Unknown engine: {}", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state_is_ready() {
        let recorder = Recorder::new();
        assert_eq!(recorder.get_state(), RecordingState::Ready);
    }
}
