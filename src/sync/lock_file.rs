use std::fs;
use std::path::Path;
use std::process;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use serde::Deserialize;
use serde::Serialize;
use tracing::error;
use tracing::info;
use tracing::warn;

use crate::Result;

/// Lock file structure for tracking sync process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncLockFile {
    /// Process ID of the sync process
    pub pid: u32,
    /// Sync status (running, stopping, error)
    pub status: String,
    /// Start time of the sync process
    pub start_time: u64,
    /// Last update time
    pub last_update: u64,
    /// Current sync progress
    pub progress: SyncProgress,
    /// Error message if any
    pub error_message: Option<String>,
}

/// Sync progress information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncProgress {
    /// Per-shard progress tracking (for parallel sync)
    pub shard_progress: std::collections::HashMap<u32, ShardProgress>,
    /// Total blocks processed across all shards
    pub total_blocks_processed: u64,
    /// Total messages processed across all shards
    pub total_messages_processed: u64,
    /// Sync range (if applicable)
    pub sync_range: Option<SyncRange>,
}

/// Progress information for a single shard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardProgress {
    /// Current block height for this shard
    pub current_block: u64,
    /// Total blocks processed by this shard
    pub blocks_processed: u64,
    /// Total messages processed by this shard
    pub messages_processed: u64,
    /// Last update timestamp for this shard
    pub last_update: u64,
}

/// Sync range information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRange {
    pub from_block: u64,
    pub to_block: Option<u64>,
}

impl SyncLockFile {
    /// Create a new lock file
    #[must_use]
    pub fn new(status: &str, sync_range: Option<SyncRange>) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            pid: process::id(),
            status: status.to_string(),
            start_time: now,
            last_update: now,
            progress: SyncProgress {
                shard_progress: std::collections::HashMap::new(),
                total_blocks_processed: 0,
                total_messages_processed: 0,
                sync_range,
            },
            error_message: None,
        }
    }

    /// Update the lock file with new progress for a specific shard
    pub fn update_progress(&mut self, shard_id: Option<u32>, block_number: Option<u64>) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.last_update = now;

        // Update per-shard progress
        if let (Some(shard), Some(block)) = (shard_id, block_number) {
            self.progress
                .shard_progress
                .entry(shard)
                .and_modify(|progress| {
                    progress.current_block = block;
                    progress.last_update = now;
                })
                .or_insert(ShardProgress {
                    current_block: block,
                    blocks_processed: 0,
                    messages_processed: 0,
                    last_update: now,
                });
        }
    }

    /// Update status
    pub fn update_status(&mut self, status: &str) {
        self.status = status.to_string();
        self.last_update = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    /// Set error message
    pub fn set_error(&mut self, error: &str) {
        self.error_message = Some(error.to_string());
        self.status = "error".to_string();
        self.last_update = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }

    /// Increment processed counts
    pub fn increment_processed(&mut self, blocks: u64, messages: u64) {
        self.progress.total_blocks_processed += blocks;
        self.progress.total_messages_processed += messages;
        self.last_update = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }
}

/// Lock file manager
pub struct SyncLockManager {
    lock_file_path: String,
    // 🔒 Mutex to prevent concurrent file writes from parallel shard sync
    write_mutex: Arc<Mutex<()>>,
}

impl SyncLockManager {
    /// Create a new lock manager
    #[must_use]
    pub fn new() -> Self {
        Self {
            lock_file_path: "snaprag.lock".to_string(),
            write_mutex: Arc::new(Mutex::new(())),
        }
    }

    /// Create lock file with enhanced error handling and timeout
    pub fn create_lock(&self, status: &str, sync_range: Option<SyncRange>) -> Result<SyncLockFile> {
        // Check if lock file already exists
        if Path::new(&self.lock_file_path).exists() {
            match self.read_lock() {
                Ok(existing_lock) => {
                    // Check if the process is still running
                    if self.is_process_running(existing_lock.pid) {
                        // Check if the existing process is stuck (no update for too long)
                        if self.is_lock_stale(&existing_lock) {
                            warn!(
                                "Found stale lock file from PID {} (last update: {}s ago), force removing it",
                                existing_lock.pid,
                                self.get_lock_age_seconds(&existing_lock)
                            );
                            self.force_remove_stale_lock(&existing_lock)?;
                        } else {
                            return Err(crate::SnapRagError::Custom(format!(
                                "Sync process is already running (PID: {}) - last update: {}s ago",
                                existing_lock.pid,
                                self.get_lock_age_seconds(&existing_lock)
                            )));
                        }
                    } else {
                        warn!(
                            "Found stale lock file from PID {}, removing it",
                            existing_lock.pid
                        );
                        self.remove_lock()?;
                    }
                }
                Err(e) => {
                    warn!("Found corrupted lock file, removing it: {}", e);
                    self.remove_lock()?;
                }
            }
        }

        let lock = SyncLockFile::new(status, sync_range);
        self.write_lock(&lock)?;
        info!("Created sync lock file (PID: {})", lock.pid);
        Ok(lock)
    }

    /// Check if lock file is stale (no update for more than 5 minutes)
    fn is_lock_stale(&self, lock: &SyncLockFile) -> bool {
        let age_seconds = self.get_lock_age_seconds(lock);
        age_seconds > 300 // 5 minutes
    }

    /// Get age of lock file in seconds
    #[allow(clippy::unused_self)]
    fn get_lock_age_seconds(&self, lock: &SyncLockFile) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        now.saturating_sub(lock.last_update)
    }

    /// Force remove stale lock file
    fn force_remove_stale_lock(&self, lock: &SyncLockFile) -> Result<()> {
        // Try to kill the process first
        if self.is_process_running(lock.pid) {
            warn!("Attempting to kill stale process PID: {}", lock.pid);
            unsafe {
                libc::kill(lock.pid as i32, libc::SIGTERM);
            }

            // Wait a bit for graceful shutdown
            std::thread::sleep(std::time::Duration::from_millis(1000));

            // Force kill if still running
            if self.is_process_running(lock.pid) {
                warn!("Force killing stale process PID: {}", lock.pid);
                unsafe {
                    #[allow(clippy::cast_possible_wrap)]
                    libc::kill(lock.pid as i32, libc::SIGKILL);
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }

        // Remove the lock file
        self.remove_lock()?;
        Ok(())
    }

    /// Read lock file
    pub fn read_lock(&self) -> Result<SyncLockFile> {
        let content = fs::read_to_string(&self.lock_file_path)
            .map_err(|e| crate::SnapRagError::Custom(format!("Failed to read lock file: {e}")))?;

        let lock: SyncLockFile = serde_json::from_str(&content)
            .map_err(|e| crate::SnapRagError::Custom(format!("Failed to parse lock file: {e}")))?;

        Ok(lock)
    }

    /// Write lock file (thread-safe for parallel shard sync)
    /// Merges shard progress from existing file to preserve all shards' data
    pub fn write_lock(&self, lock: &SyncLockFile) -> Result<()> {
        // 🔒 Acquire write mutex to prevent concurrent file writes
        let _guard = self.write_mutex.lock().map_err(|e| {
            crate::SnapRagError::Custom(format!("Failed to acquire lock file mutex: {e}"))
        })?;

        // Read existing lock file to merge shard progress
        let merged_lock = if Path::new(&self.lock_file_path).exists() {
            match self.read_lock() {
                Ok(existing) => {
                    // Start with existing lock
                    let mut merged = existing;

                    // Merge shard progress from new lock
                    for (shard_id, shard_progress) in &lock.progress.shard_progress {
                        merged
                            .progress
                            .shard_progress
                            .insert(*shard_id, shard_progress.clone());
                    }

                    // Update global counters and metadata
                    merged.last_update = lock.last_update;
                    merged.status.clone_from(&lock.status);
                    merged.progress.total_blocks_processed = lock.progress.total_blocks_processed;
                    merged.progress.total_messages_processed =
                        lock.progress.total_messages_processed;

                    merged
                }
                Err(_) => {
                    // If read fails, use new lock as-is
                    lock.clone()
                }
            }
        } else {
            // No existing file, use new lock as-is
            lock.clone()
        };

        let content = serde_json::to_string_pretty(&merged_lock).map_err(|e| {
            crate::SnapRagError::Custom(format!("Failed to serialize lock file: {e}"))
        })?;

        fs::write(&self.lock_file_path, content)
            .map_err(|e| crate::SnapRagError::Custom(format!("Failed to write lock file: {e}")))?;

        Ok(())
    }

    /// Update lock file
    pub fn update_lock(&self, lock: SyncLockFile) -> Result<SyncLockFile> {
        self.write_lock(&lock)?;
        Ok(lock)
    }

    /// Remove lock file
    pub fn remove_lock(&self) -> Result<()> {
        if Path::new(&self.lock_file_path).exists() {
            fs::remove_file(&self.lock_file_path).map_err(|e| {
                crate::SnapRagError::Custom(format!("Failed to remove lock file: {e}"))
            })?;
            info!("Removed sync lock file");
        }
        Ok(())
    }

    /// Check if lock file exists
    #[must_use]
    pub fn lock_exists(&self) -> bool {
        Path::new(&self.lock_file_path).exists()
    }

    /// Check if a process is running
    #[allow(clippy::unused_self)]
    fn is_process_running(&self, pid: u32) -> bool {
        // On Unix systems, kill with signal 0 checks if process exists
        #[allow(clippy::cast_possible_wrap)]
        unsafe {
            libc::kill(pid as i32, 0) == 0
        }
    }

    /// Get lock file path
    #[must_use]
    pub fn lock_file_path(&self) -> &str {
        &self.lock_file_path
    }
}

impl Default for SyncLockManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for SyncLockManager {
    fn clone(&self) -> Self {
        Self {
            lock_file_path: self.lock_file_path.clone(),
            write_mutex: Arc::clone(&self.write_mutex), // Share the same mutex
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn test_lock_file_creation() {
        let manager = SyncLockManager::new();
        let sync_range = SyncRange {
            from_block: 0,
            to_block: Some(100),
        };

        let lock = manager.create_lock("running", Some(sync_range)).unwrap();
        assert_eq!(lock.status, "running");
        assert_eq!(lock.pid, process::id());
        assert!(lock.progress.sync_range.is_some());

        // Clean up
        manager.remove_lock().unwrap();
    }

    #[test]
    #[ignore = "OUTDATED: Progress structure changed from current_shard/current_block to shard_progress map"]
    fn test_lock_file_update() {
        let manager = SyncLockManager::new();
        let lock = manager.create_lock("running", None).unwrap();

        // This test needs to be rewritten for new shard_progress structure
        assert_eq!(lock.progress.total_blocks_processed, 0);
        assert_eq!(lock.progress.total_messages_processed, 0);

        // Clean up
        manager.remove_lock().unwrap();
    }
}
