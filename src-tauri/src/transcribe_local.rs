//! Local engine: transcribes the recording on this computer with
//! transcribe.cpp (see `local`). Unlike the cloud engines it takes the
//! samples straight from memory, no WAV file is written.

use tauri::{AppHandle, Manager};

use crate::local::LocalEngine;
use crate::settings::Settings;

pub async fn transcribe_local(
    app: &AppHandle,
    settings: &Settings,
    samples: Vec<f32>,
) -> Result<String, String> {
    let engine = app
        .try_state::<LocalEngine>()
        .ok_or_else(|| "Local recognition is not initialised".to_string())?
        .inner()
        .clone();
    log::info!(
        "Local: model {}, backend {}",
        settings.local_model,
        if settings.local_backend.is_empty() { "auto" } else { settings.local_backend.as_str() }
    );
    engine
        .transcribe(&settings.local_model, &settings.local_backend, samples)
        .await
}

/// A dictation ended one way or another: start the unload countdown.
pub fn dictation_ended(app: &AppHandle, settings: &Settings) {
    if settings.engine != "local" {
        return;
    }
    if let Some(engine) = app.try_state::<LocalEngine>() {
        engine.schedule_unload(&settings.local_unload);
    }
}
