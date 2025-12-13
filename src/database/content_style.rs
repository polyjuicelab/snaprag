//! Content style analysis database queries
//!
//! This module provides queries for analyzing user content style including:
//! - Emoji usage
//! - Frame usage
//! - Average cast length

use super::Database;
use crate::Result;

/// Emoji frequency entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmojiFrequency {
    pub emoji: String,
    pub count: i64,
}

impl Database {
    /// Get frame usage count for a user
    ///
    /// Counts distinct frame URLs that the user has interacted with.
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
    pub async fn get_frame_usage_count(
        &self,
        fid: i64,
        start_timestamp: Option<i64>,
        end_timestamp: Option<i64>,
    ) -> Result<i64> {
        let query = if let (Some(start), Some(end)) = (start_timestamp, end_timestamp) {
            sqlx::query_scalar::<_, i64>(
                r"
                SELECT COUNT(DISTINCT url)::bigint
                FROM frame_actions
                WHERE fid = $1 
                  AND timestamp >= $2 
                  AND timestamp <= $3
                ",
            )
            .bind(fid)
            .bind(start)
            .bind(end)
        } else if let Some(start) = start_timestamp {
            sqlx::query_scalar::<_, i64>(
                r"
                SELECT COUNT(DISTINCT url)::bigint
                FROM frame_actions
                WHERE fid = $1 
                  AND timestamp >= $2
                ",
            )
            .bind(fid)
            .bind(start)
        } else if let Some(end) = end_timestamp {
            sqlx::query_scalar::<_, i64>(
                r"
                SELECT COUNT(DISTINCT url)::bigint
                FROM frame_actions
                WHERE fid = $1 
                  AND timestamp <= $2
                ",
            )
            .bind(fid)
            .bind(end)
        } else {
            sqlx::query_scalar::<_, i64>(
                r"
                SELECT COUNT(DISTINCT url)::bigint
                FROM frame_actions
                WHERE fid = $1
                ",
            )
            .bind(fid)
        };

        let count = query.fetch_one(&self.pool).await?;
        Ok(count)
    }

    /// Get casts text for emoji analysis
    ///
    /// Returns all cast texts for a user within the time range.
    /// This is used to extract emojis in the application layer.
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
    pub async fn get_casts_text_for_analysis(
        &self,
        fid: i64,
        start_timestamp: Option<i64>,
        end_timestamp: Option<i64>,
    ) -> Result<Vec<String>> {
        let query = if let (Some(start), Some(end)) = (start_timestamp, end_timestamp) {
            sqlx::query_scalar::<_, Option<String>>(
                r"
                SELECT text
                FROM casts
                WHERE fid = $1 
                  AND timestamp >= $2 
                  AND timestamp <= $3
                  AND text IS NOT NULL
                  AND text != ''
                ",
            )
            .bind(fid)
            .bind(start)
            .bind(end)
        } else if let Some(start) = start_timestamp {
            sqlx::query_scalar::<_, Option<String>>(
                r"
                SELECT text
                FROM casts
                WHERE fid = $1 
                  AND timestamp >= $2
                  AND text IS NOT NULL
                  AND text != ''
                ",
            )
            .bind(fid)
            .bind(start)
        } else if let Some(end) = end_timestamp {
            sqlx::query_scalar::<_, Option<String>>(
                r"
                SELECT text
                FROM casts
                WHERE fid = $1 
                  AND timestamp <= $2
                  AND text IS NOT NULL
                  AND text != ''
                ",
            )
            .bind(fid)
            .bind(end)
        } else {
            sqlx::query_scalar::<_, Option<String>>(
                r"
                SELECT text
                FROM casts
                WHERE fid = $1
                  AND text IS NOT NULL
                  AND text != ''
                ",
            )
            .bind(fid)
        };

        let results = query.fetch_all(&self.pool).await?;
        Ok(results.into_iter().flatten().collect())
    }
}
