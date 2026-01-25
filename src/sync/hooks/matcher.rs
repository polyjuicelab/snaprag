//! Event matcher for hook filtering

use regex::Regex;
use tracing::debug;

use super::config::EventType;
use super::config::HookConfig;
use super::config::OnChainEventType;
use super::EventData;

/// Matcher for checking if events match hook filters
pub struct EventMatcher;

impl EventMatcher {
    pub fn new() -> Self {
        Self
    }

    /// Check if an event matches the hook configuration
    pub fn matches(&self, hook: &HookConfig, event: &EventData) -> bool {
        // Check event type match
        if hook.event_type != event.event_type {
            return false;
        }

        // For OnChain events, check onchain_event_type
        if hook.event_type == EventType::OnChain {
            if let Some(expected_type) = hook.onchain_event_type {
                // Extract onchain event type from event data
                if let Some(event_type_value) = event.data.get("event_type") {
                    if let Some(event_type_num) = event_type_value.as_u64() {
                        let actual_type = match event_type_num {
                            1 => OnChainEventType::Signer,
                            2 => OnChainEventType::SignerMigrated,
                            3 => OnChainEventType::IdRegister,
                            4 => OnChainEventType::StorageRent,
                            5 => OnChainEventType::TierPurchase,
                            _ => return false,
                        };
                        if actual_type != expected_type {
                            return false;
                        }
                    }
                }
            }
        }

        // Check FID filter
        if let Some(filter_fid) = hook.fid_filter {
            if event.fid != filter_fid {
                debug!("FID filter mismatch: {} != {}", event.fid, filter_fid);
                return false;
            }
        }

        // Check target FID filter
        if let Some(filter_target_fid) = hook.target_fid_filter {
            if let Some(target_fid) = event.target_fid {
                if target_fid != filter_target_fid {
                    debug!(
                        "Target FID filter mismatch: {:?} != {}",
                        target_fid, filter_target_fid
                    );
                    return false;
                }
            } else {
                // Hook requires target_fid but event doesn't have one
                debug!("Target FID filter required but event has no target_fid");
                return false;
            }
        }

        // Check regex filter
        if let Some(ref regex_pattern) = hook.regex_filter {
            if let Some(ref text) = event.text {
                match Regex::new(regex_pattern) {
                    Ok(regex) => {
                        if !regex.is_match(text) {
                            debug!(
                                "Regex filter mismatch: pattern '{}' not found in '{}'",
                                regex_pattern, text
                            );
                            return false;
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Invalid regex pattern '{}': {}", regex_pattern, e);
                        return false;
                    }
                }
            } else {
                // Hook requires regex match but event has no text
                debug!("Regex filter required but event has no text");
                return false;
            }
        }

        true
    }
}

impl Default for EventMatcher {
    fn default() -> Self {
        Self::new()
    }
}
