pub mod batch_insert_test;
pub mod cross_batch_duplicates_test;
pub mod database_tests;
pub mod deterministic_blocks_test;
pub mod event_sourcing_test;
pub mod grpc_shard_chunks_test;
pub mod integration_sync_test;
pub mod message_types_test;
pub mod null_byte_test;
pub mod rag_integration_test;
pub mod real_data_test;
pub mod real_username_proofs_and_frames_test;
pub mod redis_cache_test;
pub mod strict_test_config;
pub mod strict_test_runner;
pub mod strict_test_validation;
pub mod unit_tests;

use crate::config::AppConfig;
use crate::database::Database;
use crate::Result;

/// Load test configuration with safety checks
/// This function MUST be used instead of AppConfig::load() in all tests
pub fn load_test_config() -> Result<AppConfig> {
    // 🛡️ CRITICAL: Force use of test configuration
    let config_path =
        std::env::var("SNAPRAG_CONFIG").unwrap_or_else(|_| "config.test.toml".to_string());

    let config = AppConfig::from_file(&config_path)?;

    // 🛡️ CRITICAL: Verify database URL points to localhost
    let db_url = config.database_url();
    if !is_local_database(db_url) {
        return Err(crate::SnapRagError::Custom(format!(
            "❌ SAFETY CHECK FAILED: Test config must use LOCAL database!\n\
             Current database URL: {}\n\
             \n\
             Tests MUST use localhost database to prevent production data loss.\n\
             \n\
             To run tests safely:\n\
             1. Ensure config.test.toml exists with localhost database\n\
             2. Set environment: export SNAPRAG_CONFIG=config.test.toml\n\
             3. Run: make test-local\n\
             ",
            mask_password(db_url)
        )));
    }

    tracing::debug!("✅ Test config loaded safely from: {}", config_path);
    Ok(config)
}

/// Test helper to create a test database connection
/// NOTE: Tests requiring database access should be marked with #[ignore]
/// to avoid accessing production database
pub async fn create_test_database() -> Result<Database> {
    // 🛡️ CRITICAL: Force use of test configuration in test environment
    let config_path =
        std::env::var("SNAPRAG_CONFIG").unwrap_or_else(|_| "config.test.toml".to_string());

    let config = AppConfig::from_file(&config_path)?;

    // 🛡️ CRITICAL SAFETY CHECK: Ensure we're connecting to localhost
    let db_url = config.database_url();
    if !is_local_database(db_url) {
        return Err(crate::SnapRagError::Custom(format!(
            "❌ SAFETY CHECK FAILED: Tests can only run against LOCAL database!\n\
             Current database URL: {}\n\
             \n\
             Tests MUST use localhost database to prevent production data loss.\n\
             \n\
             To run tests safely:\n\
             1. Create local test database: createdb snaprag_test\n\
             2. Set environment: export SNAPRAG_CONFIG=config.test.toml\n\
             3. Run: make test-local\n\
             \n\
             Or use: make test (runs only non-database tests)\n\
             ",
            mask_password(db_url)
        )));
    }

    tracing::info!("✅ Database safety check passed: using local database");
    let database = Database::from_config(&config).await?;
    Ok(database)
}

/// Check if database URL points to localhost
fn is_local_database(url: &str) -> bool {
    url.contains("localhost")
        || url.contains("127.0.0.1")
        || url.contains("::1")
        || url.contains("@localhost:")
        || url.contains("@127.0.0.1:")
}

/// Mask password in database URL for logging
fn mask_password(url: &str) -> String {
    if let Some(start) = url.find("://") {
        if let Some(at_pos) = url[start + 3..].find('@') {
            let before_creds = &url[..start + 3];
            let after_at = &url[start + 3 + at_pos..];
            if let Some(colon_pos) = url[start + 3..start + 3 + at_pos].find(':') {
                let username = &url[start + 3..start + 3 + colon_pos];
                return format!("{before_creds}{username}:****{after_at}");
            }
        }
    }
    url.to_string()
}

/// Test helper to clean up test data
pub async fn cleanup_test_data(database: &Database, test_fid: i64) -> Result<()> {
    // Clean up user profile changes (event-sourcing table)
    sqlx::query("DELETE FROM user_profile_changes WHERE fid = $1")
        .bind(test_fid)
        .execute(database.pool())
        .await?;

    // Clean up user data changes
    sqlx::query("DELETE FROM user_data_changes WHERE fid = $1")
        .bind(test_fid)
        .execute(database.pool())
        .await?;

    Ok(())
}

/// Test helper to verify data exists in database
pub async fn verify_user_profile_exists(database: &Database, fid: i64) -> Result<bool> {
    let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM user_profiles WHERE fid = $1")
        .bind(fid)
        .fetch_one(database.pool())
        .await?;

    Ok(result.0 > 0)
}

/// Test helper to get user profile data
pub async fn get_user_profile_data(
    database: &Database,
    fid: i64,
) -> Result<Option<(String, String, String)>> {
    let result: Option<(Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT username, display_name, bio FROM user_profiles WHERE fid = $1")
            .bind(fid)
            .fetch_optional(database.pool())
            .await?;

    if let Some((username, display_name, bio)) = result {
        Ok(Some((
            username.unwrap_or_default(),
            display_name.unwrap_or_default(),
            bio.unwrap_or_default(),
        )))
    } else {
        Ok(None)
    }
}
