use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::Value;
use tracing::warn;

use super::router::AppState;
use crate::config::{DestinationKind, EventToggles, NotificationDestination};
use crate::error::AppError;
use crate::notifications::{discord, ntfy, webhook};

fn gen_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    format!("{:08x}", rng.gen::<u32>())
}

fn config_err(e: impl std::fmt::Display) -> AppError {
    AppError::Config(crate::error::ConfigError::Load(
        config::ConfigError::Message(e.to_string()),
    ))
}

pub async fn list_destinations(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let config = state.config.read().await;
    Ok(Json(
        serde_json::to_value(&config.notifications.destinations).unwrap_or(Value::Array(vec![])),
    ))
}

pub async fn create_destination(
    State(state): State<AppState>,
    Json(mut dest): Json<NotificationDestination>,
) -> Result<Json<Value>, AppError> {
    dest.id = gen_id();
    let mut config = state.config.write().await;
    let id = dest.id.clone();
    config.notifications.destinations.push(dest);
    if let Err(e) = config.save("config.toml") {
        warn!("failed to save config: {e}");
        return Err(config_err(e));
    }
    Ok(Json(serde_json::json!({ "success": true, "id": id })))
}

#[derive(Deserialize)]
pub struct UpdateDestinationReq {
    pub enabled: Option<bool>,
    pub label: Option<String>,
    pub ntfy_server: Option<String>,
    pub ntfy_topic: Option<String>,
    pub discord_webhook_url: Option<String>,
    pub webhook_url: Option<String>,
    pub toggles: Option<EventToggles>,
}

pub async fn update_destination(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateDestinationReq>,
) -> Result<Json<Value>, AppError> {
    let mut config = state.config.write().await;
    let dest = config
        .notifications
        .destinations
        .iter_mut()
        .find(|d| d.id == id)
        .ok_or_else(|| AppError::Validation(format!("destination '{id}' not found")))?;

    if let Some(v) = req.enabled { dest.enabled = v; }
    if let Some(v) = req.label { dest.label = v; }
    if let Some(v) = req.ntfy_server { dest.ntfy_server = Some(v); }
    if let Some(v) = req.ntfy_topic { dest.ntfy_topic = Some(v); }
    if let Some(v) = req.discord_webhook_url { dest.discord_webhook_url = Some(v); }
    if let Some(v) = req.webhook_url { dest.webhook_url = Some(v); }
    if let Some(v) = req.toggles { dest.toggles = v; }

    if let Err(e) = config.save("config.toml") {
        warn!("failed to save config after updating destination '{id}': {e}");
        return Err(config_err(e));
    }
    Ok(Json(serde_json::json!({ "success": true })))
}

pub async fn delete_destination(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let mut config = state.config.write().await;
    let before = config.notifications.destinations.len();
    config.notifications.destinations.retain(|d| d.id != id);
    if config.notifications.destinations.len() == before {
        return Err(AppError::Validation(format!("destination {id} not found")));
    }
    if let Err(e) = config.save("config.toml") {
        warn!("failed to save config: {e}");
        return Err(config_err(e));
    }
    Ok(Json(serde_json::json!({ "success": true })))
}

pub async fn test_destination(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let config = state.config.read().await;
    let dest = config
        .notifications
        .destinations
        .iter()
        .find(|d| d.id == id)
        .cloned()
        .ok_or_else(|| AppError::Validation(format!("destination {id} not found")))?;
    drop(config);

    match dest.kind {
        DestinationKind::Ntfy => ntfy::send_test(&dest).await?,
        DestinationKind::Discord => discord::send_test(&dest).await?,
        DestinationKind::Webhook => webhook::send(&dest, "CC2 Monitor", "Test notification - webhook is working").await?,
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}
