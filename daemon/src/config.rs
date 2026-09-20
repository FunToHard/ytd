use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

pub const DEFAULT_PORT: u16 = 48123;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub video_download_dir: PathBuf,
    pub audio_download_dir: PathBuf,
    pub port: u16,
    pub auto_strip_mixes: bool,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default = "default_true")]
    pub single_track_default: bool,
    #[serde(default = "default_true")]
    pub auto_update: bool,
}

impl Default for Config {
    fn default() -> Self {
        let fallback_base = dirs::home_dir()
            .or_else(dirs::data_dir)
            .unwrap_or_else(|| {
                std::env::var("USERPROFILE")
                    .map(PathBuf::from)
                    .unwrap_or_else(|_| PathBuf::from("C:\\"))
            });

        let music_dir = dirs::audio_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join("Music")))
            .unwrap_or_else(|| fallback_base.join("Music"));

        let video_dir = dirs::download_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join("Downloads")))
            .map(|d| d.join("ytd"))
            .unwrap_or_else(|| fallback_base.join("Downloads").join("ytd"));

        Self {
            video_download_dir: video_dir,
            audio_download_dir: music_dir,
            port: DEFAULT_PORT,
            auto_strip_mixes: true,
            auto_start: false,
            single_track_default: true,
            auto_update: true,
        }
    }
}

impl Config {
    pub fn config_file_path() -> PathBuf {
        let dir = dirs::config_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
            .unwrap_or_else(|| {
                std::env::var("APPDATA")
                    .map(PathBuf::from)
                    .unwrap_or_else(|_| PathBuf::from("C:\\ProgramData"))
            })
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
        let tmp_path = path.with_extension("tmp");
        if let Ok(json) = serde_json::to_string_pretty(self) {
            if fs::write(&tmp_path, json).is_ok() {
                let _ = fs::rename(&tmp_path, &path);
            }
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

    pub fn update_single_track_default(&mut self, enable: bool) {
        self.single_track_default = enable;
        self.save();
    }

    pub fn update_auto_update(&mut self, enable: bool) {
        self.auto_update = enable;
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
            RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW,
            HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_RESERVED, REG_SZ,
        };

        let subkey = to_wide(r"Software\Microsoft\Windows\CurrentVersion\Run");
        let val_name = to_wide("YTD");

        let mut hkey = std::ptr::null_mut();
        let status = if enable {
            unsafe {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    subkey.as_ptr(),
                    0,
                    std::ptr::null(),
                    REG_OPTION_RESERVED,
                    KEY_SET_VALUE,
                    std::ptr::null(),
                    &mut hkey,
                    std::ptr::null_mut(),
                )
            }
        } else {
            unsafe {
                RegOpenKeyExW(
                    HKEY_CURRENT_USER,
                    subkey.as_ptr(),
                    0,
                    KEY_SET_VALUE,
                    &mut hkey,
                )
            }
        };

        if status != ERROR_SUCCESS {
            if !enable {
                cleanup_legacy_corrupted_keys();
                return Ok(());
            }
            return Err(format!("Failed to open/create Run registry key: error code {}", status));
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

        let approved_subkey = to_wide(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run");
        let mut approved_hkey = std::ptr::null_mut();
        if enable {
            use windows_sys::Win32::System::Registry::REG_BINARY;
            let approved_status = unsafe {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    approved_subkey.as_ptr(),
                    0,
                    std::ptr::null(),
                    REG_OPTION_RESERVED,
                    KEY_SET_VALUE,
                    std::ptr::null(),
                    &mut approved_hkey,
                    std::ptr::null_mut(),
                )
            };
            if approved_status == ERROR_SUCCESS {
                let enabled_bytes: [u8; 12] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
                unsafe {
                    RegSetValueExW(
                        approved_hkey,
                        val_name.as_ptr(),
                        0,
                        REG_BINARY,
                        enabled_bytes.as_ptr(),
                        enabled_bytes.len() as u32,
                    );
                    RegCloseKey(approved_hkey);
                }
            }
        } else {
            let approved_status = unsafe {
                RegOpenKeyExW(
                    HKEY_CURRENT_USER,
                    approved_subkey.as_ptr(),
                    0,
                    KEY_SET_VALUE,
                    &mut approved_hkey,
                )
            };
            if approved_status == ERROR_SUCCESS {
                unsafe {
                    RegDeleteValueW(approved_hkey, val_name.as_ptr());
                    RegCloseKey(approved_hkey);
                }
            }
        }

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
        if query_status != ERROR_SUCCESS {
            return false;
        }

        // Check if Windows Settings or Task Manager disabled it in StartupApproved\Run
        let approved_subkey = to_wide(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run");
        let mut approved_hkey = std::ptr::null_mut();
        let app_status = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                approved_subkey.as_ptr(),
                0,
                KEY_QUERY_VALUE,
                &mut approved_hkey,
            )
        };
        if app_status == ERROR_SUCCESS {
            let mut data = [0u8; 12];
            let mut data_len = data.len() as u32;
            let query_app = unsafe {
                RegQueryValueExW(
                    approved_hkey,
                    val_name.as_ptr(),
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    data.as_mut_ptr(),
                    &mut data_len,
                )
            };
            unsafe { RegCloseKey(approved_hkey) };
            if query_app == ERROR_SUCCESS && data_len > 0 {
                // If lowest bit of first byte is set (e.g. 0x01 or 0x03), disabled in Windows Settings
                if (data[0] & 1) != 0 {
                    return false;
                }
            }
        }

        true
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
        assert!(config.single_track_default);
        assert!(config.auto_update);
        assert!(!config.video_download_dir.as_os_str().is_empty());
        assert!(!config.audio_download_dir.as_os_str().is_empty());
    }

    #[test]
    fn test_auto_update_serde() {
        let json = r#"{
            "video_download_dir": "C:\\videos",
            "audio_download_dir": "C:\\music",
            "port": 48123,
            "auto_strip_mixes": true
        }"#;
        let cfg: Config = serde_json::from_str(json).expect("Deserialization should succeed");
        assert!(cfg.auto_update);

        let json_disabled = r#"{
            "video_download_dir": "C:\\videos",
            "audio_download_dir": "C:\\music",
            "port": 48123,
            "auto_strip_mixes": true,
            "auto_update": false
        }"#;
        let cfg_disabled: Config = serde_json::from_str(json_disabled).expect("Deserialization should succeed");
        assert!(!cfg_disabled.auto_update);
    }

    #[test]
    fn test_single_track_default_serde() {
        let json = r#"{
            "video_download_dir": "C:\\videos",
            "audio_download_dir": "C:\\music",
            "port": 48123,
            "auto_strip_mixes": true
        }"#;
        let cfg: Config = serde_json::from_str(json).expect("Deserialization should succeed");
        assert!(cfg.single_track_default);

        let json_disabled = r#"{
            "video_download_dir": "C:\\videos",
            "audio_download_dir": "C:\\music",
            "port": 48123,
            "auto_strip_mixes": true,
            "single_track_default": false
        }"#;
        let cfg_disabled: Config = serde_json::from_str(json_disabled).expect("Deserialization should succeed");
        assert!(!cfg_disabled.single_track_default);
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
