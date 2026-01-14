//! Hook command handler

use std::sync::Arc;

use crate::cli::output::*;
use crate::sync::hooks::EventType;
use crate::sync::hooks::HookConfig;
use crate::sync::hooks::HookManager;
use crate::sync::hooks::OnChainEventType;
use crate::AppConfig;
use crate::Result;
use crate::SnapRag;

/// Handle hook registration command
pub async fn handle_serve_hook(
    config: &AppConfig,
    event_type: EventType,
    url: String,
    regex: Option<String>,
    fid: Option<i64>,
    target_fid: Option<i64>,
    onchain_event_type: Option<OnChainEventType>,
) -> Result<()> {
    print_info("Registering HTTP Hook");
    println!("===============================\n");

    // Validate URL
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(crate::SnapRagError::Custom(format!(
            "Invalid webhook URL: {}. Must start with http:// or https://",
            url
        )));
    }

    // Create hook configuration
    let hook_config = HookConfig::new(
        event_type,
        url.clone(),
        regex.clone(),
        fid,
        target_fid,
        onchain_event_type,
    );

    println!("📌 Event Type: {:?}", hook_config.event_type);
    if let Some(oct) = hook_config.onchain_event_type {
        println!("📌 OnChain Event Type: {:?}", oct);
    }
    println!("🔗 Webhook URL: {}", hook_config.webhook_url);
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

    // Create SnapRag instance and get database
    let snaprag = SnapRag::new(config).await?;
    let database = snaprag.database().clone();

    // Create hook manager
    let hook_manager = Arc::new(HookManager::new());
    hook_manager.register_hook(hook_config.clone()).await;

    println!("✅ Hook registered successfully!");
    println!();
    println!("🚀 Starting sync with hook enabled...");
    println!();

    // Create sync service with hook manager
    let sync_service = Arc::new(
        crate::sync::service::SyncService::new_with_hooks(
            config,
            database,
            Some(hook_manager.clone()),
        )
        .await?,
    );

    // Start sync
    sync_service.start().await?;

    // Keep running
    println!("⏳ Sync running with hooks active. Press Ctrl+C to stop.");
    println!();

    // Wait for interrupt
    tokio::signal::ctrl_c().await?;
    println!("\n🛑 Stopping sync...");

    sync_service.stop(false).await?;

    println!("✅ Sync stopped.");

    Ok(())
}
