use reqwest::multipart;
use std::path::PathBuf;
use std::time::Instant;

use crate::net;

const DEFAULT_ENDPOINT: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "whisper-1";

pub async fn transcribe_openai(
    endpoint: &str,
    model: &str,
    api_key: &str,
    audio_path: &PathBuf,
) -> Result<String, String> {
    let audio_bytes = std::fs::read(audio_path)
        .map_err(|e| format!("Failed to read audio file: {}", e))?;
    let size_kb = audio_bytes.len() / 1024;

    let file_part = multipart::Part::bytes(audio_bytes)
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .map_err(|e| e.to_string())?;

    let model_name = if model.trim().is_empty() {
        DEFAULT_MODEL
    } else {
        model.trim()
    };

    let url = transcriptions_url(endpoint);

    let form = multipart::Form::new()
        .text("model", model_name.to_string())
        .text("response_format", "json")
        .part("file", file_part);

    let client = net::client()?;
    let mut request = client.post(&url);

    let api_key = api_key.trim();
    if !api_key.is_empty() {
        request = request.bearer_auth(api_key);
    }

    log::info!(
        "OpenAI Compatible: POST {} (model {}, {} KB, API key {})",
        url,
        model_name,
        size_kb,
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
fn transcriptions_url(endpoint: &str) -> String {
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
}
