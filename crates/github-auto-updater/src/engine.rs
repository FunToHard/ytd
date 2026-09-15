use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::client::GitHubClient;
use crate::installer::UpdateInstaller;
use crate::matcher::AssetMatcher;
use crate::models::{DownloadProgress, ReleaseInfo, UpdateEvent, UpdateOptions};
use crate::verifier::ChecksumVerifier;

/// The primary engine coordinating update checks, verification, download, and installation.
#[derive(Clone)]
pub struct AutoUpdaterEngine {
    options: UpdateOptions,
    client: GitHubClient,
}

impl AutoUpdaterEngine {
    pub fn new(options: UpdateOptions) -> Self {
        let client = GitHubClient::new(options.custom_user_agent.as_deref());
        Self { options, client }
    }

    pub fn options(&self) -> &UpdateOptions {
        &self.options
    }

    /// Checks GitHub Releases for a version higher than the currently installed version.
    pub async fn check_for_updates(
        &self,
    ) -> Result<Option<ReleaseInfo>, Box<dyn std::error::Error + Send + Sync>> {
        info!(
            "Checking GitHub updates for {}/{} (current version: {})",
            self.options.owner, self.options.repo, self.options.current_version
        );

        let release = self
            .client
            .fetch_latest_release(&self.options.owner, &self.options.repo)
            .await?;

        // Ignore prereleases unless opted in
        if release.prerelease && !self.options.allow_prerelease {
            info!("Latest release {} is a prerelease; ignoring", release.tag_name);
            return Ok(None);
        }

        let clean_current = self.options.current_version.trim_start_matches('v');
        let current_ver = match semver::Version::parse(clean_current) {
            Ok(v) => v,
            Err(e) => {
                warn!("Could not parse current version '{}' as SemVer: {}", clean_current, e);
                return Err(format!("Invalid local version string: {}", self.options.current_version).into());
            }
        };

        if let Some(ref latest_ver) = release.parsed_version {
            if latest_ver > &current_ver {
                info!(
                    "Update available! Current: {}, Latest: {}",
                    current_ver, latest_ver
                );
                return Ok(Some(release));
            } else {
                info!("Application is up to date (current: {}, latest: {})", current_ver, latest_ver);
                return Ok(None);
            }
        }

        warn!("Could not parse release tag '{}' as SemVer", release.tag_name);
        Ok(None)
    }

    /// Downloads the matched asset for the release into a temporary directory and verifies its SHA256 checksum.
    pub async fn download_update(
        &self,
        release: &ReleaseInfo,
        progress_tx: Option<mpsc::Sender<DownloadProgress>>,
    ) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
        let asset = AssetMatcher::find_target_asset(&release.assets, self.options.prefer_installer)
            .ok_or_else(|| {
                format!(
                    "No suitable asset found in release '{}' for this platform",
                    release.tag_name
                )
            })?;

        info!("Selected asset: {} ({})", asset.name, asset.browser_download_url);

        // Prepare destination path in temporary directory
        let temp_dir = std::env::temp_dir().join("ytd_updates");
        tokio::fs::create_dir_all(&temp_dir).await?;
        let destination_path = temp_dir.join(&asset.name);

        // Download asset
        self.client
            .download_asset(asset, &destination_path, progress_tx)
            .await?;

        info!("Downloaded asset to {}", destination_path.display());

        // Check if companion checksums exist
        if let Some(checksum_asset) = AssetMatcher::find_checksum_asset(&release.assets) {
            info!("Fetching checksum file: {}", checksum_asset.name);
            match self.client.fetch_text(&checksum_asset.browser_download_url).await {
                Ok(checksum_content) => {
                    let checksum_map = ChecksumVerifier::parse_checksum_map(&checksum_content);
                    if let Some(expected_hash) = checksum_map.get(&asset.name) {
                        info!("Verifying SHA256 hash for {} against expected {}", asset.name, expected_hash);
                        let is_valid = ChecksumVerifier::verify_file(&destination_path, expected_hash)?;
                        if !is_valid {
                            let _ = tokio::fs::remove_file(&destination_path).await;
                            return Err(format!(
                                "SHA256 checksum mismatch for asset '{}'",
                                asset.name
                            )
                            .into());
                        }
                        info!("SHA256 checksum verification passed for {}", asset.name);
                    } else {
                        warn!("Checksum file found, but no entry for '{}'", asset.name);
                    }
                }
                Err(e) => {
                    warn!("Failed to fetch checksums: {}; proceeding without hash verification", e);
                }
            }
        }

        Ok(destination_path)
    }

    /// Applies the downloaded update file.
    ///
    /// If the file is an installer executable, executes it silently in the background.
    /// If it is a raw executable, swaps the running binary via self-replace.
    pub fn apply_update(
        &self,
        update_file: &Path,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let filename = update_file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();

        if filename.ends_with(".exe") && (filename.contains("setup") || filename.contains("installer")) {
            info!(
                "Executing silent installer: {} with args: {:?}",
                update_file.display(),
                self.options.silent_installer_args
            );
            UpdateInstaller::apply_installer(update_file, &self.options.silent_installer_args)?;
        } else if filename.ends_with(".exe") {
            info!("Swapping binary in-place: {}", update_file.display());
            UpdateInstaller::apply_in_place_binary(update_file)?;
            UpdateInstaller::restart_process()?;
        } else {
            return Err(format!("Unsupported update file format: {}", filename).into());
        }

        Ok(())
    }

    /// Spawns a background task that periodically checks for updates and emits events on `event_tx`.
    pub fn start_background_check(
        &self,
        interval: Duration,
        event_tx: mpsc::Sender<UpdateEvent>,
    ) -> tokio::task::JoinHandle<()> {
        let engine = self.clone();

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);

            loop {
                ticker.tick().await;

                let _ = event_tx.send(UpdateEvent::Checking).await;
                match engine.check_for_updates().await {
                    Ok(Some(release)) => {
                        let _ = event_tx.send(UpdateEvent::UpdateAvailable(release)).await;
                    }
                    Ok(None) => {
                        let _ = event_tx.send(UpdateEvent::UpToDate).await;
                    }
                    Err(e) => {
                        error!("Background update check error: {}", e);
                        let _ = event_tx.send(UpdateEvent::Error(e.to_string())).await;
                    }
                }
            }
        })
    }
}
