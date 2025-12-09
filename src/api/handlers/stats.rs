/// Stats-related API handlers
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use tracing::info;

use super::AppState;
use crate::api::types::ApiResponse;
use crate::api::types::StatsResponse;

/// Get stats including cache information
pub async fn get_stats(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<StatsResponse>>, StatusCode> {
    info!("GET /api/stats");

    // Get basic counts
    let total_profiles =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(DISTINCT fid) FROM user_profile_changes")
            .fetch_one(state.database.pool())
            .await
            .unwrap_or(0);

    let total_casts = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM casts")
        .fetch_one(state.database.pool())
        .await
        .unwrap_or(0);

    let profiles_with_embeddings = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM profile_embeddings WHERE profile_embedding IS NOT NULL",
    )
    .fetch_one(state.database.pool())
    .await
    .unwrap_or(0);

    let casts_with_embeddings =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM cast_embeddings")
            .fetch_one(state.database.pool())
            .await
            .unwrap_or(0);

    // Get cache statistics
    let cache_stats = state.cache_service.get_stats().await;

    Ok(Json(ApiResponse::success(StatsResponse {
        total_profiles,
        total_casts,
        profiles_with_embeddings,
        casts_with_embeddings,
        cache_stats: Some(crate::api::types::CacheStatsResponse {
            hits: cache_stats.hits,
            misses: cache_stats.misses,
            hit_rate: cache_stats.hit_rate(),
            evictions: 0, // Redis doesn't track evictions the same way
            expired_cleanups: cache_stats.stale_hits, // Use stale_hits as a proxy
            profile_entries: 0, // Redis doesn't provide entry counts easily
            social_entries: 0,
            mbti_entries: 0,
            total_entries: 0,
            max_entries: 0, // Redis doesn't have a fixed max
            usage_percentage: 0.0,
        }),
    })))
}
