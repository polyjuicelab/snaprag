//! API endpoint tests

use reqwest::Client;
use serde_json::json;

const API_URL: &str = "http://127.0.0.1:3000";

#[tokio::test]
#[ignore = "Requires running API server (cargo test --test api_tests -- --ignored --nocapture)"]
async fn test_health_endpoint() {
    let client = Client::new();
    let response = client
        .get(format!("{API_URL}/api/health"))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["success"], true);
    println!("✅ Health endpoint test passed");
}

#[tokio::test]
#[ignore = "Requires running API server"]
async fn test_stats_endpoint() {
    let client = Client::new();
    let response = client
        .get(format!("{API_URL}/api/stats"))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["success"], true);

    let stats = &body["data"];
    println!("✅ Stats endpoint test passed");
    println!("   Total profiles: {}", stats["total_profiles"]);
    println!("   Total casts: {}", stats["total_casts"]);
}

#[tokio::test]
#[ignore = "Requires running API server"]
async fn test_mcp_tools() {
    let client = Client::new();
    let response = client
        .get(format!("{API_URL}/mcp/tools"))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let tools: Vec<serde_json::Value> = response.json().await.expect("Failed to parse JSON");
    assert!(!tools.is_empty());
    println!("✅ MCP tools test passed");
    println!("   Found {} tools", tools.len());
}

#[tokio::test]
#[ignore = "Requires running API server"]
async fn test_mcp_resources() {
    let client = Client::new();
    let response = client
        .get(format!("{API_URL}/mcp/resources"))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let resources: Vec<serde_json::Value> = response.json().await.expect("Failed to parse JSON");
    assert_eq!(resources.len(), 2); // profiles and casts
    println!("✅ MCP resources test passed");
}

#[tokio::test]
#[ignore = "Requires running API server"]
async fn test_all_free_endpoints() {
    println!("\n🧪 Testing all free endpoints...");
    println!("===================================\n");

    let client = Client::new();

    // Test health
    let response = client
        .get(format!("{API_URL}/api/health"))
        .send()
        .await
        .expect("Failed");
    assert_eq!(response.status(), 200);
    println!("✅ /api/health: OK");

    // Test stats
    let response = client
        .get(format!("{API_URL}/api/stats"))
        .send()
        .await
        .expect("Failed");
    assert_eq!(response.status(), 200);
    println!("✅ /api/stats: OK");

    // Test MCP tools
    let response = client
        .get(format!("{API_URL}/mcp/tools"))
        .send()
        .await
        .expect("Failed");
    assert_eq!(response.status(), 200);
    println!("✅ /mcp/tools: OK");

    // Test MCP resources
    let response = client
        .get(format!("{API_URL}/mcp/resources"))
        .send()
        .await
        .expect("Failed");
    assert_eq!(response.status(), 200);
    println!("✅ /mcp/resources: OK");

    println!("\n✅ All free endpoint tests passed!\n");
}
