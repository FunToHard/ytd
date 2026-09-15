use std::path::Path;
use futures_util::StreamExt;
use reqwest::header::{ACCEPT, USER_AGENT};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use crate::models::{AssetInfo, DownloadProgress, ReleaseInfo};

/// Client for communicating with the GitHub Releases API and downloading assets.
#[derive(Clone)]
pub struct GitHubClient {
    client: reqwest::Client,
    user_agent: String,
}

impl GitHubClient {
    pub fn new(custom_user_agent: Option<&str>) -> Self {
        let user_agent = custom_user_agent
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("github-auto-updater/{}", env!("CARGO_PKG_VERSION")));

        let client = reqwest::Client::builder()
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self { client, user_agent }
    }

    /// Fetches the latest published release for a given repository.
    pub async fn fetch_latest_release(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<ReleaseInfo, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("https://api.github.com/repos/{owner}/{repo}/releases/latest");

        let response = self
            .client
            .get(&url)
            .header(USER_AGENT, &self.user_agent)
            .header(ACCEPT, "application/vnd.github.v3+json")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!(
                "GitHub API request failed with status: {} ({})",
                response.status(),
                url
            )
            .into());
        }

        let mut release: ReleaseInfo = response.json().await?;
        
        // Parse semver from tag
        let clean_tag = release.tag_name.trim_start_matches('v');
        if let Ok(ver) = semver::Version::parse(clean_tag) {
            release.parsed_version = Some(ver);
        }

        Ok(release)
    }

    /// Fetches the string content of a checksums asset.
    pub async fn fetch_text(
        &self,
        url: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let text = self
            .client
            .get(url)
            .header(USER_AGENT, &self.user_agent)
            .send()
            .await?
            .text()
            .await?;
        Ok(text)
    }

    /// Downloads an asset to disk with streaming progress updates.
    pub async fn download_asset(
        &self,
        asset: &AssetInfo,
        destination_path: &Path,
        progress_tx: Option<mpsc::Sender<DownloadProgress>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let response = self
            .client
            .get(&asset.browser_download_url)
            .header(USER_AGENT, &self.user_agent)
            .header(ACCEPT, "application/octet-stream")
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!(
                "Failed to download asset '{}', status: {}",
                asset.name,
                response.status()
            )
            .into());
        }

        let total_bytes = response.content_length();
        let mut file = File::create(destination_path).await?;
        let mut stream = response.bytes_stream();
        let mut downloaded_bytes: u64 = 0;

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            file.write_all(&chunk).await?;
            downloaded_bytes += chunk.len() as u64;

            if let Some(ref tx) = progress_tx {
                let percentage = total_bytes.map(|total| {
                    if total > 0 {
                        (downloaded_bytes as f64 / total as f64) * 100.0
                    } else {
                        0.0
                    }
                });

                let _ = tx.send(DownloadProgress {
                    downloaded_bytes,
                    total_bytes,
                    percentage,
                }).await;
            }
        }

        file.flush().await?;
        Ok(())
    }
}
