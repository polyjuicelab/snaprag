/// Search-related API handlers
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use tracing::error;
use tracing::info;

use super::AppState;
use crate::api::types::ApiResponse;
use crate::api::types::CastResponse;
use crate::api::types::CastSearchRequest;
use crate::api::types::ProfileResponse;
use crate::api::types::ProfileSearchRequest;
use crate::rag::CastRetriever;
use crate::rag::Retriever;

/// Search profiles semantically
pub async fn search_profiles(
    State(state): State<AppState>,
    Json(req): Json<ProfileSearchRequest>,
) -> Result<Json<ApiResponse<Vec<ProfileResponse>>>, StatusCode> {
    info!("POST /api/search/profiles: {}", req.query);

    let retriever = Retriever::new(state.database.clone(), state.embedding_service.clone());

    let results = match req.method.as_deref() {
        Some("semantic") => retriever.semantic_search(&req.query, req.limit, None).await,
        Some("keyword") => retriever.keyword_search(&req.query, req.limit).await,
        Some("hybrid") => retriever.hybrid_search(&req.query, req.limit).await,
        _ => retriever.auto_search(&req.query, req.limit).await,
    };

    match results {
        Ok(search_results) => {
            let response: Vec<ProfileResponse> = search_results
                .into_iter()
                .map(|r| ProfileResponse {
                    fid: r.profile.fid,
                    username: r.profile.username,
                    display_name: r.profile.display_name,
                    bio: r.profile.bio,
                    pfp_url: r.profile.pfp_url,
                    location: r.profile.location,
                    twitter_username: r.profile.twitter_username,
                    github_username: r.profile.github_username,
                    registered_at: None,
                    total_casts: None,
                    total_reactions: None,
                    total_links: None,
                })
                .collect();
            Ok(Json(ApiResponse::success(response)))
        }
        Err(e) => {
            error!("Error searching profiles: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Search casts semantically
pub async fn search_casts(
    State(state): State<AppState>,
    Json(req): Json<CastSearchRequest>,
) -> Result<Json<ApiResponse<Vec<CastResponse>>>, StatusCode> {
    info!("POST /api/search/casts: {}", req.query);

    let retriever = CastRetriever::new(state.database.clone(), state.embedding_service.clone());

    match retriever
        .semantic_search(&req.query, req.limit, Some(req.threshold))
        .await
    {
        Ok(results) => {
            let response: Vec<CastResponse> = results
                .into_iter()
                .map(|r| CastResponse {
                    message_hash: hex::encode(&r.message_hash),
                    fid: r.fid,
                    text: r.text,
                    timestamp: r.timestamp,
                    similarity: Some(r.similarity),
                })
                .collect();
            Ok(Json(ApiResponse::success(response)))
        }
        Err(e) => {
            error!("Error searching casts: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
