use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tracing::{info, warn};

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const REPO: &str = "DimeusDev/cc2-openwebui";
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone)]
pub struct UpdateStatus {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub up_to_date: bool,
}

pub struct UpdateChecker {
    pub status: Arc<RwLock<UpdateStatus>>,
    client: reqwest::Client,
}

impl UpdateChecker {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            status: Arc::new(RwLock::new(UpdateStatus {
                current_version: CURRENT_VERSION.to_string(),
                latest_version: None,
                up_to_date: true,
            })),
            client: reqwest::Client::new(),
        })
    }

    pub async fn check(&self) {
        let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
        let result = self
            .client
            .get(&url)
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "cc2-openwebui-update-check")
            .timeout(Duration::from_secs(15))
            .send()
            .await;

        match result {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    if let Some(tag) = body["tag_name"].as_str() {
                        let latest = tag.trim_start_matches('v').to_string();
                        let up_to_date = latest == CURRENT_VERSION;
                        let mut s = self.status.write().await;
                        s.latest_version = Some(latest.clone());
                        s.up_to_date = up_to_date;
                        info!(current = CURRENT_VERSION, latest = %latest, up_to_date, "update check complete");
                    }
                }
            }
            Ok(resp) => warn!("update check: GitHub returned {}", resp.status()),
            Err(e) => warn!("update check: {e}"),
        }
    }

    pub fn start(self: Arc<Self>) {
        tokio::spawn(async move {
            loop {
                self.check().await;
                tokio::time::sleep(CHECK_INTERVAL).await;
            }
        });
    }
}
