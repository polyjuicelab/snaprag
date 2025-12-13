/// Follower growth tracking API handlers
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use tracing::error;
use tracing::info;

use super::AppState;
use crate::api::types::ApiResponse;
use crate::api::types::FollowerGrowthRequest;
use crate::api::types::FollowerGrowthResponse;
use crate::api::types::MonthlyFollowerSnapshot;

/// Get follower growth metrics for a user
pub async fn get_follower_growth(
    State(state): State<AppState>,
    Path(fid): Path<i64>,
    Query(params): Query<FollowerGrowthRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    let start_time = std::time::Instant::now();
    info!(
        "GET /api/users/{}/followers/growth (start={:?}, end={:?})",
        fid, params.start_timestamp, params.end_timestamp
    );

    // Get current counts
    let current_followers = match state.database.get_current_follower_count(fid).await {
        Ok(count) => count,
        Err(e) => {
            error!("Error fetching current followers for FID {}: {}", fid, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    let current_following = match state.database.get_current_following_count(fid).await {
        Ok(count) => count,
        Err(e) => {
            error!("Error fetching current following for FID {}: {}", fid, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Convert Unix timestamps to Farcaster timestamps for database queries
    let start_farcaster = params.start_timestamp.map(|unix_ts| {
        #[allow(clippy::cast_sign_loss)] // Timestamps are always positive
        let farcaster_ts = crate::unix_to_farcaster_timestamp(unix_ts as u64);
        farcaster_ts as i64
    });
    let end_farcaster = params.end_timestamp.map(|unix_ts| {
        #[allow(clippy::cast_sign_loss)] // Timestamps are always positive
        let farcaster_ts = crate::unix_to_farcaster_timestamp(unix_ts as u64);
        farcaster_ts as i64
    });

    // Get counts at start of period
    let followers_at_start = if let Some(start) = start_farcaster {
        match state
            .database
            .get_follower_count_at_timestamp(fid, start)
            .await
        {
            Ok(count) => count,
            Err(e) => {
                error!("Error fetching followers at start for FID {}: {}", fid, e);
                current_followers // Fallback to current
            }
        }
    } else {
        current_followers
    };

    let following_at_start = if let Some(start) = start_farcaster {
        match state
            .database
            .get_following_count_at_timestamp(fid, start)
            .await
        {
            Ok(count) => count,
            Err(e) => {
                error!("Error fetching following at start for FID {}: {}", fid, e);
                current_following // Fallback to current
            }
        }
    } else {
        current_following
    };

    // Get monthly snapshots
    let monthly_snapshots = match state
        .database
        .get_monthly_follower_snapshots(fid, start_farcaster, end_farcaster)
        .await
    {
        Ok(snapshots) => snapshots
            .into_iter()
            .map(|s| MonthlyFollowerSnapshot {
                month: s.month,
                followers: s.followers,
                following: s.following,
            })
            .collect(),
        Err(e) => {
            error!("Error fetching monthly snapshots for FID {}: {}", fid, e);
            Vec::new()
        }
    };

    let net_growth = current_followers.saturating_sub(followers_at_start);

    let response = FollowerGrowthResponse {
        current_followers,
        current_following,
        followers_at_start,
        following_at_start,
        net_growth,
        monthly_snapshots,
    };

    let duration = start_time.elapsed();
    info!(
        "✅ GET /api/users/{}/followers/growth - {}ms - 200",
        fid,
        duration.as_millis()
    );

    let response_data = serde_json::to_value(&response).unwrap_or_else(|_| {
        serde_json::json!({
            "error": "Failed to serialize follower growth response"
        })
    });
    Ok(Json(ApiResponse::success(response_data)))
}
