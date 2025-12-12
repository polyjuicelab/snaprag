//! API caching layer for profile and social data
//!
//! This module provides Redis-based caching for expensive API operations
//! like profile lookups and social graph analysis, with support for
//! stale-while-revalidate and background job processing.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tracing::debug;
use tracing::info;
use tracing::warn;

use crate::api::redis_client::RedisClient;
use crate::api::types::CastStatsResponse;
use crate::api::types::ProfileResponse;
use crate::personality::MbtiProfile;
use crate::social_graph::SocialProfile;

/// Cache result with stale information
#[derive(Debug, Clone)]
pub enum CacheResult<T> {
    /// Fresh cache hit
    Fresh(T),
    /// Stale cache hit (expired but still valid for stale-while-revalidate)
    /// Data is available but may be outdated, background update is triggered
    Stale(T),
    /// Cache is updating - data is expired, returning old data with updating status
    /// Frontend should decide when to reload
    Updating(T),
    /// Cache miss
    Miss,
}

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Default TTL for profile cache entries
    pub profile_ttl: Duration,
    /// Default TTL for social analysis cache entries  
    pub social_ttl: Duration,
    /// Default TTL for MBTI analysis cache entries
    pub mbti_ttl: Duration,
    /// Default TTL for cast statistics cache entries
    pub cast_stats_ttl: Duration,
    /// Stale threshold - deprecated, no longer used (data is permanently stored in Redis)
    /// Kept for API compatibility but not used in TTL calculations
    pub stale_threshold: Duration,
    /// Enable cache statistics
    pub enable_stats: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            profile_ttl: Duration::from_secs(3600),     // 1 hour default
            social_ttl: Duration::from_secs(3600),      // 1 hour default
            mbti_ttl: Duration::from_secs(7200),        // 2 hours default (more stable)
            cast_stats_ttl: Duration::from_secs(86400), // 1 day default
            stale_threshold: Duration::from_secs(0), // Default: no stale threshold (should be set from config in production)
            enable_stats: true,
        }
    }
}

/// Cache statistics
#[derive(Debug, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub stale_hits: u64,
}

impl CacheStats {
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // Hit rate calculation - precision loss acceptable for statistics
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

/// Redis-based cache service for API responses
pub struct CacheService {
    redis: Arc<RedisClient>,
    config: CacheConfig,
    stats: Arc<RwLock<CacheStats>>,
}

impl CacheService {
    /// Create a new cache service with Redis
    #[must_use]
    pub fn new(redis: Arc<RedisClient>) -> Self {
        Self::with_config(redis, CacheConfig::default())
    }

    /// Create a new cache service with custom configuration
    #[must_use]
    pub fn with_config(redis: Arc<RedisClient>, config: CacheConfig) -> Self {
        Self {
            redis,
            config,
            stats: Arc::new(RwLock::new(CacheStats::default())),
        }
    }

    #[allow(clippy::unused_self)]
    fn cache_key(&self, prefix: &str, fid: i64) -> String {
        format!("cache:{prefix}:{fid}")
    }

    #[allow(clippy::unused_self)]
    fn timestamp_key(&self, prefix: &str, fid: i64) -> String {
        format!("cache:{prefix}:{fid}:timestamp")
    }

    /// Get cached social analysis by FID with stale-while-revalidate support
    pub async fn get_social(&self, fid: i64) -> crate::Result<CacheResult<SocialProfile>> {
        let cache_key = self.cache_key("social", fid);
        let timestamp_key = self.timestamp_key("social", fid);

        // Get cached data and timestamp
        let cached_data = self.redis.get_json(&cache_key).await?;
        let cached_timestamp = self.redis.get_json(&timestamp_key).await?;

        if let (Some(data), Some(timestamp_str)) = (cached_data, cached_timestamp) {
            // Parse timestamp
            if let Ok(timestamp) = timestamp_str.parse::<i64>() {
                let now = chrono::Utc::now().timestamp();
                let age = now - timestamp;
                let ttl_secs = self.config.social_ttl.as_secs() as i64;

                // Parse cached data
                if let Ok(social) = serde_json::from_str::<SocialProfile>(&data) {
                    if age < ttl_secs {
                        // Fresh cache hit
                        self.increment_hit().await;
                        debug!("Social cache hit (fresh) for FID {}", fid);
                        return Ok(CacheResult::Fresh(social));
                    }
                    // Expired but data still available in Redis (permanent storage) - return as Updating
                    // Data is permanently stored, expiration is determined by timestamp comparison only
                    self.increment_stale_hit().await;
                    debug!(
                        "Social cache expired (updating) for FID {}, age: {}s",
                        fid, age
                    );
                    return Ok(CacheResult::Updating(social));
                }
            }
        }

        // Cache miss
        self.increment_miss().await;
        debug!("Social cache miss for FID {}", fid);
        Ok(CacheResult::Miss)
    }

    /// Cache a social analysis response
    /// Data is stored permanently in Redis (no TTL), expiration is determined by timestamp comparison
    pub async fn set_social(&self, fid: i64, social: &SocialProfile) -> crate::Result<()> {
        let cache_key = self.cache_key("social", fid);
        let timestamp_key = self.timestamp_key("social", fid);

        let json_data = serde_json::to_string(social).map_err(|e| {
            crate::SnapRagError::Custom(format!("Failed to serialize social profile: {e}"))
        })?;
        let timestamp = chrono::Utc::now().timestamp().to_string();

        // Set cache permanently (no TTL) - data will only be updated, never deleted
        self.redis.set_json(&cache_key, &json_data).await?;
        self.redis.set_json(&timestamp_key, &timestamp).await?;

        debug!("Cached social analysis for FID {} (permanent storage)", fid);
        Ok(())
    }

    /// Get cached profile by FID
    pub async fn get_profile(&self, fid: i64) -> crate::Result<CacheResult<ProfileResponse>> {
        let cache_key = self.cache_key("profile", fid);
        let timestamp_key = self.timestamp_key("profile", fid);

        let cached_data = self.redis.get_json(&cache_key).await?;
        let cached_timestamp = self.redis.get_json(&timestamp_key).await?;

        if let (Some(data), Some(timestamp_str)) = (cached_data, cached_timestamp) {
            if let Ok(timestamp) = timestamp_str.parse::<i64>() {
                let now = chrono::Utc::now().timestamp();
                let age = now - timestamp;
                let ttl_secs = self.config.profile_ttl.as_secs() as i64;

                if let Ok(profile) = serde_json::from_str::<ProfileResponse>(&data) {
                    if age < ttl_secs {
                        self.increment_hit().await;
                        return Ok(CacheResult::Fresh(profile));
                    }
                    // Expired but data still available in Redis (permanent storage) - return as Updating
                    // Data is permanently stored, expiration is determined by timestamp comparison only
                    self.increment_stale_hit().await;
                    debug!(
                        "Profile cache expired (updating) for FID {}, age: {}s",
                        fid, age
                    );
                    return Ok(CacheResult::Updating(profile));
                }
            }
        }

        self.increment_miss().await;
        Ok(CacheResult::Miss)
    }

    /// Cache a profile response
    /// Data is stored permanently in Redis (no TTL), expiration is determined by timestamp comparison
    pub async fn set_profile(&self, fid: i64, profile: &ProfileResponse) -> crate::Result<()> {
        let cache_key = self.cache_key("profile", fid);
        let timestamp_key = self.timestamp_key("profile", fid);

        let json_data = serde_json::to_string(profile).map_err(|e| {
            crate::SnapRagError::Custom(format!("Failed to serialize profile: {e}"))
        })?;
        let timestamp = chrono::Utc::now().timestamp().to_string();

        // Set cache permanently (no TTL) - data will only be updated, never deleted
        self.redis.set_json(&cache_key, &json_data).await?;
        self.redis.set_json(&timestamp_key, &timestamp).await?;

        debug!("Cached profile for FID {} (permanent storage)", fid);
        Ok(())
    }

    /// Get cached MBTI analysis by FID
    pub async fn get_mbti(&self, fid: i64) -> crate::Result<CacheResult<MbtiProfile>> {
        let cache_key = self.cache_key("mbti", fid);
        let timestamp_key = self.timestamp_key("mbti", fid);

        let cached_data = self.redis.get_json(&cache_key).await?;
        let cached_timestamp = self.redis.get_json(&timestamp_key).await?;

        if let (Some(data), Some(timestamp_str)) = (cached_data, cached_timestamp) {
            if let Ok(timestamp) = timestamp_str.parse::<i64>() {
                let now = chrono::Utc::now().timestamp();
                let age = now - timestamp;
                let ttl_secs = self.config.mbti_ttl.as_secs() as i64;

                if let Ok(mbti) = serde_json::from_str::<MbtiProfile>(&data) {
                    if age < ttl_secs {
                        self.increment_hit().await;
                        return Ok(CacheResult::Fresh(mbti));
                    }
                    // Expired but data still available in Redis (permanent storage) - return as Updating
                    // Data is permanently stored, expiration is determined by timestamp comparison only
                    self.increment_stale_hit().await;
                    debug!(
                        "MBTI cache expired (updating) for FID {}, age: {}s",
                        fid, age
                    );
                    return Ok(CacheResult::Updating(mbti));
                }
            }
        }

        self.increment_miss().await;
        Ok(CacheResult::Miss)
    }

    /// Cache an MBTI analysis response
    /// Data is stored permanently in Redis (no TTL), expiration is determined by timestamp comparison
    pub async fn set_mbti(&self, fid: i64, mbti: &MbtiProfile) -> crate::Result<()> {
        let cache_key = self.cache_key("mbti", fid);
        let timestamp_key = self.timestamp_key("mbti", fid);

        let json_data = serde_json::to_string(mbti).map_err(|e| {
            crate::SnapRagError::Custom(format!("Failed to serialize MBTI profile: {e}"))
        })?;
        let timestamp = chrono::Utc::now().timestamp().to_string();

        // Set cache permanently (no TTL) - data will only be updated, never deleted
        self.redis.set_json(&cache_key, &json_data).await?;
        self.redis.set_json(&timestamp_key, &timestamp).await?;

        debug!("Cached MBTI analysis for FID {} (permanent storage)", fid);
        Ok(())
    }

    /// Invalidate (delete) cached social analysis for a FID
    pub async fn invalidate_social(&self, fid: i64) -> crate::Result<()> {
        let cache_key = self.cache_key("social", fid);
        let timestamp_key = self.timestamp_key("social", fid);

        self.redis.delete(&cache_key).await?;
        self.redis.delete(&timestamp_key).await?;
        info!("Deleted social cache for FID {}", fid);
        Ok(())
    }

    /// Invalidate (delete) cached profile for a FID
    pub async fn invalidate_profile(&self, fid: i64) -> crate::Result<()> {
        let cache_key = self.cache_key("profile", fid);
        let timestamp_key = self.timestamp_key("profile", fid);

        self.redis.delete(&cache_key).await?;
        self.redis.delete(&timestamp_key).await?;
        info!("Deleted profile cache for FID {}", fid);
        Ok(())
    }

    /// Invalidate (delete) cached MBTI analysis for a FID
    pub async fn invalidate_mbti(&self, fid: i64) -> crate::Result<()> {
        let cache_key = self.cache_key("mbti", fid);
        let timestamp_key = self.timestamp_key("mbti", fid);

        self.redis.delete(&cache_key).await?;
        self.redis.delete(&timestamp_key).await?;
        info!("Deleted MBTI cache for FID {}", fid);
        Ok(())
    }

    /// Get cached cast statistics by FID (with optional date range)
    pub async fn get_cast_stats_with_range(
        &self,
        fid: i64,
        date_range: Option<&str>,
    ) -> crate::Result<CacheResult<CastStatsResponse>> {
        // Create cache key with date range if specified
        let cache_key = if let Some(range) = date_range {
            format!("cache:cast_stats:{}:{}", fid, range)
        } else {
            self.cache_key("cast_stats", fid)
        };
        let timestamp_key = if let Some(range) = date_range {
            format!("cache:cast_stats:{}:{}:timestamp", fid, range)
        } else {
            self.timestamp_key("cast_stats", fid)
        };

        let cached_data = self.redis.get_json(&cache_key).await?;
        let cached_timestamp = self.redis.get_json(&timestamp_key).await?;

        if let (Some(data), Some(timestamp_str)) = (cached_data, cached_timestamp) {
            if let Ok(timestamp) = timestamp_str.parse::<i64>() {
                let now = chrono::Utc::now().timestamp();
                let age = now - timestamp;
                let ttl_secs = self.config.cast_stats_ttl.as_secs() as i64;

                if let Ok(stats) = serde_json::from_str::<CastStatsResponse>(&data) {
                    if age < ttl_secs {
                        self.increment_hit().await;
                        return Ok(CacheResult::Fresh(stats));
                    }
                    // Expired but data still available in Redis (permanent storage) - return as Updating
                    self.increment_stale_hit().await;
                    debug!(
                        "Cast stats cache expired (updating) for FID {}, age: {}s",
                        fid, age
                    );
                    return Ok(CacheResult::Updating(stats));
                }
            }
        }

        self.increment_miss().await;
        Ok(CacheResult::Miss)
    }

    /// Get cached cast statistics by FID (backward compatibility)
    pub async fn get_cast_stats(&self, fid: i64) -> crate::Result<CacheResult<CastStatsResponse>> {
        self.get_cast_stats_with_range(fid, None).await
    }

    /// Cache a cast statistics response (with optional date range)
    /// Data is stored permanently in Redis (no TTL), expiration is determined by timestamp comparison
    pub async fn set_cast_stats_with_range(
        &self,
        fid: i64,
        date_range: Option<&str>,
        stats: &CastStatsResponse,
    ) -> crate::Result<()> {
        // Create cache key with date range if specified
        let cache_key = if let Some(range) = date_range {
            format!("cache:cast_stats:{}:{}", fid, range)
        } else {
            self.cache_key("cast_stats", fid)
        };
        let timestamp_key = if let Some(range) = date_range {
            format!("cache:cast_stats:{}:{}:timestamp", fid, range)
        } else {
            self.timestamp_key("cast_stats", fid)
        };

        let json_data = serde_json::to_string(stats).map_err(|e| {
            crate::SnapRagError::Custom(format!("Failed to serialize cast stats: {e}"))
        })?;
        let timestamp = chrono::Utc::now().timestamp().to_string();

        // Set cache permanently (no TTL) - data will only be updated, never deleted
        self.redis.set_json(&cache_key, &json_data).await?;
        self.redis.set_json(&timestamp_key, &timestamp).await?;

        debug!(
            "Cached cast statistics for FID {} (range: {:?}, permanent storage)",
            fid, date_range
        );
        Ok(())
    }

    /// Cache a cast statistics response (backward compatibility)
    pub async fn set_cast_stats(&self, fid: i64, stats: &CastStatsResponse) -> crate::Result<()> {
        self.set_cast_stats_with_range(fid, None, stats).await
    }

    /// Invalidate (delete) cached cast statistics for a FID
    pub async fn invalidate_cast_stats(&self, fid: i64) -> crate::Result<()> {
        let cache_key = self.cache_key("cast_stats", fid);
        let timestamp_key = self.timestamp_key("cast_stats", fid);

        self.redis.delete(&cache_key).await?;
        self.redis.delete(&timestamp_key).await?;
        info!("Deleted cast stats cache for FID {}", fid);
        Ok(())
    }

    /// Invalidate (delete) all caches for a FID
    pub async fn invalidate_user(&self, fid: i64) -> crate::Result<()> {
        self.invalidate_profile(fid).await?;
        self.invalidate_social(fid).await?;
        self.invalidate_mbti(fid).await?;
        self.invalidate_cast_stats(fid).await?;
        info!("Deleted all caches for FID {}", fid);
        Ok(())
    }

    /// Delete all cache entries (profile, social, mbti)
    pub async fn invalidate_all(&self) -> crate::Result<u64> {
        let mut total_deleted = 0u64;

        // Delete all cache keys matching patterns
        let patterns = [
            "cache:profile:*",
            "cache:social:*",
            "cache:mbti:*",
            "cache:cast_stats:*",
            "cache:*:timestamp",
            "GET:/api/*",
        ];

        for pattern in &patterns {
            let deleted = self.redis.delete_pattern(pattern).await?;
            total_deleted += deleted;
            info!("Deleted {} keys matching pattern: {}", deleted, pattern);
        }

        info!("Deleted total {} cache entries", total_deleted);
        Ok(total_deleted)
    }

    /// Get cache statistics
    pub async fn get_stats(&self) -> CacheStats {
        let stats = self.stats.read().await;
        CacheStats {
            hits: stats.hits,
            misses: stats.misses,
            stale_hits: stats.stale_hits,
        }
    }

    /// Get cache information (simplified for Redis)
    pub fn get_cache_info(&self) -> CacheInfo {
        // Redis doesn't provide easy entry counts, so we return empty info
        CacheInfo { total_entries: 0 }
    }

    // Private helper methods

    async fn increment_hit(&self) {
        if self.config.enable_stats {
            let mut stats = self.stats.write().await;
            stats.hits += 1;
        }
    }

    async fn increment_miss(&self) {
        if self.config.enable_stats {
            let mut stats = self.stats.write().await;
            stats.misses += 1;
        }
    }

    async fn increment_stale_hit(&self) {
        if self.config.enable_stats {
            let mut stats = self.stats.write().await;
            stats.stale_hits += 1;
        }
    }
}

/// Cache information for monitoring (simplified for Redis)
#[derive(Debug)]
pub struct CacheInfo {
    pub total_entries: usize,
}

impl CacheInfo {
    #[must_use]
    pub fn usage_percentage(&self) -> f64 {
        0.0 // Redis doesn't have a fixed max, so we return 0
    }
}
