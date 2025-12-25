//! Test to reproduce null byte UTF-8 encoding error
//! This test verifies that null bytes in text fields are properly sanitized

use crate::database::Database;
use crate::models::ShardBlockInfo;
use crate::sync::shard_processor::flush_batched_data;
use crate::sync::shard_processor::BatchedData;
use crate::Result;

/// Test helper to create a test database
async fn setup_test_db() -> Database {
    let config_path =
        std::env::var("SNAPRAG_CONFIG").unwrap_or_else(|_| "config.test.toml".to_string());
    let config = crate::config::AppConfig::from_file(&config_path).expect("Failed to load config");

    let db_url = config.database_url();
    assert!(
        db_url.contains("localhost") || db_url.contains("127.0.0.1") || db_url.contains("::1"),
        "❌ SAFETY CHECK FAILED: Test database must be localhost!\n\
         Current URL: {db_url}\n\
         Set SNAPRAG_CONFIG=config.test.toml to use test database"
    );

    Database::from_config(&config)
        .await
        .expect("Failed to connect to test database")
}

fn test_shard_info() -> ShardBlockInfo {
    ShardBlockInfo {
        shard_id: 1,
        block_height: 1000,
        transaction_fid: 99,
        timestamp: 1_698_765_432,
    }
}

fn test_message_hash(seed: u32) -> Vec<u8> {
    let mut hash = vec![0u8; 32];
    hash[0..4].copy_from_slice(&seed.to_be_bytes());
    hash
}

#[tokio::test]
#[ignore = "Requires database access - production database should not be modified"]
async fn test_null_byte_in_cast_text() {
    let db = setup_test_db().await;
    let shard_info = test_shard_info();

    println!("🧪 Testing null byte in cast text...");

    let mut batched = BatchedData::new();
    batched.fids_to_ensure.insert(99);

    // Create a cast with null bytes in the text
    // This should fail with: "invalid byte sequence for encoding \"UTF8\": 0x00"
    let text_with_null_bytes = "Hello\0World\0Test".to_string();
    let hash = test_message_hash(9999);

    batched.casts.push((
        99,                         // fid
        Some(text_with_null_bytes), // text with null bytes
        1_698_765_432,              // timestamp
        hash,                       // message_hash
        None,                       // parent_hash
        None,                       // root_hash
        None,                       // embeds
        None,                       // mentions
        shard_info,                 // shard_block_info
    ));

    // This should fail before the fix, succeed after the fix
    let result = flush_batched_data(&db, batched).await;

    assert!(
        result.is_ok(),
        "Batch insert should succeed after sanitization: {:?}",
        result.err()
    );

    // Verify the cast was inserted and null bytes were removed
    let stored_text: Option<String> =
        sqlx::query_scalar("SELECT text FROM casts WHERE fid = 99 AND message_hash = $1")
            .bind(test_message_hash(9999))
            .fetch_optional(db.pool())
            .await
            .expect("Failed to query cast");

    assert_eq!(
        stored_text,
        Some("HelloWorldTest".to_string()),
        "Null bytes should be removed from text"
    );

    // Cleanup
    sqlx::query("DELETE FROM casts WHERE fid = 99 AND message_hash = $1")
        .bind(test_message_hash(9999))
        .execute(db.pool())
        .await
        .ok();

    println!("✅ Null byte in cast text test passed");
}

#[tokio::test]
#[ignore = "Requires database access - production database should not be modified"]
async fn test_null_byte_in_profile_fields() {
    let db = setup_test_db().await;

    println!("🧪 Testing null byte in profile fields...");

    let mut batched = BatchedData::new();
    batched.fids_to_ensure.insert(99);

    // Create profile updates with null bytes
    let hash1 = test_message_hash(8888);
    let hash2 = test_message_hash(8889);
    let hash3 = test_message_hash(8890);

    batched.profile_updates.push((
        99,                              // fid
        "display_name".to_string(),      // field_name
        Some("John\0Doe\0".to_string()), // value with null bytes
        1_698_765_432,                   // timestamp
        hash1,                           // message_hash
    ));

    batched.profile_updates.push((
        99,                                   // fid
        "bio".to_string(),                    // field_name
        Some("Bio\0with\0nulls".to_string()), // value with null bytes
        1_698_765_433,                        // timestamp
        hash2,                                // message_hash
    ));

    batched.profile_updates.push((
        99,                             // fid
        "username".to_string(),         // field_name
        Some("user\0name".to_string()), // value with null bytes
        1_698_765_434,                  // timestamp
        hash3,                          // message_hash
    ));

    // This should succeed after the fix
    let result = flush_batched_data(&db, batched).await;

    assert!(
        result.is_ok(),
        "Batch insert should succeed after sanitization: {:?}",
        result.err()
    );

    // Verify the profile updates were inserted and null bytes were removed
    let display_name: Option<String> = sqlx::query_scalar(
        "SELECT field_value FROM user_profile_changes 
         WHERE fid = 99 AND field_name = 'display_name' 
         ORDER BY timestamp DESC LIMIT 1",
    )
    .fetch_optional(db.pool())
    .await
    .expect("Failed to query display_name");

    assert_eq!(
        display_name,
        Some("JohnDoe".to_string()),
        "Null bytes should be removed from display_name"
    );

    // Cleanup
    sqlx::query("DELETE FROM user_profile_changes WHERE fid = 99")
        .execute(db.pool())
        .await
        .ok();

    println!("✅ Null byte in profile fields test passed");
}

#[tokio::test]
#[ignore = "Requires database access - production database should not be modified"]
async fn test_null_byte_in_frame_action_text() {
    let db = setup_test_db().await;
    let shard_info = test_shard_info();

    println!("🧪 Testing null byte in frame action text fields...");

    let mut batched = BatchedData::new();
    batched.fids_to_ensure.insert(99);

    // Create frame action with null bytes in input_text
    let hash = test_message_hash(7777);

    batched.frame_actions.push((
        99,                                           // fid
        "https://example.com".to_string(),            // url (String, not Option)
        Some(1),                                      // button_index
        Some(vec![0xAA; 32]),                         // cast_hash
        Some(100),                                    // cast_fid
        Some("Input\0Text\0".to_string()),            // input_text with null bytes
        Some("State\0Data".to_string().into_bytes()), // state with null bytes (Vec<u8>)
        Some(vec![0xBB; 32]),                         // transaction_id
        1_698_765_432,                                // timestamp
        hash,                                         // message_hash
        shard_info,                                   // shard_block_info
    ));

    // This should succeed after the fix
    let result = flush_batched_data(&db, batched).await;

    assert!(
        result.is_ok(),
        "Batch insert should succeed after sanitization: {:?}",
        result.err()
    );

    // Verify the frame action was inserted and null bytes were removed
    let input_text: Option<String> = sqlx::query_scalar(
        "SELECT input_text FROM frame_actions WHERE fid = 99 AND message_hash = $1",
    )
    .bind(test_message_hash(7777))
    .fetch_optional(db.pool())
    .await
    .expect("Failed to query frame action");

    assert_eq!(
        input_text,
        Some("InputText".to_string()),
        "Null bytes should be removed from input_text"
    );

    // Cleanup
    sqlx::query("DELETE FROM frame_actions WHERE fid = 99 AND message_hash = $1")
        .bind(test_message_hash(7777))
        .execute(db.pool())
        .await
        .ok();

    println!("✅ Null byte in frame action text test passed");
}
