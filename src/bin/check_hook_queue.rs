//! Connect to Redis and print hook queue length + sample entries.
//!
//! Usage: cargo run --bin check_hook_queue
//!
//! Requires Redis config in your app config (e.g. config.toml). Uses the same
//! key as the hook system: `{namespace}hook_events` (e.g. `snaprag:hook_events`).

use snaprag::api::redis_client::RedisClient;
use snaprag::AppConfig;
use snaprag::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let config = AppConfig::load()?;
    let redis_cfg = config.redis.as_ref().ok_or_else(|| {
        snaprag::SnapRagError::Custom("Redis not configured (no [redis] in config)".to_string())
    })?;
    let redis = RedisClient::connect(redis_cfg)?;

    let key = format!("{}{}", redis_cfg.namespace, "hook_events");
    println!("Redis hook queue key: {key:?}");
    println!();

    let len = redis.hook_queue_len().await?;
    println!("Queue length (LLEN): {len}");

    if len > 0 {
        let sample: isize = 5;
        let items = redis.hook_queue_peek(sample).await?;
        println!("Oldest {sample} entries (next to be consumed by serve hook):");
        for (i, raw) in items.iter().enumerate() {
            let preview = if raw.len() > 200 {
                format!("{}...", &raw[..200])
            } else {
                raw.clone()
            };
            println!("  [{i}] {preview}");
        }
    } else {
        println!("Queue is empty. Run 'serve sync' so events are pushed, then 'serve hook' consumes them.");
    }

    Ok(())
}
