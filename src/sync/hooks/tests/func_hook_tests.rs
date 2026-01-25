//! Tests for function hooks

use crate::sync::hooks::build_func_hook;
use crate::sync::hooks::EventData;
use crate::sync::hooks::EventType;
use crate::sync::hooks::FuncHookType;

#[test]
fn test_build_log_hook() {
    let hook = build_func_hook(FuncHookType::Log);
    assert_eq!(hook.name(), "log");
}

#[test]
fn test_log_hook_handle_does_not_panic() {
    let hook = build_func_hook(FuncHookType::Log);
    let event = EventData {
        event_type: EventType::MergeMessage,
        fid: 42,
        target_fid: None,
        timestamp: 123_456_789,
        data: serde_json::json!({"message": "hello"}),
        text: Some("hello".to_string()),
    };

    hook.handle(&event);
}
