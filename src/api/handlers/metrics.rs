//! Prometheus metrics endpoint handler

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use tracing::error;

use crate::api::metrics;

/// Prometheus metrics endpoint handler
pub async fn get_metrics() -> Result<impl IntoResponse, StatusCode> {
    if let Some(m) = metrics::get_metrics() {
        match m.export() {
            Ok(metrics_text) => Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/plain; version=0.0.4")
                .body(metrics_text)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?),
            Err(e) => {
                error!("Failed to export metrics: {}", e);
                Err(StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    } else {
        error!("Metrics not initialized");
        Err(StatusCode::SERVICE_UNAVAILABLE)
    }
}
