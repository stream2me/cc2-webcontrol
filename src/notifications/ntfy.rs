use std::time::Duration;

use reqwest::Client;
use tracing::warn;

use crate::config::NotificationDestination;
use crate::error::NotificationError;

pub async fn send_test(dest: &NotificationDestination) -> Result<(), NotificationError> {
    send(dest, "CC2 Monitor", "Test notification - ntfy is working").await
}

pub async fn send(dest: &NotificationDestination, title: &str, body: &str) -> Result<(), NotificationError> {
    let server = dest.ntfy_server.as_deref().unwrap_or("https://ntfy.sh");
    let topic = dest.ntfy_topic.as_deref().unwrap_or("");

    if topic.is_empty() {
        return Err(NotificationError::NtfyFailed("topic is not configured".to_string()));
    }

    let url = format!("{}/{}", server.trim_end_matches('/'), topic);

    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    let mut req = client
        .post(&url)
        .header("Title", title);

    if let Some(tap_url) = dest.ntfy_tap_url.as_deref().filter(|u| !u.is_empty()) {
        req = req.header("Click", tap_url);
    }

    let res = req
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| NotificationError::NtfyFailed(e.to_string()))?;

    if !res.status().is_success() {
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        warn!("ntfy returned {status}: {text}");
        return Err(NotificationError::NtfyFailed(format!("server returned {status}")));
    }

    Ok(())
}
