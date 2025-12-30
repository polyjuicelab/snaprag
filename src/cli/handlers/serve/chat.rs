//! Chat job processing for worker

use std::sync::Arc;
use std::time::Duration;

use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::warn;

use crate::api::redis_client::RedisClient;
use crate::database::Database;
use crate::AppConfig;

/// Process a chat job
///
/// # Arguments
///
/// * `worker_id` - Worker identifier for logging
/// * `job_id` - Job ID from queue
/// * `job` - Parsed job data (JSON value)
/// * `job_key` - Job key for status tracking
/// * `job_start_time` - When the job processing started
/// * `redis` - Redis client
/// * `database` - Database client
/// * `config` - Application configuration
///
/// # Returns
///
/// Returns `Ok(())` if the job was processed successfully, or an error if processing failed.
/// Note: This function handles all error cases internally and updates job status accordingly.
pub async fn process_chat_job(
    worker_id: usize,
    job_id: &str,
    job: &serde_json::Value,
    job_key: &str,
    job_start_time: std::time::Instant,
    redis: Arc<RedisClient>,
    database: Arc<Database>,
    config: &AppConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Extract session_id and message from job data
    let session_id = match job.get("session_id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => {
            error!("Worker {}: Missing session_id in chat job", worker_id);
            if let Err(e) = redis
                .set_job_status(job_key, "failed", Some("Missing session_id"))
                .await
            {
                warn!("Worker {}: Failed to update job status: {}", worker_id, e);
            }
            let _ = redis.mark_job_inactive(job_key).await;
            return Err("Missing session_id".into());
        }
    };

    let message = match job.get("message").and_then(|v| v.as_str()) {
        Some(msg) => msg,
        None => {
            error!("Worker {}: Missing message in chat job", worker_id);
            if let Err(e) = redis
                .set_job_status(job_key, "failed", Some("Missing message"))
                .await
            {
                warn!("Worker {}: Failed to update job status: {}", worker_id, e);
            }
            let _ = redis.mark_job_inactive(job_key).await;
            return Err("Missing message".into());
        }
    };

    // Log job details
    info!("  Job Type: chat");
    info!("  Session ID: {}", session_id);
    info!("  Job Key: {}", job_key);
    info!("  Started at: {:?}", job_start_time);

    // Check if job is already failed or cancelled before processing
    if let Ok(Some((status, _))) = redis.get_job_status(job_key).await {
        if status == "failed" || status == "cancelled" {
            warn!(
                "Worker {}: ⚠️  SKIPPING job {} - Status: {} (job already completed/failed)",
                worker_id, job_key, status
            );
            let _ = redis.mark_job_inactive(job_key).await;
            info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            return Ok(()); // Job already processed, skip
        }
        info!("  Current Status: {} (updating to processing)", status);
    } else {
        info!("  Current Status: pending (updating to processing)");
    }

    // Update job status to processing
    let status_update_start = std::time::Instant::now();
    if let Err(e) = redis.set_job_status(job_key, "processing", None).await {
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

    // Process chat job
    let analysis_start = std::time::Instant::now();

    // Read session from Redis
    let session_key = format!("session:{}", session_id);
    let session_json = match redis.get_json(&session_key).await {
        Ok(Some(json)) => json,
        Ok(None) => {
            error!("Worker {}: Session {} not found", worker_id, session_id);
            if let Err(e) = redis
                .set_job_status(
                    job_key,
                    "failed",
                    Some(&format!("Session {} not found", session_id)),
                )
                .await
            {
                warn!("Worker {}: Failed to update job status: {}", worker_id, e);
            }
            let _ = redis.mark_job_inactive(job_key).await;
            return Err(format!("Session {} not found", session_id).into());
        }
        Err(e) => {
            error!(
                "Worker {}: Failed to get session from Redis: {}",
                worker_id, e
            );
            if let Err(update_err) = redis
                .set_job_status(job_key, "failed", Some(&format!("Redis error: {}", e)))
                .await
            {
                warn!(
                    "Worker {}: Failed to update job status: {}",
                    worker_id, update_err
                );
            }
            let _ = redis.mark_job_inactive(job_key).await;
            return Err(format!("Redis error: {}", e).into());
        }
    };

    let mut session: crate::api::session::ChatSession = match serde_json::from_str(&session_json) {
        Ok(s) => s,
        Err(e) => {
            error!("Worker {}: Failed to deserialize session: {}", worker_id, e);
            if let Err(update_err) = redis
                .set_job_status(
                    job_key,
                    "failed",
                    Some(&format!("Deserialization error: {}", e)),
                )
                .await
            {
                warn!(
                    "Worker {}: Failed to update job status: {}",
                    worker_id, update_err
                );
            }
            let _ = redis.mark_job_inactive(job_key).await;
            return Err(format!("Deserialization error: {}", e).into());
        }
    };

    // Get user profile
    let profile = match database.get_user_profile(session.fid).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            error!("Worker {}: User {} not found", worker_id, session.fid);
            if let Err(e) = redis
                .set_job_status(
                    job_key,
                    "failed",
                    Some(&format!("User {} not found", session.fid)),
                )
                .await
            {
                warn!("Worker {}: Failed to update job status: {}", worker_id, e);
            }
            let _ = redis.mark_job_inactive(job_key).await;
            return Err(format!("User {} not found", session.fid).into());
        }
        Err(e) => {
            error!("Worker {}: Database error: {}", worker_id, e);
            if let Err(update_err) = redis
                .set_job_status(job_key, "failed", Some(&format!("Database error: {}", e)))
                .await
            {
                warn!(
                    "Worker {}: Failed to update job status: {}",
                    worker_id, update_err
                );
            }
            let _ = redis.mark_job_inactive(job_key).await;
            return Err(format!("Database error: {}", e).into());
        }
    };

    // Create embedding service
    let embedding_service = match crate::embeddings::EmbeddingService::new(config) {
        Ok(service) => service,
        Err(e) => {
            error!(
                "Worker {}: Failed to create embedding service: {}",
                worker_id, e
            );
            if let Err(update_err) = redis
                .set_job_status(
                    job_key,
                    "failed",
                    Some(&format!("Embedding service error: {}", e)),
                )
                .await
            {
                warn!(
                    "Worker {}: Failed to update job status: {}",
                    worker_id, update_err
                );
            }
            let _ = redis.mark_job_inactive(job_key).await;
            return Err(format!("Embedding service error: {}", e).into());
        }
    };

    // Generate embedding
    let query_embedding = match embedding_service.generate(message).await {
        Ok(emb) => emb,
        Err(e) => {
            error!("Worker {}: Failed to generate embedding: {}", worker_id, e);
            if let Err(update_err) = redis
                .set_job_status(job_key, "failed", Some(&format!("Embedding error: {}", e)))
                .await
            {
                warn!(
                    "Worker {}: Failed to update job status: {}",
                    worker_id, update_err
                );
            }
            let _ = redis.mark_job_inactive(job_key).await;
            return Err(format!("Embedding error: {}", e).into());
        }
    };

    // Vector search
    let search_limit = (session.context_limit * 5).max(100);
    let search_results = match database
        .semantic_search_casts_simple(query_embedding, search_limit as i64, Some(0.3))
        .await
    {
        Ok(results) => results,
        Err(e) => {
            error!("Worker {}: Vector search failed: {}", worker_id, e);
            if let Err(update_err) = redis
                .set_job_status(
                    job_key,
                    "failed",
                    Some(&format!("Vector search error: {}", e)),
                )
                .await
            {
                warn!(
                    "Worker {}: Failed to update job status: {}",
                    worker_id, update_err
                );
            }
            let _ = redis.mark_job_inactive(job_key).await;
            return Err(format!("Vector search error: {}", e).into());
        }
    };

    // Filter and sort (reuse logic from chat.rs)
    let mut user_casts: Vec<_> = search_results
        .into_iter()
        .filter(|r| r.fid == session.fid)
        .collect();

    // Calculate current timestamp
    #[allow(clippy::cast_possible_wrap)]
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Sort by relevance * substance * recency
    user_casts.sort_by(|a, b| {
        fn calculate_score(similarity: f32, text_len: usize, recency: f32) -> f32 {
            let sim = if !similarity.is_finite() || similarity < 0.0 {
                0.0
            } else {
                similarity.min(1.0)
            };
            let len = text_len.clamp(1, 10000);
            #[allow(clippy::cast_precision_loss)]
            let substance = (len as f32).ln().clamp(1.0, 10.0);
            let rec = if !recency.is_finite() || recency < 0.0 {
                0.5
            } else {
                recency.min(1.0)
            };
            let score = sim * substance * rec;
            if !score.is_finite() || score < 0.0 {
                0.0
            } else {
                score
            }
        }

        #[allow(clippy::cast_precision_loss)]
        let age_days_a = ((now - a.timestamp) as f32) / 86400.0;
        #[allow(clippy::cast_precision_loss)]
        let age_days_b = ((now - b.timestamp) as f32) / 86400.0;
        let recency_a = (1.0 - (age_days_a / 365.0).min(0.5)).max(0.5);
        let recency_b = (1.0 - (age_days_b / 365.0).min(0.5)).max(0.5);

        let score_a = calculate_score(a.similarity, a.text.len(), recency_a);
        let score_b = calculate_score(b.similarity, b.text.len(), recency_b);

        score_b.total_cmp(&score_a)
    });
    user_casts.truncate(session.context_limit);

    // Build context
    let context =
        crate::api::handlers::chat::build_chat_context(&profile, &user_casts, &session, message);

    // Create LLM service
    let llm_service = match crate::llm::LlmService::new(config) {
        Ok(service) => service,
        Err(e) => {
            error!("Worker {}: Failed to create LLM service: {}", worker_id, e);
            if let Err(update_err) = redis
                .set_job_status(
                    job_key,
                    "failed",
                    Some(&format!("LLM service error: {}", e)),
                )
                .await
            {
                warn!(
                    "Worker {}: Failed to update job status: {}",
                    worker_id, update_err
                );
            }
            let _ = redis.mark_job_inactive(job_key).await;
            return Err(format!("LLM service error: {}", e).into());
        }
    };

    // Generate response
    let mut response_text = match llm_service
        .generate_with_params(&context, session.temperature, 2000)
        .await
    {
        Ok(text) => text,
        Err(e) => {
            error!("Worker {}: Failed to generate response: {}", worker_id, e);
            if let Err(update_err) = redis
                .set_job_status(
                    job_key,
                    "failed",
                    Some(&format!("LLM generation error: {}", e)),
                )
                .await
            {
                warn!(
                    "Worker {}: Failed to update job status: {}",
                    worker_id, update_err
                );
            }
            let _ = redis.mark_job_inactive(job_key).await;
            return Err(format!("LLM generation error: {}", e).into());
        }
    };

    // Clean response to remove any format markers
    response_text = crate::api::handlers::chat::clean_response_text(&response_text);

    // Update session
    session.add_message("user", message.to_string());
    session.add_message("assistant", response_text.clone());

    // Save session to Redis
    let updated_session_json = match serde_json::to_string(&session) {
        Ok(json) => json,
        Err(e) => {
            error!("Worker {}: Failed to serialize session: {}", worker_id, e);
            // Use empty JSON as fallback
            "{}".to_string()
        }
    };
    if let Err(e) = redis
        .set_json_with_ttl(
            &session_key,
            &updated_session_json,
            Some(Duration::from_secs(3600)),
        )
        .await
    {
        error!(
            "Worker {}: Failed to save session to Redis: {}",
            worker_id, e
        );
        // Continue anyway, we can still store the result
    }

    // Store result in Redis (for API polling)
    let result = serde_json::json!({
        "session_id": session_id,
        "message": response_text,
        "relevant_casts_count": user_casts.len(),
        "conversation_length": session.conversation_history.len(),
    });
    let result_json = result.to_string();
    if let Err(e) = redis
        .set_job_status(job_key, "completed", Some(&result_json))
        .await
    {
        error!("Worker {}: Failed to update job status: {}", worker_id, e);
    }

    // Mark job as inactive
    if let Err(e) = redis.mark_job_inactive(job_key).await {
        warn!("Worker {}: Failed to mark job inactive: {}", worker_id, e);
    }

    let total_duration = job_start_time.elapsed();
    info!(
        "Worker {}: ✅ Chat message processed for session {} in {}ms ({}s)",
        worker_id,
        session_id,
        total_duration.as_millis(),
        total_duration.as_secs()
    );
    info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    Ok(())
}
