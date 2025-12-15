//! Social graph analysis for user profiling
//!
//! This module analyzes user relationships and interactions to build
//! a comprehensive social profile for better AI understanding.

#![allow(clippy::cast_precision_loss)] // Acceptable for network analysis and statistics

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;

use lazy_static::lazy_static;
use serde::Deserialize;
use serde::Serialize;

use crate::database::Database;
use crate::Result;

lazy_static! {
    /// Stop words using the stop-words crate
    static ref STOP_WORDS_SET: HashSet<String> = {
        let stop_words = stop_words::get(stop_words::LANGUAGE::English);
        stop_words.into_iter().collect()
    };
}

/// Social graph profile for a user
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialProfile {
    pub fid: i64,

    // Following/Follower stats
    pub following_count: usize,
    pub followers_count: usize,
    pub influence_score: f32, // followers / following ratio

    // Network analysis
    pub top_followed_users: Vec<UserMention>,
    pub top_followers: Vec<UserMention>,
    pub most_mentioned_users: Vec<UserMention>,

    // Social circle categorization
    pub social_circles: SocialCircles,

    // Interaction patterns
    pub interaction_style: InteractionStyle,

    // Word cloud - vocabulary analysis
    pub word_cloud: WordCloud,
}

/// User mention with context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMention {
    pub fid: i64,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub count: usize,
    pub category: String, // "tech", "creator", "web3", etc.
}

/// Social circles breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialCircles {
    pub tech_builders: f32,    // % following tech people
    pub content_creators: f32, // % following creators
    pub web3_natives: f32,     // % following web3 people
    pub casual_users: f32,     // % following casual users
}

/// Interaction style analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionStyle {
    pub reply_frequency: f32,    // How often user replies to others
    pub mention_frequency: f32,  // How often user mentions others
    pub network_connector: bool, // Actively introduces people
    pub community_role: String,  // "leader", "contributor", "observer"
}

/// Word cloud data - most frequently used words/phrases
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordCloud {
    pub top_words: Vec<WordFrequency>,
    pub top_phrases: Vec<WordFrequency>,
    pub signature_words: Vec<String>, // Unique characteristic words
}

/// Word frequency entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordFrequency {
    pub word: String,
    pub count: usize,
    pub percentage: f32,
}

/// Social graph analyzer
pub struct SocialGraphAnalyzer {
    database: Arc<Database>,
    snapchain_client: Option<Arc<crate::sync::client::SnapchainClient>>,
}

impl SocialGraphAnalyzer {
    /// Create a new social graph analyzer
    #[must_use]
    pub const fn new(database: Arc<Database>) -> Self {
        Self {
            database,
            snapchain_client: None,
        }
    }

    /// Create with Snapchain client for lazy loading
    pub const fn with_snapchain(
        database: Arc<Database>,
        client: Arc<crate::sync::client::SnapchainClient>,
    ) -> Self {
        Self {
            database,
            snapchain_client: Some(client),
        }
    }

    /// Analyze user's social profile
    pub async fn analyze_user(&self, fid: i64) -> Result<SocialProfile> {
        let start_time = std::time::Instant::now();

        // Get following list
        let step_start = std::time::Instant::now();
        let following = self.get_following(fid).await?;
        tracing::debug!("get_following took {}ms", step_start.elapsed().as_millis());

        let step_start = std::time::Instant::now();
        let followers = self.get_followers(fid).await?;
        tracing::debug!("get_followers took {}ms", step_start.elapsed().as_millis());

        // Calculate influence score
        let influence_score = if following.is_empty() {
            0.0
        } else {
            followers.len() as f32 / following.len() as f32
        };

        // Analyze mentions from user's casts (this works even without links data)
        let step_start = std::time::Instant::now();
        let mentioned_users = self.analyze_mentions(fid).await?;
        tracing::debug!(
            "analyze_mentions took {}ms",
            step_start.elapsed().as_millis()
        );

        // If we have mentioned users, try to categorize them as a proxy for social circles
        let social_circles = if !mentioned_users.is_empty() {
            self.categorize_from_mentions(&mentioned_users)
        } else if !following.is_empty() {
            self.categorize_social_circles(&following).await?
        } else {
            SocialCircles {
                tech_builders: 0.0,
                content_creators: 0.0,
                web3_natives: 0.0,
                casual_users: 0.0,
            }
        };

        // Analyze interaction patterns
        let interaction_style = self.analyze_interaction_style(fid).await?;

        // Get top users in each category
        let step_start = std::time::Instant::now();
        let top_followed = self.get_top_users(&following, 5).await?;
        tracing::debug!(
            "get_top_users (following) took {}ms",
            step_start.elapsed().as_millis()
        );

        let step_start = std::time::Instant::now();
        let top_followers = self.get_top_users(&followers, 5).await?;
        tracing::debug!(
            "get_top_users (followers) took {}ms",
            step_start.elapsed().as_millis()
        );

        // Generate word cloud from user's casts
        let step_start = std::time::Instant::now();
        let word_cloud = self.generate_word_cloud(fid).await?;
        tracing::debug!(
            "generate_word_cloud took {}ms",
            step_start.elapsed().as_millis()
        );

        tracing::info!(
            "analyze_user total time: {}ms",
            start_time.elapsed().as_millis()
        );

        Ok(SocialProfile {
            fid,
            following_count: following.len(),
            followers_count: followers.len(),
            influence_score,
            top_followed_users: top_followed,
            top_followers,
            most_mentioned_users: mentioned_users,
            social_circles,
            interaction_style,
            word_cloud,
        })
    }

    /// Get list of users this FID follows (with lazy loading from Snapchain)
    async fn get_following(&self, fid: i64) -> Result<Vec<i64>> {
        // Try database first - only get active links (using window function)
        let links = sqlx::query_scalar::<_, i64>(
            r"
            WITH latest_events AS (
                SELECT *, ROW_NUMBER() OVER (
                    PARTITION BY fid, target_fid 
                    ORDER BY timestamp DESC
                ) as rn
                FROM links
                WHERE fid = $1 AND link_type = 'follow'
            )
            SELECT target_fid FROM latest_events WHERE rn = 1 AND event_type = 'add'
            ",
        )
        .bind(fid)
        .fetch_all(self.database.pool())
        .await?;

        // Check if we need to lazy load
        let should_lazy_load = if links.is_empty() {
            // No data at all - definitely need to load
            true
        } else if self.snapchain_client.is_some() {
            // Has some data - check if it looks incomplete
            // If count is exactly 1000 or 2000, it might be truncated from a previous run
            let count = links.len();
            let looks_truncated = count == 1000 || count == 2000;

            if looks_truncated {
                tracing::debug!(
                    "🔍 Found exactly {} following for FID {} - checking if complete...",
                    count,
                    fid
                );

                // Check if there's a marker indicating this is complete
                let is_marked_complete = sqlx::query_scalar::<_, bool>(
                    "SELECT EXISTS(
                        SELECT 1 FROM links 
                        WHERE fid = $1 
                        AND link_type = 'follow_complete_marker'
                    )",
                )
                .bind(fid)
                .fetch_one(self.database.pool())
                .await
                .unwrap_or(false);

                !is_marked_complete
            } else {
                false
            }
        } else {
            false
        };

        if should_lazy_load && self.snapchain_client.is_some() {
            tracing::info!(
                "⚡ Following list incomplete/empty for FID {}, lazy loading from Snapchain...",
                fid
            );
            return self.lazy_load_following(fid).await;
        }

        Ok(links)
    }

    /// Count following for a user (optimized - only returns count, not data)
    pub async fn count_following(&self, fid: i64) -> Result<usize> {
        let count = sqlx::query_scalar::<_, i64>(
            r"
            WITH latest_events AS (
                SELECT *, ROW_NUMBER() OVER (
                    PARTITION BY fid, target_fid 
                    ORDER BY timestamp DESC
                ) as rn
                FROM links
                WHERE fid = $1 AND link_type = 'follow'
            )
            SELECT COUNT(*) FROM latest_events WHERE rn = 1 AND event_type = 'add'
            ",
        )
        .bind(fid)
        .fetch_one(self.database.pool())
        .await?;

        Ok(count as usize)
    }

    /// Get list of users who follow this FID (with lazy loading from Snapchain)
    async fn get_followers(&self, fid: i64) -> Result<Vec<i64>> {
        // Try database first - only get active followers (using window function)
        let followers = sqlx::query_scalar::<_, i64>(
            r"
            WITH latest_events AS (
                SELECT *, ROW_NUMBER() OVER (
                    PARTITION BY fid, target_fid 
                    ORDER BY timestamp DESC
                ) as rn
                FROM links
                WHERE target_fid = $1 AND link_type = 'follow'
            )
            SELECT fid FROM latest_events WHERE rn = 1 AND event_type = 'add'
            ",
        )
        .bind(fid)
        .fetch_all(self.database.pool())
        .await?;

        // Check if we need to lazy load
        let should_lazy_load = if followers.is_empty() {
            // No data at all - definitely need to load
            true
        } else if self.snapchain_client.is_some() {
            // Has some data - check if it looks incomplete
            let count = followers.len();

            // If count is suspiciously low (< 100) for a well-known user, might be incomplete
            // Or if exactly 1000/2000, might be truncated
            let looks_incomplete = count < 100 || count == 1000 || count == 2000;

            if looks_incomplete {
                tracing::info!(
                    "🔍 Found {} followers for FID {} - seems incomplete, will lazy load",
                    count,
                    fid
                );
                true
            } else {
                false
            }
        } else {
            false
        };

        if should_lazy_load && self.snapchain_client.is_some() {
            tracing::info!(
                "⚡ Followers list incomplete/empty for FID {}, lazy loading from Snapchain...",
                fid
            );
            return self.lazy_load_followers(fid).await;
        }

        Ok(followers)
    }

    /// Count followers for a user (optimized - only returns count, not data)
    pub async fn count_followers(&self, fid: i64) -> Result<usize> {
        let count = sqlx::query_scalar::<_, i64>(
            r"
            WITH latest_events AS (
                SELECT *, ROW_NUMBER() OVER (
                    PARTITION BY fid, target_fid 
                    ORDER BY timestamp DESC
                ) as rn
                FROM links
                WHERE target_fid = $1 AND link_type = 'follow'
            )
            SELECT COUNT(*) FROM latest_events WHERE rn = 1 AND event_type = 'add'
            ",
        )
        .bind(fid)
        .fetch_one(self.database.pool())
        .await?;

        Ok(count as usize)
    }

    /// Lazy load following list from Snapchain and insert into database
    async fn lazy_load_following(&self, fid: i64) -> Result<Vec<i64>> {
        let client = self.snapchain_client.as_ref().ok_or_else(|| {
            crate::SnapRagError::Custom("Snapchain client not available".to_string())
        })?;

        // Step 1: 先获取数据库中已有的 message_hash，避免重复获取
        let existing_hashes: std::collections::HashSet<Vec<u8>> = sqlx::query_scalar::<_, Vec<u8>>(
            r"
            WITH latest_events AS (
                SELECT *, ROW_NUMBER() OVER (
                    PARTITION BY fid, target_fid 
                    ORDER BY timestamp DESC
                ) as rn
                FROM links
                WHERE fid = $1 AND link_type = 'follow'
            )
            SELECT message_hash FROM latest_events WHERE rn = 1 AND event_type = 'add'
            ",
        )
        .bind(fid)
        .fetch_all(self.database.pool())
        .await?
        .into_iter()
        .collect();

        tracing::info!(
            "📊 Found {} existing following links in database for FID {}",
            existing_hashes.len(),
            fid
        );

        let mut following = Vec::new();
        let mut batch_data = Vec::new();
        let mut next_page_token: Option<String> = None;
        let mut total_fetched = 0;
        let mut skipped = 0;
        let mut new_data = 0;
        let mut page_num = 0;

        // Paginate through ALL following links
        loop {
            page_num += 1;
            let response = client
                .get_links_by_fid(fid as u64, "follow", Some(1000), next_page_token.as_deref())
                .await?;

            let msg_count = response.messages.len();
            total_fetched += msg_count;

            tracing::info!(
                "📩 Page {}: {} messages (total: {})",
                page_num,
                msg_count,
                total_fetched
            );

            // Collect data from this page
            for message in &response.messages {
                if let Some(data) = &message.data {
                    if let Some(link_body) = data.body.get("link_body") {
                        let target_fid = link_body
                            .get("target_fid")
                            .and_then(serde_json::Value::as_i64)
                            .unwrap_or(0);

                        if target_fid > 0 {
                            following.push(target_fid);

                            // Decode hex hash to bytes
                            let hash_bytes = if message.hash.starts_with("0x") {
                                hex::decode(&message.hash[2..])
                                    .unwrap_or_else(|_| message.hash.as_bytes().to_vec())
                            } else {
                                hex::decode(&message.hash)
                                    .unwrap_or_else(|_| message.hash.as_bytes().to_vec())
                            };

                            // ⚡ 智能跳过：如果数据库已有此 hash，跳过
                            if existing_hashes.contains(&hash_bytes) {
                                skipped += 1;
                                continue;
                            }

                            new_data += 1;
                            let link_type = link_body
                                .get("type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("follow");

                            batch_data.push((
                                fid,
                                target_fid,
                                link_type.to_string(),
                                data.timestamp as i64,
                                hash_bytes,
                            ));
                        }
                    }
                }
            }

            // Check if we have more pages
            if msg_count < 1000 {
                // Got less than requested, means we're done
                tracing::info!("✓ Reached last page (received {} < 1000)", msg_count);
                break;
            }

            // Check if there's a next page token
            if let Some(token) = response.next_page_token {
                next_page_token = Some(token);
                tracing::debug!("→ More pages available, fetching next page...");
                // Continue to next iteration with the new token
            } else {
                // No next page token, we're done
                tracing::info!("✓ No more pages (no next_page_token)");
                break;
            }
        }

        // Batch insert only NEW links
        if batch_data.is_empty() {
            tracing::info!("✨ All data already exists - no insertion needed!");
        } else {
            tracing::info!(
                "💾 Batch inserting {} NEW links (skipped {} existing)...",
                batch_data.len(),
                skipped
            );

            for chunk in batch_data.chunks(500) {
                let mut query_builder = sqlx::QueryBuilder::new(
                    "INSERT INTO links (fid, target_fid, link_type, timestamp, message_hash) ",
                );

                query_builder.push_values(
                    chunk,
                    |mut b, (fid, target_fid, link_type, timestamp, hash)| {
                        b.push_bind(fid)
                            .push_bind(target_fid)
                            .push_bind(link_type)
                            .push_bind(timestamp)
                            .push_bind(hash);
                    },
                );

                query_builder.push(" ON CONFLICT (message_hash) DO NOTHING");

                let query = query_builder.build();
                query.execute(self.database.pool()).await?;
            }
        }

        tracing::info!(
            "✅ Lazy loaded {} following for FID {} (fetched: {}, new: {}, skipped: {})",
            following.len(),
            fid,
            total_fetched,
            new_data,
            skipped
        );
        Ok(following)
    }

    /// Lazy load followers list from Snapchain and insert into database
    async fn lazy_load_followers(&self, fid: i64) -> Result<Vec<i64>> {
        let client = self.snapchain_client.as_ref().ok_or_else(|| {
            crate::SnapRagError::Custom("Snapchain client not available".to_string())
        })?;

        // Step 1: 先获取数据库中已有的 message_hash，避免重复获取
        let existing_hashes: std::collections::HashSet<Vec<u8>> = sqlx::query_scalar::<_, Vec<u8>>(
            r"
            WITH latest_events AS (
                SELECT *, ROW_NUMBER() OVER (
                    PARTITION BY fid, target_fid 
                    ORDER BY timestamp DESC
                ) as rn
                FROM links
                WHERE target_fid = $1 AND link_type = 'follow'
            )
            SELECT message_hash FROM latest_events WHERE rn = 1 AND event_type = 'add'
            ",
        )
        .bind(fid)
        .fetch_all(self.database.pool())
        .await?
        .into_iter()
        .collect();

        tracing::info!(
            "📊 Found {} existing followers in database for FID {}",
            existing_hashes.len(),
            fid
        );

        let mut followers = Vec::new();
        let mut batch_data = Vec::new();
        let mut next_page_token: Option<String> = None;
        let mut total_fetched = 0;
        let mut skipped = 0;
        let mut new_data = 0;
        let mut page_num = 0;

        // Paginate through ALL followers
        loop {
            page_num += 1;
            let response = client
                .get_links_by_target_fid(
                    fid as u64,
                    "follow",
                    Some(1000),
                    next_page_token.as_deref(),
                )
                .await?;

            let msg_count = response.messages.len();
            total_fetched += msg_count;

            tracing::info!(
                "📩 Page {}: {} messages (total: {})",
                page_num,
                msg_count,
                total_fetched
            );

            // Collect data from this page
            for message in &response.messages {
                if let Some(data) = &message.data {
                    let follower_fid = data.fid as i64;
                    followers.push(follower_fid);

                    if let Some(link_body) = data.body.get("link_body") {
                        let link_type = link_body
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("follow");

                        // Decode hex hash to bytes
                        let hash_bytes = if message.hash.starts_with("0x") {
                            hex::decode(&message.hash[2..])
                                .unwrap_or_else(|_| message.hash.as_bytes().to_vec())
                        } else {
                            hex::decode(&message.hash)
                                .unwrap_or_else(|_| message.hash.as_bytes().to_vec())
                        };

                        // ⚡ 智能跳过：如果数据库已有此 hash，跳过
                        if existing_hashes.contains(&hash_bytes) {
                            skipped += 1;
                            continue;
                        }

                        new_data += 1;
                        batch_data.push((
                            follower_fid,
                            fid,
                            link_type.to_string(),
                            data.timestamp as i64,
                            hash_bytes,
                        ));
                    }
                }
            }

            // Check if we have more pages
            if msg_count < 1000 {
                tracing::info!("✓ Reached last page (received {} < 1000)", msg_count);
                break;
            }

            // Check if there's a next page token
            if let Some(token) = response.next_page_token {
                next_page_token = Some(token);
                tracing::debug!("→ More pages available, fetching next page...");
            } else {
                tracing::info!("✓ No more pages (no next_page_token)");
                break;
            }
        }

        // Batch insert only NEW links
        if batch_data.is_empty() {
            tracing::info!("✨ All data already exists - no insertion needed!");
        } else {
            tracing::info!(
                "💾 Batch inserting {} NEW links (skipped {} existing)...",
                batch_data.len(),
                skipped
            );

            for chunk in batch_data.chunks(500) {
                let mut query_builder = sqlx::QueryBuilder::new(
                    "INSERT INTO links (fid, target_fid, link_type, timestamp, message_hash) ",
                );

                query_builder.push_values(
                    chunk,
                    |mut b, (follower_fid, target_fid, link_type, timestamp, hash)| {
                        b.push_bind(follower_fid)
                            .push_bind(target_fid)
                            .push_bind(link_type)
                            .push_bind(timestamp)
                            .push_bind(hash);
                    },
                );

                query_builder.push(" ON CONFLICT (message_hash) DO NOTHING");

                let query = query_builder.build();
                query.execute(self.database.pool()).await?;
            }
        }

        tracing::info!(
            "✅ Lazy loaded {} followers for FID {} (fetched: {}, new: {}, skipped: {})",
            followers.len(),
            fid,
            total_fetched,
            new_data,
            skipped
        );
        Ok(followers)
    }

    /// Analyze users mentioned in casts
    async fn analyze_mentions(&self, fid: i64) -> Result<Vec<UserMention>> {
        // Get casts with mentions
        let casts = sqlx::query_as::<_, (Option<serde_json::Value>,)>(
            r"
            SELECT mentions
            FROM casts
            WHERE fid = $1 AND mentions IS NOT NULL
            ORDER BY timestamp DESC
            LIMIT 100
            ",
        )
        .bind(fid)
        .fetch_all(self.database.pool())
        .await?;

        // Count mention frequency
        let mut mention_counts: HashMap<i64, usize> = HashMap::new();

        for (mentions_json,) in casts {
            if let Some(mentions_json) = mentions_json {
                if let Some(mentions_array) = mentions_json.as_array() {
                    for mention in mentions_array {
                        if let Some(mentioned_fid) = mention.as_i64() {
                            *mention_counts.entry(mentioned_fid).or_insert(0) += 1;
                        }
                    }
                }
            }
        }

        // Sort by frequency and get top 10
        let mut sorted_mentions: Vec<_> = mention_counts.into_iter().collect();
        sorted_mentions.sort_by(|a, b| b.1.cmp(&a.1));
        sorted_mentions.truncate(10);

        // Get user profiles for mentioned users - batch query to avoid N+1
        let mut result = Vec::new();
        if sorted_mentions.is_empty() {
            return Ok(result);
        }

        // Batch fetch profiles
        let fids: Vec<i64> = sorted_mentions.iter().map(|(fid, _)| *fid).collect();
        let mut profiles_map = std::collections::HashMap::new();

        // Use a single query with IN clause (more efficient than multiple queries)
        // Build dynamic SQL with placeholders for each FID
        let placeholders: Vec<String> = (1..=fids.len()).map(|i| format!("${i}")).collect();
        let sql = format!(
            r"
            SELECT 
                id, fid, username, display_name, bio, pfp_url, banner_url, location,
                website_url, twitter_username, github_username, primary_address_ethereum,
                primary_address_solana, profile_token, profile_embedding, bio_embedding,
                interests_embedding, last_updated_timestamp, last_updated_at,
                shard_id, block_height, transaction_fid
            FROM user_profiles 
            WHERE fid IN ({})
            ",
            placeholders.join(", ")
        );

        let mut query = sqlx::query_as::<_, crate::models::UserProfile>(&sql);
        for fid in &fids {
            query = query.bind(fid);
        }
        let profiles = query.fetch_all(self.database.pool()).await?;

        for profile in profiles {
            profiles_map.insert(profile.fid, profile);
        }

        // Build result with categories (simplified - skip categorize_user for performance)
        for (mentioned_fid, count) in sorted_mentions {
            let profile = profiles_map.get(&mentioned_fid);

            result.push(UserMention {
                fid: mentioned_fid,
                username: profile.and_then(|p| p.username.clone()),
                display_name: profile.and_then(|p| p.display_name.clone()),
                count,
                category: "unknown".to_string(), // Skip expensive categorize_user call
            });
        }

        Ok(result)
    }

    /// Get top users with profiles (optimized with batch query)
    async fn get_top_users(&self, fids: &[i64], limit: usize) -> Result<Vec<UserMention>> {
        if fids.is_empty() {
            return Ok(Vec::new());
        }

        let fids_to_fetch: Vec<i64> = fids.iter().take(limit).copied().collect();

        // Batch fetch profiles in a single query
        let placeholders: Vec<String> =
            (1..=fids_to_fetch.len()).map(|i| format!("${i}")).collect();
        let sql = format!(
            r"
            SELECT 
                id, fid, username, display_name, bio, pfp_url, banner_url, location,
                website_url, twitter_username, github_username, primary_address_ethereum,
                primary_address_solana, profile_token, profile_embedding, bio_embedding,
                interests_embedding, last_updated_timestamp, last_updated_at,
                shard_id, block_height, transaction_fid
            FROM user_profiles 
            WHERE fid IN ({})
            ",
            placeholders.join(", ")
        );

        let mut query = sqlx::query_as::<_, crate::models::UserProfile>(&sql);
        for fid in &fids_to_fetch {
            query = query.bind(fid);
        }
        let profiles = query.fetch_all(self.database.pool()).await?;

        // Build result map for quick lookup
        let mut profiles_map = std::collections::HashMap::new();
        for profile in profiles {
            profiles_map.insert(profile.fid, profile);
        }

        // Build result in original order, skip expensive categorize_user
        let mut result = Vec::new();
        for fid in fids_to_fetch {
            let profile = profiles_map.get(&fid);
            result.push(UserMention {
                fid,
                username: profile.and_then(|p| p.username.clone()),
                display_name: profile.and_then(|p| p.display_name.clone()),
                count: 1,
                category: "unknown".to_string(), // Skip expensive categorize_user call
            });
        }

        Ok(result)
    }

    /// Categorize a user based on their content
    async fn categorize_user(&self, fid: i64) -> Result<String> {
        // Get recent casts to analyze content
        let casts = self
            .database
            .get_casts_by_fid(fid, Some(20), Some(0))
            .await?;

        if casts.is_empty() {
            return Ok("unknown".to_string());
        }

        // Analyze content for keywords
        let all_text: String = casts
            .iter()
            .filter_map(|c| c.text.as_ref())
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();

        let tech_score = count_keywords(
            &all_text,
            &[
                "build",
                "dev",
                "code",
                "api",
                "protocol",
                "github",
                "rust",
                "typescript",
                "solidity",
                "engineering",
            ],
        );

        let web3_score = count_keywords(
            &all_text,
            &[
                "web3",
                "crypto",
                "nft",
                "blockchain",
                "onchain",
                "eth",
                "base",
                "token",
                "defi",
                "dao",
            ],
        );

        let creator_score = count_keywords(
            &all_text,
            &[
                "art", "design", "create", "music", "writing", "video", "content", "story",
                "creative",
            ],
        );

        // Determine primary category
        let max_score = tech_score.max(web3_score).max(creator_score);

        if max_score == 0 {
            Ok("casual".to_string())
        } else if tech_score == max_score {
            Ok("tech".to_string())
        } else if web3_score == max_score {
            Ok("web3".to_string())
        } else {
            Ok("creator".to_string())
        }
    }

    /// Categorize social circles based on mentioned users
    fn categorize_from_mentions(&self, mentioned_users: &[UserMention]) -> SocialCircles {
        if mentioned_users.is_empty() {
            return SocialCircles {
                tech_builders: 0.0,
                content_creators: 0.0,
                web3_natives: 0.0,
                casual_users: 0.0,
            };
        }

        let total_weight: usize = mentioned_users.iter().map(|u| u.count).sum();

        let mut tech_weight = 0;
        let mut web3_weight = 0;
        let mut creator_weight = 0;
        let mut casual_weight = 0;

        for user in mentioned_users {
            match user.category.as_str() {
                "tech" => tech_weight += user.count,
                "web3" => web3_weight += user.count,
                "creator" => creator_weight += user.count,
                _ => casual_weight += user.count,
            }
        }

        let total = total_weight as f32;

        SocialCircles {
            tech_builders: (tech_weight as f32 / total) * 100.0,
            content_creators: (creator_weight as f32 / total) * 100.0,
            web3_natives: (web3_weight as f32 / total) * 100.0,
            casual_users: (casual_weight as f32 / total) * 100.0,
        }
    }

    /// Categorize user's social circles
    async fn categorize_social_circles(&self, following: &[i64]) -> Result<SocialCircles> {
        if following.is_empty() {
            return Ok(SocialCircles {
                tech_builders: 0.0,
                content_creators: 0.0,
                web3_natives: 0.0,
                casual_users: 0.0,
            });
        }

        // Simplified categorization - use profile data instead of expensive categorize_user
        // Sample up to 50 users to avoid too many queries
        let sample_size = following.len().min(50);
        let fids_to_check: Vec<i64> = following.iter().take(sample_size).copied().collect();

        // Batch fetch profiles
        let placeholders: Vec<String> =
            (1..=fids_to_check.len()).map(|i| format!("${i}")).collect();
        let sql = format!(
            r"
            SELECT 
                id, fid, username, display_name, bio, pfp_url, banner_url, location,
                website_url, twitter_username, github_username, primary_address_ethereum,
                primary_address_solana, profile_token, profile_embedding, bio_embedding,
                interests_embedding, last_updated_timestamp, last_updated_at,
                shard_id, block_height, transaction_fid
            FROM user_profiles 
            WHERE fid IN ({})
            ",
            placeholders.join(", ")
        );

        let mut query = sqlx::query_as::<_, crate::models::UserProfile>(&sql);
        for fid in &fids_to_check {
            query = query.bind(fid);
        }
        let profiles = query.fetch_all(self.database.pool()).await?;

        let mut tech_count = 0;
        let mut web3_count = 0;
        let mut creator_count = 0;
        let mut casual_count = 0;

        // Simple categorization based on profile fields (much faster than analyze casts)
        for profile in profiles {
            let bio_lower = profile.bio.as_deref().unwrap_or("").to_lowercase();
            let username_lower = profile.username.as_deref().unwrap_or("").to_lowercase();
            let combined = format!("{bio_lower} {username_lower}");

            // Quick keyword check
            let has_tech = combined.contains("dev")
                || combined.contains("build")
                || combined.contains("code")
                || combined.contains("github");
            let has_web3 = combined.contains("web3")
                || combined.contains("crypto")
                || combined.contains("nft")
                || combined.contains("eth")
                || combined.contains("blockchain");
            let has_creator = combined.contains("art")
                || combined.contains("design")
                || combined.contains("music")
                || combined.contains("writer");

            if has_tech && !has_web3 {
                tech_count += 1;
            } else if has_web3 {
                web3_count += 1;
            } else if has_creator {
                creator_count += 1;
            } else {
                casual_count += 1;
            }
        }

        let total = sample_size as f32;

        Ok(SocialCircles {
            tech_builders: (tech_count as f32 / total) * 100.0,
            content_creators: (creator_count as f32 / total) * 100.0,
            web3_natives: (web3_count as f32 / total) * 100.0,
            casual_users: (casual_count as f32 / total) * 100.0,
        })
    }

    /// Analyze user's interaction style
    async fn analyze_interaction_style(&self, fid: i64) -> Result<InteractionStyle> {
        // Optimized: Single query with FILTER instead of 3 separate COUNT queries
        let counts: (i64, i64, i64) = sqlx::query_as(
            r"
            SELECT 
                COUNT(*) as total,
                COUNT(*) FILTER (WHERE parent_hash IS NOT NULL) as replies,
                COUNT(*) FILTER (WHERE mentions IS NOT NULL) as mentions
            FROM casts 
            WHERE fid = $1
            ",
        )
        .bind(fid)
        .fetch_one(self.database.pool())
        .await?;

        let (total_casts, reply_count, mention_count) = counts;

        let reply_frequency = if total_casts > 0 {
            reply_count as f32 / total_casts as f32
        } else {
            0.0
        };

        let mention_frequency = if total_casts > 0 {
            mention_count as f32 / total_casts as f32
        } else {
            0.0
        };

        // Determine community role (optimized - use count methods instead of fetching all data)
        let following_count = self.count_following(fid).await?;
        let followers_count = self.count_followers(fid).await?;

        let community_role = if followers_count > 1000 && reply_frequency > 0.3 {
            "leader".to_string()
        } else if reply_frequency > 0.4 || mention_frequency > 0.3 {
            "contributor".to_string()
        } else if following_count > 100 {
            "observer".to_string()
        } else {
            "casual".to_string()
        };

        let network_connector = mention_frequency > 0.3 && reply_frequency > 0.3;

        Ok(InteractionStyle {
            reply_frequency,
            mention_frequency,
            network_connector,
            community_role,
        })
    }

    /// Generate word cloud from user's casts
    async fn generate_word_cloud(&self, fid: i64) -> Result<WordCloud> {
        // Get recent casts
        let casts = self
            .database
            .get_casts_by_fid(fid, Some(100), Some(0))
            .await?;

        if casts.is_empty() {
            return Ok(WordCloud {
                top_words: Vec::new(),
                top_phrases: Vec::new(),
                signature_words: Vec::new(),
            });
        }

        // Filter out bot/automated messages before analysis
        let filtered_casts: Vec<_> = casts
            .iter()
            .filter(|cast| !is_bot_message(cast.text.as_deref()))
            .collect();

        if filtered_casts.is_empty() {
            return Ok(WordCloud {
                top_words: Vec::new(),
                top_phrases: Vec::new(),
                signature_words: Vec::new(),
            });
        }

        // Combine all text from filtered casts
        let all_text: String = filtered_casts
            .iter()
            .filter_map(|c| c.text.as_ref())
            .cloned()
            .collect::<Vec<String>>()
            .join(" ");

        // Count word frequencies
        let word_freq = count_word_frequencies(&all_text);
        let total_words: usize = word_freq.values().sum();

        // Get top words (excluding stop words)
        let mut sorted_words: Vec<_> = word_freq.into_iter().collect();
        sorted_words.sort_by(|a, b| b.1.cmp(&a.1));

        let top_words: Vec<WordFrequency> = sorted_words
            .iter()
            .take(20)
            .map(|(word, count)| WordFrequency {
                word: word.clone(),
                count: *count,
                percentage: (*count as f32 / total_words as f32) * 100.0,
            })
            .collect();

        // Extract common 2-word phrases
        let phrases = extract_common_phrases(&all_text, 15);

        // Identify signature words (words user uses more than average)
        let signature_words = identify_signature_words(&sorted_words, 10);

        Ok(WordCloud {
            top_words,
            top_phrases: phrases,
            signature_words,
        })
    }

    /// Format social profile as a human-readable string for LLM context
    #[must_use]
    pub fn format_for_llm(&self, profile: &SocialProfile) -> String {
        let mut output = String::new();

        output.push_str("═══════════════════════════════════════════════════════\n");
        output.push_str("👥 SOCIAL NETWORK PROFILE\n");
        output.push_str("═══════════════════════════════════════════════════════\n\n");

        // Basic stats
        output.push_str(&format!(
            "Following: {} | Followers: {} | Influence: {:.1}x\n\n",
            profile.following_count, profile.followers_count, profile.influence_score
        ));

        // Social circles
        output.push_str("Social Circle Breakdown:\n");
        if profile.social_circles.tech_builders > 30.0 {
            output.push_str(&format!(
                "  🔧 Tech/Builders: {:.0}% - HEAVY tech network\n",
                profile.social_circles.tech_builders
            ));
        } else if profile.social_circles.tech_builders > 10.0 {
            output.push_str(&format!(
                "  🔧 Tech/Builders: {:.0}%\n",
                profile.social_circles.tech_builders
            ));
        }

        if profile.social_circles.web3_natives > 30.0 {
            output.push_str(&format!(
                "  ⛓️ Web3/Crypto: {:.0}% - HEAVY web3 network\n",
                profile.social_circles.web3_natives
            ));
        } else if profile.social_circles.web3_natives > 10.0 {
            output.push_str(&format!(
                "  ⛓️ Web3/Crypto: {:.0}%\n",
                profile.social_circles.web3_natives
            ));
        }

        if profile.social_circles.content_creators > 20.0 {
            output.push_str(&format!(
                "  🎨 Creators: {:.0}%\n",
                profile.social_circles.content_creators
            ));
        }

        output.push('\n');

        // Most mentioned users
        if !profile.most_mentioned_users.is_empty() {
            output.push_str("Most Frequently Mentioned:\n");
            for (idx, user) in profile.most_mentioned_users.iter().take(3).enumerate() {
                let name = user
                    .username
                    .as_ref()
                    .map(|u| format!("@{u}"))
                    .or_else(|| user.display_name.clone())
                    .unwrap_or_else(|| format!("FID {}", user.fid));

                output.push_str(&format!(
                    "  {}. {} ({}x, {})\n",
                    idx + 1,
                    name,
                    user.count,
                    user.category
                ));
            }
            output.push('\n');
        }

        // Interaction style
        output.push_str("Interaction Style:\n");
        output.push_str(&format!(
            "  Role: {} | Reply rate: {:.0}% | Mention rate: {:.0}%\n",
            profile.interaction_style.community_role,
            profile.interaction_style.reply_frequency * 100.0,
            profile.interaction_style.mention_frequency * 100.0
        ));

        if profile.interaction_style.network_connector {
            output.push_str("  🌐 Network Connector - actively introduces people\n");
        }

        output.push('\n');

        // Add context instructions
        output.push_str("🎯 Social Context Instructions:\n");

        if profile.influence_score > 2.0 {
            output.push_str("  → Influential user - speak with confidence\n");
        } else if profile.influence_score < 0.5 {
            output.push_str("  → Growing account - show learning mindset\n");
        }

        if profile.social_circles.tech_builders > 40.0 {
            output.push_str("  → Deep in tech circles - use builder language\n");
        }

        if profile.social_circles.web3_natives > 40.0 {
            output.push_str("  → Web3 native - understand crypto culture\n");
        }

        if profile.interaction_style.reply_frequency > 0.4 {
            output.push_str("  → Active conversationalist - engage with questions\n");
        }

        if !profile.most_mentioned_users.is_empty() {
            output.push_str("  → You can reference your network: ");
            let names: Vec<String> = profile
                .most_mentioned_users
                .iter()
                .take(3)
                .filter_map(|u| u.username.as_ref().map(|n| format!("@{n}")))
                .collect();
            output.push_str(&names.join(", "));
            output.push('\n');
        }

        output.push_str("\n═══════════════════════════════════════════════════════\n\n");

        output
    }
}

/// Helper function to count keywords in text
fn count_keywords(text: &str, keywords: &[&str]) -> usize {
    keywords
        .iter()
        .filter(|keyword| text.contains(*keyword))
        .count()
}

/// Count word frequencies (excluding stop words and common words)
fn count_word_frequencies(text: &str) -> HashMap<String, usize> {
    let mut word_counts: HashMap<String, usize> = HashMap::new();

    // Tokenize and count
    for word in text.split_whitespace() {
        let cleaned = word
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase();

        // Skip if empty, too short, or stop word (using stop-words crate)
        if cleaned.len() < 3 || STOP_WORDS_SET.contains(&cleaned) {
            continue;
        }

        // Skip URLs
        if cleaned.starts_with("http") || cleaned.contains("://") {
            continue;
        }

        // Skip mentions and hashtags
        if cleaned.starts_with('@') || cleaned.starts_with('#') {
            continue;
        }

        *word_counts.entry(cleaned).or_insert(0) += 1;
    }

    word_counts
}

/// Extract common 2-word phrases
fn extract_common_phrases(text: &str, limit: usize) -> Vec<WordFrequency> {
    let mut phrase_counts: HashMap<String, usize> = HashMap::new();

    let words: Vec<String> = text
        .split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|w| w.len() >= 3 && !STOP_WORDS_SET.contains(w))
        .collect();

    // Count 2-word phrases
    for window in words.windows(2) {
        if window.len() == 2 {
            let phrase = format!("{} {}", window[0], window[1]);
            *phrase_counts.entry(phrase).or_insert(0) += 1;
        }
    }

    // Sort and get top phrases (must appear at least 2 times)
    let mut sorted_phrases: Vec<_> = phrase_counts
        .into_iter()
        .filter(|(_, count)| *count >= 2)
        .collect();
    sorted_phrases.sort_by(|a, b| b.1.cmp(&a.1));

    let total: usize = sorted_phrases.iter().map(|(_, count)| count).sum();

    sorted_phrases
        .into_iter()
        .take(limit)
        .map(|(phrase, count)| WordFrequency {
            word: phrase,
            count,
            percentage: if total > 0 {
                (count as f32 / total as f32) * 100.0
            } else {
                0.0
            },
        })
        .collect()
}

/// Identify signature words - words this user uses notably
fn identify_signature_words(sorted_words: &[(String, usize)], limit: usize) -> Vec<String> {
    sorted_words
        .iter()
        .filter(|(word, count)| {
            // Filter for meaningful words used frequently (5+ times)
            *count >= 5 && word.len() >= 4
        })
        .take(limit)
        .map(|(word, _)| word.clone())
        .collect()
}

// Note: get_stop_words() function removed - now using stop-words crate via STOP_WORDS_SET

/// Check if a cast is likely a bot/automated message
fn is_bot_message(text: Option<&str>) -> bool {
    let Some(text) = text else {
        return false;
    };

    let text_lower = text.to_lowercase();

    // Bot message patterns to filter out
    let bot_patterns = [
        "ms!t",                                 // microsub bot marker
        "i'm supporting you through /microsub", // microsub support messages
        "please mute the keyword \"ms!t\"",     // microsub mute instruction
        "$degen",                               // degen tip bot (when standalone)
        "minted",                               // NFT mint notifications (alone)
        "you've been tipped",                   // tip notifications
        "airdrop claim",                        // airdrop spam
        "congratulations! you won",             // spam/scam
        "click here to claim",                  // spam/scam
        "limited time offer",                   // spam
        "visit this link",                      // spam
    ];

    // Check for exact bot patterns
    for pattern in &bot_patterns {
        if text_lower.contains(pattern) {
            return true;
        }
    }

    // Additional heuristic: very short automated messages
    // Skip if it's just a tip/support notification
    if text.len() < 50 && text_lower.contains("$degen") && text_lower.contains("supporting") {
        return true;
    }

    // Filter out pure emoji posts without meaningful text (likely automated reactions)
    let has_meaningful_text = text
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .count()
        > 10;

    if !has_meaningful_text && text.len() < 20 {
        return true; // Likely automated emoji spam
    }

    false
}
