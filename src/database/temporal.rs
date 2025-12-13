//! Temporal activity analysis database queries
//!
//! This module provides queries for analyzing user activity over time including:
//! - Hourly distribution of casts
//! - Monthly distribution of casts
//! - First and last casts in a time range

use super::Database;
use crate::models::Cast;
use crate::Result;

/// Hour count entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HourCount {
    pub hour: i32, // 0-23
    pub count: i64,
}

/// Month count entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MonthCount {
    pub month: String, // "YYYY-MM"
    pub count: i64,
}

impl Database {
    /// Get hourly distribution of casts
    ///
    /// Groups casts by hour of day (0-23) within the specified time range.
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
    pub async fn get_hourly_distribution(
        &self,
        fid: i64,
        start_timestamp: Option<i64>,
        end_timestamp: Option<i64>,
    ) -> Result<Vec<HourCount>> {
        #[derive(sqlx::FromRow)]
        struct RawResult {
            hour: i32,
            count: i64,
        }

        let query = if let (Some(start), Some(end)) = (start_timestamp, end_timestamp) {
            sqlx::query_as::<_, RawResult>(
                r"
                SELECT 
                  EXTRACT(HOUR FROM to_timestamp(
                    (1609459200 + c.timestamp)::bigint
                  ))::int as hour,
                  COUNT(*)::bigint as count
                FROM casts c
                WHERE c.fid = $1 
                  AND c.timestamp >= $2 
                  AND c.timestamp <= $3
                GROUP BY hour
                ORDER BY hour
                ",
            )
            .bind(fid)
            .bind(start)
            .bind(end)
        } else if let Some(start) = start_timestamp {
            sqlx::query_as::<_, RawResult>(
                r"
                SELECT 
                  EXTRACT(HOUR FROM to_timestamp(
                    (1609459200 + c.timestamp)::bigint
                  ))::int as hour,
                  COUNT(*)::bigint as count
                FROM casts c
                WHERE c.fid = $1 
                  AND c.timestamp >= $2
                GROUP BY hour
                ORDER BY hour
                ",
            )
            .bind(fid)
            .bind(start)
        } else if let Some(end) = end_timestamp {
            sqlx::query_as::<_, RawResult>(
                r"
                SELECT 
                  EXTRACT(HOUR FROM to_timestamp(
                    (1609459200 + c.timestamp)::bigint
                  ))::int as hour,
                  COUNT(*)::bigint as count
                FROM casts c
                WHERE c.fid = $1 
                  AND c.timestamp <= $2
                GROUP BY hour
                ORDER BY hour
                ",
            )
            .bind(fid)
            .bind(end)
        } else {
            sqlx::query_as::<_, RawResult>(
                r"
                SELECT 
                  EXTRACT(HOUR FROM to_timestamp(
                    (1609459200 + c.timestamp)::bigint
                  ))::int as hour,
                  COUNT(*)::bigint as count
                FROM casts c
                WHERE c.fid = $1
                GROUP BY hour
                ORDER BY hour
                ",
            )
            .bind(fid)
        };

        let results = query.fetch_all(&self.pool).await?;

        Ok(results
            .into_iter()
            .map(|r| HourCount {
                hour: r.hour,
                count: r.count,
            })
            .collect())
    }

    /// Get monthly distribution of casts
    ///
    /// Groups casts by month (YYYY-MM format) within the specified time range.
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
    pub async fn get_monthly_distribution(
        &self,
        fid: i64,
        start_timestamp: Option<i64>,
        end_timestamp: Option<i64>,
    ) -> Result<Vec<MonthCount>> {
        #[derive(sqlx::FromRow)]
        struct RawResult {
            month: String,
            count: i64,
        }

        let query = if let (Some(start), Some(end)) = (start_timestamp, end_timestamp) {
            sqlx::query_as::<_, RawResult>(
                r"
                SELECT 
                  TO_CHAR(to_timestamp(
                    (1609459200 + c.timestamp)::bigint
                  ), 'YYYY-MM') as month,
                  COUNT(*)::bigint as count
                FROM casts c
                WHERE c.fid = $1 
                  AND c.timestamp >= $2 
                  AND c.timestamp <= $3
                GROUP BY month
                ORDER BY month
                ",
            )
            .bind(fid)
            .bind(start)
            .bind(end)
        } else if let Some(start) = start_timestamp {
            sqlx::query_as::<_, RawResult>(
                r"
                SELECT 
                  TO_CHAR(to_timestamp(
                    (1609459200 + c.timestamp)::bigint
                  ), 'YYYY-MM') as month,
                  COUNT(*)::bigint as count
                FROM casts c
                WHERE c.fid = $1 
                  AND c.timestamp >= $2
                GROUP BY month
                ORDER BY month
                ",
            )
            .bind(fid)
            .bind(start)
        } else if let Some(end) = end_timestamp {
            sqlx::query_as::<_, RawResult>(
                r"
                SELECT 
                  TO_CHAR(to_timestamp(
                    (1609459200 + c.timestamp)::bigint
                  ), 'YYYY-MM') as month,
                  COUNT(*)::bigint as count
                FROM casts c
                WHERE c.fid = $1 
                  AND c.timestamp <= $2
                GROUP BY month
                ORDER BY month
                ",
            )
            .bind(fid)
            .bind(end)
        } else {
            sqlx::query_as::<_, RawResult>(
                r"
                SELECT 
                  TO_CHAR(to_timestamp(
                    (1609459200 + c.timestamp)::bigint
                  ), 'YYYY-MM') as month,
                  COUNT(*)::bigint as count
                FROM casts c
                WHERE c.fid = $1
                GROUP BY month
                ORDER BY month
                ",
            )
            .bind(fid)
        };

        let results = query.fetch_all(&self.pool).await?;

        Ok(results
            .into_iter()
            .map(|r| MonthCount {
                month: r.month,
                count: r.count,
            })
            .collect())
    }

    /// Get first cast in time range
    ///
    /// Returns the cast with the earliest timestamp within the specified range.
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
    pub async fn get_first_cast_in_range(
        &self,
        fid: i64,
        start_timestamp: Option<i64>,
        end_timestamp: Option<i64>,
    ) -> Result<Option<Cast>> {
        let cast = if let (Some(start), Some(end)) = (start_timestamp, end_timestamp) {
            sqlx::query_as::<_, Cast>(
                r"
                SELECT * FROM casts
                WHERE fid = $1 
                  AND timestamp >= $2 
                  AND timestamp <= $3
                ORDER BY timestamp ASC
                LIMIT 1
                ",
            )
            .bind(fid)
            .bind(start)
            .bind(end)
            .fetch_optional(&self.pool)
            .await?
        } else if let Some(start) = start_timestamp {
            sqlx::query_as::<_, Cast>(
                r"
                SELECT * FROM casts
                WHERE fid = $1 
                  AND timestamp >= $2
                ORDER BY timestamp ASC
                LIMIT 1
                ",
            )
            .bind(fid)
            .bind(start)
            .fetch_optional(&self.pool)
            .await?
        } else if let Some(end) = end_timestamp {
            sqlx::query_as::<_, Cast>(
                r"
                SELECT * FROM casts
                WHERE fid = $1 
                  AND timestamp <= $2
                ORDER BY timestamp ASC
                LIMIT 1
                ",
            )
            .bind(fid)
            .bind(end)
            .fetch_optional(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, Cast>(
                r"
                SELECT * FROM casts
                WHERE fid = $1
                ORDER BY timestamp ASC
                LIMIT 1
                ",
            )
            .bind(fid)
            .fetch_optional(&self.pool)
            .await?
        };

        Ok(cast)
    }

    /// Get last cast in time range
    ///
    /// Returns the cast with the latest timestamp within the specified range.
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
    pub async fn get_last_cast_in_range(
        &self,
        fid: i64,
        start_timestamp: Option<i64>,
        end_timestamp: Option<i64>,
    ) -> Result<Option<Cast>> {
        let cast = if let (Some(start), Some(end)) = (start_timestamp, end_timestamp) {
            sqlx::query_as::<_, Cast>(
                r"
                SELECT * FROM casts
                WHERE fid = $1 
                  AND timestamp >= $2 
                  AND timestamp <= $3
                ORDER BY timestamp DESC
                LIMIT 1
                ",
            )
            .bind(fid)
            .bind(start)
            .bind(end)
            .fetch_optional(&self.pool)
            .await?
        } else if let Some(start) = start_timestamp {
            sqlx::query_as::<_, Cast>(
                r"
                SELECT * FROM casts
                WHERE fid = $1 
                  AND timestamp >= $2
                ORDER BY timestamp DESC
                LIMIT 1
                ",
            )
            .bind(fid)
            .bind(start)
            .fetch_optional(&self.pool)
            .await?
        } else if let Some(end) = end_timestamp {
            sqlx::query_as::<_, Cast>(
                r"
                SELECT * FROM casts
                WHERE fid = $1 
                  AND timestamp <= $2
                ORDER BY timestamp DESC
                LIMIT 1
                ",
            )
            .bind(fid)
            .bind(end)
            .fetch_optional(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, Cast>(
                r"
                SELECT * FROM casts
                WHERE fid = $1
                ORDER BY timestamp DESC
                LIMIT 1
                ",
            )
            .bind(fid)
            .fetch_optional(&self.pool)
            .await?
        };

        Ok(cast)
    }
}
