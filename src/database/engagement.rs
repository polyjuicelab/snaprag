//! Engagement metrics database queries
//!
//! This module provides queries for user engagement metrics including:
//! - Reactions received on user's casts
//! - Recasts received
//! - Replies received
//! - Most popular cast
//! - Top interactive users

use super::Database;
use crate::models::Cast;
use crate::Result;

/// Popular cast with engagement metrics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PopularCast {
    pub message_hash: Vec<u8>,
    pub text: Option<String>,
    pub reactions: i64,
    pub recasts: i64,
    pub replies: i64,
    pub timestamp: i64,
}

/// Top interactive user
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TopInteractiveUser {
    pub fid: i64,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub pfp_url: Option<String>,
    pub interaction_count: i64,
}

impl Database {
    /// Count reactions received on user's casts
    ///
    /// Counts reactions where the target_cast_hash matches one of the user's casts
    /// and the reaction was added within the time range.
    ///
    /// # Arguments
    ///
    /// * `fid` - User's FID
    /// * `start_timestamp` - Start timestamp (Farcaster timestamp)
    /// * `end_timestamp` - End timestamp (Farcaster timestamp)
    /// * `reaction_type` - 1 for likes, 2 for recasts
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn get_reactions_received(
        &self,
        fid: i64,
        start_timestamp: Option<i64>,
        end_timestamp: Option<i64>,
        reaction_type: i16,
    ) -> Result<i64> {
        let query = if let (Some(start), Some(end)) = (start_timestamp, end_timestamp) {
            sqlx::query_scalar::<_, i64>(
                r"
                SELECT COUNT(*) 
                FROM reactions r
                INNER JOIN casts c ON r.target_cast_hash = c.message_hash
                WHERE c.fid = $1 
                  AND r.event_type = 'add'
                  AND r.timestamp >= $2 
                  AND r.timestamp <= $3
                  AND r.reaction_type = $4
                ",
            )
            .bind(fid)
            .bind(start)
            .bind(end)
            .bind(reaction_type)
        } else if let Some(start) = start_timestamp {
            sqlx::query_scalar::<_, i64>(
                r"
                SELECT COUNT(*) 
                FROM reactions r
                INNER JOIN casts c ON r.target_cast_hash = c.message_hash
                WHERE c.fid = $1 
                  AND r.event_type = 'add'
                  AND r.timestamp >= $2
                  AND r.reaction_type = $3
                ",
            )
            .bind(fid)
            .bind(start)
            .bind(reaction_type)
        } else if let Some(end) = end_timestamp {
            sqlx::query_scalar::<_, i64>(
                r"
                SELECT COUNT(*) 
                FROM reactions r
                INNER JOIN casts c ON r.target_cast_hash = c.message_hash
                WHERE c.fid = $1 
                  AND r.event_type = 'add'
                  AND r.timestamp <= $2
                  AND r.reaction_type = $3
                ",
            )
            .bind(fid)
            .bind(end)
            .bind(reaction_type)
        } else {
            sqlx::query_scalar::<_, i64>(
                r"
                SELECT COUNT(*) 
                FROM reactions r
                INNER JOIN casts c ON r.target_cast_hash = c.message_hash
                WHERE c.fid = $1 
                  AND r.event_type = 'add'
                  AND r.reaction_type = $2
                ",
            )
            .bind(fid)
            .bind(reaction_type)
        };

        let count = query.fetch_one(&self.pool).await?;
        Ok(count)
    }

    /// Count recasts received on user's casts
    ///
    /// This is a convenience method that calls `get_reactions_received` with reaction_type = 2.
    pub async fn get_recasts_received(
        &self,
        fid: i64,
        start_timestamp: Option<i64>,
        end_timestamp: Option<i64>,
    ) -> Result<i64> {
        self.get_reactions_received(fid, start_timestamp, end_timestamp, 2)
            .await
    }

    /// Count replies received on user's casts
    ///
    /// Counts casts where parent_hash matches one of the user's cast message_hashes
    /// and the reply was created within the time range.
    ///
    /// # Arguments
    ///
    /// * `fid` - User's FID
    /// * `start_timestamp` - Start timestamp (Farcaster timestamp)
    /// * `end_timestamp` - End timestamp (Farcaster timestamp)
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn get_replies_received(
        &self,
        fid: i64,
        start_timestamp: Option<i64>,
        end_timestamp: Option<i64>,
    ) -> Result<i64> {
        let query = if let (Some(start), Some(end)) = (start_timestamp, end_timestamp) {
            sqlx::query_scalar::<_, i64>(
                r"
                SELECT COUNT(*) 
                FROM casts r
                INNER JOIN casts c ON r.parent_hash = c.message_hash
                WHERE c.fid = $1 
                  AND r.timestamp >= $2 
                  AND r.timestamp <= $3
                ",
            )
            .bind(fid)
            .bind(start)
            .bind(end)
        } else if let Some(start) = start_timestamp {
            sqlx::query_scalar::<_, i64>(
                r"
                SELECT COUNT(*) 
                FROM casts r
                INNER JOIN casts c ON r.parent_hash = c.message_hash
                WHERE c.fid = $1 
                  AND r.timestamp >= $2
                ",
            )
            .bind(fid)
            .bind(start)
        } else if let Some(end) = end_timestamp {
            sqlx::query_scalar::<_, i64>(
                r"
                SELECT COUNT(*) 
                FROM casts r
                INNER JOIN casts c ON r.parent_hash = c.message_hash
                WHERE c.fid = $1 
                  AND r.timestamp <= $2
                ",
            )
            .bind(fid)
            .bind(end)
        } else {
            sqlx::query_scalar::<_, i64>(
                r"
                SELECT COUNT(*) 
                FROM casts r
                INNER JOIN casts c ON r.parent_hash = c.message_hash
                WHERE c.fid = $1
                ",
            )
            .bind(fid)
        };

        let count = query.fetch_one(&self.pool).await?;
        Ok(count)
    }

    /// Get the most popular cast by engagement
    ///
    /// Finds the cast with the highest total engagement (reactions + recasts + replies)
    /// within the specified time range.
    ///
    /// # Arguments
    ///
    /// * `fid` - User's FID
    /// * `start_timestamp` - Start timestamp (Farcaster timestamp)
    /// * `end_timestamp` - End timestamp (Farcaster timestamp)
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn get_most_popular_cast(
        &self,
        fid: i64,
        start_timestamp: Option<i64>,
        end_timestamp: Option<i64>,
    ) -> Result<Option<PopularCast>> {
        #[derive(sqlx::FromRow)]
        struct RawResult {
            message_hash: Vec<u8>,
            text: Option<String>,
            timestamp: i64,
            likes: i64,
            recasts: i64,
            replies: i64,
        }

        let query = if let (Some(start), Some(end)) = (start_timestamp, end_timestamp) {
            sqlx::query_as::<_, RawResult>(
                r"
                WITH cast_engagement AS (
                  SELECT 
                    c.message_hash,
                    c.text,
                    c.timestamp,
                    (SELECT COUNT(*) FROM (
                      SELECT *, ROW_NUMBER() OVER (
                        PARTITION BY fid, target_cast_hash 
                        ORDER BY timestamp DESC
                      ) as rn
                      FROM reactions
                      WHERE target_cast_hash = c.message_hash
                    ) r WHERE r.rn = 1 AND r.event_type = 'add' AND r.reaction_type = 1) as likes,
                    (SELECT COUNT(*) FROM (
                      SELECT *, ROW_NUMBER() OVER (
                        PARTITION BY fid, target_cast_hash 
                        ORDER BY timestamp DESC
                      ) as rn
                      FROM reactions
                      WHERE target_cast_hash = c.message_hash
                    ) r WHERE r.rn = 1 AND r.event_type = 'add' AND r.reaction_type = 2) as recasts,
                    (SELECT COUNT(*) FROM casts r 
                     WHERE r.parent_hash = c.message_hash) as replies
                  FROM casts c
                  WHERE c.fid = $1 
                    AND c.timestamp >= $2 
                    AND c.timestamp <= $3
                )
                SELECT *, (likes + recasts + replies) as total_engagement
                FROM cast_engagement
                ORDER BY (likes + recasts + replies) DESC
                LIMIT 1
                ",
            )
            .bind(fid)
            .bind(start)
            .bind(end)
        } else if let Some(start) = start_timestamp {
            sqlx::query_as::<_, RawResult>(
                r"
                WITH cast_engagement AS (
                  SELECT 
                    c.message_hash,
                    c.text,
                    c.timestamp,
                    (SELECT COUNT(*) FROM (
                      SELECT *, ROW_NUMBER() OVER (
                        PARTITION BY fid, target_cast_hash 
                        ORDER BY timestamp DESC
                      ) as rn
                      FROM reactions
                      WHERE target_cast_hash = c.message_hash
                    ) r WHERE r.rn = 1 AND r.event_type = 'add' AND r.reaction_type = 1) as likes,
                    (SELECT COUNT(*) FROM (
                      SELECT *, ROW_NUMBER() OVER (
                        PARTITION BY fid, target_cast_hash 
                        ORDER BY timestamp DESC
                      ) as rn
                      FROM reactions
                      WHERE target_cast_hash = c.message_hash
                    ) r WHERE r.rn = 1 AND r.event_type = 'add' AND r.reaction_type = 2) as recasts,
                    (SELECT COUNT(*) FROM casts r 
                     WHERE r.parent_hash = c.message_hash) as replies
                  FROM casts c
                  WHERE c.fid = $1 
                    AND c.timestamp >= $2
                )
                SELECT *, (likes + recasts + replies) as total_engagement
                FROM cast_engagement
                ORDER BY (likes + recasts + replies) DESC
                LIMIT 1
                ",
            )
            .bind(fid)
            .bind(start)
        } else if let Some(end) = end_timestamp {
            sqlx::query_as::<_, RawResult>(
                r"
                WITH cast_engagement AS (
                  SELECT 
                    c.message_hash,
                    c.text,
                    c.timestamp,
                    (SELECT COUNT(*) FROM (
                      SELECT *, ROW_NUMBER() OVER (
                        PARTITION BY fid, target_cast_hash 
                        ORDER BY timestamp DESC
                      ) as rn
                      FROM reactions
                      WHERE target_cast_hash = c.message_hash
                    ) r WHERE r.rn = 1 AND r.event_type = 'add' AND r.reaction_type = 1) as likes,
                    (SELECT COUNT(*) FROM (
                      SELECT *, ROW_NUMBER() OVER (
                        PARTITION BY fid, target_cast_hash 
                        ORDER BY timestamp DESC
                      ) as rn
                      FROM reactions
                      WHERE target_cast_hash = c.message_hash
                    ) r WHERE r.rn = 1 AND r.event_type = 'add' AND r.reaction_type = 2) as recasts,
                    (SELECT COUNT(*) FROM casts r 
                     WHERE r.parent_hash = c.message_hash) as replies
                  FROM casts c
                  WHERE c.fid = $1 
                    AND c.timestamp <= $2
                )
                SELECT *, (likes + recasts + replies) as total_engagement
                FROM cast_engagement
                ORDER BY (likes + recasts + replies) DESC
                LIMIT 1
                ",
            )
            .bind(fid)
            .bind(end)
        } else {
            sqlx::query_as::<_, RawResult>(
                r"
                WITH cast_engagement AS (
                  SELECT 
                    c.message_hash,
                    c.text,
                    c.timestamp,
                    (SELECT COUNT(*) FROM (
                      SELECT *, ROW_NUMBER() OVER (
                        PARTITION BY fid, target_cast_hash 
                        ORDER BY timestamp DESC
                      ) as rn
                      FROM reactions
                      WHERE target_cast_hash = c.message_hash
                    ) r WHERE r.rn = 1 AND r.event_type = 'add' AND r.reaction_type = 1) as likes,
                    (SELECT COUNT(*) FROM (
                      SELECT *, ROW_NUMBER() OVER (
                        PARTITION BY fid, target_cast_hash 
                        ORDER BY timestamp DESC
                      ) as rn
                      FROM reactions
                      WHERE target_cast_hash = c.message_hash
                    ) r WHERE r.rn = 1 AND r.event_type = 'add' AND r.reaction_type = 2) as recasts,
                    (SELECT COUNT(*) FROM casts r 
                     WHERE r.parent_hash = c.message_hash) as replies
                  FROM casts c
                  WHERE c.fid = $1
                )
                SELECT *, (likes + recasts + replies) as total_engagement
                FROM cast_engagement
                ORDER BY (likes + recasts + replies) DESC
                LIMIT 1
                ",
            )
            .bind(fid)
        };

        let result = query.fetch_optional(&self.pool).await?;

        Ok(result.map(|r| PopularCast {
            message_hash: r.message_hash,
            text: r.text,
            reactions: r.likes,
            recasts: r.recasts,
            replies: r.replies,
            timestamp: r.timestamp,
        }))
    }

    /// Get top interactive users who reacted to user's casts
    ///
    /// Returns users who have interacted most with the user's casts (reactions + recasts).
    ///
    /// # Arguments
    ///
    /// * `fid` - User's FID
    /// * `start_timestamp` - Start timestamp (Farcaster timestamp)
    /// * `end_timestamp` - End timestamp (Farcaster timestamp)
    /// * `limit` - Maximum number of users to return
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn get_top_interactive_users(
        &self,
        fid: i64,
        start_timestamp: Option<i64>,
        end_timestamp: Option<i64>,
        limit: i64,
    ) -> Result<Vec<TopInteractiveUser>> {
        #[derive(sqlx::FromRow)]
        struct RawResult {
            fid: i64,
            username: Option<String>,
            display_name: Option<String>,
            pfp_url: Option<String>,
            interaction_count: i64,
        }

        let query = if let (Some(start), Some(end)) = (start_timestamp, end_timestamp) {
            sqlx::query_as::<_, RawResult>(
                r"
                SELECT 
                  r.fid,
                  up.username,
                  up.display_name,
                  up.pfp_url,
                  COUNT(*) as interaction_count
                FROM reactions r
                INNER JOIN casts c ON r.target_cast_hash = c.message_hash
                LEFT JOIN user_profiles up ON r.fid = up.fid
                WHERE c.fid = $1 
                  AND r.event_type = 'add'
                  AND r.timestamp >= $2 
                  AND r.timestamp <= $3
                GROUP BY r.fid, up.username, up.display_name, up.pfp_url
                ORDER BY interaction_count DESC
                LIMIT $4
                ",
            )
            .bind(fid)
            .bind(start)
            .bind(end)
            .bind(limit)
        } else if let Some(start) = start_timestamp {
            sqlx::query_as::<_, RawResult>(
                r"
                SELECT 
                  r.fid,
                  up.username,
                  up.display_name,
                  up.pfp_url,
                  COUNT(*) as interaction_count
                FROM reactions r
                INNER JOIN casts c ON r.target_cast_hash = c.message_hash
                LEFT JOIN user_profiles up ON r.fid = up.fid
                WHERE c.fid = $1 
                  AND r.event_type = 'add'
                  AND r.timestamp >= $2
                GROUP BY r.fid, up.username, up.display_name, up.pfp_url
                ORDER BY interaction_count DESC
                LIMIT $3
                ",
            )
            .bind(fid)
            .bind(start)
            .bind(limit)
        } else if let Some(end) = end_timestamp {
            sqlx::query_as::<_, RawResult>(
                r"
                SELECT 
                  r.fid,
                  up.username,
                  up.display_name,
                  up.pfp_url,
                  COUNT(*) as interaction_count
                FROM reactions r
                INNER JOIN casts c ON r.target_cast_hash = c.message_hash
                LEFT JOIN user_profiles up ON r.fid = up.fid
                WHERE c.fid = $1 
                  AND r.event_type = 'add'
                  AND r.timestamp <= $2
                GROUP BY r.fid, up.username, up.display_name, up.pfp_url
                ORDER BY interaction_count DESC
                LIMIT $3
                ",
            )
            .bind(fid)
            .bind(end)
            .bind(limit)
        } else {
            sqlx::query_as::<_, RawResult>(
                r"
                SELECT 
                  r.fid,
                  up.username,
                  up.display_name,
                  up.pfp_url,
                  COUNT(*) as interaction_count
                FROM reactions r
                INNER JOIN casts c ON r.target_cast_hash = c.message_hash
                LEFT JOIN user_profiles up ON r.fid = up.fid
                WHERE c.fid = $1 
                  AND r.event_type = 'add'
                GROUP BY r.fid, up.username, up.display_name, up.pfp_url
                ORDER BY interaction_count DESC
                LIMIT $2
                ",
            )
            .bind(fid)
            .bind(limit)
        };

        let results = query.fetch_all(&self.pool).await?;

        Ok(results
            .into_iter()
            .map(|r| TopInteractiveUser {
                fid: r.fid,
                username: r.username,
                display_name: r.display_name,
                pfp_url: r.pfp_url,
                interaction_count: r.interaction_count,
            })
            .collect())
    }
}
