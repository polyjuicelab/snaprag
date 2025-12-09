//! API request and response types

use serde::Deserialize;
use serde::Serialize;

/// Create chat session request
#[derive(Debug, Deserialize)]
pub struct CreateChatRequest {
    /// FID or username (@xxx) to chat with
    pub user: String,
    /// Maximum number of relevant casts to use as context
    #[serde(default = "default_context_limit")]
    pub context_limit: usize,
    /// LLM temperature (0.0 - 1.0)
    #[serde(default = "default_temperature")]
    pub temperature: f32,
}

const fn default_context_limit() -> usize {
    20
}

const fn default_temperature() -> f32 {
    0.7
}

/// Create chat session response
#[derive(Debug, Serialize)]
pub struct CreateChatResponse {
    pub session_id: String,
    pub fid: i64,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub total_casts: usize,
}

/// Send chat message request
#[derive(Debug, Deserialize)]
pub struct ChatMessageRequest {
    pub session_id: String,
    pub message: String,
}

/// Chat message response
#[derive(Debug, Serialize)]
pub struct ChatMessageResponse {
    pub session_id: String,
    pub message: String,
    pub relevant_casts_count: usize,
    pub conversation_length: usize,
}

/// Get session info request
#[derive(Debug, Deserialize)]
pub struct GetSessionRequest {
    pub session_id: String,
}

/// Session info response
#[derive(Debug, Serialize)]
pub struct SessionInfoResponse {
    pub session_id: String,
    pub fid: i64,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub conversation_history: Vec<super::session::ChatMessage>,
    pub created_at: u64,
    pub last_activity: u64,
}

/// Standard API response wrapper
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    pub const fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// Search query parameters
#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub threshold: Option<f32>,
}

const fn default_limit() -> usize {
    20
}

/// Profile search request
#[derive(Debug, Deserialize)]
pub struct ProfileSearchRequest {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub method: Option<String>,
}

/// Cast search request
#[derive(Debug, Deserialize)]
pub struct CastSearchRequest {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default = "default_threshold")]
    pub threshold: f32,
}

const fn default_threshold() -> f32 {
    0.5
}

/// RAG query request
#[derive(Debug, Deserialize)]
pub struct RagQueryRequest {
    pub question: String,
    #[serde(default = "default_rag_limit")]
    pub retrieval_limit: usize,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
}

const fn default_rag_limit() -> usize {
    10
}

const fn default_max_tokens() -> usize {
    2000
}

/// Profile response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileResponse {
    pub fid: i64,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub pfp_url: Option<String>,
    pub location: Option<String>,
    pub twitter_username: Option<String>,
    pub github_username: Option<String>,
}

/// Cast response
#[derive(Debug, Serialize)]
pub struct CastResponse {
    pub message_hash: String,
    pub fid: i64,
    pub text: String,
    pub timestamp: i64,
    pub similarity: Option<f32>,
}

/// Cache statistics response
#[derive(Debug, Serialize)]
pub struct CacheStatsResponse {
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
    pub evictions: u64,
    pub expired_cleanups: u64,
    pub profile_entries: usize,
    pub social_entries: usize,
    pub mbti_entries: usize,
    pub total_entries: usize,
    pub max_entries: usize,
    pub usage_percentage: f64,
}

/// Statistics response
#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub total_profiles: i64,
    pub total_casts: i64,
    pub profiles_with_embeddings: i64,
    pub casts_with_embeddings: i64,
    pub cache_stats: Option<CacheStatsResponse>,
}

/// Fetch user request
#[derive(Debug, Deserialize)]
pub struct FetchUserRequest {
    #[serde(default)]
    pub with_casts: bool,
    #[serde(default)]
    pub generate_embeddings: bool,
    pub embedding_endpoint: Option<String>,
    #[serde(default = "default_max_casts")]
    pub max_casts: usize,
}

const fn default_max_casts() -> usize {
    1000
}

/// Fetch users batch request
#[derive(Debug, Deserialize)]
pub struct FetchUsersBatchRequest {
    pub fids: Vec<u64>,
    #[serde(default)]
    pub with_casts: bool,
    #[serde(default)]
    pub generate_embeddings: bool,
    pub embedding_endpoint: Option<String>,
}

/// Fetch response
#[derive(Debug, Serialize)]
pub struct FetchResponse {
    pub profile: ProfileResponse,
    pub casts_count: usize,
    pub embeddings_generated: Option<usize>,
    pub source: String, // "database" or "snapchain"
}
