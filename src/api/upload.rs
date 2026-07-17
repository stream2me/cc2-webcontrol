use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use tracing::{info, warn};

use super::router::AppState;
use crate::error::AppError;

const CHUNK_SIZE: usize = 1024 * 1024; // match slicer chunk size to avoid ack drift

pub async fn upload_file(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>, AppError> {
    let (printer_ip, token) = {
        let cfg = state.config.read().await;
        (cfg.printer.ip.clone(), cfg.printer_password().to_string())
    };

    if printer_ip.is_empty() {
        return Err(AppError::Validation("printer IP not configured".to_string()));
    }

    let total = body.len();
    if total == 0 {
        return Err(AppError::Validation("empty file".to_string()));
    }

    let filename = extract_filename(&headers, &body);
    let md5_hex = format!("{:x}", md5::compute(&body));

    let url = format!("http://{}/upload", printer_ip);
    let client = reqwest::Client::new();
    let mut start = 0usize;

    info!("uploading '{}' ({} bytes) md5={} to {}", filename, total, &md5_hex[..8], printer_ip);

    while start < total {
        let end = (start + CHUNK_SIZE - 1).min(total - 1);
        let chunk = body.slice(start..=end);
        let chunk_len = chunk.len();

        let resp = client
            .put(&url)
            .header("Content-Type", "application/octet-stream")
            .header("Content-Length", chunk_len.to_string())
            .header("Content-Range", format!("bytes {}-{}/{}", start, end, total))
            .header("X-File-Name", &filename)
            .header("X-File-MD5", &md5_hex)
            .header("X-Token", &token)
            .header("Accept", "application/json")
            .header("User-Agent", "ElegooLink/1.0.1")
            .body(chunk)
            .send()
            .await
            .map_err(|e| AppError::Validation(format!("chunk {start}-{end} send failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let snippet = resp.text().await.unwrap_or_default();
            let detail = if snippet.is_empty() {
                String::new()
            } else {
                format!("  -  {}", snippet.chars().take(200).collect::<String>())
            };
            return Err(AppError::Validation(format!(
                "printer rejected chunk {start}-{end}: HTTP {status}{detail}"
            )));
        }

        // printer may return plain 200; json parse is optional
        let resp_bytes = resp.bytes().await.unwrap_or_default();
        if !resp_bytes.is_empty() {
            if let Ok(ack) = serde_json::from_slice::<serde_json::Value>(&resp_bytes) {
                let code = ack.get("error_code").and_then(|v| v.as_i64()).unwrap_or(0);
                if code != 0 {
                    warn!("printer upload error_code={} on chunk {}-{}: {:?}", code, start, end, ack);
                    return Err(AppError::Validation(format!(
                        "printer upload error on chunk {start}-{end}: error_code={code}"
                    )));
                }
            }
        }

        start = end + 1;
    }

    info!("upload complete: '{}' ({} bytes)", filename, total);
    Ok(Json(serde_json::json!({ "ok": true, "bytes": total })))
}

/// printer rejects upload names without .gcode suffix
fn extract_filename(headers: &HeaderMap, body: &Bytes) -> String {
    if let Some(name) = headers
        .get("x-file-name")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
    {
        return name.to_string();
    }
    // stable fallback avoids duplicate-temp naming churn
    let digest = md5::compute(&body[..body.len().min(4096)]);
    format!("{:.8x}.gcode", digest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    fn make_headers(name: Option<&str>) -> HeaderMap {
        let mut h = HeaderMap::new();
        if let Some(n) = name {
            h.insert("x-file-name", n.parse().unwrap());
        }
        h
    }

    #[test]
    fn extract_filename_from_header() {
        let body = Bytes::from_static(b"GCODE DATA");
        let h = make_headers(Some("my_model.gcode"));
        assert_eq!(extract_filename(&h, &body), "my_model.gcode");
    }

    #[test]
    fn extract_filename_fallback_has_gcode_ext() {
        let body = Bytes::from_static(b"GCODE DATA");
        let h = make_headers(None);
        let name = extract_filename(&h, &body);
        assert!(name.ends_with(".gcode"), "expected .gcode suffix, got: {name}");
    }

    #[test]
    fn extract_filename_empty_header_falls_back() {
        let body = Bytes::from_static(b"GCODE DATA");
        let h = make_headers(Some(""));
        let name = extract_filename(&h, &body);
        assert!(name.ends_with(".gcode"), "expected .gcode fallback, got: {name}");
    }

    #[test]
    fn md5_hex_is_lowercase_32_chars() {
        let data = b"hello world";
        let hex = format!("{:x}", md5::compute(data));
        assert_eq!(hex.len(), 32);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn md5_known_value() {
        // "hello world" MD5 = 5eb63bbbe01eeed093cb22bb8f5acdc3
        let hex = format!("{:x}", md5::compute(b"hello world"));
        assert_eq!(hex, "5eb63bbbe01eeed093cb22bb8f5acdc3");
    }

    #[test]
    fn chunk_range_last_chunk() {
        // 2_500_000 bytes -> 3 chunks; last starts at 2*CHUNK_SIZE

        let total = 2_500_000usize;
        let start = 2 * CHUNK_SIZE;
        let end = (start + CHUNK_SIZE - 1).min(total - 1);
        assert_eq!(end, total - 1, "last chunk must end at total-1");
        assert_eq!(
            format!("bytes {}-{}/{}", start, end, total),
            "bytes 2097152-2499999/2500000"
        );
    }

    #[test]
    fn single_chunk_range() {
        let total = 512usize;
        let start = 0;
        let end = (start + CHUNK_SIZE - 1).min(total - 1);
        assert_eq!(end, 511);
        assert_eq!(format!("bytes {}-{}/{}", start, end, total), "bytes 0-511/512");
    }
}
