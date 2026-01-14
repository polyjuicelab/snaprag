use std::sync::Arc;

use tokio::sync::RwLock;
use tracing::error;
use tracing::info;

use crate::config::AppConfig;
use crate::database::Database;
use crate::sync::client::SnapchainClient;
use crate::sync::hooks::HookManager;
use crate::sync::lock_file::SyncLockFile;
use crate::sync::lock_file::SyncLockManager;
use crate::sync::shard_processor::ShardProcessor;
use crate::sync::state_manager::SyncStateManager;
use crate::sync::types::SyncConfig;
use crate::sync::types::SyncState;
use crate::Result;

// Re-export sub-modules
pub mod coordinator;
pub mod lifecycle;
pub mod monitoring;
pub mod state;

// Re-export key types
pub use coordinator::SyncCoordinator;
pub use lifecycle::LifecycleManager;
pub use monitoring::RealtimeMonitor;
pub use state::ChunkProcessStats;

/// Main sync service that coordinates synchronization with snapchain
pub struct SyncService {
    config: SyncConfig,
    client: SnapchainClient,
    database: Arc<Database>,
    state: Arc<RwLock<SyncState>>,
    state_manager: Arc<RwLock<SyncStateManager>>,
    lock_manager: SyncLockManager,
    lifecycle_manager: LifecycleManager,
    coordinator: SyncCoordinator,
    monitor: RealtimeMonitor,
}

impl SyncService {
    /// Create a new sync service
    pub async fn new(app_config: &AppConfig, database: Arc<Database>) -> Result<Self> {
        Self::new_with_hooks(app_config, database, None).await
    }

    /// Create a new sync service with optional hook manager
    pub async fn new_with_hooks(
        app_config: &AppConfig,
        database: Arc<Database>,
        hook_manager: Option<Arc<HookManager>>,
    ) -> Result<Self> {
        info!("Initializing SnapRAG sync service...");

        // Initialize sync configuration
        let config = SyncConfig {
            snapchain_http_endpoint: app_config.sync.snapchain_http_endpoint.clone(),
            snapchain_grpc_endpoint: app_config.sync.snapchain_grpc_endpoint.clone(),
            shard_ids: app_config.sync.shard_ids.clone(), // Read from config.toml
            start_block_height: None,
            batch_size: app_config.sync.batch_size,
            enable_realtime_sync: app_config.sync.enable_realtime_sync,
            enable_historical_sync: app_config.sync.enable_historical_sync,
            sync_interval_ms: app_config.sync.sync_interval_ms,
            enable_continuous_sync: app_config.sync.enable_continuous_sync,
            continuous_sync_interval_secs: app_config.sync.continuous_sync_interval_secs,
        };

        // Initialize snapchain client
        let client = SnapchainClient::new(
            &app_config.sync.snapchain_http_endpoint,
            &app_config.sync.snapchain_grpc_endpoint,
        )
        .await?;

        // Initialize state management
        let state = Arc::new(RwLock::new(SyncState::default()));
        let state_manager = Arc::new(RwLock::new(SyncStateManager::new("sync_state.json")));

        // Initialize lock manager
        let lock_manager = SyncLockManager::new();

        // Initialize sub-components
        let lifecycle_manager = LifecycleManager::new(
            config.clone(),
            client.clone(),
            database.clone(),
            state.clone(),
            state_manager.clone(),
            lock_manager.clone(),
        );

        let coordinator = if let Some(hm) = hook_manager.clone() {
            SyncCoordinator::with_hooks(client.clone(), database.clone(), hm)
        } else {
            SyncCoordinator::new(client.clone(), database.clone())
        };

        let monitor = RealtimeMonitor::new(
            client.clone(),
            database.clone(),
            state_manager.clone(),
            config.clone(),
        );

        info!("Sync service initialized successfully");

        Ok(Self {
            config,
            client,
            database,
            state,
            state_manager,
            lock_manager,
            lifecycle_manager,
            coordinator,
            monitor,
        })
    }

    /// Poll once for a specific shard and block
    pub async fn poll_once(&self, shard_id: u32, block_number: u64) -> Result<ChunkProcessStats> {
        self.coordinator.poll_once(shard_id, block_number).await
    }

    /// Poll a batch of blocks at once
    pub async fn poll_batch(
        &self,
        shard_id: u32,
        from_block: u64,
        batch_size: u32,
    ) -> Result<ChunkProcessStats> {
        self.coordinator
            .poll_batch(shard_id, from_block, batch_size)
            .await
    }

    /// Start the sync service
    pub async fn start(&self) -> Result<()> {
        self.lifecycle_manager.start().await
    }

    /// Start the sync service with a specific block range
    pub async fn start_with_range(&self, from_block: u64, to_block: u64) -> Result<()> {
        self.lifecycle_manager
            .start_with_range(from_block, to_block)
            .await
    }

    /// Start sync with parallel workers per shard
    pub async fn start_with_range_and_workers(
        &self,
        from_block: u64,
        to_block: u64,
        workers_per_shard: u32,
    ) -> Result<()> {
        self.lifecycle_manager
            .start_with_range_and_workers(from_block, to_block, workers_per_shard)
            .await
    }

    /// Stop the sync service
    pub async fn stop(&self, force: bool) -> Result<()> {
        self.lifecycle_manager.stop(force).await
    }

    /// Get sync status
    pub fn get_sync_status(&self) -> Result<Option<SyncLockFile>> {
        self.lifecycle_manager.get_sync_status()
    }
}
