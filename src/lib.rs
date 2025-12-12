//! `SnapRAG` - Farcaster data synchronization and RAG library
//!
//! `SnapRAG` is a comprehensive Rust library for working with Farcaster protocol data.
//! It provides high-performance data synchronization, vector embeddings, semantic search,
//! and retrieval-augmented generation (RAG) capabilities.
//!
//! # Features
//!
//! - **Data Sync**: Real-time and historical synchronization from Snapchain
//! - **Vector Search**: Semantic search using embeddings (`OpenAI`, Ollama, local GPU)
//! - **RAG**: Retrieval-augmented generation for natural language queries
//! - **Social Graph**: Network analysis and relationship mapping
//! - **MBTI Analysis**: Personality inference from user behavior
//! - **Event Sourcing**: Complete change tracking and audit logs

// Clippy lint configuration
// Gradually enabling lints to improve code quality
// #![allow(clippy::missing_errors_doc)] - Being fixed
// #![allow(clippy::missing_panics_doc)] - Being fixed
#![allow(clippy::doc_markdown)]
//!
//! # Quick Start
//!
//! ## 1. Configuration
//!
//! Create `config.toml` from `config.example.toml` and set your database URL:
//!
//! ```toml
//! [database]
//! url = "postgresql://user:pass@localhost/snaprag"
//!
//! [sync]
//! snapchain_http_endpoint = "http://snapchain:8080"
//! snapchain_grpc_endpoint = "http://snapchain:2283"
//! ```
//!
//! ## 2. Basic Usage
//!
//! ```rust,no_run
//! use snaprag::{SnapRag, AppConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Load configuration
//!     let config = AppConfig::load()?;
//!     
//!     // Create SnapRAG instance (mutable for sync operations)
//!     let mut snaprag = SnapRag::new(&config).await?;
//!     
//!     // Initialize database schema
//!     snaprag.init_database().await?;
//!     
//!     // Start synchronization
//!     snaprag.start_sync().await?;
//!     
//!     // Query data
//!     let profiles = snaprag.search_profiles("developer").await?;
//!     println!("Found {} profiles", profiles.len());
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## 3. Semantic Search
//!
//! ```rust,no_run
//! use snaprag::{SnapRag, AppConfig};
//!
//! # async fn example() -> snaprag::Result<()> {
//! # let config = AppConfig::load()?;
//! # let snaprag = SnapRag::new(&config).await?;
//! // Search for casts semantically
//! let results = snaprag.semantic_search_casts(
//!     "artificial intelligence and machine learning",
//!     10,  // limit
//!     Some(0.7)  // similarity threshold
//! ).await?;
//!
//! for cast in results {
//!     println!("{}: {} (similarity: {:.2})",
//!         cast.fid, cast.text, cast.similarity);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## 4. RAG Queries
//!
//! ```rust,no_run
//! use snaprag::{SnapRag, AppConfig};
//!
//! # async fn example() -> snaprag::Result<()> {
//! # let config = AppConfig::load()?;
//! # let snaprag = SnapRag::new(&config).await?;
//! // Create RAG service
//! let rag = snaprag.create_rag_service().await?;
//!
//! // Ask natural language question
//! let response = rag.query("What are people saying about Ethereum?").await?;
//! println!("Answer: {}", response.answer);
//! # Ok(())
//! # }
//! ```
//!
//! # Architecture
//!
//! `SnapRAG` follows a modular architecture:
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │           CLI / API Layer                   │
//! ├─────────────────────────────────────────────┤
//! │  SnapRag (High-level Interface)            │
//! ├──────────────┬──────────────┬───────────────┤
//! │   Sync       │     RAG      │  Social Graph │
//! │   Service    │   Service    │   Analyzer    │
//! ├──────────────┴──────────────┴───────────────┤
//! │         Embeddings & LLM Layer             │
//! ├─────────────────────────────────────────────┤
//! │           Database Layer                    │
//! └─────────────────────────────────────────────┘
//! ```
//!
//! # Modules
//!
//! - [`api`]: HTTP API server and handlers
//! - [`cli`]: Command-line interface
//! - [`config`]: Configuration management
//! - [`database`]: `PostgreSQL` operations
//! - [`embeddings`]: Vector embeddings generation
//! - [`llm`]: LLM client for text generation
//! - [`rag`]: Retrieval-augmented generation
//! - [`sync`]: Data synchronization from Snapchain
//! - [`social_graph`]: Social network analysis
//! - [`personality`]: MBTI personality inference
//!
//! # Error Handling
//!
//! All operations return [`Result<T>`] with [`SnapRagError`] for comprehensive error information.

pub mod api;
pub mod cli;
pub mod config;
pub mod database;
pub mod embeddings;
pub mod errors;
pub mod generated;
pub mod grpc_client;
pub mod llm;
pub mod logging;
pub mod models;
pub mod personality;
pub mod rag;
pub mod social_graph;
pub mod sync;
pub mod text_analysis;

// Unit test modules integrated into main lib tests
// See src/tests/unit_tests.rs for comprehensive unit tests

/// Farcaster epoch constant (January 1, 2021 UTC in milliseconds)
pub const FARCASTER_EPOCH: u64 = 1_609_459_200_000;

/// Convert Farcaster timestamp (seconds since Farcaster epoch) to Unix timestamp (seconds since Unix epoch)
///
/// # Examples
///
/// ```
/// use snaprag::farcaster_to_unix_timestamp;
///
/// let farcaster_ts = 0; // Farcaster epoch
/// let unix_ts = farcaster_to_unix_timestamp(farcaster_ts);
/// assert_eq!(unix_ts, 1609459200); // Jan 1, 2021
/// ```
#[must_use]
pub const fn farcaster_to_unix_timestamp(farcaster_timestamp: u64) -> u64 {
    farcaster_timestamp + (FARCASTER_EPOCH / 1000)
}

/// Convert Unix timestamp (seconds since Unix epoch) to Farcaster timestamp (seconds since Farcaster epoch)
///
/// # Examples
///
/// ```
/// use snaprag::unix_to_farcaster_timestamp;
///
/// let unix_ts = 1609459200; // Jan 1, 2021
/// let farcaster_ts = unix_to_farcaster_timestamp(unix_ts);
/// assert_eq!(farcaster_ts, 0); // Farcaster epoch
/// ```
#[must_use]
pub const fn unix_to_farcaster_timestamp(unix_timestamp: u64) -> u64 {
    unix_timestamp - (FARCASTER_EPOCH / 1000)
}

#[cfg(test)]
pub mod tests;

// Re-export commonly used types
use std::sync::Arc;

pub use config::AppConfig;
pub use database::Database;
// Re-export embedding stats types
pub use embeddings::backfill::BackfillStats as ProfileBackfillStats;
// Re-export embeddings functionality
pub use embeddings::{
    backfill_cast_embeddings,
    backfill_embeddings as backfill_profile_embeddings,
    CastBackfillStats,
    EmbeddingService,
};
pub use errors::*;
// Re-export LLM functionality
pub use llm::{
    ChatMessage,
    LlmService,
    StreamingResponse,
};
pub use models::*;
// Re-export RAG functionality
pub use rag::{
    CastContextAssembler,
    CastRetriever,
    ContextAssembler,
    RagQuery,
    RagResponse,
    RagService,
    RetrievalMethod,
    Retriever,
    SearchResult,
};
pub use sync::lazy_loader::LazyLoader;
pub use sync::service::SyncService;
use tracing::info;

/// Main `SnapRAG` client for high-level operations
pub struct SnapRag {
    config: AppConfig,
    database: Arc<Database>,
    sync_service: Option<Arc<SyncService>>,
    lazy_loader: Option<Arc<LazyLoader>>,
    cache_service: Option<Arc<crate::api::cache::CacheService>>,
}

impl SnapRag {
    /// Create a new `SnapRAG` instance
    ///
    /// # Errors
    /// Returns error if database connection fails or configuration is invalid
    pub async fn new(config: &AppConfig) -> Result<Self> {
        let database = Arc::new(Database::from_config(config).await?);

        // Initialize cache service if Redis is configured
        let cache_service = if let Some(redis_cfg) = &config.redis {
            let redis = Arc::new(crate::api::redis_client::RedisClient::connect(redis_cfg)?);
            Some(Arc::new(crate::api::cache::CacheService::with_config(
                redis,
                crate::api::cache::CacheConfig {
                    profile_ttl: std::time::Duration::from_secs(config.cache.profile_ttl_secs),
                    social_ttl: std::time::Duration::from_secs(config.cache.social_ttl_secs),
                    mbti_ttl: std::time::Duration::from_secs(7200), // 2 hours for MBTI
                    cast_stats_ttl: std::time::Duration::from_secs(
                        config.cache.cast_stats_ttl_secs,
                    ),
                    stale_threshold: std::time::Duration::from_secs(redis_cfg.stale_threshold_secs),
                    enable_stats: config.cache.enable_stats,
                },
            )))
        } else {
            None
        };

        Ok(Self {
            config: config.clone(),
            database,
            sync_service: None,
            lazy_loader: None,
            cache_service,
        })
    }

    /// Create `SnapRAG` instance with lazy loading enabled
    ///
    /// # Errors
    /// Returns error if database connection fails, gRPC client creation fails, or configuration is invalid
    pub async fn new_with_lazy_loading(config: &AppConfig) -> Result<Self> {
        let database = Arc::new(Database::from_config(config).await?);
        let snapchain_client = Arc::new(sync::SnapchainClient::from_config(config).await?);
        let lazy_loader = Some(Arc::new(LazyLoader::new(
            database.clone(),
            snapchain_client,
        )));

        // Initialize cache service if Redis is configured
        let cache_service = if let Some(redis_cfg) = &config.redis {
            let redis = Arc::new(crate::api::redis_client::RedisClient::connect(redis_cfg)?);
            Some(Arc::new(crate::api::cache::CacheService::with_config(
                redis,
                crate::api::cache::CacheConfig {
                    profile_ttl: std::time::Duration::from_secs(config.cache.profile_ttl_secs),
                    social_ttl: std::time::Duration::from_secs(config.cache.social_ttl_secs),
                    mbti_ttl: std::time::Duration::from_secs(7200), // 2 hours for MBTI
                    cast_stats_ttl: std::time::Duration::from_secs(
                        config.cache.cast_stats_ttl_secs,
                    ),
                    stale_threshold: std::time::Duration::from_secs(redis_cfg.stale_threshold_secs),
                    enable_stats: config.cache.enable_stats,
                },
            )))
        } else {
            None
        };

        Ok(Self {
            config: config.clone(),
            database,
            sync_service: None,
            lazy_loader,
            cache_service,
        })
    }

    /// Initialize the database schema
    ///
    /// # Errors
    /// Returns error if database schema initialization fails
    pub async fn init_database(&self) -> Result<()> {
        self.database.init_schema().await?;
        info!("Database schema initialized");
        Ok(())
    }

    /// Get the database instance for direct access
    #[must_use]
    pub const fn database(&self) -> &Arc<Database> {
        &self.database
    }

    /// Get reference to lazy loader
    #[must_use]
    pub const fn lazy_loader(&self) -> Option<&Arc<LazyLoader>> {
        self.lazy_loader.as_ref()
    }

    /// Get user profile with automatic lazy loading
    ///
    /// # Errors
    /// Returns error if database query fails or lazy loading from Snapchain fails
    pub async fn get_user_profile_smart(&self, fid: i64) -> Result<Option<UserProfile>> {
        // Try database first
        if let Some(profile) = self.database.get_user_profile(fid).await? {
            return Ok(Some(profile));
        }

        // If lazy loader is available, try lazy loading
        if let Some(loader) = &self.lazy_loader {
            #[allow(clippy::cast_sign_loss)]
            match loader.fetch_user_profile(fid as u64).await {
                Ok(profile) => Ok(Some(profile)),
                Err(e) => {
                    tracing::warn!("Failed to lazy load profile {}: {}", fid, e);
                    Ok(None) // Graceful degradation
                }
            }
        } else {
            Ok(None)
        }
    }

    /// Get user casts with automatic lazy loading
    ///
    /// # Errors
    /// Returns error if database query fails
    pub async fn get_user_casts_smart(&self, fid: i64, limit: Option<i64>) -> Result<Vec<Cast>> {
        // Try database first
        let existing_casts = self.database.get_casts_by_fid(fid, limit, Some(0)).await?;

        if !existing_casts.is_empty() {
            return Ok(existing_casts);
        }

        // If lazy loader is available, try lazy loading
        if let Some(loader) = &self.lazy_loader {
            #[allow(clippy::cast_sign_loss)]
            match loader.fetch_user_casts(fid as u64).await {
                Ok(casts) => {
                    // Return limited results if requested
                    if let Some(lim) = limit {
                        Ok(casts.into_iter().take(lim as usize).collect())
                    } else {
                        Ok(casts)
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to lazy load casts for {}: {}", fid, e);
                    Ok(Vec::new())
                }
            }
        } else {
            Ok(Vec::new())
        }
    }

    /// Override sync configuration from command-line arguments
    ///
    /// # Errors
    /// Currently never returns an error, but returns `Result` for future extensibility
    /// to allow validation of shard IDs, batch sizes, and intervals
    pub fn override_sync_config(
        &mut self,
        shard_ids: Vec<u32>,
        batch_size: Option<u32>,
        interval_ms: Option<u64>,
    ) -> Result<()> {
        if !shard_ids.is_empty() {
            self.config.sync.shard_ids = shard_ids;
        }
        if let Some(batch) = batch_size {
            self.config.sync.batch_size = batch;
        }
        if let Some(interval) = interval_ms {
            self.config.sync.sync_interval_ms = interval;
        }
        Ok(())
    }

    /// Start data synchronization
    ///
    /// # Errors
    /// - Database connection errors
    /// - gRPC client creation failures (invalid endpoint, network issues)
    /// - Sync service initialization failures (lock file conflicts, invalid configuration)
    pub async fn start_sync(&mut self) -> Result<()> {
        let sync_service = Arc::new(SyncService::new(&self.config, self.database.clone()).await?);
        sync_service.start().await?;
        self.sync_service = Some(sync_service);
        Ok(())
    }

    /// Start synchronization with a specific block range
    ///
    /// # Errors
    /// - Invalid block range (from_block > to_block)
    /// - Database connection errors
    /// - gRPC client creation failures
    /// - Sync service initialization failures
    pub async fn start_sync_with_range(&mut self, from_block: u64, to_block: u64) -> Result<()> {
        self.sync_service = Some(Arc::new(
            SyncService::new(&self.config, self.database.clone()).await?,
        ));
        self.sync_service
            .as_ref()
            .unwrap()
            .start_with_range(from_block, to_block)
            .await?;
        Ok(())
    }

    /// Start sync with custom workers per shard
    ///
    /// # Errors
    /// - Invalid block range (from_block > to_block)
    /// - Invalid worker count (0 workers or exceeds system resources)
    /// - Database connection errors
    /// - gRPC client creation failures
    /// - Sync service initialization failures
    pub async fn start_sync_with_range_and_workers(
        &mut self,
        from_block: u64,
        to_block: u64,
        workers_per_shard: u32,
    ) -> Result<()> {
        self.sync_service = Some(Arc::new(
            SyncService::new(&self.config, self.database.clone()).await?,
        ));
        self.sync_service
            .as_ref()
            .unwrap()
            .start_with_range_and_workers(from_block, to_block, workers_per_shard)
            .await?;
        Ok(())
    }

    /// Stop synchronization
    ///
    /// # Errors
    /// Returns error if:
    /// - Lock file cannot be read
    /// - Process cannot be killed (permission denied)
    /// - Lock file cannot be removed
    pub async fn stop_sync(&self, force: bool) -> Result<()> {
        use crate::sync::lock_file::SyncLockManager;

        // Always use lock file approach since stop command runs in a different process
        let lock_manager = SyncLockManager::new();

        if lock_manager.lock_exists() {
            match lock_manager.read_lock() {
                Ok(lock) => {
                    let pid = lock.pid;
                    tracing::info!("Found running sync process with PID: {}", pid);

                    // Send signal to kill the process
                    #[cfg(unix)]
                    {
                        let signal = if force { 9 } else { 15 }; // SIGKILL or SIGTERM
                        tracing::info!("Sending signal {} to process {}", signal, pid);

                        let result = unsafe { libc::kill(pid.cast_signed(), signal) };

                        if result == 0 {
                            tracing::info!("✅ Signal sent successfully");
                            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

                            // Verify process is gone
                            let check = unsafe { libc::kill(pid.cast_signed(), 0) };
                            if check == 0 {
                                tracing::warn!("Process still running, sending SIGKILL");
                                unsafe {
                                    libc::kill(pid.cast_signed(), 9);
                                }
                                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                            }
                        } else {
                            let errno = std::io::Error::last_os_error();
                            tracing::warn!("Failed to send signal: {}", errno);
                        }
                    }

                    #[cfg(not(unix))]
                    {
                        tracing::warn!("Process termination not supported on this platform");
                    }

                    // Remove lock file
                    lock_manager.remove_lock()?;
                }
                Err(e) => {
                    tracing::warn!("Failed to read lock file: {}", e);
                    lock_manager.remove_lock()?;
                }
            }
        } else {
            tracing::info!("No sync process found");
        }

        Ok(())
    }

    /// Get sync status
    ///
    /// # Errors
    /// - Lock file read errors (I/O errors, corrupted lock file)
    /// - JSON deserialization errors (invalid lock file format)
    pub fn get_sync_status(&self) -> Result<Option<crate::sync::lock_file::SyncLockFile>> {
        // Always try to read from lock file first, regardless of whether this instance has a sync_service
        // This allows status commands to see sync processes started by other instances
        use crate::sync::lock_file::SyncLockManager;
        let lock_manager = SyncLockManager::new();

        if lock_manager.lock_exists() {
            match lock_manager.read_lock() {
                Ok(lock) => Ok(Some(lock)),
                Err(_) => {
                    // Fallback to sync_service if lock file read failed
                    self.sync_service
                        .as_ref()
                        .map_or_else(|| Ok(None), |sync_service| sync_service.get_sync_status())
                }
            }
        } else {
            self.sync_service
                .as_ref()
                .map_or_else(|| Ok(None), |sync_service| sync_service.get_sync_status())
        }
    }

    /// Search user profiles
    ///
    /// # Errors
    /// - Database connection errors
    /// - SQL query execution errors
    /// - Invalid search patterns (malformed regex, SQL injection attempts)
    pub async fn search_profiles(&self, query: &str) -> Result<Vec<models::UserProfile>> {
        let search_query = models::UserProfileQuery {
            fid: None,
            username: None,
            display_name: None,
            bio: None,
            location: None,
            twitter_username: None,
            github_username: None,
            limit: Some(20),
            offset: None,
            start_timestamp: None,
            end_timestamp: None,
            sort_by: Some(models::ProfileSortBy::LastUpdated),
            sort_order: Some(models::SortOrder::Desc),
            search_term: Some(query.to_string()),
        };
        self.database.list_user_profiles(search_query).await
    }

    /// Get user profile by FID
    ///
    /// # Errors
    /// - Database connection errors
    /// - SQL query execution errors
    pub async fn get_profile(&self, fid: i64) -> Result<Option<models::UserProfile>> {
        let query = models::UserProfileQuery {
            fid: Some(fid),
            username: None,
            display_name: None,
            bio: None,
            location: None,
            twitter_username: None,
            github_username: None,
            limit: Some(1),
            offset: None,
            start_timestamp: None,
            end_timestamp: None,
            sort_by: None,
            sort_order: None,
            search_term: None,
        };
        let profiles = self.database.list_user_profiles(query).await?;
        Ok(profiles.into_iter().next())
    }

    /// Get statistics
    ///
    /// # Errors
    /// - Database connection errors
    /// - SQL query execution errors (aggregation failures)
    pub async fn get_statistics(&self) -> Result<models::StatisticsResult> {
        let stats_query = models::StatisticsQuery {
            start_date: None,
            end_date: None,
            group_by: None,
        };
        self.database.get_statistics(stats_query).await
    }

    /// List casts
    ///
    /// # Errors
    /// - Database connection errors
    /// - SQL query execution errors
    /// - Invalid limit value (negative or exceeds maximum)
    pub async fn list_casts(&self, limit: Option<i64>) -> Result<Vec<models::Cast>> {
        let cast_query = models::CastQuery {
            fid: None,
            text_search: None,
            parent_hash: None,
            root_hash: None,
            has_mentions: None,
            has_embeds: None,
            start_timestamp: None,
            end_timestamp: None,
            limit,
            offset: None,
            sort_by: Some(models::CastSortBy::Timestamp),
            sort_order: Some(models::SortOrder::Desc),
        };
        self.database.list_casts(cast_query).await
    }

    /// List follows
    ///
    /// # Errors
    /// - Database connection errors
    /// - SQL query execution errors
    /// - Invalid limit value (negative or exceeds maximum)
    pub async fn list_follows(
        &self,
        fid: Option<i64>,
        limit: Option<i64>,
    ) -> Result<Vec<models::Link>> {
        let link_query = models::LinkQuery {
            fid,
            target_fid: None,
            link_type: Some("follow".to_string()),
            start_timestamp: None,
            end_timestamp: None,
            limit,
            offset: None,
            sort_by: Some(models::LinkSortBy::Timestamp),
            sort_order: Some(models::SortOrder::Desc),
        };
        self.database.list_links(link_query).await
    }

    /// Get user activity timeline
    ///
    /// # Errors
    /// - Database connection errors
    /// - SQL query execution errors
    /// - Invalid activity_type (unsupported activity types)
    /// - Invalid limit or offset values
    pub async fn get_user_activity(
        &self,
        fid: i64,
        limit: i64,
        offset: i64,
        activity_type: Option<String>,
    ) -> Result<Vec<models::UserActivityTimeline>> {
        self.database
            .get_user_activity_timeline(fid, activity_type, None, None, Some(limit), Some(offset))
            .await
    }

    /// Create a RAG service for natural language queries
    ///
    /// # Errors
    /// - Embedding service configuration errors (invalid API keys, endpoints)
    /// - LLM service configuration errors (missing or invalid LLM config)
    /// - Network connectivity issues (cannot reach embedding/LLM APIs)
    pub async fn create_rag_service(&self) -> Result<RagService> {
        RagService::new(&self.config).await
    }

    /// Create an embedding service for vector generation
    ///
    /// # Errors
    /// - Invalid embedding configuration (missing API keys, invalid endpoints)
    /// - Unsupported embedding provider
    /// - Missing required configuration fields
    pub fn create_embedding_service(&self) -> Result<Arc<EmbeddingService>> {
        Ok(Arc::new(EmbeddingService::new(&self.config)?))
    }

    /// Create an LLM service for text generation
    ///
    /// # Errors
    /// - Invalid LLM configuration (missing API keys, invalid endpoints)
    /// - Unsupported LLM provider
    /// - Missing required configuration fields
    pub fn create_llm_service(&self) -> Result<Arc<LlmService>> {
        Ok(Arc::new(LlmService::new(&self.config)?))
    }

    /// Semantic search for profiles
    ///
    /// # Errors
    /// - Embedding service errors (API failures, network issues, invalid API keys)
    /// - Database connection errors
    /// - Vector search execution errors
    /// - Invalid threshold value (not in range 0.0-1.0)
    pub async fn semantic_search_profiles(
        &self,
        query: &str,
        limit: usize,
        threshold: Option<f32>,
    ) -> Result<Vec<SearchResult>> {
        let embedding_service = self.create_embedding_service()?;
        let retriever = Retriever::new(self.database.clone(), embedding_service);
        retriever.semantic_search(query, limit, threshold).await
    }

    /// Semantic search for casts
    ///
    /// # Errors
    /// - Embedding service errors (API failures, network issues, invalid API keys)
    /// - Database connection errors
    /// - Vector search execution errors
    /// - Invalid threshold value (not in range 0.0-1.0)
    pub async fn semantic_search_casts(
        &self,
        query: &str,
        limit: usize,
        threshold: Option<f32>,
    ) -> Result<Vec<models::CastSearchResult>> {
        let embedding_service = self.create_embedding_service()?;
        let cast_retriever = CastRetriever::new(self.database.clone(), embedding_service);
        cast_retriever
            .semantic_search(query, limit, threshold)
            .await
    }

    /// Get cast thread (parent chain + root + children)
    ///
    /// # Errors
    /// - Database connection errors
    /// - SQL query execution errors
    /// - Invalid message_hash (malformed hash)
    /// - Circular reference detection errors in thread traversal
    pub async fn get_cast_thread(
        &self,
        message_hash: Vec<u8>,
        depth: usize,
    ) -> Result<database::CastThread> {
        self.database.get_cast_thread(message_hash, depth).await
    }

    /// Backfill profile embeddings
    ///
    /// # Errors
    /// - Embedding service errors (API failures, rate limits, network issues)
    /// - Database connection errors
    /// - Database update errors (constraint violations, transaction failures)
    /// - Profile text extraction errors
    pub async fn backfill_profile_embeddings(
        &self,
        limit: Option<usize>,
    ) -> Result<ProfileBackfillStats> {
        let embedding_service = self.create_embedding_service()?;
        embeddings::backfill_embeddings(self.database.clone(), embedding_service).await
    }

    /// Backfill cast embeddings
    ///
    /// # Errors
    /// - Embedding service errors (API failures, rate limits, network issues)
    /// - Database connection errors  
    /// - Database update errors (constraint violations, transaction failures)
    /// - Cast text extraction errors
    pub async fn backfill_cast_embeddings(
        &self,
        limit: Option<usize>,
    ) -> Result<CastBackfillStats> {
        let embedding_service = self.create_embedding_service()?;
        embeddings::backfill_cast_embeddings(self.database.clone(), embedding_service, limit).await
    }
}

#[cfg(test)]
mod lib_tests {
    use super::*;

    #[test]
    fn test_farcaster_epoch_constant() {
        // Verify Farcaster epoch is Jan 1, 2021
        assert_eq!(FARCASTER_EPOCH, 1_609_459_200_000);
    }

    #[test]
    fn test_farcaster_to_unix_timestamp() {
        // Epoch conversion
        assert_eq!(farcaster_to_unix_timestamp(0), 1_609_459_200);

        // 1 day after epoch
        assert_eq!(farcaster_to_unix_timestamp(86400), 1_609_459_200 + 86400);

        // Large timestamp
        assert_eq!(
            farcaster_to_unix_timestamp(100_000_000),
            1_609_459_200 + 100_000_000
        );
    }

    #[test]
    fn test_unix_to_farcaster_timestamp() {
        // Epoch conversion
        assert_eq!(unix_to_farcaster_timestamp(1_609_459_200), 0);

        // 1 day after epoch
        assert_eq!(unix_to_farcaster_timestamp(1_609_459_200 + 86400), 86400);

        // Current time (approximate)
        let current_unix = 1_700_000_000u64;
        let farcaster = unix_to_farcaster_timestamp(current_unix);
        assert!(farcaster > 90_000_000); // Should be > 90M seconds since epoch
    }

    #[test]
    fn test_timestamp_roundtrip() {
        // Test that conversions are reversible
        let original_farcaster = 50_000_000u64;
        let unix = farcaster_to_unix_timestamp(original_farcaster);
        let back_to_farcaster = unix_to_farcaster_timestamp(unix);
        assert_eq!(original_farcaster, back_to_farcaster);

        // Reverse direction
        let original_unix = 1_650_000_000u64;
        let farcaster = unix_to_farcaster_timestamp(original_unix);
        let back_to_unix = farcaster_to_unix_timestamp(farcaster);
        assert_eq!(original_unix, back_to_unix);
    }

    #[test]
    fn test_timestamp_edge_cases() {
        // Zero farcaster timestamp
        assert_eq!(farcaster_to_unix_timestamp(0), 1_609_459_200);

        // Zero would underflow for unix_to_farcaster, so test smallest valid
        assert_eq!(unix_to_farcaster_timestamp(1_609_459_200), 0);

        // Maximum reasonable timestamp (year 2100)
        let year_2100 = 4_102_444_800u64;
        let farcaster = unix_to_farcaster_timestamp(year_2100);
        let back = farcaster_to_unix_timestamp(farcaster);
        assert_eq!(back, year_2100);
    }
}
