use reqwest::multipart;
use std::path::PathBuf;

pub async fn transcribe_openai(
    endpoint: &str,
    model: &str,
    api_key: &str,
    audio_path: &PathBuf,
) -> Result<String, String> {
    let audio_bytes = std::fs::read(audio_path)
        .map_err(|e| format!("Failed to read audio file: {}", e))?;

    let file_part = multipart::Part::bytes(audio_bytes)
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .map_err(|e| e.to_string())?;

    let model_name = if model.trim().is_empty() {
        "whisper-1"
    } else {
        model.trim()
    };

    let base_endpoint = if endpoint.trim().is_empty() {
        "https://api.openai.com/v1"
    } else {
        endpoint.trim()
    };

    let url = if base_endpoint.ends_with("/audio/transcriptions") {
        base_endpoint.to_string()
    } else {
        format!("{}/audio/transcriptions", base_endpoint.trim_end_matches('/'))
    };

    let form = multipart::Form::new()
        .text("model", model_name.to_string())
        .text("response_format", "json")
        .part("file", file_part);

    let client = reqwest::Client::new();
    let mut request = client.post(&url);

    if !api_key.trim().is_empty() {
        request = request.header("Authorization", format!("Bearer {}", api_key.trim()));
    }

    let response = request
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("OpenAI Compatible API request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("OpenAI Compatible API error ({}): {}", status, body));
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse OpenAI response: {}", e))?;

    json["text"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No 'text' field in OpenAI response".to_string())
}
