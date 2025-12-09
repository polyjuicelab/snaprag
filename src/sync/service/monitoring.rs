use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::error;
use tracing::info;

use super::state::count_chunk_messages;
use super::state::extract_block_number;
use super::state::ChunkProcessStats;
use crate::database::Database;
use crate::sync::client::proto;
use crate::sync::client::SnapchainClient;
use crate::sync::shard_processor::ShardProcessor;
use crate::sync::state_manager::SyncStateManager;
use crate::sync::types::SyncConfig;
use crate::Result;

/// Monitor for real-time sync operations
pub struct RealtimeMonitor {
    client: SnapchainClient,
    database: Arc<Database>,
    state_manager: Arc<RwLock<SyncStateManager>>,
    config: SyncConfig,
}

impl RealtimeMonitor {
    pub const fn new(
        client: SnapchainClient,
        database: Arc<Database>,
        state_manager: Arc<RwLock<SyncStateManager>>,
        config: SyncConfig,
    ) -> Self {
        Self {
            client,
            database,
            state_manager,
            config,
        }
    }

    /// Monitor a shard for real-time updates
    pub async fn monitor_shard_realtime(&self, shard_id: u32) -> Result<()> {
        info!("Starting real-time monitoring for shard {}", shard_id);
        let retry_delay = tokio::time::Duration::from_millis(self.config.sync_interval_ms.max(100));

        let mut last_processed_height = {
            let sm = self.state_manager.read().await;
            sm.get_last_processed_height(shard_id)
        };

        loop {
            // Check for new chunks in this shard
            let request = proto::ShardChunksRequest {
                shard_id,
                start_block_number: last_processed_height,
                stop_block_number: Some(last_processed_height + 10), // Small batch for real-time
            };

            match self.client.get_shard_chunks(request).await {
                Ok(response) => {
                    let chunks = response.shard_chunks;
                    if !chunks.is_empty() {
                        let chunk_count = chunks.len();
                        info!(
                            "Shard {}: found {} new chunks at height {}",
                            shard_id, chunk_count, last_processed_height
                        );

                        let stats = process_shard_chunks(&self.database, shard_id, chunks).await?;

                        if stats.blocks_processed > 0 {
                            let next_height = stats.last_block_number.map_or_else(
                                || last_processed_height.saturating_add(stats.blocks_processed),
                                |block| block.saturating_add(1),
                            );

                            {
                                let mut sm = self.state_manager.write().await;
                                sm.increment_blocks_processed(shard_id, stats.blocks_processed)?;
                                sm.increment_messages_processed(
                                    shard_id,
                                    stats.messages_processed,
                                )?;
                                sm.update_last_processed_height(shard_id, next_height)?;
                            }

                            info!(
                                "Shard {}: processed {} blocks ({} messages), next height: {}",
                                shard_id,
                                stats.blocks_processed,
                                stats.messages_processed,
                                next_height
                            );

                            last_processed_height = next_height;
                        }
                    }
                }
                Err(err) => {
                    error!(
                        "Failed to check for new chunks in shard {}: {}",
                        shard_id, err
                    );
                    self.state_manager
                        .write()
                        .await
                        .add_error(format!("Shard {shard_id} real-time sync error: {err}"))?;
                }
            }

            tokio::time::sleep(retry_delay).await;
        }
    }
}

/// Process shard chunks and return statistics
async fn process_shard_chunks(
    database: &Database,
    shard_id: u32,
    chunks: Vec<proto::ShardChunk>,
) -> Result<ChunkProcessStats> {
    let mut stats = ChunkProcessStats::default();
    let processor = ShardProcessor::new(database.clone());

    for chunk in chunks {
        let block_number = extract_block_number(&chunk);
        let message_count = count_chunk_messages(&chunk);

        processor.process_chunk(&chunk, shard_id).await?;
        stats.record_chunk(block_number, message_count);
    }

    Ok(stats)
}
