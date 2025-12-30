//! Integration tests for Chat Queue functionality
//!
//! These tests validate:
//! 1. Chat job creation and queueing
//! 2. Worker processing of chat jobs
//! 3. Session persistence in Redis during worker processing
//! 4. Result retrieval and polling
//! 5. Concurrent chat request handling

use std::sync::Arc;
use std::time::Duration;

use crate::api::redis_client::RedisClient;
use crate::api::session::ChatSession;
use crate::api::session::SessionManager;
use crate::config::RedisConfig;

/// Helper to create a test Redis client
fn create_test_redis_client() -> Option<Arc<RedisClient>> {
    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());

    let config = RedisConfig {
        url: redis_url,
        namespace: "test:snaprag:chat:".to_string(),
        default_ttl_secs: 3600,
        stale_threshold_secs: 300,
        refresh_channel: "test:snaprag.refresh".to_string(),
    };

    match RedisClient::connect(&config) {
        Ok(client) => Some(Arc::new(client)),
        Err(_) => {
            eprintln!("⚠️  Redis not available, skipping chat queue tests");
            None
        }
    }
}

/// Helper to create test SessionManager
fn create_test_session_manager() -> Option<SessionManager> {
    create_test_redis_client().map(|redis| SessionManager::new(3600, redis))
}

/// Helper to cleanup test data
async fn cleanup_test_data(redis: &RedisClient, prefix: &str) {
    // Clean up test sessions and jobs
    let _ = redis.delete_pattern(&format!("{}*", prefix)).await;
}

#[tokio::test]
#[ignore] // Requires Redis instance
async fn test_chat_job_creation() {
    let redis = match create_test_redis_client() {
        Some(r) => r,
        None => return,
    };

    let session_manager = match create_test_session_manager() {
        Some(m) => m,
        None => return,
    };

    // Create a test session
    let session = session_manager
        .create_session(
            99,
            Some("test".to_string()),
            Some("Test".to_string()),
            20,
            0.7,
        )
        .await
        .expect("Failed to create session");

    // Create a chat job
    let message_id = uuid::Uuid::new_v4().to_string();
    let job_key = format!("chat:{}:{}", session.session_id, message_id);
    let job_data = serde_json::json!({
        "session_id": session.session_id,
        "message": "Hello, this is a test message",
        "type": "chat"
    })
    .to_string();

    // Ensure job is not active before pushing
    if redis.is_job_active(&job_key).await.unwrap() {
        redis.mark_job_inactive(&job_key).await.unwrap();
    }

    let job_id = redis
        .push_job("chat", &job_key, &job_data)
        .await
        .expect("Failed to push job");

    assert!(job_id.is_some(), "Job should be created");

    // Verify job status
    let status = redis
        .get_job_status(&job_key)
        .await
        .expect("Failed to get job status");

    assert!(status.is_some());
    let (status_str, _) = status.unwrap();
    assert_eq!(status_str, "pending");

    // Cleanup
    cleanup_test_data(&redis, "session:").await;
    cleanup_test_data(&redis, "chat:").await;
}

#[tokio::test]
#[ignore] // Requires Redis instance and full setup
async fn test_chat_session_persistence() {
    let session_manager = match create_test_session_manager() {
        Some(m) => m,
        None => return,
    };

    // Create session
    let mut session = session_manager
        .create_session(
            99,
            Some("test".to_string()),
            Some("Test".to_string()),
            20,
            0.7,
        )
        .await
        .expect("Failed to create session");

    // Add a message
    session.add_message("user", "Test message".to_string());

    // Update session
    session_manager
        .update_session(session.clone())
        .await
        .expect("Failed to update session");

    // Retrieve session from another "process" (simulating worker)
    let retrieved = session_manager
        .get_session(&session.session_id)
        .await
        .expect("Failed to get session")
        .expect("Session not found");

    assert_eq!(retrieved.conversation_history.len(), 1);
    assert_eq!(retrieved.conversation_history[0].content, "Test message");

    // Cleanup
    session_manager
        .delete_session(&session.session_id)
        .await
        .expect("Failed to delete session");
}

#[tokio::test]
#[ignore] // Requires Redis instance
async fn test_chat_result_retrieval() {
    let redis = match create_test_redis_client() {
        Some(r) => r,
        None => return,
    };

    let job_key = "chat:test-session:test-message";
    let result = serde_json::json!({
        "session_id": "test-session",
        "message": "Test response",
        "relevant_casts_count": 5,
        "conversation_length": 2,
    });

    // Set job status with result
    redis
        .set_job_status(&job_key, "completed", Some(&result.to_string()))
        .await
        .expect("Failed to set job status");

    // Retrieve result
    let status = redis
        .get_job_status(&job_key)
        .await
        .expect("Failed to get job status")
        .expect("Status not found");

    let (status_str, result_json) = status;
    assert_eq!(status_str, "completed");
    assert!(result_json.is_some());

    let retrieved_result: serde_json::Value =
        serde_json::from_str(&result_json.unwrap()).expect("Failed to parse result");
    assert_eq!(retrieved_result["message"], "Test response");

    // Cleanup
    cleanup_test_data(&redis, "chat:").await;
}

#[tokio::test]
#[ignore] // Requires Redis instance
async fn test_chat_concurrent_requests() {
    let redis = match create_test_redis_client() {
        Some(r) => r,
        None => return,
    };

    let session_manager = match create_test_session_manager() {
        Some(m) => m,
        None => return,
    };

    // Create a session
    let session = session_manager
        .create_session(
            99,
            Some("test".to_string()),
            Some("Test".to_string()),
            20,
            0.7,
        )
        .await
        .expect("Failed to create session");

    // Create multiple jobs concurrently
    let mut handles = Vec::new();
    for i in 0..5 {
        let redis_clone = redis.clone();
        let session_id = session.session_id.clone();
        let handle = tokio::spawn(async move {
            let message_id = uuid::Uuid::new_v4().to_string();
            let job_key = format!("chat:{}:{}", session_id, message_id);
            let job_data = serde_json::json!({
                "session_id": session_id,
                "message": format!("Concurrent message {}", i),
                "type": "chat"
            })
            .to_string();

            // Ensure job is not active
            if redis_clone.is_job_active(&job_key).await.unwrap() {
                redis_clone.mark_job_inactive(&job_key).await.unwrap();
            }

            redis_clone.push_job("chat", &job_key, &job_data).await
        });
        handles.push(handle);
    }

    // Wait for all jobs to be created
    let mut created_count = 0;
    for handle in handles {
        if let Ok(Ok(Some(_))) = handle.await {
            created_count += 1;
        }
    }

    assert!(created_count > 0, "At least some jobs should be created");

    // Cleanup
    cleanup_test_data(&redis, "session:").await;
    cleanup_test_data(&redis, "chat:").await;
}
