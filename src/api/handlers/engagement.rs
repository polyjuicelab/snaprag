/// Engagement metrics API handlers
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use tracing::error;
use tracing::info;

use super::AppState;
use crate::api::types::ApiResponse;
use crate::api::types::EngagementRequest;
use crate::api::types::EngagementResponse;
use crate::api::types::PopularCast;
use crate::api::types::TopInteractiveUser;

fn unix_to_farcaster_i64(unix_ts: i64) -> i64 {
    let unix_u64 = u64::try_from(unix_ts).unwrap_or(0);
    let farcaster_ts = crate::unix_to_farcaster_timestamp(unix_u64);
    i64::try_from(farcaster_ts).unwrap_or(i64::MAX)
}

/// Get engagement metrics for a user
///
/// # Errors
/// Returns an error if database queries fail or timestamps are invalid.
#[allow(clippy::too_many_lines)]
pub async fn get_engagement(
    State(state): State<AppState>,
    Path(fid): Path<i64>,
    Query(params): Query<EngagementRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    let start_time = std::time::Instant::now();
    info!(
        "GET /api/users/{}/engagement (start={:?}, end={:?})",
        fid, params.start_timestamp, params.end_timestamp
    );

    // Convert Unix timestamps to Farcaster timestamps for database queries
    let start_farcaster = params.start_timestamp.map(unix_to_farcaster_i64);
    let end_farcaster = params.end_timestamp.map(unix_to_farcaster_i64);

    // Get reactions received (likes)
    let reactions_received = match state
        .database
        .get_reactions_received(fid, start_farcaster, end_farcaster, 1)
        .await
    {
        Ok(count) => count,
        Err(e) => {
            error!("Error fetching reactions received for FID {}: {}", fid, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Get recasts received
    let recasts_received = match state
        .database
        .get_recasts_received(fid, start_farcaster, end_farcaster)
        .await
    {
        Ok(count) => count,
        Err(e) => {
            error!("Error fetching recasts received for FID {}: {}", fid, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Get replies received
    let replies_received = match state
        .database
        .get_replies_received(fid, start_farcaster, end_farcaster)
        .await
    {
        Ok(count) => count,
        Err(e) => {
            error!("Error fetching replies received for FID {}: {}", fid, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Get most popular cast
    let most_popular_cast = match state
        .database
        .get_most_popular_cast(fid, start_farcaster, end_farcaster)
        .await
    {
        Ok(cast) => cast.map(|c| PopularCast {
            message_hash: hex::encode(c.message_hash),
            text: c.text,
            reactions: c.reactions,
            recasts: c.recasts,
            replies: c.replies,
            timestamp: c.timestamp,
        }),
        Err(e) => {
            error!("Error fetching most popular cast for FID {}: {}", fid, e);
            None
        }
    };

    // Get top interactive users
    let top_reactors = match state
        .database
        .get_top_interactive_users(fid, start_farcaster, end_farcaster, 10)
        .await
    {
        Ok(users) => users
            .into_iter()
            .map(|u| TopInteractiveUser {
                fid: u.fid,
                username: u.username,
                display_name: u.display_name,
                pfp_url: u.pfp_url,
                interaction_count: u.interaction_count,
            })
            .collect(),
        Err(e) => {
            error!(
                "Error fetching top interactive users for FID {}: {}",
                fid, e
            );
            Vec::new()
        }
    };

    let total_engagement = reactions_received + recasts_received + replies_received;

    let response = EngagementResponse {
        reactions_received,
        recasts_received,
        replies_received,
        total_engagement,
        most_popular_cast,
        top_reactors,
    };

    let duration = start_time.elapsed();
    info!(
        "✅ GET /api/users/{}/engagement - {}ms - 200",
        fid,
        duration.as_millis()
    );

    let response_data = serde_json::to_value(&response).unwrap_or_else(|_| {
        serde_json::json!({
            "error": "Failed to serialize engagement response"
        })
    });
    Ok(Json(ApiResponse::success(response_data)))
}
