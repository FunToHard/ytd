use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::info;

static IS_INSTALLING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, serde::Serialize)]
pub struct DependencyStatus {
    pub ytdlp_available: bool,
    pub ffmpeg_available: bool,
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

    let ytdlp_available = ytdlp_in_bin || is_in_path("yt-dlp");
    let ffmpeg_available = ffmpeg_in_bin || is_in_path("ffmpeg");

    DependencyStatus {
        ytdlp_available,
        ffmpeg_available,
        all_ready: ytdlp_available && ffmpeg_available,
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

    setup_bin_path();
    Ok(())
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
