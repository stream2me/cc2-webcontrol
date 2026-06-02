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
    let new_val = !current;
    state.det_enabled_tx.send(new_val).map_err(|_| {
        AppError::Detection(crate::error::DetectionError::NotRunning)
    })?;
    let det_to_save = {
        let mut cfg = state.config.write().await;
        cfg.detection.enabled = new_val;
        cfg.detection.clone()
    };
    if let Err(e) = crate::db::save_detection_config(&state.db, &det_to_save).await {
        warn!("failed to persist detection enabled: {e}");
    }
    debug!("API: detection toggled to {new_val}");
    Ok(Json(Value::Null))
}

#[derive(Deserialize)]
pub struct DetectionConfigRequest {
    pub enabled: Option<bool>,
    pub notify_threshold: Option<f64>,
    pub pause_threshold: Option<f64>,
    pub interval_secs: Option<u32>,
    pub confirmation_frames: Option<u32>,
}

pub async fn update_config(
    State(state): State<AppState>,
    Json(req): Json<DetectionConfigRequest>,
) -> Result<Json<Value>, AppError> {
    let det_to_save = {
        let mut cfg = state.config.write().await;
        if let Some(v) = req.enabled { cfg.detection.enabled = v; }
        if let Some(v) = req.notify_threshold { cfg.detection.notify_threshold = v.clamp(0.0, 1.0); }
        if let Some(v) = req.pause_threshold { cfg.detection.pause_threshold = v.clamp(0.0, 1.0); }
        if let Some(v) = req.interval_secs { cfg.detection.interval_secs = v.max(5); }
        if let Some(v) = req.confirmation_frames { cfg.detection.confirmation_frames = v.max(1); }

        if cfg.detection.pause_threshold < cfg.detection.notify_threshold {
            return Err(AppError::Validation(
                "pause_threshold must be >= notify_threshold".to_string(),
            ));
        }
        cfg.detection.clone()
    };

    state.det_config_tx.send(det_to_save.clone()).map_err(|_| {
        AppError::Detection(crate::error::DetectionError::NotRunning)
    })?;
    if let Err(e) = crate::db::save_detection_config(&state.db, &det_to_save).await {
        warn!("failed to persist detection config: {e}");
    }

    debug!("API: detection config updated");
    Ok(Json(Value::Null))
}

#[derive(Deserialize)]
pub struct HistoryQuery {
    pub filename: Option<String>,
    pub limit: Option<usize>,
}

pub async fn get_history(
    State(state): State<AppState>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<Value>, AppError> {
    let max_graph_pts = q.limit.unwrap_or(300).min(300);
    let raw_limit = if q.filename.is_some() { 20_000 } else { 5_000 };
    let points = crate::db::query_detection_points(
        &state.db,
        q.filename.as_deref(),
        raw_limit,
    )
    .await;
    let pts = crate::db::downsample_for_graph(&points, max_graph_pts);
    Ok(Json(serde_json::to_value(&pts).unwrap_or(Value::Array(vec![]))))
}

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

pub async fn set_zones(
    State(state): State<AppState>,
    Json(zones): Json<Vec<ExcludeZone>>,
) -> Result<Json<Value>, AppError> {
    let det_to_save = {
        let mut cfg = state.config.write().await;
        cfg.detection.exclude_zones = zones;
        cfg.detection.clone()
    };
    state.det_config_tx.send(det_to_save.clone()).map_err(|_| {
        AppError::Detection(crate::error::DetectionError::NotRunning)
    })?;
    if let Err(e) = crate::db::save_detection_config(&state.db, &det_to_save).await {
        warn!("failed to persist detection zones: {e}");
    }
    debug!("API: exclusion zones updated");
    Ok(Json(Value::Null))
}

#[derive(Deserialize)]
pub struct GroupedQuery {
    pub filename: Option<String>,
    pub limit: Option<usize>,
    pub window_secs: Option<u64>,
}

pub async fn get_grouped(
    State(state): State<AppState>,
    Query(q): Query<GroupedQuery>,
) -> Result<Json<Value>, AppError> {
    let limit = q.limit.unwrap_or(20_000).min(20_000);
    let window_secs = q.window_secs.unwrap_or(300);
    let points = crate::db::query_detection_points(
        &state.db,
        q.filename.as_deref(),
        limit,
    )
    .await;
    let groups = grouping::group_detection_points(&points, window_secs, 0.4);
    Ok(Json(serde_json::to_value(&groups).unwrap_or(Value::Array(vec![]))))
}

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
