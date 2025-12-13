use std::collections::HashMap;

/// Content style analysis API handlers
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use tracing::error;
use tracing::info;

use super::AppState;
use crate::api::types::ApiResponse;
use crate::api::types::ContentStyleRequest;
use crate::api::types::ContentStyleResponse;
use crate::api::types::EmojiFrequency;
use crate::utils::emoji::count_emoji_frequencies;

/// Get content style analysis for a user
pub async fn get_content_style(
    State(state): State<AppState>,
    Path(fid): Path<i64>,
    Query(params): Query<ContentStyleRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    let start_time = std::time::Instant::now();
    info!(
        "GET /api/users/{}/content/style (start={:?}, end={:?})",
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

    // Get casts text for emoji analysis
    let casts_text = match state
        .database
        .get_casts_text_for_analysis(fid, start_farcaster, end_farcaster)
        .await
    {
        Ok(texts) => texts,
        Err(e) => {
            error!("Error fetching casts text for FID {}: {}", fid, e);
            Vec::new()
        }
    };

    // Extract and count emojis
    let mut emoji_frequencies: HashMap<String, usize> = HashMap::new();
    let mut total_characters = 0;
    let mut total_casts = 0;

    for text in &casts_text {
        total_characters += text.len();
        total_casts += 1;
        let frequencies = count_emoji_frequencies(text);
        for (emoji, count) in frequencies {
            *emoji_frequencies.entry(emoji).or_insert(0) += count;
        }
    }

    // Convert to sorted vector
    let mut top_emojis: Vec<EmojiFrequency> = emoji_frequencies
        .into_iter()
        .map(|(emoji, count)| EmojiFrequency {
            emoji,
            count: count as i64,
        })
        .collect();
    top_emojis.sort_by(|a, b| b.count.cmp(&a.count));
    top_emojis.truncate(20); // Top 20 emojis

    // Calculate average cast length
    let avg_cast_length = if total_casts > 0 {
        total_characters as f64 / total_casts as f64
    } else {
        0.0
    };

    // Get frame usage count
    let frames_used = match state
        .database
        .get_frame_usage_count(fid, start_farcaster, end_farcaster)
        .await
    {
        Ok(count) => count,
        Err(e) => {
            error!("Error fetching frame usage for FID {}: {}", fid, e);
            0
        }
    };

    let response = ContentStyleResponse {
        top_emojis,
        avg_cast_length,
        total_characters: total_characters as i64,
        frames_used,
        frames_created: 0,   // Not tracked yet
        channels_created: 0, // Not tracked yet
    };

    let duration = start_time.elapsed();
    info!(
        "✅ GET /api/users/{}/content/style - {}ms - 200",
        fid,
        duration.as_millis()
    );

    let response_data = serde_json::to_value(&response).unwrap_or_else(|_| {
        serde_json::json!({
            "error": "Failed to serialize content style response"
        })
    });
    Ok(Json(ApiResponse::success(response_data)))
}
