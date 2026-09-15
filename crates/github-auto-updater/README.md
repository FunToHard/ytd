# github-auto-updater

[![Rust](https://img.shields.io/badge/rust-stable-brightgreen.svg)](https://www.rust-lang.org/)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)

An asynchronous, lightweight Rust library to check, download, verify (SHA256), and apply automatic updates for Windows applications hosted on GitHub Releases.

Ported from [GitHubAutoUpdater.NET](https://github.com/FunToHard/GitHubAutoUpdater.git).

---

## Features

- **GitHub Releases Integration**: Queries public GitHub Releases API (`/releases/latest`) with streaming asset downloads.
- **SemVer 2.0 Compliant**: Full semantic version comparison supporting `v1.2.3`, `1.2.3-beta.1`, and build tags.
- **Cryptographic Integrity Verification**: Parses GNU/BSD style checksum files (`SHA256SUMS.txt`, `checksums.txt`) and validates file digests before running any update.
- **Dual Update Mechanisms**:
  - **Installer Execution**: Executes Inno Setup / EXE installers in detached mode with silent parameters (`/SILENT /CLOSEAPPLICATIONS /RESTARTAPPLICATIONS`).
  - **In-Place Binary Replacement**: Safely swaps running Windows `.exe` files on disk via [`self-replace`](https://github.com/mitsuhiko/self-replace) without file-lock collisions (`ERROR_ACCESS_DENIED`).
- **Asynchronous & Non-Blocking**: Built on Tokio and `reqwest` with native TLS via `rustls`.
- **Event-Driven Architecture**: Emits lifecycle events (`Checking`, `UpdateAvailable`, `DownloadProgress`, `DownloadComplete`, `Error`) with optional periodic background polling.

---

## Installation

Add `github-auto-updater` to your `Cargo.toml`:

```toml
[dependencies]
github-auto-updater = { git = "https://github.com/FunToHard/github-auto-updater-rs.git", branch = "main" }
```

---

## Quickstart

```rust
use github_auto_updater::{AutoUpdaterEngine, UpdateOptions};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 1. Configure repository and current version
    let options = UpdateOptions::new("FunToHard", "ytd", env!("CARGO_PKG_VERSION"))
        .with_prefer_installer(true)
        .with_allow_prerelease(false);

    let engine = AutoUpdaterEngine::new(options);

    // 2. Check for updates
    if let Some(release) = engine.check_for_updates().await? {
        println!("New release found: {}", release.tag_name);

        // 3. Download update and verify SHA256 checksum
        let update_file = engine.download_update(&release, None).await?;
        println!("Update downloaded to: {}", update_file.display());

        // 4. Apply installer or swap running binary in-place
        engine.apply_update(&update_file)?;
    } else {
        println!("Application is up to date.");
    }

    Ok(())
}
```

---

## Periodic Background Polling

You can schedule background checks at regular intervals:

```rust
use github_auto_updater::{AutoUpdaterEngine, UpdateEvent, UpdateOptions};
use std::time::Duration;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    let options = UpdateOptions::new("FunToHard", "ytd", "1.0.0");
    let engine = AutoUpdaterEngine::new(options);

    let (tx, mut rx) = mpsc::channel(10);
    // Poll every 24 hours
    engine.start_background_check(Duration::from_secs(24 * 3600), tx);

    while let Some(event) = rx.recv().await {
        match event {
            UpdateEvent::UpdateAvailable(release) => {
                println!("Update available: {}", release.tag_name);
            }
            UpdateEvent::UpToDate => {
                println!("Up to date.");
            }
            UpdateEvent::Error(err) => {
                eprintln!("Update error: {}", err);
            }
            _ => {}
        }
    }
}
```

---

## Architecture

```
src/
├── lib.rs                  # Public API re-exports
├── models.rs               # ReleaseInfo, AssetInfo, UpdateOptions, UpdateEvent
├── client.rs               # GitHubClient (REST API v3 & streaming downloads)
├── matcher.rs              # AssetMatcher (Setup .exe vs portable .zip resolution)
├── verifier.rs             # ChecksumVerifier (SHA256 parsing and validation)
├── installer.rs            # UpdateInstaller (Inno Setup execution & self-replace)
└── engine.rs               # AutoUpdaterEngine (lifecycle and background coordinator)
```

---

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
