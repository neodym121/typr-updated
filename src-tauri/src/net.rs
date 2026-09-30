//! HTTP helpers shared by the transcription clients.

use std::error::Error;
use std::time::{Duration, Instant};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Client with timeouts, so a stalled connection can never hang a dictation.
pub fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))
}

/// reqwest's own message is just "error sending request"; this adds the
/// kind of failure and the underlying cause chain.
pub fn describe_error(err: &reqwest::Error) -> String {
    let mut message = if err.is_timeout() {
        "request timed out".to_string()
    } else if err.is_connect() {
        "could not connect to the server (check your internet connection, VPN or firewall)"
            .to_string()
    } else {
        err.to_string()
    };

    let mut source = err.source();
    while let Some(cause) = source {
        message.push_str(" → ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}

/// Reads a Whisper-style JSON response (`{"text": "..."}`) and logs how it went.
pub async fn read_transcription(
    provider: &str,
    response: reqwest::Response,
    started: Instant,
) -> Result<String, String> {
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("{} response could not be read: {}", provider, describe_error(&e)))?;

    log::info!(
        "{} responded {} in {} ms",
        provider,
        status,
        started.elapsed().as_millis()
    );

    if !status.is_success() {
        return Err(format!(
            "{} API error ({}): {}",
            provider,
            status,
            shorten(&body, 600)
        ));
    }

    let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
        format!(
            "Failed to parse {} response: {} (body: {})",
            provider,
            e,
            shorten(&body, 300)
        )
    })?;

    json["text"]
        .as_str()
        .map(|text| text.to_string())
        .ok_or_else(|| {
            format!(
                "No 'text' field in {} response: {}",
                provider,
                shorten(&body, 300)
            )
        })
}

fn shorten(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        trimmed.to_string()
    } else {
        let cut: String = trimmed.chars().take(max_chars).collect();
        format!("{}…", cut)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shorten() {
        assert_eq!(shorten("  short  ", 10), "short");
        assert_eq!(shorten("abcdef", 3), "abc…");
        assert_eq!(shorten("привет", 3), "при…");
    }
}
