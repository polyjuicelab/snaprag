/// API request handlers
use std::sync::Arc;

use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use tracing::error;
use tracing::info;

use crate::api::cache::CacheService;
use crate::api::handlers::job_helpers::*;
use crate::api::types::ApiResponse;
use crate::api::types::FetchResponse;
use crate::api::types::FetchUsersBatchRequest;
use crate::api::types::HealthResponse;
use crate::api::types::ProfileResponse;
use crate::config::AppConfig;
use crate::database::Database;
use crate::embeddings::EmbeddingService;
use crate::llm::LlmService;
use crate::models::UserProfileQuery;
use crate::rag::CastRetriever;
use crate::rag::RagQuery;
use crate::rag::RagService;
use crate::rag::RetrievalMethod;
use crate::rag::Retriever;
use crate::social_graph::SocialGraphAnalyzer;

// Re-export sub-modules
pub mod annual_report;
pub mod cast_stats;
pub mod chat;
pub mod content_style;
pub mod domains;
pub mod engagement;
pub mod follower_growth;
pub mod job_helpers;
pub mod mbti;
pub mod metrics;
pub mod network_stats;
pub mod profile;
pub mod rag;
pub mod search;
pub mod stats;
pub mod temporal_activity;

// Re-export handlers
pub use annual_report::*;
pub use cast_stats::*;
pub use chat::*;
pub use content_style::*;
pub use domains::*;
pub use engagement::*;
pub use follower_growth::*;
pub use job_helpers::*;
pub use mbti::*;
pub use metrics::*;
pub use network_stats::*;
pub use profile::*;
pub use rag::*;
pub use search::*;
pub use stats::*;
pub use temporal_activity::*;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<crate::AppConfig>,
    pub database: Arc<Database>,
    pub embedding_service: Arc<EmbeddingService>,
    pub llm_service: Option<Arc<LlmService>>,
    pub lazy_loader: Option<Arc<crate::sync::LazyLoader>>,
    pub session_manager: Arc<crate::api::session::SessionManager>,
    pub cache_service: Arc<CacheService>,
}

/// Health check handler
pub async fn health() -> Json<ApiResponse<HealthResponse>> {
    Json(ApiResponse::success(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }))
}

/// Fetch user on-demand (POST /api/fetch/user/:fid)
pub async fn fetch_user(
    State(state): State<AppState>,
    Path(fid): Path<i64>,
) -> Result<Json<ApiResponse<FetchResponse>>, StatusCode> {
    info!("POST /api/fetch/user/{}", fid);

    // Check if lazy loader is available
    let loader = match &state.lazy_loader {
        Some(l) => l,
        None => {
            return Ok(Json(ApiResponse::error(
                "Lazy loading is not enabled".to_string(),
            )));
        }
    };

    // Check if user already exists
    let existing_profile = state.database.get_user_profile(fid).await.ok().flatten();
    if let Some(profile) = existing_profile {
        let casts_count = state.database.count_casts_by_fid(fid).await.unwrap_or(0) as usize;
        return Ok(Json(ApiResponse::success(FetchResponse {
            profile: ProfileResponse {
                fid: profile.fid,
                username: profile.username,
                display_name: profile.display_name,
                bio: profile.bio,
                pfp_url: profile.pfp_url,
                location: profile.location,
                twitter_username: profile.twitter_username,
                github_username: profile.github_username,
                registered_at: None,
                total_casts: Some(casts_count as i64),
                total_reactions: None,
                total_links: None,
            },
            casts_count,
            embeddings_generated: None,
            source: "database".to_string(),
        })));
    }

    // Fetch user profile
    match loader.fetch_user_profile(fid as u64).await {
        Ok(profile) => {
            info!("✅ Successfully fetched user profile for FID {}", fid);
            let casts_count = state.database.count_casts_by_fid(fid).await.unwrap_or(0) as usize;
            Ok(Json(ApiResponse::success(FetchResponse {
                profile: ProfileResponse {
                    fid: profile.fid,
                    username: profile.username,
                    display_name: profile.display_name,
                    bio: profile.bio,
                    pfp_url: profile.pfp_url,
                    location: profile.location,
                    twitter_username: profile.twitter_username,
                    github_username: profile.github_username,
                    registered_at: None,
                    total_casts: Some(casts_count as i64),
                    total_reactions: None,
                    total_links: None,
                },
                casts_count,
                embeddings_generated: None,
                source: "snapchain".to_string(),
            })))
        }
        Err(e) => {
            error!("Failed to fetch user profile for FID {}: {}", fid, e);
            Ok(Json(ApiResponse::error(format!(
                "Failed to fetch user profile: {e}"
            ))))
        }
    }
}

/// Fetch multiple users on-demand (POST /api/fetch/users)
pub async fn fetch_users_batch(
    State(state): State<AppState>,
    Json(req): Json<FetchUsersBatchRequest>,
) -> Result<Json<ApiResponse<Vec<FetchResponse>>>, StatusCode> {
    info!("POST /api/fetch/users - {} FIDs", req.fids.len());

    // Check if lazy loader is available
    let loader = match &state.lazy_loader {
        Some(l) => l,
        None => {
            return Ok(Json(ApiResponse::error(
                "Lazy loading is not enabled".to_string(),
            )));
        }
    };

    let mut responses = Vec::new();

    for fid in req.fids {
        // Check if user already exists
        #[allow(clippy::cast_possible_wrap)] // FID is guaranteed to be positive and fit in i64
        let existing_profile = state
            .database
            .get_user_profile(fid as i64)
            .await
            .ok()
            .flatten();
        if let Some(profile) = existing_profile {
            #[allow(clippy::cast_possible_wrap)] // FID is guaranteed to be positive and fit in i64
            let casts_count = state
                .database
                .count_casts_by_fid(fid as i64)
                .await
                .unwrap_or(0) as usize;
            responses.push(FetchResponse {
                profile: ProfileResponse {
                    fid: profile.fid,
                    username: profile.username,
                    display_name: profile.display_name,
                    bio: profile.bio,
                    pfp_url: profile.pfp_url,
                    location: profile.location,
                    twitter_username: profile.twitter_username,
                    github_username: profile.github_username,
                    registered_at: None,
                    total_casts: Some(casts_count as i64),
                    total_reactions: None,
                    total_links: None,
                },
                casts_count,
                embeddings_generated: None,
                source: "database".to_string(),
            });
            continue;
        }

        // Fetch user profile
        match loader.fetch_user_profile(fid).await {
            Ok(profile) => {
                info!("✅ Successfully fetched user profile for FID {}", fid);
                #[allow(clippy::cast_possible_wrap)]
                // FID is guaranteed to be positive and fit in i64
                let casts_count = state
                    .database
                    .count_casts_by_fid(fid as i64)
                    .await
                    .unwrap_or(0) as usize;
                responses.push(FetchResponse {
                    profile: ProfileResponse {
                        fid: profile.fid,
                        username: profile.username,
                        display_name: profile.display_name,
                        bio: profile.bio,
                        pfp_url: profile.pfp_url,
                        location: profile.location,
                        twitter_username: profile.twitter_username,
                        github_username: profile.github_username,
                        registered_at: None,
                        total_casts: Some(casts_count as i64),
                        total_reactions: None,
                        total_links: None,
                    },
                    casts_count,
                    embeddings_generated: None,
                    source: "snapchain".to_string(),
                });
            }
            Err(e) => {
                error!("Failed to fetch user profile for FID {}: {}", fid, e);
                // Skip failed users for now
            }
        }
    }

    Ok(Json(ApiResponse::success(responses)))
}

/// Social graph analysis handler (GET /api/social/:fid) with caching
pub async fn get_social_analysis(
    State(state): State<AppState>,
    Path(fid): Path<i64>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    let start_time = std::time::Instant::now();
    info!("GET /api/social/{}", fid);

    let job_key = format!("social:{fid}");

    // Check cache first
    tracing::debug!("Checking cache for social analysis FID {}", fid);
    match state.cache_service.get_social(fid).await {
        Ok(crate::api::cache::CacheResult::Fresh(cached_social)) => {
            let duration = start_time.elapsed();
            info!(
                "📦 Social cache hit (fresh) for FID {} - {}ms",
                fid,
                duration.as_millis()
            );
            let social_data = serde_json::to_value(&cached_social).unwrap_or_else(|_| {
                serde_json::json!({
                    "error": "Failed to serialize cached social profile"
                })
            });
            return Ok(Json(ApiResponse::success(social_data)));
        }
        Ok(crate::api::cache::CacheResult::Stale(cached_social)) => {
            // Stale cache - return stale data and trigger background update
            info!(
                "📦 Social cache hit (stale) for FID {}, triggering background update",
                fid
            );

            let job_config = JobConfig {
                job_type: "social",
                job_key: job_key.clone(),
                fid,
            };
            trigger_background_update(&state, &job_config).await;

            let duration = start_time.elapsed();
            let social_data = serde_json::to_value(&cached_social).unwrap_or_else(|_| {
                serde_json::json!({
                    "error": "Failed to serialize cached social profile"
                })
            });
            return Ok(Json(ApiResponse::success(social_data)));
        }
        Ok(crate::api::cache::CacheResult::Updating(cached_social)) => {
            // Cache expired - return updating status with old data
            info!(
                "🔄 Social cache expired (updating) for FID {}, returning old data with updating status",
                fid
            );

            let job_config = JobConfig {
                job_type: "social",
                job_key: job_key.clone(),
                fid,
            };
            trigger_background_update(&state, &job_config).await;

            return Ok(create_updating_response(
                &cached_social,
                "Data is being updated in the background. Please refresh to get the latest data.",
            ));
        }
        Ok(crate::api::cache::CacheResult::Miss) => {
            tracing::debug!(
                "No cache hit for social analysis FID {}, checking job status",
                fid
            );
        }
        Err(e) => {
            error!("Cache error for FID {}: {}", fid, e);
            // Continue to process synchronously
        }
    }

    // Cache miss - check if job is already processing
    let job_config = JobConfig {
        job_type: "social",
        job_key: job_key.clone(),
        fid,
    };
    match check_or_create_job(&state, &job_config).await {
        JobResult::AlreadyExists(status) => {
            let message = match status.as_str() {
                "pending" => "Analysis is queued, please check back later",
                "processing" => "Analysis in progress, please check back later",
                "completed" => {
                    // Job completed but cache not updated yet - try to get result from status
                    if let Some(redis_cfg) = &state.config.redis {
                        if let Ok(redis_client) =
                            crate::api::redis_client::RedisClient::connect(redis_cfg)
                        {
                            if let Ok(Some((_, Some(result_json)))) =
                                redis_client.get_job_status(&job_key).await
                            {
                                // Try to parse the result and return it
                                if let Ok(social_data) =
                                    serde_json::from_str::<serde_json::Value>(&result_json)
                                {
                                    return Ok(Json(ApiResponse::success(social_data)));
                                }
                            }
                        }
                    }
                    "Analysis completed, refreshing cache..."
                }
                "failed" => "Analysis failed, please try again",
                _ => "Analysis in progress, please check back later",
            };
            return Ok(create_job_status_response(&job_key, &status, message));
        }
        JobResult::Created => {
            return Ok(create_job_status_response(
                &job_key,
                "pending",
                "Analysis started, please check back later",
            ));
        }
        JobResult::Failed => {
            // Fall through to synchronous processing
        }
    }

    // Fallback: process synchronously if Redis is not available
    error!("Redis not available, falling back to synchronous processing");

    // Get user profile first
    let profile = match state.database.get_user_profile(fid).await {
        Ok(Some(profile)) => profile,
        Ok(None) => {
            let duration = start_time.elapsed();
            info!(
                "❌ GET /api/social/{} - {}ms - 404 (user not found)",
                fid,
                duration.as_millis()
            );
            return Ok(Json(ApiResponse::error(format!(
                "User with FID {fid} not found"
            ))));
        }
        Err(e) => {
            error!("Failed to get user profile for FID {}: {}", fid, e);
            let duration = start_time.elapsed();
            info!(
                "❌ GET /api/social/{} - {}ms - 500 (profile error)",
                fid,
                duration.as_millis()
            );
            return Ok(Json(ApiResponse::error(format!(
                "Failed to get user profile: {e}"
            ))));
        }
    };

    // Initialize social graph analyzer
    let analyzer = SocialGraphAnalyzer::new(state.database.clone());

    // Analyze user's social graph
    match analyzer.analyze_user(fid).await {
        Ok(social_profile) => {
            // Cache the social analysis
            tracing::debug!("Caching social analysis response for FID {}", fid);
            if let Err(e) = state.cache_service.set_social(fid, &social_profile).await {
                error!("Failed to cache social profile: {}", e);
            }

            // Convert social profile to JSON
            let social_data = serde_json::to_value(&social_profile).unwrap_or_else(|_| {
                serde_json::json!({
                    "error": "Failed to serialize social profile"
                })
            });

            let duration = start_time.elapsed();
            info!(
                "✅ GET /api/social/{} - {}ms - 200",
                fid,
                duration.as_millis()
            );
            Ok(Json(ApiResponse::success(social_data)))
        }
        Err(e) => {
            error!("Failed to analyze social graph for FID {}: {}", fid, e);
            let duration = start_time.elapsed();
            info!(
                "❌ GET /api/social/{} - {}ms - 500 (analysis error)",
                fid,
                duration.as_millis()
            );
            Ok(Json(ApiResponse::error(format!(
                "Failed to analyze social graph: {e}"
            ))))
        }
    }
}

/// Social graph analysis handler by username (GET /api/social/username/:username) with caching
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
    let job_key = format!("social:{fid}");

    // Check cache first for the FID
    match state.cache_service.get_social(fid).await {
        Ok(crate::api::cache::CacheResult::Fresh(cached_social)) => {
            info!(
                "📦 Social cache hit (fresh) for username {} (FID {})",
                username, fid
            );
            let social_data = serde_json::to_value(&cached_social).unwrap_or_else(|_| {
                serde_json::json!({
                    "error": "Failed to serialize cached social profile"
                })
            });
            return Ok(Json(ApiResponse::success(social_data)));
        }
        Ok(crate::api::cache::CacheResult::Stale(cached_social)) => {
            // Stale cache - return stale data and trigger background update
            info!("📦 Social cache hit (stale) for username {} (FID {}), triggering background update", username, fid);

            let job_config = JobConfig {
                job_type: "social",
                job_key: job_key.clone(),
                fid,
            };
            trigger_background_update(&state, &job_config).await;

            let social_data = serde_json::to_value(&cached_social).unwrap_or_else(|_| {
                serde_json::json!({
                    "error": "Failed to serialize cached social profile"
                })
            });
            return Ok(Json(ApiResponse::success(social_data)));
        }
        Ok(crate::api::cache::CacheResult::Updating(cached_social)) => {
            // Cache expired - return updating status with old data
            info!("🔄 Social cache expired (updating) for username {} (FID {}), returning old data with updating status", username, fid);

            let job_config = JobConfig {
                job_type: "social",
                job_key: job_key.clone(),
                fid,
            };
            trigger_background_update(&state, &job_config).await;

            return Ok(create_updating_response(
                &cached_social,
                "Data is being updated in the background. Please refresh to get the latest data.",
            ));
        }
        Ok(crate::api::cache::CacheResult::Miss) => {
            // Cache miss - check if job is already processing
            let job_config = JobConfig {
                job_type: "social",
                job_key: job_key.clone(),
                fid,
            };
            match check_or_create_job(&state, &job_config).await {
                JobResult::AlreadyExists(status) => {
                    let message = match status.as_str() {
                        "pending" => "Analysis is queued, please check back later",
                        "processing" => "Analysis in progress, please check back later",
                        "completed" => {
                            // Job completed but cache not updated yet - try to get result from status
                            if let Some(redis_cfg) = &state.config.redis {
                                if let Ok(redis_client) =
                                    crate::api::redis_client::RedisClient::connect(redis_cfg)
                                {
                                    if let Ok(Some((_, Some(result_json)))) =
                                        redis_client.get_job_status(&job_key).await
                                    {
                                        // Try to parse the result and return it
                                        if let Ok(social_data) =
                                            serde_json::from_str::<serde_json::Value>(&result_json)
                                        {
                                            return Ok(Json(ApiResponse::success(social_data)));
                                        }
                                    }
                                }
                            }
                            "Analysis completed, refreshing cache..."
                        }
                        "failed" => "Analysis failed, please try again",
                        _ => "Analysis in progress, please check back later",
                    };
                    return Ok(create_job_status_response(&job_key, &status, message));
                }
                JobResult::Created => {
                    return Ok(create_job_status_response(
                        &job_key,
                        "pending",
                        "Analysis started, please check back later",
                    ));
                }
                JobResult::Failed => {
                    // Fall through to synchronous processing
                    error!("Redis not available, falling back to synchronous processing");
                }
            }
        }
        Err(e) => {
            error!("Cache error: {}", e);
            // Continue to process synchronously
        }
    }

    // Initialize social graph analyzer
    let analyzer = SocialGraphAnalyzer::new(state.database.clone());

    // Analyze user's social graph using the FID from the profile
    match analyzer.analyze_user(fid).await {
        Ok(social_profile) => {
            // Cache the social analysis
            if let Err(e) = state.cache_service.set_social(fid, &social_profile).await {
                error!("Failed to cache social profile: {}", e);
            }

            // Convert social profile to JSON
            let social_data = serde_json::to_value(&social_profile).unwrap_or_else(|_| {
                serde_json::json!({
                    "error": "Failed to serialize social profile"
                })
            });

            Ok(Json(ApiResponse::success(social_data)))
        }
        Err(e) => {
            error!(
                "Failed to analyze social graph for username {} (FID {}): {}",
                username, fid, e
            );
            Ok(Json(ApiResponse::error(format!(
                "Failed to analyze social graph: {e}"
            ))))
        }
    }
}
