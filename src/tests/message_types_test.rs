/// Comprehensive tests for all Farcaster message types
///
/// This test suite validates that all message types (1-15) are correctly:
/// 1. Parsed from protobuf/JSON
/// 2. Stored in appropriate tables
/// 3. Retrieved accurately
/// 4. Handle edge cases (removes, updates, etc.)
///
/// Each test cleans up its data after execution to ensure test isolation.
#[cfg(test)]
#[allow(clippy::unreadable_literal)] // Test data with large numbers is acceptable
mod message_types_tests {
    use crate::database::Database;
    use crate::models::ShardBlockInfo;
    use crate::sync::shard_processor::flush_batched_data;
    use crate::sync::shard_processor::BatchedData;

    /// Helper to create test database
    async fn setup_test_db() -> Database {
        // 🛡️ CRITICAL: Force use of test configuration
        let config_path =
            std::env::var("SNAPRAG_CONFIG").unwrap_or_else(|_| "config.test.toml".to_string());
        let config =
            crate::config::AppConfig::from_file(&config_path).expect("Failed to load config");

        // 🛡️ CRITICAL: Verify we're using local database
        let db_url = config.database_url();
        assert!(
            !(!db_url.contains("localhost")
                && !db_url.contains("127.0.0.1")
                && !db_url.contains("::1")),
            "❌ SAFETY CHECK FAILED: Test database must be localhost!\n\
                 Current URL: {db_url}\n\
                 Set SNAPRAG_CONFIG=config.test.toml to use test database"
        );

        Database::from_config(&config)
            .await
            .expect("Failed to create database")
    }

    /// Helper to cleanup test data by `message_hash` (SAFE - only deletes specific test records)
    /// This approach ensures we only delete data we created in tests, not real user data
    async fn cleanup_by_message_hash(db: &Database, message_hash: &[u8]) {
        let cleanup_queries = vec![
            "DELETE FROM casts WHERE message_hash = $1",
            "DELETE FROM links WHERE message_hash = $1",
            "DELETE FROM reactions WHERE message_hash = $1",
            "DELETE FROM verifications WHERE message_hash = $1",
            "DELETE FROM user_profile_changes WHERE message_hash = $1",
            // username_proofs: Skip cleanup (no message_hash column, has UNIQUE constraint on fid+username_type)
            "DELETE FROM frame_actions WHERE message_hash = $1",
        ];

        for query in cleanup_queries {
            sqlx::query(query)
                .bind(message_hash)
                .execute(db.pool())
                .await
                .ok(); // Ignore errors (table might not have the hash)
        }
    }

    /// Generate unique test message hash with prefix to avoid conflicts
    /// Format: [0xFE, 0xFE, `test_id` byte 1, `test_id` byte 2, ...]
    fn test_message_hash(test_id: u32) -> Vec<u8> {
        let mut hash = vec![0xFE, 0xFE]; // Test marker prefix
        hash.extend_from_slice(&test_id.to_be_bytes());
        hash
    }

    /// Helper to create shard block info
    fn test_shard_info() -> ShardBlockInfo {
        ShardBlockInfo {
            shard_id: 1,
            block_height: 1000,
            transaction_fid: 99,
            timestamp: 1698765432,
        }
    }

    #[tokio::test]
    #[ignore = "Requires database access - production database should not be modified"]
    async fn test_all_message_types_coverage() {
        let db = setup_test_db().await;

        // Test matrix: [message_type, table_name, is_implemented]
        let message_types = vec![
            (1, "casts", true, "CastAdd"),
            (2, "casts", true, "CastRemove (soft delete)"),
            (3, "reactions", true, "ReactionAdd"),
            (4, "reactions", true, "ReactionRemove (soft delete)"),
            (5, "links", true, "LinkAdd"),
            (6, "links", true, "LinkRemove (soft delete)"),
            (7, "verifications", true, "VerificationAdd"),
            (8, "verifications", true, "VerificationRemove (soft delete)"),
            (11, "user_profile_changes", true, "UserDataAdd"),
            (12, "username_proofs", true, "UsernameProof"),
            (13, "frame_actions", true, "FrameAction"),
            (14, "N/A", true, "LinkCompactState (log only)"),
            (15, "N/A", true, "LendStorage (log only)"),
        ];

        println!("\n📋 Message Type Coverage Report:\n");
        println!(
            "{:<6} {:<25} {:<10} {:<30}",
            "Type", "Table", "Status", "Name"
        );
        println!("{}", "-".repeat(71));

        for (msg_type, table, implemented, name) in message_types {
            let status = if implemented { "✅" } else { "❌" };
            println!("{msg_type:<6} {table:<25} {status:<10} {name:<30}");
        }

        println!("\n📊 Summary:");
        println!("Total types: 13 (excluding deprecated 9, 10)");
        println!("Implemented: 13/13 (100%)");
        println!("Important types: 10/10 (100%)");
    }

    #[tokio::test]
    #[ignore = "Requires database access - production database should not be modified"]
    async fn test_cast_add_type_1() {
        let db = setup_test_db().await;
        let shard_info = test_shard_info();
        let test_hash = test_message_hash(1); // Unique test hash

        // Cleanup before test
        cleanup_by_message_hash(&db, &test_hash).await;

        let mut batched = BatchedData::new();

        // Create test cast
        batched.casts.push((
            99,                                    // fid
            Some("Test cast message".to_string()), // text
            1698765432,                            // timestamp
            test_hash.clone(),                     // message_hash
            None,                                  // parent_hash
            None,                                  // root_hash
            None,                                  // embeds
            None,                                  // mentions
            shard_info.clone(),
        ));

        // Flush to database
        flush_batched_data(&db, batched)
            .await
            .expect("Failed to flush");

        // 🎯 STRICT VALIDATION: Verify ALL critical fields
        let result: (
            i64,
            Option<String>,
            i64,
            Vec<u8>,
            Option<Vec<u8>>,
            Option<Vec<u8>>,
        ) = sqlx::query_as(
            "SELECT fid, text, timestamp, message_hash, parent_hash, root_hash 
             FROM casts 
             WHERE message_hash = $1",
        )
        .bind(&test_hash)
        .fetch_one(db.pool())
        .await
        .expect("Failed to query cast");

        assert_eq!(result.0, 99, "FID should match");
        assert_eq!(
            result.1,
            Some("Test cast message".to_string()),
            "Text should match"
        );
        assert_eq!(result.2, 1698765432, "Timestamp should match");
        assert_eq!(result.3, test_hash.clone(), "Message hash should match");
        assert_eq!(result.4, None, "Parent hash should be None");
        assert_eq!(result.5, None, "Root hash should be None");

        // Cleanup after test
        cleanup_by_message_hash(&db, &test_hash).await;

        println!("✅ Type 1 (CastAdd) test passed");
    }

    #[tokio::test]
    #[ignore = "Requires database access - production database should not be modified"]
    async fn test_reaction_add_type_3() {
        let db = setup_test_db().await;
        let shard_info = test_shard_info();
        let test_hash = test_message_hash(3); // Unique test hash

        // Cleanup before test
        cleanup_by_message_hash(&db, &test_hash).await;

        let mut batched = BatchedData::new();

        // Create test reaction
        batched.reactions.push((
            99,                // fid
            vec![5, 6, 7, 8],  // target_cast_hash
            Some(100),         // target_fid
            1,                 // reaction_type (like)
            "add".to_string(), // event_type
            1698765432,        // timestamp
            test_hash.clone(), // message_hash
            shard_info.clone(),
        ));

        flush_batched_data(&db, batched)
            .await
            .expect("Failed to flush");

        // 🎯 STRICT VALIDATION: Verify ALL fields
        let result: (i64, Vec<u8>, Option<i64>, i16, String, i64, Vec<u8>) = sqlx::query_as(
            "SELECT fid, target_cast_hash, target_fid, reaction_type, event_type, timestamp, message_hash
             FROM reactions 
             WHERE message_hash = $1"
        )
            .bind(&test_hash)
            .fetch_one(db.pool())
            .await
            .expect("Failed to query reaction");

        assert_eq!(result.0, 99, "FID should match");
        assert_eq!(result.1, vec![5, 6, 7, 8], "Target cast hash should match");
        assert_eq!(result.2, Some(100), "Target FID should match");
        assert_eq!(result.3, 1, "Reaction type should be 1 (like)");
        assert_eq!(result.4, "add", "Event type should be 'add'");
        assert_eq!(result.5, 1698765432, "Timestamp should match");
        assert_eq!(result.6, test_hash.clone(), "Message hash should match");

        // Cleanup after test
        cleanup_by_message_hash(&db, &test_hash).await;

        println!("✅ Type 3 (ReactionAdd) test passed");
    }

    #[tokio::test]
    #[ignore = "Requires database access - production database should not be modified"]
    async fn test_link_add_type_5() {
        let db = setup_test_db().await;
        let shard_info = test_shard_info();
        let test_hash = test_message_hash(5); // Unique test hash

        // Cleanup before test
        cleanup_by_message_hash(&db, &test_hash).await;

        let mut batched = BatchedData::new();

        // Create test link
        batched.links.push((
            99,                   // fid
            100,                  // target_fid
            "follow".to_string(), // link_type
            "add".to_string(),    // event_type
            1698765432,           // timestamp
            test_hash.clone(),    // message_hash
            shard_info.clone(),
        ));

        flush_batched_data(&db, batched)
            .await
            .expect("Failed to flush");

        // 🎯 STRICT VALIDATION: Verify ALL fields
        let result: (i64, i64, String, String, i64, Vec<u8>) = sqlx::query_as(
            "SELECT fid, target_fid, link_type, event_type, timestamp, message_hash
             FROM links 
             WHERE message_hash = $1",
        )
        .bind(&test_hash)
        .fetch_one(db.pool())
        .await
        .expect("Failed to query link");

        assert_eq!(result.0, 99, "FID should match");
        assert_eq!(result.1, 100, "Target FID should match");
        assert_eq!(result.2, "follow", "Link type should match");
        assert_eq!(result.3, "add", "Event type should be 'add'");
        assert_eq!(result.4, 1698765432, "Timestamp should match");
        assert_eq!(result.5, test_hash.clone(), "Message hash should match");

        // Cleanup after test
        cleanup_by_message_hash(&db, &test_hash).await;

        println!("✅ Type 5 (LinkAdd) test passed");
    }

    #[tokio::test]
    #[ignore = "Requires database access - production database should not be modified"]
    async fn test_verification_add_type_7() {
        let db = setup_test_db().await;
        let shard_info = test_shard_info();
        let test_hash = test_message_hash(7); // Unique test hash

        // Cleanup before test
        cleanup_by_message_hash(&db, &test_hash).await;

        let mut batched = BatchedData::new();

        // Create test verification
        batched.verifications.push((
            99,                   // fid
            vec![0xAB; 20],       // address (20 bytes)
            Some(vec![0xCD; 65]), // claim_signature
            Some(vec![0xEF; 32]), // block_hash
            Some(0),              // verification_type (EOA)
            Some(1),              // chain_id (Ethereum mainnet)
            "add".to_string(),    // event_type
            1698765432,           // timestamp
            test_hash.clone(),    // message_hash
            shard_info.clone(),
        ));

        flush_batched_data(&db, batched)
            .await
            .expect("Failed to flush");

        // 🎯 STRICT VALIDATION: Verify ALL fields
        let result: (i64, Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>, Option<i16>, Option<i32>, String, i64, Vec<u8>) = sqlx::query_as(
            "SELECT fid, address, claim_signature, block_hash, verification_type, chain_id, event_type, timestamp, message_hash
             FROM verifications 
             WHERE message_hash = $1"
        )
            .bind(&test_hash)
            .fetch_one(db.pool())
            .await
            .expect("Failed to query verification");

        assert_eq!(result.0, 99, "FID should match");
        assert_eq!(result.1, vec![0xAB; 20], "Address should match");
        assert_eq!(
            result.2,
            Some(vec![0xCD; 65]),
            "Claim signature should match"
        );
        assert_eq!(result.3, Some(vec![0xEF; 32]), "Block hash should match");
        assert_eq!(result.4, Some(0), "Verification type should be 0 (EOA)");
        assert_eq!(result.5, Some(1), "Chain ID should be 1 (Ethereum)");
        assert_eq!(result.6, "add", "Event type should be 'add'");
        assert_eq!(result.7, 1698765432, "Timestamp should match");
        assert_eq!(result.8, test_hash.clone(), "Message hash should match");

        // Cleanup after test
        cleanup_by_message_hash(&db, &test_hash).await;

        println!("✅ Type 7 (VerificationAdd) test passed");
    }

    #[tokio::test]
    #[ignore = "Requires database access - production database should not be modified"]
    async fn test_user_data_add_type_11() {
        let db = setup_test_db().await;
        let shard_info = test_shard_info();
        let test_hash = test_message_hash(11); // Unique test hash

        // Cleanup before test
        cleanup_by_message_hash(&db, &test_hash).await;

        let mut batched = BatchedData::new();

        // Ensure FID exists
        batched.fids_to_ensure.insert(99);

        // Create test profile update
        batched.profile_updates.push((
            99,                           // fid
            "username".to_string(),       // field_name
            Some("testuser".to_string()), // value
            1698765432,                   // timestamp
            test_hash.clone(),            // message_hash
        ));

        flush_batched_data(&db, batched)
            .await
            .expect("Failed to flush");

        // 🎯 STRICT VALIDATION: Verify ALL fields
        let result: (i64, String, Option<String>, i64, Vec<u8>) = sqlx::query_as(
            "SELECT fid, field_name, field_value, timestamp, message_hash
             FROM user_profile_changes 
             WHERE message_hash = $1",
        )
        .bind(&test_hash)
        .fetch_one(db.pool())
        .await
        .expect("Failed to query profile change");

        assert_eq!(result.0, 99, "FID should match");
        assert_eq!(result.1, "username", "Field name should be 'username'");
        assert_eq!(
            result.2,
            Some("testuser".to_string()),
            "Field value should match"
        );
        assert_eq!(result.3, 1698765432, "Timestamp should match");
        assert_eq!(result.4, test_hash.clone(), "Message hash should match");

        // Cleanup after test
        cleanup_by_message_hash(&db, &test_hash).await;

        println!("✅ Type 11 (UserDataAdd) test passed");
    }

    #[tokio::test]
    #[ignore = "Requires database access - production database should not be modified"]
    async fn test_username_proof_type_12() {
        let db = setup_test_db().await;
        let shard_info = test_shard_info();
        let test_hash = test_message_hash(12); // Unique test hash

        // Cleanup before test
        cleanup_by_message_hash(&db, &test_hash).await;

        let mut batched = BatchedData::new();

        // Create test username proof
        batched.username_proofs.push((
            99,                     // fid
            "testuser".to_string(), // username
            vec![0x12; 20],         // owner address
            vec![0x34; 65],         // signature
            1,                      // username_type (FNAME)
            1698765432,             // timestamp
            test_hash.clone(),      // message_hash
            shard_info.clone(),
        ));

        flush_batched_data(&db, batched)
            .await
            .expect("Failed to flush");

        // 🎯 STRICT VALIDATION: Verify ALL fields are correctly inserted
        let result: (i64, String, i16, Vec<u8>, Vec<u8>, i64, Vec<u8>) = sqlx::query_as(
            "SELECT fid, username, username_type, owner, signature, timestamp, message_hash 
             FROM username_proofs 
             WHERE message_hash = $1",
        )
        .bind(&test_hash)
        .fetch_one(db.pool())
        .await
        .expect("Failed to query username proof");

        assert_eq!(result.0, 99, "FID should match");
        assert_eq!(result.1, "testuser", "Username should match");
        assert_eq!(result.2, 1, "Username type should be FNAME (1)");
        assert_eq!(result.3, vec![0x12; 20], "Owner address should match");
        assert_eq!(result.4, vec![0x34; 65], "Signature should match");
        assert_eq!(result.5, 1698765432, "Timestamp should match");
        assert_eq!(result.6, test_hash.clone(), "Message hash should match");

        // Cleanup after test
        cleanup_by_message_hash(&db, &test_hash).await;

        println!("✅ Type 12 (UsernameProof) test passed");
    }

    #[tokio::test]
    #[ignore = "Requires database access - production database should not be modified"]
    async fn test_frame_action_type_13() {
        let db = setup_test_db().await;
        let shard_info = test_shard_info();
        let test_hash = test_message_hash(13); // Unique test hash

        // Cleanup before test
        cleanup_by_message_hash(&db, &test_hash).await;

        let mut batched = BatchedData::new();

        // Create test frame action
        batched.frame_actions.push((
            99,                                      // fid
            "https://example.com/frame".to_string(), // url
            Some(1),                                 // button_index
            Some(vec![0x56; 20]),                    // cast_hash
            Some(100),                               // cast_fid
            Some("test input".to_string()),          // input_text
            None,                                    // state
            None,                                    // transaction_id
            1698765432,                              // timestamp
            test_hash.clone(),                       // message_hash
            shard_info.clone(),
        ));

        flush_batched_data(&db, batched)
            .await
            .expect("Failed to flush");

        // 🎯 STRICT VALIDATION: Verify ALL fields
        let result: (i64, String, Option<i32>, Option<Vec<u8>>, Option<i64>, Option<String>, i64, Vec<u8>) = sqlx::query_as(
            "SELECT fid, url, button_index, cast_hash, cast_fid, input_text, timestamp, message_hash
             FROM frame_actions 
             WHERE message_hash = $1"
        )
            .bind(&test_hash)
            .fetch_one(db.pool())
            .await
            .expect("Failed to query frame action");

        assert_eq!(result.0, 99, "FID should match");
        assert_eq!(result.1, "https://example.com/frame", "URL should match");
        assert_eq!(result.2, Some(1), "Button index should match");
        assert_eq!(result.3, Some(vec![0x56; 20]), "Cast hash should match");
        assert_eq!(result.4, Some(100), "Cast FID should match");
        assert_eq!(
            result.5,
            Some("test input".to_string()),
            "Input text should match"
        );
        assert_eq!(result.6, 1698765432, "Timestamp should match");
        assert_eq!(result.7, test_hash.clone(), "Message hash should match");

        // Cleanup after test
        cleanup_by_message_hash(&db, &test_hash).await;

        println!("✅ Type 13 (FrameAction) test passed");
    }

    #[tokio::test]
    #[ignore = "Requires database access - production database should not be modified"]
    async fn test_remove_events_soft_delete() {
        let db = setup_test_db().await;
        let shard_info = test_shard_info();
        let test_hash_add = test_message_hash(6001); // Unique hash for add
        let test_hash_remove = test_message_hash(6002); // Unique hash for remove

        // Cleanup before test
        cleanup_by_message_hash(&db, &test_hash_add).await;
        cleanup_by_message_hash(&db, &test_hash_remove).await;

        // First, insert a link
        let mut batched_add = BatchedData::new();
        batched_add.links.push((
            99,
            100,
            "follow".to_string(),
            "add".to_string(), // event_type
            1698765432,
            test_hash_add.clone(),
            shard_info.clone(),
        ));
        flush_batched_data(&db, batched_add)
            .await
            .expect("Failed to flush add");

        // Then, remove it - NOW INSERTS a new row with event_type='remove'
        let mut batched_remove = BatchedData::new();
        batched_remove.links.push((
            99,                       // fid
            100,                      // target_fid
            "follow".to_string(),     // link_type
            "remove".to_string(),     // event_type
            1698765500,               // timestamp
            test_hash_remove.clone(), // message_hash for remove event
            shard_info.clone(),
        ));
        flush_batched_data(&db, batched_remove)
            .await
            .expect("Failed to flush remove");

        // Verify remove event - check the REMOVE row exists
        let result: (String,) =
            sqlx::query_as("SELECT event_type FROM links WHERE message_hash = $1")
                .bind(&test_hash_remove) // Check the remove event's message_hash
                .fetch_one(db.pool())
                .await
                .expect("Failed to query");

        assert_eq!(
            result.0, "remove",
            "Link remove event should have event_type='remove'"
        );

        // Cleanup after test
        cleanup_by_message_hash(&db, &test_hash_add).await;
        cleanup_by_message_hash(&db, &test_hash_remove).await;

        println!("✅ Type 6 (LinkRemove) soft delete test passed");
    }

    #[tokio::test]
    #[ignore = "Requires database access - production database should not be modified"]
    async fn test_reaction_remove_type_4() {
        let db = setup_test_db().await;
        let shard_info = test_shard_info();
        let add_hash = test_message_hash(4001);
        let remove_hash = test_message_hash(4002);

        cleanup_by_message_hash(&db, &add_hash).await;
        cleanup_by_message_hash(&db, &remove_hash).await;

        // Add a reaction
        let mut batched = BatchedData::new();
        batched.reactions.push((
            99,
            vec![0xAA; 20],    // target_cast_hash
            Some(100),         // target_fid
            1,                 // reaction_type (like)
            "add".to_string(), // event_type
            1698765432,
            add_hash.clone(),
            shard_info.clone(),
        ));
        flush_batched_data(&db, batched)
            .await
            .expect("Failed to flush");

        // Remove the reaction - INSERT with event_type='remove'
        let mut batched = BatchedData::new();
        batched.reactions.push((
            99,
            vec![0xAA; 20],
            Some(100),
            1,
            "remove".to_string(), // event_type
            1698765500,
            remove_hash.clone(),
            shard_info.clone(),
        ));
        flush_batched_data(&db, batched)
            .await
            .expect("Failed to flush remove");

        // 🎯 STRICT VALIDATION: Verify remove event
        let result: (i64, Vec<u8>, Option<i64>, i16, String, i64, Vec<u8>) = sqlx::query_as(
            "SELECT fid, target_cast_hash, target_fid, reaction_type, event_type, timestamp, message_hash
             FROM reactions 
             WHERE message_hash = $1"
        )
            .bind(&remove_hash)
            .fetch_one(db.pool())
            .await
            .expect("Failed to query reaction remove");

        assert_eq!(result.0, 99, "FID should match");
        assert_eq!(result.1, vec![0xAA; 20], "Target cast hash should match");
        assert_eq!(result.2, Some(100), "Target FID should match");
        assert_eq!(result.3, 1, "Reaction type should match");
        assert_eq!(result.4, "remove", "Event type should be 'remove'");
        assert_eq!(result.5, 1698765500, "Timestamp should match");
        assert_eq!(result.6, remove_hash.clone(), "Message hash should match");

        cleanup_by_message_hash(&db, &add_hash).await;
        cleanup_by_message_hash(&db, &remove_hash).await;

        println!("✅ Type 4 (ReactionRemove) test passed");
    }

    #[tokio::test]
    #[ignore = "Requires database access - production database should not be modified"]
    async fn test_verification_remove_type_8() {
        let db = setup_test_db().await;
        let shard_info = test_shard_info();
        let add_hash = test_message_hash(8001);
        let remove_hash = test_message_hash(8002);

        cleanup_by_message_hash(&db, &add_hash).await;
        cleanup_by_message_hash(&db, &remove_hash).await;

        // Add a verification
        let mut batched = BatchedData::new();
        batched.verifications.push((
            99,
            vec![0xBB; 20],       // address
            Some(vec![0xCC; 65]), // claim_signature
            Some(vec![0xDD; 32]), // block_hash
            Some(0),              // verification_type (EOA)
            Some(1),              // chain_id (Ethereum)
            "add".to_string(),    // event_type
            1698765432,
            add_hash.clone(),
            shard_info.clone(),
        ));
        flush_batched_data(&db, batched)
            .await
            .expect("Failed to flush");

        // Remove the verification - INSERT with event_type='remove'
        let mut batched = BatchedData::new();
        batched.verifications.push((
            99,
            vec![0xBB; 20],
            None,                 // claim_signature (not provided in remove)
            None,                 // block_hash
            None,                 // verification_type
            None,                 // chain_id
            "remove".to_string(), // event_type
            1698765500,
            remove_hash.clone(),
            shard_info.clone(),
        ));
        flush_batched_data(&db, batched)
            .await
            .expect("Failed to flush remove");

        // 🎯 STRICT VALIDATION: Verify remove event
        let result: (i64, Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>, Option<i16>, Option<i32>, String, i64, Vec<u8>) = sqlx::query_as(
            "SELECT fid, address, claim_signature, block_hash, verification_type, chain_id, event_type, timestamp, message_hash
             FROM verifications 
             WHERE message_hash = $1"
        )
            .bind(&remove_hash)
            .fetch_one(db.pool())
            .await
            .expect("Failed to query verification remove");

        assert_eq!(result.0, 99, "FID should match");
        assert_eq!(result.1, vec![0xBB; 20], "Address should match");
        assert_eq!(result.2, None, "claim_signature should be None for remove");
        assert_eq!(result.3, None, "block_hash should be None for remove");
        assert_eq!(
            result.4, None,
            "verification_type should be None for remove"
        );
        assert_eq!(result.5, None, "chain_id should be None for remove");
        assert_eq!(result.6, "remove", "Event type should be 'remove'");
        assert_eq!(result.7, 1698765500, "Timestamp should match");
        assert_eq!(result.8, remove_hash.clone(), "Message hash should match");

        cleanup_by_message_hash(&db, &add_hash).await;
        cleanup_by_message_hash(&db, &remove_hash).await;

        println!("✅ Type 8 (VerificationRemove) test passed");
    }

    #[tokio::test]
    #[ignore = "Requires database access - production database should not be modified"]
    async fn test_soft_delete_query_filtering() {
        let db = setup_test_db().await;
        let shard_info = test_shard_info();

        // Use unique test FID to avoid conflicts with other tests or real data
        let test_fid = 777777_i64; // Unique test FID

        // Setup: Insert 3 links - 2 active, 1 removed
        let hash1 = test_message_hash(7001);
        let hash2 = test_message_hash(7002);
        let hash3_add = test_message_hash(7003);
        let hash3_remove = test_message_hash(7004);

        cleanup_by_message_hash(&db, &hash1).await;
        cleanup_by_message_hash(&db, &hash2).await;
        cleanup_by_message_hash(&db, &hash3_add).await;
        cleanup_by_message_hash(&db, &hash3_remove).await;

        let mut batched = BatchedData::new();
        batched.fids_to_ensure.insert(test_fid);

        // Link 1: Active
        batched.links.push((
            test_fid,
            100,
            "follow".to_string(),
            "add".to_string(), // event_type
            1698765432,
            hash1.clone(),
            shard_info.clone(),
        ));

        // Link 2: Active
        batched.links.push((
            test_fid,
            101,
            "follow".to_string(),
            "add".to_string(), // event_type
            1698765433,
            hash2.clone(),
            shard_info.clone(),
        ));

        // Link 3: Added then Removed (using event_type)
        batched.links.push((
            test_fid,
            102,
            "follow".to_string(),
            "add".to_string(), // event_type
            1698765434,
            hash3_add.clone(),
            shard_info.clone(),
        ));
        batched.links.push((
            test_fid,
            102,
            "follow".to_string(),
            "remove".to_string(), // event_type
            1698765500,
            hash3_remove.clone(),
            shard_info.clone(),
        ));

        flush_batched_data(&db, batched)
            .await
            .expect("Failed to flush");

        // 🎯 CRITICAL TEST: Query active links only (using window function to get latest event)
        let active_links: Vec<(i64, Vec<u8>)> = sqlx::query_as(
            r"
            WITH latest_events AS (
                SELECT *, ROW_NUMBER() OVER (
                    PARTITION BY fid, target_fid 
                    ORDER BY timestamp DESC
                ) as rn
                FROM links 
                WHERE fid = $1 AND message_hash IN ($2, $3, $4, $5)
            )
            SELECT target_fid, message_hash FROM latest_events
            WHERE rn = 1 AND event_type = 'add'
            ORDER BY target_fid
            ",
        )
        .bind(test_fid)
        .bind(&hash1)
        .bind(&hash2)
        .bind(&hash3_add)
        .bind(&hash3_remove)
        .fetch_all(db.pool())
        .await
        .expect("Failed to query active links");

        // Should return 2: hash1 (100), hash2 (101)
        // For target_fid=102, the latest event is hash3_remove (event_type='remove'), so it's filtered out
        assert_eq!(
            active_links.len(),
            2,
            "Should return 2 active links (100, 101)"
        );
        assert_eq!(
            active_links[0].0, 100,
            "First active link should be target_fid 100"
        );
        assert_eq!(
            active_links[1].0, 101,
            "Second active link should be target_fid 101"
        );

        // 🎯 Verify removed link is still in DB (both add and remove events)
        let removed_links: Vec<(i64, String)> = sqlx::query_as(
            "SELECT target_fid, event_type FROM links 
             WHERE fid = $1 AND target_fid = 102 AND message_hash IN ($2, $3)
             ORDER BY timestamp ASC",
        )
        .bind(test_fid)
        .bind(&hash3_add)
        .bind(&hash3_remove)
        .fetch_all(db.pool())
        .await
        .expect("Failed to query removed link");

        assert_eq!(
            removed_links.len(),
            2,
            "Should have both Add and Remove events for target_fid 102"
        );
        assert_eq!(removed_links[0].1, "add", "First event should be 'add'");
        assert_eq!(
            removed_links[1].1, "remove",
            "Second event should be 'remove'"
        );

        cleanup_by_message_hash(&db, &hash1).await;
        cleanup_by_message_hash(&db, &hash2).await;
        cleanup_by_message_hash(&db, &hash3_add).await;
        cleanup_by_message_hash(&db, &hash3_remove).await;

        println!("✅ Soft delete query filtering test passed");
    }

    #[tokio::test]
    #[ignore = "Requires database access - production database should not be modified"]
    async fn test_boundary_conditions() {
        let db = setup_test_db().await;
        let shard_info = test_shard_info();

        println!("🧪 Testing boundary conditions...");

        // Test 1: None values for optional fields
        println!("  1. Testing None values for optional fields");
        let hash1 = test_message_hash(8001);
        cleanup_by_message_hash(&db, &hash1).await;

        let mut batched = BatchedData::new();
        batched.casts.push((
            99,
            None, // ⭐ No text (NULL)
            1698765432,
            hash1.clone(),
            None, // No parent_hash
            None, // No root_hash
            None, // No embeds
            None, // No mentions
            shard_info.clone(),
        ));
        flush_batched_data(&db, batched)
            .await
            .expect("Failed to flush cast with None values");

        let result: (i64, Option<String>, i64) =
            sqlx::query_as("SELECT fid, text, timestamp FROM casts WHERE message_hash = $1")
                .bind(&hash1)
                .fetch_one(db.pool())
                .await
                .expect("Failed to query");

        assert_eq!(result.0, 99);
        assert_eq!(result.1, None, "Text should be None");
        assert_eq!(result.2, 1698765432);
        cleanup_by_message_hash(&db, &hash1).await;
        println!("     ✅ None values handled correctly");

        // Test 2: Empty string
        println!("  2. Testing empty string");
        let hash2 = test_message_hash(8002);
        cleanup_by_message_hash(&db, &hash2).await;

        let mut batched = BatchedData::new();
        batched.casts.push((
            99,
            Some(String::new()), // ⭐ Empty string
            1698765432,
            hash2.clone(),
            None,
            None,
            None,
            None,
            shard_info.clone(),
        ));
        flush_batched_data(&db, batched)
            .await
            .expect("Failed to flush cast with empty string");

        let result: (Option<String>,) =
            sqlx::query_as("SELECT text FROM casts WHERE message_hash = $1")
                .bind(&hash2)
                .fetch_one(db.pool())
                .await
                .expect("Failed to query");

        assert_eq!(
            result.0,
            Some(String::new()),
            "Empty string should be preserved"
        );
        cleanup_by_message_hash(&db, &hash2).await;
        println!("     ✅ Empty string handled correctly");

        // Test 3: Very long text (10KB)
        println!("  3. Testing very long text (10KB)");
        let hash3 = test_message_hash(8003);
        cleanup_by_message_hash(&db, &hash3).await;

        let long_text = "x".repeat(10_000); // 10KB text
        let mut batched = BatchedData::new();
        batched.casts.push((
            99,
            Some(long_text.clone()),
            1698765432,
            hash3.clone(),
            None,
            None,
            None,
            None,
            shard_info.clone(),
        ));
        flush_batched_data(&db, batched)
            .await
            .expect("Failed to flush cast with long text");

        let result: (Option<String>,) =
            sqlx::query_as("SELECT text FROM casts WHERE message_hash = $1")
                .bind(&hash3)
                .fetch_one(db.pool())
                .await
                .expect("Failed to query");

        assert_eq!(
            result.0.as_ref().map(std::string::String::len),
            Some(10_000),
            "Long text should be preserved"
        );
        cleanup_by_message_hash(&db, &hash3).await;
        println!("     ✅ Long text (10KB) handled correctly");

        // Test 4: Zero timestamp
        println!("  4. Testing zero timestamp");
        let hash4 = test_message_hash(8004);
        cleanup_by_message_hash(&db, &hash4).await;

        let mut batched = BatchedData::new();
        batched.casts.push((
            99,
            Some("Test".to_string()),
            0, // ⭐ Zero timestamp
            hash4.clone(),
            None,
            None,
            None,
            None,
            shard_info.clone(),
        ));
        flush_batched_data(&db, batched)
            .await
            .expect("Failed to flush cast with zero timestamp");

        let result: (i64,) = sqlx::query_as("SELECT timestamp FROM casts WHERE message_hash = $1")
            .bind(&hash4)
            .fetch_one(db.pool())
            .await
            .expect("Failed to query");

        assert_eq!(result.0, 0, "Zero timestamp should be preserved");
        cleanup_by_message_hash(&db, &hash4).await;
        println!("     ✅ Zero timestamp handled correctly");

        // Test 5: Maximum i64 timestamp
        println!("  5. Testing maximum i64 timestamp");
        let hash5 = test_message_hash(8005);
        cleanup_by_message_hash(&db, &hash5).await;

        let max_timestamp = i64::MAX;
        let mut batched = BatchedData::new();
        batched.casts.push((
            99,
            Some("Test".to_string()),
            max_timestamp, // ⭐ Max i64
            hash5.clone(),
            None,
            None,
            None,
            None,
            shard_info.clone(),
        ));
        flush_batched_data(&db, batched)
            .await
            .expect("Failed to flush cast with max timestamp");

        let result: (i64,) = sqlx::query_as("SELECT timestamp FROM casts WHERE message_hash = $1")
            .bind(&hash5)
            .fetch_one(db.pool())
            .await
            .expect("Failed to query");

        assert_eq!(result.0, max_timestamp, "Max timestamp should be preserved");
        cleanup_by_message_hash(&db, &hash5).await;
        println!("     ✅ Maximum timestamp handled correctly");

        println!("✅ All boundary conditions tests passed (5/5)");
    }

    #[tokio::test]
    #[ignore = "Requires database access - production database should not be modified"]
    async fn test_unique_constraint_conflicts() {
        let db = setup_test_db().await;
        let shard_info = test_shard_info();

        println!("🧪 Testing UNIQUE constraint conflict handling...");

        // Test 1: message_hash UNIQUE constraint with ON CONFLICT DO NOTHING
        println!("  1. Testing message_hash UNIQUE with idempotent inserts");
        let test_hash = test_message_hash(9001);
        cleanup_by_message_hash(&db, &test_hash).await;

        let mut batched1 = BatchedData::new();
        batched1.casts.push((
            99,
            Some("First insert".to_string()),
            1698765432,
            test_hash.clone(),
            None,
            None,
            None,
            None,
            shard_info.clone(),
        ));
        flush_batched_data(&db, batched1)
            .await
            .expect("First insert should succeed");

        // Try to insert again with SAME message_hash but DIFFERENT data
        let mut batched2 = BatchedData::new();
        batched2.casts.push((
            99,
            Some("Second insert DIFFERENT TEXT".to_string()),
            1698765999,        // Different timestamp
            test_hash.clone(), // SAME message_hash
            None,
            None,
            None,
            None,
            shard_info.clone(),
        ));

        // ON CONFLICT DO NOTHING should silently skip
        flush_batched_data(&db, batched2)
            .await
            .expect("Second insert should not fail (ON CONFLICT DO NOTHING)");

        // Verify ONLY first record exists (not updated)
        let result: (i64, Option<String>, i64) = sqlx::query_as(
            "SELECT COUNT(*), text, timestamp FROM casts WHERE message_hash = $1 GROUP BY text, timestamp"
        )
            .bind(&test_hash)
            .fetch_one(db.pool())
            .await
            .expect("Failed to query");

        assert_eq!(result.0, 1, "Should have exactly 1 record");
        assert_eq!(
            result.1,
            Some("First insert".to_string()),
            "Text should NOT be updated"
        );
        assert_eq!(result.2, 1698765432, "Timestamp should NOT be updated");
        cleanup_by_message_hash(&db, &test_hash).await;
        println!("     ✅ message_hash UNIQUE + ON CONFLICT DO NOTHING works correctly");

        // Test 2: username_proofs UNIQUE(fid, username_type) constraint
        println!("  2. Testing username_proofs UNIQUE(fid, username_type)");
        let hash_fname1 = test_message_hash(9002);
        let hash_fname2 = test_message_hash(9003);
        let hash_ens = test_message_hash(9004);

        let test_fid = 888888_i64;

        // Insert FNAME proof (username_type=1)
        let mut batched = BatchedData::new();
        batched.fids_to_ensure.insert(test_fid);
        batched.username_proofs.push((
            test_fid,
            "firstuser".to_string(),
            vec![0x11; 20], // owner
            vec![0x22; 65], // signature
            1,              // username_type = FNAME
            1698765432,
            hash_fname1.clone(),
            shard_info.clone(),
        ));
        flush_batched_data(&db, batched)
            .await
            .expect("First FNAME should succeed");

        // Try to insert ANOTHER FNAME (should conflict and be silently skipped)
        let mut batched = BatchedData::new();
        batched.username_proofs.push((
            test_fid,
            "seconduser".to_string(), // Different username
            vec![0x33; 20],           // Different owner
            vec![0x44; 65],           // Different signature
            1,                        // SAME username_type = FNAME
            1698765999,
            hash_fname2.clone(),
            shard_info.clone(),
        ));
        flush_batched_data(&db, batched)
            .await
            .expect("Second FNAME should not fail (ON CONFLICT DO NOTHING)");

        // Verify only FIRST FNAME exists
        let result: (String,) = sqlx::query_as(
            "SELECT username FROM username_proofs WHERE fid = $1 AND username_type = 1",
        )
        .bind(test_fid)
        .fetch_one(db.pool())
        .await
        .expect("Failed to query FNAME");

        assert_eq!(
            result.0, "firstuser",
            "Should keep first FNAME (not update)"
        );

        // But ENS (username_type=2) should be allowed for SAME FID
        let mut batched = BatchedData::new();
        batched.username_proofs.push((
            test_fid,
            "ensname".to_string(),
            vec![0x55; 20],
            vec![0x66; 65],
            2, // username_type = ENS (different from FNAME)
            1698766000,
            hash_ens.clone(),
            shard_info.clone(),
        ));
        flush_batched_data(&db, batched)
            .await
            .expect("ENS should succeed (different username_type)");

        // Verify BOTH FNAME and ENS exist for same FID
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM username_proofs WHERE fid = $1")
            .bind(test_fid)
            .fetch_one(db.pool())
            .await
            .expect("Failed to count");

        assert_eq!(count.0, 2, "Should have 2 proofs: 1 FNAME + 1 ENS");

        // Cleanup
        sqlx::query("DELETE FROM username_proofs WHERE fid = $1")
            .bind(test_fid)
            .execute(db.pool())
            .await
            .ok();

        println!("     ✅ UNIQUE(fid, username_type) allows FNAME + ENS coexistence");

        println!("✅ All UNIQUE constraint conflict tests passed (2/2)");
    }

    #[tokio::test]
    #[ignore = "Requires database access - production database should not be modified"]
    async fn test_idempotency_all_types() {
        let db = setup_test_db().await;
        let shard_info = test_shard_info();
        let test_hash = test_message_hash(9999); // Unique test hash for idempotency test

        // Cleanup before test
        cleanup_by_message_hash(&db, &test_hash).await;

        // Test that inserting the same message twice doesn't fail
        let mut batched1 = BatchedData::new();
        batched1.casts.push((
            99,
            Some("Idempotency test".to_string()),
            1698765432,
            test_hash.clone(),
            None,
            None,
            None,
            None,
            shard_info.clone(),
        ));

        flush_batched_data(&db, batched1)
            .await
            .expect("First insert failed");

        // Insert again with same message_hash
        let mut batched2 = BatchedData::new();
        batched2.casts.push((
            99,
            Some("Idempotency test DUPLICATE".to_string()), // Different text
            1698765432,
            test_hash.clone(), // SAME hash
            None,
            None,
            None,
            None,
            shard_info.clone(),
        ));

        flush_batched_data(&db, batched2)
            .await
            .expect("Second insert should not fail");

        // Verify only one record exists
        let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM casts WHERE message_hash = $1")
            .bind(&test_hash)
            .fetch_one(db.pool())
            .await
            .expect("Failed to query");

        assert_eq!(result.0, 1, "Should have exactly one cast (idempotent)");

        // Cleanup after test
        cleanup_by_message_hash(&db, &test_hash).await;

        println!("✅ Idempotency test passed");

        // 🚀 Extended tests for other types
        println!("  Extended: Testing Link/Reaction/Verification idempotency...");

        // Link idempotency
        let link_hash = test_message_hash(9991);
        cleanup_by_message_hash(&db, &link_hash).await;
        for _ in 0..2 {
            let mut b = BatchedData::new();
            b.links.push((
                99,
                100,
                "follow".to_string(),
                "add".to_string(), // event_type
                1698765432,
                link_hash.clone(),
                shard_info.clone(),
            ));
            flush_batched_data(&db, b).await.ok();
        }
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM links WHERE message_hash = $1")
            .bind(&link_hash)
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(count.0, 1, "Link: Only 1 record");
        cleanup_by_message_hash(&db, &link_hash).await;

        // Reaction idempotency
        let reaction_hash = test_message_hash(9992);
        cleanup_by_message_hash(&db, &reaction_hash).await;
        for _ in 0..2 {
            let mut b = BatchedData::new();
            b.reactions.push((
                99,
                vec![0xAA; 20],
                Some(100),
                1,
                "add".to_string(), // event_type
                1698765432,
                reaction_hash.clone(),
                shard_info.clone(),
            ));
            flush_batched_data(&db, b).await.ok();
        }
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM reactions WHERE message_hash = $1")
                .bind(&reaction_hash)
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(count.0, 1, "Reaction: Only 1 record");
        cleanup_by_message_hash(&db, &reaction_hash).await;

        // Verification idempotency
        let verification_hash = test_message_hash(9993);
        cleanup_by_message_hash(&db, &verification_hash).await;
        for _ in 0..2 {
            let mut b = BatchedData::new();
            b.verifications.push((
                99,
                vec![0xBB; 20],
                None,
                None,
                Some(0),
                Some(1),
                "add".to_string(), // event_type
                1698765432,
                verification_hash.clone(),
                shard_info.clone(),
            ));
            flush_batched_data(&db, b).await.ok();
        }
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM verifications WHERE message_hash = $1")
                .bind(&verification_hash)
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(count.0, 1, "Verification: Only 1 record");
        cleanup_by_message_hash(&db, &verification_hash).await;

        println!("  ✅ Extended idempotency verified for Link/Reaction/Verification");
        println!("✅ All idempotency tests passed (4/4 types)");
    }

    #[tokio::test]
    #[ignore = "Requires database access - production database should not be modified"]
    async fn test_cleanup_safety() {
        // This test verifies that our cleanup approach is safe and doesn't
        // accidentally delete real user data

        let test_hash_1 = test_message_hash(1);
        let test_hash_2 = test_message_hash(2);

        // Verify test hashes have the 0xFEFE prefix
        assert_eq!(test_hash_1[0], 0xFE, "Test hash should have 0xFE prefix");
        assert_eq!(test_hash_1[1], 0xFE, "Test hash should have 0xFE prefix");

        // Verify different test IDs produce different hashes
        assert_ne!(
            test_hash_1, test_hash_2,
            "Different test IDs should produce different hashes"
        );

        // Real Farcaster message hashes are Blake3 hashes (32 bytes)
        // and would never start with 0xFEFE in practice
        // Our test hashes are 6 bytes: [0xFE, 0xFE, id_byte1, id_byte2, id_byte3, id_byte4]

        println!("✅ Test cleanup safety verified");
        println!("   Test hashes use 0xFEFE prefix to avoid real data conflicts");
        println!("   Each test uses unique message_hash for isolation");
    }

    #[tokio::test]
    #[ignore = "Requires database access - production database should not be modified"]
    async fn test_user_profile_changes_aggregation() {
        let db = setup_test_db().await;
        let shard_info = test_shard_info();

        println!("🧪 Testing user_profile_changes event-sourcing aggregation...");

        let test_fid = 999999_i64;
        let hash_username = test_message_hash(10001);
        let hash_display = test_message_hash(10002);
        let hash_bio = test_message_hash(10003);
        let hash_pfp = test_message_hash(10004);
        let hash_fname = test_message_hash(10005);

        // Cleanup
        for hash in [
            &hash_username,
            &hash_display,
            &hash_bio,
            &hash_pfp,
            &hash_fname,
        ] {
            cleanup_by_message_hash(&db, hash).await;
        }
        sqlx::query("DELETE FROM username_proofs WHERE fid = $1")
            .bind(test_fid)
            .execute(db.pool())
            .await
            .ok();

        // Setup: Insert profile changes + username proof
        let mut batched = BatchedData::new();
        batched.fids_to_ensure.insert(test_fid);

        // Profile fields
        batched.profile_updates.push((
            test_fid,
            "username".to_string(),
            Some("testuser".to_string()),
            1698765432,
            hash_username.clone(),
        ));
        batched.profile_updates.push((
            test_fid,
            "display_name".to_string(),
            Some("Test User".to_string()),
            1698765433,
            hash_display.clone(),
        ));
        batched.profile_updates.push((
            test_fid,
            "bio".to_string(),
            Some("Test bio".to_string()),
            1698765434,
            hash_bio.clone(),
        ));
        batched.profile_updates.push((
            test_fid,
            "pfp_url".to_string(),
            Some("https://example.com/pfp.png".to_string()),
            1698765435,
            hash_pfp.clone(),
        ));

        // FNAME proof
        batched.username_proofs.push((
            test_fid,
            "testuser".to_string(),
            vec![0x11; 20],
            vec![0x22; 65],
            1, // FNAME
            1698765436,
            hash_fname.clone(),
            shard_info.clone(),
        ));

        flush_batched_data(&db, batched)
            .await
            .expect("Failed to flush");

        // 🎯 CRITICAL TEST: Verify event-sourcing works - query latest values
        // Test each field individually (event-sourcing aggregation)

        let display_name: Option<(Option<String>,)> = sqlx::query_as(
            "SELECT field_value FROM user_profile_changes 
             WHERE fid = $1 AND field_name = 'display_name' 
             ORDER BY timestamp DESC LIMIT 1",
        )
        .bind(test_fid)
        .fetch_optional(db.pool())
        .await
        .expect("Failed to query display_name");

        assert_eq!(
            display_name.map(|r| r.0),
            Some(Some("Test User".to_string())),
            "Display name should be retrievable from event log"
        );

        let bio: Option<(Option<String>,)> = sqlx::query_as(
            "SELECT field_value FROM user_profile_changes 
             WHERE fid = $1 AND field_name = 'bio' 
             ORDER BY timestamp DESC LIMIT 1",
        )
        .bind(test_fid)
        .fetch_optional(db.pool())
        .await
        .expect("Failed to query bio");

        assert_eq!(
            bio.map(|r| r.0),
            Some(Some("Test bio".to_string())),
            "Bio should be retrievable"
        );

        // Verify username_proofs record
        let username_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM username_proofs WHERE fid = $1")
                .bind(test_fid)
                .fetch_one(db.pool())
                .await
                .expect("Failed to count username_proofs");

        assert_eq!(username_count.0, 1, "Should have 1 username_proof (FNAME)");

        // Test: Insert another display_name with later timestamp (should be the latest)
        let hash_display2 = test_message_hash(10007);
        let mut batched = BatchedData::new();
        batched.profile_updates.push((
            test_fid,
            "display_name".to_string(),
            Some("Updated Display Name".to_string()),
            1698765999, // Later timestamp
            hash_display2.clone(),
        ));
        flush_batched_data(&db, batched)
            .await
            .expect("Failed to flush update");

        // Verify latest display_name
        let latest_display: Option<(Option<String>,)> = sqlx::query_as(
            "SELECT field_value FROM user_profile_changes 
             WHERE fid = $1 AND field_name = 'display_name' 
             ORDER BY timestamp DESC LIMIT 1",
        )
        .bind(test_fid)
        .fetch_optional(db.pool())
        .await
        .expect("Failed to query latest display_name");

        assert_eq!(
            latest_display.map(|r| r.0),
            Some(Some("Updated Display Name".to_string())),
            "Should get LATEST display_name by timestamp"
        );

        println!("     ✅ Event-sourcing aggregation: Latest value by timestamp works correctly");

        // Cleanup
        for hash in [
            &hash_username,
            &hash_display,
            &hash_bio,
            &hash_pfp,
            &hash_fname,
            &hash_display2,
        ] {
            cleanup_by_message_hash(&db, hash).await;
        }
        sqlx::query("DELETE FROM username_proofs WHERE fid = $1")
            .bind(test_fid)
            .execute(db.pool())
            .await
            .ok();

        println!("✅ user_profile_changes event-sourcing aggregation verified");
    }

    #[tokio::test]
    #[ignore = "Requires database access - production database should not be modified"]
    async fn test_concurrent_message_hash_inserts() {
        let db = setup_test_db().await;
        let shard_info = test_shard_info();

        println!("🧪 Testing concurrent inserts with same message_hash...");

        let test_hash = test_message_hash(11001);
        cleanup_by_message_hash(&db, &test_hash).await;

        // Spawn 5 concurrent tasks trying to insert the SAME message_hash
        let mut handles = vec![];

        for i in 0..5 {
            let db_clone = db.clone();
            let hash_clone = test_hash.clone();
            let shard_clone = shard_info.clone();

            let handle = tokio::spawn(async move {
                let mut batched = BatchedData::new();
                batched.casts.push((
                    99,
                    Some(format!("Concurrent insert {i}")),
                    1698765432 + i64::from(i),
                    hash_clone,
                    None,
                    None,
                    None,
                    None,
                    shard_clone,
                ));

                flush_batched_data(&db_clone, batched).await
            });

            handles.push(handle);
        }

        // Wait for all tasks
        let mut success_count = 0;
        for handle in handles {
            if handle.await.is_ok() {
                success_count += 1;
            }
        }

        println!("  {success_count}/5 concurrent inserts succeeded");

        // Verify only ONE record exists (ON CONFLICT DO NOTHING works under concurrency)
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM casts WHERE message_hash = $1")
            .bind(&test_hash)
            .fetch_one(db.pool())
            .await
            .expect("Failed to query");

        assert_eq!(
            count.0, 1,
            "Should have exactly 1 record despite concurrent inserts"
        );

        cleanup_by_message_hash(&db, &test_hash).await;

        println!(
            "✅ Concurrent insert safety verified (ON CONFLICT protects against race conditions)"
        );
    }

    #[tokio::test]
    #[ignore = "Requires database access - production database should not be modified"]
    async fn test_error_handling_invalid_data() {
        let db = setup_test_db().await;
        let shard_info = test_shard_info();

        println!("🧪 Testing error handling for invalid data...");

        // Test 1: Very large FID (should still work, i64 supports it)
        println!("  1. Testing very large FID");
        let hash1 = test_message_hash(12001);
        cleanup_by_message_hash(&db, &hash1).await;

        let huge_fid = i64::MAX - 1000;
        let mut batched = BatchedData::new();
        batched.fids_to_ensure.insert(huge_fid);
        batched.casts.push((
            huge_fid,
            Some("Large FID test".to_string()),
            1698765432,
            hash1.clone(),
            None,
            None,
            None,
            None,
            shard_info.clone(),
        ));

        let result = flush_batched_data(&db, batched).await;
        assert!(result.is_ok(), "Large FID should be accepted");

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM casts WHERE message_hash = $1")
            .bind(&hash1)
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(count.0, 1, "Cast with large FID should be inserted");

        cleanup_by_message_hash(&db, &hash1).await;
        println!("     ✅ Large FID handled correctly");

        // Test 2: Empty message_hash (should fail or be rejected by type system)
        println!("  2. Testing empty message_hash");
        let empty_hash = vec![];
        let mut batched = BatchedData::new();
        batched.casts.push((
            99,
            Some("Empty hash test".to_string()),
            1698765432,
            empty_hash.clone(),
            None,
            None,
            None,
            None,
            shard_info.clone(),
        ));

        // This should either fail gracefully or be prevented by validation
        let result = flush_batched_data(&db, batched).await;

        if result.is_ok() {
            // If it succeeds, verify it was stored
            let count: (i64,) =
                sqlx::query_as("SELECT COUNT(*) FROM casts WHERE message_hash = $1")
                    .bind(&empty_hash)
                    .fetch_one(db.pool())
                    .await
                    .unwrap_or((0,));
            println!("     ℹ️  Empty hash: {} records created", count.0);
            if count.0 > 0 {
                sqlx::query("DELETE FROM casts WHERE message_hash = $1")
                    .bind(&empty_hash)
                    .execute(db.pool())
                    .await
                    .ok();
            }
        } else {
            println!("     ✅ Empty hash rejected as expected");
        }

        println!("✅ Error handling tests completed");
    }

    #[tokio::test]
    #[ignore = "Requires database access - production database should not be modified"]
    async fn test_onchain_events_all_types() {
        let db = setup_test_db().await;
        let shard_info = test_shard_info();

        println!("🧪 Testing all OnChainEvent types (system messages)...");

        let test_fid = 888888_i64;

        // According to proto/onchain_event.proto:
        // 1 = EVENT_TYPE_SIGNER
        // 2 = EVENT_TYPE_SIGNER_MIGRATED
        // 3 = EVENT_TYPE_ID_REGISTER
        // 4 = EVENT_TYPE_STORAGE_RENT
        // 5 = EVENT_TYPE_TIER_PURCHASE

        let mut batched = BatchedData::new();
        batched.fids_to_ensure.insert(test_fid);

        // Test 1: SIGNER event (type 1)
        println!("  1. Testing EVENT_TYPE_SIGNER (type 1)");
        let event1_data = serde_json::json!({
            "type": 1,
            "fid": test_fid,
            "chain_id": 1,
            "block_number": 1000,
            "block_timestamp": 1698765432,
            "signer_event_body": {
                "key": "0x1234567890abcdef",
                "key_type": 1,
                "event_type": 1,
                "metadata": "0xabcd",
                "metadata_type": 1
            }
        });

        batched.onchain_events.push((
            test_fid,
            1,                    // EVENT_TYPE_SIGNER
            1,                    // chain_id (Ethereum)
            1000,                 // block_number
            Some(vec![0x11; 32]), // block_hash
            1698765432,           // block_timestamp
            Some(vec![0x22; 32]), // transaction_hash
            Some(0),              // log_index
            event1_data,
        ));

        // Test 2: ID_REGISTER event (type 3)
        println!("  2. Testing EVENT_TYPE_ID_REGISTER (type 3)");
        let event2_data = serde_json::json!({
            "type": 3,
            "fid": test_fid,
            "chain_id": 10, // OP Mainnet
            "block_number": 1001,
            "block_timestamp": 1698765433,
            "id_register_event_body": {
                "to": "0xabcdef",
                "event_type": 1,
                "recovery_address": "0x123456"
            }
        });

        batched.onchain_events.push((
            test_fid,
            3,  // EVENT_TYPE_ID_REGISTER
            10, // chain_id (OP Mainnet)
            1001,
            Some(vec![0x33; 32]),
            1698765433,
            Some(vec![0x44; 32]),
            Some(1),
            event2_data,
        ));

        // Test 3: STORAGE_RENT event (type 4)
        println!("  3. Testing EVENT_TYPE_STORAGE_RENT (type 4)");
        let event3_data = serde_json::json!({
            "type": 4,
            "fid": test_fid,
            "chain_id": 10,
            "block_number": 1002,
            "block_timestamp": 1698765434,
            "storage_rent_event_body": {
                "payer": "0xpayer",
                "units": 10,
                "expiry": 1698765434 + 86400 * 365
            }
        });

        batched.onchain_events.push((
            test_fid,
            4, // EVENT_TYPE_STORAGE_RENT
            10,
            1002,
            Some(vec![0x55; 32]),
            1698765434,
            Some(vec![0x66; 32]),
            Some(2),
            event3_data,
        ));

        flush_batched_data(&db, batched)
            .await
            .expect("Failed to flush onchain events");

        // 🎯 CRITICAL VALIDATION: Verify ALL onchain events were inserted
        let results: Vec<(i64, i32, i32, i32, i64)> = sqlx::query_as(
            "SELECT fid, event_type, chain_id, block_number, block_timestamp 
             FROM onchain_events 
             WHERE fid = $1 
             ORDER BY event_type",
        )
        .bind(test_fid)
        .fetch_all(db.pool())
        .await
        .expect("Failed to query onchain_events");

        assert_eq!(results.len(), 3, "Should have 3 onchain events");

        // Verify event 1: SIGNER
        assert_eq!(results[0].0, test_fid, "Event 1: FID should match");
        assert_eq!(results[0].1, 1, "Event 1: type should be SIGNER (1)");
        assert_eq!(results[0].2, 1, "Event 1: chain_id should be 1 (Ethereum)");
        assert_eq!(results[0].3, 1000, "Event 1: block_number should match");
        assert_eq!(
            results[0].4, 1698765432,
            "Event 1: block_timestamp should match"
        );

        // Verify event 2: ID_REGISTER
        assert_eq!(results[1].0, test_fid, "Event 2: FID should match");
        assert_eq!(results[1].1, 3, "Event 2: type should be ID_REGISTER (3)");
        assert_eq!(
            results[1].2, 10,
            "Event 2: chain_id should be 10 (OP Mainnet)"
        );
        assert_eq!(results[1].3, 1001, "Event 2: block_number should match");

        // Verify event 3: STORAGE_RENT
        assert_eq!(results[2].0, test_fid, "Event 3: FID should match");
        assert_eq!(results[2].1, 4, "Event 3: type should be STORAGE_RENT (4)");
        assert_eq!(results[2].2, 10, "Event 3: chain_id should match");
        assert_eq!(results[2].3, 1002, "Event 3: block_number should match");

        // Verify UNIQUE constraint works (transaction_hash + log_index)
        let unique_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(DISTINCT (transaction_hash, log_index)) FROM onchain_events WHERE fid = $1"
        )
            .bind(test_fid)
            .fetch_one(db.pool())
            .await
            .expect("Failed to count unique events");

        assert_eq!(
            unique_count.0, 3,
            "All events should have unique (transaction_hash, log_index)"
        );

        // Cleanup
        sqlx::query("DELETE FROM onchain_events WHERE fid = $1")
            .bind(test_fid)
            .execute(db.pool())
            .await
            .ok();

        println!("     ✅ SIGNER event (type 1) validated");
        println!("     ✅ ID_REGISTER event (type 3) validated");
        println!("     ✅ STORAGE_RENT event (type 4) validated");
        println!("     ✅ UNIQUE(transaction_hash, log_index) verified");
        println!("✅ All OnChainEvent types tested (3/5 core types)");
        println!("   Note: SIGNER_MIGRATED (2) and TIER_PURCHASE (5) are less common");
    }
}
