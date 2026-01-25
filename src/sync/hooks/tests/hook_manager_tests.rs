//! Tests for hook manager

use crate::sync::hooks::config::EventType;
use crate::sync::hooks::config::HookConfig;
use crate::sync::hooks::EventData;
use crate::sync::hooks::HookManager;

#[tokio::test]
async fn test_register_hook() {
    let manager = HookManager::new();
    let hook = HookConfig::new(
        EventType::MergeMessage,
        Some("http://example.com/webhook".to_string()),
        None,
        None,
        None,
        None,
        None,
    );

    manager.register_hook(hook.clone()).await;

    let hooks = manager.get_hooks().await;
    assert_eq!(hooks.len(), 1);
    assert_eq!(hooks[0].webhook_url, hook.webhook_url);
}

#[tokio::test]
async fn test_remove_hooks() {
    let manager = HookManager::new();
    let hook1 = HookConfig::new(
        EventType::MergeMessage,
        Some("http://example.com/webhook1".to_string()),
        None,
        None,
        None,
        None,
        None,
    );
    let hook2 = HookConfig::new(
        EventType::MergeUsernameProof,
        Some("http://example.com/webhook2".to_string()),
        None,
        None,
        None,
        None,
        None,
    );

    manager.register_hook(hook1).await;
    manager.register_hook(hook2).await;

    assert_eq!(manager.get_hooks().await.len(), 2);

    manager
        .remove_hooks(Some(EventType::MergeMessage), None)
        .await;

    let registered_hooks = manager.get_hooks().await;
    assert_eq!(registered_hooks.len(), 1);
    assert_eq!(
        registered_hooks[0].event_type,
        EventType::MergeUsernameProof
    );
}

#[tokio::test]
async fn test_multiple_hooks() {
    let manager = HookManager::new();
    let hook1 = HookConfig::new(
        EventType::MergeMessage,
        Some("http://example.com/webhook1".to_string()),
        None,
        Some(123),
        None,
        None,
        None,
    );
    let hook2 = HookConfig::new(
        EventType::MergeMessage,
        Some("http://example.com/webhook2".to_string()),
        None,
        Some(456),
        None,
        None,
        None,
    );

    manager.register_hook(hook1).await;
    manager.register_hook(hook2).await;

    assert_eq!(manager.get_hooks().await.len(), 2);
}
