use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use tracing::{error, info, warn};
use tauri_winrt_notification::{Duration, IconCrop, Toast};

pub const AUMID: &str = "YTD";

static ICON_PATH: OnceLock<PathBuf> = OnceLock::new();

const EMBEDDED_ICON_PNG: &[u8] = include_bytes!("../../extension/icons/icon48.png");

/// Ensures that the YTD AppUserModelId and icon are registered in Windows
/// so that Windows Toast notifications display "YTD" and the YTD icon.
pub fn init_app_identity() {
    let icon_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ytd");
    let _ = fs::create_dir_all(&icon_dir);

    let icon_file = icon_dir.join("icon48.png");
    if !icon_file.exists() || fs::metadata(&icon_file).map(|m| m.len()).unwrap_or(0) == 0 {
        let _ = fs::write(&icon_file, EMBEDDED_ICON_PNG);
    }

    let icon_path_str = icon_file.to_string_lossy().to_string();
    let _ = ICON_PATH.set(icon_file);

    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::ERROR_SUCCESS;
        use windows_sys::Win32::System::Registry::{
            RegCloseKey, RegCreateKeyExW, RegDeleteKeyW, RegSetValueExW,
            HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_RESERVED, REG_SZ,
        };

        // Register AppUserModelId in HKCU\Software\Classes\AppUserModelId\YTD
        let subkey = crate::config::to_wide(&format!(r"Software\Classes\AppUserModelId\{}", AUMID));
        let mut hkey = std::ptr::null_mut();

        let status = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                std::ptr::null(),
                REG_OPTION_RESERVED,
                KEY_WRITE,
                std::ptr::null(),
                &mut hkey,
                std::ptr::null_mut(),
            )
        };

        if status == ERROR_SUCCESS {
            let entries = [
                ("DisplayName", "YTD"),
                ("IconUri", &icon_path_str),
                ("IconBackgroundColor", "0"),
            ];

            for (name, val) in entries {
                let wide_name = crate::config::to_wide(name);
                let wide_val = crate::config::to_wide(val);
                let byte_len = (wide_val.len() * std::mem::size_of::<u16>()) as u32;

                unsafe {
                    RegSetValueExW(
                        hkey,
                        wide_name.as_ptr(),
                        0,
                        REG_SZ,
                        wide_val.as_ptr() as *const u8,
                        byte_len,
                    );
                }
            }
            unsafe { RegCloseKey(hkey) };
        } else {
            tracing::warn!("Failed to create AppUserModelId registry key: error code {}", status);
        }

        // Clean up legacy corrupted AppUserModelId\YTD" key if present
        let corrupt_key = crate::config::to_wide(&format!("Software\\Classes\\AppUserModelId\\{}\"", AUMID));
        unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, corrupt_key.as_ptr()) };

        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        // Create Start Menu shortcut so Windows 10/11 Action Center resolves the application icon & name
        if let Some(programs_dir) = dirs::data_dir().map(|d| {
            d.join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
        }) {
            let shortcut_path = programs_dir.join("YTD.lnk");
            if !shortcut_path.exists() {
                if let Ok(current_exe) = std::env::current_exe() {
                    let shortcut_str = shortcut_path.display().to_string().replace('\'', "''");
                    let exe_str = current_exe.display().to_string().replace('\'', "''");
                    let icon_str = icon_path_str.replace('\'', "''");
                    let script = format!(
                        "$ws = New-Object -ComObject WScript.Shell; $s = $ws.CreateShortcut('{}'); $s.TargetPath = '{}'; $s.IconLocation = '{},0'; $s.Save();",
                        shortcut_str,
                        exe_str,
                        icon_str
                    );
                    let mut cmd = std::process::Command::new("powershell");
                    cmd.creation_flags(CREATE_NO_WINDOW);
                    cmd.arg("-NoProfile")
                        .arg("-NonInteractive")
                        .arg("-Command")
                        .arg(&script);
                    let _ = cmd.output();
                }
            }
        }
    }

    info!("Initialized YTD application identity for Windows Toast notifications");
}

fn show_toast_interactive<F>(
    title: &str,
    text1: &str,
    text2: Option<&str>,
    duration: Duration,
    action_button: Option<&str>,
    on_activated: Option<F>,
) where
    F: Fn(Option<String>) -> tauri_winrt_notification::Result<()> + Send + 'static,
{
    let mut toast = Toast::new(AUMID)
        .title(title)
        .text1(text1)
        .duration(duration);

    if let Some(t2) = text2 {
        toast = toast.text2(t2);
    }

    if let Some(btn) = action_button {
        toast = toast.add_button(btn, "open_folder");
    }

    if let Some(handler) = on_activated {
        toast = toast.on_activated(handler);
    }

    if let Some(icon_path) = ICON_PATH.get() {
        if icon_path.exists() {
            toast = toast.icon(icon_path, IconCrop::Square, "YTD");
        }
    }

    let res = toast.show();

    // If showing under AUMID fails, fallback to PowerShell App ID with YTD icon
    if let Err(e) = res {
        warn!("Primary AUMID toast failed ({:?}), falling back", e);
        let mut fallback = Toast::new(Toast::POWERSHELL_APP_ID)
            .title(title)
            .text1(text1)
            .duration(duration);

        if let Some(t2) = text2 {
            fallback = fallback.text2(t2);
        }

        if let Some(btn) = action_button {
            fallback = fallback.add_button(btn, "open_folder");
        }

        if let Some(icon_path) = ICON_PATH.get() {
            if icon_path.exists() {
                fallback = fallback.icon(icon_path, IconCrop::Square, "YTD");
            }
        }

        if let Err(err) = fallback.show() {
            error!("Fallback toast also failed: {:?}", err);
        }
    }
}

fn show_toast(title: &str, text1: &str, text2: Option<&str>, duration: Duration) {
    show_toast_interactive::<fn(Option<String>) -> tauri_winrt_notification::Result<()>>(
        title, text1, text2, duration, None, None,
    );
}

pub fn notify_download_started(title: &str, is_music: bool) {
    let category = if is_music { "YT Music" } else { "YouTube" };
    info!("Notification: Download started - [{}] {}", category, title);
    show_toast(
        &format!("YTD: Downloading ({})", category),
        title,
        None,
        Duration::Short,
    );
}

pub fn notify_download_completed(title: &str, destination: &str, is_music: bool) {
    let category = if is_music { "MP3 Audio" } else { "MP4 Video" };
    info!("Notification: Download completed - [{}] {} -> {}", category, title, destination);

    let path = std::path::PathBuf::from(destination);
    let target_folder = if path.is_file() {
        path.parent().map(|p| p.to_path_buf()).unwrap_or(path)
    } else {
        path
    };

    let folder_to_open = target_folder.clone();
    let folder_display = target_folder.to_string_lossy().to_string();

    show_toast_interactive(
        &format!("YTD: Download Finished ({})", category),
        title,
        Some(&format!("Saved to: {} (click to open)", folder_display)),
        Duration::Short,
        Some("Open Folder"),
        Some(move |_action| {
            info!("Notification clicked: opening folder {}", folder_to_open.display());
            if let Err(e) = open::that(&folder_to_open) {
                error!("Failed to open folder {}: {}", folder_to_open.display(), e);
            }
            Ok(())
        }),
    );
}

pub fn notify_download_failed(title: &str, err: &str) {
    error!("Notification: Download failed - {} (Error: {})", title, err);
    show_toast(
        "YTD: Download Failed",
        title,
        Some(err),
        Duration::Long,
    );
}

pub fn notify_download_cancelled(title: &str) {
    info!("Notification: Download cancelled - {}", title);
    show_toast(
        "YTD: Download Cancelled",
        title,
        None,
        Duration::Short,
    );
}

pub fn notify_setup_required() {
    show_toast(
        "YTD: Setup Required",
        "yt-dlp, ffmpeg, or Deno not detected.",
        Some("Right-click the YTD tray icon to install with 1 click."),
        Duration::Long,
    );
}

pub fn notify_info(title: &str, message: &str) {
    show_toast(
        title,
        message,
        None,
        Duration::Short,
    );
}

pub fn notify_update_available(version: &str) {
    show_toast(
        "YTD: Update Available",
        &format!("Version {} is available to install.", version),
        Some("Right-click the YTD tray icon to install."),
        Duration::Long,
    );
}

