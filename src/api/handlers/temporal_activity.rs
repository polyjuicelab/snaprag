/// Temporal activity analysis API handlers
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use tracing::error;
use tracing::info;

use super::AppState;
use crate::api::types::ApiResponse;
use crate::api::types::CastSummary;
use crate::api::types::HourCount;
use crate::api::types::MonthCount;
use crate::api::types::TemporalActivityRequest;
use crate::api::types::TemporalActivityResponse;

/// Get temporal activity analysis for a user
pub async fn get_temporal_activity(
    State(state): State<AppState>,
    Path(fid): Path<i64>,
    Query(params): Query<TemporalActivityRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    let start_time = std::time::Instant::now();
    info!(
        "GET /api/users/{}/activity/temporal (start={:?}, end={:?})",
        fid, params.start_timestamp, params.end_timestamp
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

    // Get hourly distribution
    let hourly_distribution = match state
        .database
        .get_hourly_distribution(fid, start_farcaster, end_farcaster)
        .await
    {
        Ok(dist) => dist
            .into_iter()
            .map(|h| HourCount {
                hour: h.hour,
                count: h.count,
            })
            .collect(),
        Err(e) => {
            error!("Error fetching hourly distribution for FID {}: {}", fid, e);
            Vec::new()
        }
    };

    // Get monthly distribution
    let monthly_distribution = match state
        .database
        .get_monthly_distribution(fid, start_farcaster, end_farcaster)
        .await
    {
        Ok(dist) => dist
            .into_iter()
            .map(|m| MonthCount {
                month: m.month,
                count: m.count,
            })
            .collect(),
        Err(e) => {
            error!("Error fetching monthly distribution for FID {}: {}", fid, e);
            Vec::new()
        }
    };

    // Find most active hour
    let most_active_hour = hourly_distribution
        .iter()
        .max_by_key(|h| h.count)
        .map(|h| h.hour);

    // Find most active month
    let most_active_month = monthly_distribution
        .iter()
        .max_by_key(|m| m.count)
        .map(|m| m.month.clone());

    // Get first cast
    let first_cast = match state
        .database
        .get_first_cast_in_range(fid, start_farcaster, end_farcaster)
        .await
    {
        Ok(Some(cast)) => Some(CastSummary {
            message_hash: hex::encode(cast.message_hash),
            text: cast.text,
            timestamp: cast.timestamp,
        }),
        Ok(None) => None,
        Err(e) => {
            error!("Error fetching first cast for FID {}: {}", fid, e);
            None
        }
    };

    // Get last cast
    let last_cast = match state
        .database
        .get_last_cast_in_range(fid, start_farcaster, end_farcaster)
        .await
    {
        Ok(Some(cast)) => Some(CastSummary {
            message_hash: hex::encode(cast.message_hash),
            text: cast.text,
            timestamp: cast.timestamp,
        }),
        Ok(None) => None,
        Err(e) => {
            error!("Error fetching last cast for FID {}: {}", fid, e);
            None
        }
    };

    let response = TemporalActivityResponse {
        hourly_distribution,
        monthly_distribution,
        most_active_hour,
        most_active_month,
        first_cast,
        last_cast,
    };

    let duration = start_time.elapsed();
    info!(
        "✅ GET /api/users/{}/activity/temporal - {}ms - 200",
        fid,
        duration.as_millis()
    );

    let response_data = serde_json::to_value(&response).unwrap_or_else(|_| {
        serde_json::json!({
            "error": "Failed to serialize temporal activity response"
        })
    });
    Ok(Json(ApiResponse::success(response_data)))
}
