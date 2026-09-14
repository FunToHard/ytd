use crate::config::SharedConfig;
use crate::notifier::{
    notify_download_completed, notify_download_failed, notify_download_started,
};
use crate::sanitizer::{DownloadTarget, SanitizedRequest};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{error, info};

static ACTIVE_DOWNLOADS: AtomicUsize = AtomicUsize::new(0);

pub fn get_active_downloads_count() -> usize {
    ACTIVE_DOWNLOADS.load(Ordering::Relaxed)
}

pub struct Downloader;

impl Downloader {
    pub fn spawn_download(req: SanitizedRequest, config: SharedConfig) {
        tokio::spawn(async move {
            ACTIVE_DOWNLOADS.fetch_add(1, Ordering::SeqCst);
            let is_music = req.target == DownloadTarget::MusicAudio;
            let display_target = if is_music { "Music" } else { "Video" };
            info!("Queued {} download: {}", display_target, req.clean_url);

            notify_download_started(&req.clean_url, is_music);

            let (destination, is_playlist) = {
                let cfg = config.read().unwrap();
                let dest = if is_music {
                    cfg.audio_download_dir.clone()
                } else {
                    cfg.video_download_dir.clone()
                };
                let is_pl = req.playlist_id.is_some();
                (dest, is_pl)
            };

            match Self::execute_yt_dlp(&req, &destination, is_playlist).await {
                Ok(title) => {
                    let dest_display = destination.to_string_lossy().to_string();
                    let final_title = if title.trim().is_empty() {
                        req.clean_url.clone()
                    } else {
                        title
                    };
                    notify_download_completed(&final_title, &dest_display, is_music);
                    info!(
                        "Successfully downloaded: {} to {}",
                        final_title, dest_display
                    );
                }
                Err(err) => {
                    error!("Download failed for {}: {}", req.clean_url, err);
                    notify_download_failed(&req.clean_url, &err);
                }
            }

            ACTIVE_DOWNLOADS.fetch_sub(1, Ordering::SeqCst);
        });
    }

    async fn execute_yt_dlp(
        req: &SanitizedRequest,
        dest_dir: &PathBuf,
        is_playlist: bool,
    ) -> Result<String, String> {
        let is_music = req.target == DownloadTarget::MusicAudio;
        let dest_str = dest_dir.to_string_lossy().to_string();
        let _ = tokio::fs::create_dir_all(dest_dir).await;

        let mut cmd = Command::new("yt-dlp");
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        #[cfg(windows)]
        {
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        // Basic settings
        cmd.arg("--no-simulate")
            .arg("--windows-filenames")
            .arg("--no-mtime")
            .arg("--paths")
            .arg(&dest_str)
            .arg("--output")
            .arg("%(title)s [%(id)s].%(ext)s")
            .arg("--print")
            .arg("title");

        if !is_playlist {
            cmd.arg("--no-playlist");
        }

        if is_music {
            // High quality MP3 with embedded metadata and thumbnail
            cmd.arg("--extract-audio")
                .arg("--audio-format")
                .arg("mp3")
                .arg("--audio-quality")
                .arg("0")
                .arg("--embed-metadata")
                .arg("--embed-thumbnail");
        } else {
            // Best video quality muxed into MP4 with embedded metadata and thumbnail
            cmd.arg("-f")
                .arg("bv*[ext=mp4]+ba[ext=m4a]/b[ext=mp4] / bv*+ba/b")
                .arg("--merge-output-format")
                .arg("mp4")
                .arg("--embed-metadata")
                .arg("--embed-thumbnail");
        }

        cmd.arg(&req.clean_url);

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn yt-dlp (is it in PATH?): {}", e))?;

        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut captured_title = String::new();
        let mut error_lines = Vec::new();

        loop {
            tokio::select! {
                line = stdout_reader.next_line() => {
                    match line {
                        Ok(Some(l)) => {
                            if captured_title.is_empty() && !l.trim().is_empty() {
                                captured_title = l.trim().to_string();
                            }
                            info!("[yt-dlp stdout] {}", l);
                        }
                        Ok(None) => break,
                        Err(e) => {
                            error!("Error reading yt-dlp stdout: {}", e);
                            break;
                        }
                    }
                }
                err_line = stderr_reader.next_line() => {
                    if let Ok(Some(l)) = err_line {
                        info!("[yt-dlp stderr] {}", l);
                        if l.contains("ERROR:") {
                            error_lines.push(l);
                        }
                    }
                }
            }
        }

        let status = child
            .wait()
            .await
            .map_err(|e| format!("Process wait failed: {}", e))?;

        if status.success() {
            Ok(captured_title)
        } else {
            let error_summary = if !error_lines.is_empty() {
                error_lines.join("; ")
            } else {
                format!("yt-dlp exited with status: {}", status)
            };
            Err(error_summary)
        }
    }
}
