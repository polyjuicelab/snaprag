//! Lazy loading service for on-demand data fetching
//!
//! This module provides functionality to fetch user data on-demand from Snapchain
//! when queried, rather than waiting for full synchronization to complete.

use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tracing::debug;
use tracing::info;
use tracing::warn;

use crate::database::Database;
use crate::errors::Result;
use crate::models::Cast;
use crate::models::UserProfile;
use crate::sync::client::SnapchainClient;

/// Cache for tracking lazy loading operations
pub struct LazyLoadCache {
    /// FIDs currently being loaded
    loading: Arc<Mutex<HashSet<u64>>>,
    /// Recently loaded FIDs with timestamp
    recent: Arc<RwLock<HashMap<u64, Instant>>>,
    /// Rate limiter
    rate_limiter: Arc<Mutex<VecDeque<Instant>>>,
    /// Cache TTL (time to live)
    cache_ttl: Duration,
    /// Max requests per minute
    max_requests_per_minute: usize,
}

impl LazyLoadCache {
    #[must_use]
    pub fn new() -> Self {
        Self {
            loading: Arc::new(Mutex::new(HashSet::new())),
            recent: Arc::new(RwLock::new(HashMap::new())),
            rate_limiter: Arc::new(Mutex::new(VecDeque::new())),
            cache_ttl: Duration::from_secs(300), // 5 minutes
            max_requests_per_minute: 100,
        }
    }

    /// Check if FID should be loaded
    pub async fn should_load(&self, fid: u64) -> bool {
        // Check if already loading
        if self.loading.lock().await.contains(&fid) {
            debug!("FID {} is already being loaded", fid);
            return false;
        }

        // Check if recently loaded
        if let Some(last_time) = self.recent.read().await.get(&fid) {
            if last_time.elapsed() < self.cache_ttl {
                debug!(
                    "FID {} was loaded {} seconds ago",
                    fid,
                    last_time.elapsed().as_secs()
                );
                return false;
            }
        }

        // Check rate limit
        if !self.check_rate_limit().await {
            warn!("Rate limit exceeded for lazy loading");
            return false;
        }

        true
    }

    /// Mark FID as loading
    pub async fn mark_loading(&self, fid: u64) {
        self.loading.lock().await.insert(fid);
    }

    /// Mark FID as loaded
    pub async fn mark_loaded(&self, fid: u64) {
        self.loading.lock().await.remove(&fid);
        self.recent.write().await.insert(fid, Instant::now());
    }

    /// Check rate limit
    async fn check_rate_limit(&self) -> bool {
        let mut reqs = self.rate_limiter.lock().await;

        // Clean up old requests (older than 1 minute)
        let cutoff = Instant::now().checked_sub(Duration::from_secs(60)).unwrap();
        while reqs.front().is_some_and(|&t| t < cutoff) {
            reqs.pop_front();
        }

        // Check if we're over the limit
        if reqs.len() >= self.max_requests_per_minute {
            return false;
        }

        // Add current request
        reqs.push_back(Instant::now());
        true
    }
}

impl Default for LazyLoadCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Lazy loading service
pub struct LazyLoader {
    database: Arc<Database>,
    snapchain_client: Arc<SnapchainClient>,
    cache: LazyLoadCache,
}

impl LazyLoader {
    pub fn new(database: Arc<Database>, snapchain_client: Arc<SnapchainClient>) -> Self {
        Self {
            database,
            snapchain_client,
            cache: LazyLoadCache::new(),
        }
    }

    /// Get the snapchain client for direct API access
    pub fn client(&self) -> &SnapchainClient {
        &self.snapchain_client
    }

    /// Fetch user profile and casts on demand
    pub async fn fetch_user_complete(&self, fid: u64) -> Result<(UserProfile, Vec<Cast>)> {
        // Check if should load
        if !self.cache.should_load(fid).await {
            return Err(crate::errors::SnapRagError::Custom(format!(
                "FID {fid} is being loaded or was recently loaded"
            )));
        }

        self.cache.mark_loading(fid).await;

        let result = async {
            info!("🔄 Lazy loading FID {} (profile + casts)", fid);

            // 1. Fetch and store profile
            let profile = self.fetch_user_profile(fid).await?;

            // 2. Fetch and store casts
            let casts = self.fetch_user_casts(fid).await?;

            info!(
                "✅ Successfully lazy loaded FID {}: profile + {} casts",
                fid,
                casts.len()
            );

            Ok((profile, casts))
        }
        .await;

        self.cache.mark_loaded(fid).await;
        result
    }

    /// Fetch user profile only
    pub async fn fetch_user_profile(&self, fid: u64) -> Result<UserProfile> {
        // Check if already exists
        #[allow(clippy::cast_possible_wrap)] // FID values in Farcaster never exceed i64::MAX
        if let Some(existing) = self.database.get_user_profile(fid as i64).await? {
            info!("✓ Profile {} found in database (cached)", fid);
            return Ok(existing);
        }

        info!("⚡ Profile {} not found, fetching from Snapchain...", fid);

        // Fetch from Snapchain
        let user_data_response = self
            .snapchain_client
            .get_user_data_by_fid(fid, None)
            .await?;

        if user_data_response.messages.is_empty() {
            return Err(crate::errors::SnapRagError::Custom(format!(
                "User {fid} not found on Snapchain"
            )));
        }

        // Build profile from messages
        #[allow(clippy::cast_possible_wrap)] // FID values in Farcaster never exceed i64::MAX
        let mut profile = UserProfile {
            fid: fid as i64,
            username: None,
            display_name: None,
            bio: None,
            pfp_url: None,
            banner_url: None,
            location: None,
            website_url: None,
            twitter_username: None,
            github_username: None,
            primary_address_ethereum: None,
            primary_address_solana: None,
            profile_token: None,
            profile_embedding: None,
            bio_embedding: None,
            interests_embedding: None,
            last_updated_timestamp: 0,
            last_updated_at: chrono::Utc::now(),
            id: uuid::Uuid::new_v4(),
            shard_id: None,
            block_height: None,
            transaction_fid: None,
        };

        // Parse messages and extract latest values
        for message in user_data_response.messages {
            if let Some(data) = &message.data {
                let body = &data.body;
                if let Some(user_data_body) = body.get("user_data_body") {
                    let data_type = user_data_body
                        .get("type")
                        .and_then(serde_json::Value::as_i64)
                        .unwrap_or(0);
                    let value = user_data_body
                        .get("value")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    // Update profile fields based on data type
                    match data_type {
                        1 => profile.pfp_url = Some(value),
                        2 => profile.display_name = Some(value),
                        3 => profile.bio = Some(value),
                        5 => profile.website_url = Some(value),
                        6 => profile.username = Some(value),
                        7 => profile.location = Some(value),
                        8 => profile.twitter_username = Some(value),
                        9 => profile.github_username = Some(value),
                        10 => profile.banner_url = Some(value),
                        _ => {}
                    }

                    // Update timestamp to latest
                    #[allow(clippy::cast_possible_wrap)]
                    // Farcaster timestamps never exceed i64::MAX
                    if data.timestamp as i64 > profile.last_updated_timestamp {
                        profile.last_updated_timestamp = data.timestamp as i64;
                    }
                }
            }
        }

        // Save to database (upsert - safe for concurrent access)
        self.database.upsert_user_profile(&profile).await?;

        info!("✅ Saved profile {} to database", fid);
        Ok(profile)
    }

    /// Fetch user casts on demand
    pub async fn fetch_user_casts(&self, fid: u64) -> Result<Vec<Cast>> {
        // Check if casts exist in database first
        let existing = self
            .database
            .get_casts_by_fid(i64::try_from(fid).unwrap_or(i64::MAX), Some(10), Some(0))
            .await?;
        if !existing.is_empty() {
            info!("✓ Casts for FID {} found in database, fetching all...", fid);
            // Has some casts, get all from database
            return self
                .database
                .get_casts_by_fid(i64::try_from(fid).unwrap_or(i64::MAX), Some(10000), Some(0))
                .await;
        }

        info!(
            "⚡ Casts for FID {} not found, fetching from Snapchain...",
            fid
        );

        let mut all_casts = Vec::new();
        let mut page_token: Option<String> = None;
        let max_pages = 100; // Increased limit to fetch more casts (was 10, now 100)
        let mut page_count = 0;

        loop {
            // Fetch casts page
            let casts_response = self
                .snapchain_client
                .get_casts_by_fid(fid, None, page_token.as_deref())
                .await?;

            if casts_response.messages.is_empty() {
                break;
            }

            info!(
                "📄 Page {}: fetched {} casts for FID {}",
                page_count + 1,
                casts_response.messages.len(),
                fid
            );

            // Parse and save casts
            for message in casts_response.messages {
                if let Some(cast) = self.parse_cast_message(&message) {
                    // Save cast to database (upsert)
                    if let Err(e) = self
                        .database
                        .upsert_cast(
                            cast.fid,
                            cast.text.clone(),
                            cast.timestamp,
                            cast.message_hash.clone(),
                            cast.parent_hash.clone(),
                            cast.root_hash.clone(),
                            cast.embeds.clone(),
                            cast.mentions.clone(),
                        )
                        .await
                    {
                        warn!("Failed to save cast: {}", e);
                    } else {
                        all_casts.push(cast);
                    }
                }
            }

            // Check for next page
            page_token = casts_response.next_page_token;
            page_count += 1;

            if page_token.is_none() || page_count >= max_pages {
                break;
            }
        }

        info!("✅ Saved {} casts for FID {}", all_casts.len(), fid);
        Ok(all_casts)
    }

    /// Parse cast message from Snapchain response
    fn parse_cast_message(&self, message: &crate::sync::client::FarcasterMessage) -> Option<Cast> {
        let Some(ref data) = message.data else {
            return None;
        };

        let fid = data.fid as i64;
        let timestamp = data.timestamp as i64;
        let message_hash = hex::decode(&message.hash).unwrap_or_default();

        // Extract cast body
        let body = &data.body;

        let text = body
            .get("castAddBody")
            .and_then(|cast_body| cast_body.get("text"))
            .and_then(|v| v.as_str())
            .map(std::string::ToString::to_string);

        let parent_hash = body
            .get("castAddBody")
            .and_then(|cast_body| cast_body.get("parentCastId"))
            .and_then(|parent| parent.get("hash"))
            .and_then(|v| v.as_str())
            .and_then(|s| hex::decode(s).ok());

        let root_hash = parent_hash.clone(); // Simplified - could parse full parent chain

        let embeds = body
            .get("castAddBody")
            .and_then(|cast_body| cast_body.get("embeds"))
            .and_then(|v| serde_json::to_value(v).ok());

        let mentions = body
            .get("castAddBody")
            .and_then(|cast_body| cast_body.get("mentions"))
            .and_then(|v| serde_json::to_value(v).ok());

        Some(Cast {
            id: uuid::Uuid::new_v4(),
            fid,
            text,
            timestamp,
            message_hash,
            parent_hash,
            root_hash,
            embeds,
            mentions,
            shard_id: None,
            block_height: None,
            transaction_fid: None,
            created_at: chrono::Utc::now(),
        })
    }

    /// Get or fetch user profile (smart query)
    pub async fn get_user_profile_smart(&self, fid: i64) -> Result<Option<UserProfile>> {
        // Try database first
        if let Some(profile) = self.database.get_user_profile(fid).await? {
            return Ok(Some(profile));
        }

        // Not found? Try lazy loading
        info!("Profile {} not in database, attempting lazy load", fid);

        match self.fetch_user_profile(fid as u64).await {
            Ok(profile) => Ok(Some(profile)),
            Err(e) => {
                warn!("Failed to lazy load profile {}: {}", fid, e);
                Ok(None) // Graceful degradation
            }
        }
    }

    /// Get or fetch user casts (smart query)
    pub async fn get_user_casts_smart(&self, fid: i64) -> Result<Vec<Cast>> {
        self.get_user_casts_smart_with_limit(fid, None).await
    }

    /// Get or fetch user casts with optional limit (smart query)
    pub async fn get_user_casts_smart_with_limit(
        &self,
        fid: i64,
        limit: Option<usize>,
    ) -> Result<Vec<Cast>> {
        // Convert limit to Option<i64> for database query
        // None means no limit (get all casts)
        let db_limit = limit.map(|l| l as i64);

        // Try database first
        let existing_casts = self
            .database
            .get_casts_by_fid(fid, db_limit, Some(0))
            .await?;

        if !existing_casts.is_empty() {
            return Ok(existing_casts);
        }

        // Not found? Try lazy loading
        info!(
            "Casts for FID {} not in database, attempting lazy load",
            fid
        );

        #[allow(clippy::cast_sign_loss)]
        match self.fetch_user_casts(fid as u64).await {
            Ok(mut casts) => {
                // Apply limit if specified
                if let Some(max) = limit {
                    casts.truncate(max);
                }
                Ok(casts)
            }
            Err(e) => {
                warn!("Failed to lazy load casts for {}: {}", fid, e);
                Ok(Vec::new()) // Return empty rather than error
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_should_load() {
        let cache = LazyLoadCache::new();

        // First time should load
        assert!(cache.should_load(123).await);

        // Mark as loading
        cache.mark_loading(123).await;

        // Should not load while loading
        assert!(!cache.should_load(123).await);

        // Mark as loaded
        cache.mark_loaded(123).await;

        // Should not load immediately after
        assert!(!cache.should_load(123).await);
    }
}
