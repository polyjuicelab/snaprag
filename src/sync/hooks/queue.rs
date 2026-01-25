//! Redis-backed hook event queue

use std::time::Duration;

use serde::Deserialize;
use serde::Serialize;
use tracing::debug;
use tracing::warn;

use crate::api::redis_client::RedisClient;
use crate::Result;

/// Redis list key for hook events (namespaced by RedisClient)
pub const HOOK_EVENT_QUEUE_KEY: &str = "hook_events";
/// Maximum number of events to keep in the queue
pub const HOOK_EVENT_QUEUE_MAX_LEN: usize = 1000;

/// Hook event payload stored in Redis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEvent {
    pub event_type: super::EventType,
    pub fid: i64,
    pub target_fid: Option<i64>,
    pub timestamp: i64,
    pub data: serde_json::Value,
    pub text: Option<String>,
}

impl From<&super::EventData> for HookEvent {
    fn from(event: &super::EventData) -> Self {
        Self {
            event_type: event.event_type,
            fid: event.fid,
            target_fid: event.target_fid,
            timestamp: event.timestamp,
            data: event.data.clone(),
            text: event.text.clone(),
        }
    }
}

impl From<HookEvent> for super::EventData {
    fn from(event: HookEvent) -> Self {
        Self {
            event_type: event.event_type,
            fid: event.fid,
            target_fid: event.target_fid,
            timestamp: event.timestamp,
            data: event.data,
            text: event.text,
        }
    }
}

/// Queue wrapper for hook events
#[derive(Clone)]
pub struct HookQueue {
    redis: RedisClient,
}

impl HookQueue {
    pub fn new(redis: RedisClient) -> Self {
        Self { redis }
    }

    /// Enqueue a hook event with a fixed queue size
    pub async fn enqueue(&self, event: &super::EventData) -> Result<()> {
        let payload = HookEvent::from(event);
        let json = serde_json::to_string(&payload)
            .map_err(|e| crate::SnapRagError::Custom(format!("Hook event serialize error: {e}")))?;
        self.redis
            .push_hook_event(&json, HOOK_EVENT_QUEUE_MAX_LEN)
            .await
    }

    /// Pop a hook event (blocking with timeout)
    pub async fn pop(&self, timeout: Duration) -> Result<Option<super::EventData>> {
        match self.redis.pop_hook_event(timeout).await? {
            Some(raw) => match serde_json::from_str::<HookEvent>(&raw) {
                Ok(event) => Ok(Some(event.into())),
                Err(e) => {
                    warn!("Failed to parse hook event JSON: {e}");
                    Ok(None)
                }
            },
            None => {
                debug!("No hook event received");
                Ok(None)
            }
        }
    }
}
