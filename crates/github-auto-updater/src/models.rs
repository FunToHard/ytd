use std::path::PathBuf;
use serde::{Deserialize, Serialize};

/// Detailed information about a GitHub Release.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseInfo {
    pub tag_name: String,
    #[serde(skip)]
    pub parsed_version: Option<semver::Version>,
    pub name: Option<String>,
    pub body: Option<String>,
    pub prerelease: bool,
    pub published_at: Option<String>,
    pub html_url: String,
    pub assets: Vec<AssetInfo>,
}

/// Information about a release asset on GitHub.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetInfo {
    pub id: u64,
    pub name: String,
    pub size: u64,
    pub browser_download_url: String,
    pub content_type: Option<String>,
}

/// Progress details for an ongoing asset download.
#[derive(Debug, Clone, Copy)]
pub struct DownloadProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub percentage: Option<f64>,
}

/// Lifecycle events emitted during update checking and installation.
#[derive(Debug, Clone)]
pub enum UpdateEvent {
    Checking,
    UpdateAvailable(ReleaseInfo),
    UpToDate,
    DownloadProgress(DownloadProgress),
    DownloadComplete(PathBuf),
    ApplyingUpdate,
    Error(String),
}

/// Configuration options for the auto-updater engine.
#[derive(Debug, Clone)]
pub struct UpdateOptions {
    pub owner: String,
    pub repo: String,
    pub current_version: String,
    pub allow_prerelease: bool,
    pub prefer_installer: bool,
    pub silent_installer_args: Vec<String>,
    pub custom_user_agent: Option<String>,
}

impl UpdateOptions {
    pub fn new(owner: impl Into<String>, repo: impl Into<String>, current_version: impl Into<String>) -> Self {
        Self {
            owner: owner.into(),
            repo: repo.into(),
            current_version: current_version.into(),
            allow_prerelease: false,
            prefer_installer: true,
            silent_installer_args: vec![
                "/SILENT".to_string(),
                "/CLOSEAPPLICATIONS".to_string(),
                "/RESTARTAPPLICATIONS".to_string(),
            ],
            custom_user_agent: None,
        }
    }

    pub fn with_allow_prerelease(mut self, allow: bool) -> Self {
        self.allow_prerelease = allow;
        self
    }

    pub fn with_prefer_installer(mut self, prefer: bool) -> Self {
        self.prefer_installer = prefer;
        self
    }

    pub fn with_silent_installer_args(mut self, args: Vec<String>) -> Self {
        self.silent_installer_args = args;
        self
    }

    pub fn with_user_agent(mut self, agent: impl Into<String>) -> Self {
        self.custom_user_agent = Some(agent.into());
        self
    }
}
