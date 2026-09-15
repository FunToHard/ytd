#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod deps;
mod downloader;
mod notifier;
mod sanitizer;
mod server;
mod tray;

use config::init_shared_config;
use tokio::sync::broadcast;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ytd_daemon=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("Starting YTD Desktop Daemon...");

    // Setup local %APPDATA%/ytd/bin in PATH
    deps::setup_bin_path();

    // Register AUMID and icon for Windows Toast notifications
    notifier::init_app_identity();

    // Check dependency health on startup
    let dep_status = deps::check_dependencies();
    if !dep_status.all_ready {
        info!("Dependencies missing: yt-dlp={}, ffmpeg={}", dep_status.ytdlp_available, dep_status.ffmpeg_available);
        notifier::notify_setup_required();
    } else {
        info!("All dependencies verified (yt-dlp & ffmpeg ready)");
    }

    let config = init_shared_config();
    {
        let cfg = config.read().unwrap();
        info!("Audio Download Dir: {:?}", cfg.audio_download_dir);
        info!("Video Download Dir: {:?}", cfg.video_download_dir);
        info!("Listening Port: {}", cfg.port);
    }

    let (tx_exit, mut rx_exit) = broadcast::channel::<()>(2);
    let tx_exit_tray = tx_exit.clone();

    // Create multithreaded Tokio runtime
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    // Spawn HTTP Server inside tokio runtime
    let config_server = config.clone();
    let tx_exit_server = tx_exit.clone();
    rt.spawn(async move {
        let mut rx_server_exit = tx_exit_server.subscribe();
        let server_future = server::run_server(config_server);

        tokio::select! {
            res = server_future => {
                if let Err(e) = res {
                    error!("Server error: {}", e);
                }
            }
            _ = rx_server_exit.recv() => {
                info!("Server received shutdown signal");
            }
        }
    });

    // Spawn Background Auto-Update Check
    rt.spawn(async {
        use github_auto_updater::{AutoUpdaterEngine, UpdateOptions};
        use std::time::Duration;

        // Wait 10 seconds after startup before initial background check
        tokio::time::sleep(Duration::from_secs(10)).await;

        let current_ver = env!("CARGO_PKG_VERSION");
        let options = UpdateOptions::new("FunToHard", "ytd", current_ver);
        let engine = AutoUpdaterEngine::new(options);

        let (tx_events, mut rx_events) = tokio::sync::mpsc::channel(10);
        engine.start_background_check(Duration::from_secs(24 * 3600), tx_events);

        while let Some(event) = rx_events.recv().await {
            if let github_auto_updater::UpdateEvent::UpdateAvailable(release) = event {
                notifier::notify_update_available(&release.tag_name);
            }
        }
    });

    // Run system tray on the main thread
    let tray_config = config.clone();
    let tray_thread = std::thread::spawn(move || {
        if let Err(e) = tray::run_tray(tray_config, tx_exit_tray) {
            error!("System tray encountered error: {}", e);
        }
    });

    // Wait on runtime for exit signal (e.g. from tray or Ctrl+C)
    rt.block_on(async {
        tokio::select! {
            _ = rx_exit.recv() => {
                info!("Shutdown initiated by tray menu");
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Ctrl+C detected, shutting down");
                let _ = tx_exit.send(());
            }
        }
    });

    info!("Shutting down YTD Desktop Daemon. Goodbye!");
    let _ = tray_thread.join();

    Ok(())
}
