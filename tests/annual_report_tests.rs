//! Integration tests for annual report APIs
//!
//! These tests verify that the annual report endpoints work correctly
//! and return the expected data structures.

use snaprag::api::handlers::AppState;
use snaprag::api::types::ApiResponse;
use snaprag::AppConfig;
use snaprag::Database;
use snaprag::SnapRag;

/// Test helper to create a test AppState
async fn create_test_app_state() -> AppState {
    let config = AppConfig::load().expect("Failed to load config");
    let snaprag = SnapRag::new(&config)
        .await
        .expect("Failed to create SnapRag instance");

    AppState {
        config: std::sync::Arc::new(config),
        database: snaprag.database.clone(),
        embedding_service: snaprag.embedding_service.clone(),
        llm_service: None,
        lazy_loader: None,
        session_manager: snaprag.session_manager.clone(),
        cache_service: snaprag.cache_service.clone(),
    }
}

#[tokio::test]
#[ignore] // Requires database connection
async fn test_engagement_endpoint_structure() {
    let state = create_test_app_state().await;

    // Test with a known FID (if available in test database)
    let fid = 1;

    // Test the endpoint structure (we'll test actual data if database has data)
    // This is a structural test to ensure the API compiles and responds
    let result = snaprag::api::handlers::engagement::get_engagement(
        axum::extract::State(state),
        axum::extract::Path(fid),
        axum::extract::Query(snaprag::api::types::EngagementRequest {
            start_timestamp: None,
            end_timestamp: None,
        }),
    )
    .await;

    match result {
        Ok(response) => {
            // Verify response structure
            assert!(response.0.success);
            if let Some(data) = response.0.data {
                // Verify it's a valid JSON object
                assert!(data.is_object());
            }
        }
        Err(status) => {
            // 404 is acceptable if user doesn't exist
            // 500 would indicate a real problem
            assert_ne!(status, axum::http::StatusCode::INTERNAL_SERVER_ERROR);
        }
    }
}

#[tokio::test]
#[ignore] // Requires database connection
async fn test_temporal_activity_endpoint_structure() {
    let state = create_test_app_state().await;
    let fid = 1;

    let result = snaprag::api::handlers::temporal_activity::get_temporal_activity(
        axum::extract::State(state),
        axum::extract::Path(fid),
        axum::extract::Query(snaprag::api::types::TemporalActivityRequest {
            start_timestamp: None,
            end_timestamp: None,
        }),
    )
    .await;

    match result {
        Ok(response) => {
            assert!(response.0.success);
        }
        Err(status) => {
            assert_ne!(status, axum::http::StatusCode::INTERNAL_SERVER_ERROR);
        }
    }
}

#[tokio::test]
async fn test_emoji_extraction() {
    use snaprag::utils::emoji::{count_emoji_frequencies, extract_emojis};

    let text = "Hello 🔥 world 💎 and 🚀";
    let emojis = extract_emojis(text);
    assert_eq!(emojis.len(), 3);
    assert!(emojis.contains(&"🔥".to_string()));
    assert!(emojis.contains(&"💎".to_string()));
    assert!(emojis.contains(&"🚀".to_string()));

    let frequencies = count_emoji_frequencies(text);
    assert_eq!(frequencies.get("🔥"), Some(&1));
    assert_eq!(frequencies.get("💎"), Some(&1));
    assert_eq!(frequencies.get("🚀"), Some(&1));
}

#[tokio::test]
async fn test_emoji_no_emojis() {
    use snaprag::utils::emoji::extract_emojis;

    let text = "Hello world";
    let emojis = extract_emojis(text);
    assert_eq!(emojis.len(), 0);
}

