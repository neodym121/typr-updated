use reqwest::multipart;
use std::path::PathBuf;

/// Transcribe audio via Polza.AI aggregator.
///
/// Polza.AI is an OpenAI-compatible API aggregator (base URL `https://polza.ai/api/v1`).
/// We hit the `/v1/audio/transcriptions` endpoint, passing an optional sub-provider
/// via the alias syntax `model@provider=<provider>` when `provider` is non-empty.
pub async fn transcribe_polza(
    api_key: &str,
    model: &str,
    provider: &str,
    audio_path: &PathBuf,
) -> Result<String, String> {
    if api_key.is_empty() {
        return Err("Polza API key not set. Please enter your API key in settings.".to_string());
    }

    let audio_bytes = std::fs::read(audio_path)
        .map_err(|e| format!("Failed to read audio file: {}", e))?;

    let file_part = multipart::Part::bytes(audio_bytes)
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .map_err(|e| e.to_string())?;

    // Build the model string. If a sub-provider is specified, use Polza alias syntax.
    let model_name = if model.trim().is_empty() {
        "openai/whisper-large-v3".to_string()
    } else {
        model.trim().to_string()
    };

    let model_with_provider = if provider.trim().is_empty() {
        model_name
    } else {
        format!("{}@provider={}", model_name, provider.trim())
    };

    let form = multipart::Form::new()
        .text("model", model_with_provider)
        .text("response_format", "json")
        .part("file", file_part);

    let client = reqwest::Client::new();
    let response = client
        .post("https://polza.ai/api/v1/audio/transcriptions")
        .header("Authorization", format!("Bearer {}", api_key))
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("Polza API request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("Polza API error ({}): {}", status, body));
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Polza response: {}", e))?;

    json["text"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No 'text' field in Polza response".to_string())
}
