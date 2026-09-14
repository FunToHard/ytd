use crate::config::SharedConfig;
use crate::downloader::{get_active_downloads_count, Downloader};
use crate::sanitizer::{sanitize_url, DownloadTarget};
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use tracing::info;

#[derive(Clone)]
pub struct AppState {
    pub config: SharedConfig,
}

#[derive(Debug, Deserialize)]
pub struct DownloadPayload {
    pub url: String,
}

#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct HealthData {
    pub status: String,
    pub version: &'static str,
    pub active_downloads: usize,
}

#[derive(Debug, Serialize)]
pub struct ConfigData {
    pub video_download_dir: String,
    pub audio_download_dir: String,
    pub port: u16,
}

#[derive(Debug, Serialize)]
pub struct QueuedDownloadData {
    pub raw_url: String,
    pub clean_url: String,
    pub target: DownloadTarget,
    pub video_id: Option<String>,
    pub playlist_id: Option<String>,
    pub message: &'static str,
}

pub async fn run_server(config: SharedConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let port = {
        let cfg = config.read().unwrap();
        cfg.port
    };

    let state = AppState {
        config: config.clone(),
    };

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/config", get(config_handler))
        .route("/download", post(download_handler))
        .route("/dependencies/status", get(deps_status_handler))
        .route("/dependencies/install", post(deps_install_handler))
        .route("/helper/open-extension", post(open_extension_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    info!("YTD Daemon HTTP server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_handler() -> impl IntoResponse {
    let data = HealthData {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION"),
        active_downloads: get_active_downloads_count(),
    };

    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            data: Some(data),
            error: None,
        }),
    )
}

async fn config_handler(State(state): State<AppState>) -> impl IntoResponse {
    let cfg = state.config.read().unwrap();
    let data = ConfigData {
        video_download_dir: cfg.video_download_dir.to_string_lossy().to_string(),
        audio_download_dir: cfg.audio_download_dir.to_string_lossy().to_string(),
        port: cfg.port,
    };

    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            data: Some(data),
            error: None,
        }),
    )
}

async fn download_handler(
    State(state): State<AppState>,
    Json(payload): Json<DownloadPayload>,
) -> impl IntoResponse {
    match sanitize_url(&payload.url) {
        Ok(sanitized) => {
            let data = QueuedDownloadData {
                raw_url: sanitized.raw_url.clone(),
                clean_url: sanitized.clean_url.clone(),
                target: sanitized.target,
                video_id: sanitized.video_id.clone(),
                playlist_id: sanitized.playlist_id.clone(),
                message: "Download queued successfully",
            };

            Downloader::spawn_download(sanitized, state.config.clone());

            (
                StatusCode::OK,
                Json(ApiResponse {
                    success: true,
                    data: Some(data),
                    error: None,
                }),
            )
        }
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some(err),
            }),
        ),
    }
}

async fn deps_status_handler() -> impl IntoResponse {
    let status = crate::deps::check_dependencies();
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            data: Some(status),
            error: None,
        }),
    )
}

async fn deps_install_handler() -> impl IntoResponse {
    tokio::spawn(async {
        if let Err(e) = crate::deps::install_dependencies().await {
            tracing::error!("Dependency installation failed: {}", e);
            crate::notifier::notify_download_failed("Dependency Setup", &e);
        } else {
            crate::notifier::notify_download_completed(
                "yt-dlp & ffmpeg ready",
                &crate::deps::get_bin_dir().to_string_lossy(),
                false,
            );
        }
    });

    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            data: Some("Dependency installation started in background"),
            error: None,
        }),
    )
}

async fn open_extension_handler() -> impl IntoResponse {
    crate::deps::open_extension_helper();
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            data: Some("Opened extension manager and folder"),
            error: None,
        }),
    )
}
