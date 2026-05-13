use axum::extract::State;
use axum::Json;
use serde_json::Value;
use std::time::UNIX_EPOCH;

use super::router::AppState;
use crate::printer::state::EventKind;

pub async fn delete_logs(State(state): State<AppState>) -> Json<Value> {
    let mut s = state.printer_state.write().await;
    s.events.clear();
    drop(s);
    crate::db::clear_events(&state.db).await;
    Json(serde_json::json!({ "deleted": true }))
}

pub async fn get_logs(State(state): State<AppState>) -> Json<Value> {
    let db_events = crate::db::query_events(&state.db, 500).await;
    let s = state.printer_state.read().await;
    let mem_events = s.events.clone();
    let mem_total = s.events_total;
    drop(s);

    let last_db_ts = db_events
        .last()
        .and_then(|e| e.timestamp.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut logs: Vec<Value> = db_events
        .iter()
        .map(|e| fmt_event(e))
        .collect();

    for e in &mem_events {
        let ts = e.timestamp.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        if ts > last_db_ts {
            logs.push(fmt_event(e));
        }
    }

    Json(serde_json::json!({ "logs": logs, "total": mem_total }))
}

fn fmt_event(e: &crate::printer::state::PrinterEvent) -> Value {
    let ts = e.timestamp.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let kind = match &e.kind {
        EventKind::Loaded(s) => s.clone(),
        other => format!("{other:?}"),
    };
    let mut entry = serde_json::json!({
        "timestamp": ts,
        "kind": kind,
        "message": e.description,
    });
    if let Some(snap) = &e.snapshot {
        entry["snapshot"] = Value::String(snap.clone());
    }
    entry
}
