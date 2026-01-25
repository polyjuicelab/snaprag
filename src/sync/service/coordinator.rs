use std::sync::Arc;

use tracing::error;
use tracing::info;

use super::state::count_chunk_messages;
use super::state::extract_block_number;
use super::state::ChunkProcessStats;
use crate::database::Database;
use crate::sync::client::proto;
use crate::sync::client::SnapchainClient;
use crate::sync::hooks::HookManager;
use crate::sync::shard_processor::ShardProcessor;
use crate::Result;

/// Coordinator for managing sync operations
pub struct SyncCoordinator {
    client: SnapchainClient,
    database: Arc<Database>,
    hook_manager: Option<Arc<HookManager>>,
}

impl SyncCoordinator {
    pub const fn new(client: SnapchainClient, database: Arc<Database>) -> Self {
        Self {
            client,
            database,
            hook_manager: None,
        }
    }

    pub fn with_hooks(
        client: SnapchainClient,
        database: Arc<Database>,
        hook_manager: Arc<HookManager>,
    ) -> Self {
        Self {
            client,
            database,
            hook_manager: Some(hook_manager),
        }
    }

    /// Poll once for a specific shard and block
    pub async fn poll_once(&self, shard_id: u32, block_number: u64) -> Result<ChunkProcessStats> {
        info!("Polling once for shard {} block {}", shard_id, block_number);

        let request = proto::ShardChunksRequest {
            shard_id,
            start_block_number: block_number,
            stop_block_number: Some(block_number + 1),
        };

        match self.client.get_shard_chunks(request).await {
            Ok(response) => {
                let chunk_count = response.shard_chunks.len();
                info!(
                    "Received {} chunks for shard {} block {}",
                    chunk_count, shard_id, block_number
                );

                let mut stats = process_shard_chunks(
                    &self.database,
                    shard_id,
                    response.shard_chunks,
                    self.hook_manager.clone(),
                )
                .await?;

                if stats.blocks_processed() == 0 {
                    stats.blocks_processed = 1;
                    stats.last_block_number = Some(block_number);
                }

                let processed_block = stats.last_block_number.unwrap_or(block_number);

                info!(
                    "Poll once completed for shard {} block {} (messages: {})",
                    shard_id, processed_block, stats.messages_processed
                );

                Ok(stats)
            }
            Err(err) => {
                error!(
                    "Failed to get shard chunks for shard {} at block {}: {}",
                    shard_id, block_number, err
                );
                Err(err)
            }
        }
    }

    /// Poll a batch of blocks at once
    pub async fn poll_batch(
        &self,
        shard_id: u32,
        from_block: u64,
        batch_size: u32,
    ) -> Result<ChunkProcessStats> {
        let to_block = from_block + u64::from(batch_size);

        info!(
            "📦 Processing blocks {} to {} (batch size: {})",
            from_block,
            to_block - 1,
            batch_size
        );

        let request = proto::ShardChunksRequest {
            shard_id,
            start_block_number: from_block,
            stop_block_number: Some(to_block),
        };

        match self.client.get_shard_chunks(request).await {
            Ok(response) => {
                let chunk_count = response.shard_chunks.len();
                info!("   ↳ Fetched {} chunks from server", chunk_count);

                let stats = process_shard_chunks(
                    &self.database,
                    shard_id,
                    response.shard_chunks,
                    self.hook_manager.clone(),
                )
                .await?;

                info!(
                    "   ✓ Completed blocks {} to {} → {} messages, {} blocks processed",
                    from_block,
                    to_block - 1,
                    stats.messages_processed,
                    stats.blocks_processed
                );

                Ok(stats)
            }
            Err(err) => {
                error!(
                    "   ✗ Failed to fetch blocks {}-{}: {}",
                    from_block,
                    to_block - 1,
                    err
                );
                Err(err)
            }
        }
    }
}

/// Process shard chunks and return statistics
async fn process_shard_chunks(
    database: &Database,
    shard_id: u32,
    chunks: Vec<proto::ShardChunk>,
    hook_manager: Option<Arc<HookManager>>,
) -> Result<ChunkProcessStats> {
    let mut stats = ChunkProcessStats::default();
    let processor = if let Some(hm) = hook_manager {
        ShardProcessor::with_hooks(database.clone(), hm)
    } else {
        ShardProcessor::new(database.clone())
    };

    for chunk in chunks {
        let block_number = extract_block_number(&chunk);
        let message_count = count_chunk_messages(&chunk);

        processor.process_chunk(&chunk, shard_id).await?;
        stats.record_chunk(block_number, message_count);
    }

    Ok(stats)
}
