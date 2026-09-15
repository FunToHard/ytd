use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::info;

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

fn install_dependencies_sync() -> Result<(), String> {
    use std::fs;
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let bin_dir = get_bin_dir();
    fs::create_dir_all(&bin_dir).map_err(|e| format!("Failed to create bin dir: {}", e))?;

    info!("Starting automated dependency installation into {:?}", bin_dir);

    // 1. Download yt-dlp.exe
    let ytdlp_path = bin_dir.join("yt-dlp.exe");
    if !ytdlp_path.exists() {
        info!("Downloading yt-dlp.exe...");
        let url = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";
        let mut cmd = std::process::Command::new("curl.exe");
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.arg("-L")
            .arg("-s")
            .arg("-o")
            .arg(&ytdlp_path)
            .arg(url);

        let status = cmd.status().map_err(|e| format!("Failed to run curl.exe for yt-dlp: {}", e))?;
        if !status.success() || !ytdlp_path.exists() {
            // Fallback to powershell Invoke-WebRequest
            let ps_script = format!(
                "[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -Uri '{}' -OutFile '{}'",
                url, ytdlp_path.display()
            );
            let mut ps_cmd = std::process::Command::new("powershell");
            ps_cmd.creation_flags(CREATE_NO_WINDOW);
            ps_cmd.arg("-NoProfile").arg("-Command").arg(&ps_script);
            let ps_status = ps_cmd.status().map_err(|e| format!("PowerShell download failed: {}", e))?;
            if !ps_status.success() {
                return Err("Failed to download yt-dlp.exe".to_string());
            }
        }
        info!("yt-dlp.exe downloaded successfully");
    }

    // 2. Download & Extract ffmpeg
    let ffmpeg_path = bin_dir.join("ffmpeg.exe");
    if !ffmpeg_path.exists() {
        info!("Downloading ffmpeg release archive...");
        let temp_dir = std::env::temp_dir().join("ytd_setup");
        let _ = fs::create_dir_all(&temp_dir);
        let zip_path = temp_dir.join("ffmpeg.zip");

        let ffmpeg_url = "https://github.com/yt-dlp/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip";
        
        let mut cmd = std::process::Command::new("curl.exe");
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.arg("-L")
            .arg("-s")
            .arg("-o")
            .arg(&zip_path)
            .arg(ffmpeg_url);

        let status = cmd.status().map_err(|e| format!("Failed to run curl.exe for ffmpeg: {}", e))?;
        if !status.success() || !zip_path.exists() {
            let ps_script = format!(
                "[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -Uri '{}' -OutFile '{}'",
                ffmpeg_url, zip_path.display()
            );
            let mut ps_cmd = std::process::Command::new("powershell");
            ps_cmd.creation_flags(CREATE_NO_WINDOW);
            ps_cmd.arg("-NoProfile").arg("-Command").arg(&ps_script);
            let _ = ps_cmd.status();
        }

        // Extract using tar.exe
        info!("Extracting ffmpeg binaries...");
        let mut tar_cmd = std::process::Command::new("tar.exe");
        tar_cmd.creation_flags(CREATE_NO_WINDOW);
        tar_cmd.arg("-xf")
            .arg(&zip_path)
            .arg("-C")
            .arg(&temp_dir);
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

        let _ = fs::remove_dir_all(&temp_dir);
        info!("ffmpeg extracted to {:?}", bin_dir);
    }

    // 3. Download & Extract Deno
    let deno_path = bin_dir.join("deno.exe");
    if !deno_path.exists() && !is_in_path("deno") {
        info!("Downloading Deno release archive...");
        let temp_dir = std::env::temp_dir().join("ytd_deno_setup");
        let _ = fs::create_dir_all(&temp_dir);
        let zip_path = temp_dir.join("deno.zip");

        let deno_url = "https://github.com/denoland/deno/releases/latest/download/deno-x86_64-pc-windows-msvc.zip";

        let mut cmd = std::process::Command::new("curl.exe");
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.arg("-L")
            .arg("-s")
            .arg("-o")
            .arg(&zip_path)
            .arg(deno_url);

        let status = cmd.status().map_err(|e| format!("Failed to run curl.exe for Deno: {}", e))?;
        if !status.success() || !zip_path.exists() {
            let ps_script = format!(
                "[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -Uri '{}' -OutFile '{}'",
                deno_url, zip_path.display()
            );
            let mut ps_cmd = std::process::Command::new("powershell");
            ps_cmd.creation_flags(CREATE_NO_WINDOW);
            ps_cmd.arg("-NoProfile").arg("-Command").arg(&ps_script);
            let _ = ps_cmd.status();
        }

        info!("Extracting Deno binary...");
        let mut tar_cmd = std::process::Command::new("tar.exe");
        tar_cmd.creation_flags(CREATE_NO_WINDOW);
        tar_cmd.arg("-xf")
            .arg(&zip_path)
            .arg("-C")
            .arg(&temp_dir);
        let _ = tar_cmd.status();

        let extracted_deno = temp_dir.join("deno.exe");
        if !extracted_deno.exists() {
            // Fallback to PowerShell Expand-Archive
            let ps_script = format!(
                "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
                zip_path.display(), temp_dir.display()
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
            let ytdlp_path = bin_dir.join("yt-dlp.exe");
            let url = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";
            let mut cmd = std::process::Command::new("curl.exe");
            cmd.creation_flags(CREATE_NO_WINDOW);
            cmd.arg("-L").arg("-s").arg("-o").arg(&ytdlp_path).arg(url);
            if let Ok(status) = cmd.status() {
                if status.success() && ytdlp_path.exists() {
                    (true, "Replaced with latest release binary".to_string())
                } else {
                    (false, "Update failed".to_string())
                }
            } else {
                (false, "Update failed".to_string())
            }
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
            let temp_dir = std::env::temp_dir().join("ytd_deno_update");
            let _ = fs::create_dir_all(&temp_dir);
            let zip_path = temp_dir.join("deno.zip");
            let deno_url = "https://github.com/denoland/deno/releases/latest/download/deno-x86_64-pc-windows-msvc.zip";

            let mut cmd = std::process::Command::new("curl.exe");
            cmd.creation_flags(CREATE_NO_WINDOW);
            cmd.arg("-L").arg("-s").arg("-o").arg(&zip_path).arg(deno_url);

            let res = if let Ok(status) = cmd.status() {
                if status.success() && zip_path.exists() {
                    let mut tar_cmd = std::process::Command::new("tar.exe");
                    tar_cmd.creation_flags(CREATE_NO_WINDOW);
                    tar_cmd.arg("-xf").arg(&zip_path).arg("-C").arg(&temp_dir);
                    let _ = tar_cmd.status();

                    let extracted = temp_dir.join("deno.exe");
                    if !extracted.exists() {
                        let ps_script = format!(
                            "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
                            zip_path.display(), temp_dir.display()
                        );
                        let mut ps_cmd = std::process::Command::new("powershell");
                        ps_cmd.creation_flags(CREATE_NO_WINDOW);
                        ps_cmd.arg("-NoProfile").arg("-Command").arg(&ps_script);
                        let _ = ps_cmd.status();
                    }

                    if extracted.exists() {
                        let _ = fs::copy(&extracted, bin_dir.join("deno.exe"));
                        (true, "Installed latest release binary".to_string())
                    } else {
                        (false, "Extraction failed".to_string())
                    }
                } else {
                    (false, "Download failed".to_string())
                }
            } else {
                (false, "Download failed".to_string())
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
        let ps_cmd = format!("Set-Clipboard -Value '{}'", ext_path_str);
        let mut clip = std::process::Command::new("powershell");
        clip.creation_flags(CREATE_NO_WINDOW);
        clip.arg("-NoProfile").arg("-Command").arg(&ps_cmd);
        let _ = clip.output();

        // Open extension page in browser
        let mut browser_cmd = std::process::Command::new("cmd");
        browser_cmd.creation_flags(CREATE_NO_WINDOW);
        browser_cmd.arg("/C").arg("start").arg("edge://extensions");
        let _ = browser_cmd.output();

        // Open Explorer with manifest.json or folder selected
        let manifest_path = ext_abs_path.join("manifest.json");
        if manifest_path.exists() {
            let mut exp = std::process::Command::new("explorer.exe");
            exp.creation_flags(CREATE_NO_WINDOW);
            exp.arg(format!("/select,{}", manifest_path.display()));
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
}
