//! Real data integration tests
//!
//! These tests use actual data from Snapchain to verify the sync and processing logic.
//! They require a running Snapchain node and proper configuration.
//!
//! Run with: `cargo test --test real_data_test -- --ignored`

#[cfg(test)]
mod real_data_tests {
    use crate::config::AppConfig;
    use crate::database::Database;
    use crate::sync::client::SnapchainClient;
    use crate::sync::service::SyncService;
    use crate::Result;

    /// Test syncing real data from Snapchain
    ///
    /// This test requires:
    /// 1. A running Snapchain node accessible via config
    /// 2. Database properly initialized
    /// 3. Valid authentication/credentials
    #[tokio::test]
    #[ignore = "Ignored by default as it requires Snapchain setup"]
    async fn test_sync_real_data_from_snapchain() -> Result<()> {
        // Load configuration
        let config = crate::tests::load_test_config()?;

        // Create database connection
        let database = std::sync::Arc::new(Database::from_config(&config).await?);

        // Verify schema is initialized
        database.verify_schema_or_error().await?;

        // Create sync service
        let sync_service = SyncService::new(&config, database.clone()).await?;

        // Sync a small range of blocks for testing
        let test_from_block = 0;
        let test_to_block = 10; // Just 10 blocks for quick test

        println!("📡 Syncing blocks {test_from_block} to {test_to_block}");
        sync_service
            .start_with_range(test_from_block, test_to_block)
            .await?;

        // Wait for sync to complete
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

        // Verify data was synced by checking casts table
        let cast_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM casts")
            .fetch_one(database.pool())
            .await?;

        println!("📊 Synced {cast_count} casts");

        assert!(cast_count > 0, "Should have synced some casts");

        Ok(())
    }

    /// Test fetching a specific user profile from Snapchain
    #[tokio::test]
    #[ignore = "Ignored by default as it requires Snapchain setup"]
    async fn test_fetch_real_user_profile() -> Result<()> {
        use crate::sync::lazy_loader::LazyLoader;

        let config = crate::tests::load_test_config()?;
        let database = std::sync::Arc::new(Database::from_config(&config).await?);
        let client = std::sync::Arc::new(SnapchainClient::from_config(&config).await?);

        let lazy_loader = LazyLoader::new(database.clone(), client);

        // Fetch a known FID (e.g., FID 3 is dwr.eth)
        let test_fid = 3;

        println!("👤 Fetching profile for FID {test_fid}");
        let profile = lazy_loader.fetch_user_profile(test_fid).await?;

        println!("✅ Got profile: {profile:?}");

        assert_eq!(profile.fid, test_fid as i64);
        assert!(profile.username.is_some(), "Should have username");

        Ok(())
    }

    /// Test fetching real casts from Snapchain
    #[tokio::test]
    #[ignore = "Ignored by default as it requires Snapchain setup"]
    async fn test_fetch_real_user_casts() -> Result<()> {
        use crate::sync::lazy_loader::LazyLoader;

        let config = crate::tests::load_test_config()?;
        let database = std::sync::Arc::new(Database::from_config(&config).await?);
        let client = std::sync::Arc::new(SnapchainClient::from_config(&config).await?);

        let lazy_loader = LazyLoader::new(database.clone(), client);

        // Fetch casts for a known active user (e.g., FID 3 is dwr.eth)
        let test_fid = 3;

        println!("📝 Fetching casts for FID {test_fid}");
        let casts = lazy_loader.fetch_user_casts(test_fid).await?;

        println!("✅ Got {} casts", casts.len());

        assert!(!casts.is_empty(), "Should have some casts");

        // Verify cast structure
        if let Some(first_cast) = casts.first() {
            assert_eq!(first_cast.fid, test_fid as i64);
            println!("   First cast: {:?}", first_cast.text);
        }

        Ok(())
    }

    /// Test end-to-end: sync, query, and verify data consistency
    #[tokio::test]
    #[ignore = "Ignored by default as it requires Snapchain setup and takes time"]
    async fn test_end_to_end_sync_and_query() -> Result<()> {
        let config = crate::tests::load_test_config()?;
        let database = std::sync::Arc::new(Database::from_config(&config).await?);

        // 1. Sync some data
        let sync_service = SyncService::new(&config, database.clone()).await?;
        sync_service.start_with_range(0, 50).await?;

        // Wait for sync
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

        // 2. Query profiles
        let profiles = database
            .list_user_profiles(crate::models::UserProfileQuery {
                fid: None,
                username: None,
                display_name: None,
                bio: None,
                location: None,
                twitter_username: None,
                github_username: None,
                limit: Some(10),
                offset: None,
                start_timestamp: None,
                end_timestamp: None,
                sort_by: None,
                sort_order: None,
                search_term: None,
            })
            .await?;

        println!("👥 Found {} profiles", profiles.len());

        // 3. Query casts
        let casts = database
            .list_casts(crate::models::CastQuery {
                fid: None,
                text_search: None,
                parent_hash: None,
                root_hash: None,
                has_mentions: None,
                has_embeds: None,
                start_timestamp: None,
                end_timestamp: None,
                limit: Some(10),
                offset: None,
                sort_by: Some(crate::models::CastSortBy::Timestamp),
                sort_order: Some(crate::models::SortOrder::Desc),
            })
            .await?;

        println!("📝 Found {} casts", casts.len());

        // Verify we got some data
        assert!(
            !profiles.is_empty() || !casts.is_empty(),
            "Should have synced some data"
        );

        Ok(())
    }

    /// Helper function to setup test environment
    ///
    /// This can be expanded based on specific test requirements
    #[allow(dead_code)]
    async fn setup_test_environment() -> Result<(AppConfig, std::sync::Arc<Database>)> {
        let config = crate::tests::load_test_config()?;
        let database = std::sync::Arc::new(Database::from_config(&config).await?);

        // Verify schema
        database.verify_schema_or_error().await?;

        Ok((config, database))
    }

    /// Helper function to cleanup test data
    ///
    /// Use with caution - this will delete data!
    #[allow(dead_code)]
    async fn cleanup_test_data(database: &Database, test_fid: i64) -> Result<()> {
        // Clean up test data for a specific FID
        sqlx::query("DELETE FROM casts WHERE fid = $1")
            .bind(test_fid)
            .execute(database.pool())
            .await?;

        sqlx::query("DELETE FROM links WHERE fid = $1")
            .bind(test_fid)
            .execute(database.pool())
            .await?;

        sqlx::query("DELETE FROM user_profile_changes WHERE fid = $1")
            .bind(test_fid)
            .execute(database.pool())
            .await?;

        Ok(())
    }
}
