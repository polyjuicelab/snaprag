//! Network statistics and comparison queries
//!
//! This module provides aggregate statistics across all users for comparison purposes.

use super::Database;
use crate::Result;

/// Network averages
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NetworkAverages {
    pub avg_casts_per_user: f64,
    pub avg_reactions_per_user: f64,
    pub avg_followers_per_user: f64,
    pub total_active_users: i64,
}

// Percentiles type is defined in api/types.rs

impl Database {
    /// Get network averages
    ///
    /// Calculates average statistics across all active users within the time range.
    ///
    /// # Arguments
    ///
    /// * `start_timestamp` - Start timestamp (Farcaster timestamp)
    /// * `end_timestamp` - End timestamp (Farcaster timestamp)
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    ///
    /// # Performance
    ///
    /// This is an expensive query. Consider caching the results.
    pub async fn get_network_averages(
        &self,
        start_timestamp: Option<i64>,
        end_timestamp: Option<i64>,
    ) -> Result<NetworkAverages> {
        // Count active users (users who have casts in the time range)
        let total_active_users: (i64,) =
            if let (Some(start), Some(end)) = (start_timestamp, end_timestamp) {
                sqlx::query_as(
                    r"
                SELECT COUNT(DISTINCT fid)::bigint
                FROM casts
                WHERE timestamp >= $1 AND timestamp <= $2
                ",
                )
                .bind(start)
                .bind(end)
                .fetch_one(&self.pool)
                .await?
            } else if let Some(start) = start_timestamp {
                sqlx::query_as(
                    r"
                SELECT COUNT(DISTINCT fid)::bigint
                FROM casts
                WHERE timestamp >= $1
                ",
                )
                .bind(start)
                .fetch_one(&self.pool)
                .await?
            } else if let Some(end) = end_timestamp {
                sqlx::query_as(
                    r"
                SELECT COUNT(DISTINCT fid)::bigint
                FROM casts
                WHERE timestamp <= $1
                ",
                )
                .bind(end)
                .fetch_one(&self.pool)
                .await?
            } else {
                sqlx::query_as(
                    r"
                SELECT COUNT(DISTINCT fid)::bigint
                FROM casts
                ",
                )
                .fetch_one(&self.pool)
                .await?
            };

        let active_users = total_active_users.0.max(1); // Avoid division by zero

        // Calculate average casts per user
        let total_casts: (i64,) = if let (Some(start), Some(end)) = (start_timestamp, end_timestamp)
        {
            sqlx::query_as(
                r"
                SELECT COUNT(*)::bigint
                FROM casts
                WHERE timestamp >= $1 AND timestamp <= $2
                ",
            )
            .bind(start)
            .bind(end)
            .fetch_one(&self.pool)
            .await?
        } else if let Some(start) = start_timestamp {
            sqlx::query_as(
                r"
                SELECT COUNT(*)::bigint
                FROM casts
                WHERE timestamp >= $1
                ",
            )
            .bind(start)
            .fetch_one(&self.pool)
            .await?
        } else if let Some(end) = end_timestamp {
            sqlx::query_as(
                r"
                SELECT COUNT(*)::bigint
                FROM casts
                WHERE timestamp <= $1
                ",
            )
            .bind(end)
            .fetch_one(&self.pool)
            .await?
        } else {
            sqlx::query_as(
                r"
                SELECT COUNT(*)::bigint
                FROM casts
                ",
            )
            .fetch_one(&self.pool)
            .await?
        };

        let avg_casts_per_user = total_casts.0 as f64 / active_users as f64;

        // Calculate average reactions per user (reactions given)
        let total_reactions: (i64,) =
            if let (Some(start), Some(end)) = (start_timestamp, end_timestamp) {
                sqlx::query_as(
                    r"
                SELECT COUNT(*)::bigint
                FROM (
                  SELECT DISTINCT ON (fid, target_cast_hash) *
                  FROM reactions
                  WHERE timestamp >= $1 AND timestamp <= $2
                  ORDER BY fid, target_cast_hash, timestamp DESC
                ) r
                WHERE r.event_type = 'add'
                ",
                )
                .bind(start)
                .bind(end)
                .fetch_one(&self.pool)
                .await?
            } else if let Some(start) = start_timestamp {
                sqlx::query_as(
                    r"
                SELECT COUNT(*)::bigint
                FROM (
                  SELECT DISTINCT ON (fid, target_cast_hash) *
                  FROM reactions
                  WHERE timestamp >= $1
                  ORDER BY fid, target_cast_hash, timestamp DESC
                ) r
                WHERE r.event_type = 'add'
                ",
                )
                .bind(start)
                .fetch_one(&self.pool)
                .await?
            } else if let Some(end) = end_timestamp {
                sqlx::query_as(
                    r"
                SELECT COUNT(*)::bigint
                FROM (
                  SELECT DISTINCT ON (fid, target_cast_hash) *
                  FROM reactions
                  WHERE timestamp <= $1
                  ORDER BY fid, target_cast_hash, timestamp DESC
                ) r
                WHERE r.event_type = 'add'
                ",
                )
                .bind(end)
                .fetch_one(&self.pool)
                .await?
            } else {
                sqlx::query_as(
                    r"
                SELECT COUNT(*)::bigint
                FROM (
                  SELECT DISTINCT ON (fid, target_cast_hash) *
                  FROM reactions
                  ORDER BY fid, target_cast_hash, timestamp DESC
                ) r
                WHERE r.event_type = 'add'
                ",
                )
                .fetch_one(&self.pool)
                .await?
            };

        let avg_reactions_per_user = total_reactions.0 as f64 / active_users as f64;

        // Calculate average followers per user
        // This is more complex - we need to count distinct followers for each user
        // For performance, we'll use a simplified approach
        let total_followers: (i64,) = sqlx::query_as(
            r"
            SELECT COUNT(*)::bigint
            FROM (
              SELECT DISTINCT ON (fid, target_fid) *
              FROM links
              WHERE link_type = 'follow'
                AND event_type = 'add'
              ORDER BY fid, target_fid, timestamp DESC
            ) l
            ",
        )
        .fetch_one(&self.pool)
        .await?;

        let avg_followers_per_user = total_followers.0 as f64 / active_users as f64;

        Ok(NetworkAverages {
            avg_casts_per_user,
            avg_reactions_per_user,
            avg_followers_per_user,
            total_active_users: active_users,
        })
    }

    /// Get percentiles for casts
    ///
    /// Calculates 50th, 75th, and 90th percentiles for cast counts per user.
    ///
    /// # Arguments
    ///
    /// * `start_timestamp` - Start timestamp (Farcaster timestamp)
    /// * `end_timestamp` - End timestamp (Farcaster timestamp)
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn get_cast_percentiles(
        &self,
        start_timestamp: Option<i64>,
        end_timestamp: Option<i64>,
    ) -> Result<crate::api::types::Percentiles> {
        #[derive(sqlx::FromRow)]
        struct PercentileResult {
            p50: Option<i64>,
            p75: Option<i64>,
            p90: Option<i64>,
        }

        let query = if let (Some(start), Some(end)) = (start_timestamp, end_timestamp) {
            sqlx::query_as::<_, PercentileResult>(
                r"
                WITH user_cast_counts AS (
                  SELECT fid, COUNT(*)::bigint as cast_count
                  FROM casts
                  WHERE timestamp >= $1 AND timestamp <= $2
                  GROUP BY fid
                )
                SELECT 
                  PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY cast_count)::bigint as p50,
                  PERCENTILE_CONT(0.75) WITHIN GROUP (ORDER BY cast_count)::bigint as p75,
                  PERCENTILE_CONT(0.90) WITHIN GROUP (ORDER BY cast_count)::bigint as p90
                FROM user_cast_counts
                ",
            )
            .bind(start)
            .bind(end)
        } else if let Some(start) = start_timestamp {
            sqlx::query_as::<_, PercentileResult>(
                r"
                WITH user_cast_counts AS (
                  SELECT fid, COUNT(*)::bigint as cast_count
                  FROM casts
                  WHERE timestamp >= $1
                  GROUP BY fid
                )
                SELECT 
                  PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY cast_count)::bigint as p50,
                  PERCENTILE_CONT(0.75) WITHIN GROUP (ORDER BY cast_count)::bigint as p75,
                  PERCENTILE_CONT(0.90) WITHIN GROUP (ORDER BY cast_count)::bigint as p90
                FROM user_cast_counts
                ",
            )
            .bind(start)
        } else if let Some(end) = end_timestamp {
            sqlx::query_as::<_, PercentileResult>(
                r"
                WITH user_cast_counts AS (
                  SELECT fid, COUNT(*)::bigint as cast_count
                  FROM casts
                  WHERE timestamp <= $1
                  GROUP BY fid
                )
                SELECT 
                  PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY cast_count)::bigint as p50,
                  PERCENTILE_CONT(0.75) WITHIN GROUP (ORDER BY cast_count)::bigint as p75,
                  PERCENTILE_CONT(0.90) WITHIN GROUP (ORDER BY cast_count)::bigint as p90
                FROM user_cast_counts
                ",
            )
            .bind(end)
        } else {
            sqlx::query_as::<_, PercentileResult>(
                r"
                WITH user_cast_counts AS (
                  SELECT fid, COUNT(*)::bigint as cast_count
                  FROM casts
                  GROUP BY fid
                )
                SELECT 
                  PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY cast_count)::bigint as p50,
                  PERCENTILE_CONT(0.75) WITHIN GROUP (ORDER BY cast_count)::bigint as p75,
                  PERCENTILE_CONT(0.90) WITHIN GROUP (ORDER BY cast_count)::bigint as p90
                FROM user_cast_counts
                ",
            )
        };

        let result = query.fetch_one(&self.pool).await?;

        Ok(crate::api::types::Percentiles {
            p50: result.p50.unwrap_or(0),
            p75: result.p75.unwrap_or(0),
            p90: result.p90.unwrap_or(0),
        })
    }
}
