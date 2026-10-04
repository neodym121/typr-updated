use reqwest::multipart;
use std::time::Instant;

use crate::audio::{AudioFormat, EncodedAudio};
use crate::net;

const DEFAULT_ENDPOINT: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "whisper-1";

/// OpenAI itself takes FLAC; other OpenAI-compatible servers (whisper.cpp,
/// local Whisper servers) may read only WAV, so they get WAV.
pub fn format_for(endpoint: &str) -> AudioFormat {
    if transcriptions_url(endpoint).starts_with("https://api.openai.com/") {
        AudioFormat::Flac
    } else {
        AudioFormat::Wav
    }
}

/// `language` is an ISO 639-1 code; `None` lets the model detect it.
pub async fn transcribe_openai(
    endpoint: &str,
    model: &str,
    api_key: &str,
    language: Option<&str>,
    audio: &EncodedAudio,
) -> Result<String, String> {
    let file_part = multipart::Part::bytes(audio.bytes.clone())
        .file_name(audio.format.file_name())
        .mime_str(audio.format.mime())
        .map_err(|e| e.to_string())?;

    let model_name = if model.trim().is_empty() {
        DEFAULT_MODEL
    } else {
        model.trim()
    };

    let url = transcriptions_url(endpoint);

    let mut form = multipart::Form::new()
        .text("model", model_name.to_string())
        .text("response_format", "json")
        .part("file", file_part);
    if let Some(language) = language {
        form = form.text("language", language.to_string());
    }

    let mut request = net::client()?.post(&url);

    let api_key = api_key.trim();
    if !api_key.is_empty() {
        request = request.bearer_auth(api_key);
    }

    log::info!(
        "OpenAI Compatible: POST {} (model {}, language {}, {} KB {:?}, API key {})",
        url,
        model_name,
        language.unwrap_or("auto"),
        audio.size_kb(),
        audio.format,
        if api_key.is_empty() { "not set" } else { "set" }
    );
    let started = Instant::now();
    let response = request
        .multipart(form)
        .send()
        .await
        .map_err(|e| {
            format!(
                "OpenAI Compatible API request failed: {}",
                net::describe_error(&e)
            )
        })?;

    net::read_transcription("OpenAI Compatible", response, started).await
}

/// Accepts either a base URL (`https://host/v1`) or the full
/// `/audio/transcriptions` URL.
pub fn transcriptions_url(endpoint: &str) -> String {
    let base = if endpoint.trim().is_empty() {
        DEFAULT_ENDPOINT
    } else {
        endpoint.trim()
    };

    if base.ends_with("/audio/transcriptions") {
        base.to_string()
    } else {
        format!("{}/audio/transcriptions", base.trim_end_matches('/'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transcriptions_url() {
        assert_eq!(
            transcriptions_url(""),
            "https://api.openai.com/v1/audio/transcriptions"
        );
        assert_eq!(
            transcriptions_url("https://host/v1/"),
            "https://host/v1/audio/transcriptions"
        );
        assert_eq!(
            transcriptions_url("https://host/v1/audio/transcriptions"),
            "https://host/v1/audio/transcriptions"
        );
    }

    #[test]
    fn test_only_openai_itself_gets_flac() {
        assert_eq!(format_for(""), AudioFormat::Flac);
        assert_eq!(format_for("https://api.openai.com/v1"), AudioFormat::Flac);
        assert_eq!(format_for("http://localhost:8080/v1"), AudioFormat::Wav);
    }
}
