//! API route definitions

use axum::routing::delete;
use axum::routing::get;
use axum::routing::post;
use axum::Router;

use super::handlers::AppState;
use super::handlers::{
    self,
};

/// Create `RESTful` API router
pub fn api_routes(state: AppState) -> Router {
    Router::new()
        // Health check
        .route("/health", get(handlers::health))
        // Profile endpoints
        .route("/profiles", get(handlers::list_profiles))
        .route("/profiles/:fid", get(handlers::get_profile))
        .route(
            "/profiles/username/:username",
            get(handlers::get_profile_by_username),
        )
        .route(
            "/profiles/address/:address",
            get(handlers::get_profile_by_address),
        )
        .route(
            "/profiles/address/ethereum/:address",
            get(handlers::get_profile_by_ethereum_address),
        )
        .route(
            "/profiles/address/solana/:address",
            get(handlers::get_profile_by_solana_address),
        )
        // Fetch endpoints (lazy loading)
        .route("/fetch/user/:fid", post(handlers::fetch_user))
        .route("/fetch/users", post(handlers::fetch_users_batch))
        // Search endpoints
        .route("/search/profiles", post(handlers::search_profiles))
        .route("/search/casts", post(handlers::search_casts))
        // RAG endpoints
        .route("/rag/query", post(handlers::rag_query))
        // Chat endpoints (interactive AI role-play)
        .route("/chat/create", post(handlers::create_chat_session))
        .route("/chat/message", post(handlers::send_chat_message))
        .route("/chat/session", get(handlers::get_chat_session))
        .route(
            "/chat/session/:session_id",
            delete(handlers::delete_chat_session),
        )
        // Statistics
        .route("/stats", get(handlers::get_stats))
        // Cast statistics
        .route("/casts/stats/:fid", get(handlers::get_cast_stats))
        // Prometheus metrics
        .route("/metrics", get(handlers::get_metrics))
        // Social graph endpoints
        .route("/social/:fid", get(handlers::get_social_analysis))
        .route(
            "/social/username/:username",
            get(handlers::get_social_analysis_by_username),
        )
        // MBTI personality analysis endpoints
        .route("/mbti/:fid", get(handlers::get_mbti_analysis))
        .route(
            "/mbti/username/:username",
            get(handlers::get_mbti_analysis_by_username),
        )
        .route("/mbti/batch", post(handlers::batch_mbti_analysis))
        .route("/mbti/stats", get(handlers::get_mbti_stats))
        .route(
            "/mbti/search/:mbti_type",
            get(handlers::search_by_mbti_type),
        )
        .route(
            "/mbti/compatibility/:fid1/:fid2",
            get(handlers::get_mbti_compatibility),
        )
        .with_state(state)
}
