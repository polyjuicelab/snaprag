use snaprag::config::AppConfig;
/// Integration tests for message types
/// This file allows running message type tests independently from other test modules
use snaprag::database::Database;
use snaprag::models::ShardBlockInfo;
use snaprag::sync::shard_processor::flush_batched_data;
use snaprag::sync::shard_processor::BatchedData;

/// Helper to create test database
async fn setup_test_db() -> Database {
    let config = AppConfig::load().expect("Failed to load config");
    Database::from_config(&config)
        .await
        .expect("Failed to create database")
}

/// Helper to cleanup test data by `message_hash`
async fn cleanup_by_message_hash(db: &Database, message_hash: &[u8]) {
    let cleanup_queries = vec![
        "DELETE FROM casts WHERE message_hash = $1",
        "DELETE FROM links WHERE message_hash = $1",
        "DELETE FROM reactions WHERE message_hash = $1",
        "DELETE FROM verifications WHERE message_hash = $1",
        "DELETE FROM user_profile_changes WHERE message_hash = $1",
        // username_proofs: Skip (no message_hash column, has UNIQUE constraint)
        "DELETE FROM frame_actions WHERE message_hash = $1",
    ];

    for query in cleanup_queries {
        sqlx::query(query)
            .bind(message_hash)
            .execute(db.pool())
            .await
            .ok();
    }
}

/// Generate unique test message hash with prefix
fn test_message_hash(test_id: u32) -> Vec<u8> {
    let mut hash = vec![0xFE, 0xFE]; // Test marker prefix
    hash.extend_from_slice(&test_id.to_be_bytes());
    hash
}

/// Helper to create shard block info
const fn test_shard_info() -> ShardBlockInfo {
    ShardBlockInfo {
        shard_id: 1,
        block_height: 1000,
        transaction_fid: 99,
        timestamp: 1_698_765_432,
    }
}

#[tokio::test]
async fn test_message_types_quick() {
    let db = setup_test_db().await;
    let shard_info = test_shard_info();

    println!("\n🧪 Running Quick Message Types Test\n");

    // Test 1: CastAdd
    let test_hash_1 = test_message_hash(10001);
    cleanup_by_message_hash(&db, &test_hash_1).await;

    let mut batched = BatchedData::new();
    batched.casts.push((
        99,
        Some("Test cast".to_string()),
        1_698_765_432,
        test_hash_1.clone(),
        None,
        None,
        None,
        None,
        shard_info.clone(),
    ));
    flush_batched_data(&db, batched)
        .await
        .expect("Cast insert failed");

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM casts WHERE message_hash = $1")
        .bind(&test_hash_1)
        .fetch_one(db.pool())
        .await
        .expect("Query failed");
    assert_eq!(count.0, 1);
    cleanup_by_message_hash(&db, &test_hash_1).await;
    println!("✅ CastAdd test passed");

    // Test 2: LinkAdd
    let test_hash_2 = test_message_hash(10002);
    cleanup_by_message_hash(&db, &test_hash_2).await;

    let mut batched = BatchedData::new();
    batched.links.push((
        99,
        100,
        "follow".to_string(),
        "add".to_string(), // event_type
        1_698_765_432,
        test_hash_2.clone(),
        shard_info.clone(),
    ));
    flush_batched_data(&db, batched)
        .await
        .expect("Link insert failed");

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM links WHERE message_hash = $1")
        .bind(&test_hash_2)
        .fetch_one(db.pool())
        .await
        .expect("Query failed");
    assert_eq!(count.0, 1);
    cleanup_by_message_hash(&db, &test_hash_2).await;
    println!("✅ LinkAdd test passed");

    // Test 3: UsernameProof
    let test_hash_3 = test_message_hash(10003);
    cleanup_by_message_hash(&db, &test_hash_3).await;

    let mut batched = BatchedData::new();
    batched.username_proofs.push((
        99,
        "testuser".to_string(),
        vec![0x12; 20],
        vec![0x34; 65],
        1,
        1_698_765_432,
        test_hash_3.clone(),
        shard_info.clone(),
    ));
    flush_batched_data(&db, batched)
        .await
        .expect("Username proof insert failed");

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM username_proofs WHERE fid = 99 AND username_type = 1")
            .fetch_one(db.pool())
            .await
            .expect("Query failed");
    assert_eq!(count.0, 1);
    cleanup_by_message_hash(&db, &test_hash_3).await;
    println!("✅ UsernameProof test passed");

    println!("\n✅ All quick tests passed!\n");
}

#[tokio::test]
async fn test_cleanup_safety_verification() {
    let test_hash_1 = test_message_hash(1);
    let test_hash_2 = test_message_hash(2);

    // Verify test hashes have the 0xFEFE prefix
    assert_eq!(test_hash_1[0], 0xFE);
    assert_eq!(test_hash_1[1], 0xFE);
    assert_ne!(test_hash_1, test_hash_2);

    println!("✅ Test cleanup safety verified");
}
