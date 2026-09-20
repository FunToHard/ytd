use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{info, warn};

static IS_INSTALLING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, serde::Serialize)]
pub struct DependencyStatus {
    pub ytdlp_available: bool,
    pub ffmpeg_available: bool,
    pub deno_available: bool,
    pub all_ready: bool,
    pub is_installing: bool,
    pub bin_dir: String,
}

pub fn get_bin_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ytd")
        .join("bin")
}

pub fn is_in_path(binary: &str) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let mut cmd = std::process::Command::new("where");
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.arg(binary);
        if let Ok(output) = cmd.output() {
            return output.status.success();
        }
    }

    #[cfg(not(windows))]
    {
        let mut cmd = std::process::Command::new("which");
        cmd.arg(binary);
        if let Ok(output) = cmd.output() {
            return output.status.success();
        }
    }

    false
}

pub fn check_dependencies() -> DependencyStatus {
    let bin_dir = get_bin_dir();
    let ytdlp_in_bin = bin_dir.join("yt-dlp.exe").exists() || bin_dir.join("yt-dlp").exists();
    let ffmpeg_in_bin = bin_dir.join("ffmpeg.exe").exists() || bin_dir.join("ffmpeg").exists();
    let deno_in_bin = bin_dir.join("deno.exe").exists() || bin_dir.join("deno").exists();

    let ytdlp_available = ytdlp_in_bin || is_in_path("yt-dlp");
    let ffmpeg_available = ffmpeg_in_bin || is_in_path("ffmpeg");
    let deno_available = deno_in_bin || is_in_path("deno");

    DependencyStatus {
        ytdlp_available,
        ffmpeg_available,
        deno_available,
        all_ready: ytdlp_available && ffmpeg_available && deno_available,
        is_installing: IS_INSTALLING.load(Ordering::Relaxed),
        bin_dir: bin_dir.to_string_lossy().to_string(),
    }
}

/// Prepends the local bin directory (%APPDATA%\ytd\bin) to the process PATH
/// so that child processes (yt-dlp, ffmpeg) are discovered automatically without
/// requiring user modification of global Windows environment variables.
pub fn setup_bin_path() {
    let bin_dir = get_bin_dir();
    let _ = std::fs::create_dir_all(&bin_dir);

    if let Ok(current_path) = std::env::var("PATH") {
        let bin_str = bin_dir.to_string_lossy();
        if !current_path.contains(&*bin_str) {
            let new_path = format!("{};{}", bin_str, current_path);
            std::env::set_var("PATH", new_path);
            info!("Prepended {:?} to process PATH", bin_dir);
        }
    }
}

/// Automatically downloads yt-dlp and ffmpeg to the local bin directory.
pub async fn install_dependencies() -> Result<(), String> {
    if IS_INSTALLING.swap(true, Ordering::SeqCst) {
        return Err("Dependency installation is already in progress".to_string());
    }

    let res = tokio::task::spawn_blocking(move || {
        install_dependencies_sync()
    }).await.map_err(|e| format!("Task error: {}", e))?;

    IS_INSTALLING.store(false, Ordering::SeqCst);
    res
}

/// Computes the SHA-256 hash of a file on disk.
pub fn compute_file_sha256(path: &std::path::Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|e| format!("Failed to open file for checksum calculation: {}", e))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 65536];
    loop {
        let bytes_read = file.read(&mut buffer)
            .map_err(|e| format!("Error reading file during hash calculation: {}", e))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let hash = hasher.finalize();
    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";
    let mut hex = String::with_capacity(64);
    for &byte in &hash {
        hex.push(HEX_CHARS[(byte >> 4) as usize] as char);
        hex.push(HEX_CHARS[(byte & 0x0f) as usize] as char);
    }
    Ok(hex)
}

/// Parses an expected 64-character hex SHA-256 hash from a checksum manifest file.
pub fn extract_sha256_for_asset(manifest: &str, asset_name: Option<&str>) -> Option<String> {
    for line in manifest.lines() {
        let line = line.trim();
        if let Some(target) = asset_name {
            if !line.contains(target) {
                continue;
            }
        }
        for word in line.split_whitespace() {
            let clean = word.trim().trim_matches('"').trim_matches('\'');
            if clean.len() == 64 && clean.chars().all(|c| c.is_ascii_hexdigit()) {
                return Some(clean.to_lowercase());
            }
        }
    }
    None
}

/// Downloads a file via curl.exe, falling back to PowerShell Invoke-WebRequest.
fn download_file(url: &str, dest: &std::path::Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut cmd = std::process::Command::new("curl.exe");
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.arg("-L").arg("-s").arg("-f").arg("-o").arg(dest).arg(url);

    if let Ok(status) = cmd.status() {
        if status.success() && dest.exists() {
            return Ok(());
        }
    }

    // PowerShell fallback with single quote escaping
    let dest_str = dest.display().to_string().replace('\'', "''");
    let url_str = url.replace('\'', "''");
    let ps_script = format!(
        "[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -Uri '{}' -OutFile '{}'",
        url_str, dest_str
    );
    let mut ps_cmd = std::process::Command::new("powershell");
    ps_cmd.creation_flags(CREATE_NO_WINDOW);
    ps_cmd.arg("-NoProfile").arg("-Command").arg(&ps_script);
    let ps_status = ps_cmd.status().map_err(|e| format!("PowerShell download failed: {}", e))?;
    if ps_status.success() && dest.exists() {
        Ok(())
    } else {
        Err(format!("Failed to download from {}", url))
    }
}

/// SEC-03: Downloads a release binary or archive and verifies its SHA-256 checksum against official manifests.
fn download_and_verify_sha256(
    url: &str,
    dest: &std::path::Path,
    checksum_url: &str,
    asset_name: Option<&str>,
) -> Result<(), String> {
    let temp_manifest = dest.with_extension(format!(
        "manifest_{}.tmp",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()
    ));

    // 1. Download checksum manifest
    download_file(checksum_url, &temp_manifest)
        .map_err(|e| format!("Failed to download checksum manifest from {}: {}", checksum_url, e))?;

    let manifest_content = std::fs::read_to_string(&temp_manifest)
        .map_err(|e| format!("Failed to read checksum manifest: {}", e))?;
    let _ = std::fs::remove_file(&temp_manifest);

    let expected_hash = extract_sha256_for_asset(&manifest_content, asset_name)
        .ok_or_else(|| format!("Could not find expected SHA-256 hash for {:?}", asset_name))?;

    // 2. Download target binary
    download_file(url, dest)?;

    // 3. Compute and verify SHA-256
    let computed_hash = compute_file_sha256(dest)?;
    if computed_hash.to_lowercase() != expected_hash.to_lowercase() {
        let _ = std::fs::remove_file(dest);
        return Err(format!(
            "Security verification failed: SHA-256 checksum mismatch for {} (expected {}, computed {})",
            dest.display(),
            expected_hash,
            computed_hash
        ));
    }

    info!(
        "SHA-256 verified for {:?}: {}",
        dest.file_name().unwrap_or_default(),
        computed_hash
    );
    Ok(())
}

/// Creates an isolated, unpredictable temporary directory for archive extractions.
pub fn create_unique_temp_dir(prefix: &str) -> Result<PathBuf, String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let pid = std::process::id();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir_name = format!("{}_{}_{}_{}", prefix, pid, timestamp, count);
    let temp_dir = std::env::temp_dir().join(dir_name);
    std::fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("Failed to create temporary directory: {}", e))?;
    Ok(temp_dir)
}

fn install_dependencies_sync() -> Result<(), String> {
    use std::fs;
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let bin_dir = get_bin_dir();
    fs::create_dir_all(&bin_dir).map_err(|e| format!("Failed to create bin dir: {}", e))?;

    info!("Starting automated dependency installation into {:?}", bin_dir);

    // 1. Download and verify yt-dlp.exe atomically
    let ytdlp_path = bin_dir.join("yt-dlp.exe");
    if !ytdlp_path.exists() {
        info!("Downloading yt-dlp.exe with SHA-256 verification...");
        let url = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";
        let checksum_url = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/SHA2-256SUMS";
        let temp_ytdlp = bin_dir.join(format!(
            "yt-dlp_{}.tmp",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()
        ));
        if let Err(e) = download_and_verify_sha256(url, &temp_ytdlp, checksum_url, Some("yt-dlp.exe")) {
            let _ = fs::remove_file(&temp_ytdlp);
            return Err(e);
        }
        if let Err(e) = fs::rename(&temp_ytdlp, &ytdlp_path) {
            let _ = fs::remove_file(&temp_ytdlp);
            return Err(format!("Failed to move verified yt-dlp.exe into destination: {}", e));
        }
        info!("yt-dlp.exe verified and installed successfully");
    }

    // 2. Download, verify, & extract ffmpeg
    let ffmpeg_path = bin_dir.join("ffmpeg.exe");
    if !ffmpeg_path.exists() {
        info!("Downloading ffmpeg release archive with SHA-256 verification...");
        let temp_dir = create_unique_temp_dir("ytd_setup_ffmpeg")?;
        let zip_path = temp_dir.join("ffmpeg.zip");

        let ffmpeg_url = "https://github.com/yt-dlp/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip";
        let checksum_url = "https://github.com/yt-dlp/FFmpeg-Builds/releases/download/latest/checksums.sha256";
        download_and_verify_sha256(ffmpeg_url, &zip_path, checksum_url, Some("ffmpeg-master-latest-win64-gpl.zip"))?;

        // Extract using tar.exe
        info!("Extracting verified ffmpeg binaries...");
        let mut tar_cmd = std::process::Command::new("tar.exe");
        tar_cmd.creation_flags(CREATE_NO_WINDOW);
        tar_cmd.arg("-xf").arg(&zip_path).arg("-C").arg(&temp_dir);
        let _ = tar_cmd.status();

        // Search for ffmpeg.exe and ffprobe.exe inside temp_dir and copy to bin_dir
        if let Ok(entries) = fs::read_dir(&temp_dir) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    let sub_bin = entry.path().join("bin");
                    if sub_bin.exists() {
                        let _ = fs::copy(sub_bin.join("ffmpeg.exe"), bin_dir.join("ffmpeg.exe"));
                        let _ = fs::copy(sub_bin.join("ffprobe.exe"), bin_dir.join("ffprobe.exe"));
                    }
                }
            }
        }

        // If tar extraction didn't yield ffmpeg.exe, try PowerShell fallback
        if !ffmpeg_path.exists() {
            let zip_str = zip_path.display().to_string().replace('\'', "''");
            let temp_str = temp_dir.display().to_string().replace('\'', "''");
            let ps_script = format!(
                "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
                zip_str, temp_str
            );
            let mut ps_cmd = std::process::Command::new("powershell");
            ps_cmd.creation_flags(CREATE_NO_WINDOW);
            ps_cmd.arg("-NoProfile").arg("-Command").arg(&ps_script);
            let _ = ps_cmd.status();

            if let Ok(entries) = fs::read_dir(&temp_dir) {
                for entry in entries.flatten() {
                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        let sub_bin = entry.path().join("bin");
                        if sub_bin.exists() {
                            let _ = fs::copy(sub_bin.join("ffmpeg.exe"), bin_dir.join("ffmpeg.exe"));
                            let _ = fs::copy(sub_bin.join("ffprobe.exe"), bin_dir.join("ffprobe.exe"));
                        }
                    }
                }
            }
        }

        let _ = fs::remove_dir_all(&temp_dir);

        if !ffmpeg_path.exists() {
            return Err("Failed to extract ffmpeg.exe from downloaded archive".to_string());
        }

        info!("ffmpeg extracted successfully to {:?}", bin_dir);
    }

    // 3. Download, verify, & extract Deno
    let deno_path = bin_dir.join("deno.exe");
    if !deno_path.exists() && !is_in_path("deno") {
        info!("Downloading Deno release archive with SHA-256 verification...");
        let temp_dir = create_unique_temp_dir("ytd_setup_deno")?;
        let zip_path = temp_dir.join("deno.zip");

        let deno_url = "https://github.com/denoland/deno/releases/latest/download/deno-x86_64-pc-windows-msvc.zip";
        let checksum_url = "https://github.com/denoland/deno/releases/latest/download/deno-x86_64-pc-windows-msvc.zip.sha256sum";
        download_and_verify_sha256(deno_url, &zip_path, checksum_url, None)?;

        info!("Extracting verified Deno binary...");
        let mut tar_cmd = std::process::Command::new("tar.exe");
        tar_cmd.creation_flags(CREATE_NO_WINDOW);
        tar_cmd.arg("-xf").arg(&zip_path).arg("-C").arg(&temp_dir);
        let _ = tar_cmd.status();

        let extracted_deno = temp_dir.join("deno.exe");
        if !extracted_deno.exists() {
            // Fallback to PowerShell Expand-Archive with escaped path
            let zip_str = zip_path.display().to_string().replace('\'', "''");
            let temp_str = temp_dir.display().to_string().replace('\'', "''");
            let ps_script = format!(
                "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
                zip_str, temp_str
            );
            let mut ps_cmd = std::process::Command::new("powershell");
            ps_cmd.creation_flags(CREATE_NO_WINDOW);
            ps_cmd.arg("-NoProfile").arg("-Command").arg(&ps_script);
            let _ = ps_cmd.status();
        }

        if extracted_deno.exists() {
            let _ = fs::copy(&extracted_deno, &deno_path);
            info!("Deno installed successfully to {:?}", deno_path);
        } else {
            return Err("Failed to extract deno.exe from archive".to_string());
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }

    setup_bin_path();
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DependencyUpdateResult {
    pub ytdlp_updated: bool,
    pub ytdlp_message: String,
    pub deno_updated: bool,
    pub deno_message: String,
}

/// Automatically checks and applies updates for yt-dlp and Deno.
pub async fn update_dependencies() -> Result<DependencyUpdateResult, String> {
    if IS_INSTALLING.swap(true, Ordering::SeqCst) {
        return Err("Dependency operation is already in progress".to_string());
    }

    let res = tokio::task::spawn_blocking(move || {
        update_dependencies_sync()
    }).await.map_err(|e| format!("Task error: {}", e))?;

    IS_INSTALLING.store(false, Ordering::SeqCst);
    res
}

fn update_dependencies_sync() -> Result<DependencyUpdateResult, String> {
    use std::fs;
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    setup_bin_path();
    let bin_dir = get_bin_dir();
    fs::create_dir_all(&bin_dir).map_err(|e| format!("Failed to create bin dir: {}", e))?;

    info!("Checking for dependency updates...");

    // 1. Check/Update yt-dlp via yt-dlp -U
    let mut ytdlp_cmd = std::process::Command::new("yt-dlp");
    ytdlp_cmd.creation_flags(CREATE_NO_WINDOW);
    ytdlp_cmd.arg("-U");

    let (ytdlp_updated, ytdlp_message) = match ytdlp_cmd.output() {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let combined = format!("{}\n{}", stdout, stderr);
            let updated = combined.contains("Updated yt-dlp to") || combined.contains("Updating to");
            let msg = if updated {
                "Updated to latest version".to_string()
            } else {
                "Up to date".to_string()
            };
            info!("yt-dlp update check: {}", msg);
            (updated, msg)
        }
        _ => {
            info!("yt-dlp -U unsuccessful, attempting direct download update fallback...");
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            let temp_ytdlp = bin_dir.join(format!("yt-dlp_update_{}.tmp", timestamp));
            let ytdlp_path = bin_dir.join("yt-dlp.exe");
            let url = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";
            let checksum_url = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/SHA2-256SUMS";
            let res = match download_and_verify_sha256(url, &temp_ytdlp, checksum_url, Some("yt-dlp.exe")) {
                Ok(_) => {
                    if let Err(e) = fs::copy(&temp_ytdlp, &ytdlp_path) {
                        warn!("Failed to replace yt-dlp.exe with updated binary: {}", e);
                        (false, format!("Failed to install update: {}", e))
                    } else {
                        (true, "Replaced with latest release binary (checksum verified)".to_string())
                    }
                }
                Err(e) => {
                    warn!("yt-dlp fallback download/verification failed: {}", e);
                    (false, format!("Update failed: {}", e))
                }
            };
            let _ = fs::remove_file(&temp_ytdlp);
            res
        }
    };

    // 2. Check/Update Deno via deno upgrade
    let mut deno_cmd = std::process::Command::new("deno");
    deno_cmd.creation_flags(CREATE_NO_WINDOW);
    deno_cmd.arg("upgrade");

    let (deno_updated, deno_message) = match deno_cmd.output() {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let combined = format!("{}\n{}", stdout, stderr);
            let updated = combined.contains("Upgraded to") || combined.contains("Upgraded Deno to") || combined.contains("Upgrading to");
            let msg = if updated {
                "Upgraded to latest version".to_string()
            } else {
                "Up to date".to_string()
            };
            info!("Deno update check: {}", msg);
            (updated, msg)
        }
        _ => {
            info!("deno upgrade unsuccessful, attempting direct download update fallback...");
            let temp_dir = match create_unique_temp_dir("ytd_deno_update") {
                Ok(d) => d,
                Err(e) => return Ok(DependencyUpdateResult {
                    ytdlp_updated,
                    ytdlp_message,
                    deno_updated: false,
                    deno_message: format!("Failed to create temporary directory: {}", e),
                }),
            };
            let zip_path = temp_dir.join("deno.zip");
            let deno_url = "https://github.com/denoland/deno/releases/latest/download/deno-x86_64-pc-windows-msvc.zip";
            let checksum_url = "https://github.com/denoland/deno/releases/latest/download/deno-x86_64-pc-windows-msvc.zip.sha256sum";

            let res = match download_and_verify_sha256(deno_url, &zip_path, checksum_url, None) {
                Ok(_) => {
                    let mut tar_cmd = std::process::Command::new("tar.exe");
                    tar_cmd.creation_flags(CREATE_NO_WINDOW);
                    tar_cmd.arg("-xf").arg(&zip_path).arg("-C").arg(&temp_dir);
                    let _ = tar_cmd.status();

                    let extracted = temp_dir.join("deno.exe");
                    if !extracted.exists() {
                        let zip_str = zip_path.display().to_string().replace('\'', "''");
                        let temp_str = temp_dir.display().to_string().replace('\'', "''");
                        let ps_script = format!(
                            "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
                            zip_str, temp_str
                        );
                        let mut ps_cmd = std::process::Command::new("powershell");
                        ps_cmd.creation_flags(CREATE_NO_WINDOW);
                        ps_cmd.arg("-NoProfile").arg("-Command").arg(&ps_script);
                        let _ = ps_cmd.status();
                    }

                    if extracted.exists() {
                        let _ = fs::copy(&extracted, bin_dir.join("deno.exe"));
                        (true, "Installed latest release binary (checksum verified)".to_string())
                    } else {
                        (false, "Extraction failed".to_string())
                    }
                }
                Err(e) => {
                    warn!("Deno fallback download/verification failed: {}", e);
                    (false, format!("Download failed: {}", e))
                }
            };
            let _ = fs::remove_dir_all(&temp_dir);
            res
        }
    };

    Ok(DependencyUpdateResult {
        ytdlp_updated,
        ytdlp_message,
        deno_updated,
        deno_message,
    })
}

/// Opens the browser extension management tab and highlights the extension folder in Explorer.
pub fn open_extension_helper() {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let ext_dir = if let Ok(mut dir) = std::env::current_exe() {
            dir.pop(); // remove exe name
            let candidate1 = dir.join("extension");
            let candidate2 = dir.join("..").join("extension");
            if candidate1.exists() {
                candidate1
            } else if candidate2.exists() {
                candidate2
            } else {
                PathBuf::from("extension")
            }
        } else {
            PathBuf::from("extension")
        };

        let ext_abs_path = std::fs::canonicalize(&ext_dir).unwrap_or(ext_dir);
        let ext_path_str = ext_abs_path.to_string_lossy().to_string();

        // Copy folder path to clipboard
        let escaped_path = ext_path_str.replace('\'', "''");
        let ps_cmd = format!("Set-Clipboard -Value '{}'", escaped_path);
        let mut clip = std::process::Command::new("powershell");
        clip.creation_flags(CREATE_NO_WINDOW);
        clip.arg("-NoProfile").arg("-NonInteractive").arg("-Command").arg(&ps_cmd);
        let _ = clip.output();

        // Open extension page in browser
        let _ = open::that("edge://extensions").or_else(|_| open::that("chrome://extensions"));

        // Open Explorer with manifest.json or folder selected
        let manifest_path = ext_abs_path.join("manifest.json");
        if manifest_path.exists() {
            let mut exp = std::process::Command::new("explorer.exe");
            exp.creation_flags(CREATE_NO_WINDOW);
            exp.arg(format!("/select,\"{}\"", manifest_path.display()));
            let _ = exp.output();
        } else {
            let _ = open::that(&ext_abs_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_dependencies_structure() {
        let status = check_dependencies();
        assert_eq!(
            status.all_ready,
            status.ytdlp_available && status.ffmpeg_available && status.deno_available
        );
        let json = serde_json::to_string(&status).expect("Serialization failed");
        assert!(json.contains("deno_available"));
        assert!(json.contains("ytdlp_available"));
        assert!(json.contains("ffmpeg_available"));
        assert!(json.contains("all_ready"));
    }

    #[test]
    fn test_dependency_update_result_serialization() {
        let res = DependencyUpdateResult {
            ytdlp_updated: true,
            ytdlp_message: "Updated to 2026.09".to_string(),
            deno_updated: false,
            deno_message: "Up to date".to_string(),
        };
        let json = serde_json::to_string(&res).expect("Serialization failed");
        assert!(json.contains("\"ytdlp_updated\":true"));
        assert!(json.contains("\"deno_updated\":false"));
    }

    #[test]
    fn test_extract_sha256_for_asset() {
        let manifest = r#"
9174df0b9826f5fc274ef43d4ebadab339d675b28d4fa9fa0e0b3e6eefaa6a4a  yt-dlp
d4735e3a265e16eee03f59718b9b5d03019c07d8b6c51f90da3a666eec13ab35  yt-dlp.exe
a1b2c3d4e5f60718293a4b5c6d7e8f90123456789abcdef0123456789abcdef0  yt-dlp_macos
"#;
        let hash = extract_sha256_for_asset(manifest, Some("yt-dlp.exe"));
        assert_eq!(
            hash,
            Some("d4735e3a265e16eee03f59718b9b5d03019c07d8b6c51f90da3a666eec13ab35".to_string())
        );

        let first_hash = extract_sha256_for_asset(manifest, None);
        assert_eq!(
            first_hash,
            Some("9174df0b9826f5fc274ef43d4ebadab339d675b28d4fa9fa0e0b3e6eefaa6a4a".to_string())
        );

        let missing = extract_sha256_for_asset(manifest, Some("nonexistent.tar.gz"));
        assert_eq!(missing, None);

        let single_manifest = "f2ca1bb6c7e907d06dafe4687e579fce76b37e4e93b7605022da52e6ccc26fd2  deno.zip\n";
        let deno_hash = extract_sha256_for_asset(single_manifest, None);
        assert_eq!(
            deno_hash,
            Some("f2ca1bb6c7e907d06dafe4687e579fce76b37e4e93b7605022da52e6ccc26fd2".to_string())
        );
    }

    #[test]
    fn test_compute_file_sha256() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join(format!(
            "ytd_test_hash_{}.txt",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ));
        std::fs::write(&test_file, b"hello world").expect("Write failed");

        let hash = compute_file_sha256(&test_file).expect("Compute failed");
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );

        let _ = std::fs::remove_file(&test_file);
    }

    #[test]
    fn test_create_unique_temp_dir() {
        let dir1 = create_unique_temp_dir("ytd_test_dir").expect("Failed to create dir1");
        let dir2 = create_unique_temp_dir("ytd_test_dir").expect("Failed to create dir2");

        assert!(dir1.exists());
        assert!(dir2.exists());
        assert_ne!(dir1, dir2);

        let _ = std::fs::remove_dir_all(&dir1);
        let _ = std::fs::remove_dir_all(&dir2);
    }
}
