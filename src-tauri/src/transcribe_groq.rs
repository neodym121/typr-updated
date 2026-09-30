use reqwest::multipart;
use std::path::PathBuf;
use std::time::Instant;

use crate::net;

const GROQ_URL: &str = "https://api.groq.com/openai/v1/audio/transcriptions";
const GROQ_MODEL: &str = "whisper-large-v3-turbo";

pub async fn transcribe_groq(api_key: &str, audio_path: &PathBuf) -> Result<String, String> {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return Err("Groq API key not set. Please enter your API key in settings.".to_string());
    }

    let audio_bytes = std::fs::read(audio_path)
        .map_err(|e| format!("Failed to read audio file: {}", e))?;
    let size_kb = audio_bytes.len() / 1024;

    let file_part = multipart::Part::bytes(audio_bytes)
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .map_err(|e| e.to_string())?;

    let form = multipart::Form::new()
        .text("model", GROQ_MODEL)
        .text("response_format", "json")
        .part("file", file_part);

    log::info!("Groq: POST {} (model {}, {} KB)", GROQ_URL, GROQ_MODEL, size_kb);
    let started = Instant::now();
    let response = net::client()?
        .post(GROQ_URL)
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("Groq API request failed: {}", net::describe_error(&e)))?;

    net::read_transcription("Groq", response, started).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_empty_api_key() {
        let path = PathBuf::from("/tmp/test.wav");
        let result = transcribe_groq("", &path).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("API key not set"));
    }
}
