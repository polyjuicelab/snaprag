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
    /// Registration timestamp (Unix timestamp in seconds)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registered_at: Option<i64>,
    /// Total number of casts (posts) by this user
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_casts: Option<i64>,
    /// Total number of reactions given by this user
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_reactions: Option<i64>,
    /// Total number of links (follows) by this user
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_links: Option<i64>,
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

/// Cast statistics request (query parameters)
#[derive(Debug, Deserialize)]
pub struct CastStatsRequest {
    /// Start timestamp (Unix timestamp in seconds)
    #[serde(default)]
    pub start_timestamp: Option<i64>,
    /// End timestamp (Unix timestamp in seconds)
    #[serde(default)]
    pub end_timestamp: Option<i64>,
}

/// Daily date distribution entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateDistributionEntry {
    pub date: String, // YYYY-MM-DD format
    pub count: i64,
}

/// Word frequency entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordFrequency {
    pub word: String,
    pub count: i64,
    pub language: String, // ISO 639-1 language code (e.g., "en", "zh")
}

/// Cast statistics response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CastStatsResponse {
    pub total_casts: i64,
    pub date_range: DateRange,
    pub date_distribution: Vec<DateDistributionEntry>,
    pub language_distribution: std::collections::HashMap<String, i64>,
    pub top_nouns: Vec<WordFrequency>,
    pub top_verbs: Vec<WordFrequency>,
}

/// Date range
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateRange {
    pub start: String, // ISO 8601 format
    pub end: String,   // ISO 8601 format
}

/// Engagement metrics request (query parameters)
#[derive(Debug, Deserialize)]
pub struct EngagementRequest {
    /// Start timestamp (Unix timestamp in seconds)
    #[serde(default)]
    pub start_timestamp: Option<i64>,
    /// End timestamp (Unix timestamp in seconds)
    #[serde(default)]
    pub end_timestamp: Option<i64>,
}

/// Popular cast with engagement metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PopularCast {
    pub message_hash: String, // hex-encoded
    pub text: Option<String>,
    pub reactions: i64,
    pub recasts: i64,
    pub replies: i64,
    pub timestamp: i64,
}

/// Top interactive user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopInteractiveUser {
    pub fid: i64,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub interaction_count: i64,
}

/// Engagement metrics response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngagementResponse {
    pub reactions_received: i64,
    pub recasts_received: i64,
    pub replies_received: i64,
    pub total_engagement: i64,
    pub most_popular_cast: Option<PopularCast>,
    pub top_reactors: Vec<TopInteractiveUser>,
}

/// Temporal activity request (query parameters)
#[derive(Debug, Deserialize)]
pub struct TemporalActivityRequest {
    /// Start timestamp (Unix timestamp in seconds)
    #[serde(default)]
    pub start_timestamp: Option<i64>,
    /// End timestamp (Unix timestamp in seconds)
    #[serde(default)]
    pub end_timestamp: Option<i64>,
}

/// Hour count entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HourCount {
    pub hour: i32, // 0-23
    pub count: i64,
}

/// Month count entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthCount {
    pub month: String, // "YYYY-MM"
    pub count: i64,
}

/// Cast summary for temporal activity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CastSummary {
    pub message_hash: String, // hex-encoded
    pub text: Option<String>,
    pub timestamp: i64,
}

/// Temporal activity response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalActivityResponse {
    pub hourly_distribution: Vec<HourCount>,
    pub monthly_distribution: Vec<MonthCount>,
    pub most_active_hour: Option<i32>,
    pub most_active_month: Option<String>,
    pub first_cast: Option<CastSummary>,
    pub last_cast: Option<CastSummary>,
}

/// Content style request (query parameters)
#[derive(Debug, Deserialize)]
pub struct ContentStyleRequest {
    /// Start timestamp (Unix timestamp in seconds)
    #[serde(default)]
    pub start_timestamp: Option<i64>,
    /// End timestamp (Unix timestamp in seconds)
    #[serde(default)]
    pub end_timestamp: Option<i64>,
}

/// Emoji frequency entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmojiFrequency {
    pub emoji: String,
    pub count: i64,
}

/// Content style response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentStyleResponse {
    pub top_emojis: Vec<EmojiFrequency>,
    pub avg_cast_length: f64,
    pub total_characters: i64,
    pub frames_used: i64,
    pub frames_created: i64,   // Always 0 for now - not tracked
    pub channels_created: i64, // Always 0 for now - not tracked
}

/// Follower growth request (query parameters)
#[derive(Debug, Deserialize)]
pub struct FollowerGrowthRequest {
    /// Start timestamp (Unix timestamp in seconds)
    #[serde(default)]
    pub start_timestamp: Option<i64>,
    /// End timestamp (Unix timestamp in seconds)
    #[serde(default)]
    pub end_timestamp: Option<i64>,
}

/// Monthly follower snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthlyFollowerSnapshot {
    pub month: String, // "YYYY-MM"
    pub followers: i64,
    pub following: i64,
}

/// Follower growth response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowerGrowthResponse {
    pub current_followers: i64,
    pub current_following: i64,
    pub followers_at_start: i64,
    pub following_at_start: i64,
    pub net_growth: i64,
    pub monthly_snapshots: Vec<MonthlyFollowerSnapshot>,
}

/// Network averages request (query parameters)
#[derive(Debug, Deserialize)]
pub struct NetworkAveragesRequest {
    /// Start timestamp (Unix timestamp in seconds)
    #[serde(default)]
    pub start_timestamp: Option<i64>,
    /// End timestamp (Unix timestamp in seconds)
    #[serde(default)]
    pub end_timestamp: Option<i64>,
}

/// Percentiles for comparison
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Percentiles {
    pub p50: i64,
    pub p75: i64,
    pub p90: i64,
}

/// Network averages response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkAveragesResponse {
    pub avg_casts_per_user: f64,
    pub avg_reactions_per_user: f64,
    pub avg_followers_per_user: f64,
    pub total_active_users: i64,
    pub percentiles: PercentilesResponse,
}

/// Percentiles response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PercentilesResponse {
    pub casts: Percentiles,
    pub reactions: Percentiles, // Placeholder - can be implemented later
}

/// Domain/username status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainStatusResponse {
    pub has_ens: bool,
    pub ens_name: Option<String>,
    pub has_farcaster_name: bool,
    pub farcaster_name: Option<String>,
    pub username_type: Option<String>, // "fname" or "ens"
}
