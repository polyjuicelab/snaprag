//! Snapchain synchronization module
//!
//! This module provides functionality to sync data from snapchain nodes,
//! including block-by-block synchronization and real-time event streaming.

pub mod client;
pub mod hooks;
pub mod lazy_loader;
pub mod lock_file;
pub mod process_monitor;
pub mod service;
pub mod shard_processor;
pub mod state_manager;
pub mod types;

pub use client::SnapchainClient;
pub use hooks::HookManager;
pub use lazy_loader::LazyLoader;
pub use lock_file::SyncLockFile;
pub use lock_file::SyncLockManager;
pub use lock_file::SyncProgress;
pub use lock_file::SyncRange;
pub use service::SyncService;
pub use shard_processor::ShardProcessor;
pub use state_manager::SyncStateManager;
pub use state_manager::SyncStats;
pub use types::*;
