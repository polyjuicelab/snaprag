//! Integration tests for annual report APIs
//!
//! These tests verify that the annual report endpoints work correctly
//! and return the expected data structures.
#![allow(clippy::significant_drop_tightening)]

use snaprag::api::cache::CacheConfig;
use snaprag::api::cache::CacheService;
use snaprag::api::handlers::AppState;
use snaprag::api::redis_client::RedisClient;
use snaprag::api::session::SessionManager;
use snaprag::api::types::ApiResponse;
use snaprag::embeddings::EmbeddingService;
use snaprag::llm::LlmService;
use snaprag::sync::LazyLoader;
use snaprag::AppConfig;
use snaprag::Database;

/// Test helper to create a test `AppState`
async fn create_test_app_state() -> AppState {
    let config = AppConfig::load().expect("Failed to load config");
    let database = std::sync::Arc::new(
        Database::from_config(&config)
            .await
            .expect("Failed to create database"),
    );
    let embedding_service =
        std::sync::Arc::new(EmbeddingService::new(&config).expect("Embedding init failed"));
    let llm_service = LlmService::new(&config).ok().map(std::sync::Arc::new);
    let lazy_loader: Option<std::sync::Arc<LazyLoader>> = None;

    let redis_cfg = config
        .redis
        .as_ref()
        .expect("Redis configuration required for tests");
    let redis_client =
        std::sync::Arc::new(RedisClient::connect(redis_cfg).expect("Redis connect failed"));
    let session_manager = std::sync::Arc::new(SessionManager::new(3600, redis_client.clone()));
    let cache_service = std::sync::Arc::new(CacheService::with_config(
        redis_client,
        CacheConfig {
            profile_ttl: std::time::Duration::from_secs(config.cache.profile_ttl_secs),
            social_ttl: std::time::Duration::from_secs(config.cache.social_ttl_secs),
            mbti_ttl: std::time::Duration::from_secs(7200),
            cast_stats_ttl: std::time::Duration::from_secs(config.cache.cast_stats_ttl_secs),
            annual_report_ttl: std::time::Duration::from_secs(0),
            stale_threshold: std::time::Duration::from_secs(redis_cfg.stale_threshold_secs),
            enable_stats: config.cache.enable_stats,
        },
    ));

    AppState {
        config: std::sync::Arc::new(config),
        database,
        embedding_service,
        llm_service,
        lazy_loader,
        session_manager,
        cache_service,
    }
}

#[tokio::test]
#[ignore = "Requires database connection"]
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
#[ignore = "Requires database connection"]
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
    use snaprag::utils::emoji::count_emoji_frequencies;
    use snaprag::utils::emoji::extract_emojis;

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
