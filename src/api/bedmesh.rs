use axum::extract::State;

use super::router::AppState;
use crate::error::AppError;

pub async fn get_autosave_cfg(
    State(_state): State<AppState>,
) -> Result<String, AppError> {
    let path = "/opt/usr/cfg/autosave.cfg";

    match std::fs::read_to_string(path) {
        Ok(content) => {
            Ok(content)
        }
        Err(e) => {
            tracing::error!("failed to read {}: {}", path, e);
            Err(AppError::Validation(format!("Failed to read {}: {}", path, e)))
        }
    }
}
