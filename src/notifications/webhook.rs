use std::time::Duration;

use reqwest::Client;
use serde_json::json;
use tracing::warn;

use crate::config::NotificationDestination;
use crate::error::NotificationError;

pub async fn send(dest: &NotificationDestination, title: &str, body: &str) -> Result<(), NotificationError> {
    let url = match dest.webhook_url.as_deref() {
        Some(u) if !u.is_empty() => u.to_string(),
        _ => return Err(NotificationError::WebhookFailed("webhook URL is not configured".to_string())),
    };

    let payload = json!({ "title": title, "body": body, "source": "cc2-monitor" });

    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    let res = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| NotificationError::WebhookFailed(e.to_string()))?;

    if !res.status().is_success() {
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        warn!("webhook returned {status}: {text}");
        return Err(NotificationError::WebhookFailed(format!("server returned {status}")));
    }

    Ok(())
}
