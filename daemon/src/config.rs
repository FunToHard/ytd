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
        set_auto_start_registry(enable);
        self.save();
    }
}

pub fn set_auto_start_registry(enable: bool) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let run_key = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run";

        if enable {
            if let Ok(exe) = std::env::current_exe() {
                let exe_str = exe.to_string_lossy().to_string();
                let cmd_str = format!(
                    "reg add \"{}\" /v \"YTD\" /t REG_SZ /d \"\\\"{}\\\"\" /f",
                    run_key, exe_str
                );
                let mut cmd = std::process::Command::new("cmd");
                cmd.creation_flags(CREATE_NO_WINDOW);
                cmd.arg("/C").arg(cmd_str);
                let _ = cmd.output();
            }
        } else {
            let cmd_str = format!("reg delete \"{}\" /v \"YTD\" /f", run_key);
            let mut cmd = std::process::Command::new("cmd");
            cmd.creation_flags(CREATE_NO_WINDOW);
            cmd.arg("/C").arg(cmd_str);
            let _ = cmd.output();
        }
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
}
