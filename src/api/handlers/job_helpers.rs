//! Helper functions for job management in API handlers
//!
//! Provides reusable functions for creating and managing background jobs
//! for social, MBTI, and annual report analysis endpoints.

use axum::Json;
use serde_json::Value;
use tracing::error;
use tracing::info;

use crate::api::handlers::AppState;
use crate::api::redis_client::RedisClient;
use crate::api::types::ApiResponse;

/// Job configuration for creating background jobs
pub struct JobConfig {
    pub job_type: &'static str, // "social", "mbti", or "annual_report"
    pub job_key: String,
    pub fid: i64,
    pub year: Option<u32>, // Required for "annual_report" jobs
}

/// Result of attempting to create or check a job
pub enum JobResult {
    /// Job already exists with a specific status (pending, processing, etc.)
    AlreadyExists(String), // status string
    /// New job was created successfully
    Created,
    /// Failed to create job (Redis unavailable or error)
    Failed,
}

/// Check if a job is already processing, or create a new one
///
/// Returns:
/// - `JobResult::AlreadyExists(status)` if job already exists with the given status
/// - `JobResult::Created` if new job was created
/// - `JobResult::Failed` if Redis is unavailable or job creation failed
pub async fn check_or_create_job(state: &AppState, config: &JobConfig) -> JobResult {
    if let Some(redis_cfg) = &state.config.redis {
        if let Ok(redis_client) = RedisClient::connect(redis_cfg) {
            // First check if job is already active
            if redis_client
                .is_job_active(&config.job_key)
                .await
                .unwrap_or(false)
            {
                // Get the actual job status from Redis
                if let Ok(Some((status, _))) = redis_client.get_job_status(&config.job_key).await {
                    info!(
                        "⏳ Job already exists for {} FID {} with status: {}",
                        config.job_type, config.fid, status
                    );
                    return JobResult::AlreadyExists(status);
                }
                // Job is active but no status found - assume pending
                info!(
                    "⏳ Job already exists for {} FID {} (no status found, assuming pending)",
                    config.job_type, config.fid
                );
                return JobResult::AlreadyExists("pending".to_string());
            }

            // Create new job
            let job_data = if config.job_type == "annual_report" {
                // Annual report jobs require year field - it must be provided
                match config.year {
                    Some(year) => {
                        serde_json::json!({"fid": config.fid, "year": year, "type": config.job_type}).to_string()
                    }
                    None => {
                        error!("Missing year for annual_report job - year is required");
                        return JobResult::Failed;
                    }
                }
            } else {
                serde_json::json!({"fid": config.fid, "type": config.job_type}).to_string()
            };
            if let Ok(Some(_)) = redis_client
                .push_job(config.job_type, &config.job_key, &job_data)
                .await
            {
                info!(
                    "📤 Created background job for {} FID {}",
                    config.job_type, config.fid
                );
                return JobResult::Created;
            }
        }
    }

    JobResult::Failed
}

/// Trigger a background update for a job (used when cache is stale or updating)
pub async fn trigger_background_update(state: &AppState, config: &JobConfig) {
    if let Some(redis_cfg) = &state.config.redis {
        if let Ok(redis_client) = RedisClient::connect(redis_cfg) {
            let job_data = if config.job_type == "annual_report" {
                // Annual report jobs require year field - it must be provided
                match config.year {
                    Some(year) => {
                        serde_json::json!({"fid": config.fid, "year": year, "type": config.job_type}).to_string()
                    }
                    None => {
                        error!("Missing year for annual_report job - year is required");
                        return;
                    }
                }
            } else {
                serde_json::json!({"fid": config.fid, "type": config.job_type}).to_string()
            };
            if let Ok(Some(_)) = redis_client
                .push_job(config.job_type, &config.job_key, &job_data)
                .await
            {
                info!(
                    "🔄 Triggered background update for {} FID {}",
                    config.job_type, config.fid
                );
            }
        }
    }
}

/// Create a job status response with detailed status information
pub fn create_job_status_response(
    job_key: &str,
    status: &str,
    message: &str,
) -> Json<ApiResponse<Value>> {
    Json(ApiResponse::success(serde_json::json!({
        "status": status,
        "job_key": job_key,
        "message": message
    })))
}

/// Create a pending response for a job (backward compatibility)
pub fn create_pending_response(job_key: &str, message: &str) -> Json<ApiResponse<Value>> {
    create_job_status_response(job_key, "pending", message)
}

/// Create an updating response with cached data
pub fn create_updating_response<T: serde::Serialize>(
    cached_data: &T,
    message: &str,
) -> Json<ApiResponse<Value>> {
    let data = serde_json::to_value(cached_data).unwrap_or_else(|_| {
        serde_json::json!({
            "error": "Failed to serialize cached data"
        })
    });
    Json(ApiResponse::success(serde_json::json!({
        "status": "updating",
        "data": data,
        "message": message
    })))
}
