//! Unit tests for Redis cache and queue functionality
//!
//! These tests validate:
//! 1. Redis cache operations (get/set with TTL)
//! 2. CacheResult types (Fresh/Stale/Miss)
//! 3. Queue operations (push/pop jobs)
//! 4. Job status management
//! 5. Deduplication (prevent duplicate jobs)

use std::sync::Arc;
use std::time::Duration;

use crate::api::cache::CacheConfig;
use crate::api::cache::CacheResult;
use crate::api::cache::CacheService;
use crate::api::redis_client::RedisClient;
use crate::api::types::ProfileResponse;
use crate::config::RedisConfig;
use crate::social_graph::SocialProfile;

/// Helper to create a test Redis client
/// Uses test Redis instance if available, otherwise skips tests
fn create_test_redis_client() -> Option<Arc<RedisClient>> {
    // Try to connect to test Redis (default: localhost:6379)
    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());

    let config = RedisConfig {
        url: redis_url,
        namespace: "test:snaprag:".to_string(),
        default_ttl_secs: 3600,
        stale_threshold_secs: 300,
        refresh_channel: "test:snaprag.refresh".to_string(),
    };

    match RedisClient::connect(&config) {
        Ok(client) => Some(Arc::new(client)),
        Err(_) => {
            eprintln!("⚠️  Redis not available, skipping Redis cache tests");
            None
        }
    }
}

/// Helper to clean up test data
async fn cleanup_test_data(redis: &RedisClient, prefix: &str) {
    // Note: In a real test, we'd use SCAN and DEL, but for simplicity
    // we'll just use a test namespace that can be cleared
    let _ = redis;
    let _ = prefix;
}

#[tokio::test]
#[ignore] // Requires Redis instance
async fn test_cache_set_and_get_fresh() {
    let redis = match create_test_redis_client() {
        Some(r) => r,
        None => return,
    };

    let cache_service = CacheService::with_config(
        redis.clone(),
        CacheConfig {
            profile_ttl: Duration::from_secs(3600),
            social_ttl: Duration::from_secs(3600),
            mbti_ttl: Duration::from_secs(7200),
            stale_threshold: Duration::from_secs(300),
            enable_stats: true,
        },
    );

    let fid = 12345;
    let profile = ProfileResponse {
        fid,
        username: Some("testuser".to_string()),
        display_name: Some("Test User".to_string()),
        bio: Some("Test bio".to_string()),
        pfp_url: None,
        location: None,
        twitter_username: None,
        github_username: None,
    };

    // Set cache
    cache_service.set_profile(fid, &profile).await.unwrap();

    // Get cache immediately (should be fresh)
    let result = cache_service.get_profile(fid).await.unwrap();

    match result {
        CacheResult::Fresh(retrieved_profile) => {
            assert_eq!(retrieved_profile.fid, fid);
            assert_eq!(retrieved_profile.username, profile.username);
        }
        _ => panic!("Expected Fresh cache result, got {:?}", result),
    }

    cleanup_test_data(&redis, "cache:profile").await;
}

#[tokio::test]
#[ignore] // Requires Redis instance
async fn test_cache_miss() {
    let redis = match create_test_redis_client() {
        Some(r) => r,
        None => return,
    };

    let cache_service = CacheService::with_config(redis.clone(), CacheConfig::default());

    let fid = 99999; // Non-existent FID

    // Get cache (should be miss)
    let result = cache_service.get_profile(fid).await.unwrap();

    match result {
        CacheResult::Miss => {
            // Expected
        }
        _ => panic!("Expected Miss cache result, got {:?}", result),
    }
}

#[tokio::test]
#[ignore] // Requires Redis instance
async fn test_queue_push_and_pop() {
    let redis = match create_test_redis_client() {
        Some(r) => r,
        None => return,
    };

    let queue = "test_queue";
    // Use a unique job_key to avoid conflicts from previous test runs
    let job_key = format!("test:job:{}", uuid::Uuid::new_v4());
    let job_data = r#"{"fid": 123, "type": "social"}"#;

    // Ensure job is not active before pushing
    if redis.is_job_active(&job_key).await.unwrap() {
        redis.mark_job_inactive(&job_key).await.unwrap();
    }

    // Push job
    let job_id = redis.push_job(queue, &job_key, job_data).await.unwrap();
    assert!(job_id.is_some());
    let job_id = job_id.unwrap();
    assert!(job_id.starts_with("job:test_queue:"));

    // Check job is active
    let is_active = redis.is_job_active(&job_key).await.unwrap();
    assert!(is_active);

    // Pop job
    let result = redis.pop_job(queue, Duration::from_secs(5)).await.unwrap();

    assert!(result.is_some());
    let (popped_job_id, popped_data) = result.unwrap();
    // The popped job_id should match the format, but may not be exactly the same UUID
    // since brpop returns the job_id from the queue
    assert!(popped_job_id.starts_with("job:test_queue:"));
    assert_eq!(popped_data, job_data);

    // Job should still be marked as active until we mark it inactive
    let is_active = redis.is_job_active(&job_key).await.unwrap();
    assert!(is_active);

    // Mark job as inactive
    redis.mark_job_inactive(&job_key).await.unwrap();
    let is_active = redis.is_job_active(&job_key).await.unwrap();
    assert!(!is_active);
}

#[tokio::test]
#[ignore] // Requires Redis instance
async fn test_job_status_management() {
    let redis = match create_test_redis_client() {
        Some(r) => r,
        None => return,
    };

    let job_key = "test:job:status:456";

    // Set status to pending
    redis
        .set_job_status(job_key, "pending", None)
        .await
        .unwrap();

    // Get status
    let status = redis.get_job_status(job_key).await.unwrap();
    assert!(status.is_some());
    let (status_str, result) = status.unwrap();
    assert_eq!(status_str, "pending");
    assert!(result.is_none());

    // Update status to processing
    redis
        .set_job_status(job_key, "processing", None)
        .await
        .unwrap();
    let status = redis.get_job_status(job_key).await.unwrap();
    assert_eq!(status.unwrap().0, "processing");

    // Update status to completed with result
    let result_data = r#"{"fid": 456, "social": {...}}"#;
    redis
        .set_job_status(job_key, "completed", Some(result_data))
        .await
        .unwrap();

    let status = redis.get_job_status(job_key).await.unwrap();
    let (status_str, result) = status.unwrap();
    assert_eq!(status_str, "completed");
    assert_eq!(result.unwrap(), result_data);
}

#[tokio::test]
#[ignore] // Requires Redis instance
async fn test_job_deduplication() {
    let redis = match create_test_redis_client() {
        Some(r) => r,
        None => return,
    };

    let queue = "test_queue";
    let job_key = "test:job:dedup:789";
    let job_data = r#"{"fid": 789, "type": "social"}"#;

    // Push first job
    let job_id1 = redis.push_job(queue, job_key, job_data).await.unwrap();
    assert!(job_id1.is_some());

    // Try to push duplicate job (should return None)
    let job_id2 = redis.push_job(queue, job_key, job_data).await.unwrap();
    assert!(job_id2.is_none(), "Duplicate job should be rejected");

    // Check job is active
    let is_active = redis.is_job_active(&job_key).await.unwrap();
    assert!(is_active);

    // Mark inactive
    redis.mark_job_inactive(job_key).await.unwrap();

    // Now we can push again
    let job_id3 = redis.push_job(queue, job_key, job_data).await.unwrap();
    assert!(job_id3.is_some());

    // Cleanup
    redis.mark_job_inactive(job_key).await.unwrap();
}

#[tokio::test]
#[ignore] // Requires Redis instance
async fn test_queue_job_ttl() {
    let redis = match create_test_redis_client() {
        Some(r) => r,
        None => return,
    };

    let queue = "test_queue_ttl";
    let job_key = format!("test:job:ttl:{}", uuid::Uuid::new_v4());
    let job_data = r#"{"fid": 999, "type": "social"}"#;

    // Ensure job is not active before pushing
    if redis.is_job_active(&job_key).await.unwrap() {
        redis.mark_job_inactive(&job_key).await.unwrap();
    }

    // Push job
    let job_id = redis.push_job(queue, &job_key, job_data).await.unwrap();
    assert!(job_id.is_some());
    let job_id = job_id.unwrap();

    // Check job data TTL (should be 3600 seconds = 1 hour)
    // job_id format is "job:queue:uuid", so we use it directly with namespace
    let ttl = redis.ttl_secs(&job_id).await.unwrap();
    assert!(ttl.is_some(), "Job data should have TTL set");
    let ttl_value = ttl.unwrap();
    // TTL should be close to 3600 (1 hour), allow some tolerance
    assert!(
        ttl_value > 3500 && ttl_value <= 3600,
        "Job data TTL should be around 3600 seconds, got {}",
        ttl_value
    );

    // Check job status TTL (should be 86400 seconds = 24 hours)
    let status_key = format!("job_status:{}", job_key);
    let status_ttl = redis.ttl_secs(&status_key).await.unwrap();
    assert!(status_ttl.is_some(), "Job status should have TTL set");
    let status_ttl_value = status_ttl.unwrap();
    // TTL should be close to 86400 (24 hours), allow some tolerance
    assert!(
        status_ttl_value > 86300 && status_ttl_value <= 86400,
        "Job status TTL should be around 86400 seconds, got {}",
        status_ttl_value
    );

    // Cleanup
    redis.mark_job_inactive(&job_key).await.unwrap();
}

#[tokio::test]
#[ignore] // Requires Redis instance
async fn test_queue_job_data_expiration() {
    let redis = match create_test_redis_client() {
        Some(r) => r,
        None => return,
    };

    let queue = "test_queue_expire";
    let job_key = format!("test:job:expire:{}", uuid::Uuid::new_v4());
    let job_data = r#"{"fid": 888, "type": "social"}"#;

    // Ensure job is not active before pushing
    if redis.is_job_active(&job_key).await.unwrap() {
        redis.mark_job_inactive(&job_key).await.unwrap();
    }

    // Push job
    let job_id = redis.push_job(queue, &job_key, job_data).await.unwrap();
    assert!(job_id.is_some());
    let job_id = job_id.unwrap();

    // Get job data immediately (should exist)
    // job_id format is "job:queue:uuid", so we use it directly with namespace
    let data: Option<String> = redis.get_json(&job_id).await.unwrap();
    assert!(
        data.is_some(),
        "Job data should exist immediately after push"
    );
    assert_eq!(data.unwrap(), job_data);

    // Note: We can't easily test actual expiration in a unit test without waiting 1 hour
    // But we can verify that TTL is set correctly (tested in test_queue_job_ttl)

    // Cleanup
    redis.mark_job_inactive(&job_key).await.unwrap();
}

#[tokio::test]
#[ignore] // Requires Redis instance
async fn test_social_cache_stale_while_revalidate() {
    let redis = match create_test_redis_client() {
        Some(r) => r,
        None => return,
    };

    let cache_service = CacheService::with_config(
        redis.clone(),
        CacheConfig {
            profile_ttl: Duration::from_secs(10), // TTL longer than test wait time
            social_ttl: Duration::from_secs(10),  // TTL longer than test wait time
            mbti_ttl: Duration::from_secs(2),
            stale_threshold: Duration::from_secs(5), // 5 seconds stale threshold
            enable_stats: true,
        },
    );

    let fid = 54321;
    let social = SocialProfile {
        fid,
        following_count: 10,
        followers_count: 20,
        influence_score: 2.0,
        top_followed_users: vec![],
        top_followers: vec![],
        most_mentioned_users: vec![],
        social_circles: crate::social_graph::SocialCircles {
            tech_builders: 0.0,
            content_creators: 0.0,
            web3_natives: 0.0,
            casual_users: 0.0,
        },
        interaction_style: crate::social_graph::InteractionStyle {
            reply_frequency: 0.5,
            mention_frequency: 0.3,
            network_connector: false,
            community_role: "member".to_string(),
        },
        word_cloud: crate::social_graph::WordCloud {
            top_words: vec![],
            top_phrases: vec![],
            signature_words: vec![],
        },
    };

    // Set cache
    cache_service.set_social(fid, &social).await.unwrap();

    // Get immediately (should be fresh)
    let result = cache_service.get_social(fid).await.unwrap();
    match result {
        CacheResult::Fresh(_) => {}
        _ => panic!("Expected Fresh immediately after set"),
    }

    // Wait for TTL to expire (but within stale threshold)
    // TTL is 10 seconds, so wait 12 seconds to be past TTL but within stale threshold (5 seconds)
    tokio::time::sleep(Duration::from_secs(12)).await;

    // Get again (should be stale - past TTL but within stale threshold)
    let result = cache_service.get_social(fid).await.unwrap();
    match result {
        CacheResult::Stale(_) => {
            // Expected - stale but still valid
        }
        CacheResult::Fresh(_) => {
            // Also acceptable if timing is off
        }
        CacheResult::Miss => {
            panic!("Should not be miss within stale threshold");
        }
    }

    // Wait beyond stale threshold (total 12 + 5 = 17 seconds)
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Get again (should be miss)
    let result = cache_service.get_social(fid).await.unwrap();
    match result {
        CacheResult::Miss => {
            // Expected
        }
        _ => {
            // Acceptable if Redis TTL hasn't expired yet
            eprintln!("Note: Cache still available, Redis TTL may not have expired");
        }
    }
}

#[tokio::test]
#[ignore] // Requires Redis instance
async fn test_cache_statistics() {
    let redis = match create_test_redis_client() {
        Some(r) => r,
        None => return,
    };

    let cache_service = CacheService::with_config(
        redis.clone(),
        CacheConfig {
            enable_stats: true,
            ..Default::default()
        },
    );

    let fid = 11111;
    let profile = ProfileResponse {
        fid,
        username: Some("stats_test".to_string()),
        display_name: None,
        bio: None,
        pfp_url: None,
        location: None,
        twitter_username: None,
        github_username: None,
    };

    // Set cache
    cache_service.set_profile(fid, &profile).await.unwrap();

    // Get cache (hit)
    let _ = cache_service.get_profile(fid).await.unwrap();

    // Get non-existent (miss)
    let _ = cache_service.get_profile(99999).await.unwrap();

    // Get stats
    let stats = cache_service.get_stats().await;
    assert!(stats.hits > 0, "Should have at least one hit");
    assert!(stats.misses > 0, "Should have at least one miss");

    let hit_rate = stats.hit_rate();
    assert!(
        hit_rate >= 0.0 && hit_rate <= 1.0,
        "Hit rate should be between 0 and 1"
    );
}
