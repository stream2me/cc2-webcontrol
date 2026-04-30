use axum::body::Body;
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};

use super::router::AppState;

/// latest jpeg frame
pub async fn snapshot(State(state): State<AppState>) -> Response<Body> {
    let frame = state.frame_buffer.read().await.clone();

    match frame {
        Some(jpeg) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            jpeg,
        )
            .into_response(),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            "frame grabber not ready yet",
        )
            .into_response(),
    }
}
