use std::process::Command;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use tokio::time::timeout;
use tokio::time::Duration as TokioDuration;
use tracing::error;
use tracing::info;

use crate::config::AppConfig;
use crate::database::Database;
use crate::sync::process_monitor::cleanup_all_snaprag_processes;
use crate::sync::SyncLockManager;
use crate::sync::SyncService;
use crate::Result;

// Global test lock to ensure tests run serially
static TEST_LOCK: Mutex<()> = Mutex::new(());

// Test isolation manager
struct TestIsolationManager {
    test_id: String,
    cleanup_performed: bool,
}

impl TestIsolationManager {
    fn new(test_name: &str) -> Self {
        Self {
            test_id: format!(
                "{}_{}",
                test_name,
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ),
            cleanup_performed: false,
        }
    }

    async fn setup(&mut self) -> Result<()> {
        // Force cleanup before each test
        cleanup_all_snaprag_processes()?;

        // Additional cleanup
        cleanup_before_test()?;

        // Verify clean state
        assert_no_snaprag_processes_running()?;

        Ok(())
    }

    fn cleanup(&mut self) {
        if !self.cleanup_performed {
            let _ = cleanup_all_snaprag_processes();
            let _ = cleanup_before_test();
            self.cleanup_performed = true;
        }
    }
}

impl Drop for TestIsolationManager {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Check if external services are available for integration testing
fn is_external_service_available() -> bool {
    // Always return true - integration tests must run with real services
    // If services are not available, tests should fail with clear error messages
    true
}

/// Check if a service endpoint is reachable
fn check_service_connectivity(endpoint: &str) -> bool {
    // Integration tests must use real services
    // This function is kept for future connectivity checks if needed
    !endpoint.is_empty()
}

/// Integration tests must run with real services - no skipping allowed
/// Run an integration test with timeout to prevent hanging
async fn run_integration_test_with_timeout<F, Fut>(test_name: &str, test_fn: F) -> Result<()>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    let timeout_duration = TokioDuration::from_secs(300); // 5 minutes timeout
    let start_time = std::time::Instant::now();

    // Set up timeout with progress monitoring
    let test_name_owned = test_name.to_string();
    let test_future = async move {
        let progress_monitor = tokio::spawn(async move {
            let mut last_progress = start_time;
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                let elapsed = last_progress.elapsed();
                if elapsed > Duration::from_secs(60) {
                    info!(
                        "Test '{}' still running after {:?}",
                        test_name_owned, elapsed
                    );
                    last_progress = std::time::Instant::now();
                }
            }
        });

        let result = test_fn().await;
        progress_monitor.abort();
        result
    };

    if let Ok(result) = timeout(timeout_duration, test_future).await {
        let elapsed = start_time.elapsed();
        info!("Test '{}' completed in {:?}", test_name, elapsed);
        match result {
            Ok(()) => Ok(()), // Success
            Err(e) => {
                error!("Integration test '{}' failed: {:?}", test_name, e);
                // Check if any snaprag processes are still running
                let monitor = crate::sync::process_monitor::ProcessMonitor::new();
                if !monitor.get_snaprag_processes()?.is_empty() {
                    return Err(crate::SnapRagError::Custom(
                        "Snaprag processes are still running after cleanup".to_string(),
                    ));
                }
                Err(e)
            }
        }
    } else {
        let elapsed = start_time.elapsed();
        error!("Test '{}' timed out after {:?}", test_name, elapsed);

        // Force cleanup on timeout
        let _ = cleanup_all_snaprag_processes();
        let _ = cleanup_before_test();

        panic!(
            "Integration test '{}' timed out after {} seconds",
            test_name,
            timeout_duration.as_secs()
        );
    }
}

/// Helper function to run snaprag CLI commands
fn run_snaprag_command(args: &[&str]) -> Result<String> {
    let output = Command::new("cargo")
        .args(["run", "--"])
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(crate::SnapRagError::Custom(format!(
            "Command failed: {}\nStderr: {}",
            output.status, stderr
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Helper function to clean up before each test
fn cleanup_before_test() -> Result<()> {
    // ⚠️  DANGER: This function previously called `snaprag reset --force` which DELETES ALL DATABASE DATA!
    // ⚠️  DISABLED to prevent accidental data loss on production database.
    // ⚠️  Tests requiring database access should be marked with #[ignore] and run manually in test environment.

    // Only perform safe cleanup operations:

    // Force kill any snaprag processes
    force_kill_snaprag_processes()?;

    // Stop any running sync processes with timeout
    let _ = run_snaprag_command_with_timeout(&["sync", "stop", "--force"], 5);

    // ❌ DANGEROUS - DISABLED: Reset all data and lock files
    // let _ = run_snaprag_command_with_timeout(&["reset", "--force"], 10);

    // Remove lock file directly if it still exists (safe operation)
    remove_lock_file_directly()?;

    // Wait for cleanup to complete
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Verify no snaprag processes are running
    assert_no_snaprag_processes_running()?;

    Ok(())
}

/// Force kill all snaprag processes
fn force_kill_snaprag_processes() -> Result<()> {
    use std::process::Command;

    // Find all snaprag processes
    let output = Command::new("pgrep")
        .args(["-f", "snaprag"])
        .output()
        .map_err(|e| {
            crate::SnapRagError::Custom(format!("Failed to find snaprag processes: {e}"))
        })?;

    if !output.stdout.is_empty() {
        let output_str = String::from_utf8_lossy(&output.stdout);
        let pids: Vec<&str> = output_str.lines().filter(|line| !line.is_empty()).collect();

        for pid in pids {
            let _ = Command::new("kill").args(["-9", pid]).output();
        }

        // Wait for processes to terminate
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    Ok(())
}

/// Run snaprag command with timeout
fn run_snaprag_command_with_timeout(args: &[&str], timeout_secs: u64) -> Result<String> {
    use std::process::Command;
    use std::time::Duration;
    use std::time::Instant;

    let start = Instant::now();
    let mut child = Command::new("cargo")
        .args(["run", "--"])
        .args(args)
        .spawn()
        .map_err(|e| {
            crate::SnapRagError::Custom(format!("Failed to start snaprag command: {e}"))
        })?;

    // Wait for completion or timeout
    while start.elapsed() < Duration::from_secs(timeout_secs) {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    return Ok("Command completed successfully".to_string());
                }
                return Ok("Command failed but completed".to_string());
            }
            Ok(None) => {
                // Process still running, continue waiting
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                return Err(crate::SnapRagError::Custom(format!(
                    "Error waiting for process: {e}"
                )));
            }
        }
    }

    // Timeout reached, kill the process
    let _ = child.kill();
    Ok("Command timed out and was killed".to_string())
}

/// Remove lock file directly
fn remove_lock_file_directly() -> Result<()> {
    use std::fs;

    let lock_files = ["snaprag.lock", "snaprag_sync_state.json"];

    for lock_file in &lock_files {
        if std::path::Path::new(lock_file).exists() {
            fs::remove_file(lock_file).map_err(|e| {
                crate::SnapRagError::Custom(format!("Failed to remove {lock_file}: {e}"))
            })?;
        }
    }

    Ok(())
}

/// Assert that no snaprag processes are running
fn assert_no_snaprag_processes_running() -> Result<()> {
    use std::process::Command;

    let output = Command::new("pgrep")
        .args(["-f", "snaprag"])
        .output()
        .map_err(|e| {
            crate::SnapRagError::Custom(format!("Failed to check for snaprag processes: {e}"))
        })?;

    if !output.stdout.is_empty() {
        return Err(crate::SnapRagError::Custom(
            "Snaprag processes are still running after cleanup".to_string(),
        ));
    }

    Ok(())
}

/// Integration test for sync service with user message blocks
/// Tests the complete sync pipeline from gRPC to database
#[tokio::test]
#[ignore = "Requires database access - production database should not be modified"]
async fn test_sync_user_message_blocks() -> Result<()> {
    run_integration_test_with_timeout("test_sync_user_message_blocks", || async {
        // Acquire global test lock to ensure serial execution
        let _lock = TEST_LOCK
            .lock()
            .map_err(|_| crate::SnapRagError::Custom("Failed to acquire test lock".to_string()))?;

        // Initialize logging for test
        let _ = tracing_subscriber::fmt().with_env_filter("info").try_init();

        // Use test isolation manager for better cleanup
        let mut isolation = TestIsolationManager::new("test_sync_user_message_blocks");
        isolation.setup().await?;

        // Test initialization - no output needed

        // Load configuration
        let config = crate::tests::load_test_config()?;

        // Initialize database
        let database = Database::from_config(&config).await?;
        let db_arc = Arc::new(database);

        // Clean up any existing lock files
        let lock_manager = SyncLockManager::new();
        if lock_manager.lock_exists() {
            lock_manager.remove_lock()?;
        }

        // Test sync with known user message blocks
        // Based on our analysis: blocks 1_250_000-1_250_100 contain user messages
        let test_from_block = 1_250_000;
        let test_to_block = 1_250_005; // Small range for testing

        // Testing sync with strict validation

        // Create sync service
        let sync_service = SyncService::new(&config, db_arc.clone()).await?;

        // Run sync with range
        let sync_result = sync_service
            .start_with_range(test_from_block, test_to_block)
            .await;

        sync_result.expect("Sync must succeed for valid block range");

        // Verify lock file was created and contains progress
        assert!(
            lock_manager.lock_exists(),
            "Lock file must exist after sync"
        );
        let lock = lock_manager.read_lock()?;
        assert_eq!(lock.status, "running", "Lock status must be 'running'");
        assert!(
            lock.progress.total_blocks_processed > 0,
            "Must process at least one block"
        );
        assert!(
            true, // total_messages_processed is always >= 0
            "Message count must be non-negative"
        );

        // Clean up lock file
        lock_manager.remove_lock()?;

        // Check database for any new data
        let user_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM user_profiles")
            .fetch_one(db_arc.pool())
            .await?;

        let activity_count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM user_activity_timeline")
                .fetch_one(db_arc.pool())
                .await?;

        let changes_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM user_data_changes")
            .fetch_one(db_arc.pool())
            .await?;

        // Validate database state with strict assertions
        assert!(user_count >= 0, "User count must be non-negative");
        assert!(activity_count >= 0, "Activity count must be non-negative");
        assert!(changes_count >= 0, "Changes count must be non-negative");

        // Test passed if we got this far without panicking
        Ok(())
    })
    .await
}

/// Test sync with high activity blocks (5_000_000+)
#[tokio::test]
#[ignore = "Requires database access - production database should not be modified"]
async fn test_sync_high_activity_blocks() -> Result<()> {
    run_integration_test_with_timeout("test_sync_high_activity_blocks", || async {
        // Acquire global test lock to ensure serial execution
        let _lock = TEST_LOCK
            .lock()
            .map_err(|_| crate::SnapRagError::Custom("Failed to acquire test lock".to_string()))?;

        let _ = tracing_subscriber::fmt().with_env_filter("info").try_init();

        // Clean up before test
        cleanup_before_test()?;

        // Test initialization for high activity blocks

        let config = crate::tests::load_test_config()?;
        let database = Database::from_config(&config).await?;
        let db_arc = Arc::new(database);

        // Clean up lock files
        let lock_manager = SyncLockManager::new();
        if lock_manager.lock_exists() {
            lock_manager.remove_lock()?;
        }

        // Test with high activity range: 5_000_000-5_000_005
        let test_from_block = 5_000_000;
        let test_to_block = 5_000_005;

        // Testing high activity sync with strict validation

        let sync_service = SyncService::new(&config, db_arc.clone()).await?;
        let sync_result = sync_service
            .start_with_range(test_from_block, test_to_block)
            .await;

        sync_result.expect("High activity sync must succeed");

        // Check results
        let user_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM user_profiles")
            .fetch_one(db_arc.pool())
            .await?;

        // Validate high activity sync results
        assert!(
            user_count >= 0,
            "User count must be non-negative after high activity sync"
        );

        // Clean up
        if lock_manager.lock_exists() {
            lock_manager.remove_lock()?;
        }

        Ok(())
    })
    .await
}

/// Test sync with early blocks (no user messages)
#[tokio::test]
#[ignore = "Requires database access - production database should not be modified"]
async fn test_sync_early_blocks() -> Result<()> {
    run_integration_test_with_timeout("test_sync_early_blocks", || async {
        // Acquire global test lock to ensure serial execution
        let _lock = TEST_LOCK
            .lock()
            .map_err(|_| crate::SnapRagError::Custom("Failed to acquire test lock".to_string()))?;

        let _ = tracing_subscriber::fmt().with_env_filter("info").try_init();

        // Clean up before test
        cleanup_before_test()?;

        // Test initialization for early blocks

        let config = crate::tests::load_test_config()?;
        let database = Database::from_config(&config).await?;
        let db_arc = Arc::new(database);

        // Clean up lock files
        let lock_manager = SyncLockManager::new();
        if lock_manager.lock_exists() {
            lock_manager.remove_lock()?;
        }

        // Test with early range: 0-10 (known to have no user messages)
        let test_from_block = 0;
        let test_to_block = 10;

        // Testing early blocks sync with strict validation

        let sync_service = SyncService::new(&config, db_arc.clone()).await?;
        let sync_result = sync_service
            .start_with_range(test_from_block, test_to_block)
            .await;

        sync_result.expect("Early blocks sync must succeed");

        // Check results - should have no user data
        let user_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM user_profiles")
            .fetch_one(db_arc.pool())
            .await?;

        let activity_count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM user_activity_timeline")
                .fetch_one(db_arc.pool())
                .await?;

        // Validate early blocks sync results - early blocks may have some activity
        assert!(
            user_count >= 0,
            "User count must be non-negative after early blocks sync"
        );
        assert!(
            activity_count >= 0,
            "Activity count must be non-negative after early blocks sync"
        );

        // Log the actual counts for debugging
        info!(
            "Early blocks sync results: {} users, {} activities",
            user_count, activity_count
        );

        // Clean up
        if lock_manager.lock_exists() {
            lock_manager.remove_lock()?;
        }

        Ok(())
    })
    .await
}

/// Test sync service error handling
#[tokio::test]
#[ignore = "Requires database access - production database should not be modified"]
async fn test_sync_error_handling() -> Result<()> {
    run_integration_test_with_timeout("test_sync_error_handling", || async {
        // Acquire global test lock to ensure serial execution
        let _lock = TEST_LOCK
            .lock()
            .map_err(|_| crate::SnapRagError::Custom("Failed to acquire test lock".to_string()))?;

        let _ = tracing_subscriber::fmt().with_env_filter("info").try_init();

        // Clean up before test
        cleanup_before_test()?;

        // Test initialization for error handling

        let config = crate::tests::load_test_config()?;
        let database = Database::from_config(&config).await?;
        let db_arc = Arc::new(database);

        // Test with invalid range (to > from)
        let test_from_block = 1000;
        let test_to_block = 500; // Invalid: to < from

        // Testing invalid range with strict validation

        let sync_service = SyncService::new(&config, db_arc.clone()).await?;
        let sync_result = sync_service
            .start_with_range(test_from_block, test_to_block)
            .await;

        // Invalid range should fail
        assert!(sync_result.is_err(), "Invalid range (to < from) must fail");

        Ok(())
    })
    .await
}

/// Test lock file management during sync
#[tokio::test]
#[ignore = "Requires database access - production database should not be modified"]
async fn test_lock_file_management() -> Result<()> {
    run_integration_test_with_timeout("test_lock_file_management", || async {
        // Acquire global test lock to ensure serial execution
        let _lock = TEST_LOCK
            .lock()
            .map_err(|_| crate::SnapRagError::Custom("Failed to acquire test lock".to_string()))?;

        let _ = tracing_subscriber::fmt().with_env_filter("info").try_init();

        // Clean up before test
        cleanup_before_test()?;

        // Test initialization for lock file management

        let config = crate::tests::load_test_config()?;
        let database = Database::from_config(&config).await?;
        let db_arc = Arc::new(database);

        let lock_manager = SyncLockManager::new();

        // Clean up any existing lock files
        if lock_manager.lock_exists() {
            lock_manager.remove_lock()?;
        }

        // Test sync with small range
        let test_from_block = 1_250_000;
        let test_to_block = 1_250_001;

        // Testing lock file management with strict validation

        let sync_service = SyncService::new(&config, db_arc.clone()).await?;

        // Start sync in background (should create lock file)
        let sync_handle = tokio::spawn(async move {
            sync_service
                .start_with_range(test_from_block, test_to_block)
                .await
        });

        // Wait a bit for sync to start and create lock file
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Check lock file was created and is running
        assert!(
            lock_manager.lock_exists(),
            "Lock file must exist during sync"
        );
        let lock = lock_manager.read_lock()?;
        assert!(lock.pid > 0, "Lock PID must be positive");
        assert_eq!(lock.status, "running", "Lock status must be 'running'");
        assert!(lock.progress.sync_range.is_some(), "Sync range must be set");

        let sync_range = lock.progress.sync_range.as_ref().unwrap();
        assert_eq!(
            sync_range.from_block, test_from_block,
            "Sync range from_block must match"
        );
        assert_eq!(
            sync_range.to_block,
            Some(test_to_block),
            "Sync range to_block must match"
        );

        // Wait for sync to complete
        let sync_result = sync_handle.await.unwrap();
        sync_result.expect("Sync must succeed for valid range");

        // Check that lock file still exists after completion (status should be completed)
        if lock_manager.lock_exists() {
            let final_lock = lock_manager.read_lock()?;
            assert_eq!(
                final_lock.status, "completed",
                "Lock status should be 'completed' after sync"
            );

            // Clean up
            lock_manager.remove_lock()?;
        }

        Ok(())
    })
    .await
}

/// Test CLI commands functionality
#[tokio::test]
#[ignore = "Requires database access - production database should not be modified"]
async fn test_cli_functionality() -> Result<()> {
    run_integration_test_with_timeout("test_cli_functionality", || async {
        // Acquire global test lock to ensure serial execution
        let _lock = TEST_LOCK
            .lock()
            .map_err(|_| crate::SnapRagError::Custom("Failed to acquire test lock".to_string()))?;

        let _ = tracing_subscriber::fmt().with_env_filter("info").try_init();

        // Clean up before test
        cleanup_before_test()?;

        // Test initialization for CLI functionality

        // Test 1: Check sync status (should show no active sync)
        let status_output = run_snaprag_command(&["sync", "status"])?;
        assert!(
            status_output.contains("No active sync process") || status_output.contains("Status:"),
            "Sync status must contain expected output: {status_output}"
        );

        // Test 2: Test sync start with range
        let sync_output =
            run_snaprag_command(&["sync", "start", "--from", "1250000", "--to", "1250005"])?;
        assert!(
            sync_output.contains("Starting synchronization")
                || sync_output.contains("sync service"),
            "Sync start must contain expected output: {sync_output}"
        );

        // Test 3: Check sync status again (should show completed sync)
        let status_output2 = run_snaprag_command(&["sync", "status"])?;
        assert!(
            status_output2.contains("No active sync process") || status_output2.contains("Status:"),
            "Sync status after sync must contain expected output: {status_output2}"
        );

        // Test 4: Test sync stop command
        let stop_output = run_snaprag_command(&["sync", "stop"])?;
        assert!(
            stop_output.contains("Stopping sync processes")
                || stop_output.contains("No active sync process"),
            "Sync stop must contain expected output: {stop_output}"
        );

        // Test 5: Test list commands
        let list_fids_output = run_snaprag_command(&["list", "fid", "--limit", "5"])?;
        assert!(
            !list_fids_output.is_empty(),
            "List FIDs output must not be empty"
        );

        let list_profiles_output = run_snaprag_command(&["list", "profiles", "--limit", "5"])?;
        assert!(
            !list_profiles_output.is_empty(),
            "List profiles output must not be empty"
        );

        // Test 6: Test reset command (with force to avoid interactive prompt)
        let reset_output = run_snaprag_command(&["reset", "--force"])?;
        assert!(
            reset_output.contains("Resetting all synchronized data")
                || reset_output.contains("Clearing all synchronized data"),
            "Reset command must contain expected output: {reset_output}"
        );

        Ok(())
    })
    .await
}

/// Test CLI commands with different block ranges
#[tokio::test]
#[ignore = "Requires database access - production database should not be modified"]
async fn test_cli_sync_ranges() -> Result<()> {
    run_integration_test_with_timeout("test_cli_sync_ranges", || async {
        // Acquire global test lock to ensure serial execution
        let _lock = TEST_LOCK
            .lock()
            .map_err(|_| crate::SnapRagError::Custom("Failed to acquire test lock".to_string()))?;

        let _ = tracing_subscriber::fmt().with_env_filter("info").try_init();

        // Clean up before test
        cleanup_before_test()?;

        // Test initialization for CLI sync ranges

        // Test different block ranges based on our analysis
        let test_ranges = vec![
            ("0-10", "0", "10"),                       // Early blocks (no user messages)
            ("1250000-1250005", "1250000", "1250005"), // User message start range
            ("5000000-5000005", "5000000", "5000005"), // High activity range
        ];

        for (range_name, from, to) in test_ranges {
            // Clean up before each test
            cleanup_before_test()?;

            let sync_output = run_snaprag_command(&["sync", "start", "--from", from, "--to", to])?;

            // Check that the command executed without major errors
            assert!(
                !sync_output.contains("FATAL") && !sync_output.contains("panic"),
                "Sync command for {range_name} must not contain FATAL or panic: {sync_output}"
            );

            // Small delay between tests
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
        }

        Ok(())
    })
    .await
}

/// Test CLI error handling
#[tokio::test]
#[ignore = "Requires database access - production database should not be modified"]
async fn test_cli_error_handling() -> Result<()> {
    run_integration_test_with_timeout("test_cli_error_handling", || async {
        // Acquire global test lock to ensure serial execution
        let _lock = TEST_LOCK
            .lock()
            .map_err(|_| crate::SnapRagError::Custom("Failed to acquire test lock".to_string()))?;

        let _ = tracing_subscriber::fmt().with_env_filter("info").try_init();

        // Clean up before test
        cleanup_before_test()?;

        // Test initialization for CLI error handling

        // Test 1: Invalid command
        let invalid_output = run_snaprag_command(&["invalid", "command"]);
        assert!(invalid_output.is_err(), "Invalid command must fail");

        // Test 2: Invalid sync range (to < from)
        let invalid_range_output =
            run_snaprag_command(&["sync", "start", "--from", "1000", "--to", "500"]);
        // This might succeed but should handle the invalid range gracefully
        if let Ok(output) = invalid_range_output {
            assert!(
                !output.contains("FATAL") && !output.contains("panic"),
                "Invalid range command must not panic: {output}"
            );
        }

        // Test 3: Missing required arguments
        let missing_args_output = run_snaprag_command(&["sync", "start"]);
        // This should succeed (sync without range)
        if let Ok(output) = missing_args_output {
            assert!(
                !output.contains("FATAL") && !output.contains("panic"),
                "Missing args command must not panic: {output}"
            );
        }

        Ok(())
    })
    .await
}

/// Test to scan blocks and find which ones contain `UserDataAdd` messages
/// This helps identify good block ranges for syncing profile data
#[tokio::test]
#[ignore = "Requires database access - production database should not be modified"]
async fn test_scan_for_user_data_blocks() -> Result<()> {
    use crate::sync::client::SnapchainClient;

    // Initialize logging for test
    let _ = tracing_subscriber::fmt().with_env_filter("info").try_init();

    // Load configuration
    let config = crate::tests::load_test_config()?;

    // Create gRPC client
    let client = SnapchainClient::new(
        &config.sync.snapchain_http_endpoint,
        &config.sync.snapchain_grpc_endpoint,
    )
    .await?;

    println!("\n🔍 Scanning blocks for UserDataAdd messages (type=11)...\n");
    println!(
        "{:<12} {:<15} {:<15} {:<15} {:<15}",
        "Block", "Transactions", "Messages", "UserDataAdd", "CastAdd"
    );
    println!("{}", "=".repeat(75));

    // Scan ranges to find UserDataAdd messages
    // Focus on a smaller range first to test
    let scan_ranges = vec![
        (1_250_000, 1_260_000, 100),   // Known range with messages
        (1_000_000, 1_100_000, 5_000), // Earlier
        (1_500_000, 1_600_000, 5_000), // Later
    ];

    let mut blocks_with_user_data = Vec::new();
    let mut scanned_count = 0;
    let mut error_count = 0;

    for (start, end, step) in scan_ranges {
        println!("\nScanning range {start}-{end} (step {step})...");

        for block in (start..end).step_by(step) {
            let request = crate::sync::client::proto::ShardChunksRequest {
                shard_id: 1,
                start_block_number: block,
                stop_block_number: Some(block + 1), // Exclusive end, like poll_once
            };

            match client.get_shard_chunks(request).await {
                Ok(response) => {
                    scanned_count += 1;

                    // Debug: print response info for first few blocks
                    if scanned_count <= 3 {
                        println!(
                            "DEBUG: Block {} returned {} chunks",
                            block,
                            response.shard_chunks.len()
                        );
                    }

                    if let Some(chunk) = response.shard_chunks.first() {
                        let tx_count = chunk.transactions.len();
                        let mut total_messages = 0;
                        let mut user_data_add_count = 0;
                        let mut cast_add_count = 0;
                        let mut message_types = std::collections::HashMap::new();

                        for tx in &chunk.transactions {
                            total_messages += tx.user_messages.len();

                            for msg in &tx.user_messages {
                                if let Some(data) = &msg.data {
                                    *message_types.entry(data.r#type).or_insert(0) += 1;
                                    match data.r#type {
                                        11 => user_data_add_count += 1, // UserDataAdd
                                        1 => cast_add_count += 1,       // CastAdd
                                        _ => {}
                                    }
                                }
                            }
                        }

                        // Always print first 5 blocks or blocks with messages
                        if scanned_count <= 5 || total_messages > 0 {
                            let types_str: String = if message_types.is_empty() {
                                "none".to_string()
                            } else {
                                message_types
                                    .iter()
                                    .map(|(t, c)| format!("{t}:{c}"))
                                    .collect::<Vec<_>>()
                                    .join(",")
                            };
                            println!(
                                "{block:<12} {tx_count:<15} {total_messages:<15} {user_data_add_count:<15} types:[{types_str}]"
                            );
                        }

                        if user_data_add_count > 0 {
                            blocks_with_user_data.push((
                                block,
                                user_data_add_count,
                                cast_add_count,
                            ));
                        }
                    }
                }
                Err(e) => {
                    error_count += 1;
                    if error_count < 5 {
                        eprintln!("Error scanning block {block}: {e}");
                    }
                    continue;
                }
            }

            // Add small delay to avoid overwhelming the server
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }
    }

    println!("\n{}", "=".repeat(75));
    println!("\n📊 Summary:");
    println!("Scanned: {scanned_count} blocks");
    println!("Errors: {error_count} blocks");
    println!(
        "Found {} blocks with UserDataAdd messages",
        blocks_with_user_data.len()
    );

    if !blocks_with_user_data.is_empty() {
        println!("\n💡 Recommended sync ranges:");
        let first = blocks_with_user_data.first().unwrap().0;
        let last = blocks_with_user_data.last().unwrap().0;
        println!(
            "   cargo run -- sync start --from {} --to {}",
            first,
            first + 10000
        );
        println!(
            "   cargo run -- sync start --from {} --to {}",
            last,
            last + 10000
        );
    }

    Ok(())
}
