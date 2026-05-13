use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("config error: {0}")]
    Config(#[from] ConfigError),

    #[error("printer error: {0}")]
    Printer(#[from] PrinterError),

    #[error("detection error: {0}")]
    Detection(#[from] DetectionError),

    #[error("notification error: {0}")]
    Notification(#[from] NotificationError),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("setup error: {0}")]
    Setup(#[from] SetupError),

    #[error("validation error: {0}")]
    Validation(String),
}

#[derive(Error, Debug)]
pub enum SetupError {
    #[error("connection verification failed: {0}")]
    VerificationFailed(String),

    #[error("invalid pincode: must be 6 uppercase alphanumeric characters")]
    InvalidPincode,
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        if let AppError::Printer(PrinterError::CommandFailed { error_code, .. }) = &self {
            let (status, msg) = match *error_code {
                1009 => (StatusCode::CONFLICT, "Printer Busy"),
                1010 => (StatusCode::CONFLICT, "Cannot perform this action while the printer is busy"),
                _ => (StatusCode::BAD_REQUEST, "Command was rejected by the printer"),
            };
            return (status, Json(serde_json::json!({ "error": msg }))).into_response();
        }

        let status = match &self {
            AppError::Printer(PrinterError::NotConnected) => StatusCode::SERVICE_UNAVAILABLE,
            AppError::Printer(PrinterError::RpcTimeout) => StatusCode::GATEWAY_TIMEOUT,
            AppError::Config(_) => StatusCode::BAD_REQUEST,
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            AppError::Setup(SetupError::InvalidPincode) => StatusCode::BAD_REQUEST,
            AppError::Setup(SetupError::VerificationFailed(_)) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        (status, Json(serde_json::json!({ "error": self.to_string() }))).into_response()
    }
}

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),

    #[error("invalid pincode: must be 6 uppercase characters")]
    InvalidPincode,
}

#[derive(Error, Debug)]
pub enum PrinterError {
    #[error("MQTT error: {0}")]
    Mqtt(#[from] rumqttc::ConnectionError),

    #[error("WebSocket error: {0}")]
    WebSocket(String),

    #[error("registration failed: {0}")]
    Registration(String),

    #[error("printer not connected")]
    NotConnected,

    // cmd non-zero code
    #[error("command failed: method {method}, error_code {error_code}")]
    CommandFailed { method: u16, error_code: u16 },

    #[error("discovery timed out after {0}s")]
    DiscoveryTimeout(u64),

    #[error("printer RPC timed out")]
    RpcTimeout,

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

#[derive(Error, Debug)]
pub enum DetectionError {
    #[error("Obico ML request failed: {0}")]
    ObicoFailed(String),

    #[error("detection engine not running")]
    NotRunning,
}

#[derive(Error, Debug)]
pub enum NotificationError {
    #[error("ntfy notification failed: {0}")]
    NtfyFailed(String),
    #[error("discord notification failed: {0}")]
    DiscordFailed(String),
    #[error("webhook notification failed: {0}")]
    WebhookFailed(String),
}
