//! Database layer for `PostgreSQL` operations
//!
//! This module provides a comprehensive database abstraction layer for all data operations
//! in `SnapRAG`. It handles:
//!
//! - User profiles and activity tracking
//! - Casts (posts) and reactions
//! - Links (follows) and social relationships
//! - Embeddings storage and vector search
//! - Event sourcing and change tracking
//! - Sync state management
//!
//! # Architecture
//!
//! The database module uses an event-sourcing pattern for user data:
//! - All changes are recorded in `*_changes` tables
//! - Materialized views provide current state
//! - Window functions handle deduplication
//!
//! # Connection Pool
//!
//! Connection pooling is managed by `sqlx::PgPool` with configurable:
//! - Maximum connections
//! - Minimum connections
//! - Connection timeout
//!
//! # Examples
//!
//! ```rust,no_run
//! use snaprag::{Database, AppConfig};
//!
//! # async fn example() -> snaprag::Result<()> {
//! let config = AppConfig::load()?;
//! let database = Database::from_config(&config).await?;
//!
//! // Initialize schema
//! database.init_schema().await?;
//!
//! // Query user profile
//! if let Some(profile) = database.get_user_profile(3).await? {
//!     println!("User: {}", profile.username.unwrap_or_default());
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Modules
//!
//! - `casts`: Cast storage and retrieval operations
//! - `links`: Social link management (follows, etc.)
//! - `schema`: Database schema initialization and validation
//! - `sync`: Sync state tracking
//! - `user_activity`: Activity timeline queries
//! - `user_data`: User data change tracking
//! - `user_profiles`: Profile queries and updates
//! - `user_snapshots`: Historical profile snapshots

use sqlx::PgPool;

use crate::models::*;
use crate::Result;
use crate::SnapRagError;

// Re-export submodules
mod casts;
mod links;
mod schema;
mod sync;
mod user_activity;
mod user_data;
mod user_data_changes;
mod user_profiles;
mod user_snapshots;
mod username_proofs;
mod verifications;

// Re-export public types
pub use casts::CastThread;
pub use sync::SyncStats;

/// Database connection pool wrapper
///
/// Provides a high-level interface to `PostgreSQL` operations with connection pooling.
///
/// # Thread Safety
///
/// This type is `Clone` and thread-safe. Cloning creates a new reference to the same
/// connection pool (cheap operation).
#[derive(Debug, Clone)]
pub struct Database {
    pool: PgPool,
}

impl Database {
    /// Create a new database instance with an existing pool
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use sqlx::PgPool;
    /// use snaprag::Database;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let pool = PgPool::connect("postgresql://localhost/snaprag").await?;
    /// let database = Database::new(pool);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new database instance from configuration
    ///
    /// Establishes a connection pool with the configured parameters.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Database connection fails
    /// - Configuration is invalid
    /// - Network issues prevent connection
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use snaprag::{Database, AppConfig};
    ///
    /// # async fn example() -> snaprag::Result<()> {
    /// let config = AppConfig::load()?;
    /// let database = Database::from_config(&config).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn from_config(config: &crate::config::AppConfig) -> Result<Self> {
        let pool_options = sqlx::postgres::PgPoolOptions::new()
            .max_connections(config.max_connections())
            .min_connections(config.min_connections())
            .acquire_timeout(std::time::Duration::from_secs(config.connection_timeout()));

        let pool = pool_options.connect(config.database_url()).await?;

        tracing::debug!(
            "Database pool configured: max_connections={}, min_connections={}",
            config.max_connections(),
            config.min_connections()
        );

        Ok(Self::new(pool))
    }

    /// Run database migrations
    ///
    /// Note: Migrations are currently managed manually via SQL files in `/migrations`.
    /// Future enhancement: Could integrate with sqlx migrations or refinery.
    pub const fn migrate(&self) -> Result<()> {
        Ok(())
    }

    /// Get a reference to the database pool for raw queries
    ///
    /// Use this when you need direct access to the underlying `sqlx::PgPool`
    /// for custom queries or transactions.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use snaprag::Database;
    ///
    /// # async fn example(database: &Database) -> Result<(), Box<dyn std::error::Error>> {
    /// let result = sqlx::query("SELECT COUNT(*) FROM casts")
    ///     .fetch_one(database.pool())
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn pool(&self) -> &sqlx::PgPool {
        &self.pool
    }

    /// Get long-running queries that may be stuck
    ///
    /// Returns queries that have been running for more than the specified duration (in seconds).
    /// Excludes system processes like autovacuum.
    ///
    /// # Arguments
    ///
    /// * `min_duration_secs` - Minimum query duration in seconds to consider as "long-running" (default: 30)
    ///
    /// # Returns
    ///
    /// Vector of tuples: (pid, duration_secs, state, query, application_name, client_addr)
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use snaprag::Database;
    ///
    /// # async fn example(database: &Database) -> snaprag::Result<()> {
    /// let stuck_queries = database.get_long_running_queries(30).await?;
    /// for (pid, duration, state, query, app, addr) in stuck_queries {
    ///     println!("PID {}: running for {}s - {}", pid, duration, query);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_long_running_queries(
        &self,
        min_duration_secs: i64,
    ) -> Result<Vec<(i32, i64, String, String, Option<String>, Option<String>)>> {
        let rows = sqlx::query_as::<_, (i32, i64, String, String, Option<String>, Option<String>)>(
            r#"
            SELECT 
                pid,
                EXTRACT(EPOCH FROM (NOW() - query_start))::bigint as duration_secs,
                state,
                COALESCE(SUBSTRING(query, 1, 200), '') as query_preview,
                application_name,
                client_addr::text
            FROM pg_stat_activity
            WHERE 
                datname = current_database()
                AND pid != pg_backend_pid()
                AND state != 'idle'
                AND state != 'idle in transaction (aborted)'
                AND query NOT LIKE '%pg_stat_activity%'
                AND EXTRACT(EPOCH FROM (NOW() - query_start)) > $1
            ORDER BY query_start ASC
            "#,
        )
        .bind(min_duration_secs)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }
}
