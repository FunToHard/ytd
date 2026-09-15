//! # GitHub Auto Updater (Rust)
//!
//! An asynchronous, lightweight library to check, download, verify (SHA256),
//! and apply automatic updates for Windows applications hosted on GitHub Releases.
//!
//! Supports both silent Inno Setup / EXE installers and in-place binary swapping via `self-replace`.

pub mod client;
pub mod engine;
pub mod installer;
pub mod matcher;
pub mod models;
pub mod verifier;

pub use client::GitHubClient;
pub use engine::AutoUpdaterEngine;
pub use installer::UpdateInstaller;
pub use matcher::AssetMatcher;
pub use models::{AssetInfo, DownloadProgress, ReleaseInfo, UpdateEvent, UpdateOptions};
pub use verifier::ChecksumVerifier;
