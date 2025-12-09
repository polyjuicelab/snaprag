//! API server handlers

use std::sync::Arc;

use crate::cli::output::*;
use crate::AppConfig;
use crate::Result;
use crate::SnapRag;

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
pub async fn handle_serve_worker(config: &AppConfig, queue: String, workers: usize) -> Result<()> {
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::time::sleep;
    use tracing::debug;
    use tracing::error;
    use tracing::info;
    use tracing::warn;

    println!("🔧 Starting SnapRAG Worker");
    println!("==========================\n");
    println!("📦 Queue: {queue}");
    println!("👷 Workers: {workers}");

    // Check Redis configuration
    let redis_cfg = config.redis.as_ref().ok_or_else(|| {
        crate::SnapRagError::Custom("Redis configuration is required for worker".to_string())
    })?;

    let redis = Arc::new(crate::api::redis_client::RedisClient::connect(redis_cfg)?);
    let database = Arc::new(crate::database::Database::from_config(config).await?);

    println!("✅ Redis connected");
    println!("✅ Database connected");
    println!();

    // Spawn worker tasks
    let mut handles = Vec::new();
    for worker_id in 0..workers {
        let redis_clone = redis.clone();
        let database_clone = database.clone();
        let queue_clone = queue.clone();

        let handle = tokio::spawn(async move {
            info!("Worker {} started", worker_id);
            loop {
                // Pop job from queue (blocking, 5 second timeout)
                match redis_clone
                    .pop_job(&queue_clone, Duration::from_secs(5))
                    .await
                {
                    Ok(Some((job_id, job_data))) => {
                        let job_start_time = std::time::Instant::now();
                        info!(
                            "Worker {}: Processing job {} (started at {:?})",
                            worker_id, job_id, job_start_time
                        );

                        // Parse job data
                        let parse_start = std::time::Instant::now();
                        let job: serde_json::Value = match serde_json::from_str(&job_data) {
                            Ok(j) => {
                                let parse_duration = parse_start.elapsed();
                                debug!(
                                    "Worker {}: Parsed job data in {}ms",
                                    worker_id,
                                    parse_duration.as_millis()
                                );
                                j
                            }
                            Err(e) => {
                                error!(
                                    "Worker {}: Failed to parse job data: {} (took {}ms)",
                                    worker_id,
                                    e,
                                    parse_start.elapsed().as_millis()
                                );
                                continue;
                            }
                        };

                        // Extract job type and FID
                        let job_type = job
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        let fid = job.get("fid").and_then(|v| v.as_i64()).ok_or_else(|| {
                            crate::SnapRagError::Custom("Missing fid in job data".to_string())
                        });

                        match fid {
                            Ok(fid) => {
                                // Construct job key from job type and FID
                                let job_key = format!("{}:{}", job_type, fid);

                                // Update status to processing
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
                                let result = match job_type {
                                    "social" => {
                                        info!(
                                            "Worker {}: Analyzing social graph for FID {} (started at {:?})",
                                            worker_id, fid, analysis_start
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
                                        result
                                    }
                                    _ => {
                                        error!(
                                            "Worker {}: Unknown job type: {}",
                                            worker_id, job_type
                                        );
                                        Err(crate::SnapRagError::Custom(format!(
                                            "Unknown job type: {}",
                                            job_type
                                        )))
                                    }
                                };

                                match result {
                                    Ok(social_profile) => {
                                        // Cache the result
                                        let cache_start = std::time::Instant::now();
                                        let cache_service =
                                            crate::api::cache::CacheService::with_config(
                                                redis_clone.clone(),
                                                crate::api::cache::CacheConfig::default(),
                                            );
                                        if let Err(e) =
                                            cache_service.set_social(fid, &social_profile).await
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
                                        let status_complete_start = std::time::Instant::now();
                                        let result_json = serde_json::to_string(&social_profile)
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
                                                Some(&format!("{}", e)),
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
                            Err(e) => {
                                error!("Worker {}: Invalid job data: {}", worker_id, e);
                            }
                        }
                    }
                    Ok(None) => {
                        // Timeout - no job available, continue waiting
                    }
                    Err(e) => {
                        error!("Worker {}: Error popping job: {}", worker_id, e);
                        sleep(Duration::from_secs(1)).await; // Wait before retrying
                    }
                }
            }
        });

        handles.push(handle);
    }

    println!("✅ {} worker(s) started", workers);
    println!("⏳ Waiting for jobs...\n");

    // Wait for all workers (they run forever)
    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}
