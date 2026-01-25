//! Hook command handler

use std::sync::Arc;

use crate::api::redis_client::RedisClient;
use crate::cli::output::*;
use crate::sync::hooks::EventType;
use crate::sync::hooks::FuncHookType;
use crate::sync::hooks::HookConfig;
use crate::sync::hooks::HookManager;
use crate::sync::hooks::HookQueue;
use crate::sync::hooks::OnChainEventType;
use crate::AppConfig;
use crate::Result;

/// Handle hook registration command
pub async fn handle_serve_hook(
    config: &AppConfig,
    event_type: EventType,
    url: Option<String>,
    regex: Option<String>,
    fid: Option<i64>,
    target_fid: Option<i64>,
    onchain_event_type: Option<OnChainEventType>,
    func_hook: Option<FuncHookType>,
) -> Result<()> {
    print_info("Registering Hook");
    println!("===============================\n");

    if url.is_none() && func_hook.is_none() {
        return Err(crate::SnapRagError::Custom(
            "Either --url or --func-hook must be provided".to_string(),
        ));
    }

    // Validate URL
    if let Some(ref webhook_url) = url {
        if !webhook_url.starts_with("http://") && !webhook_url.starts_with("https://") {
            return Err(crate::SnapRagError::Custom(format!(
                "Invalid webhook URL: {}. Must start with http:// or https://",
                webhook_url
            )));
        }
    }

    // Create hook configuration
    let hook_config = HookConfig::new(
        event_type,
        url.clone(),
        regex.clone(),
        fid,
        target_fid,
        onchain_event_type,
        func_hook,
    );

    println!("📌 Event Type: {:?}", hook_config.event_type);
    if let Some(oct) = hook_config.onchain_event_type {
        println!("📌 OnChain Event Type: {:?}", oct);
    }
    if let Some(ref webhook_url) = hook_config.webhook_url {
        println!("🔗 Webhook URL: {}", webhook_url);
    }
    if let Some(func_hook) = hook_config.func_hook {
        println!("🧩 Function Hook: {:?}", func_hook);
    }
    if let Some(ref r) = hook_config.regex_filter {
        println!("🔍 Regex Filter: {}", r);
    }
    if let Some(f) = hook_config.fid_filter {
        println!("👤 FID Filter: {}", f);
    }
    if let Some(tf) = hook_config.target_fid_filter {
        println!("🎯 Target FID Filter: {}", tf);
    }
    println!();

    let redis_cfg = config.redis.as_ref().ok_or_else(|| {
        crate::SnapRagError::Custom("Redis configuration is required for hooks".to_string())
    })?;
    let redis = RedisClient::connect(redis_cfg)?;

    // Create hook manager (local matching + execution)
    let hook_manager = Arc::new(HookManager::new());
    hook_manager.register_hook(hook_config.clone()).await;

    println!("✅ Hook registered successfully!");
    println!();
    println!("🚀 Starting hook consumer...");
    println!();

    let hook_queue = HookQueue::new(redis);

    // Keep running
    println!("⏳ Hook consumer running. Press Ctrl+C to stop.");
    println!();

    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => {
                println!("\n🛑 Stopping hook consumer...");
                break;
            }
            event = hook_queue.pop(std::time::Duration::from_secs(5)) => {
                match event {
                    Ok(Some(event)) => {
                        hook_manager.check_and_trigger(&event).await;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        tracing::warn!("Hook queue pop error: {}", e);
                    }
                }
            }
        }
    }

    println!("✅ Hook consumer stopped.");

    Ok(())
}
