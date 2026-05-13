use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use serde_json::Value;
use tracing::{info, warn};

use super::router::AppState;
use crate::config::{DestinationKind, EventToggles, NotificationDestination};
use crate::error::{AppError, ConfigError};
use crate::notifications::{discord, ntfy, webhook};

fn gen_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    format!("{:08x}", rng.gen::<u32>())
}

fn db_err(e: sqlx::Error) -> AppError {
    AppError::Config(ConfigError::Db(e))
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
    let id = dest.id.clone();
    crate::db::upsert_destination(&state.db, &dest).await.map_err(db_err)?;
    let mut config = state.config.write().await;
    config.notifications.destinations.push(dest);
    Ok(Json(serde_json::json!({ "success": true, "id": id })))
}

#[derive(Deserialize)]
pub struct UpdateDestinationReq {
    pub enabled: Option<bool>,
    pub label: Option<String>,
    pub ntfy_server: Option<String>,
    pub ntfy_topic: Option<String>,
    pub ntfy_tap_url: Option<String>,
    pub discord_webhook_url: Option<String>,
    pub webhook_url: Option<String>,
    pub toggles: Option<EventToggles>,
}

pub async fn update_destination(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateDestinationReq>,
) -> Result<Json<Value>, AppError> {
    info!("[notifications] update_destination called for {id}");
    let dest_clone = {
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
        if let Some(v) = req.ntfy_tap_url { dest.ntfy_tap_url = if v.is_empty() { None } else { Some(v) }; }
        if let Some(v) = req.discord_webhook_url { dest.discord_webhook_url = Some(v); }
        if let Some(v) = req.webhook_url { dest.webhook_url = Some(v); }
        if let Some(v) = req.toggles { dest.toggles = v; }

        dest.clone()
    };
    if let Err(e) = crate::db::upsert_destination(&state.db, &dest_clone).await {
        warn!("[notifications] upsert_destination failed for {id}: {e}");
        return Err(db_err(e));
    }
    info!("[notifications] updated destination {id}");
    Ok(Json(serde_json::json!({ "success": true })))
}

pub async fn delete_destination(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    {
        let mut config = state.config.write().await;
        let before = config.notifications.destinations.len();
        config.notifications.destinations.retain(|d| d.id != id);
        if config.notifications.destinations.len() == before {
            return Err(AppError::Validation(format!("destination {id} not found")));
        }
    }
    crate::db::delete_destination(&state.db, &id).await.map_err(db_err)?;
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
