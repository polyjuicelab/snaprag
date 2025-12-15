//! Prometheus metrics for SnapRAG monitoring
//!
//! This module exports metrics for:
//! - Sync progress and status
//! - Database statistics
//! - API request metrics
//! - Cache performance
//! - System resources

use std::sync::Arc;
use std::sync::Mutex;

use prometheus::register_counter;
use prometheus::register_counter_vec;
use prometheus::register_gauge;
use prometheus::register_gauge_vec;
use prometheus::register_histogram;
use prometheus::register_histogram_vec;
use prometheus::Counter;
use prometheus::CounterVec;
use prometheus::Encoder;
use prometheus::Gauge;
use prometheus::GaugeVec;
use prometheus::Histogram;
use prometheus::HistogramOpts;
use prometheus::HistogramVec;
use prometheus::Opts;
use prometheus::Registry;
use prometheus::TextEncoder;

/// Metrics registry and collectors
pub struct Metrics {
    pub registry: Registry,

    // Sync metrics
    pub sync_blocks_processed: Counter,
    pub sync_messages_processed: Counter,
    pub sync_current_height: GaugeVec,
    pub sync_status: GaugeVec,
    pub sync_errors: Counter,
    pub sync_duration: Histogram,

    // Database metrics
    pub db_total_profiles: Gauge,
    pub db_total_casts: Gauge,
    pub db_profiles_with_embeddings: Gauge,
    pub db_casts_with_embeddings: Gauge,
    pub db_connection_pool_size: Gauge,
    pub db_query_duration: Histogram,

    // API metrics
    pub api_requests_total: Counter,
    pub api_request_duration: HistogramVec,
    pub api_errors_total: CounterVec,

    // Cache metrics
    pub cache_hits: Counter,
    pub cache_misses: Counter,
    pub cache_evictions: Counter,
    pub cache_size: Gauge,

    // System metrics (from prometheus process collector)
    pub process_cpu_seconds_total: Gauge,
    pub process_resident_memory_bytes: Gauge,
}

impl Metrics {
    /// Create a new metrics instance
    pub fn new() -> Result<Self, prometheus::Error> {
        let registry = Registry::new();

        // Sync metrics
        let sync_blocks_processed = register_counter!(Opts::new(
            "snaprag_sync_blocks_processed_total",
            "Total number of blocks processed during sync"
        ))?;

        let sync_messages_processed = register_counter!(Opts::new(
            "snaprag_sync_messages_processed_total",
            "Total number of messages processed during sync"
        ))?;

        let sync_current_height = register_gauge_vec!(
            Opts::new(
                "snaprag_sync_current_height",
                "Current block height being processed per shard"
            ),
            &["shard_id"]
        )?;

        let sync_status = register_gauge_vec!(
            Opts::new(
                "snaprag_sync_status",
                "Sync status (1=running, 0=stopped) per shard"
            ),
            &["shard_id", "status"]
        )?;

        let sync_errors = register_counter!(Opts::new(
            "snaprag_sync_errors_total",
            "Total number of sync errors"
        ))?;

        let sync_duration = register_histogram!(HistogramOpts::new(
            "snaprag_sync_duration_seconds",
            "Time spent processing sync batches"
        )
        .buckets(vec![0.1, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0]))?;

        // Database metrics
        let db_total_profiles = register_gauge!(Opts::new(
            "snaprag_db_total_profiles",
            "Total number of user profiles in database"
        ))?;

        let db_total_casts = register_gauge!(Opts::new(
            "snaprag_db_total_casts",
            "Total number of casts in database"
        ))?;

        let db_profiles_with_embeddings = register_gauge!(Opts::new(
            "snaprag_db_profiles_with_embeddings",
            "Number of profiles with embeddings"
        ))?;

        let db_casts_with_embeddings = register_gauge!(Opts::new(
            "snaprag_db_casts_with_embeddings",
            "Number of casts with embeddings"
        ))?;

        let db_connection_pool_size = register_gauge!(Opts::new(
            "snaprag_db_connection_pool_size",
            "Current database connection pool size"
        ))?;

        let db_query_duration = register_histogram!(HistogramOpts::new(
            "snaprag_db_query_duration_seconds",
            "Database query duration"
        )
        .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 2.0, 5.0]))?;

        // API metrics
        let api_requests_total = register_counter!(Opts::new(
            "snaprag_api_requests_total",
            "Total number of API requests"
        ))?;

        let api_request_duration = register_histogram_vec!(
            HistogramOpts::new(
                "snaprag_api_request_duration_seconds",
                "API request duration"
            )
            .buckets(vec![0.01, 0.05, 0.1, 0.5, 1.0, 2.0, 5.0, 10.0]),
            &["method", "endpoint", "status"]
        )?;

        let api_errors_total = register_counter_vec!(
            Opts::new("snaprag_api_errors_total", "Total number of API errors"),
            &["method", "endpoint", "status"]
        )?;

        // Cache metrics
        let cache_hits = register_counter!(Opts::new(
            "snaprag_cache_hits_total",
            "Total number of cache hits"
        ))?;

        let cache_misses = register_counter!(Opts::new(
            "snaprag_cache_misses_total",
            "Total number of cache misses"
        ))?;

        let cache_evictions = register_counter!(Opts::new(
            "snaprag_cache_evictions_total",
            "Total number of cache evictions"
        ))?;

        let cache_size = register_gauge!(Opts::new(
            "snaprag_cache_size",
            "Current cache size (number of entries)"
        ))?;

        // System metrics (will be populated by process collector)
        let process_cpu_seconds_total = register_gauge!(Opts::new(
            "process_cpu_seconds_total",
            "Total CPU time used by the process"
        ))?;

        let process_resident_memory_bytes = register_gauge!(Opts::new(
            "process_resident_memory_bytes",
            "Resident memory size in bytes"
        ))?;

        // Register all metrics
        registry.register(Box::new(sync_blocks_processed.clone()))?;
        registry.register(Box::new(sync_messages_processed.clone()))?;
        registry.register(Box::new(sync_current_height.clone()))?;
        registry.register(Box::new(sync_status.clone()))?;
        registry.register(Box::new(sync_errors.clone()))?;
        registry.register(Box::new(sync_duration.clone()))?;
        registry.register(Box::new(db_total_profiles.clone()))?;
        registry.register(Box::new(db_total_casts.clone()))?;
        registry.register(Box::new(db_profiles_with_embeddings.clone()))?;
        registry.register(Box::new(db_casts_with_embeddings.clone()))?;
        registry.register(Box::new(db_connection_pool_size.clone()))?;
        registry.register(Box::new(db_query_duration.clone()))?;
        registry.register(Box::new(api_requests_total.clone()))?;
        registry.register(Box::new(api_request_duration.clone()))?;
        registry.register(Box::new(api_errors_total.clone()))?;
        registry.register(Box::new(cache_hits.clone()))?;
        registry.register(Box::new(cache_misses.clone()))?;
        registry.register(Box::new(cache_evictions.clone()))?;
        registry.register(Box::new(cache_size.clone()))?;

        // Note: Process collector is registered in default registry automatically
        // when prometheus crate is used with "process" feature

        Ok(Self {
            registry,
            sync_blocks_processed,
            sync_messages_processed,
            sync_current_height,
            sync_status,
            sync_errors,
            sync_duration,
            db_total_profiles,
            db_total_casts,
            db_profiles_with_embeddings,
            db_casts_with_embeddings,
            db_connection_pool_size,
            db_query_duration,
            api_requests_total,
            api_request_duration,
            api_errors_total,
            cache_hits,
            cache_misses,
            cache_evictions,
            cache_size,
            process_cpu_seconds_total,
            process_resident_memory_bytes,
        })
    }

    /// Export metrics in Prometheus text format
    pub fn export(&self) -> Result<String, prometheus::Error> {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer)?;
        Ok(String::from_utf8_lossy(&buffer).to_string())
    }

    /// Update sync metrics from database
    pub async fn update_sync_metrics(&self, database: &crate::database::Database) {
        // Get sync progress from database
        if let Ok(stats) = database.get_sync_stats().await {
            for stat in stats {
                let shard_id = stat.shard_id.to_string();

                // Update current height
                if let Some(height) = stat.last_processed_height {
                    self.sync_current_height
                        .with_label_values(&[&shard_id])
                        .set(height as f64);
                }

                // Update status (1 = syncing, 0 = stopped)
                let status_value = if stat.status.as_deref() == Some("syncing") {
                    1.0
                } else {
                    0.0
                };
                self.sync_status
                    .with_label_values(&[&shard_id, stat.status.as_deref().unwrap_or("unknown")])
                    .set(status_value);
            }
        }
    }

    /// Update database metrics
    pub async fn update_db_metrics(&self, database: &crate::database::Database) {
        let pool = database.pool();

        // Get total profiles
        if let Ok(count) =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(DISTINCT fid) FROM user_profile_changes")
                .fetch_one(pool)
                .await
        {
            self.db_total_profiles.set(count as f64);
        }

        // Get total casts
        if let Ok(count) = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM casts")
            .fetch_one(pool)
            .await
        {
            self.db_total_casts.set(count as f64);
        }

        // Get profiles with embeddings
        if let Ok(count) = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM profile_embeddings WHERE profile_embedding IS NOT NULL",
        )
        .fetch_one(pool)
        .await
        {
            self.db_profiles_with_embeddings.set(count as f64);
        }

        // Get casts with embeddings
        if let Ok(count) = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM cast_embeddings")
            .fetch_one(pool)
            .await
        {
            self.db_casts_with_embeddings.set(count as f64);
        }

        // Get connection pool size
        self.db_connection_pool_size.set(f64::from(pool.size()));
    }
}

// Global metrics instance
lazy_static::lazy_static! {
    pub static ref METRICS: Arc<Mutex<Option<Arc<Metrics>>>> = Arc::new(Mutex::new(None));
}

/// Initialize metrics
pub fn init_metrics() -> Result<Arc<Metrics>, prometheus::Error> {
    let metrics = Arc::new(Metrics::new()?);
    *METRICS.lock().unwrap() = Some(metrics.clone());
    Ok(metrics)
}

/// Get metrics instance
pub fn get_metrics() -> Option<Arc<Metrics>> {
    METRICS.lock().unwrap().as_ref().map(Arc::clone)
}
