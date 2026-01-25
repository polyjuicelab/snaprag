//! Tests for event matcher

use crate::sync::hooks::config::EventType;
use crate::sync::hooks::config::HookConfig;
use crate::sync::hooks::config::OnChainEventType;
use crate::sync::hooks::matcher::EventMatcher;
use crate::sync::hooks::EventData;

#[test]
fn test_matcher_fid_filter() {
    let matcher = EventMatcher::new();
    let hook = HookConfig::new(
        EventType::MergeMessage,
        Some("http://example.com/webhook".to_string()),
        None,
        Some(123),
        None,
        None,
        None,
    );

    // Match: FID matches
    let event = EventData {
        event_type: EventType::MergeMessage,
        fid: 123,
        target_fid: None,
        timestamp: 1000,
        data: serde_json::json!({}),
        text: None,
    };
    assert!(matcher.matches(&hook, &event));

    // No match: FID doesn't match
    let event = EventData {
        event_type: EventType::MergeMessage,
        fid: 456,
        target_fid: None,
        timestamp: 1000,
        data: serde_json::json!({}),
        text: None,
    };
    assert!(!matcher.matches(&hook, &event));
}

#[test]
fn test_matcher_target_fid_filter() {
    let matcher = EventMatcher::new();
    let hook = HookConfig::new(
        EventType::MergeMessage,
        Some("http://example.com/webhook".to_string()),
        None,
        None,
        Some(789),
        None,
        None,
    );

    // Match: target FID matches
    let event = EventData {
        event_type: EventType::MergeMessage,
        fid: 123,
        target_fid: Some(789),
        timestamp: 1000,
        data: serde_json::json!({}),
        text: None,
    };
    assert!(matcher.matches(&hook, &event));

    // No match: target FID doesn't match
    let event = EventData {
        event_type: EventType::MergeMessage,
        fid: 123,
        target_fid: Some(999),
        timestamp: 1000,
        data: serde_json::json!({}),
        text: None,
    };
    assert!(!matcher.matches(&hook, &event));

    // No match: no target FID in event
    let event = EventData {
        event_type: EventType::MergeMessage,
        fid: 123,
        target_fid: None,
        timestamp: 1000,
        data: serde_json::json!({}),
        text: None,
    };
    assert!(!matcher.matches(&hook, &event));
}

#[test]
fn test_matcher_regex_filter() {
    let matcher = EventMatcher::new();
    let hook = HookConfig::new(
        EventType::MergeMessage,
        Some("http://example.com/webhook".to_string()),
        Some("hello".to_string()),
        None,
        None,
        None,
        None,
    );

    // Match: regex matches
    let event = EventData {
        event_type: EventType::MergeMessage,
        fid: 123,
        target_fid: None,
        timestamp: 1000,
        data: serde_json::json!({}),
        text: Some("hello world".to_string()),
    };
    assert!(matcher.matches(&hook, &event));

    // No match: regex doesn't match
    let event = EventData {
        event_type: EventType::MergeMessage,
        fid: 123,
        target_fid: None,
        timestamp: 1000,
        data: serde_json::json!({}),
        text: Some("goodbye world".to_string()),
    };
    assert!(!matcher.matches(&hook, &event));

    // No match: no text in event
    let event = EventData {
        event_type: EventType::MergeMessage,
        fid: 123,
        target_fid: None,
        timestamp: 1000,
        data: serde_json::json!({}),
        text: None,
    };
    assert!(!matcher.matches(&hook, &event));
}

#[test]
fn test_matcher_event_type_mismatch() {
    let matcher = EventMatcher::new();
    let hook = HookConfig::new(
        EventType::MergeMessage,
        Some("http://example.com/webhook".to_string()),
        None,
        None,
        None,
        None,
        None,
    );

    // No match: event type doesn't match
    let event = EventData {
        event_type: EventType::MergeUsernameProof,
        fid: 123,
        target_fid: None,
        timestamp: 1000,
        data: serde_json::json!({}),
        text: None,
    };
    assert!(!matcher.matches(&hook, &event));
}

#[test]
fn test_matcher_onchain_event_type() {
    let matcher = EventMatcher::new();
    let hook = HookConfig::new(
        EventType::OnChain,
        Some("http://example.com/webhook".to_string()),
        None,
        None,
        None,
        Some(OnChainEventType::IdRegister),
        None,
    );

    // Match: onchain event type matches
    let event = EventData {
        event_type: EventType::OnChain,
        fid: 123,
        target_fid: None,
        timestamp: 1000,
        data: serde_json::json!({
            "event_type": 3  // ID_REGISTER
        }),
        text: None,
    };
    assert!(matcher.matches(&hook, &event));

    // No match: onchain event type doesn't match
    let event = EventData {
        event_type: EventType::OnChain,
        fid: 123,
        target_fid: None,
        timestamp: 1000,
        data: serde_json::json!({
            "event_type": 1  // SIGNER
        }),
        text: None,
    };
    assert!(!matcher.matches(&hook, &event));
}

#[test]
fn test_matcher_combined_filters() {
    let matcher = EventMatcher::new();
    let hook = HookConfig::new(
        EventType::MergeMessage,
        Some("http://example.com/webhook".to_string()),
        Some("hello".to_string()),
        Some(123),
        Some(789),
        None,
        None,
    );

    // Match: all filters match
    let event = EventData {
        event_type: EventType::MergeMessage,
        fid: 123,
        target_fid: Some(789),
        timestamp: 1000,
        data: serde_json::json!({}),
        text: Some("hello world".to_string()),
    };
    assert!(matcher.matches(&hook, &event));

    // No match: FID doesn't match
    let event = EventData {
        event_type: EventType::MergeMessage,
        fid: 456,
        target_fid: Some(789),
        timestamp: 1000,
        data: serde_json::json!({}),
        text: Some("hello world".to_string()),
    };
    assert!(!matcher.matches(&hook, &event));
}
