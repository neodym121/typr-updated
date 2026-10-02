//! Large-file downloads: resumable (an interrupted download continues from
//! its `.part` file), cancellable, and verified against the expected size
//! and SHA-256 before the file gets its final name.

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::net;

#[derive(Debug, Clone, PartialEq)]
pub enum DownloadError {
    Cancelled,
    Failed(String),
}

impl From<String> for DownloadError {
    fn from(message: String) -> Self {
        DownloadError::Failed(message)
    }
}

pub struct Expected {
    pub size: u64,
    /// Hex SHA-256
    pub sha256: &'static str,
}

/// No overall timeout (a model takes minutes), only for connecting and for
/// a stalled connection.
pub fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(60))
        .user_agent(concat!("Typr/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))
}

/// Downloads `url` into `part` (resuming what is already there) and checks
/// it. `progress` gets the bytes so far and the total. On success the
/// verified file is still at `part`; the caller moves it.
pub async fn fetch(
    client: &reqwest::Client,
    url: &str,
    part: &Path,
    expected: &Expected,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, u64),
) -> Result<(), DownloadError> {
    if let Some(parent) = part.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
    }

    let mut existing = tokio::fs::metadata(part).await.map(|m| m.len()).unwrap_or(0);
    if existing > expected.size {
        existing = 0;
    }

    let mut hasher = Sha256::new();
    if existing > 0 {
        hash_existing(part, existing, &mut hasher, cancel).await?;
    }

    let mut downloaded = existing;
    let complete = existing == expected.size;
    if !complete {
        let mut request = client.get(url);
        if existing > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={}-", existing));
        }
        let response = request
            .send()
            .await
            .map_err(|e| format!("Download failed: {}", net::describe_error(&e)))?;
        let status = response.status();

        let resumed = status == reqwest::StatusCode::PARTIAL_CONTENT;
        if !status.is_success() {
            if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
                // The partial file is unusable; the next attempt starts over
                let _ = tokio::fs::remove_file(part).await;
            }
            return Err(DownloadError::Failed(format!(
                "The server answered {} for {}",
                status, url
            )));
        }
        if existing > 0 && !resumed {
            log::info!("The server can't resume this download, starting it over");
            hasher = Sha256::new();
            downloaded = 0;
        } else if resumed {
            log::info!("Resuming the download at {} MB", existing / (1024 * 1024));
        }

        let total = expected.size;
        progress(downloaded, total);

        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(resumed)
            .truncate(!resumed)
            .open(part)
            .await
            .map_err(|e| format!("Failed to open {}: {}", part.display(), e))?;
        let mut writer = tokio::io::BufWriter::with_capacity(1 << 20, &mut file);

        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::SeqCst) {
                let _ = writer.flush().await;
                return Err(DownloadError::Cancelled);
            }
            let chunk = chunk.map_err(|e| format!("Download interrupted: {}", net::describe_error(&e)))?;
            writer
                .write_all(&chunk)
                .await
                .map_err(|e| format!("Failed to write {}: {}", part.display(), e))?;
            hasher.update(&chunk);
            downloaded += chunk.len() as u64;
            progress(downloaded, total);
        }
        writer
            .flush()
            .await
            .map_err(|e| format!("Failed to write {}: {}", part.display(), e))?;
        drop(writer);
        file.sync_all()
            .await
            .map_err(|e| format!("Failed to write {}: {}", part.display(), e))?;
    } else {
        progress(downloaded, expected.size);
    }

    if downloaded != expected.size {
        return Err(DownloadError::Failed(format!(
            "The download is incomplete ({} of {} bytes)",
            downloaded, expected.size
        )));
    }

    let actual = to_hex(&hasher.finalize());
    if !actual.eq_ignore_ascii_case(expected.sha256) {
        let _ = tokio::fs::remove_file(part).await;
        return Err(DownloadError::Failed(format!(
            "The downloaded file is damaged (SHA-256 {} instead of {})",
            actual, expected.sha256
        )));
    }
    log::debug!("Verified {} (SHA-256 {})", part.display(), actual);
    Ok(())
}

async fn hash_existing(
    part: &Path,
    length: u64,
    hasher: &mut Sha256,
    cancel: &AtomicBool,
) -> Result<(), DownloadError> {
    let mut file = tokio::fs::File::open(part)
        .await
        .map_err(|e| format!("Failed to open {}: {}", part.display(), e))?;
    let mut buffer = vec![0u8; 1 << 20];
    let mut remaining = length;
    while remaining > 0 {
        if cancel.load(Ordering::SeqCst) {
            return Err(DownloadError::Cancelled);
        }
        let want = remaining.min(buffer.len() as u64) as usize;
        let read = file
            .read(&mut buffer[..want])
            .await
            .map_err(|e| format!("Failed to read {}: {}", part.display(), e))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        remaining -= read as u64;
    }
    Ok(())
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_hex() {
        assert_eq!(to_hex(&Sha256::digest(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
