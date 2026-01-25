use std::collections::HashSet;
use std::sync::Arc;

use tracing::debug;

use crate::database::Database;
use crate::sync::client::proto::ShardChunk;
use crate::sync::HookManager;
use crate::Result;

// Re-export submodules
mod batch;
mod cast_handlers;
mod handlers;
mod types;
mod utils;

// Re-export types
// Re-export batch function for testing (both unit and integration tests)
pub use batch::flush_batched_data;
pub use types::BatchedData;

/// Processor for handling shard chunks and extracting user data
pub struct ShardProcessor {
    database: Database,
    // Cache for FIDs that have been ensured to exist in this batch
    fid_cache: std::sync::Mutex<HashSet<i64>>,
    // Cache for FIDs that have been registered (via id_register event)
    registered_fids: std::sync::Mutex<HashSet<i64>>,
    // Optional hook manager for event callbacks
    hook_manager: Option<Arc<HookManager>>,
}

impl ShardProcessor {
    /// Create a new shard processor
    #[must_use]
    pub fn new(database: Database) -> Self {
        Self {
            database,
            fid_cache: std::sync::Mutex::new(HashSet::new()),
            registered_fids: std::sync::Mutex::new(HashSet::new()),
            hook_manager: None,
        }
    }

    /// Create a new shard processor with hook manager
    #[must_use]
    pub fn with_hooks(database: Database, hook_manager: Arc<HookManager>) -> Self {
        Self {
            database,
            fid_cache: std::sync::Mutex::new(HashSet::new()),
            registered_fids: std::sync::Mutex::new(HashSet::new()),
            hook_manager: Some(hook_manager),
        }
    }

    /// Clear the FID cache (call this between batches)
    pub fn clear_cache(&self) {
        if let Ok(mut cache) = self.fid_cache.lock() {
            cache.clear();
        }
        // Note: We DON'T clear registered_fids as it should persist across batches
    }

    /// Process multiple chunks in a single batch for maximum performance
    pub async fn process_chunks_batch(&self, chunks: &[ShardChunk], shard_id: u32) -> Result<()> {
        // Collect all data from all chunks
        let mut batched = BatchedData::new();

        for chunk in chunks {
            let header = chunk
                .header
                .as_ref()
                .ok_or_else(|| crate::SnapRagError::Custom("Missing chunk header".to_string()))?;

            let height = header
                .height
                .as_ref()
                .ok_or_else(|| crate::SnapRagError::Custom("Missing header height".to_string()))?;

            let block_number = height.block_number;
            let timestamp = header.timestamp;

            debug!(
                "Processing shard {} block {} with {} transactions",
                shard_id,
                block_number,
                chunk.transactions.len()
            );

            // Process each transaction and collect data
            for (tx_idx, transaction) in chunk.transactions.iter().enumerate() {
                handlers::collect_transaction_data(
                    transaction,
                    shard_id,
                    block_number,
                    timestamp,
                    tx_idx,
                    &mut batched,
                    self.hook_manager.as_ref(),
                )
                .await?;
            }
        }

        // Single batch insert for all chunks
        batch::flush_batched_data(&self.database, batched).await?;

        // Update sync progress for the last chunk
        if let Some(last_chunk) = chunks.last() {
            if let Some(header) = &last_chunk.header {
                if let Some(height) = &header.height {
                    self.database
                        .update_last_processed_height(shard_id, height.block_number)
                        .await?;
                }
            }
        }

        Ok(())
    }

    /// Process a shard chunk and extract all user data
    pub async fn process_chunk(&self, chunk: &ShardChunk, shard_id: u32) -> Result<()> {
        let header = chunk
            .header
            .as_ref()
            .ok_or_else(|| crate::SnapRagError::Custom("Missing chunk header".to_string()))?;

        let height = header
            .height
            .as_ref()
            .ok_or_else(|| crate::SnapRagError::Custom("Missing header height".to_string()))?;

        let block_number = height.block_number;
        let timestamp = header.timestamp;

        debug!(
            "Processing shard {} block {} with {} transactions",
            shard_id,
            block_number,
            chunk.transactions.len()
        );

        // Collect all data for batch insert
        let mut batched = BatchedData::new();

        // Process each transaction and collect data
        for (tx_idx, transaction) in chunk.transactions.iter().enumerate() {
            handlers::collect_transaction_data(
                transaction,
                shard_id,
                block_number,
                timestamp,
                tx_idx,
                &mut batched,
                self.hook_manager.as_ref(),
            )
            .await?;
        }

        // Batch insert all collected data
        batch::flush_batched_data(&self.database, batched).await?;

        // Update sync progress
        self.database
            .update_last_processed_height(shard_id, block_number)
            .await?;

        Ok(())
    }
}
