/// Social graph analysis handler with Redis cache and worker pattern
use axum::extract::Path;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use tracing::error;
use tracing::info;

use super::AppState;
use crate::api::cache::CacheResult;
use crate::api::types::ApiResponse;
use crate::social_graph::SocialGraphAnalyzer;

/// Social graph analysis handler by username (GET /api/social/username/:username)
/// Supports stale-while-revalidate and background job processing
pub async fn get_social_analysis_by_username(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    info!("GET /api/social/username/{}", username);

    // Get user profile by username first
    let profile = match state.database.get_user_profile_by_username(&username).await {
        Ok(Some(profile)) => profile,
        Ok(None) => {
            return Ok(Json(ApiResponse::error(format!(
                "User with username {username} not found"
            ))));
        }
        Err(e) => {
            error!(
                "Failed to get user profile for username {}: {}",
                username, e
            );
            return Ok(Json(ApiResponse::error(format!(
                "Failed to get user profile: {e}"
            ))));
        }
    };

    let fid = profile.fid;
    let job_key = format!("social:{}", fid);

    // Check cache
    match state.cache_service.get_social(fid).await {
        Ok(CacheResult::Fresh(social)) => {
            // Fresh cache hit - return immediately
            info!("📦 Social cache hit (fresh) for username {} (FID {})", username, fid);
            let social_data = serde_json::to_value(&social).unwrap_or_else(|_| {
                serde_json::json!({
                    "error": "Failed to serialize cached social profile"
                })
            });
            return Ok(Json(ApiResponse::success(social_data)));
        }
        Ok(CacheResult::Stale(social)) => {
            // Stale cache hit - return stale data and trigger background update
            info!("📦 Social cache hit (stale) for username {} (FID {})", username, fid);
            
            // Trigger background update if not already in queue
            if let Ok(redis) = state.config.redis.as_ref() {
                let redis_client = crate::api::redis_client::RedisClient::connect(redis).ok();
                if let Some(redis) = redis_client {
                    let job_data = serde_json::json!({"fid": fid, "type": "social"}).to_string();
                    if let Ok(Some(_)) = redis.push_job("social", &job_key, &job_data).await {
                        info!("🔄 Triggered background update for FID {}", fid);
                    }
                }
            }
            
            let social_data = serde_json::to_value(&social).unwrap_or_else(|_| {
                serde_json::json!({
                    "error": "Failed to serialize cached social profile"
                })
            });
            return Ok(Json(ApiResponse::success(social_data)));
        }
        Ok(CacheResult::Miss) => {
            // Cache miss - check if job is already processing
            if let Ok(redis) = state.config.redis.as_ref() {
                let redis_client = crate::api::redis_client::RedisClient::connect(redis).ok();
                if let Some(redis) = redis_client {
                    // Check job status
                    if let Ok(Some((status, _))) = redis.get_job_status(&job_key).await {
                        if status == "pending" || status == "processing" {
                            // Job already in queue or processing
                            info!("⏳ Job already processing for FID {}", fid);
                            return Ok(Json(ApiResponse::success(serde_json::json!({
                                "status": "pending",
                                "job_key": job_key,
                                "message": "Analysis in progress, please check back later"
                            }))));
                        }
                    }
                    
                    // Create new job
                    let job_data = serde_json::json!({"fid": fid, "type": "social"}).to_string();
                    if let Ok(Some(_)) = redis.push_job("social", &job_key, &job_data).await {
                        info!("📤 Created background job for FID {}", fid);
                        return Ok(Json(ApiResponse::success(serde_json::json!({
                            "status": "pending",
                            "job_key": job_key,
                            "message": "Analysis started, please check back later"
                        }))));
                    }
                }
            }
            
            // Fallback: process synchronously if Redis is not available
            error!("Redis not available, falling back to synchronous processing");
            let analyzer = SocialGraphAnalyzer::new(state.database.clone());
            match analyzer.analyze_user(fid).await {
                Ok(social_profile) => {
                    if let Err(e) = state.cache_service.set_social(fid, &social_profile).await {
                        error!("Failed to cache social profile: {}", e);
                    }
                    let social_data = serde_json::to_value(&social_profile).unwrap_or_else(|_| {
                        serde_json::json!({
                            "error": "Failed to serialize social profile"
                        })
                    });
                    Ok(Json(ApiResponse::success(social_data)))
                }
                Err(e) => {
                    error!("Failed to analyze social graph: {}", e);
                    Ok(Json(ApiResponse::error(format!(
                        "Failed to analyze social graph: {e}"
                    ))))
                }
            }
        }
        Err(e) => {
            error!("Cache error: {}", e);
            // Fallback to synchronous processing
            let analyzer = SocialGraphAnalyzer::new(state.database.clone());
            match analyzer.analyze_user(fid).await {
                Ok(social_profile) => {
                    if let Err(e) = state.cache_service.set_social(fid, &social_profile).await {
                        error!("Failed to cache social profile: {}", e);
                    }
                    let social_data = serde_json::to_value(&social_profile).unwrap_or_else(|_| {
                        serde_json::json!({
                            "error": "Failed to serialize social profile"
                        })
                    });
                    Ok(Json(ApiResponse::success(social_data)))
                }
                Err(e) => {
                    Ok(Json(ApiResponse::error(format!(
                        "Failed to analyze social graph: {e}"
                    ))))
                }
            }
        }
    }
}

