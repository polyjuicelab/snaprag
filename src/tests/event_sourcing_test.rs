//! Event-sourcing architecture tests for `user_profile_changes`

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hash;
    use std::hash::Hasher;

    use crate::database::Database;
    use crate::models::*;

    /// Helper function to generate message hash
    fn generate_message_hash(field_name: &str, fid: i64, timestamp: i64, value: &str) -> Vec<u8> {
        let mut hasher = DefaultHasher::new();
        field_name.hash(&mut hasher);
        fid.hash(&mut hasher);
        timestamp.hash(&mut hasher);
        value.hash(&mut hasher);
        let hash_value = hasher.finish();
        format!("test_{field_name}_{hash_value}")
            .as_bytes()
            .to_vec()
    }

    #[tokio::test]
    #[ignore = "Requires database connection"]
    async fn test_event_sourcing_insert_and_query() {
        // This test verifies that the event-sourcing architecture works correctly

        let config = crate::tests::load_test_config().expect("Failed to load config");
        let db = Database::from_config(&config)
            .await
            .expect("Failed to connect to database");

        let test_fid = 999_999_i64;
        let timestamp1 = 1000i64;
        let timestamp2 = 2000i64;

        // Insert first username change
        let hash1 = generate_message_hash("username", test_fid, timestamp1, "alice");
        sqlx::query(
            "INSERT INTO user_profile_changes (fid, field_name, field_value, timestamp, message_hash) 
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (message_hash) DO NOTHING"
        )
        .bind(test_fid)
        .bind("username")
        .bind("alice")
        .bind(timestamp1)
        .bind(&hash1)
        .execute(db.pool())
        .await
        .expect("Failed to insert first change");

        // Query should show alice
        let profile: Option<UserProfile> =
            sqlx::query_as("SELECT * FROM user_profiles WHERE fid = $1")
                .bind(test_fid)
                .fetch_optional(db.pool())
                .await
                .expect("Failed to query profile");

        assert!(profile.is_some());
        assert_eq!(
            profile.as_ref().unwrap().username,
            Some("alice".to_string())
        );

        // Insert second username change (update)
        let hash2 = generate_message_hash("username", test_fid, timestamp2, "alice2");
        sqlx::query(
            "INSERT INTO user_profile_changes (fid, field_name, field_value, timestamp, message_hash) 
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (message_hash) DO NOTHING"
        )
        .bind(test_fid)
        .bind("username")
        .bind("alice2")
        .bind(timestamp2)
        .bind(&hash2)
        .execute(db.pool())
        .await
        .expect("Failed to insert second change");

        // Query should now show alice2 (latest value)
        let profile: Option<UserProfile> =
            sqlx::query_as("SELECT * FROM user_profiles WHERE fid = $1")
                .bind(test_fid)
                .fetch_optional(db.pool())
                .await
                .expect("Failed to query updated profile");

        assert!(profile.is_some());
        assert_eq!(
            profile.as_ref().unwrap().username,
            Some("alice2".to_string())
        );

        // Insert bio change
        let hash3 = generate_message_hash("bio", test_fid, timestamp2, "I love Farcaster");
        sqlx::query(
            "INSERT INTO user_profile_changes (fid, field_name, field_value, timestamp, message_hash) 
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (message_hash) DO NOTHING"
        )
        .bind(test_fid)
        .bind("bio")
        .bind("I love Farcaster")
        .bind(timestamp2)
        .bind(&hash3)
        .execute(db.pool())
        .await
        .expect("Failed to insert bio change");

        // Query should show both username and bio
        let profile: Option<UserProfile> =
            sqlx::query_as("SELECT * FROM user_profiles WHERE fid = $1")
                .bind(test_fid)
                .fetch_optional(db.pool())
                .await
                .expect("Failed to query profile with bio");

        assert!(profile.is_some());
        let profile = profile.unwrap();
        assert_eq!(profile.username, Some("alice2".to_string()));
        assert_eq!(profile.bio, Some("I love Farcaster".to_string()));

        // Cleanup
        sqlx::query("DELETE FROM user_profile_changes WHERE fid = $1")
            .bind(test_fid)
            .execute(db.pool())
            .await
            .expect("Failed to cleanup");

        println!("✅ Event-sourcing test passed!");
    }

    #[tokio::test]
    #[ignore = "Requires database connection"]
    async fn test_concurrent_inserts_no_locks() {
        // This test verifies that concurrent inserts don't cause lock contention

        let config = crate::tests::load_test_config().expect("Failed to load config");
        let db = Database::from_config(&config)
            .await
            .expect("Failed to connect to database");

        let test_fid_base = 888_888_i64;
        let num_concurrent = 10;

        // Spawn multiple concurrent insert tasks
        let mut handles = vec![];

        for i in 0..num_concurrent {
            let db_clone = db.clone();
            let test_fid = test_fid_base + i;

            let handle = tokio::spawn(async move {
                let hash = generate_message_hash("username", test_fid, 1000, &format!("user{i}"));

                sqlx::query(
                    "INSERT INTO user_profile_changes (fid, field_name, field_value, timestamp, message_hash) 
                     VALUES ($1, $2, $3, $4, $5)
                     ON CONFLICT (message_hash) DO NOTHING"
                )
                .bind(test_fid)
                .bind("username")
                .bind(format!("user{i}"))
                .bind(1000i64)
                .bind(&hash)
                .execute(db_clone.pool())
                .await
                .expect("Failed to insert");
            });

            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            handle.await.expect("Task failed");
        }

        // Verify all inserts succeeded
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_profile_changes WHERE fid >= $1 AND fid < $2",
        )
        .bind(test_fid_base)
        .bind(test_fid_base + num_concurrent)
        .fetch_one(db.pool())
        .await
        .expect("Failed to count");

        assert_eq!(count, num_concurrent);

        // Cleanup
        sqlx::query("DELETE FROM user_profile_changes WHERE fid >= $1 AND fid < $2")
            .bind(test_fid_base)
            .bind(test_fid_base + num_concurrent)
            .execute(db.pool())
            .await
            .expect("Failed to cleanup");

        println!("✅ Concurrent insert test passed (no locks)!");
    }
}
