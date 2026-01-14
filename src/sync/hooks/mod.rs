//! HTTP Hook system for event processing during synchronization
//!
//! This module provides functionality to register hooks that trigger HTTP callbacks
//! when specific events match configured filters during the sync process.

mod config;
mod matcher;
mod webhook_client;

#[cfg(test)]
mod tests;

use std::sync::Arc;

pub use config::EventType;
pub use config::HookConfig;
pub use config::OnChainEventType;
pub use matcher::EventMatcher;
use tokio::sync::RwLock;
pub use webhook_client::WebhookClient;

/// Manager for all registered hooks
#[derive(Clone)]
pub struct HookManager {
    hooks: Arc<RwLock<Vec<HookConfig>>>,
    webhook_client: WebhookClient,
}

impl HookManager {
    /// Create a new HookManager
    pub fn new() -> Self {
        Self {
            hooks: Arc::new(RwLock::new(Vec::new())),
            webhook_client: WebhookClient::new(),
        }
    }

    /// Register a new hook
    pub async fn register_hook(&self, config: HookConfig) {
        let mut hooks = self.hooks.write().await;
        hooks.push(config);
        tracing::info!("Registered hook: {:?}", hooks.last());
    }

    /// Remove all hooks matching the given criteria
    pub async fn remove_hooks(&self, event_type: Option<EventType>, url: Option<&str>) {
        let mut hooks = self.hooks.write().await;
        hooks.retain(|hook| {
            if let Some(et) = &event_type {
                if hook.event_type != *et {
                    return true;
                }
            }
            if let Some(u) = url {
                if hook.webhook_url != u {
                    return true;
                }
            }
            false
        });
    }

    /// Get all registered hooks
    pub async fn get_hooks(&self) -> Vec<HookConfig> {
        let hooks = self.hooks.read().await;
        hooks.clone()
    }

    /// Check if any hooks match the given event and trigger webhooks
    pub async fn check_and_trigger(&self, event: &EventData) {
        let hooks = self.hooks.read().await;
        let matcher = EventMatcher::new();

        for hook in hooks.iter() {
            if matcher.matches(hook, event) {
                tracing::debug!("Hook matched for event: {:?}", event);
                let payload = serde_json::json!({
                    "event_type": format!("{:?}", event.event_type),
                    "fid": event.fid,
                    "target_fid": event.target_fid,
                    "timestamp": event.timestamp,
                    "data": event.data
                });

                // Trigger webhook asynchronously without blocking
                let client = self.webhook_client.clone();
                let url = hook.webhook_url.clone();
                tokio::spawn(async move {
                    if let Err(e) = client.send_webhook(&url, &payload).await {
                        tracing::error!("Failed to send webhook to {}: {}", url, e);
                    }
                });
            }
        }
    }
}

impl Default for HookManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Event data structure for hook matching
#[derive(Debug, Clone)]
pub struct EventData {
    pub event_type: EventType,
    pub fid: i64,
    pub target_fid: Option<i64>,
    pub timestamp: i64,
    pub data: serde_json::Value,
    pub text: Option<String>, // For regex matching on cast text, username, etc.
}
