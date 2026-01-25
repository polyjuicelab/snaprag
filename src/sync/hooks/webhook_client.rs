//! HTTP webhook client for sending event notifications

use std::time::Duration;

use reqwest::Client;
use serde_json::Value;
use tracing::error;
use tracing::info;

/// HTTP client for sending webhook requests
#[derive(Clone)]
pub struct WebhookClient {
    client: Client,
}

impl WebhookClient {
    /// Create a new webhook client
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_else(|e| {
                error!("Failed to create webhook client: {}", e);
                // Fallback to default client
                Client::new()
            });

        Self { client }
    }

    /// Send a webhook POST request with retry logic
    pub async fn send_webhook(&self, url: &str, payload: &Value) -> crate::Result<()> {
        const MAX_RETRIES: u32 = 3;
        const RETRY_DELAY_MS: u64 = 1000;

        for attempt in 1..=MAX_RETRIES {
            match self.client.post(url).json(payload).send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        info!("Webhook sent successfully to {} (status: {})", url, status);
                        return Ok(());
                    }
                    let error_text = response.text().await.unwrap_or_default();
                    error!(
                        "Webhook failed with status {}: {} (attempt {}/{})",
                        status, error_text, attempt, MAX_RETRIES
                    );
                    if attempt < MAX_RETRIES {
                        tokio::time::sleep(Duration::from_millis(
                            RETRY_DELAY_MS * u64::from(attempt),
                        ))
                        .await;
                        continue;
                    }
                    return Err(crate::SnapRagError::Custom(format!(
                        "Webhook failed with status {}: {}",
                        status, error_text
                    )));
                }
                Err(e) => {
                    error!(
                        "Webhook request failed: {} (attempt {}/{})",
                        e, attempt, MAX_RETRIES
                    );
                    if attempt < MAX_RETRIES {
                        tokio::time::sleep(Duration::from_millis(
                            RETRY_DELAY_MS * u64::from(attempt),
                        ))
                        .await;
                        continue;
                    }
                    return Err(crate::SnapRagError::Custom(format!(
                        "Webhook request failed after {} attempts: {}",
                        MAX_RETRIES, e
                    )));
                }
            }
        }

        Err(crate::SnapRagError::Custom(
            "Webhook failed after all retries".to_string(),
        ))
    }
}

impl Default for WebhookClient {
    fn default() -> Self {
        Self::new()
    }
}
