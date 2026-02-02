use std::sync::Arc;

use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::warn;

use super::state::ChunkProcessStats;
use crate::config::AppConfig;
use crate::database::Database;
use crate::sync::client::SnapchainClient;
use crate::sync::hooks::HookManager;
use crate::sync::lock_file::SyncLockFile;
use crate::sync::lock_file::SyncLockManager;
use crate::sync::lock_file::SyncRange;
use crate::sync::shard_processor::ShardProcessor;
use crate::sync::state_manager::SyncStateManager;
use crate::sync::types::SyncConfig;
use crate::sync::types::SyncState;
use crate::Result;

/// Lifecycle management for sync service
pub struct LifecycleManager {
    config: SyncConfig,
    client: SnapchainClient,
    database: Arc<Database>,
    state: Arc<tokio::sync::RwLock<SyncState>>,
    state_manager: Arc<tokio::sync::RwLock<SyncStateManager>>,
    lock_manager: SyncLockManager,
    hook_manager: Option<Arc<HookManager>>,
}

impl LifecycleManager {
    pub const fn new(
        config: SyncConfig,
        client: SnapchainClient,
        database: Arc<Database>,
        state: Arc<tokio::sync::RwLock<SyncState>>,
        state_manager: Arc<tokio::sync::RwLock<SyncStateManager>>,
        lock_manager: SyncLockManager,
        hook_manager: Option<Arc<HookManager>>,
    ) -> Self {
        Self {
            config,
            client,
            database,
            state,
            state_manager,
            lock_manager,
            hook_manager,
        }
    }

    fn processor(&self, database: &Database) -> ShardProcessor {
        if let Some(ref hm) = self.hook_manager {
            ShardProcessor::with_hooks(database.clone(), hm.clone())
        } else {
            ShardProcessor::new(database.clone())
        }
    }

    /// Start the sync service
    pub async fn start(&self) -> Result<()> {
        info!("Starting SnapRAG sync service...");
        info!("Sync configuration: {:?}", self.config);

        // Always start with historical sync for full data import
        if self.config.enable_historical_sync {
            info!("Starting full historical data sync from genesis...");
            self.start_full_historical_sync().await?;
        }

        // Then start real-time sync for new data
        if self.config.enable_realtime_sync {
            info!("Starting real-time sync for new data...");
            self.start_full_realtime_sync();
        }

        Ok(())
    }

    /// Start the sync service with a specific block range
    pub async fn start_with_range(&self, from_block: u64, to_block: u64) -> Result<()> {
        // Validate range parameters
        if from_block > to_block {
            return Err(crate::SnapRagError::Custom(format!(
                "Invalid range: from_block ({from_block}) cannot be greater than to_block ({to_block})"
            )));
        }

        info!(
            "Starting SnapRAG sync service with range {} to {}...",
            from_block, to_block
        );
        info!("Sync configuration: {:?}", self.config);

        // Create lock file for this sync process
        let sync_range = SyncRange {
            from_block,
            to_block: if to_block == u64::MAX {
                None
            } else {
                Some(to_block)
            },
        };
        let mut lock = self.lock_manager.create_lock("running", Some(sync_range))?;

        // Start historical sync with the specified range
        if self.config.enable_historical_sync {
            info!(
                "Starting historical data sync from block {} to block {}...",
                from_block, to_block
            );
            self.start_historical_sync_with_range(from_block, to_block, &lock)
                .await?;
        }

        // Update lock file to completed status
        lock.update_status("completed");
        self.lock_manager.update_lock(lock)?;

        // Note: We don't start real-time sync when using a range, as it's typically for testing
        info!("Range sync completed. Use 'sync start' without range for continuous sync.");

        Ok(())
    }

    /// Stop the sync service
    pub async fn stop(&self, force: bool) -> Result<()> {
        info!("Stopping SnapRAG sync service...");

        // Update state to stopping
        {
            let mut state = self.state.write().await;
            state.status = crate::sync::types::SyncStatus::Paused;
        }

        // Update lock file to stopped status
        if let Ok(lock) = self.lock_manager.read_lock() {
            let mut lock = lock;
            if force {
                lock.update_status("force_stopped");
            } else {
                lock.update_status("stopped");
            }
            let _ = self.lock_manager.update_lock(lock);
        }

        info!("Sync service stopped");
        Ok(())
    }

    /// Get sync status
    pub fn get_sync_status(&self) -> Result<Option<SyncLockFile>> {
        match self.lock_manager.read_lock() {
            Ok(lock) => Ok(Some(lock)),
            Err(_) => Ok(None),
        }
    }

    // Private methods for historical sync
    async fn start_full_historical_sync(&self) -> Result<()> {
        info!("Starting full historical data sync from genesis...");

        // Update status
        {
            let mut state_manager = self.state_manager.write().await;
            state_manager.update_status("HistoricalSync")?;
        }

        // For now, we just log that this is not yet fully implemented
        // The actual implementation requires spawning parallel tasks which needs restructuring
        warn!("Full historical sync requires manual use of 'snaprag sync start --from-block 0 --to-block <latest>'");
        Ok(())
    }

    async fn start_historical_sync_with_range(
        &self,
        from_block: u64,
        to_block: u64,
        lock: &SyncLockFile,
    ) -> Result<()> {
        info!(
            "Starting historical data sync from block {} to block {}...",
            from_block, to_block
        );

        // Update status
        {
            let mut state_manager = self.state_manager.write().await;
            state_manager.update_status("RangeSync")?;
        }

        // 🚀 Parallel shard sync: Process all configured shards simultaneously
        info!(
            "Starting parallel sync of {} shards...",
            self.config.shard_ids.len()
        );

        let mut handles = vec![];

        for &shard_id in &self.config.shard_ids {
            info!("🔄 Spawning sync task for shard {}", shard_id);

            // Clone necessary resources for each shard task
            let client = self.client.clone();
            let database = self.database.clone();
            let state_manager = self.state_manager.clone();
            let config = self.config.clone();
            let lock_manager = self.lock_manager.clone();
            let hook_manager = self.hook_manager.clone();

            // Spawn parallel task for this shard
            let handle = tokio::spawn(async move {
                // Check if we should resume from last saved progress
                let last_saved_height = database
                    .get_last_processed_height(shard_id)
                    .await
                    .unwrap_or(0);

                // Resume from last saved height if it's greater than requested from_block
                let resume_from = if from_block == 0 && last_saved_height > 0 {
                    info!(
                        "📍 Resuming shard {} from last saved height {} (instead of {})",
                        shard_id, last_saved_height, from_block
                    );
                    last_saved_height
                } else if last_saved_height > from_block && last_saved_height < to_block {
                    info!(
                        "📍 Progress found for shard {}: resuming from block {} (was at {})",
                        shard_id, last_saved_height, from_block
                    );
                    last_saved_height
                } else {
                    from_block
                };

                info!(
                    "📥 Starting sync for shard {} from block {} to {}",
                    shard_id, resume_from, to_block
                );

                let mut current_block = resume_from;
                let mut total_messages = 0u64;
                let mut total_blocks = 0u64;

                while current_block <= to_block {
                    let remaining = to_block.saturating_sub(current_block).saturating_add(1);
                    let batch = config.batch_size.min(remaining as u32);

                    // Create request and fetch chunks
                    let request = crate::sync::client::proto::ShardChunksRequest {
                        shard_id,
                        start_block_number: current_block,
                        stop_block_number: Some(current_block + u64::from(batch) - 1),
                    };

                    match client.get_shard_chunks(request).await {
                        Ok(response) => {
                            let chunks = response.shard_chunks;

                            if chunks.is_empty() {
                                info!(
                                    "Shard {}: no more chunks at block {}",
                                    shard_id, current_block
                                );
                                break;
                            }

                            let processor = if let Some(ref hm) = hook_manager {
                                ShardProcessor::with_hooks(database.as_ref().clone(), hm.clone())
                            } else {
                                ShardProcessor::new(database.as_ref().clone())
                            };
                            processor.process_chunks_batch(&chunks, shard_id).await?;

                            // Update stats
                            let messages_in_batch: u64 = chunks
                                .iter()
                                .map(|c| {
                                    c.transactions
                                        .iter()
                                        .map(|tx| tx.user_messages.len() as u64)
                                        .sum::<u64>()
                                })
                                .sum();

                            total_blocks += chunks.len() as u64;
                            total_messages += messages_in_batch;

                            // Find max block number processed
                            let max_block = chunks
                                .iter()
                                .filter_map(|c| c.header.as_ref())
                                .filter_map(|h| h.height.as_ref())
                                .map(|height| height.block_number)
                                .max()
                                .unwrap_or(current_block);

                            current_block = max_block.saturating_add(1);

                            // Save progress to database
                            database
                                .update_last_processed_height(shard_id, current_block)
                                .await?;

                            info!(
                                "Shard {}: processed {} blocks, {} messages (current: {})",
                                shard_id, total_blocks, total_messages, current_block
                            );

                            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        }
                        Err(e) if e.to_string().contains("no more chunks") => {
                            info!("Shard {}: reached end at block {}", shard_id, current_block);
                            break;
                        }
                        Err(e) => {
                            error!(
                                "Shard {} sync error at block {}: {}",
                                shard_id, current_block, e
                            );
                            return Err(e);
                        }
                    }
                }

                info!(
                    "✅ Shard {} completed: {} blocks, {} messages",
                    shard_id, total_blocks, total_messages
                );
                Ok::<(), crate::SnapRagError>(())
            });

            handles.push((shard_id, handle));
        }

        // Wait for all shards to complete
        info!(
            "⏳ Waiting for {} shard sync tasks to complete...",
            handles.len()
        );

        for (shard_id, handle) in handles {
            match handle.await {
                Ok(Ok(())) => {
                    info!("✅ Shard {} sync completed", shard_id);
                }
                Ok(Err(e)) => {
                    error!("❌ Shard {} sync failed: {}", shard_id, e);
                    return Err(e);
                }
                Err(e) => {
                    error!("❌ Shard {} task panicked: {}", shard_id, e);
                    return Err(crate::SnapRagError::Custom(format!(
                        "Shard {shard_id} sync task panicked: {e}"
                    )));
                }
            }
        }

        info!(
            "🎉 Parallel sync completed across {} shards",
            self.config.shard_ids.len()
        );
        Ok(())
    }

    fn start_full_realtime_sync(&self) {
        if self.config.enable_continuous_sync {
            info!(
                "Starting continuous sync monitoring (polling every {} seconds)...",
                self.config.continuous_sync_interval_secs
            );

            // Spawn continuous sync task
            let config = self.config.clone();
            let client = self.client.clone();
            let database = self.database.clone();
            let state_manager = self.state_manager.clone();
            let hook_manager = self.hook_manager.clone();

            tokio::spawn(async move {
                Self::run_continuous_sync(config, client, database, state_manager, hook_manager)
                    .await;
            });
        } else {
            info!("Real-time sync not yet implemented in refactored service");
            warn!("Use 'snaprag sync start --from-block <last_block>' for now");
        }
    }

    /// Start sync with parallel workers per shard (fail-fast strategy)
    /// If any worker fails, all workers stop and progress is saved
    pub async fn start_with_range_and_workers(
        &self,
        from_block: u64,
        to_block: u64,
        workers_per_shard: u32,
    ) -> Result<()> {
        use std::sync::atomic::AtomicBool;
        use std::sync::atomic::AtomicU64;
        use std::sync::atomic::Ordering;

        use tokio::sync::Semaphore;

        info!(
            "Starting parallel sync: {} shards × {} workers = {} total workers (fail-fast mode)",
            self.config.shard_ids.len(),
            workers_per_shard,
            self.config.shard_ids.len() * workers_per_shard as usize
        );

        // Get Snapchain info to determine actual max heights per shard
        let shard_max_heights: std::collections::HashMap<u32, u64> =
            match self.client.get_info().await {
                Ok(info) => info
                    .shard_infos
                    .iter()
                    .map(|s| (s.shard_id, s.max_height))
                    .collect(),
                Err(_) => std::collections::HashMap::new(),
            };

        let mut all_shard_handles = vec![];

        // For each shard, create a coordinator that manages workers
        for &shard_id in &self.config.shard_ids {
            // Determine the actual range for this shard
            let shard_to_block = if to_block == u64::MAX {
                shard_max_heights
                    .get(&shard_id)
                    .copied()
                    .unwrap_or(to_block)
            } else {
                to_block
            };

            // Check if we should resume from last saved progress
            let last_saved_height = self
                .database
                .get_last_processed_height(shard_id)
                .await
                .unwrap_or(0);
            let shard_from_block = if from_block == 0 && last_saved_height > 0 {
                info!(
                    "📍 Shard {} resuming from last saved height {}",
                    shard_id, last_saved_height
                );
                last_saved_height
            } else {
                from_block
            };

            let total_blocks = shard_to_block.saturating_sub(shard_from_block);

            info!(
                "🔄 Shard {}: spawning tasks with max {} concurrent workers ({} total blocks)",
                shard_id, workers_per_shard, total_blocks
            );

            let client = self.client.clone();
            let database = self.database.clone();
            let config = self.config.clone();
            let hook_manager = self.hook_manager.clone();

            // Spawn shard coordinator
            let handle = tokio::spawn(async move {
                // 🎯 Semaphore: Limit concurrent tasks to workers_per_shard
                let semaphore = Arc::new(Semaphore::new(workers_per_shard as usize));
                let current_block = Arc::new(AtomicU64::new(shard_from_block));
                let should_stop = Arc::new(AtomicBool::new(false));
                // 🚀 LOCK-FREE: Use DashMap instead of Mutex<BTreeSet> for zero-contention tracking
                let completed_batches = Arc::new(dashmap::DashSet::new());
                let mut task_handles = vec![];

                // Spawn a background task to periodically save progress
                let progress_saver_db = database.clone();
                let progress_saver_batches = completed_batches.clone();
                let progress_saver_stop = should_stop.clone();
                let progress_saver = tokio::spawn(async move {
                    let mut last_saved_progress = shard_from_block;
                    loop {
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

                        if progress_saver_stop.load(Ordering::SeqCst) {
                            break;
                        }

                        // 🚀 LOCK-FREE: Calculate continuous progress without blocking
                        let mut continuous_progress = shard_from_block;
                        let batch_size_u64 = u64::from(config.batch_size);

                        while progress_saver_batches.contains(&continuous_progress) {
                            continuous_progress += batch_size_u64;
                        }

                        // Only update if progress changed
                        if continuous_progress > last_saved_progress {
                            if let Err(e) = progress_saver_db
                                .update_last_processed_height(shard_id, continuous_progress)
                                .await
                            {
                                warn!("Failed to save progress: {}", e);
                            } else {
                                info!(
                                    "💾 Shard {} progress saved: {} blocks",
                                    shard_id, continuous_progress
                                );
                                last_saved_progress = continuous_progress;
                            }
                        }
                    }
                });

                // Spawn tasks dynamically until we reach the end or stop signal
                loop {
                    // Check if we should stop (another worker failed)
                    if should_stop.load(Ordering::SeqCst) {
                        warn!(
                            "Shard {}: stopping task spawning due to failure in another worker",
                            shard_id
                        );
                        break;
                    }

                    let batch_start =
                        current_block.fetch_add(u64::from(config.batch_size), Ordering::SeqCst);

                    if batch_start >= shard_to_block {
                        break; // No more batches to process
                    }

                    let batch_end =
                        (batch_start + u64::from(config.batch_size) - 1).min(shard_to_block);

                    // Acquire semaphore permit (will wait if at max workers)
                    let permit = semaphore.clone().acquire_owned().await.map_err(|e| {
                        crate::SnapRagError::Custom(format!("Semaphore error: {e}"))
                    })?;

                    let client = client.clone();
                    let database = database.clone();
                    let hook_manager = hook_manager.clone();
                    let should_stop_shared = should_stop.clone();
                    let completed_batches_shared = completed_batches.clone();

                    // Spawn task for this batch with retry logic
                    let task = tokio::spawn(async move {
                        let _permit = permit; // Hold permit until task completes

                        const MAX_RETRIES: u32 = 5; // Increased from 3 to 5
                        let mut attempt = 0;

                        loop {
                            attempt += 1;

                            // Check stop signal before each attempt
                            if should_stop_shared.load(Ordering::SeqCst) {
                                return Ok::<(u64, u64), crate::SnapRagError>((0, 0));
                            }

                            let request = crate::sync::client::proto::ShardChunksRequest {
                                shard_id,
                                start_block_number: batch_start,
                                stop_block_number: Some(batch_end),
                            };

                            // 📊 Measure gRPC fetch time
                            let fetch_start = std::time::Instant::now();
                            match client.get_shard_chunks(request).await {
                                Ok(response) => {
                                    let fetch_elapsed = fetch_start.elapsed();
                                    let chunks = response.shard_chunks;

                                    if !chunks.is_empty() {
                                        // 📊 Measure processing time
                                        let process_start = std::time::Instant::now();
                                        let processor = if let Some(ref hm) = hook_manager {
                                            ShardProcessor::with_hooks(
                                                database.as_ref().clone(),
                                                hm.clone(),
                                            )
                                        } else {
                                            ShardProcessor::new(database.as_ref().clone())
                                        };
                                        match processor
                                            .process_chunks_batch(&chunks, shard_id)
                                            .await
                                        {
                                            Ok(()) => {
                                                let process_elapsed = process_start.elapsed();
                                                let messages_in_batch: u64 = chunks
                                                    .iter()
                                                    .map(|c| {
                                                        c.transactions
                                                            .iter()
                                                            .map(|tx| tx.user_messages.len() as u64)
                                                            .sum::<u64>()
                                                    })
                                                    .sum();

                                                // 📊 Log performance breakdown
                                                if fetch_elapsed.as_millis() > 500
                                                    || process_elapsed.as_millis() > 500
                                                {
                                                    info!(
                                                        "⏱️  Shard {} batch {}-{}: fetch={}ms, process={}ms (total={}ms)",
                                                        shard_id, batch_start, batch_end,
                                                        fetch_elapsed.as_millis(),
                                                        process_elapsed.as_millis(),
                                                        fetch_elapsed.as_millis() + process_elapsed.as_millis()
                                                    );
                                                } else {
                                                    tracing::debug!(
                                                        "Shard {} batch {}-{}: {} blocks, {} msgs, fetch={}ms, process={}ms",
                                                        shard_id, batch_start, batch_end, chunks.len(), messages_in_batch,
                                                        fetch_elapsed.as_millis(), process_elapsed.as_millis()
                                                    );
                                                }

                                                // ✅ Success - mark batch as completed (progress saver will handle DB update)
                                                // 🚀 LOCK-FREE: Insert without blocking other workers
                                                completed_batches_shared.insert(batch_start);

                                                return Ok::<(u64, u64), crate::SnapRagError>((
                                                    chunks.len() as u64,
                                                    messages_in_batch,
                                                ));
                                            }
                                            Err(e) if attempt < MAX_RETRIES => {
                                                // Exponential backoff: 2^attempt seconds (2s, 4s, 8s, 16s, 32s)
                                                let backoff_secs = 2u64.pow(attempt - 1).min(30);
                                                let error_msg = e.to_string();
                                                let error_type = if error_msg.contains("Database")
                                                    || error_msg.contains("Sqlx")
                                                {
                                                    "💾 Database Error"
                                                } else if error_msg.contains("timeout") {
                                                    "⏱️  Processing Timeout"
                                                } else {
                                                    "❌ Processing Error"
                                                };

                                                warn!(
                                                    "{} - Shard {} batch {}-{} (attempt {}/{}): {}\n   Retrying in {}s...",
                                                    error_type, shard_id, batch_start, batch_end, attempt, MAX_RETRIES, error_msg, backoff_secs
                                                );
                                                tokio::time::sleep(
                                                    tokio::time::Duration::from_secs(backoff_secs),
                                                )
                                                .await;
                                                continue; // Retry
                                            }
                                            Err(e) => {
                                                let error_msg = e.to_string();
                                                let error_debug = format!("{e:?}");

                                                error!(
                                                    "🔴 CRITICAL: Shard {} batch {}-{} processing FAILED after {} attempts\n\
                                                     ├─ Error Type: {}\n\
                                                     ├─ Error Message: {}\n\
                                                     ├─ Error Debug: {}\n\
                                                     ├─ Shard ID: {}\n\
                                                     ├─ Batch Range: {}-{}\n\
                                                     ├─ Total Attempts: {}\n\
                                                     └─ Action: Stopping all workers to prevent data holes",
                                                    shard_id, batch_start, batch_end, MAX_RETRIES,
                                                    if error_msg.contains("Database") || error_msg.contains("Sqlx") { "Database" }
                                                    else if error_msg.contains("timeout") { "Timeout" }
                                                    else { "Processing" },
                                                    error_msg,
                                                    error_debug,
                                                    shard_id,
                                                    batch_start,
                                                    batch_end,
                                                    MAX_RETRIES
                                                );

                                                // Signal all workers to stop
                                                should_stop_shared.store(true, Ordering::SeqCst);
                                                return Err(e);
                                            }
                                        }
                                    }
                                    return Ok((0, 0)); // Empty batch
                                }
                                Err(e) if e.to_string().contains("no more chunks") => {
                                    return Ok((0, 0)); // Skip empty batches
                                }
                                Err(e) if attempt < MAX_RETRIES => {
                                    // Exponential backoff: 2^attempt seconds (2s, 4s, 8s, 16s, 32s)
                                    let backoff_secs = 2u64.pow(attempt - 1).min(30);
                                    let error_msg = e.to_string();
                                    let error_type = if error_msg.contains("transport error") {
                                        "🌐 Network Transport Error"
                                    } else if error_msg.contains("connection refused") {
                                        "🔌 Connection Refused"
                                    } else if error_msg.contains("connection reset") {
                                        "🔄 Connection Reset"
                                    } else if error_msg.contains("timed out") {
                                        "⏱️  Request Timeout"
                                    } else if error_msg.contains("broken pipe") {
                                        "📡 Broken Pipe"
                                    } else {
                                        "❌ gRPC Error"
                                    };

                                    warn!(
                                        "{} - Shard {} batch {}-{} (attempt {}/{}): {}\n   Retrying in {}s...",
                                        error_type, shard_id, batch_start, batch_end, attempt, MAX_RETRIES, error_msg, backoff_secs
                                    );
                                    tokio::time::sleep(tokio::time::Duration::from_secs(
                                        backoff_secs,
                                    ))
                                    .await;
                                    // Retry
                                }
                                Err(e) => {
                                    let error_msg = e.to_string();
                                    let error_debug = format!("{e:?}");

                                    error!(
                                        "🔴 CRITICAL: Shard {} batch {}-{} fetch FAILED after {} attempts\n\
                                         ├─ Error Type: {}\n\
                                         ├─ Error Message: {}\n\
                                         ├─ Error Debug: {}\n\
                                         ├─ Shard ID: {}\n\
                                         ├─ Batch Range: {}-{}\n\
                                         ├─ Total Attempts: {}\n\
                                         └─ Action: Stopping all workers to prevent data holes",
                                        shard_id, batch_start, batch_end, MAX_RETRIES,
                                        if error_msg.contains("transport") { "Network/Transport" } 
                                        else if error_msg.contains("connection") { "Connection" }
                                        else if error_msg.contains("timeout") { "Timeout" }
                                        else { "Unknown" },
                                        error_msg,
                                        error_debug,
                                        shard_id,
                                        batch_start,
                                        batch_end,
                                        MAX_RETRIES
                                    );

                                    // Signal all workers to stop (fail-fast to avoid holes)
                                    should_stop_shared.store(true, Ordering::SeqCst);
                                    return Err(e);
                                }
                            }
                        }
                    });

                    task_handles.push((batch_start, task));
                }

                // Wait for all tasks to complete
                let total_tasks = task_handles.len();
                info!(
                    "Shard {}: waiting for {} tasks to complete...",
                    shard_id, total_tasks
                );

                let mut total_blocks = 0u64;
                let mut total_messages = 0u64;
                let mut completed_tasks = 0usize;
                let failed_tasks = 0usize;

                let mut task_handles_iter = task_handles.into_iter();
                while let Some((batch_start, task)) = task_handles_iter.next() {
                    match task.await {
                        Ok(Ok((blocks, messages))) => {
                            total_blocks += blocks;
                            total_messages += messages;
                            completed_tasks += 1;

                            if completed_tasks.is_multiple_of(100) {
                                let progress_pct = (completed_tasks as f64 / total_tasks as f64
                                    * 100.0)
                                    .min(100.0);
                                info!(
                                    "Shard {}: {}/{} tasks ({:.1}%), {} blocks, {} msgs total",
                                    shard_id,
                                    completed_tasks,
                                    total_tasks,
                                    progress_pct,
                                    total_blocks,
                                    total_messages
                                );
                            }
                        }
                        Ok(Err(e)) => {
                            error!("🔴 Shard {} batch {} failed: {}", shard_id, batch_start, e);

                            // Signal all workers to stop (already set by the failed worker, but set again to be safe)
                            should_stop.store(true, Ordering::SeqCst);

                            // 🚀 LOCK-FREE: Calculate final continuous progress
                            let mut continuous_progress = shard_from_block;
                            let batch_size_u64 = u64::from(config.batch_size);
                            while completed_batches.contains(&continuous_progress) {
                                continuous_progress += batch_size_u64;
                            }

                            if let Err(save_err) = database
                                .update_last_processed_height(shard_id, continuous_progress)
                                .await
                            {
                                error!("Failed to save progress: {}", save_err);
                            } else {
                                info!(
                                    "💾 Saved final progress at block {} before stopping",
                                    continuous_progress
                                );
                            }

                            // Wait for remaining spawned tasks to finish gracefully
                            let remaining = task_handles_iter.len();
                            if remaining > 0 {
                                info!("⏳ Waiting for {} remaining tasks to finish...", remaining);
                                for (_batch_start, task) in task_handles_iter {
                                    let _ = task.await; // Ignore individual errors, we're already failing
                                }
                                info!("✅ All tasks stopped gracefully");
                            }

                            // Stop progress saver
                            let _ = progress_saver.await;

                            // Fail fast - stop entire shard sync
                            return Err(e);
                        }
                        Err(e) => {
                            error!(
                                "🔴 Shard {} batch {} panicked: {}",
                                shard_id, batch_start, e
                            );

                            // Signal all workers to stop
                            should_stop.store(true, Ordering::SeqCst);

                            // 🚀 LOCK-FREE: Calculate final continuous progress
                            let mut continuous_progress = shard_from_block;
                            let batch_size_u64 = u64::from(config.batch_size);
                            while completed_batches.contains(&continuous_progress) {
                                continuous_progress += batch_size_u64;
                            }

                            if let Err(save_err) = database
                                .update_last_processed_height(shard_id, continuous_progress)
                                .await
                            {
                                error!("Failed to save progress: {}", save_err);
                            } else {
                                info!(
                                    "💾 Saved final progress at block {} before stopping",
                                    continuous_progress
                                );
                            }

                            // Wait for remaining spawned tasks to finish gracefully
                            let remaining = task_handles_iter.len();
                            if remaining > 0 {
                                info!("⏳ Waiting for {} remaining tasks to finish...", remaining);
                                for (_batch_start, task) in task_handles_iter {
                                    let _ = task.await;
                                }
                                info!("✅ All tasks stopped gracefully");
                            }

                            // Stop progress saver
                            let _ = progress_saver.await;

                            return Err(crate::SnapRagError::Custom(format!("Task panicked: {e}")));
                        }
                    }
                }

                if failed_tasks > 0 {
                    // This shouldn't happen due to fail-fast, but just in case
                    warn!(
                        "Shard {}: {} tasks failed (unexpected - should have stopped early)",
                        shard_id, failed_tasks
                    );
                }

                info!(
                    "✅ Shard {} completed: {} blocks, {} messages",
                    shard_id, total_blocks, total_messages
                );

                // Stop the progress saver and wait for it
                should_stop.store(true, Ordering::SeqCst);
                let _ = progress_saver.await;

                Ok::<(), crate::SnapRagError>(())
            });

            all_shard_handles.push((shard_id, handle));
        }

        // Wait for all shards to complete
        info!(
            "⏳ Waiting for {} shards to complete...",
            all_shard_handles.len()
        );

        for (shard_id, handle) in all_shard_handles {
            match handle.await {
                Ok(Ok(())) => {
                    info!("✅ Shard {} finished successfully", shard_id);
                }
                Ok(Err(e)) => {
                    error!("❌ Shard {} failed: {}", shard_id, e);
                    return Err(e);
                }
                Err(e) => {
                    error!("❌ Shard {} panicked: {}", shard_id, e);
                    return Err(crate::SnapRagError::Custom(format!(
                        "Shard {shard_id} panicked: {e}"
                    )));
                }
            }
        }

        info!("🎉 All shards completed successfully!");

        // If we're syncing to latest (u64::MAX), start continuous sync
        if to_block == u64::MAX && self.config.enable_continuous_sync {
            info!(
                "🔄 Starting continuous sync monitoring (polling every {} seconds)...",
                self.config.continuous_sync_interval_secs
            );

            // Start continuous sync and wait for it to complete (it runs forever)
            let config = self.config.clone();
            let client = self.client.clone();
            let database = self.database.clone();
            let state_manager = self.state_manager.clone();
            let hook_manager = self.hook_manager.clone();

            Self::run_continuous_sync(config, client, database, state_manager, hook_manager).await;
        } else if to_block == u64::MAX {
            info!("⚠️  Continuous sync is disabled. Enable it in config to automatically sync new blocks.");
        } else {
            info!("⚠️  Note: Run 'snaprag sync start' again (without --workers) to fill any gaps from failed batches");
        }

        Ok(())
    }

    /// Run continuous sync monitoring for new blocks
    async fn run_continuous_sync(
        config: crate::sync::types::SyncConfig,
        client: crate::sync::client::SnapchainClient,
        database: Arc<crate::database::Database>,
        state_manager: Arc<tokio::sync::RwLock<crate::sync::state_manager::SyncStateManager>>,
        hook_manager: Option<Arc<HookManager>>,
    ) {
        use tracing::error;
        use tracing::info;
        use tracing::warn;

        info!("🔄 Starting continuous sync monitoring...");

        loop {
            let interval_duration =
                tokio::time::Duration::from_secs(config.continuous_sync_interval_secs);

            // Sleep for the configured interval
            tokio::time::sleep(interval_duration).await;

            // Check each shard for new blocks
            for &shard_id in &config.shard_ids {
                match Self::check_and_sync_new_blocks(
                    shard_id,
                    &client,
                    &database,
                    &state_manager,
                    hook_manager.clone(),
                )
                .await
                {
                    Ok(blocks_synced) => {
                        if blocks_synced > 0 {
                            info!("📦 Shard {}: synced {} new blocks", shard_id, blocks_synced);
                        }
                    }
                    Err(e) => {
                        error!("❌ Shard {} continuous sync error: {}", shard_id, e);
                        // Continue with other shards even if one fails
                    }
                }
            }
        }
    }

    /// Check for and sync new blocks for a specific shard
    async fn check_and_sync_new_blocks(
        shard_id: u32,
        client: &crate::sync::client::SnapchainClient,
        database: &Arc<crate::database::Database>,
        state_manager: &Arc<tokio::sync::RwLock<crate::sync::state_manager::SyncStateManager>>,
        hook_manager: Option<Arc<HookManager>>,
    ) -> Result<u64> {
        use tracing::debug;
        use tracing::warn;

        // Get current processed height for this shard
        let current_height = database.get_last_processed_height(shard_id).await?;

        // Get latest block height from snapchain
        let info = client.get_info().await?;
        let shard_info = info
            .shard_infos
            .iter()
            .find(|s| s.shard_id == shard_id)
            .ok_or_else(|| crate::SnapRagError::Custom(format!("Shard {shard_id} not found")))?;

        let latest_height = shard_info.max_height;

        if latest_height <= current_height {
            debug!(
                "Shard {}: no new blocks (current: {}, latest: {})",
                shard_id, current_height, latest_height
            );
            return Ok(0);
        }

        // Sync new blocks in small batches
        let batch_size = 10; // Small batch for continuous sync
        let mut blocks_synced = 0;
        let mut from_block = current_height + 1;

        while from_block <= latest_height {
            let to_block = (from_block + batch_size - 1).min(latest_height);

            debug!(
                "Shard {}: syncing blocks {} to {}",
                shard_id, from_block, to_block
            );

            // Create a small sync request
            let request = crate::sync::client::proto::ShardChunksRequest {
                shard_id,
                start_block_number: from_block,
                stop_block_number: Some(to_block),
            };

            match client.get_shard_chunks(request).await {
                Ok(response) => {
                    if response.shard_chunks.is_empty() {
                        debug!(
                            "Shard {}: no chunks returned for blocks {} to {}",
                            shard_id, from_block, to_block
                        );
                        break;
                    }

                    // Process the chunks
                    let processor = if let Some(ref hm) = hook_manager {
                        ShardProcessor::with_hooks(database.as_ref().clone(), hm.clone())
                    } else {
                        ShardProcessor::new(database.as_ref().clone())
                    };

                    match processor
                        .process_chunks_batch(&response.shard_chunks, shard_id)
                        .await
                    {
                        Ok(()) => {
                            blocks_synced += to_block - from_block + 1;

                            debug!(
                                "Shard {}: processed blocks {} to {}",
                                shard_id, from_block, to_block
                            );
                        }
                        Err(e) => {
                            warn!(
                                "Shard {}: failed to process chunks for blocks {} to {}: {}",
                                shard_id, from_block, to_block, e
                            );
                            break;
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        "Shard {}: failed to fetch chunks for blocks {} to {}: {}",
                        shard_id, from_block, to_block, e
                    );
                    break;
                }
            }

            from_block = to_block + 1;
        }

        Ok(blocks_synced)
    }
}
