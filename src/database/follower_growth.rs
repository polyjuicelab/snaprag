//! Follower growth tracking database queries
//!
//! This module provides queries for tracking follower and following growth over time.

use chrono::Datelike;

use super::Database;
use crate::Result;

/// Monthly follower snapshot
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MonthlyFollowerSnapshot {
    pub month: String, // "YYYY-MM"
    pub followers: i64,
    pub following: i64,
}

impl Database {
    /// Get current follower count
    ///
    /// Counts active followers (links where target_fid = fid and event_type = 'add').
    ///
    /// # Arguments
    ///
    /// * `fid` - User's FID
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn get_current_follower_count(&self, fid: i64) -> Result<i64> {
        let count: (i64,) = sqlx::query_as(
            r"
            SELECT COUNT(DISTINCT l.fid)::bigint
            FROM (
              SELECT DISTINCT ON (fid, target_fid) *
              FROM links
              WHERE target_fid = $1 
                AND link_type = 'follow'
              ORDER BY fid, target_fid, timestamp DESC
            ) l
            WHERE l.event_type = 'add'
            ",
        )
        .bind(fid)
        .fetch_one(&self.pool)
        .await?;

        Ok(count.0)
    }

    /// Get current following count
    ///
    /// Counts active following (links where fid = fid and event_type = 'add').
    ///
    /// # Arguments
    ///
    /// * `fid` - User's FID
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn get_current_following_count(&self, fid: i64) -> Result<i64> {
        let count: (i64,) = sqlx::query_as(
            r"
            SELECT COUNT(DISTINCT l.target_fid)::bigint
            FROM (
              SELECT DISTINCT ON (fid, target_fid) *
              FROM links
              WHERE fid = $1 
                AND link_type = 'follow'
              ORDER BY fid, target_fid, timestamp DESC
            ) l
            WHERE l.event_type = 'add'
            ",
        )
        .bind(fid)
        .fetch_one(&self.pool)
        .await?;

        Ok(count.0)
    }

    /// Get follower count at a specific timestamp
    ///
    /// Reconstructs the follower count by counting all 'add' events that occurred
    /// before or at the timestamp, excluding any 'remove' events that occurred after.
    ///
    /// # Arguments
    ///
    /// * `fid` - User's FID
    /// * `timestamp` - Farcaster timestamp to check at
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn get_follower_count_at_timestamp(&self, fid: i64, timestamp: i64) -> Result<i64> {
        let count: (i64,) = sqlx::query_as(
            r"
            SELECT COUNT(DISTINCT l.fid)::bigint
            FROM (
              SELECT DISTINCT ON (fid, target_fid) *
              FROM links
              WHERE target_fid = $1 
                AND link_type = 'follow'
                AND timestamp <= $2
              ORDER BY fid, target_fid, timestamp DESC
            ) l
            WHERE l.event_type = 'add'
            ",
        )
        .bind(fid)
        .bind(timestamp)
        .fetch_one(&self.pool)
        .await?;

        Ok(count.0)
    }

    /// Get following count at a specific timestamp
    ///
    /// Reconstructs the following count by counting all 'add' events that occurred
    /// before or at the timestamp, excluding any 'remove' events that occurred after.
    ///
    /// # Arguments
    ///
    /// * `fid` - User's FID
    /// * `timestamp` - Farcaster timestamp to check at
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn get_following_count_at_timestamp(&self, fid: i64, timestamp: i64) -> Result<i64> {
        let count: (i64,) = sqlx::query_as(
            r"
            SELECT COUNT(DISTINCT l.target_fid)::bigint
            FROM (
              SELECT DISTINCT ON (fid, target_fid) *
              FROM links
              WHERE fid = $1 
                AND link_type = 'follow'
                AND timestamp <= $2
              ORDER BY fid, target_fid, timestamp DESC
            ) l
            WHERE l.event_type = 'add'
            ",
        )
        .bind(fid)
        .bind(timestamp)
        .fetch_one(&self.pool)
        .await?;

        Ok(count.0)
    }

    /// Get monthly follower snapshots
    ///
    /// Returns follower and following counts at the end of each month within the time range.
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
    ///
    /// # Note
    ///
    /// This is an expensive operation as it reconstructs state for each month.
    /// Consider caching the results.
    pub async fn get_monthly_follower_snapshots(
        &self,
        fid: i64,
        start_timestamp: Option<i64>,
        end_timestamp: Option<i64>,
    ) -> Result<Vec<MonthlyFollowerSnapshot>> {
        // For now, we'll generate monthly timestamps and query each one
        // This could be optimized with a more complex SQL query, but this approach
        // is clearer and easier to maintain

        // Get the time range
        let start = start_timestamp.unwrap_or(0);
        let end = end_timestamp.unwrap_or_else(|| {
            // Use current Farcaster timestamp if not specified
            #[allow(clippy::cast_sign_loss)]
            let unix_now = chrono::Utc::now().timestamp() as u64;
            crate::unix_to_farcaster_timestamp(unix_now) as i64
        });

        // Generate monthly timestamps
        let mut snapshots = Vec::new();
        let mut current = start;

        while current <= end {
            // Get the end of the current month
            #[allow(clippy::cast_sign_loss)]
            let unix_ts = crate::farcaster_to_unix_timestamp(current as u64) as i64;
            let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(unix_ts, 0)
                .unwrap_or_else(chrono::Utc::now);

            // Get the last day of the month
            let date = dt.date_naive();
            let year = date.year();
            let month = date.month();
            let last_day = chrono::NaiveDate::from_ymd_opt(year, month, 1)
                .and_then(|d| d.with_day(1))
                .and_then(|d| d.with_month(month + 1))
                .and_then(|d| d.pred_opt())
                .map_or(28, |d| d.day());

            let month_end = chrono::NaiveDate::from_ymd_opt(year, month, last_day)
                .and_then(|d| d.and_hms_opt(23, 59, 59))
                .and_then(|d| d.and_local_timezone(chrono::Utc).single())
                .map_or(unix_ts, |dt| dt.timestamp());

            #[allow(clippy::cast_sign_loss)]
            let month_end_farcaster = crate::unix_to_farcaster_timestamp(month_end as u64) as i64;

            // Get counts at end of month
            let followers = self
                .get_follower_count_at_timestamp(fid, month_end_farcaster)
                .await
                .unwrap_or(0);
            let following = self
                .get_following_count_at_timestamp(fid, month_end_farcaster)
                .await
                .unwrap_or(0);

            let month_str = format!("{year:04}-{month:02}");
            snapshots.push(MonthlyFollowerSnapshot {
                month: month_str,
                followers,
                following,
            });

            // Move to next month
            if month == 12 {
                current = crate::unix_to_farcaster_timestamp(
                    chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)
                        .and_then(|d| d.and_hms_opt(0, 0, 0))
                        .and_then(|d| d.and_local_timezone(chrono::Utc).single())
                        .map_or(current as u64, |dt| dt.timestamp() as u64),
                ) as i64;
            } else {
                current = crate::unix_to_farcaster_timestamp(
                    chrono::NaiveDate::from_ymd_opt(year, month + 1, 1)
                        .and_then(|d| d.and_hms_opt(0, 0, 0))
                        .and_then(|d| d.and_local_timezone(chrono::Utc).single())
                        .map_or(current as u64, |dt| dt.timestamp() as u64),
                ) as i64;
            }
        }

        Ok(snapshots)
    }
}
