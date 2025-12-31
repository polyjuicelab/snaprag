/// Profile-related API handlers
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use tracing::error;
use tracing::info;
use tracing::warn;

use super::AppState;
use crate::api::types::ApiResponse;
use crate::api::types::ProfileResponse;
use crate::api::types::SearchQuery;
use crate::models::UserProfileQuery;

/// Extract user data fields from API response
fn extract_user_data_from_api(
    messages: Vec<crate::sync::client::FarcasterMessage>,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    let mut api_display_name = None;
    let mut api_bio = None;
    let mut api_pfp_url = None;
    let mut api_location = None;
    let mut api_twitter_username = None;
    let mut api_github_username = None;

    for message in messages {
        if let Some(data) = &message.data {
            // FarcasterMessageData has body as HashMap<String, serde_json::Value>
            if let Some(user_data_body) = data.body.get("userDataBody") {
                let data_type = user_data_body
                    .get("type")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0);
                let value = user_data_body
                    .get("value")
                    .and_then(|v| v.as_str())
                    .map(std::string::ToString::to_string);

                match data_type {
                    2 => api_display_name = value,     // Display
                    3 => api_bio = value,              // Bio
                    1 => api_pfp_url = value,          // Pfp
                    7 => api_location = value,         // Location
                    8 => api_twitter_username = value, // Twitter
                    9 => api_github_username = value,  // Github
                    _ => {}
                }
            }
        }
    }

    (
        api_display_name,
        api_bio,
        api_pfp_url,
        api_location,
        api_twitter_username,
        api_github_username,
    )
}

/// Get registration timestamp from API or database
async fn get_registration_timestamp(
    fid: i64,
    lazy_loader: Option<&std::sync::Arc<crate::sync::lazy_loader::LazyLoader>>,
    database: &crate::database::Database,
) -> Option<i64> {
    if let Some(loader) = lazy_loader {
        let client = loader.client();
        let Ok(fid_u64) = u64::try_from(fid) else {
            warn!("FID {} is negative, cannot fetch from API", fid);
            return database
                .get_registration_timestamp(fid)
                .await
                .unwrap_or(None);
        };

        match client.get_id_registry_onchain_event(fid_u64).await {
            Ok(Some(event)) => {
                // Convert block_timestamp (Unix timestamp) to Farcaster timestamp
                let Ok(farcaster_ts) =
                    i64::try_from(crate::unix_to_farcaster_timestamp(event.block_timestamp))
                else {
                    warn!("Farcaster timestamp overflow for FID {}", fid);
                    return database
                        .get_registration_timestamp(fid)
                        .await
                        .unwrap_or(None);
                };
                Some(farcaster_ts)
            }
            Ok(None) => {
                // No event found, fallback to database
                database
                    .get_registration_timestamp(fid)
                    .await
                    .unwrap_or(None)
            }
            Err(e) => {
                warn!(
                    "Failed to get registration event from API for FID {}: {}, falling back to database",
                    fid, e
                );
                database
                    .get_registration_timestamp(fid)
                    .await
                    .unwrap_or(None)
            }
        }
    } else {
        // Fallback to database if lazy_loader is not available
        database
            .get_registration_timestamp(fid)
            .await
            .unwrap_or(None)
    }
}

/// Get user data fields from API or database
async fn get_user_data_fields(
    profile: &crate::models::UserProfile,
    lazy_loader: Option<&std::sync::Arc<crate::sync::lazy_loader::LazyLoader>>,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    if let Some(loader) = lazy_loader {
        let client = loader.client();
        let Ok(fid_u64) = u64::try_from(profile.fid) else {
            warn!("FID {} is negative, cannot fetch from API", profile.fid);
            return (
                profile.display_name.clone(),
                profile.bio.clone(),
                profile.pfp_url.clone(),
                profile.location.clone(),
                profile.twitter_username.clone(),
                profile.github_username.clone(),
            );
        };

        match client.get_user_data_by_fid(fid_u64, None).await {
            Ok(user_data_response) => {
                let (
                    api_display_name,
                    api_bio,
                    api_pfp_url,
                    api_location,
                    api_twitter_username,
                    api_github_username,
                ) = extract_user_data_from_api(user_data_response.messages);

                // Use API values if available, otherwise fallback to database values
                (
                    api_display_name.or_else(|| profile.display_name.clone()),
                    api_bio.or_else(|| profile.bio.clone()),
                    api_pfp_url.or_else(|| profile.pfp_url.clone()),
                    api_location.or_else(|| profile.location.clone()),
                    api_twitter_username.or_else(|| profile.twitter_username.clone()),
                    api_github_username.or_else(|| profile.github_username.clone()),
                )
            }
            Err(e) => {
                warn!(
                    "Failed to get user data from API for FID {}: {}, using database values",
                    profile.fid, e
                );
                // Fallback to database values
                (
                    profile.display_name.clone(),
                    profile.bio.clone(),
                    profile.pfp_url.clone(),
                    profile.location.clone(),
                    profile.twitter_username.clone(),
                    profile.github_username.clone(),
                )
            }
        }
    } else {
        // Fallback to database values if lazy_loader is not available
        (
            profile.display_name.clone(),
            profile.bio.clone(),
            profile.pfp_url.clone(),
            profile.location.clone(),
            profile.twitter_username.clone(),
            profile.github_username.clone(),
        )
    }
}

/// Check cache and return cached profile if available
async fn check_profile_cache(
    cache_service: &crate::api::cache::CacheService,
    fid: i64,
    start_time: std::time::Instant,
) -> Option<Result<Json<ApiResponse<serde_json::Value>>, StatusCode>> {
    tracing::debug!("Checking cache for profile FID {}", fid);
    match cache_service.get_profile(fid).await {
        Ok(crate::api::cache::CacheResult::Fresh(cached_profile)) => {
            let duration = start_time.elapsed();
            info!(
                "📦 Profile cache hit (fresh) for FID {} - {}ms",
                fid,
                duration.as_millis()
            );
            let profile_data = serde_json::to_value(&cached_profile).unwrap_or_else(|_| {
                serde_json::json!({
                    "error": "Failed to serialize cached profile"
                })
            });
            Some(Ok(Json(ApiResponse::success(profile_data))))
        }
        Ok(crate::api::cache::CacheResult::Stale(cached_profile)) => {
            let duration = start_time.elapsed();
            info!(
                "📦 Profile cache hit (stale) for FID {} - {}ms",
                fid,
                duration.as_millis()
            );
            let profile_data = serde_json::to_value(&cached_profile).unwrap_or_else(|_| {
                serde_json::json!({
                    "error": "Failed to serialize cached profile"
                })
            });
            Some(Ok(Json(ApiResponse::success(profile_data))))
        }
        Ok(crate::api::cache::CacheResult::Updating(cached_profile)) => {
            let duration = start_time.elapsed();
            info!(
                "🔄 Profile cache expired (updating) for FID {} - {}ms",
                fid,
                duration.as_millis()
            );
            let profile_data = serde_json::to_value(&cached_profile).unwrap_or_else(|_| {
                serde_json::json!({
                    "error": "Failed to serialize cached profile"
                })
            });
            Some(Ok(Json(ApiResponse::success(serde_json::json!({
                "status": "updating",
                "data": profile_data,
                "message": "Profile data is being updated in the background. Please refresh to get the latest data."
            })))))
        }
        Ok(crate::api::cache::CacheResult::Miss) => {
            tracing::debug!(
                "No cache hit for profile FID {}, proceeding to database",
                fid
            );
            None
        }
        Err(e) => {
            error!("Cache error for FID {}: {}", fid, e);
            None
        }
    }
}

/// Build profile response from profile data
async fn build_profile_response(
    profile: &crate::models::UserProfile,
    lazy_loader: Option<&std::sync::Arc<crate::sync::lazy_loader::LazyLoader>>,
    database: &crate::database::Database,
) -> ProfileResponse {
    let registered_at = get_registration_timestamp(profile.fid, lazy_loader, database).await;
    let (display_name, bio, pfp_url, location, twitter_username, github_username) =
        get_user_data_fields(profile, lazy_loader).await;
    let total_casts = database.count_casts_by_fid(profile.fid).await.unwrap_or(0);
    let total_reactions = database
        .count_reactions_by_fid(profile.fid)
        .await
        .unwrap_or(0);
    let total_links = database.count_links_by_fid(profile.fid).await.unwrap_or(0);

    ProfileResponse {
        fid: profile.fid,
        username: profile.username.clone(),
        display_name,
        bio,
        pfp_url,
        location,
        twitter_username,
        github_username,
        registered_at,
        total_casts: Some(total_casts),
        total_reactions: Some(total_reactions),
        total_links: Some(total_links),
    }
}

/// Get profile by FID (always fetches from Snapchain and updates database)
///
/// # Errors
/// Returns an error if Snapchain API calls fail or database update fails
pub async fn get_profile(
    State(state): State<AppState>,
    Path(fid): Path<i64>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    let start_time = std::time::Instant::now();
    info!("GET /api/profiles/{}", fid);

    // Always fetch from Snapchain and update database
    let profile = if let Some(loader) = &state.lazy_loader {
        if let Ok(fid_val) = u64::try_from(fid) {
            match loader.fetch_user_profile_force(fid_val).await {
                Ok(p) => {
                    info!(
                        "✅ Successfully fetched and updated profile {} from Snapchain",
                        fid
                    );
                    Some(p)
                }
                Err(e) => {
                    error!("Failed to fetch profile {} from Snapchain: {}", fid, e);
                    // Fallback to database if Snapchain fails
                    match state.database.get_user_profile(fid).await {
                        Ok(Some(p)) => {
                            warn!(
                                "Using cached profile {} from database (Snapchain fetch failed)",
                                fid
                            );
                            Some(p)
                        }
                        Ok(None) => None,
                        Err(db_err) => {
                            error!("Database error while fetching profile {}: {}", fid, db_err);
                            None
                        }
                    }
                }
            }
        } else {
            error!("FID {} is negative, cannot fetch from Snapchain", fid);
            // Fallback to database
            match state.database.get_user_profile(fid).await {
                Ok(Some(p)) => Some(p),
                Ok(None) => None,
                Err(e) => {
                    error!("Database error: {}", e);
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            }
        }
    } else {
        // No lazy loader available, fallback to database
        warn!(
            "Lazy loader not available, falling back to database for FID {}",
            fid
        );
        match state.database.get_user_profile(fid).await {
            Ok(Some(p)) => Some(p),
            Ok(None) => None,
            Err(e) => {
                error!("Database error: {}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    };

    if let Some(profile) = profile {
        let response =
            build_profile_response(&profile, state.lazy_loader.as_ref(), &state.database).await;

        // Cache the response
        tracing::debug!("Caching profile response for FID {}", fid);
        if let Err(e) = state.cache_service.set_profile(fid, &response).await {
            error!("Failed to cache profile: {}", e);
        }

        let duration = start_time.elapsed();
        info!(
            "✅ GET /api/profiles/{} - {}ms - 200",
            fid,
            duration.as_millis()
        );
        let response_data = serde_json::to_value(&response).unwrap_or_else(|_| {
            serde_json::json!({
                "error": "Failed to serialize profile response"
            })
        });
        Ok(Json(ApiResponse::success(response_data)))
    } else {
        let duration = start_time.elapsed();
        info!(
            "❌ GET /api/profiles/{} - {}ms - 404",
            fid,
            duration.as_millis()
        );
        Err(StatusCode::NOT_FOUND)
    }
}

/// List profiles
///
/// # Errors
/// Returns an error if database queries fail
pub async fn list_profiles(
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    info!("GET /api/profiles?q={}&limit={}", params.q, params.limit);

    let query = UserProfileQuery {
        fid: None,
        username: None,
        display_name: None,
        bio: None,
        location: None,
        twitter_username: None,
        github_username: None,
        limit: Some(i64::try_from(params.limit).unwrap_or(i64::MAX)),
        offset: None,
        start_timestamp: None,
        end_timestamp: None,
        sort_by: None,
        sort_order: None,
        search_term: if params.q.is_empty() {
            None
        } else {
            Some(params.q)
        },
    };

    match state.database.list_user_profiles(query).await {
        Ok(profiles) => {
            // For list endpoints, we can optionally include stats, but it might be slow
            // For now, we'll skip stats for list endpoints to keep it fast
            let response: Vec<ProfileResponse> = profiles
                .into_iter()
                .map(|p| ProfileResponse {
                    fid: p.fid,
                    username: p.username,
                    display_name: p.display_name,
                    bio: p.bio,
                    pfp_url: p.pfp_url,
                    location: p.location,
                    twitter_username: p.twitter_username,
                    github_username: p.github_username,
                    registered_at: None,
                    total_casts: None,
                    total_reactions: None,
                    total_links: None,
                })
                .collect();
            let response_data = serde_json::to_value(&response).unwrap_or_else(|_| {
                serde_json::json!({
                    "error": "Failed to serialize profile response"
                })
            });
            Ok(Json(ApiResponse::success(response_data)))
        }
        Err(e) => {
            error!("Error listing profiles: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get profile by username (with caching)
///
/// # Errors
/// Returns an error if database queries fail or profile is not found
pub async fn get_profile_by_username(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    info!("GET /api/profiles/username/{}", username);

    // Get FID from username
    let profile = match state.database.get_user_profile_by_username(&username).await {
        Ok(Some(p)) => Some(p),
        Ok(None) => {
            // Try lazy loading if available
            state.lazy_loader.as_ref().map_or_else(
                || {
                    info!("⚡ Profile {} not found, attempting lazy load", username);
                    // For username-based lazy loading, we need to find the FID first
                    // This is a limitation - we can't lazy load by username directly
                    info!(
                        "⚠️ Lazy loading by username not supported, user {} not found",
                        username
                    );
                    None
                },
                |_| None,
            )
        }
        Err(e) => {
            error!("Error fetching profile by username: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // If we found a profile, get the FID and call get_profile() which handles caching
    if let Some(profile) = profile {
        get_profile(State(state), Path(profile.fid)).await
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

/// Get profile by Ethereum address (with caching)
///
/// # Errors
/// Returns an error if database queries fail or profile is not found
pub async fn get_profile_by_ethereum_address(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    info!("GET /api/profiles/address/ethereum/{}", address);

    // Get FID from Ethereum address
    let fid = match state.database.get_fid_by_ethereum_address(&address).await {
        Ok(Some(fid)) => fid,
        Ok(None) => {
            info!("No FID found for Ethereum address: {}", address);
            return Err(StatusCode::NOT_FOUND);
        }
        Err(e) => {
            error!("Error fetching FID by Ethereum address: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Use existing get_profile handler which handles caching
    get_profile(State(state), Path(fid)).await
}

/// Get profile by Solana address (with caching)
///
/// # Errors
/// Returns an error if database queries fail or profile is not found
pub async fn get_profile_by_solana_address(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    info!("GET /api/profiles/address/solana/{}", address);

    // Get FID from Solana address
    let fid = match state.database.get_fid_by_solana_address(&address).await {
        Ok(Some(fid)) => fid,
        Ok(None) => {
            info!("No FID found for Solana address: {}", address);
            return Err(StatusCode::NOT_FOUND);
        }
        Err(e) => {
            error!("Error fetching FID by Solana address: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Use existing get_profile handler which handles caching
    get_profile(State(state), Path(fid)).await
}

/// Get profile by address (auto-detect Ethereum or Solana, with caching)
///
/// # Errors
/// Returns an error if database queries fail or profile is not found
pub async fn get_profile_by_address(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    info!("GET /api/profiles/address/{}", address);

    // Get FID from address (auto-detect format)
    let fid = match state.database.get_fid_by_address(&address).await {
        Ok(Some(fid)) => fid,
        Ok(None) => {
            info!("No FID found for address: {}", address);
            return Err(StatusCode::NOT_FOUND);
        }
        Err(e) => {
            error!("Error fetching FID by address: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Use existing get_profile handler which handles caching
    get_profile(State(state), Path(fid)).await
}
