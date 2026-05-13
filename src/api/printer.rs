use axum::extract::State;
use axum::Json;
use serde::Deserialize;
use serde_json::Value;
use tracing::debug;

use super::router::AppState;
use crate::error::AppError;

#[derive(serde::Serialize)]
pub struct PrinterStatusResponse {
    pub connected: bool,
    pub connected_raw: bool,
    pub connected_ws: bool,
    pub printer_id: String,
    pub printer_ip: String,
    pub state: Value,
    pub phase: crate::printer::state::PhaseInfo,
}

pub async fn get_status(
    State(state): State<AppState>,
) -> Result<Json<PrinterStatusResponse>, AppError> {
    let printer_state = state.printer_state.read().await;
    let full = serde_json::to_value(&printer_state.full).unwrap_or(Value::Null);
    let connected = printer_state.connected;
    let connected_raw = printer_state.connected_raw;
    let connected_ws = printer_state.connected_ws;
    let phase = crate::printer::state::build_phase_info(
        printer_state.full.machine_status.status,
        printer_state.full.machine_status.sub_status,
        &printer_state.full.print_status.state,
    );
    drop(printer_state);

    Ok(Json(PrinterStatusResponse {
        connected,
        connected_raw,
        connected_ws,
        printer_id: state.manager.printer_id().await,
        printer_ip: state.manager.printer_ip().await,
        state: full,
        phase,
    }))
}

#[derive(Deserialize)]
pub struct PrintRequest {
    pub filename: String,
    pub storage_media: String,
    #[serde(default = "default_plate")]
    pub plate: String,
    #[serde(default)]
    pub tray_id: Option<i64>,
    /// 0-based canvas slot (`t` in slot_map)
    #[serde(default)]
    pub tray_slot: Option<i64>,
    #[serde(default)]
    pub canvas_id: i64,
    #[serde(default)]
    pub timelapse: bool,
    #[serde(default = "default_true")]
    pub bedlevel_force: bool,
}

fn default_plate() -> String { "textured".to_string() }
fn default_true() -> bool { true }

pub async fn start_print(
    State(state): State<AppState>,
    Json(req): Json<PrintRequest>,
) -> Result<Json<Value>, AppError> {
    debug!("API: start_print {} plate={} canvas={} slot={:?} tray={:?}",
        req.filename, req.plate, req.canvas_id, req.tray_slot, req.tray_id);

    // fetch thumbnail pre-print; method 1045 often fails once print starts
    let needs_thumb = !state.printer_state.read().await
        .thumbnail_cache.contains_key(&req.filename);
    if needs_thumb {
        if let Ok(data) = state.manager.get_file_thumbnail("local", &req.filename).await {
            let thumb = data.get("thumbnail").and_then(|v| v.as_str()).unwrap_or("");
            if !thumb.is_empty() {
                state.printer_state.write().await
                    .thumbnail_cache.insert(req.filename.clone(), thumb.to_string());
            }
        }
    }

    state.manager.start_print(
        &req.filename, &req.storage_media, &req.plate,
        req.tray_id, req.tray_slot, req.canvas_id,
        req.timelapse, req.bedlevel_force,
    ).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn pause_print(
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    debug!("API: pause_print");
    state.manager.pause().await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn resume_print(
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    debug!("API: resume_print");
    state.manager.resume().await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn stop_print(
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    debug!("API: stop_print");
    state.manager.stop_print().await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct HomeRequest {
    pub axes: String,
}

pub async fn home_axes(
    State(state): State<AppState>,
    Json(req): Json<HomeRequest>,
) -> Result<Json<Value>, AppError> {
    let axes = req.axes.trim().to_lowercase();
    if axes.is_empty() || !axes.chars().all(|c| matches!(c, 'x' | 'y' | 'z')) {
        return Err(AppError::Validation("axes must be a non-empty combination of x, y, z".to_string()));
    }
    debug!("API: home_axes axes={axes}");
    state.manager.home_axes(&axes).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct JogRequest {
    pub axis: String,
    pub distance: f64,
}

pub async fn jog_axis(
    State(state): State<AppState>,
    Json(req): Json<JogRequest>,
) -> Result<Json<Value>, AppError> {
    let axis = req.axis.trim().to_lowercase();
    if !matches!(axis.as_str(), "x" | "y" | "z") {
        return Err(AppError::Validation("axis must be x, y, or z".to_string()));
    }
    if !req.distance.is_finite() || req.distance == 0.0 {
        return Err(AppError::Validation("distance must be a non-zero finite number".to_string()));
    }
    debug!("API: jog_axis axis={axis} distance={}", req.distance);
    state.manager.jog_axis(&axis, req.distance).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct LedRequest {
    pub power: i64,
}

pub async fn set_led(
    State(state): State<AppState>,
    Json(req): Json<LedRequest>,
) -> Result<Json<Value>, AppError> {
    debug!("API: set_led power={}", req.power);
    state.manager.set_led(req.power != 0).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct FanRequest {
    pub name: String,
    pub speed: u8,
}

pub async fn set_fan(
    State(state): State<AppState>,
    Json(req): Json<FanRequest>,
) -> Result<Json<Value>, AppError> {
    debug!("API: set_fan {}={}", req.name, req.speed);
    state.manager.set_fan(&req.name, req.speed).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct SpeedModeRequest {
    pub mode: u8,
}

pub async fn set_speed_mode(
    State(state): State<AppState>,
    Json(req): Json<SpeedModeRequest>,
) -> Result<Json<Value>, AppError> {
    debug!("API: set_speed_mode mode={}", req.mode);
    state.manager.set_speed_mode(req.mode).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct FileListQuery {
    pub storage: Option<String>,
    // native page semantics (1-based)
    pub page_number: Option<i64>,
    pub page_size: Option<i64>,
    // legacy offset/limit  -  translated to page for backwards compat
    pub offset: Option<i64>,
    pub limit: Option<i64>,
}

pub async fn get_files(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<FileListQuery>,
) -> Result<Json<Value>, AppError> {
    let storage = query.storage.as_deref().unwrap_or("local");
    let (page_number, page_size) = if let (Some(pn), Some(ps)) = (query.page_number, query.page_size) {
        (pn.max(1), ps.max(1))
    } else {
        let page_size = query.limit.unwrap_or(50).max(1);
        let offset = query.offset.unwrap_or(0).max(0);
        let page_number = (offset / page_size) + 1;
        (page_number, page_size)
    };
    let result = state.manager.get_file_list(storage, page_number, page_size).await?;
    Ok(Json(result))
}

#[derive(Deserialize)]
pub struct FileDetailQuery {
    pub storage: Option<String>,
    pub filename: String,
}

pub async fn get_file_detail(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<FileDetailQuery>,
) -> Result<Json<Value>, AppError> {
    if query.filename.is_empty() {
        return Err(AppError::Validation("filename is required".to_string()));
    }
    let storage = query.storage.as_deref().unwrap_or("local");
    let data = state.manager.get_file_info(storage, &query.filename).await?;
    Ok(Json(data))
}

pub async fn get_history(
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let data = state.manager.get_print_history().await?;
    let history = data.get("history_task_list")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(Json(serde_json::json!({ "history": history })))
}

pub async fn canvas_refresh(
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    debug!("API: canvas_refresh");
    let data = state.manager.canvas_refresh().await?;
    Ok(Json(data))
}

#[derive(Deserialize)]
pub struct AutoRefillRequest {
    pub enabled: bool,
}

pub async fn set_canvas_auto_refill(
    State(state): State<AppState>,
    Json(req): Json<AutoRefillRequest>,
) -> Result<Json<Value>, AppError> {
    debug!("API: set_canvas_auto_refill enabled={}", req.enabled);
    state.manager.set_ams_auto_refill(req.enabled).await?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct ThumbnailQuery {
    pub storage: Option<String>,
    pub filename: String,
}

pub async fn get_thumbnail(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ThumbnailQuery>,
) -> Result<Json<Value>, AppError> {
    if query.filename.is_empty() {
        return Err(AppError::Validation("filename is required".to_string()));
    }
    let storage = query.storage.as_deref().unwrap_or("local");

    {
        let ps = state.printer_state.read().await;
        if let Some(cached) = ps.thumbnail_cache.get(&query.filename) {
            return Ok(Json(serde_json::json!({
                "thumbnail": cached,
                "filename": query.filename,
            })));
        }
    }

    let data = state.manager.get_file_thumbnail(storage, &query.filename).await?;
    let thumbnail = data.get("thumbnail").and_then(|v| v.as_str()).unwrap_or("").to_string();

    if !thumbnail.is_empty() {
        state.printer_state.write().await
            .thumbnail_cache.insert(query.filename.clone(), thumbnail.clone());
    }

    Ok(Json(serde_json::json!({
        "thumbnail": thumbnail,
        "filename": query.filename,
    })))
}
