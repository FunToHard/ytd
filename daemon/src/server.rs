use crate::config::SharedConfig;
use crate::downloader::{get_active_downloads_count, get_queued_downloads_count, Downloader};
use crate::sanitizer::{sanitize_url, DownloadTarget};
use axum::{
    extract::State,
    http::{
        header::{HeaderName, CONTENT_TYPE},
        Method, StatusCode,
    },
    middleware::{from_fn, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::info;

pub const EXPECTED_CLIENT_HEADER: &str = "ytd-browser-extension";

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
    pub queued_downloads: usize,
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
        let cfg = config.read().unwrap_or_else(|e| e.into_inner());
        cfg.port
    };

    let state = AppState {
        config: config.clone(),
    };

    // SEC-01: Restrict CORS to browser extension origins only.
    // Prohibits arbitrary web pages from executing cross-origin requests to the local daemon.
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin, _| {
            let s = origin.to_str().unwrap_or("");
            s.starts_with("chrome-extension://") || s.starts_with("moz-extension://")
        }))
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([
            CONTENT_TYPE,
            HeaderName::from_static("x-ytd-client"),
        ]);

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/config", get(config_handler))
        .route("/download", post(download_handler))
        .route("/dependencies/status", get(deps_status_handler))
        .route("/dependencies/install", post(deps_install_handler))
        .route("/dependencies/update", post(deps_update_handler))
        .layer(cors)
        .layer(from_fn(validate_client_header))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    info!("YTD Daemon HTTP server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn validate_client_header(
    req: axum::extract::Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<ApiResponse<()>>)> {
    // /health endpoint is exempt to allow simple daemon ping checks
    if req.uri().path() == "/health" {
        return Ok(next.run(req).await);
    }

    let client_hdr = req
        .headers()
        .get("x-ytd-client")
        .and_then(|h| h.to_str().ok());

    if client_hdr == Some(EXPECTED_CLIENT_HEADER) {
        Ok(next.run(req).await)
    } else {
        Err((
            StatusCode::FORBIDDEN,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some("Access denied: missing or invalid X-YTD-Client header".to_string()),
            }),
        ))
    }
}

async fn health_handler() -> impl IntoResponse {
    let data = HealthData {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION"),
        active_downloads: get_active_downloads_count(),
        queued_downloads: get_queued_downloads_count(),
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
    let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
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
            match Downloader::spawn_download(sanitized.clone(), state.config.clone()) {
                Ok(_) => {
                    let data = QueuedDownloadData {
                        raw_url: sanitized.raw_url,
                        clean_url: sanitized.clean_url,
                        target: sanitized.target,
                        video_id: sanitized.video_id,
                        playlist_id: sanitized.playlist_id,
                        message: "Download queued successfully",
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
                Err(err) => (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(ApiResponse {
                        success: false,
                        data: None,
                        error: Some(err),
                    }),
                ),
            }
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
                "yt-dlp, ffmpeg & Deno ready",
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

async fn deps_update_handler() -> impl IntoResponse {
    match crate::deps::update_dependencies().await {
        Ok(result) => (
            StatusCode::OK,
            Json(ApiResponse {
                success: true,
                data: Some(result),
                error: None,
            }),
        ),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some(err),
            }),
        ),
    }
}
