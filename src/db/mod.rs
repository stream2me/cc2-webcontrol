use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
    Row, SqlitePool,
};
use std::str::FromStr;
use tracing::{info, warn};

use crate::config::{
    AppConfig, DetectionConfig, DestinationKind, EventToggles, NotificationDestination, PrinterConfig,
};
use crate::printer::state::{DetectionPoint, EventKind, PrinterEvent};

pub async fn init_db(path: &str) -> Result<SqlitePool, sqlx::Error> {
    let opts = SqliteConnectOptions::from_str(&format!("sqlite:{path}"))?
        .journal_mode(SqliteJournalMode::Wal)
        .create_if_missing(true);
    let pool = SqlitePool::connect_with(opts).await?;
    create_schema(&pool).await?;
    Ok(pool)
}

async fn create_schema(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS printer_config (
            id         INTEGER PRIMARY KEY CHECK (id = 1),
            ip         TEXT NOT NULL DEFAULT '',
            printer_id TEXT NOT NULL DEFAULT '',
            pincode    TEXT NOT NULL DEFAULT ''
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS detection_config (
            id                  INTEGER PRIMARY KEY CHECK (id = 1),
            enabled             INTEGER NOT NULL DEFAULT 1,
            interval_secs       INTEGER NOT NULL DEFAULT 15,
            notify_threshold    REAL    NOT NULL DEFAULT 0.5,
            pause_threshold     REAL    NOT NULL DEFAULT 0.7,
            confirmation_frames INTEGER NOT NULL DEFAULT 2,
            obico_url           TEXT    NOT NULL DEFAULT '',
            exclude_zones_json  TEXT    NOT NULL DEFAULT '[]'
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS server_config (
            id                  INTEGER PRIMARY KEY CHECK (id = 1),
            host                TEXT    NOT NULL DEFAULT '0.0.0.0',
            port                INTEGER NOT NULL DEFAULT 8484,
            log_level           TEXT    NOT NULL DEFAULT 'info',
            onboarding_complete INTEGER NOT NULL DEFAULT 0
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS notification_destinations (
            id                  TEXT PRIMARY KEY,
            kind                TEXT NOT NULL,
            enabled             INTEGER NOT NULL DEFAULT 1,
            label               TEXT NOT NULL DEFAULT '',
            ntfy_server         TEXT,
            ntfy_topic          TEXT,
            ntfy_tap_url        TEXT,
            discord_webhook_url TEXT,
            webhook_url         TEXT,
            toggles_json        TEXT NOT NULL DEFAULT '{}'
        )",
    )
    .execute(pool)
    .await?;
    // migration: add ntfy_tap_url if upgrading from older schema
    let _ = sqlx::query(
        "ALTER TABLE notification_destinations ADD COLUMN ntfy_tap_url TEXT",
    )
    .execute(pool)
    .await;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS detection_points (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ts INTEGER NOT NULL,
            score REAL NOT NULL,
            print_filename TEXT,
            snapshot TEXT,
            boxes_json TEXT NOT NULL DEFAULT '[]'
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_det_ts ON detection_points(ts)")
        .execute(pool)
        .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_det_fn ON detection_points(print_filename)")
        .execute(pool)
        .await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ts INTEGER NOT NULL,
            kind TEXT NOT NULL,
            msg TEXT NOT NULL,
            snapshot TEXT
        )",
    )
    .execute(pool)
    .await?;
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_evt_ts ON events(ts)")
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn insert_detection_point(pool: &SqlitePool, pt: &DetectionPoint) {
    let boxes_json = serde_json::to_string(&pt.boxes).unwrap_or_else(|_| "[]".to_string());
    if let Err(e) = sqlx::query(
        "INSERT INTO detection_points (ts, score, print_filename, snapshot, boxes_json)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(pt.ts as i64)
    .bind(pt.score)
    .bind(&pt.print_filename)
    .bind(&pt.snapshot)
    .bind(&boxes_json)
    .execute(pool)
    .await
    {
        warn!("[db] insert detection_point failed: {e}");
    }
}

pub async fn insert_event(pool: &SqlitePool, ts: u64, kind: &str, msg: &str, snapshot: Option<&str>) {
    if let Err(e) = sqlx::query(
        "INSERT INTO events (ts, kind, msg, snapshot) VALUES (?, ?, ?, ?)",
    )
    .bind(ts as i64)
    .bind(kind)
    .bind(msg)
    .bind(snapshot)
    .execute(pool)
    .await
    {
        warn!("[db] insert event failed: {e}");
    }
}

pub async fn query_detection_points(
    pool: &SqlitePool,
    filename: Option<&str>,
    limit: usize,
) -> Vec<DetectionPoint> {
    let rows = if let Some(fname) = filename {
        sqlx::query(
            "SELECT ts, score, print_filename, snapshot, boxes_json
             FROM detection_points WHERE print_filename = ?
             ORDER BY ts DESC LIMIT ?",
        )
        .bind(fname)
        .bind(limit as i64)
        .fetch_all(pool)
        .await
    } else {
        sqlx::query(
            "SELECT ts, score, print_filename, snapshot, boxes_json
             FROM detection_points ORDER BY ts DESC LIMIT ?",
        )
        .bind(limit as i64)
        .fetch_all(pool)
        .await
    };

    let rows = match rows {
        Ok(r) => r,
        Err(e) => {
            warn!("[db] query detection_points failed: {e}");
            return Vec::new();
        }
    };

    let mut points: Vec<DetectionPoint> = rows
        .iter()
        .map(|row| {
            let ts: i64 = row.get("ts");
            let score: f64 = row.get("score");
            let print_filename: Option<String> = row.get("print_filename");
            let snapshot: Option<String> = row.get("snapshot");
            let boxes_json: String = row.get("boxes_json");
            let boxes = serde_json::from_str(&boxes_json).unwrap_or_default();
            DetectionPoint { ts: ts as u64, score, print_filename, snapshot, boxes }
        })
        .collect();

    points.reverse();
    points
}

pub async fn query_events(pool: &SqlitePool, limit: usize) -> Vec<PrinterEvent> {
    let rows = sqlx::query(
        "SELECT ts, kind, msg, snapshot FROM events ORDER BY ts DESC LIMIT ?",
    )
    .bind(limit as i64)
    .fetch_all(pool)
    .await;

    let rows = match rows {
        Ok(r) => r,
        Err(e) => {
            warn!("[db] query events failed: {e}");
            return Vec::new();
        }
    };

    let mut events: Vec<PrinterEvent> = rows
        .iter()
        .map(|row| {
            let ts: i64 = row.get("ts");
            let kind: String = row.get("kind");
            let msg: String = row.get("msg");
            let snapshot: Option<String> = row.get("snapshot");
            let timestamp = std::time::UNIX_EPOCH + std::time::Duration::from_secs(ts as u64);
            PrinterEvent {
                timestamp,
                kind: EventKind::Loaded(kind),
                description: msg,
                snapshot,
            }
        })
        .collect();

    events.reverse();
    events
}

pub async fn count_events(pool: &SqlitePool) -> u64 {
    let row = sqlx::query("SELECT COUNT(*) as cnt FROM events")
        .fetch_one(pool)
        .await;
    match row {
        Ok(r) => {
            let cnt: i64 = r.get("cnt");
            cnt as u64
        }
        Err(_) => 0,
    }
}

pub async fn clear_events(pool: &SqlitePool) {
    if let Err(e) = sqlx::query("DELETE FROM events").execute(pool).await {
        warn!("[db] clear events failed: {e}");
    }
}

pub async fn delete_detection_point_by_snapshot(pool: &SqlitePool, snapshot: &str) {
    if let Err(e) = sqlx::query("DELETE FROM detection_points WHERE snapshot = ?")
        .bind(snapshot)
        .execute(pool)
        .await
    {
        warn!("[db] delete detection_point by snapshot failed: {e}");
    }
}

pub async fn clear_detection_points(pool: &SqlitePool) {
    if let Err(e) = sqlx::query("DELETE FROM detection_points").execute(pool).await {
        warn!("[db] clear detection_points failed: {e}");
    }
}

pub fn downsample_for_graph(points: &[DetectionPoint], max_points: usize) -> Vec<DetectionPoint> {
    if max_points == 0 || points.len() <= max_points {
        return points.to_vec();
    }

    let min_ts = points.iter().map(|p| p.ts).min().unwrap_or(0);
    let max_ts = points.iter().map(|p| p.ts).max().unwrap_or(0);

    if min_ts == max_ts {
        return points.to_vec();
    }

    let span = max_ts - min_ts;
    let bucket_secs = ((span as f64 / max_points as f64).ceil() as u64).max(1);
    let n_buckets = ((span / bucket_secs) + 2) as usize;
    let mut buckets: Vec<Option<DetectionPoint>> = vec![None; n_buckets];

    for pt in points {
        let idx = (((pt.ts - min_ts) / bucket_secs) as usize).min(n_buckets - 1);
        match &buckets[idx] {
            None => buckets[idx] = Some(pt.clone()),
            Some(existing) if pt.score > existing.score => buckets[idx] = Some(pt.clone()),
            _ => {}
        }
    }

    buckets.into_iter().flatten().collect()
}

pub async fn load_app_config(pool: &SqlitePool) -> Result<AppConfig, sqlx::Error> {
    let mut config = AppConfig::default();

    if let Some(row) = sqlx::query(
        "SELECT ip, printer_id, pincode FROM printer_config WHERE id = 1",
    )
    .fetch_optional(pool)
    .await?
    {
        config.printer.ip = row.get("ip");
        config.printer.printer_id = row.get("printer_id");
        config.printer.pincode = row.get("pincode");
    }

    if let Some(row) = sqlx::query(
        "SELECT enabled, interval_secs, notify_threshold, pause_threshold,
                confirmation_frames, obico_url, exclude_zones_json
         FROM detection_config WHERE id = 1",
    )
    .fetch_optional(pool)
    .await?
    {
        config.detection.enabled = row.get::<i64, _>("enabled") != 0;
        config.detection.interval_secs = row.get::<i64, _>("interval_secs") as u32;
        config.detection.notify_threshold = row.get("notify_threshold");
        config.detection.pause_threshold = row.get("pause_threshold");
        config.detection.confirmation_frames = row.get::<i64, _>("confirmation_frames") as u32;
        config.detection.obico_url = row.get("obico_url");
        let zones_json: String = row.get("exclude_zones_json");
        config.detection.exclude_zones = serde_json::from_str(&zones_json).unwrap_or_default();
    }

    if let Some(row) = sqlx::query(
        "SELECT host, port, log_level, onboarding_complete FROM server_config WHERE id = 1",
    )
    .fetch_optional(pool)
    .await?
    {
        config.server.host = row.get("host");
        config.server.port = row.get::<i64, _>("port") as u16;
        config.logging.level = row.get("log_level");
        config.onboarding_complete = row.get::<i64, _>("onboarding_complete") != 0;
    }

    let rows = sqlx::query(
        "SELECT id, kind, enabled, label, ntfy_server, ntfy_topic, ntfy_tap_url,
                discord_webhook_url, webhook_url, toggles_json
         FROM notification_destinations",
    )
    .fetch_all(pool)
    .await?;

    config.notifications.destinations = rows
        .iter()
        .filter_map(|row| {
            let kind_str: String = row.get("kind");
            let kind = match kind_str.as_str() {
                "ntfy" => DestinationKind::Ntfy,
                "discord" => DestinationKind::Discord,
                "webhook" => DestinationKind::Webhook,
                _ => return None,
            };
            let toggles_json: String = row.get("toggles_json");
            let toggles: EventToggles = serde_json::from_str(&toggles_json).unwrap_or_default();
            Some(NotificationDestination {
                id: row.get("id"),
                kind,
                enabled: row.get::<i64, _>("enabled") != 0,
                label: row.get("label"),
                ntfy_server: row.get("ntfy_server"),
                ntfy_topic: row.get("ntfy_topic"),
                ntfy_tap_url: row.get("ntfy_tap_url"),
                discord_webhook_url: row.get("discord_webhook_url"),
                webhook_url: row.get("webhook_url"),
                toggles,
            })
        })
        .collect();

    Ok(config)
}

pub async fn save_printer_config(pool: &SqlitePool, cfg: &PrinterConfig) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT OR REPLACE INTO printer_config (id, ip, printer_id, pincode) VALUES (1, ?, ?, ?)",
    )
    .bind(&cfg.ip)
    .bind(&cfg.printer_id)
    .bind(&cfg.pincode)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn save_detection_config(pool: &SqlitePool, cfg: &DetectionConfig) -> Result<(), sqlx::Error> {
    let zones_json = serde_json::to_string(&cfg.exclude_zones).unwrap_or_else(|_| "[]".to_string());
    sqlx::query(
        "INSERT OR REPLACE INTO detection_config
         (id, enabled, interval_secs, notify_threshold, pause_threshold,
          confirmation_frames, obico_url, exclude_zones_json)
         VALUES (1, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(cfg.enabled as i64)
    .bind(cfg.interval_secs as i64)
    .bind(cfg.notify_threshold)
    .bind(cfg.pause_threshold)
    .bind(cfg.confirmation_frames as i64)
    .bind(&cfg.obico_url)
    .bind(&zones_json)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn save_server_config(
    pool: &SqlitePool,
    host: &str,
    port: u16,
    log_level: &str,
    onboarding_complete: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT OR REPLACE INTO server_config (id, host, port, log_level, onboarding_complete)
         VALUES (1, ?, ?, ?, ?)",
    )
    .bind(host)
    .bind(port as i64)
    .bind(log_level)
    .bind(onboarding_complete as i64)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn upsert_destination(pool: &SqlitePool, dest: &NotificationDestination) -> Result<(), sqlx::Error> {
    let kind_str = match dest.kind {
        DestinationKind::Ntfy => "ntfy",
        DestinationKind::Discord => "discord",
        DestinationKind::Webhook => "webhook",
    };
    let toggles_json = serde_json::to_string(&dest.toggles).unwrap_or_else(|_| "{}".to_string());
    sqlx::query(
        "INSERT OR REPLACE INTO notification_destinations
         (id, kind, enabled, label, ntfy_server, ntfy_topic, ntfy_tap_url,
          discord_webhook_url, webhook_url, toggles_json)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&dest.id)
    .bind(kind_str)
    .bind(dest.enabled as i64)
    .bind(&dest.label)
    .bind(&dest.ntfy_server)
    .bind(&dest.ntfy_topic)
    .bind(&dest.ntfy_tap_url)
    .bind(&dest.discord_webhook_url)
    .bind(&dest.webhook_url)
    .bind(&toggles_json)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_destination(pool: &SqlitePool, id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM notification_destinations WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn reset_config(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM printer_config").execute(pool).await?;
    sqlx::query("DELETE FROM detection_config").execute(pool).await?;
    sqlx::query("DELETE FROM server_config").execute(pool).await?;
    sqlx::query("DELETE FROM notification_destinations").execute(pool).await?;
    Ok(())
}

pub async fn migrate_jsonl(pool: &SqlitePool) {
    migrate_detection_log(pool, "data/detection.log").await;
    migrate_events_log(pool, "data/events.log").await;
}

async fn migrate_detection_log(pool: &SqlitePool, path: &str) {
    let Ok(data) = std::fs::read_to_string(path) else { return };
    if data.trim().is_empty() { return }

    let points: Vec<DetectionPoint> = data
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    if points.is_empty() { return }

    info!("[db] migrating {} detection points from {path}", points.len());
    for pt in &points {
        insert_detection_point(pool, pt).await;
    }
    let migrated = format!("{path}.migrated");
    match std::fs::rename(path, &migrated) {
        Ok(_) => info!("[db] {path} → {migrated}"),
        Err(e) => warn!("[db] rename {path} failed: {e}"),
    }
}

async fn migrate_events_log(pool: &SqlitePool, path: &str) {
    let Ok(data) = std::fs::read_to_string(path) else { return };
    if data.trim().is_empty() { return }

    let mut count = 0u32;
    for line in data.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let Some(ts) = v["ts"].as_u64() else { continue };
        let Some(kind) = v["kind"].as_str() else { continue };
        let Some(msg) = v["msg"].as_str() else { continue };
        insert_event(pool, ts, kind, msg, v["snap"].as_str()).await;
        count += 1;
    }
    if count > 0 {
        info!("[db] migrated {count} events from {path}");
        let migrated = format!("{path}.migrated");
        match std::fs::rename(path, &migrated) {
            Ok(_) => info!("[db] {path} → {migrated}"),
            Err(e) => warn!("[db] rename {path} failed: {e}"),
        }
    }
}
