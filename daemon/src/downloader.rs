use crate::config::SharedConfig;
use crate::notifier::{
    notify_download_cancelled, notify_download_completed, notify_download_failed,
    notify_download_started,
};
use crate::sanitizer::{DownloadTarget, SanitizedRequest};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{oneshot, Semaphore};
use tracing::{error, info, warn};

static ACTIVE_DOWNLOADS: AtomicUsize = AtomicUsize::new(0);
static QUEUED_DOWNLOADS: AtomicUsize = AtomicUsize::new(0);
static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(1);

pub const MAX_QUEUED_DOWNLOADS: usize = 50;
pub const MAX_CONCURRENT_DOWNLOADS: usize = 3;

static DOWNLOAD_SEMAPHORE: OnceLock<Arc<Semaphore>> = OnceLock::new();

fn get_download_semaphore() -> &'static Arc<Semaphore> {
    DOWNLOAD_SEMAPHORE.get_or_init(|| Arc::new(Semaphore::new(MAX_CONCURRENT_DOWNLOADS)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    Queued,
    Downloading,
    Converting,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveDownloadInfo {
    pub id: u64,
    pub title: String,
    pub clean_url: String,
    pub progress: Option<u8>,
    pub status: DownloadStatus,
    pub target: DownloadTarget,
}

struct DownloadTask {
    id: u64,
    url: String,
    title: String,
    target: DownloadTarget,
    progress: Option<u8>,
    status: DownloadStatus,
    cancel_tx: Option<oneshot::Sender<()>>,
    child_pid: Option<u32>,
    dest_dir: PathBuf,
    video_id: Option<String>,
}

struct DownloadTracker {
    tasks: Mutex<HashMap<u64, DownloadTask>>,
}

impl DownloadTracker {
    fn new() -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
        }
    }

    fn register_task(
        &self,
        id: u64,
        url: String,
        title: String,
        target: DownloadTarget,
        dest_dir: PathBuf,
        video_id: Option<String>,
        cancel_tx: oneshot::Sender<()>,
    ) {
        let mut tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        tasks.insert(
            id,
            DownloadTask {
                id,
                url,
                title,
                target,
                progress: None,
                status: DownloadStatus::Queued,
                cancel_tx: Some(cancel_tx),
                child_pid: None,
                dest_dir,
                video_id,
            },
        );
    }

    fn update_task_status(&self, id: u64, status: DownloadStatus) {
        let mut tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(task) = tasks.get_mut(&id) {
            task.status = status;
        }
    }

    fn update_task_title(&self, id: u64, title: String) {
        let mut tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(task) = tasks.get_mut(&id) {
            task.title = title;
        }
    }

    fn update_task_progress(&self, id: u64, progress: u8) {
        let mut tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(task) = tasks.get_mut(&id) {
            task.progress = Some(progress);
        }
    }

    fn set_child_pid(&self, id: u64, pid: u32) {
        let mut tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(task) = tasks.get_mut(&id) {
            task.child_pid = Some(pid);
        }
    }

    fn get_active_downloads(&self) -> Vec<ActiveDownloadInfo> {
        let tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        let mut active: Vec<ActiveDownloadInfo> = tasks
            .values()
            .filter(|t| matches!(t.status, DownloadStatus::Queued | DownloadStatus::Downloading | DownloadStatus::Converting))
            .map(|t| ActiveDownloadInfo {
                id: t.id,
                title: t.title.clone(),
                clean_url: t.url.clone(),
                progress: t.progress,
                status: t.status,
                target: t.target,
            })
            .collect();
        active.sort_by_key(|t| t.id);
        active
    }

    fn cancel_task(&self, id: u64) -> Option<(String, PathBuf, Option<String>, Option<u32>)> {
        let mut tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(task) = tasks.get_mut(&id) {
            if matches!(task.status, DownloadStatus::Queued | DownloadStatus::Downloading | DownloadStatus::Converting) {
                task.status = DownloadStatus::Cancelled;
                if let Some(tx) = task.cancel_tx.take() {
                    let _ = tx.send(());
                }
                return Some((task.title.clone(), task.dest_dir.clone(), task.video_id.clone(), task.child_pid));
            }
        }
        None
    }

    fn cancel_all(&self) -> Vec<(u64, String, PathBuf, Option<String>, Option<u32>)> {
        let mut tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        let mut cancelled = Vec::new();
        for task in tasks.values_mut() {
            if matches!(task.status, DownloadStatus::Queued | DownloadStatus::Downloading | DownloadStatus::Converting) {
                task.status = DownloadStatus::Cancelled;
                if let Some(tx) = task.cancel_tx.take() {
                    let _ = tx.send(());
                }
                cancelled.push((task.id, task.title.clone(), task.dest_dir.clone(), task.video_id.clone(), task.child_pid));
            }
        }
        cancelled
    }

    fn remove_task(&self, id: u64) {
        let mut tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        tasks.remove(&id);
    }
}

static TRACKER: OnceLock<DownloadTracker> = OnceLock::new();

fn get_tracker() -> &'static DownloadTracker {
    TRACKER.get_or_init(DownloadTracker::new)
}

pub fn get_active_downloads_count() -> usize {
    ACTIVE_DOWNLOADS.load(Ordering::Relaxed)
}

pub fn get_queued_downloads_count() -> usize {
    QUEUED_DOWNLOADS.load(Ordering::Relaxed)
}

/// Truncate title cleanly with `..` if longer than max_chars
pub fn truncate_title(title: &str, max_chars: usize) -> String {
    let trimmed = title.trim();
    if trimmed.chars().count() <= max_chars {
        trimmed.to_string()
    } else {
        let prefix: String = trimmed.chars().take(max_chars.saturating_sub(2)).collect();
        format!("{}..", prefix.trim_end())
    }
}

/// Format the cancellation label for Option B (e.g. `✕ Cancel: title on th.. [67%]`)
pub fn format_cancel_label(info: &ActiveDownloadInfo, max_title_chars: usize) -> String {
    let truncated = truncate_title(&info.title, max_title_chars);
    match info.status {
        DownloadStatus::Queued => format!("✕ Cancel: [Queued] {}", truncated),
        DownloadStatus::Converting => format!("✕ Cancel: {} [Converting]", truncated),
        _ => {
            if let Some(pct) = info.progress {
                format!("✕ Cancel: {} [{}%]", truncated, pct)
            } else {
                format!("✕ Cancel: {}", truncated)
            }
        }
    }
}

/// Parse progress percentage from yt-dlp stdout lines
pub fn parse_progress(line: &str) -> Option<u8> {
    if let Some(pos) = line.find("[progress]") {
        let part = &line[pos + 10..].trim();
        if let Some(pct_str) = part.split('%').next() {
            if let Ok(pct) = pct_str.trim().parse::<f32>() {
                return Some(pct.clamp(0.0, 100.0).round() as u8);
            }
        }
    }

    if line.starts_with("[download]") && line.contains('%') {
        let mut tokens = line.split_whitespace();
        tokens.next(); // skip "[download]"
        if let Some(pct_token) = tokens.next() {
            if let Some(num_str) = pct_token.strip_suffix('%') {
                if let Ok(pct) = num_str.parse::<f32>() {
                    return Some(pct.clamp(0.0, 100.0).round() as u8);
                }
            }
        }
    }
    None
}

/// Determines if a request should be treated as a playlist download.
/// If `single_track_default` is true and a specific video/track ID is present in the request (`v=...`),
/// it resolves to `false` (single track).
/// Dedicated playlist URLs (without `video_id`) always resolve to `true`.
pub fn resolve_is_playlist(req: &SanitizedRequest, single_track_default: bool) -> bool {
    if single_track_default && req.video_id.is_some() {
        false
    } else {
        req.playlist_id.is_some()
    }
}

#[cfg(windows)]
fn kill_process_tree(pid: u32) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    info!("Executing taskkill /F /T /PID {}", pid);
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}

#[cfg(not(windows))]
fn kill_process_tree(pid: u32) {
    let _ = std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .output();
}

fn remove_file_with_retry(path: &Path, max_attempts: usize) -> std::io::Result<()> {
    for attempt in 0..max_attempts {
        match std::fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(e) if attempt + 1 < max_attempts && e.raw_os_error() == Some(32) => {
                // Windows ERROR_SHARING_VIOLATION: wait briefly for terminating processes to release handles
                std::thread::sleep(std::time::Duration::from_millis(50 * (attempt as u64 + 1)));
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

fn cleanup_partial_files(dest_dir: &Path, video_id: Option<&str>, title: &str) {
    if let Ok(entries) = std::fs::read_dir(dest_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext == "part" || ext == "ytdl" {
                    let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
                    let matches = if let Some(id) = video_id {
                        filename.contains(&format!("[{}]", id)) || filename.contains(id)
                    } else if !title.is_empty() && !title.starts_with("http://") && !title.starts_with("https://") {
                        let prefix: String = title.chars().take(15).collect();
                        prefix.chars().count() >= 3 && filename.starts_with(&prefix)
                    } else {
                        false
                    };

                    if matches {
                        if let Err(e) = remove_file_with_retry(&path, 3) {
                            warn!("Failed to delete residual partial file {:?}: {}", path, e);
                        } else {
                            info!("Cleaned up cancelled partial file: {:?}", path);
                        }
                    }
                }
            }
        }
    }
}

struct ActiveDownloadGuard {
    task_id: u64,
}

impl Drop for ActiveDownloadGuard {
    fn drop(&mut self) {
        ACTIVE_DOWNLOADS.fetch_sub(1, Ordering::SeqCst);
        get_tracker().remove_task(self.task_id);
    }
}

pub struct Downloader;

impl Downloader {
    pub fn spawn_download(req: SanitizedRequest, config: SharedConfig) -> Result<u64, String> {
        let queued = QUEUED_DOWNLOADS.load(Ordering::Relaxed);
        if queued >= MAX_QUEUED_DOWNLOADS {
            return Err(format!(
                "Download queue is full ({} pending downloads). Please wait for active downloads to finish.",
                queued
            ));
        }

        let is_music = req.target == DownloadTarget::MusicAudio;
        let destination = {
            let cfg = config.read().unwrap_or_else(|e| e.into_inner());
            if is_music {
                cfg.audio_download_dir.clone()
            } else {
                cfg.video_download_dir.clone()
            }
        };

        let task_id = NEXT_TASK_ID.fetch_add(1, Ordering::SeqCst);
        let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();

        get_tracker().register_task(
            task_id,
            req.clean_url.clone(),
            req.clean_url.clone(), // Initial title is URL until parsed
            req.target,
            destination.clone(),
            req.video_id.clone(),
            cancel_tx,
        );

        QUEUED_DOWNLOADS.fetch_add(1, Ordering::SeqCst);
        let display_target = if is_music { "Music" } else { "Video" };
        info!("Enqueued {} download [ID {}]: {}", display_target, task_id, req.clean_url);

        let cfg_clone = config.clone();
        tokio::spawn(async move {
            let sem = get_download_semaphore().clone();
            
            // Wait for permit or cancel signal
            let _permit = tokio::select! {
                res = sem.acquire_owned() => match res {
                    Ok(permit) => permit,
                    Err(_) => {
                        QUEUED_DOWNLOADS.fetch_sub(1, Ordering::SeqCst);
                        get_tracker().remove_task(task_id);
                        return;
                    }
                },
                _ = &mut cancel_rx => {
                    info!("Task {} cancelled while in queue", task_id);
                    QUEUED_DOWNLOADS.fetch_sub(1, Ordering::SeqCst);
                    get_tracker().remove_task(task_id);
                    return;
                }
            };

            QUEUED_DOWNLOADS.fetch_sub(1, Ordering::SeqCst);
            ACTIVE_DOWNLOADS.fetch_add(1, Ordering::SeqCst);
            let _active_guard = ActiveDownloadGuard { task_id };
            get_tracker().update_task_status(task_id, DownloadStatus::Downloading);

            notify_download_started(&req.clean_url, is_music);

            let single_track_default = {
                let cfg = cfg_clone.read().unwrap_or_else(|e| e.into_inner());
                cfg.single_track_default
            };
            let is_playlist = resolve_is_playlist(&req, single_track_default);

            match Self::execute_yt_dlp(task_id, &req, &destination, is_playlist, cancel_rx).await {
                Ok(title) => {
                    let dest_display = destination.to_string_lossy().to_string();
                    let final_title = if title.trim().is_empty() {
                        req.clean_url.clone()
                    } else {
                        title
                    };
                    get_tracker().update_task_status(task_id, DownloadStatus::Completed);
                    notify_download_completed(&final_title, &dest_display, is_music);
                    info!(
                        "Successfully downloaded [ID {}]: {} to {}",
                        task_id, final_title, dest_display
                    );
                }
                Err(err) => {
                    if err == "CANCELLED" {
                        info!("Download [ID {}] cancelled cleanly by user", task_id);
                    } else {
                        error!("Download failed for [ID {}] {}: {}", task_id, req.clean_url, err);
                        get_tracker().update_task_status(task_id, DownloadStatus::Failed);
                        notify_download_failed(&req.clean_url, &err);
                    }
                }
            }

            // _active_guard and _permit automatically dropped here
        });

        Ok(task_id)
    }

    pub fn cancel_download(id: u64) -> bool {
        if let Some((title, dest_dir, video_id, pid)) = get_tracker().cancel_task(id) {
            if let Some(p) = pid {
                kill_process_tree(p);
            }
            cleanup_partial_files(&dest_dir, video_id.as_deref(), &title);
            notify_download_cancelled(&title);
            true
        } else {
            false
        }
    }

    pub fn cancel_all_downloads() -> usize {
        let cancelled = get_tracker().cancel_all();
        let count = cancelled.len();
        for (_id, title, dest_dir, video_id, pid) in cancelled {
            if let Some(p) = pid {
                kill_process_tree(p);
            }
            cleanup_partial_files(&dest_dir, video_id.as_deref(), &title);
        }
        if count > 0 {
            notify_download_cancelled(&format!("All active downloads ({})", count));
        }
        count
    }

    pub fn get_active_downloads() -> Vec<ActiveDownloadInfo> {
        get_tracker().get_active_downloads()
    }

    async fn execute_yt_dlp(
        task_id: u64,
        req: &SanitizedRequest,
        dest_dir: &PathBuf,
        is_playlist: bool,
        mut cancel_rx: oneshot::Receiver<()>,
    ) -> Result<String, String> {
        let is_music = req.target == DownloadTarget::MusicAudio;
        let dest_str = dest_dir.to_string_lossy().to_string();
        let _ = tokio::fs::create_dir_all(dest_dir).await;

        let bin_dir = crate::deps::get_bin_dir();
        let ytdlp_path = bin_dir.join("yt-dlp.exe");
        let ytdlp_bin = if ytdlp_path.exists() {
            ytdlp_path
        } else {
            PathBuf::from("yt-dlp")
        };

        let mut cmd = Command::new(&ytdlp_bin);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

        #[cfg(windows)]
        {
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        // Pass verified ffmpeg location if present
        if bin_dir.join("ffmpeg.exe").exists() {
            cmd.arg("--ffmpeg-location").arg(&bin_dir);
        }

        // Basic settings: output newlines for real-time progress parsing
        cmd.arg("--no-simulate")
            .arg("--no-colors")
            .arg("--windows-filenames")
            .arg("--no-mtime")
            .arg("--newline")
            .arg("--progress-template")
            .arg("download:[progress] %(progress._percent_str)s")
            .arg("--progress-delta")
            .arg("1")
            .arg("--paths")
            .arg(&dest_str)
            .arg("--output")
            .arg("%(title)s [%(id)s].%(ext)s")
            .arg("--print")
            .arg("ytd_title:%(title)s");

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

        // SEC-02: Option terminator ensures clean_url is never parsed as a CLI flag
        cmd.arg("--");
        cmd.arg(&req.clean_url);

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn yt-dlp (is it in PATH?): {}", e))?;

        let pid = child.id();
        if let Some(p) = pid {
            get_tracker().set_child_pid(task_id, p);
        }

        let stdout = child.stdout.take().ok_or("Failed to capture stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to capture stderr")?;

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut captured_title = String::new();
        let mut error_lines = Vec::new();
        let mut was_cancelled = false;
        let mut stderr_eof = false;

        loop {
            tokio::select! {
                _ = &mut cancel_rx => {
                    info!("Cancellation signal received for task {}", task_id);
                    was_cancelled = true;
                    let _ = child.kill().await;
                    cleanup_partial_files(dest_dir, req.video_id.as_deref(), &captured_title);
                    break;
                }
                line = stdout_reader.next_line() => {
                    match line {
                        Ok(Some(l)) => {
                            let trimmed = l.trim();
                            if let Some(clean_title) = trimmed.strip_prefix("ytd_title:") {
                                let clean = clean_title.trim();
                                if !clean.is_empty() {
                                    captured_title = clean.to_string();
                                    get_tracker().update_task_title(task_id, captured_title.clone());
                                }
                            } else if captured_title.is_empty() && !trimmed.is_empty() && !trimmed.starts_with('[') && !trimmed.starts_with("download:") {
                                captured_title = trimmed.to_string();
                                get_tracker().update_task_title(task_id, captured_title.clone());
                            } else if let Some(pct) = parse_progress(trimmed) {
                                get_tracker().update_task_progress(task_id, pct);
                            } else if trimmed.contains("[ExtractAudio]") || trimmed.contains("[ffmpeg]") || trimmed.contains("[Merger]") {
                                get_tracker().update_task_status(task_id, DownloadStatus::Converting);
                            }
                            tracing::debug!("[yt-dlp stdout] {}", l);
                        }
                        Ok(None) => break,
                        Err(e) => {
                            error!("Error reading yt-dlp stdout: {}", e);
                            break;
                        }
                    }
                }
                err_line = stderr_reader.next_line(), if !stderr_eof => {
                    match err_line {
                        Ok(Some(l)) => {
                            tracing::debug!("[yt-dlp stderr] {}", l);
                            if l.contains("ERROR:") {
                                error_lines.push(l);
                            }
                        }
                        Ok(None) | Err(_) => {
                            stderr_eof = true;
                        }
                    }
                }
            }
        }

        // Drain any remaining stderr output after stdout closes
        while let Ok(Some(l)) = stderr_reader.next_line().await {
            tracing::debug!("[yt-dlp stderr trailing] {}", l);
            if l.contains("ERROR:") {
                error_lines.push(l);
            }
        }

        if was_cancelled {
            return Err("CANCELLED".to_string());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_progress_lines() {
        assert_eq!(
            parse_progress("[progress]  45.2%"),
            Some(45)
        );
        assert_eq!(
            parse_progress("[download]  67.0% of  120.45MiB at  10.23MiB/s ETA 00:11"),
            Some(67)
        );
        assert_eq!(
            parse_progress("[download] 100% of 15.00MiB"),
            Some(100)
        );
        assert_eq!(
            parse_progress("[download]   0.0%"),
            Some(0)
        );
        assert_eq!(parse_progress("[ExtractAudio] Destination: ..."), None);
        assert_eq!(parse_progress("Rick Astley - Never Gonna Give You Up"), None);
    }

    #[test]
    fn test_truncate_title() {
        assert_eq!(truncate_title("Short", 20), "Short");
        assert_eq!(
            truncate_title("another blog day on the internet", 20),
            "another blog day o.."
        );
        assert_eq!(truncate_title("   Hello World   ", 10), "Hello Wo..");
    }

    #[test]
    fn test_format_cancel_label() {
        let info = ActiveDownloadInfo {
            id: 1,
            title: "another blog day on the internet".to_string(),
            clean_url: "https://www.youtube.com/watch?v=123".to_string(),
            progress: Some(67),
            status: DownloadStatus::Downloading,
            target: DownloadTarget::Video,
        };
        let label = format_cancel_label(&info, 20);
        assert_eq!(label, "✕ Cancel: another blog day o.. [67%]");

        let queued_info = ActiveDownloadInfo {
            id: 2,
            title: "another blog day".to_string(),
            clean_url: "https://www.youtube.com/watch?v=123".to_string(),
            progress: None,
            status: DownloadStatus::Queued,
            target: DownloadTarget::Video,
        };
        assert_eq!(
            format_cancel_label(&queued_info, 20),
            "✕ Cancel: [Queued] another blog day"
        );

        let converting_info = ActiveDownloadInfo {
            id: 3,
            title: "Song Title".to_string(),
            clean_url: "https://music.youtube.com/watch?v=123".to_string(),
            progress: Some(99),
            status: DownloadStatus::Converting,
            target: DownloadTarget::MusicAudio,
        };
        assert_eq!(
            format_cancel_label(&converting_info, 20),
            "✕ Cancel: Song Title [Converting]"
        );
    }

    #[test]
    fn test_tracker_registration_and_cancellation() {
        let tracker = DownloadTracker::new();
        let (tx, mut rx) = oneshot::channel();
        tracker.register_task(
            42,
            "https://youtube.com/watch?v=test".to_string(),
            "Initial Title".to_string(),
            DownloadTarget::Video,
            PathBuf::from("C:\\temp"),
            Some("test".to_string()),
            tx,
        );

        assert_eq!(tracker.get_active_downloads().len(), 1);
        tracker.update_task_title(42, "Updated Title".to_string());
        tracker.update_task_progress(42, 50);

        let active = tracker.get_active_downloads();
        assert_eq!(active[0].title, "Updated Title");
        assert_eq!(active[0].progress, Some(50));

        let res = tracker.cancel_task(42);
        assert!(res.is_some());
        assert_eq!(rx.try_recv(), Ok(()));
        assert_eq!(tracker.get_active_downloads().len(), 0);
    }

    #[test]
    fn test_resolve_is_playlist() {
        use crate::sanitizer::{DownloadTarget, SanitizedRequest};

        // Case 1: URL with both video_id and playlist_id (e.g. watch?v=...&list=OLAK...)
        let req_combo = SanitizedRequest {
            raw_url: "https://music.youtube.com/watch?v=UsWKMa8bGx8&list=OLAK5uy_lFDtNmRi8kq8TtWYZ207VigtFdTW43FiM".to_string(),
            clean_url: "https://music.youtube.com/watch?v=UsWKMa8bGx8&list=OLAK5uy_lFDtNmRi8kq8TtWYZ207VigtFdTW43FiM".to_string(),
            target: DownloadTarget::MusicAudio,
            video_id: Some("UsWKMa8bGx8".to_string()),
            playlist_id: Some("OLAK5uy_lFDtNmRi8kq8TtWYZ207VigtFdTW43FiM".to_string()),
        };

        // When single_track_default is true -> should resolve to false (single track)
        assert!(!resolve_is_playlist(&req_combo, true));
        // When single_track_default is false -> should resolve to true (full playlist)
        assert!(resolve_is_playlist(&req_combo, false));

        // Case 2: Dedicated playlist URL (no video_id, e.g. /playlist?list=...)
        let req_playlist_only = SanitizedRequest {
            raw_url: "https://music.youtube.com/playlist?list=OLAK5uy_lFDtNmRi8kq8TtWYZ207VigtFdTW43FiM".to_string(),
            clean_url: "https://music.youtube.com/playlist?list=OLAK5uy_lFDtNmRi8kq8TtWYZ207VigtFdTW43FiM".to_string(),
            target: DownloadTarget::MusicAudio,
            video_id: None,
            playlist_id: Some("OLAK5uy_lFDtNmRi8kq8TtWYZ207VigtFdTW43FiM".to_string()),
        };

        // Should ALWAYS resolve to true regardless of single_track_default
        assert!(resolve_is_playlist(&req_playlist_only, true));
        assert!(resolve_is_playlist(&req_playlist_only, false));

        // Case 3: Single video only (no playlist_id)
        let req_single = SanitizedRequest {
            raw_url: "https://youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
            clean_url: "https://youtube.com/watch?v=dQw4w9WgXcQ".to_string(),
            target: DownloadTarget::Video,
            video_id: Some("dQw4w9WgXcQ".to_string()),
            playlist_id: None,
        };

        assert!(!resolve_is_playlist(&req_single, true));
        assert!(!resolve_is_playlist(&req_single, false));
    }

    #[test]
    fn test_cleanup_partial_files_unicode_and_isolation() {
        let temp_dir = std::env::temp_dir().join(format!(
            "ytd_cleanup_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&temp_dir);

        // File 1: Sibling with same 15-character prefix "The Daily Show "
        let sibling_part = temp_dir.join("The Daily Show #01 [sibling123].mp4.part");
        let _ = std::fs::write(&sibling_part, b"sibling part content");

        // File 2: Target download with Japanese / multi-byte title crossing byte 15
        let target_part = temp_dir.join("日本語のテスト動画です [target456].mp4.part");
        let _ = std::fs::write(&target_part, b"target part content");

        // Cancel target download: should cleanly remove target_part without panicking on multi-byte chars
        cleanup_partial_files(
            &temp_dir,
            Some("target456"),
            "日本語のテスト動画です",
        );

        assert!(!target_part.exists(), "Target partial file must be deleted");
        assert!(sibling_part.exists(), "Sibling partial file must be preserved");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
