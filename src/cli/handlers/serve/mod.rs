//! API server handlers

use std::sync::Arc;

use crate::cli::output::*;
use crate::AppConfig;
use crate::Result;
use crate::SnapRag;

mod chat;
mod hook;

pub use hook::handle_serve_hook;

pub async fn handle_serve_api(
    config: &AppConfig,
    host: String,
    port: u16,
    cors: bool,
    #[cfg(feature = "payment")] payment: bool,
    #[cfg(feature = "payment")] payment_address: Option<String>,
    #[cfg(feature = "payment")] testnet: Option<bool>,
) -> Result<()> {
    use crate::api::serve_api;

    println!("🚀 Starting SnapRAG API Server");
    println!("===============================\n");
    println!("📍 Host: {host}");
    println!("🔌 Port: {port}");
    println!("🌐 CORS: {}", if cors { "Enabled" } else { "Disabled" });

    #[cfg(feature = "payment")]
    // CLI arguments take priority over config
    let testnet_final = testnet.unwrap_or(config.x402.use_testnet);

    #[cfg(feature = "payment")]
    // CLI argument takes priority over config
    let payment_final = payment || config.x402.enabled;

    #[cfg(feature = "payment")]
    // Helper function to normalize Ethereum address
    fn normalize_address(addr: &str) -> String {
        let addr = addr.trim();
        if addr.starts_with("0x") || addr.starts_with("0X") {
            format!("0x{}", addr[2..].to_lowercase())
        } else {
            format!("0x{}", addr.to_lowercase())
        }
    }

    #[cfg(feature = "payment")]
    // Get payment address: prefer CLI argument, fall back to config
    // Read from config even if payment is disabled (for potential future use)
    let payment_address_final = if let Some(addr) = payment_address {
        let normalized = normalize_address(&addr);
        println!("🔧 Using CLI payment address (normalized): {normalized}");
        Some(normalized)
    } else if !config.x402.payment_address.is_empty() {
        let normalized = normalize_address(&config.x402.payment_address);
        // Check if payment.toml exists to show correct source
        let config_source = if std::path::Path::new("payment.toml").exists() {
            "payment.toml"
        } else {
            "config.toml"
        };
        println!("🔧 Using payment address from {config_source} (normalized): {normalized}");
        Some(normalized)
    } else {
        println!("⚠️ No payment address found in CLI or config");
        None
    };

    #[cfg(feature = "payment")]
    if payment_final {
        println!("💰 Payment: ENABLED");
        if let Some(addr) = &payment_address_final {
            println!("📍 Payment Address: {addr}");
        }
        println!(
            "🌐 Network: {}",
            if testnet_final {
                "base-sepolia (testnet)"
            } else {
                "base (mainnet)"
            }
        );
        println!("🔍 Facilitator URL: {}", config.x402.facilitator_url);
        if let Some(rpc) = &config.x402.rpc_url {
            println!("⛓️  RPC URL: {rpc}");
        }
    } else {
        println!("💰 Payment: DISABLED");
    }

    #[cfg(not(feature = "payment"))]
    println!("💡 Payment: Not compiled (use --features payment)");

    println!();

    // Start the API server
    serve_api(
        config,
        host,
        port,
        cors,
        #[cfg(feature = "payment")]
        payment_final,
        #[cfg(feature = "payment")]
        payment_address_final,
        #[cfg(feature = "payment")]
        testnet_final,
    )
    .await?;

    Ok(())
}

/// Handle MCP server command
pub async fn handle_serve_mcp(
    config: &AppConfig,
    host: String,
    port: u16,
    cors: bool,
    #[cfg(feature = "payment")] payment: bool,
    #[cfg(feature = "payment")] payment_address: Option<String>,
    #[cfg(feature = "payment")] testnet: Option<bool>,
) -> Result<()> {
    use crate::api::serve_mcp;

    println!("🚀 Starting SnapRAG MCP Server");
    println!("===============================\n");
    println!("📍 Host: {host}");
    println!("🔌 Port: {port}");
    println!("🌐 CORS: {}", if cors { "Enabled" } else { "Disabled" });

    #[cfg(feature = "payment")]
    // CLI arguments take priority over config
    let testnet_final = testnet.unwrap_or(config.x402.use_testnet);

    #[cfg(feature = "payment")]
    // CLI argument takes priority over config
    let payment_final = payment || config.x402.enabled;

    #[cfg(feature = "payment")]
    // Helper function to normalize Ethereum address
    fn normalize_address(addr: &str) -> String {
        let addr = addr.trim();
        if addr.starts_with("0x") || addr.starts_with("0X") {
            format!("0x{}", addr[2..].to_lowercase())
        } else {
            format!("0x{}", addr.to_lowercase())
        }
    }

    #[cfg(feature = "payment")]
    // Get payment address: prefer CLI argument, fall back to config
    // Read from config even if payment is disabled (for potential future use)
    let payment_address_final = if let Some(addr) = payment_address {
        let normalized = normalize_address(&addr);
        println!("🔧 Using CLI payment address (normalized): {normalized}");
        Some(normalized)
    } else if !config.x402.payment_address.is_empty() {
        let normalized = normalize_address(&config.x402.payment_address);
        // Check if payment.toml exists to show correct source
        let config_source = if std::path::Path::new("payment.toml").exists() {
            "payment.toml"
        } else {
            "config.toml"
        };
        println!("🔧 Using payment address from {config_source} (normalized): {normalized}");
        Some(normalized)
    } else {
        println!("⚠️ No payment address found in CLI or config");
        None
    };

    #[cfg(feature = "payment")]
    if payment_final {
        println!("💰 Payment: ENABLED");
        if let Some(addr) = &payment_address_final {
            println!("📍 Payment Address: {addr}");
        }
        println!(
            "🌐 Network: {}",
            if testnet_final {
                "base-sepolia (testnet)"
            } else {
                "base (mainnet)"
            }
        );
        println!("🔍 Facilitator URL: {}", config.x402.facilitator_url);
        if let Some(rpc) = &config.x402.rpc_url {
            println!("⛓️  RPC URL: {rpc}");
        }
    } else {
        println!("💰 Payment: DISABLED");
    }

    #[cfg(not(feature = "payment"))]
    println!("💡 Payment: Not compiled (use --features payment)");

    println!();

    // Start the MCP server
    serve_mcp(
        config,
        host,
        port,
        cors,
        #[cfg(feature = "payment")]
        payment_final,
        #[cfg(feature = "payment")]
        payment_address_final,
        #[cfg(feature = "payment")]
        testnet_final,
    )
    .await?;

    Ok(())
}

/// Handle worker command - process background jobs from Redis queue
pub async fn handle_serve_worker(
    config: &AppConfig,
    queue: String,
    workers: usize,
    cleanup: bool,
) -> Result<()> {
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::time::sleep;
    use tracing::debug;
    use tracing::error;
    use tracing::info;
    use tracing::warn;

    println!("🔧 Starting SnapRAG Worker");
    println!("==========================\n");

    // Parse queues (comma-separated)
    let queues: Vec<String> = queue
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if queues.is_empty() {
        return Err(crate::SnapRagError::Custom(
            "At least one queue must be specified".to_string(),
        ));
    }

    println!("📦 Queues: {}", queues.join(", "));
    println!("👷 Workers: {workers}");
    if cleanup {
        println!("🧹 Cleanup: enabled");
    }

    // Check Redis configuration
    let redis_cfg = config.redis.as_ref().ok_or_else(|| {
        crate::SnapRagError::Custom("Redis configuration is required for worker".to_string())
    })?;

    let redis = Arc::new(crate::api::redis_client::RedisClient::connect(redis_cfg)?);
    let database = Arc::new(crate::database::Database::from_config(config).await?);

    println!("✅ Redis connected");
    println!("✅ Database connected");

    // Clean up old jobs if requested
    if cleanup {
        warn!("🧹 Cleaning up old/stuck jobs...");
        let mut total_cleaned = 0;
        for queue_name in &queues {
            match redis.cleanup_old_jobs(Some(queue_name)).await {
                Ok(count) => {
                    if count > 0 {
                        warn!(
                            "✅ Cleaned up {} old/stuck job(s) from queue '{}'",
                            count, queue_name
                        );
                        total_cleaned += count;
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to cleanup old jobs from queue '{}': {}",
                        queue_name, e
                    );
                }
            }
        }
        if total_cleaned == 0 {
            warn!("✅ No old jobs to clean up");
        }
    }

    println!();

    // Spawn worker tasks
    let mut handles = Vec::new();
    for worker_id in 0..workers {
        let redis_clone = redis.clone();
        let database_clone = database.clone();
        let queues_clone = queues.clone();
        let config_clone = config.clone();

        let handle = tokio::spawn(async move {
            info!(
                "Worker {} started (monitoring {} queue(s))",
                worker_id,
                queues_clone.len()
            );
            loop {
                // Pop job from any queue using BRPOP (fair, returns first available)
                let queue_refs: Vec<&str> = queues_clone
                    .iter()
                    .map(std::string::String::as_str)
                    .collect();
                match redis_clone
                    .pop_job_from_queues(&queue_refs, Duration::from_secs(5))
                    .await
                {
                    Ok(Some((queue_name, job_id, job_data))) => {
                        // Got a job from one of the queues

                        // Process the job we got
                        let job_start_time = std::time::Instant::now();
                        info!(
                            "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
                        );
                        info!(
                            "Worker {}: 📥 RECEIVED JOB from queue '{}'",
                            worker_id, queue_name
                        );
                        info!("  Job ID: {}", job_id);
                        debug!("  Raw job data: {}", job_data);

                        // Parse job data
                        let parse_start = std::time::Instant::now();
                        let job: serde_json::Value = match serde_json::from_str(&job_data) {
                            Ok(j) => {
                                let parse_duration = parse_start.elapsed();
                                debug!("  Parsed job data in {}ms", parse_duration.as_millis());
                                j
                            }
                            Err(e) => {
                                error!(
                                    "Worker {}: ❌ Failed to parse job data: {} (took {}ms)",
                                    worker_id,
                                    e,
                                    parse_start.elapsed().as_millis()
                                );
                                continue; // Skip this job, try next iteration
                            }
                        };

                        // Extract job type
                        let job_type = job
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");

                        // Handle chat jobs separately (they don't have fid, use session_id instead)
                        if job_type == "chat" {
                            // Get session_id from job data to build job_key
                            let session_id = match job.get("session_id").and_then(|v| v.as_str()) {
                                Some(id) => id,
                                None => {
                                    error!("Worker {}: Missing session_id in chat job", worker_id);
                                    continue;
                                }
                            };

                            // Extract message_id from job_id to reconstruct job_key
                            // Job key format: chat:{session_id}:{message_id}
                            // Job ID format: job:chat:{uuid}
                            // Extract message_id from job_id (fallback to "unknown" if not found)
                            let message_id = job_id.strip_prefix("job:chat:").unwrap_or("unknown");

                            let job_key = format!("chat:{}:{}", session_id, message_id);

                            // Process chat job using the chat module
                            if let Err(e) = chat::process_chat_job(
                                worker_id,
                                &job_id,
                                &job,
                                &job_key,
                                job_start_time,
                                redis_clone.clone(),
                                database_clone.clone(),
                                &config_clone,
                            )
                            .await
                            {
                                error!("Worker {}: Chat job processing failed: {}", worker_id, e);
                            }
                            continue; // Skip the match fid block below
                        }

                        // For non-chat jobs, extract FID
                        let fid = job
                            .get("fid")
                            .and_then(serde_json::Value::as_i64)
                            .ok_or_else(|| {
                                crate::SnapRagError::Custom("Missing fid in job data".to_string())
                            });

                        match fid {
                            Ok(fid) => {
                                // Extract year for annual_report jobs
                                let year = if job_type == "annual_report" {
                                    job.get("year")
                                        .and_then(serde_json::Value::as_u64)
                                        .map(|y| y as u32)
                                } else {
                                    None
                                };

                                // Construct job key from job type, FID, and optionally year
                                let job_key = if job_type == "annual_report" {
                                    if let Some(y) = year {
                                        format!("{job_type}:{fid}:{y}")
                                    } else {
                                        error!(
                                            "Worker {}: ❌ Missing year for annual_report job (FID: {})",
                                            worker_id, fid
                                        );
                                        continue; // Skip this job
                                    }
                                } else {
                                    format!("{job_type}:{fid}")
                                };

                                // Log job details clearly
                                info!("  Job Type: {}", job_type);
                                info!("  FID: {}", fid);
                                if let Some(y) = year {
                                    info!("  Year: {}", y);
                                }
                                info!("  Job Key: {}", job_key);
                                info!("  Started at: {:?}", job_start_time);

                                // Check if job is already failed or cancelled before processing
                                if let Ok(Some((status, _))) =
                                    redis_clone.get_job_status(&job_key).await
                                {
                                    if status == "failed" || status == "cancelled" {
                                        warn!(
                                            "Worker {}: ⚠️  SKIPPING job {} - Status: {} (job already completed/failed)",
                                            worker_id, job_key, status
                                        );
                                        // Mark as inactive and continue to next job
                                        let _ = redis_clone.mark_job_inactive(&job_key).await;
                                        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                                        continue;
                                    }
                                    info!("  Current Status: {} (updating to processing)", status);
                                } else {
                                    info!("  Current Status: pending (updating to processing)");
                                }

                                // Update status to processing
                                info!("  → Updating status to 'processing'...");
                                let status_update_start = std::time::Instant::now();
                                if let Err(e) = redis_clone
                                    .set_job_status(&job_key, "processing", None)
                                    .await
                                {
                                    warn!(
                                        "Worker {}: Failed to update job status: {} (took {}ms)",
                                        worker_id,
                                        e,
                                        status_update_start.elapsed().as_millis()
                                    );
                                } else {
                                    debug!(
                                        "Worker {}: Updated job status to processing in {}ms",
                                        worker_id,
                                        status_update_start.elapsed().as_millis()
                                    );
                                }

                                // Process job based on type
                                let analysis_start = std::time::Instant::now();
                                match job_type {
                                    "social" => {
                                        info!(
                                            "Worker {}: 🔍 Processing SOCIAL graph analysis for FID {}",
                                            worker_id, fid
                                        );
                                        let analyzer =
                                            crate::social_graph::SocialGraphAnalyzer::new(
                                                database_clone.clone(),
                                            );
                                        let result = analyzer.analyze_user(fid).await;
                                        let analysis_duration = analysis_start.elapsed();
                                        match &result {
                                            Ok(_) => {
                                                info!(
                                                    "Worker {}: Social graph analysis completed for FID {} in {}ms ({}s)",
                                                    worker_id, fid, analysis_duration.as_millis(), analysis_duration.as_secs()
                                                );
                                            }
                                            Err(e) => {
                                                error!(
                                                    "Worker {}: Social graph analysis failed for FID {} after {}ms ({}s): {}",
                                                    worker_id, fid, analysis_duration.as_millis(), analysis_duration.as_secs(), e
                                                );
                                            }
                                        }

                                        match result {
                                            Ok(social_profile) => {
                                                // Cache the result
                                                let cache_start = std::time::Instant::now();
                                                let cache_service =
                                                    crate::api::cache::CacheService::with_config(
                                                        redis_clone.clone(),
                                                        crate::api::cache::CacheConfig::default(),
                                                    );
                                                if let Err(e) = cache_service
                                                    .set_social(fid, &social_profile)
                                                    .await
                                                {
                                                    error!(
                                                "Worker {}: Failed to cache result: {} (took {}ms)",
                                                worker_id,
                                                e,
                                                cache_start.elapsed().as_millis()
                                            );
                                                } else {
                                                    let cache_duration = cache_start.elapsed();
                                                    debug!(
                                                        "Worker {}: Cached result in {}ms",
                                                        worker_id,
                                                        cache_duration.as_millis()
                                                    );
                                                }

                                                // Update job status to completed
                                                let status_complete_start =
                                                    std::time::Instant::now();
                                                let result_json =
                                                    serde_json::to_string(&social_profile)
                                                        .unwrap_or_else(|_| "{}".to_string());
                                                if let Err(e) = redis_clone
                                                    .set_job_status(
                                                        &job_key,
                                                        "completed",
                                                        Some(&result_json),
                                                    )
                                                    .await
                                                {
                                                    warn!(
                                                "Worker {}: Failed to update job status: {} (took {}ms)",
                                                worker_id, e, status_complete_start.elapsed().as_millis()
                                            );
                                                } else {
                                                    debug!("Worker {}: Updated job status to completed in {}ms", worker_id, status_complete_start.elapsed().as_millis());
                                                }

                                                // Mark job as inactive
                                                let inactive_start = std::time::Instant::now();
                                                if let Err(e) =
                                                    redis_clone.mark_job_inactive(&job_key).await
                                                {
                                                    warn!(
                                                "Worker {}: Failed to mark job inactive: {} (took {}ms)",
                                                worker_id, e, inactive_start.elapsed().as_millis()
                                            );
                                                } else {
                                                    debug!(
                                                        "Worker {}: Marked job inactive in {}ms",
                                                        worker_id,
                                                        inactive_start.elapsed().as_millis()
                                                    );
                                                }

                                                let total_duration = job_start_time.elapsed();
                                                info!(
                                            "Worker {}: ✅ Completed job {} for FID {} - Total time: {}ms ({}s) | Analysis: {}ms ({}s) | Cache: {}ms | Status updates: {}ms",
                                            worker_id,
                                            job_id,
                                            fid,
                                            total_duration.as_millis(),
                                            total_duration.as_secs(),
                                            analysis_start.elapsed().as_millis(),
                                            analysis_start.elapsed().as_secs(),
                                            cache_start.elapsed().as_millis(),
                                            status_update_start.elapsed().as_millis() + status_complete_start.elapsed().as_millis() + inactive_start.elapsed().as_millis()
                                        );
                                            }
                                            Err(e) => {
                                                error!(
                                                    "Worker {}: Job {} failed: {}",
                                                    worker_id, job_id, e
                                                );

                                                // Update job status to failed
                                                if let Err(update_err) = redis_clone
                                                    .set_job_status(
                                                        &job_key,
                                                        "failed",
                                                        Some(&format!("{e}")),
                                                    )
                                                    .await
                                                {
                                                    warn!(
                                                "Worker {}: Failed to update job status: {}",
                                                worker_id, update_err
                                            );
                                                }

                                                // Mark job as inactive
                                                if let Err(e) =
                                                    redis_clone.mark_job_inactive(&job_key).await
                                                {
                                                    warn!(
                                                "Worker {}: Failed to mark job inactive: {}",
                                                worker_id, e
                                            );
                                                }
                                            }
                                        }
                                    }
                                    "mbti" => {
                                        info!(
                                            "Worker {}: 🔍 Processing MBTI analysis for FID {}",
                                            worker_id, fid
                                        );

                                        // Get config from state (we need to pass it through or reconstruct)
                                        // For now, we'll use default config - this should be improved to pass config through
                                        let method = crate::config::MbtiMethod::RuleBased; // Default, should get from config

                                        // Try to get social profile from cache if needed
                                        let cache_service =
                                            crate::api::cache::CacheService::with_config(
                                                redis_clone.clone(),
                                                crate::api::cache::CacheConfig::default(),
                                            );
                                        let social_profile = if matches!(
                                            method,
                                            crate::config::MbtiMethod::RuleBased
                                                | crate::config::MbtiMethod::Ensemble
                                        ) {
                                            match cache_service.get_social(fid).await {
                                                Ok(
                                                    crate::api::cache::CacheResult::Fresh(s)
                                                    | crate::api::cache::CacheResult::Stale(s),
                                                ) => Some(s),
                                                Ok(crate::api::cache::CacheResult::Updating(s)) => {
                                                    Some(s)
                                                }
                                                Ok(crate::api::cache::CacheResult::Miss)
                                                | Err(_) => None,
                                            }
                                        } else {
                                            None
                                        };

                                        // Analyze MBTI
                                        let analyzer = crate::personality::MbtiAnalyzer::new(
                                            database_clone.clone(),
                                        );
                                        let result = analyzer
                                            .analyze_mbti(fid, social_profile.as_ref())
                                            .await;
                                        let analysis_duration = analysis_start.elapsed();
                                        match &result {
                                            Ok(profile) => {
                                                info!(
                                                    "Worker {}: MBTI analysis completed for FID {} in {}ms ({}s) - type: {}, confidence: {:.2}",
                                                    worker_id, fid, analysis_duration.as_millis(), analysis_duration.as_secs(),
                                                    profile.mbti_type, profile.confidence
                                                );
                                            }
                                            Err(e) => {
                                                error!(
                                                    "Worker {}: MBTI analysis failed for FID {} after {}ms ({}s): {}",
                                                    worker_id, fid, analysis_duration.as_millis(), analysis_duration.as_secs(), e
                                                );
                                            }
                                        }

                                        match result {
                                            Ok(mbti_profile) => {
                                                // Cache the result
                                                let cache_start = std::time::Instant::now();
                                                let cache_service =
                                                    crate::api::cache::CacheService::with_config(
                                                        redis_clone.clone(),
                                                        crate::api::cache::CacheConfig::default(),
                                                    );
                                                if let Err(e) =
                                                    cache_service.set_mbti(fid, &mbti_profile).await
                                                {
                                                    error!(
                                                "Worker {}: Failed to cache result: {} (took {}ms)",
                                                worker_id,
                                                e,
                                                cache_start.elapsed().as_millis()
                                            );
                                                } else {
                                                    let cache_duration = cache_start.elapsed();
                                                    debug!(
                                                        "Worker {}: Cached result in {}ms",
                                                        worker_id,
                                                        cache_duration.as_millis()
                                                    );
                                                }

                                                // Update job status to completed
                                                let status_complete_start =
                                                    std::time::Instant::now();
                                                let result_json =
                                                    serde_json::to_string(&mbti_profile)
                                                        .unwrap_or_else(|_| "{}".to_string());
                                                if let Err(e) = redis_clone
                                                    .set_job_status(
                                                        &job_key,
                                                        "completed",
                                                        Some(&result_json),
                                                    )
                                                    .await
                                                {
                                                    warn!(
                                                "Worker {}: Failed to update job status: {} (took {}ms)",
                                                worker_id, e, status_complete_start.elapsed().as_millis()
                                            );
                                                } else {
                                                    debug!("Worker {}: Updated job status to completed in {}ms", worker_id, status_complete_start.elapsed().as_millis());
                                                }

                                                // Mark job as inactive
                                                let inactive_start = std::time::Instant::now();
                                                if let Err(e) =
                                                    redis_clone.mark_job_inactive(&job_key).await
                                                {
                                                    warn!(
                                                "Worker {}: Failed to mark job inactive: {} (took {}ms)",
                                                worker_id, e, inactive_start.elapsed().as_millis()
                                            );
                                                } else {
                                                    debug!(
                                                        "Worker {}: Marked job inactive in {}ms",
                                                        worker_id,
                                                        inactive_start.elapsed().as_millis()
                                                    );
                                                }

                                                let total_duration = job_start_time.elapsed();
                                                info!(
                                            "Worker {}: ✅ Completed job {} for FID {} - Total time: {}ms ({}s) | Analysis: {}ms ({}s) | Cache: {}ms | Status updates: {}ms",
                                            worker_id,
                                            job_id,
                                            fid,
                                            total_duration.as_millis(),
                                            total_duration.as_secs(),
                                            analysis_start.elapsed().as_millis(),
                                            analysis_start.elapsed().as_secs(),
                                            cache_start.elapsed().as_millis(),
                                            status_update_start.elapsed().as_millis() + status_complete_start.elapsed().as_millis() + inactive_start.elapsed().as_millis()
                                        );
                                            }
                                            Err(e) => {
                                                error!(
                                                    "Worker {}: Job {} failed: {}",
                                                    worker_id, job_id, e
                                                );

                                                // Update job status to failed
                                                if let Err(update_err) = redis_clone
                                                    .set_job_status(
                                                        &job_key,
                                                        "failed",
                                                        Some(&format!("{e}")),
                                                    )
                                                    .await
                                                {
                                                    warn!(
                                                "Worker {}: Failed to update job status: {}",
                                                worker_id, update_err
                                            );
                                                }

                                                // Mark job as inactive
                                                if let Err(e) =
                                                    redis_clone.mark_job_inactive(&job_key).await
                                                {
                                                    warn!(
                                                "Worker {}: Failed to mark job inactive: {}",
                                                worker_id, e
                                            );
                                                }
                                            }
                                        }
                                    }
                                    "cast_stats" => {
                                        info!(
                                            "Worker {}: 🔍 Processing CAST STATISTICS for FID {}",
                                            worker_id, fid
                                        );

                                        // Query casts from database (no time range filter for cached stats)
                                        let result = database_clone
                                            .get_casts_by_fid_and_time_range(fid, None, None)
                                            .await;
                                        let analysis_duration = analysis_start.elapsed();

                                        match result {
                                            Ok(casts) => {
                                                if casts.is_empty() {
                                                    error!(
                                                        "Worker {}: No casts found for FID {}",
                                                        worker_id, fid
                                                    );
                                                    // Update job status to failed
                                                    if let Err(update_err) = redis_clone
                                                        .set_job_status(
                                                            &job_key,
                                                            "failed",
                                                            Some("No casts found"),
                                                        )
                                                        .await
                                                    {
                                                        warn!(
                                                            "Worker {}: Failed to update job status: {}",
                                                            worker_id, update_err
                                                        );
                                                    }
                                                    if let Err(e) = redis_clone
                                                        .mark_job_inactive(&job_key)
                                                        .await
                                                    {
                                                        warn!(
                                                            "Worker {}: Failed to mark job inactive: {}",
                                                            worker_id, e
                                                        );
                                                    }
                                                    continue;
                                                }

                                                info!(
                                                    "Worker {}: Found {} casts for FID {} in {}ms",
                                                    worker_id,
                                                    casts.len(),
                                                    fid,
                                                    analysis_duration.as_millis()
                                                );

                                                // Calculate statistics (same logic as handler)
                                                let stats = crate::api::handlers::cast_stats::compute_cast_statistics(&casts);
                                                let stats_duration = analysis_start.elapsed();

                                                info!(
                                                    "Worker {}: Cast statistics computed for FID {} in {}ms ({}s)",
                                                    worker_id, fid, stats_duration.as_millis(), stats_duration.as_secs()
                                                );

                                                // Cache the result
                                                let cache_start = std::time::Instant::now();
                                                let cache_service =
                                                    crate::api::cache::CacheService::with_config(
                                                        redis_clone.clone(),
                                                        crate::api::cache::CacheConfig::default(),
                                                    );
                                                if let Err(e) =
                                                    cache_service.set_cast_stats(fid, &stats).await
                                                {
                                                    error!(
                                                        "Worker {}: Failed to cache result: {} (took {}ms)",
                                                        worker_id,
                                                        e,
                                                        cache_start.elapsed().as_millis()
                                                    );
                                                } else {
                                                    let cache_duration = cache_start.elapsed();
                                                    debug!(
                                                        "Worker {}: Cached result in {}ms",
                                                        worker_id,
                                                        cache_duration.as_millis()
                                                    );
                                                }

                                                // Update job status to completed
                                                let status_complete_start =
                                                    std::time::Instant::now();
                                                let result_json = serde_json::to_string(&stats)
                                                    .unwrap_or_else(|_| "{}".to_string());
                                                if let Err(e) = redis_clone
                                                    .set_job_status(
                                                        &job_key,
                                                        "completed",
                                                        Some(&result_json),
                                                    )
                                                    .await
                                                {
                                                    warn!(
                                                        "Worker {}: Failed to update job status: {} (took {}ms)",
                                                        worker_id, e, status_complete_start.elapsed().as_millis()
                                                    );
                                                } else {
                                                    debug!("Worker {}: Updated job status to completed in {}ms", worker_id, status_complete_start.elapsed().as_millis());
                                                }

                                                // Mark job as inactive
                                                let inactive_start = std::time::Instant::now();
                                                if let Err(e) =
                                                    redis_clone.mark_job_inactive(&job_key).await
                                                {
                                                    warn!(
                                                        "Worker {}: Failed to mark job inactive: {} (took {}ms)",
                                                        worker_id, e, inactive_start.elapsed().as_millis()
                                                    );
                                                } else {
                                                    debug!(
                                                        "Worker {}: Marked job inactive in {}ms",
                                                        worker_id,
                                                        inactive_start.elapsed().as_millis()
                                                    );
                                                }

                                                let total_duration = job_start_time.elapsed();
                                                info!(
                                                    "Worker {}: ✅ Completed cast stats job {} for FID {} - Total time: {}ms ({}s) | Analysis: {}ms ({}s) | Cache: {}ms | Status updates: {}ms",
                                                    worker_id,
                                                    job_id,
                                                    fid,
                                                    total_duration.as_millis(),
                                                    total_duration.as_secs(),
                                                    analysis_start.elapsed().as_millis(),
                                                    analysis_start.elapsed().as_secs(),
                                                    cache_start.elapsed().as_millis(),
                                                    status_update_start.elapsed().as_millis() + status_complete_start.elapsed().as_millis() + inactive_start.elapsed().as_millis()
                                                );
                                            }
                                            Err(e) => {
                                                error!(
                                                    "Worker {}: Failed to fetch casts for FID {}: {}",
                                                    worker_id, fid, e
                                                );

                                                // Update job status to failed
                                                if let Err(update_err) = redis_clone
                                                    .set_job_status(
                                                        &job_key,
                                                        "failed",
                                                        Some(&format!("{e}")),
                                                    )
                                                    .await
                                                {
                                                    warn!(
                                                        "Worker {}: Failed to update job status: {}",
                                                        worker_id, update_err
                                                    );
                                                }

                                                // Mark job as inactive
                                                if let Err(e) =
                                                    redis_clone.mark_job_inactive(&job_key).await
                                                {
                                                    warn!(
                                                        "Worker {}: Failed to mark job inactive: {}",
                                                        worker_id, e
                                                    );
                                                }
                                            }
                                        }
                                    }
                                    "annual_report" => {
                                        let year = year.ok_or_else(|| {
                                            crate::SnapRagError::Custom(
                                                "Missing year for annual_report job".to_string(),
                                            )
                                        });

                                        match year {
                                            Ok(year) => {
                                                info!(
                                                    "Worker {}: 🔍 Processing ANNUAL REPORT for FID {} year {}",
                                                    worker_id, fid, year
                                                );

                                                // Create cache service
                                                let cache_service =
                                                    crate::api::cache::CacheService::with_config(
                                                        redis_clone.clone(),
                                                        crate::api::cache::CacheConfig::default(),
                                                    );

                                                // Generate annual report (force=false to respect cache)
                                                let result = crate::cli::handlers::annual_report::generate_annual_report(
                                                    &database_clone,
                                                    Some(&cache_service),
                                                    fid,
                                                    year,
                                                    false, // Don't force, respect cache
                                                )
                                                .await;
                                                let analysis_duration = analysis_start.elapsed();

                                                match &result {
                                                    Ok(_) => {
                                                        info!(
                                                            "Worker {}: Annual report generation completed for FID {} year {} in {}ms ({}s)",
                                                            worker_id, fid, year, analysis_duration.as_millis(), analysis_duration.as_secs()
                                                        );
                                                    }
                                                    Err(e) => {
                                                        error!(
                                                            "Worker {}: Annual report generation failed for FID {} year {} after {}ms ({}s): {}",
                                                            worker_id, fid, year, analysis_duration.as_millis(), analysis_duration.as_secs(), e
                                                        );
                                                    }
                                                }

                                                match result {
                                                    Ok(report) => {
                                                        // Cache is already updated by generate_annual_report
                                                        // Just update job status to completed
                                                        let status_complete_start =
                                                            std::time::Instant::now();
                                                        let result_json =
                                                            serde_json::to_string(&report)
                                                                .unwrap_or_else(|_| {
                                                                    "{}".to_string()
                                                                });
                                                        if let Err(e) = redis_clone
                                                            .set_job_status(
                                                                &job_key,
                                                                "completed",
                                                                Some(&result_json),
                                                            )
                                                            .await
                                                        {
                                                            warn!(
                                                                "Worker {}: Failed to update job status: {} (took {}ms)",
                                                                worker_id, e, status_complete_start.elapsed().as_millis()
                                                            );
                                                        } else {
                                                            debug!("Worker {}: Updated job status to completed in {}ms", worker_id, status_complete_start.elapsed().as_millis());
                                                        }

                                                        // Mark job as inactive
                                                        let inactive_start =
                                                            std::time::Instant::now();
                                                        if let Err(e) = redis_clone
                                                            .mark_job_inactive(&job_key)
                                                            .await
                                                        {
                                                            warn!(
                                                                "Worker {}: Failed to mark job inactive: {} (took {}ms)",
                                                                worker_id, e, inactive_start.elapsed().as_millis()
                                                            );
                                                        } else {
                                                            debug!(
                                                                "Worker {}: Marked job inactive in {}ms",
                                                                worker_id,
                                                                inactive_start.elapsed().as_millis()
                                                            );
                                                        }

                                                        let total_duration =
                                                            job_start_time.elapsed();
                                                        info!(
                                                            "Worker {}: ✅ Completed annual report job {} for FID {} year {} - Total time: {}ms ({}s) | Generation: {}ms ({}s) | Status updates: {}ms",
                                                            worker_id,
                                                            job_id,
                                                            fid,
                                                            year,
                                                            total_duration.as_millis(),
                                                            total_duration.as_secs(),
                                                            analysis_start.elapsed().as_millis(),
                                                            analysis_start.elapsed().as_secs(),
                                                            status_update_start.elapsed().as_millis() + status_complete_start.elapsed().as_millis() + inactive_start.elapsed().as_millis()
                                                        );
                                                    }
                                                    Err(e) => {
                                                        error!(
                                                            "Worker {}: Job {} failed: {}",
                                                            worker_id, job_id, e
                                                        );

                                                        // Update job status to failed
                                                        if let Err(update_err) = redis_clone
                                                            .set_job_status(
                                                                &job_key,
                                                                "failed",
                                                                Some(&format!("{e}")),
                                                            )
                                                            .await
                                                        {
                                                            warn!(
                                                                "Worker {}: Failed to update job status: {}",
                                                                worker_id, update_err
                                                            );
                                                        }

                                                        // Mark job as inactive
                                                        if let Err(e) = redis_clone
                                                            .mark_job_inactive(&job_key)
                                                            .await
                                                        {
                                                            warn!(
                                                                "Worker {}: Failed to mark job inactive: {}",
                                                                worker_id, e
                                                            );
                                                        }
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                error!(
                                                    "Worker {}: Invalid annual_report job data: {}",
                                                    worker_id, e
                                                );

                                                // Update job status to failed
                                                if let Err(update_err) = redis_clone
                                                    .set_job_status(
                                                        &job_key,
                                                        "failed",
                                                        Some(&format!("{e}")),
                                                    )
                                                    .await
                                                {
                                                    warn!(
                                                        "Worker {}: Failed to update job status: {}",
                                                        worker_id, update_err
                                                    );
                                                }

                                                // Mark job as inactive
                                                if let Err(e) =
                                                    redis_clone.mark_job_inactive(&job_key).await
                                                {
                                                    warn!(
                                                        "Worker {}: Failed to mark job inactive: {}",
                                                        worker_id, e
                                                    );
                                                }
                                            }
                                        }
                                    }
                                    // Note: "chat" jobs are handled separately above (before match fid)
                                    // because they don't have a fid field
                                    _ => {
                                        error!(
                                            "Worker {}: Unknown job type: {}",
                                            worker_id, job_type
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Worker {}: Invalid job data: {}", worker_id, e);
                            }
                        }
                    }
                    Ok(None) => {
                        // No job available in any queue (timeout)
                        debug!(
                            "Worker {}: No job available in any queue (timeout), continuing...",
                            worker_id
                        );
                    }
                    Err(e) => {
                        error!("Worker {}: Error popping job from queues: {}", worker_id, e);
                        sleep(Duration::from_secs(1)).await; // Wait before retrying
                    }
                }
            }
        });

        handles.push(handle);
    }

    println!("✅ {workers} worker(s) started");
    println!("⏳ Waiting for jobs...\n");

    // Wait for all workers (they run forever)
    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

/// Handle worker status command - show active jobs being processed
pub async fn handle_worker_status(
    config: &AppConfig,
    queue: Option<String>,
    job: Option<String>,
) -> Result<()> {
    use tracing::warn;

    println!("📊 Worker Status");
    println!("================\n");

    // Check Redis configuration
    let redis_cfg = config.redis.as_ref().ok_or_else(|| {
        crate::SnapRagError::Custom("Redis configuration is required for worker status".to_string())
    })?;

    let redis = crate::api::redis_client::RedisClient::connect(redis_cfg)?;

    println!("✅ Redis connected");

    // If --job is specified, show detailed info for that specific job
    if let Some(job_key) = &job {
        return handle_job_details(config, job_key, &redis).await;
    }

    // Check database for long-running queries
    let database = crate::database::Database::from_config(config).await;
    if let Ok(db) = database {
        println!("✅ Database connected\n");

        // Check for long-running queries (30 seconds threshold)
        match db.get_long_running_queries(30).await {
            Ok(stuck_queries) => {
                if stuck_queries.is_empty() {
                    println!("✅ No long-running database queries found\n");
                } else {
                    println!(
                        "⚠️  Long-running Database Queries ({}):\n",
                        stuck_queries.len()
                    );
                    for (pid, duration_secs, state, query, app_name, client_addr) in stuck_queries {
                        let hours = duration_secs / 3600;
                        let minutes = (duration_secs % 3600) / 60;
                        let seconds = duration_secs % 60;

                        let duration_str = if hours > 0 {
                            format!("{hours}h {minutes}m {seconds}s")
                        } else if minutes > 0 {
                            format!("{minutes}m {seconds}s")
                        } else {
                            format!("{seconds}s")
                        };

                        println!("   🔴 PID {pid} - Running for {duration_str} ({state})");
                        if let Some(app) = app_name {
                            println!("      Application: {app}");
                        }
                        if let Some(addr) = client_addr {
                            println!("      Client: {addr}");
                        }
                        println!("      Query: {query}");
                        println!();
                    }
                }
            }
            Err(e) => {
                warn!("⚠️  Failed to check database queries: {}\n", e);
            }
        }
    } else {
        println!("⚠️  Database connection failed (skipping query check)\n");
    }

    // Get active jobs
    let active_jobs = redis.get_active_jobs_status(queue.as_deref()).await?;

    if active_jobs.is_empty() {
        if let Some(q) = &queue {
            println!("ℹ️  No active jobs found in queue '{q}'");
        } else {
            println!("ℹ️  No active jobs found");
        }
        println!();
        println!("💡 Tip: Use --queue <queue_name> to filter by specific queue");
        return Ok(());
    }

    println!("🔍 Found {} active job(s):\n", active_jobs.len());

    // Group by status
    let mut processing = Vec::new();
    let mut completed = Vec::new();
    let mut failed = Vec::new();
    let mut pending = Vec::new();
    let mut unknown = Vec::new();

    for (job_key, status, result, processing_duration) in active_jobs {
        match status.as_str() {
            "processing" => processing.push((job_key, result, processing_duration)),
            "completed" => completed.push((job_key, result)),
            "failed" => failed.push((job_key, result)),
            "pending" => pending.push((job_key, result)),
            _ => unknown.push((job_key, status, result)),
        }
    }

    // Display processing jobs
    if !processing.is_empty() {
        println!("⏳ Processing ({}):", processing.len());
        for (job_key, _result, duration) in processing {
            let (job_type, fid) = job_key.split_once(':').unwrap_or(("unknown", "unknown"));
            if let Some(dur) = duration {
                let hours = dur / 3600;
                let minutes = (dur % 3600) / 60;
                let seconds = dur % 60;
                if hours > 0 {
                    println!(
                        "   • {job_type}:{fid} - Processing for {hours}h {minutes}m {seconds}s"
                    );
                } else if minutes > 0 {
                    println!("   • {job_type}:{fid} - Processing for {minutes}m {seconds}s");
                } else {
                    println!("   • {job_type}:{fid} - Processing for {seconds}s");
                }
            } else {
                println!("   • {job_type}:{fid} - Processing");
            }
        }
        println!();
    }

    // Display pending jobs
    if !pending.is_empty() {
        println!("📋 Pending ({}):", pending.len());
        for (job_key, _result) in pending {
            let (job_type, fid) = job_key.split_once(':').unwrap_or(("unknown", "unknown"));
            println!("   • {job_type}:{fid}");
        }
        println!();
        println!("   ⚠️  Note: Pending jobs are waiting for a worker to process them.");
        println!("      Make sure you have a worker running with:");
        if let Some(q) = &queue {
            println!("      snaprag serve worker --queue {q}");
        } else {
            println!("      snaprag serve worker --queue <queue_name>");
            println!("      (Current pending jobs are in different queues)");
        }
        println!();
    }

    // Display completed jobs
    if !completed.is_empty() {
        println!("✅ Completed ({}):", completed.len());
        for (job_key, _result) in completed {
            let (job_type, fid) = job_key.split_once(':').unwrap_or(("unknown", "unknown"));
            println!("   • {job_type}:{fid}");
        }
        println!();
    }

    // Display failed jobs
    if !failed.is_empty() {
        println!("❌ Failed ({}):", failed.len());
        for (job_key, result) in failed {
            let (job_type, fid) = job_key.split_once(':').unwrap_or(("unknown", "unknown"));
            if let Some(err) = result {
                // Truncate error message if too long
                let error_msg = if err.len() > 100 {
                    format!("{}...", &err[..100])
                } else {
                    err
                };
                println!("   • {job_type}:{fid} - {error_msg}");
            } else {
                println!("   • {job_type}:{fid}");
            }
        }
        println!();
    }

    // Display unknown status jobs
    if !unknown.is_empty() {
        warn!("⚠️  Unknown status ({}):", unknown.len());
        for (job_key, status, _result) in unknown {
            let (job_type, fid) = job_key.split_once(':').unwrap_or(("unknown", "unknown"));
            println!("   • {job_type}:{fid} - Status: {status}");
        }
        println!();
    }

    // Show queue statistics if no filter
    if queue.is_none() {
        println!("💡 Tip: Use --queue <queue_name> to filter by specific queue");
        println!("      Use --job <job_key> to see detailed information for a specific job");
    }

    Ok(())
}

/// Handle detailed job information request
async fn handle_job_details(
    config: &AppConfig,
    job_key: &str,
    redis: &crate::api::redis_client::RedisClient,
) -> Result<()> {
    use tracing::warn;

    println!("🔍 Job Details: {job_key}");
    println!("================{}\n", "=".repeat(job_key.len()));

    // Parse job key (format: type:fid)
    let (job_type, fid_str) = job_key.split_once(':').ok_or_else(|| {
        crate::SnapRagError::Custom(format!(
            "Invalid job key format: {job_key}. Expected format: type:fid (e.g., social:66)"
        ))
    })?;

    let fid: i64 = fid_str
        .parse()
        .map_err(|_| crate::SnapRagError::Custom(format!("Invalid FID: {fid_str}")))?;

    println!("   Type: {job_type}");
    println!("   FID: {fid}\n");

    // Get job status from Redis
    match redis.get_job_status(job_key).await {
        Ok(Some((status, result))) => {
            println!("📊 Redis Status:");
            println!("   Status: {status}");

            if let Some(result_data) = result {
                if status == "completed" {
                    // Try to parse as JSON and show summary
                    if serde_json::from_str::<serde_json::Value>(&result_data).is_ok() {
                        println!("   Result: Available ({} bytes)", result_data.len());
                        // Don't print full result as it might be huge
                    } else {
                        println!(
                            "   Result: {}",
                            if result_data.len() > 200 {
                                format!("{}...", &result_data[..200])
                            } else {
                                result_data
                            }
                        );
                    }
                } else if status == "failed" {
                    println!(
                        "   Error: {}",
                        if result_data.len() > 500 {
                            format!("{}...", &result_data[..500])
                        } else {
                            result_data
                        }
                    );
                } else {
                    println!("   Data: {} bytes", result_data.len());
                }
            }
            println!();
        }
        Ok(None) => {
            println!("⚠️  No status found in Redis for job: {job_key}\n");
        }
        Err(e) => {
            warn!("Failed to get job status from Redis: {}\n", e);
        }
    }

    // Check database for queries related to this FID
    let database_result = crate::database::Database::from_config(config).await;
    if let Ok(ref db) = database_result {
        println!("🔍 Database Activity:");

        // Check for long-running queries (1 second threshold for more details)
        match db.get_long_running_queries(1).await {
            Ok(stuck_queries) => {
                // Filter queries that might be related to this job
                let relevant_queries: Vec<_> = stuck_queries
                    .into_iter()
                    .filter(|(_, _, _, query, _, _)| {
                        // Check if query mentions the FID or related tables
                        let query_lower = query.to_lowercase();
                        query_lower.contains(&fid.to_string())
                            || (job_type == "social"
                                && (query_lower.contains("links")
                                    || query_lower.contains("casts")
                                    || query_lower.contains("reactions")
                                    || query_lower.contains("follow")))
                            || (job_type == "mbti"
                                && (query_lower.contains("casts")
                                    || query_lower.contains("profile")))
                    })
                    .collect();

                if relevant_queries.is_empty() {
                    // Show all long-running queries anyway to help debug
                    if let Ok(all_queries) = db.get_long_running_queries(30).await {
                        if all_queries.is_empty() {
                            println!("   ✅ No long-running database queries found");
                            println!("   💡 This suggests the job might be:");
                            println!("      - Waiting on external API calls (Snapchain)");
                            println!("      - Processing large datasets in memory");
                            println!("      - Stuck in a loop or deadlock");
                            println!();
                        } else {
                            println!("   ⚠️  No direct matches found, but there are {} long-running queries:", all_queries.len());
                            println!("      (This job might be waiting on network/API calls, not database)\n");
                            for (pid, duration_secs, state, query, _, _) in
                                all_queries.into_iter().take(5)
                            {
                                println!(
                                    "      PID {} - {}s - {}: {}...",
                                    pid,
                                    duration_secs,
                                    state,
                                    if query.len() > 100 {
                                        &query[..100]
                                    } else {
                                        &query
                                    }
                                );
                            }
                            println!();
                        }
                    }
                } else {
                    println!(
                        "   Found {} potentially related query/queries:\n",
                        relevant_queries.len()
                    );
                    for (pid, duration_secs, state, query, app_name, client_addr) in
                        relevant_queries
                    {
                        let hours = duration_secs / 3600;
                        let minutes = (duration_secs % 3600) / 60;
                        let seconds = duration_secs % 60;

                        let duration_str = if hours > 0 {
                            format!("{hours}h {minutes}m {seconds}s")
                        } else if minutes > 0 {
                            format!("{minutes}m {seconds}s")
                        } else {
                            format!("{seconds}s")
                        };

                        println!("   🔴 PID {pid} - Running for {duration_str} ({state})");
                        if let Some(app) = app_name {
                            println!("      Application: {app}");
                        }
                        if let Some(addr) = client_addr {
                            println!("      Client: {addr}");
                        }
                        println!("      Query: {query}");
                        println!();
                    }
                }
            }
            Err(e) => {
                warn!("Failed to check database queries: {}\n", e);
            }
        }

        // Additional info: check if there's data in database for this FID
        if job_type == "social" {
            println!("📊 Database Data Check:");
            let pool = db.pool();

            // Check if worker has Snapchain client (for lazy loading)
            println!("   Worker Configuration:");
            println!("      ⚠️  Worker does NOT have Snapchain client");
            println!("      ⚠️  This means lazy loading is NOT available");
            println!("      ⚠️  Worker will only use existing database data");
            println!();

            // Check what analyze_user might be doing
            println!("   🔍 Analysis Steps (what might be running):");
            println!("      1. get_following() - Query database for following list");
            println!("      2. get_followers() - Query database for followers list");
            println!("      3. analyze_mentions() - Analyze mentions from casts (can be slow)");
            println!("      4. categorize_social_circles() - Categorize users (can be slow)");
            println!("      5. analyze_interaction_style() - Analyze interaction patterns");
            println!("      6. get_top_users() - Get top users (2 queries)");
            println!("      7. generate_word_cloud() - Generate word cloud from casts");
            println!();

            // Check if there are many casts (which would make analyze_mentions slow)
            if let Ok(cast_count) =
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM casts WHERE fid = $1")
                    .bind(fid)
                    .fetch_one(pool)
                    .await
            {
                if cast_count > 1000 {
                    println!("   ⚠️  WARNING: User has {cast_count} casts");
                    println!("      analyze_mentions() will process ALL casts - this can take a long time!");
                    println!("      Each cast needs to be parsed for mentions (@username)");
                    println!();
                }
            }

            // Check following/followers count
            if let Ok(following_count) = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(DISTINCT target_fid) FROM links WHERE fid = $1 AND link_type = 'follow' AND event_type = 'add'"
            )
            .bind(fid)
            .fetch_one(pool)
            .await
            {
                if following_count > 10000 {
                    println!("   ⚠️  WARNING: User follows {following_count} users");
                    println!("      categorize_social_circles() needs to analyze each one - this can take a long time!");
                    println!();
                }
            }

            if let Ok(followers_count) = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(DISTINCT fid) FROM links WHERE target_fid = $1 AND link_type = 'follow' AND event_type = 'add'"
            )
            .bind(fid)
            .fetch_one(pool)
            .await
            {
                if followers_count > 10000 {
                    println!("   ⚠️  CRITICAL: User has {followers_count} followers");
                    println!("      get_followers() query is the BOTTLENECK!");
                    println!("      - Must scan {followers_count} link records with window function");
                    println!("      - Window function (ROW_NUMBER OVER) is expensive at this scale");
                    println!("      - Estimated time: 20-40+ minutes for this query alone");
                    println!("      - categorize_social_circles() only samples 50 users (fast)");
                    println!();
                }
            }

            // Check links count
            match sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM links WHERE (fid = $1 OR target_fid = $1) AND link_type = 'follow'"
            )
            .bind(fid)
            .fetch_one(pool)
            .await
            {
                Ok(count) => {
                    println!("   Follow links: {count} (total entries)");
                }
                Err(e) => {
                    warn!("Failed to check links count: {}", e);
                }
            }

            // Check casts count
            match sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM casts WHERE fid = $1")
                .bind(fid)
                .fetch_one(pool)
                .await
            {
                Ok(count) => {
                    println!("   Casts: {count}");
                }
                Err(e) => {
                    warn!("Failed to check casts count: {}", e);
                }
            }

            println!();
        }
    } else {
        println!("⚠️  Database connection failed\n");
    }

    println!("💡 Diagnostic Tips:");
    println!("   1. Check worker logs:");
    println!("      Look for 'Worker X: Analyzing social graph for FID {fid}'");
    println!("      Look for step-by-step debug logs (analyze_mentions, categorize_social_circles, etc.)");
    println!();
    println!("   2. Most likely causes for long runtime:");
    if job_type == "social" {
        // Get counts for diagnostic info
        if let Ok(ref db) = database_result {
            let pool = db.pool();
            let cast_count =
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM casts WHERE fid = $1")
                    .bind(fid)
                    .fetch_one(pool)
                    .await
                    .unwrap_or(0);

            let links_count = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM links WHERE (fid = $1 OR target_fid = $1) AND link_type = 'follow'"
            )
            .bind(fid)
            .fetch_one(pool)
            .await
            .unwrap_or(0);

            // Get followers count for more accurate diagnosis
            let followers_count = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(DISTINCT fid) FROM links WHERE target_fid = $1 AND link_type = 'follow' AND event_type = 'add'"
            )
            .bind(fid)
            .fetch_one(pool)
            .await
            .unwrap_or(0);

            println!("      🔴 get_followers() - CRITICAL BOTTLENECK ⚠️");
            println!("         Querying {followers_count} followers with window function");
            println!("         This is the most likely cause of 40+ minute runtime!");
            println!(
                "         The query must scan all links and apply ROW_NUMBER() window function"
            );
            println!("         Estimated time: 20-40+ minutes for {followers_count} followers");
            println!();
            println!(
                "      • analyze_mentions() - Only processes top 100 casts (LIMIT 100) - FAST"
            );
            println!("      • categorize_social_circles() - Only samples 50 users - FAST");
            println!("      • generate_word_cloud() - Processes recent casts - FAST");
            println!(
                "      • get_top_users() - Sorts {followers_count} followers to get top 5 - MODERATE"
            );
        } else {
            println!("      • analyze_mentions() - Processing ALL casts for mentions");
            println!("      • categorize_social_circles() - Analyzing following/followers");
            println!("      • generate_word_cloud() - Processing casts for word frequency");
        }
    }
    println!();
    println!("   3. To see real-time progress:");
    println!("      tail -f <log_file> | grep 'FID {fid}'");
    println!();
    println!("   4. If stuck for > 1 hour, consider:");
    println!("      • Check if worker process is actually running (not hung)");
    println!("      • Check database connection is healthy");
    println!("      • Consider optimizing analyze_user() for large datasets");

    Ok(())
}

/// Handle list tasks command - show all tasks with their execution status
pub async fn handle_list_tasks(
    config: &AppConfig,
    queue: Option<String>,
    status_filter: Option<String>,
    limit: usize,
) -> Result<()> {
    println!("📋 Task List");
    println!("============\n");

    // Check Redis configuration
    let redis_cfg = config.redis.as_ref().ok_or_else(|| {
        crate::SnapRagError::Custom("Redis configuration is required for listing tasks".to_string())
    })?;

    let redis = crate::api::redis_client::RedisClient::connect(redis_cfg)?;

    println!("✅ Redis connected\n");

    // Get active jobs
    let active_jobs = redis.get_active_jobs_status(queue.as_deref()).await?;

    if active_jobs.is_empty() {
        if let Some(q) = &queue {
            println!("ℹ️  No tasks found in queue '{q}'");
        } else {
            println!("ℹ️  No tasks found");
        }
        return Ok(());
    }

    // Filter by status if specified
    let filtered_jobs: Vec<_> = if let Some(status) = &status_filter {
        active_jobs
            .into_iter()
            .filter(|(_, job_status, _, _)| job_status == status)
            .take(limit)
            .collect()
    } else {
        active_jobs.into_iter().take(limit).collect()
    };

    if filtered_jobs.is_empty() {
        if let Some(status) = &status_filter {
            println!("ℹ️  No tasks found with status '{}'", status);
        } else {
            println!("ℹ️  No tasks found");
        }
        return Ok(());
    }

    println!("🔍 Found {} task(s):\n", filtered_jobs.len());

    // Group by status
    let mut processing = Vec::new();
    let mut completed = Vec::new();
    let mut failed = Vec::new();
    let mut pending = Vec::new();
    let mut unknown = Vec::new();

    for (job_key, status, result, processing_duration) in filtered_jobs {
        match status.as_str() {
            "processing" => processing.push((job_key, result, processing_duration)),
            "completed" => completed.push((job_key, result)),
            "failed" => failed.push((job_key, result)),
            "pending" => pending.push((job_key, result)),
            _ => unknown.push((job_key, status, result)),
        }
    }

    // Display tasks grouped by status
    if !processing.is_empty() {
        println!("⏳ Processing ({}):", processing.len());
        for (job_key, _result, duration) in processing {
            if let Some(dur) = duration {
                let hours = dur / 3600;
                let minutes = (dur % 3600) / 60;
                let seconds = dur % 60;
                let duration_str = if hours > 0 {
                    format!("{hours}h {minutes}m {seconds}s")
                } else if minutes > 0 {
                    format!("{minutes}m {seconds}s")
                } else {
                    format!("{seconds}s")
                };
                println!("   • {} - Processing for {}", job_key, duration_str);
            } else {
                println!("   • {} - Processing", job_key);
            }
        }
        println!();
    }

    if !pending.is_empty() {
        println!("📋 Pending ({}):", pending.len());
        for (job_key, _result) in pending {
            println!("   • {}", job_key);
        }
        println!();
    }

    if !completed.is_empty() {
        println!("✅ Completed ({}):", completed.len());
        for (job_key, _result) in completed {
            println!("   • {}", job_key);
        }
        println!();
    }

    if !failed.is_empty() {
        println!("❌ Failed ({}):", failed.len());
        for (job_key, result) in failed {
            println!("   • {}", job_key);
            if let Some(err) = result {
                if err.len() < 100 {
                    println!("     Error: {}", err);
                } else {
                    println!("     Error: {}...", &err[..100]);
                }
            }
        }
        println!();
    }

    if !unknown.is_empty() {
        println!("❓ Unknown Status ({}):", unknown.len());
        for (job_key, status, _result) in unknown {
            println!("   • {} - Status: {}", job_key, status);
        }
        println!();
        println!("   ⚠️  Note: Jobs with unknown status may be orphaned.");
        println!("      Their status may have expired (TTL: 24 hours) or never been set.");
        println!("      Consider using 'snaprag serve worker --cleanup' to clean them up.");
        println!();
    }

    println!("💡 Tip: Use 'snaprag task stop <job_key>' to stop a task");
    println!("   Example: snaprag task stop social:66");

    Ok(())
}

/// Handle stop task command - stop a task by its job key
pub async fn handle_stop_task(config: &AppConfig, job_key: String, force: bool) -> Result<()> {
    use tracing::error;
    use tracing::info;
    use tracing::warn;

    println!("🛑 Stop Task");
    println!("============\n");

    // Check Redis configuration
    let redis_cfg = config.redis.as_ref().ok_or_else(|| {
        crate::SnapRagError::Custom(
            "Redis configuration is required for stopping tasks".to_string(),
        )
    })?;

    let redis = crate::api::redis_client::RedisClient::connect(redis_cfg)?;

    println!("✅ Redis connected\n");

    // Check if job exists and get its status
    let job_status = redis.get_job_status(&job_key).await?;

    if job_status.is_none() {
        return Err(crate::SnapRagError::Custom(format!(
            "Task '{}' not found",
            job_key
        )));
    }

    let (status, _result) = job_status.unwrap();

    // Check if job is already completed or failed
    if status == "completed" {
        println!("ℹ️  Task '{}' is already completed", job_key);
        return Ok(());
    }

    if status == "failed" {
        println!("ℹ️  Task '{}' has already failed", job_key);
        return Ok(());
    }

    // Check if job is processing and force is not set
    if status == "processing" && !force {
        return Err(crate::SnapRagError::Custom(format!(
            "Task '{}' is currently processing. Use --force to stop it anyway (may cause data inconsistency)",
            job_key
        )));
    }

    // Extract job type from job_key (format: "type:fid" or "type:fid:year")
    let job_type = job_key.split(':').next().unwrap_or("unknown");

    // Mark job as inactive
    if let Err(e) = redis.mark_job_inactive(&job_key).await {
        warn!("Failed to mark job as inactive: {}", e);
    } else {
        info!("Marked job {} as inactive", job_key);
    }

    // Set job status to "cancelled"
    if let Err(e) = redis.set_job_status(&job_key, "cancelled", None).await {
        error!("Failed to set job status: {}", e);
        return Err(crate::SnapRagError::Custom(format!(
            "Failed to stop task: {}",
            e
        )));
    }

    println!("✅ Task '{}' has been stopped", job_key);
    println!();
    println!("💡 The task status has been set to 'cancelled'");
    println!("   Note: If the task was in a queue, it may still be processed by a worker");
    println!("   until the worker checks the job status.");

    Ok(())
}
