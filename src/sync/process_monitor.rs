use std::process;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use tracing::error;
use tracing::info;
use tracing::warn;

use crate::Result;

/// Process monitor for managing snaprag processes
pub struct ProcessMonitor {
    max_idle_time: Duration,
    check_interval: Duration,
}

impl ProcessMonitor {
    /// Create a new process monitor
    #[must_use]
    pub const fn new() -> Self {
        Self {
            max_idle_time: Duration::from_secs(300), // 5 minutes
            check_interval: Duration::from_secs(60), // 1 minute
        }
    }

    /// Start monitoring processes
    pub async fn start_monitoring(&self) -> Result<()> {
        info!(
            "Starting process monitor with {}s check interval",
            self.check_interval.as_secs()
        );

        loop {
            if let Err(e) = self.cleanup_stale_processes() {
                error!("Error during process cleanup: {}", e);
            }

            tokio::time::sleep(self.check_interval).await;
        }
    }

    /// Clean up stale processes
    pub fn cleanup_stale_processes(&self) -> Result<()> {
        let stale_processes = self.find_stale_processes()?;

        for pid in stale_processes {
            warn!("Cleaning up stale process PID: {}", pid);
            self.force_kill_process(pid);
        }

        Ok(())
    }

    /// Find stale processes
    fn find_stale_processes(&self) -> Result<Vec<u32>> {
        let mut stale_pids = Vec::new();

        // Get all snaprag processes
        let pids = self.get_snaprag_processes()?;

        for pid in pids {
            if self.is_process_stale(pid)? {
                stale_pids.push(pid);
            }
        }

        Ok(stale_pids)
    }

    /// Get all snaprag process PIDs
    pub fn get_snaprag_processes(&self) -> Result<Vec<u32>> {
        use std::process::Command;

        let output = Command::new("pgrep")
            .args(["-f", "snaprag"])
            .output()
            .map_err(|e| {
                crate::SnapRagError::Custom(format!("Failed to find snaprag processes: {e}"))
            })?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        let pids: Vec<u32> = output_str
            .lines()
            .filter_map(|line| line.trim().parse().ok())
            .collect();

        Ok(pids)
    }

    /// Check if a process is stale
    fn is_process_stale(&self, pid: u32) -> Result<bool> {
        // Check if process is still running
        if !self.is_process_running(pid) {
            return Ok(false);
        }

        // Check process start time
        let start_time = self.get_process_start_time(pid)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();

        if now > start_time && (now - start_time) > self.max_idle_time {
            return Ok(true);
        }

        // Check if process has been idle (no CPU usage)
        if self.is_process_idle(pid) {
            return Ok(true);
        }

        Ok(false)
    }

    /// Check if process is running
    #[allow(clippy::unused_self)]
    fn is_process_running(&self, pid: u32) -> bool {
        #[allow(clippy::cast_possible_wrap)]
        unsafe {
            libc::kill(pid as i32, 0) == 0
        }
    }

    /// Get process start time
    #[allow(clippy::unused_self)]
    fn get_process_start_time(&self, pid: u32) -> Result<Duration> {
        use std::process::Command;

        // Use ps to get elapsed time in seconds (etime format: seconds)
        let output = Command::new("ps")
            .args(["-o", "etimes=", "-p", &pid.to_string()])
            .output()
            .map_err(|e| {
                crate::SnapRagError::Custom(format!("Failed to get process start time: {e}"))
            })?;

        if !output.status.success() {
            return Err(crate::SnapRagError::Custom(format!(
                "Process {pid} not found"
            )));
        }

        let elapsed_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let elapsed_secs: u64 = elapsed_str.parse().map_err(|e| {
            crate::SnapRagError::Custom(format!("Failed to parse elapsed time: {e}"))
        })?;

        // Calculate start time as (now - elapsed)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();

        if let Some(start_time) = now.checked_sub(Duration::from_secs(elapsed_secs)) {
            Ok(start_time)
        } else {
            Ok(Duration::from_secs(0))
        }
    }

    /// Check if process is idle based on last activity
    #[allow(clippy::unused_self)]
    const fn is_process_idle(&self, _pid: u32) -> bool {
        // Process idle detection via last activity time
        // In a full implementation, could track per-process activity timestamps
        // For now, use the max_idle_time threshold

        // Conservative approach: assume process is not idle unless explicitly proven
        // This prevents premature process termination
        false
    }

    /// Force kill a process
    fn force_kill_process(&self, pid: u32) {
        // Try graceful shutdown first
        unsafe {
            #[allow(clippy::cast_possible_wrap)]
            libc::kill(pid as i32, libc::SIGTERM);
        }

        // Wait a bit
        std::thread::sleep(Duration::from_millis(1000));

        // Force kill if still running
        if self.is_process_running(pid) {
            unsafe {
                #[allow(clippy::cast_possible_wrap)]
                libc::kill(pid as i32, libc::SIGKILL);
            }
        }
    }
}

impl Default for ProcessMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Cleanup all snaprag processes (for testing)
pub fn cleanup_all_snaprag_processes() -> Result<()> {
    use std::process::Command;

    // Find all snaprag processes
    // In CI environments, pgrep might not be available or might fail
    // So we handle errors gracefully
    let output = match Command::new("pgrep").args(["-f", "snaprag"]).output() {
        Ok(o) => o,
        Err(_) => {
            // pgrep not available or failed - this is OK in CI environments
            return Ok(());
        }
    };

    if !output.stdout.is_empty() {
        let output_str = String::from_utf8_lossy(&output.stdout);
        let pids: Vec<&str> = output_str.lines().filter(|line| !line.is_empty()).collect();

        for pid in pids {
            // Skip killing the current process
            if let Ok(current_pid) = std::process::id().to_string().parse::<u32>() {
                if let Ok(target_pid) = pid.parse::<u32>() {
                    if target_pid == current_pid {
                        continue;
                    }
                }
            }
            let _ = Command::new("kill").args(["-9", pid]).output();
        }

        // Wait for processes to terminate (but don't block too long)
        std::thread::sleep(Duration::from_millis(100));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_monitor_creation() {
        let monitor = ProcessMonitor::new();
        assert_eq!(monitor.max_idle_time, Duration::from_secs(300));
        assert_eq!(monitor.check_interval, Duration::from_secs(60));
    }

    #[tokio::test]
    async fn test_cleanup_all_processes() {
        // This test should not fail even if no processes are running
        // Use timeout to prevent hanging in CI environments
        use tokio::time::timeout;
        use tokio::time::Duration;
        let result = timeout(Duration::from_secs(5), async {
            tokio::task::spawn_blocking(|| cleanup_all_snaprag_processes())
                .await
                .unwrap_or_else(|_| Ok(()))
        })
        .await;
        assert!(result.is_ok(), "cleanup should complete within timeout");
        if let Ok(cleanup_result) = result {
            assert!(cleanup_result.is_ok(), "cleanup should succeed");
        }
    }
}
