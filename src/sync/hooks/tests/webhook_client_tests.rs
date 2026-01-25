//! Tests for webhook client
//!
//! Note: These tests require a mock HTTP server or will test against real endpoints
//! For production, consider using wiremock or similar for HTTP mocking

use crate::sync::hooks::webhook_client::WebhookClient;

#[tokio::test]
#[ignore = "Requires a real HTTP endpoint or mock server"]
async fn test_webhook_client_send() {
    let client = WebhookClient::new();
    let payload = serde_json::json!({
        "event_type": "MERGE_MESSAGE",
        "fid": 123,
        "timestamp": 1000
    });

    // This test would require a mock server or a test endpoint
    // For now, we just test that the client can be created
    let _result = client
        .send_webhook("http://httpbin.org/post", &payload)
        .await;
    // Don't assert on result as httpbin.org may not be available
}

#[test]
fn test_webhook_client_creation() {
    let client = WebhookClient::new();
    // Just verify it can be created
    assert!(true);
}
