/// Network statistics API handlers
use axum::extract::Query;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use tracing::error;
use tracing::info;

use super::AppState;
use crate::api::types::ApiResponse;
use crate::api::types::NetworkAveragesRequest;
use crate::api::types::NetworkAveragesResponse;
use crate::api::types::Percentiles;
use crate::api::types::PercentilesResponse;

/// Get network averages for comparison
pub async fn get_network_averages(
    State(state): State<AppState>,
    Query(params): Query<NetworkAveragesRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    let start_time = std::time::Instant::now();
    info!(
        "GET /api/stats/network/averages (start={:?}, end={:?})",
        params.start_timestamp, params.end_timestamp
    );

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

    // Get network averages
    let averages = match state
        .database
        .get_network_averages(start_farcaster, end_farcaster)
        .await
    {
        Ok(avg) => avg,
        Err(e) => {
            error!("Error fetching network averages: {}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Get percentiles
    let cast_percentiles = match state
        .database
        .get_cast_percentiles(start_farcaster, end_farcaster)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            error!("Error fetching cast percentiles: {}", e);
            Percentiles {
                p50: 0,
                p75: 0,
                p90: 0,
            }
        }
    };

    let response = NetworkAveragesResponse {
        avg_casts_per_user: averages.avg_casts_per_user,
        avg_reactions_per_user: averages.avg_reactions_per_user,
        avg_followers_per_user: averages.avg_followers_per_user,
        total_active_users: averages.total_active_users,
        percentiles: PercentilesResponse {
            casts: cast_percentiles,
            reactions: Percentiles {
                p50: 0,
                p75: 0,
                p90: 0,
            }, // Placeholder - can be implemented later
        },
    };

    let duration = start_time.elapsed();
    info!(
        "✅ GET /api/stats/network/averages - {}ms - 200",
        duration.as_millis()
    );

    let response_data = serde_json::to_value(&response).unwrap_or_else(|_| {
        serde_json::json!({
            "error": "Failed to serialize network averages response"
        })
    });
    Ok(Json(ApiResponse::success(response_data)))
}
