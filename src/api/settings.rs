use axum::extract::State;
use axum::Json;
use serde_json::Value;
use tracing::warn;

use super::router::AppState;
use crate::error::{AppError, ConfigError};

pub async fn get_settings(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let config = state.config.read().await;
    let value = serde_json::to_value(&*config).unwrap_or(Value::Null);
    Ok(Json(value))
}

/// update config + save
pub async fn update_settings(
    State(state): State<AppState>,
    Json(req): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let mut config = state.config.write().await;

    if let Some(printer) = req.get("printer") {
        if let Some(ip) = printer.get("ip").and_then(|v| v.as_str()) {
            config.printer.ip = ip.to_string();
        }
        if let Some(pincode) = printer.get("pincode").and_then(|v| v.as_str()) {
            config.printer.pincode = pincode.to_string();
        }
    }

    if let Some(detection) = req.get("detection") {
        if let Some(v) = detection.get("enabled").and_then(|v| v.as_bool()) {
            config.detection.enabled = v;
        }
        if let Some(v) = detection.get("notify_threshold").and_then(|v| v.as_f64()) {
            config.detection.notify_threshold = v.clamp(0.0, 1.0);
        }
        if let Some(v) = detection.get("pause_threshold").and_then(|v| v.as_f64()) {
            config.detection.pause_threshold = v.clamp(0.0, 1.0);
        }
        if let Some(v) = detection.get("interval_secs").and_then(|v| v.as_u64()) {
            config.detection.interval_secs = (v as u32).max(5);
        }
        if let Some(v) = detection.get("obico_url").and_then(|v| v.as_str()) {
            config.detection.obico_url = v.to_string();
        }
    }

    if let Some(server) = req.get("server") {
        if let Some(v) = server.get("host").and_then(|v| v.as_str()) {
            config.server.host = v.to_string();
        }
        if let Some(v) = server.get("port").and_then(|v| v.as_u64()) {
            config.server.port = v as u16;
        }
    }

    if let Some(v) = req.get("logging").and_then(|l| l.get("level")).and_then(|v| v.as_str()) {
        config.logging.level = v.to_string();
    }

    let det_config = config.detection.clone();
    let det_enabled = config.detection.enabled;
    let printer_cfg = config.printer.clone();
    let host = config.server.host.clone();
    let port = config.server.port;
    let log_level = config.logging.level.clone();
    let onboarding_complete = config.onboarding_complete;
    drop(config);

    if let Err(e) = crate::db::save_printer_config(&state.db, &printer_cfg).await {
        warn!("failed to persist printer config: {e}");
        return Err(AppError::Config(ConfigError::Db(e)));
    }
    if let Err(e) = crate::db::save_detection_config(&state.db, &det_config).await {
        warn!("failed to persist detection config: {e}");
        return Err(AppError::Config(ConfigError::Db(e)));
    }
    if let Err(e) = crate::db::save_server_config(&state.db, &host, port, &log_level, onboarding_complete).await {
        warn!("failed to persist server config: {e}");
        return Err(AppError::Config(ConfigError::Db(e)));
    }

    let _ = state.det_config_tx.send(det_config);
    let _ = state.det_enabled_tx.send(det_enabled);

    Ok(Json(Value::Null))
}
