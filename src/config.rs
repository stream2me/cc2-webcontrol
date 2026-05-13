use serde::{Deserialize, Serialize};

use crate::error::ConfigError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub printer: PrinterConfig,
    pub detection: DetectionConfig,
    pub notifications: NotificationsConfig,
    pub server: ServerConfig,
    pub logging: LoggingConfig,
    #[serde(default)]
    pub onboarding_complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrinterConfig {
    pub ip: String,
    pub printer_id: String,
    pub pincode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExcludeZone {
    /// norm 0..1
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

impl ExcludeZone {
    pub fn contains_center(&self, det_x1: f64, det_y1: f64, det_x2: f64, det_y2: f64) -> bool {
        let cx = (det_x1 + det_x2) / 2.0;
        let cy = (det_y1 + det_y2) / 2.0;
        cx >= self.x1 && cx <= self.x2 && cy >= self.y1 && cy <= self.y2
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionConfig {
    pub enabled: bool,
    pub interval_secs: u32,
    /// warn threshold
    #[serde(default = "default_notify_threshold")]
    pub notify_threshold: f64,
    /// pause threshold >= notify
    #[serde(alias = "threshold", default = "default_pause_threshold")]
    pub pause_threshold: f64,
    pub confirmation_frames: u32,
    pub obico_url: String,
    #[serde(default)]
    pub exclude_zones: Vec<ExcludeZone>,
}

fn default_notify_threshold() -> f64 { 0.5 }
fn default_pause_threshold() -> f64 { 0.7 }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DestinationKind {
    Ntfy,
    Discord,
    Webhook,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventToggles {
    #[serde(default = "default_true")]
    pub print_started: bool,
    #[serde(default = "default_true")]
    pub print_finished: bool,
    #[serde(default = "default_true")]
    pub print_paused: bool,
    #[serde(default = "default_true")]
    pub failure_notify: bool,
    #[serde(default = "default_true")]
    pub failure_pause: bool,
    #[serde(default = "default_true")]
    pub auto_paused: bool,
    #[serde(default = "default_true")]
    pub camera_lost: bool,
    #[serde(default = "default_true")]
    pub camera_restored: bool,
    #[serde(default = "default_true")]
    pub emergency_stop: bool,
    #[serde(default = "default_true")]
    pub machine_error: bool,
    #[serde(default = "default_true")]
    pub id_not_match: bool,
    #[serde(default = "default_true")]
    pub auth_error: bool,
    #[serde(default = "default_true")]
    pub print_resumed: bool,
    #[serde(default = "default_true")]
    pub print_stopped: bool,
    #[serde(default = "default_true")]
    pub print_finished_ok: bool,
    #[serde(default = "default_true")]
    pub connected: bool,
    #[serde(default = "default_true")]
    pub disconnected: bool,
    #[serde(default = "default_true")]
    pub detection_engine_error: bool,
}

fn default_true() -> bool { true }

impl Default for EventToggles {
    fn default() -> Self {
        Self {
            print_started: true,
            print_finished: true,
            print_paused: true,
            failure_notify: true,
            failure_pause: true,
            auto_paused: true,
            camera_lost: true,
            camera_restored: true,
            emergency_stop: true,
            machine_error: true,
            id_not_match: true,
            auth_error: true,
            print_resumed: true,
            print_stopped: true,
            print_finished_ok: true,
            connected: true,
            disconnected: true,
            detection_engine_error: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationDestination {
    pub id: String,
    pub kind: DestinationKind,
    pub enabled: bool,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ntfy_server: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ntfy_topic: Option<String>,
    /// URL opened when user taps the notification
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ntfy_tap_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discord_webhook_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_url: Option<String>,
    #[serde(default)]
    pub toggles: EventToggles,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotificationsConfig {
    #[serde(default)]
    pub destinations: Vec<NotificationDestination>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
}

impl AppConfig {
    pub fn printer_password(&self) -> &str {
        if !self.printer.pincode.is_empty() {
            &self.printer.pincode
        } else {
            "123456"
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            printer: PrinterConfig {
                ip: String::new(),
                printer_id: String::new(),
                pincode: String::new(),
            },
            detection: DetectionConfig {
                enabled: true,
                interval_secs: 15,
                notify_threshold: 0.5,
                pause_threshold: 0.7,
                confirmation_frames: 2,
                obico_url: "http://localhost:3333".to_string(),
                exclude_zones: Vec::new(),
            },
            notifications: NotificationsConfig {
                destinations: Vec::new(),
            },
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 8484,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
            },
            onboarding_complete: false,
        }
    }
}

/// pincode: 6 ascii alnum
pub fn validate_pincode(p: &str) -> Result<(), ConfigError> {
    if p.len() != 6 || !p.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(ConfigError::InvalidPincode);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_config() -> AppConfig {
        AppConfig::default()
    }

    #[test]
    fn validate_pincode_accepts_valid() {
        assert!(validate_pincode("ABC123").is_ok());
        assert!(validate_pincode("000000").is_ok());
        assert!(validate_pincode("ZZZZZZ").is_ok());
    }

    #[test]
    fn validate_pincode_rejects_short_or_invalid() {
        assert!(validate_pincode("ABC12").is_err());
        assert!(validate_pincode("ABC1234").is_err());
        assert!(validate_pincode("AB-C12").is_err());
        assert!(validate_pincode("AB C12").is_err());
        assert!(validate_pincode("ABC12!").is_err());
    }

    #[test]
    fn printer_password_uses_pincode_when_set() {
        let mut cfg = base_config();
        cfg.printer.pincode = "ABC123".to_string();
        assert_eq!(cfg.printer_password(), "ABC123");
    }

    #[test]
    fn printer_password_defaults_to_123456() {
        let cfg = base_config();
        assert_eq!(cfg.printer_password(), "123456");
    }

    #[test]
    fn exclude_zone_contains_center() {
        let zone = ExcludeZone { x1: 0.2, y1: 0.2, x2: 0.8, y2: 0.8 };
        assert!(zone.contains_center(0.3, 0.3, 0.7, 0.7));
        assert!(!zone.contains_center(0.0, 0.0, 0.1, 0.1));
        assert!(!zone.contains_center(0.85, 0.85, 0.95, 0.95));
    }
}
