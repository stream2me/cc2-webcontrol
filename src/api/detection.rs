use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::Value;
use tracing::{debug, warn};

use super::router::AppState;
use crate::config::ExcludeZone;
use crate::detection::grouping;
use crate::detection::obico::ObicoClient;
use crate::error::AppError;
use crate::printer::state::PrinterState;

#[derive(serde::Serialize)]
pub struct DetectionStatusResponse {
    pub enabled: bool,
    pub notify_threshold: f64,
    pub pause_threshold: f64,
    pub interval_secs: u32,
    pub confirmation_frames: u32,
}

pub async fn get_status(
    State(state): State<AppState>,
) -> Result<Json<DetectionStatusResponse>, AppError> {
    let config = state.det_config_rx.borrow();
    Ok(Json(DetectionStatusResponse {
        enabled: *state.det_enabled_rx.borrow(),
        notify_threshold: config.notify_threshold,
        pause_threshold: config.pause_threshold,
        interval_secs: config.interval_secs,
        confirmation_frames: config.confirmation_frames,
    }))
}

pub async fn toggle(
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let current = *state.det_enabled_rx.borrow();
    state.det_enabled_tx.send(!current).map_err(|_| {
        AppError::Detection(crate::error::DetectionError::NotRunning)
    })?;
    debug!("API: detection toggled to {}", !current);
    Ok(Json(Value::Null))
}

#[derive(Deserialize)]
pub struct DetectionConfigRequest {
    pub notify_threshold: Option<f64>,
    pub pause_threshold: Option<f64>,
    pub interval_secs: Option<u32>,
    pub confirmation_frames: Option<u32>,
}

pub async fn update_config(
    State(state): State<AppState>,
    Json(req): Json<DetectionConfigRequest>,
) -> Result<Json<Value>, AppError> {
    let mut config = state.det_config_rx.borrow().clone();

    if let Some(v) = req.notify_threshold {
        config.notify_threshold = v.clamp(0.0, 1.0);
    }
    if let Some(v) = req.pause_threshold {
        config.pause_threshold = v.clamp(0.0, 1.0);
    }
    if let Some(interval_secs) = req.interval_secs {
        config.interval_secs = interval_secs.max(5);
    }
    if let Some(confirmation_frames) = req.confirmation_frames {
        config.confirmation_frames = confirmation_frames.max(1);
    }

    if config.pause_threshold < config.notify_threshold {
        return Err(AppError::Validation(
            "pause_threshold must be >= notify_threshold".to_string(),
        ));
    }

    state.det_config_tx.send(config).map_err(|_| {
        AppError::Detection(crate::error::DetectionError::NotRunning)
    })?;

    debug!("API: detection config updated");
    Ok(Json(Value::Null))
}

/// detection history
#[derive(Deserialize)]
pub struct HistoryQuery {
    pub filename: Option<String>,
    pub limit: Option<usize>,
}

pub async fn get_history(
    Query(q): Query<HistoryQuery>,
) -> Result<Json<Value>, AppError> {
    let limit = q.limit.unwrap_or(200).min(500);
    let mut points = PrinterState::load_detection_history(limit);
    if let Some(ref filename) = q.filename {
        points.retain(|p| p.print_filename.as_deref() == Some(filename.as_str()));
    }
    let pts: Vec<_> = points.into_iter().collect();
    Ok(Json(serde_json::to_value(&pts).unwrap_or(Value::Array(vec![]))))
}

/// latest detection
pub async fn get_latest(
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let ps = state.printer_state.read().await;
    Ok(Json(serde_json::json!({
        "score": ps.detection_score,
        "detections": ps.latest_detections,
        "timestamp": ps.latest_detection_ts,
    })))
}

/// set zones
pub async fn set_zones(
    State(state): State<AppState>,
    Json(zones): Json<Vec<ExcludeZone>>,
) -> Result<Json<Value>, AppError> {
    let mut config = state.det_config_rx.borrow().clone();
    config.exclude_zones = zones;
    state.det_config_tx.send(config).map_err(|_| {
        AppError::Detection(crate::error::DetectionError::NotRunning)
    })?;
    debug!("API: exclusion zones updated");
    Ok(Json(Value::Null))
}

/// grouped detection points
#[derive(Deserialize)]
pub struct GroupedQuery {
    pub filename: Option<String>,
    pub limit: Option<usize>,
    pub window_secs: Option<u64>,
}

pub async fn get_grouped(
    Query(q): Query<GroupedQuery>,
) -> Result<Json<Value>, AppError> {
    let limit = q.limit.unwrap_or(500).min(1000);
    let window_secs = q.window_secs.unwrap_or(300);
    let mut points = PrinterState::load_detection_history(limit);
    if let Some(ref filename) = q.filename {
        points.retain(|p| p.print_filename.as_deref() == Some(filename.as_str()));
    }
    let pts: Vec<_> = points.into_iter().collect();
    let groups = grouping::group_detection_points(&pts, window_secs, 0.4);
    Ok(Json(serde_json::to_value(&groups).unwrap_or(Value::Array(vec![]))))
}

/// run detection
pub async fn run_detection(
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let frame = state.frame_buffer.read().await.clone();
    let Some(jpeg) = frame else {
        return Err(AppError::Detection(
            crate::error::DetectionError::ObicoFailed("no camera frame available".to_string()),
        ));
    };

    let config = state.det_config_rx.borrow().clone();
    let app_config = state.config.read().await;
    let port = app_config.server.port;
    drop(app_config);

    let obico = ObicoClient::new(&config.obico_url);
    let proxy_url = format!("http://127.0.0.1:{port}/api/camera/snapshot");

    match obico.analyze_snapshot(&proxy_url, &jpeg, &config.exclude_zones).await {
        Ok(result) => {
            let detections: Vec<Value> = result.detections.iter().map(|d| {
                serde_json::json!({
                    "x1": d.x1,
                    "y1": d.y1,
                    "x2": d.x2,
                    "y2": d.y2,
                    "confidence": d.confidence,
                })
            }).collect();

            Ok(Json(serde_json::json!({
                "score": result.score,
                "detections": detections,
            })))
        }
        Err(e) => {
            warn!("on-demand detection failed: {e}");
            Err(AppError::Detection(e))
        }
    }
}
