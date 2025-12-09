//! Sync state persistence manager

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use serde::Deserialize;
use serde::Serialize;

use crate::Result;

/// Persistent sync state stored in a temporary file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentSyncState {
    /// Last update timestamp
    pub last_updated: u64,
    /// Status of the sync
    pub status: String,
    /// Last processed heights for each shard
    pub last_processed_heights: HashMap<u32, u64>,
    /// Total messages processed per shard
    pub total_messages_processed: HashMap<u32, u64>,
    /// Total blocks processed per shard
    pub total_blocks_processed: HashMap<u32, u64>,
    /// Last sync timestamp
    pub last_sync_timestamp: Option<u64>,
    /// Error messages
    pub errors: Vec<String>,
}

impl Default for PersistentSyncState {
    fn default() -> Self {
        Self {
            last_updated: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            status: "NotStarted".to_string(),
            last_processed_heights: HashMap::new(),
            total_messages_processed: HashMap::new(),
            total_blocks_processed: HashMap::new(),
            last_sync_timestamp: None,
            errors: Vec::new(),
        }
    }
}

/// Sync state manager for persistence
pub struct SyncStateManager {
    state_file_path: String,
    state: PersistentSyncState,
}

impl SyncStateManager {
    /// Create a new sync state manager
    #[must_use]
    pub fn new(state_file_path: &str) -> Self {
        Self {
            state_file_path: state_file_path.to_string(),
            state: PersistentSyncState::default(),
        }
    }

    /// Load state from file, or create default if file doesn't exist
    pub fn load(&mut self) -> Result<()> {
        if Path::new(&self.state_file_path).exists() {
            let content = fs::read_to_string(&self.state_file_path)?;
            self.state =
                serde_json::from_str(&content).unwrap_or_else(|_| PersistentSyncState::default());
            tracing::info!("Loaded sync state from {}", self.state_file_path);
        } else {
            tracing::info!("No existing sync state found, starting fresh");
            self.save()?;
        }
        Ok(())
    }

    /// Save state to file
    pub fn save(&mut self) -> Result<()> {
        self.state.last_updated = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let content = serde_json::to_string_pretty(&self.state)?;
        fs::write(&self.state_file_path, content)?;
        tracing::debug!("Saved sync state to {}", self.state_file_path);
        Ok(())
    }

    /// Get current state
    #[must_use]
    pub const fn get_state(&self) -> &PersistentSyncState {
        &self.state
    }

    /// Get mutable reference to state
    pub const fn get_state_mut(&mut self) -> &mut PersistentSyncState {
        &mut self.state
    }

    /// Update sync status
    pub fn update_status(&mut self, status: &str) -> Result<()> {
        self.state.status = status.to_string();
        self.state.last_sync_timestamp = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        );
        self.save()
    }

    /// Update last processed height for a shard
    pub fn update_last_processed_height(&mut self, shard_id: u32, height: u64) -> Result<()> {
        self.state.last_processed_heights.insert(shard_id, height);
        self.save()
    }

    /// Get last processed height for a shard
    #[must_use]
    pub fn get_last_processed_height(&self, shard_id: u32) -> u64 {
        self.state
            .last_processed_heights
            .get(&shard_id)
            .copied()
            .unwrap_or(0)
    }

    /// Increment processed messages for a shard
    pub fn increment_messages_processed(&mut self, shard_id: u32, count: u64) -> Result<()> {
        let current = self
            .state
            .total_messages_processed
            .get(&shard_id)
            .copied()
            .unwrap_or(0);
        self.state
            .total_messages_processed
            .insert(shard_id, current + count);
        self.save()
    }

    /// Increment processed blocks for a shard
    pub fn increment_blocks_processed(&mut self, shard_id: u32, count: u64) -> Result<()> {
        let current = self
            .state
            .total_blocks_processed
            .get(&shard_id)
            .copied()
            .unwrap_or(0);
        self.state
            .total_blocks_processed
            .insert(shard_id, current + count);
        self.save()
    }

    /// Add error message
    ///
    /// # Errors
    /// Returns an error if saving the state fails
    pub fn add_error(&mut self, error: String) -> Result<()> {
        self.state.errors.push(error);
        // Keep only last 100 errors to prevent file from growing too large
        if self.state.errors.len() > 100 {
            self.state.errors.drain(0..self.state.errors.len() - 100);
        }
        self.save()
    }

    /// Clear all errors
    pub fn clear_errors(&mut self) -> Result<()> {
        self.state.errors.clear();
        self.save()
    }

    /// Get sync statistics
    #[must_use]
    pub fn get_stats(&self) -> SyncStats {
        SyncStats {
            status: self.state.status.clone(),
            total_messages: self.state.total_messages_processed.values().sum(),
            total_blocks: self.state.total_blocks_processed.values().sum(),
            last_sync_timestamp: self.state.last_sync_timestamp,
            error_count: self.state.errors.len(),
        }
    }

    /// Reset state (useful for starting fresh)
    pub fn reset(&mut self) -> Result<()> {
        self.state = PersistentSyncState::default();
        self.save()
    }
}

/// Sync statistics
#[derive(Debug, Clone)]
pub struct SyncStats {
    pub status: String,
    pub total_messages: u64,
    pub total_blocks: u64,
    pub last_sync_timestamp: Option<u64>,
    pub error_count: usize,
}

impl SyncStats {
    /// Format stats for display
    #[must_use]
    pub fn format(&self) -> String {
        format!(
            "Status: {}, Messages: {}, Blocks: {}, Errors: {}, Last Sync: {}",
            self.status,
            self.total_messages,
            self.total_blocks,
            self.error_count,
            self.last_sync_timestamp.map_or_else(
                || "Never".to_string(),
                |ts| chrono::DateTime::from_timestamp(ts as i64, 0).map_or_else(
                    || "Unknown".to_string(),
                    |dt| dt.format("%Y-%m-%d %H:%M:%S").to_string()
                )
            )
        )
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_sync_state_manager() {
        let temp_dir = TempDir::new().unwrap();
        let state_file = temp_dir.path().join("test_state.json");
        let mut manager = SyncStateManager::new(state_file.to_str().unwrap());

        // Test initial state
        assert_eq!(manager.get_state().status, "NotStarted");
        assert_eq!(manager.get_last_processed_height(0), 0);

        // Test updating status
        manager.update_status("Running").unwrap();
        assert_eq!(manager.get_state().status, "Running");

        // Test updating height
        manager.update_last_processed_height(0, 100).unwrap();
        assert_eq!(manager.get_last_processed_height(0), 100);

        // Test incrementing counters
        manager.increment_messages_processed(0, 50).unwrap();
        manager.increment_blocks_processed(0, 10).unwrap();

        let stats = manager.get_stats();
        assert_eq!(stats.total_messages, 50);
        assert_eq!(stats.total_blocks, 10);
        assert_eq!(stats.status, "Running");

        // Test adding errors
        manager.add_error("Test error 1".to_string()).unwrap();
        manager.add_error("Test error 2".to_string()).unwrap();
        assert_eq!(manager.get_state().errors.len(), 2);

        // Test error handling - add one more error
        manager.add_error("Test error 3".to_string()).unwrap();
        assert_eq!(manager.get_state().errors.len(), 3);
        assert_eq!(manager.get_stats().error_count, 3);
    }
}
