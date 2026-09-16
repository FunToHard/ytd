use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

pub const DEFAULT_PORT: u16 = 48123;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub video_download_dir: PathBuf,
    pub audio_download_dir: PathBuf,
    pub port: u16,
    pub auto_strip_mixes: bool,
    #[serde(default)]
    pub auto_start: bool,
}

impl Default for Config {
    fn default() -> Self {
        let music_dir = dirs::audio_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join("Music")))
            .unwrap_or_else(|| PathBuf::from("./Music"));

        let video_dir = dirs::download_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join("Downloads")))
            .map(|d| d.join("ytd"))
            .unwrap_or_else(|| PathBuf::from("./Downloads/ytd"));

        Self {
            video_download_dir: video_dir,
            audio_download_dir: music_dir,
            port: DEFAULT_PORT,
            auto_strip_mixes: true,
            auto_start: false,
        }
    }
}

impl Config {
    pub fn config_file_path() -> PathBuf {
        let dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ytd");
        let _ = fs::create_dir_all(&dir);
        dir.join("config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_file_path();
        if path.exists() {
            if let Ok(data) = fs::read_to_string(&path) {
                if let Ok(cfg) = serde_json::from_str::<Config>(&data) {
                    cfg.ensure_dirs();
                    return cfg;
                }
            }
        }

        let default_cfg = Config::default();
        default_cfg.save();
        default_cfg.ensure_dirs();
        default_cfg
    }

    pub fn save(&self) {
        let path = Self::config_file_path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, json);
        }
    }

    pub fn ensure_dirs(&self) {
        let _ = fs::create_dir_all(&self.video_download_dir);
        let _ = fs::create_dir_all(&self.audio_download_dir);
    }

    pub fn update_video_dir(&mut self, new_dir: PathBuf) {
        self.video_download_dir = new_dir;
        self.ensure_dirs();
        self.save();
    }

    pub fn update_auto_start(&mut self, enable: bool) {
        self.auto_start = enable;
        if let Err(e) = set_auto_start_registry(enable) {
            tracing::error!("Failed to update auto start in registry: {}", e);
        }
        self.save();
    }
}

#[cfg(windows)]
pub(crate) fn to_wide(s: &str) -> Vec<u16> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

pub fn set_auto_start_registry(enable: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::ERROR_SUCCESS;
        use windows_sys::Win32::System::Registry::{
            RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW,
            HKEY_CURRENT_USER, KEY_SET_VALUE, REG_SZ,
        };

        let subkey = to_wide(r"Software\Microsoft\Windows\CurrentVersion\Run");
        let val_name = to_wide("YTD");

        let mut hkey = std::ptr::null_mut();
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                KEY_SET_VALUE,
                &mut hkey,
            )
        };

        if status != ERROR_SUCCESS {
            return Err(format!("Failed to open Run registry key: error code {}", status));
        }

        let result = if enable {
            match std::env::current_exe() {
                Ok(exe) => {
                    let exe_str = format!("\"{}\"", exe.display());
                    let wide_data = to_wide(&exe_str);
                    let byte_len = (wide_data.len() * std::mem::size_of::<u16>()) as u32;

                    let set_status = unsafe {
                        RegSetValueExW(
                            hkey,
                            val_name.as_ptr(),
                            0,
                            REG_SZ,
                            wide_data.as_ptr() as *const u8,
                            byte_len,
                        )
                    };
                    if set_status == ERROR_SUCCESS {
                        tracing::info!("Registered YTD auto-start in registry: {}", exe_str);
                        Ok(())
                    } else {
                        let err_msg = format!("RegSetValueExW failed: error code {}", set_status);
                        tracing::error!("{}", err_msg);
                        Err(err_msg)
                    }
                }
                Err(e) => {
                    let err_msg = format!("Failed to determine current executable path: {}", e);
                    tracing::error!("{}", err_msg);
                    Err(err_msg)
                }
            }
        } else {
            let del_status = unsafe { RegDeleteValueW(hkey, val_name.as_ptr()) };
            if del_status == ERROR_SUCCESS {
                tracing::info!("Unregistered YTD auto-start from registry");
                Ok(())
            } else {
                Ok(())
            }
        };

        unsafe { RegCloseKey(hkey) };
        cleanup_legacy_corrupted_keys();
        result
    }
    #[cfg(not(windows))]
    {
        let _ = enable;
        Ok(())
    }
}

pub fn is_auto_start_registered() -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::ERROR_SUCCESS;
        use windows_sys::Win32::System::Registry::{
            RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER, KEY_QUERY_VALUE,
        };

        let subkey = to_wide(r"Software\Microsoft\Windows\CurrentVersion\Run");
        let val_name = to_wide("YTD");
        let mut hkey = std::ptr::null_mut();

        let status = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                KEY_QUERY_VALUE,
                &mut hkey,
            )
        };

        if status != ERROR_SUCCESS {
            return false;
        }

        let query_status = unsafe {
            RegQueryValueExW(
                hkey,
                val_name.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };

        unsafe { RegCloseKey(hkey) };
        query_status == ERROR_SUCCESS
    }
    #[cfg(not(windows))]
    false
}

pub fn cleanup_legacy_corrupted_keys() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Registry::{RegDeleteKeyW, HKEY_CURRENT_USER};
        let corrupt_run = to_wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run\"");
        unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, corrupt_run.as_ptr()) };

        let corrupt_aumid = to_wide("Software\\Classes\\AppUserModelId\\YTD\"");
        unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, corrupt_aumid.as_ptr()) };
    }
}

pub type SharedConfig = Arc<RwLock<Config>>;

pub fn init_shared_config() -> SharedConfig {
    Arc::new(RwLock::new(Config::load()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.port, DEFAULT_PORT);
        assert!(config.auto_strip_mixes);
        assert!(!config.auto_start);
        assert!(!config.video_download_dir.as_os_str().is_empty());
        assert!(!config.audio_download_dir.as_os_str().is_empty());
    }

    #[test]
    fn test_auto_start_win32_api() {
        let res = set_auto_start_registry(true);
        assert!(res.is_ok(), "Failed to set auto-start: {:?}", res);
        assert!(is_auto_start_registered(), "Expected auto-start to be registered");

        let res = set_auto_start_registry(false);
        assert!(res.is_ok(), "Failed to unset auto-start: {:?}", res);
        assert!(!is_auto_start_registered(), "Expected auto-start to be unregistered");
    }
}
