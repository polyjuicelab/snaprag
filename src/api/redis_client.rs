use std::time::Duration;

use redis::AsyncCommands;
use tracing::info;
use tracing::warn;

use crate::config::RedisConfig;

#[derive(Clone)]
pub struct RedisClient {
    client: redis::Client,
    namespace: String,
    default_ttl: Duration,
    refresh_channel: String,
    stale_threshold: Duration,
}

impl RedisClient {
    pub fn connect(config: &RedisConfig) -> crate::Result<Self> {
        // Mask password in URL for logging (if present)
        let log_url = if config.url.contains('@') {
            // URL contains authentication, mask password
            if let Some(at_pos) = config.url.find('@') {
                if let Some(scheme_end) = config.url.find("://") {
                    let scheme_part = &config.url[..scheme_end + 3];
                    let after_scheme = &config.url[scheme_end + 3..at_pos];
                    let rest = &config.url[at_pos..];
                    if let Some(colon_pos) = after_scheme.find(':') {
                        format!("{}{}:***{}", scheme_part, &after_scheme[..colon_pos], rest)
                    } else {
                        format!("{}{}{}", scheme_part, after_scheme, rest)
                    }
                } else {
                    config.url.clone()
                }
            } else {
                config.url.clone()
            }
        } else {
            config.url.clone()
        };

        info!("🔧 Initializing Redis client...");
        info!("  Redis URL: {}", log_url);
        info!("  Namespace: {}", config.namespace);
        info!("  Default TTL: {} seconds", config.default_ttl_secs);
        info!("  Stale threshold: {} seconds", config.stale_threshold_secs);

        let client = redis::Client::open(config.url.as_str()).map_err(|e| {
            let error_msg = format!("Redis open error: {e}");
            warn!("❌ Failed to open Redis client: {}", error_msg);
            crate::SnapRagError::Custom(error_msg)
        })?;

        info!("✅ Redis client opened successfully");

        Ok(Self {
            client,
            namespace: config.namespace.clone(),
            default_ttl: Duration::from_secs(config.default_ttl_secs),
            refresh_channel: config.refresh_channel.clone(),
            stale_threshold: Duration::from_secs(config.stale_threshold_secs),
        })
    }

    fn key(&self, k: &str) -> String {
        format!("{}{}", self.namespace, k)
    }

    pub async fn get_json(&self, key: &str) -> crate::Result<Option<String>> {
        let k = self.key(key);
        let mut conn = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis connect error: {e}")))?;
        let val: Option<String> = conn
            .get(k)
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis GET error: {e}")))?;
        Ok(val)
    }

    /// Set JSON value in Redis without TTL (permanent storage)
    pub async fn set_json(&self, key: &str, json: &str) -> crate::Result<()> {
        let k = self.key(key);
        let mut conn = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis connect error: {e}")))?;
        conn.set::<_, _, ()>(&k, json)
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis SET error: {e}")))?;
        Ok(())
    }

    pub async fn set_json_with_ttl(
        &self,
        key: &str,
        json: &str,
        ttl: Option<Duration>,
    ) -> crate::Result<()> {
        let k = self.key(key);
        let ttl = ttl.unwrap_or(self.default_ttl);
        let mut conn = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis connect error: {e}")))?;
        redis::pipe()
            .set(&k, json)
            .ignore()
            .expire(&k, ttl.as_secs() as i64)
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis SET/EXPIRE error: {e}")))?;
        Ok(())
    }

    pub async fn ttl_secs(&self, key: &str) -> crate::Result<Option<i64>> {
        let k = self.key(key);
        let mut conn = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis connect error: {e}")))?;
        let ttl: i64 = conn
            .ttl(k)
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis TTL error: {e}")))?;
        if ttl < 0 {
            Ok(None)
        } else {
            Ok(Some(ttl))
        }
    }

    /// Delete a key from Redis
    pub async fn delete(&self, key: &str) -> crate::Result<bool> {
        let k = self.key(key);
        let mut conn = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis connect error: {e}")))?;
        let deleted: i64 = conn
            .del(&k)
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis DEL error: {e}")))?;
        Ok(deleted > 0)
    }

    /// Delete multiple keys matching a pattern
    /// Returns the number of keys deleted
    /// Note: Uses KEYS command which may block Redis in production, but acceptable for CLI tools
    pub async fn delete_pattern(&self, pattern: &str) -> crate::Result<u64> {
        let pattern_with_namespace = self.key(pattern);
        let mut conn = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis connect error: {e}")))?;

        // Use KEYS to find all matching keys (acceptable for CLI tools)
        let keys: Vec<String> = conn
            .keys(&pattern_with_namespace)
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis KEYS error: {e}")))?;

        // Delete all matching keys in batches
        let mut deleted_count = 0u64;
        if !keys.is_empty() {
            for chunk in keys.chunks(100) {
                let deleted: i64 = conn
                    .del(chunk)
                    .await
                    .map_err(|e| crate::SnapRagError::Custom(format!("Redis DEL error: {e}")))?;
                deleted_count += deleted as u64;
            }
        }

        Ok(deleted_count)
    }

    #[must_use]
    pub const fn stale_threshold(&self) -> Duration {
        self.stale_threshold
    }

    pub async fn publish_refresh(&self, subject: &str, key: &str) -> crate::Result<()> {
        let mut conn = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis connect error: {e}")))?;
        let payload = format!("{subject}|{key}");
        conn.publish::<_, _, ()>(&self.refresh_channel, payload)
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis PUBLISH error: {e}")))?;
        Ok(())
    }

    /// Push a job to the queue
    /// Returns job_id if successfully pushed, or None if job already exists
    pub async fn push_job(
        &self,
        queue: &str,
        job_key: &str,
        job_data: &str,
    ) -> crate::Result<Option<String>> {
        // Check if job is already active
        if self.is_job_active(job_key).await? {
            return Ok(None); // Job already exists
        }

        let queue_key = self.key(&format!("queue:{queue}"));
        let mut conn = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis connect error: {e}")))?;

        // Generate job ID
        let job_id = format!("job:{}:{}", queue, uuid::Uuid::new_v4());

        // Push to queue, set job data, and mark as active
        redis::pipe()
            .lpush(&queue_key, &job_id)
            .ignore()
            .set(self.key(&job_id), job_data)
            .ignore()
            .expire(self.key(&job_id), 3600) // 1 hour TTL for job data
            .sadd(self.key("active_jobs"), job_key)
            .ignore()
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis queue push error: {e}")))?;

        // Set initial status
        self.set_job_status(job_key, "pending", None).await?;

        Ok(Some(job_id))
    }

    /// Pop a job from the queue (blocking)
    pub async fn pop_job(
        &self,
        queue: &str,
        timeout: Duration,
    ) -> crate::Result<Option<(String, String)>> {
        let queue_key = self.key(&format!("queue:{queue}"));
        let mut conn = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis connect error: {e}")))?;

        // Blocking pop from queue
        let result: Option<(String, String)> = conn
            .brpop(&queue_key, timeout.as_secs() as f64)
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis queue pop error: {e}")))?;

        if let Some((_, job_id)) = result {
            // Get job data
            let job_data_key = self.key(&job_id);
            let job_data: Option<String> = conn.get(&job_data_key).await.map_err(|e| {
                crate::SnapRagError::Custom(format!("Redis GET job data error: {e}"))
            })?;

            if let Some(data) = job_data {
                // Remove job data after reading
                let _: () = conn.del(&job_data_key).await.map_err(|e| {
                    crate::SnapRagError::Custom(format!("Redis DEL job data error: {e}"))
                })?;

                Ok(Some((job_id, data)))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Push a hook event onto a fixed-size queue
    pub async fn push_hook_event(&self, event_json: &str, max_len: usize) -> crate::Result<()> {
        let queue_key = self.key("hook_events");
        let mut conn = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis connect error: {e}")))?;

        let max_len = isize::try_from(max_len.saturating_sub(1)).unwrap_or(999);
        redis::pipe()
            .lpush(&queue_key, event_json)
            .ignore()
            .ltrim(&queue_key, 0, max_len)
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| {
                crate::SnapRagError::Custom(format!("Redis hook queue push error: {e}"))
            })?;

        Ok(())
    }

    /// Pop a hook event from the queue (blocking with timeout)
    pub async fn pop_hook_event(&self, timeout: Duration) -> crate::Result<Option<String>> {
        let queue_key = self.key("hook_events");
        let mut conn = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis connect error: {e}")))?;

        let result: Option<(String, String)> = conn
            .brpop(&queue_key, timeout.as_secs() as f64)
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis hook queue pop error: {e}")))?;

        Ok(result.map(|(_, payload)| payload))
    }

    /// Return the length of the hook event queue (for diagnostics).
    pub async fn hook_queue_len(&self) -> crate::Result<u64> {
        let queue_key = self.key("hook_events");
        let mut conn = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis connect error: {e}")))?;
        let len: u64 = conn
            .llen(&queue_key)
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis LLEN error: {e}")))?;
        Ok(len)
    }

    /// Peek up to `count` entries from the right (oldest) side of the hook queue (for diagnostics).
    pub async fn hook_queue_peek(&self, count: isize) -> crate::Result<Vec<String>> {
        let queue_key = self.key("hook_events");
        let mut conn = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis connect error: {e}")))?;
        let items: Vec<String> = conn
            .lrange(&queue_key, -count, -1)
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis LRANGE error: {e}")))?;
        Ok(items)
    }

    /// Pop a job from multiple queues (blocking, returns first available)
    /// Returns (queue_name, job_id, job_data) if a job is found
    pub async fn pop_job_from_queues(
        &self,
        queues: &[&str],
        timeout: Duration,
    ) -> crate::Result<Option<(String, String, String)>> {
        if queues.is_empty() {
            return Ok(None);
        }

        let mut conn = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis connect error: {e}")))?;

        // Build queue keys
        let queue_keys: Vec<String> = queues
            .iter()
            .map(|q| self.key(&format!("queue:{q}")))
            .collect();

        // Use BRPOP to pop from multiple queues at once
        // BRPOP returns the first queue that has data
        let result: Option<(String, String)> = redis::cmd("BRPOP")
            .arg(&queue_keys)
            .arg(timeout.as_secs())
            .query_async(&mut conn)
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis BRPOP error: {e}")))?;

        if let Some((queue_key, job_id)) = result {
            // Extract queue name from queue_key (format: "snaprag:queue:social")
            // queue_key is the full key like "snaprag:queue:social"
            // We need to extract "social" from it
            let queue_prefix = self.key("queue:");
            let queue_name = if queue_key.starts_with(&queue_prefix) {
                queue_key
                    .strip_prefix(&queue_prefix)
                    .unwrap_or("unknown")
                    .to_string()
            } else {
                // Fallback: try to extract from the key
                queue_key
                    .split(':')
                    .next_back()
                    .unwrap_or("unknown")
                    .to_string()
            };

            // Get job data
            let job_data_key = self.key(&job_id);
            let job_data: Option<String> = conn.get(&job_data_key).await.map_err(|e| {
                crate::SnapRagError::Custom(format!("Redis GET job data error: {e}"))
            })?;

            if let Some(data) = job_data {
                // Remove job data after reading
                let _: () = conn.del(&job_data_key).await.map_err(|e| {
                    crate::SnapRagError::Custom(format!("Redis DEL job data error: {e}"))
                })?;

                Ok(Some((queue_name, job_id, data)))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Set job status
    pub async fn set_job_status(
        &self,
        job_id: &str,
        status: &str,
        result: Option<&str>,
    ) -> crate::Result<()> {
        let status_key = self.key(&format!("job_status:{job_id}"));
        let mut conn = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis connect error: {e}")))?;

        let status_data = if let Some(result_data) = result {
            format!("{status}|{result_data}")
        } else {
            status.to_string()
        };

        redis::pipe()
            .set(&status_key, status_data)
            .ignore()
            .expire(&status_key, 86400) // 24 hours TTL
            .query_async::<()>(&mut conn)
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis set job status error: {e}")))?;

        Ok(())
    }

    /// Get job status
    pub async fn get_job_status(
        &self,
        job_id: &str,
    ) -> crate::Result<Option<(String, Option<String>)>> {
        let status_key = self.key(&format!("job_status:{job_id}"));
        let mut conn = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis connect error: {e}")))?;

        let status_data: Option<String> = conn
            .get(&status_key)
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis GET job status error: {e}")))?;

        if let Some(data) = status_data {
            let parts: Vec<&str> = data.splitn(2, '|').collect();
            let status = parts[0].to_string();
            let result = parts.get(1).map(|s| (*s).to_string());
            Ok(Some((status, result)))
        } else {
            Ok(None)
        }
    }

    /// Check if a job is already in queue or processing (for deduplication)
    /// Uses a simpler approach: check if there's an active job for this key
    /// Also checks if the job is stuck (processing for too long) and clears it
    pub async fn is_job_active(&self, job_key: &str) -> crate::Result<bool> {
        use tracing::debug;
        use tracing::warn;

        let mut conn = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis connect error: {e}")))?;

        // Use a set to track active jobs (more efficient than scanning queue)
        let active_jobs_key = self.key("active_jobs");
        let is_member: bool = conn
            .sismember(&active_jobs_key, job_key)
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis SISMEMBER error: {e}")))?;

        if is_member {
            // Check job status to see if it's stuck
            if let Ok(Some((status, _))) = self.get_job_status(job_key).await {
                debug!("Job {} is active with status: {}", job_key, status);

                // If status is "processing", check if it's been stuck for too long
                if status == "processing" {
                    let status_key = self.key(&format!("job_status:{job_key}"));
                    let ttl: i64 = conn.ttl(&status_key).await.map_err(|e| {
                        crate::SnapRagError::Custom(format!("Redis TTL error: {e}"))
                    })?;

                    // Status key has 24 hour TTL (86400 seconds)
                    // If TTL is less than 23 hours (82800 seconds), the job has been processing for > 1 hour
                    // Consider it stuck and clear it
                    if ttl > 0 && ttl < 82800 {
                        warn!(
                            "Clearing stuck job {} (status: processing, TTL: {}s, processing for >{}s)",
                            job_key,
                            ttl,
                            86400 - ttl
                        );
                        let _: () = conn.srem(&active_jobs_key, job_key).await.map_err(|e| {
                            crate::SnapRagError::Custom(format!("Redis SREM error: {e}"))
                        })?;
                        return Ok(false);
                    }
                } else if status == "completed" || status == "failed" {
                    // Job is done but still in active_jobs - clean it up
                    debug!(
                        "Cleaning up completed/failed job {} from active_jobs",
                        job_key
                    );
                    let _: () = conn.srem(&active_jobs_key, job_key).await.map_err(|e| {
                        crate::SnapRagError::Custom(format!("Redis SREM error: {e}"))
                    })?;
                    return Ok(false);
                }
            } else {
                // Job is in active_jobs but has no status - might be orphaned, check if it's in queue
                // Extract queue name from job_key (format: "type:fid")
                if let Some((job_type, _)) = job_key.split_once(':') {
                    let queue_key = self.key(&format!("queue:{job_type}"));
                    // Check queue length - if queue is empty or very small, job might be stuck
                    let queue_len: i64 = conn.llen(&queue_key).await.map_err(|e| {
                        crate::SnapRagError::Custom(format!("Redis LLEN error: {e}"))
                    })?;

                    // If queue is empty and job has no status, it's likely orphaned
                    if queue_len == 0 {
                        warn!("Clearing orphaned job {} (no status, queue empty)", job_key);
                        let _: () = conn.srem(&active_jobs_key, job_key).await.map_err(|e| {
                            crate::SnapRagError::Custom(format!("Redis SREM error: {e}"))
                        })?;
                        return Ok(false);
                    }
                }
            }
        }

        Ok(is_member)
    }

    /// Mark a job as active (add to active jobs set)
    pub async fn mark_job_active(&self, job_key: &str) -> crate::Result<()> {
        let mut conn = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis connect error: {e}")))?;

        let active_jobs_key = self.key("active_jobs");
        let _: () = conn
            .sadd(&active_jobs_key, job_key)
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis SADD error: {e}")))?;

        Ok(())
    }

    /// Mark a job as inactive (remove from active jobs set)
    pub async fn mark_job_inactive(&self, job_key: &str) -> crate::Result<()> {
        let mut conn = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis connect error: {e}")))?;

        let active_jobs_key = self.key("active_jobs");
        let _: () = conn
            .srem(&active_jobs_key, job_key)
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis SREM error: {e}")))?;

        Ok(())
    }

    /// Clean up old/stuck jobs from active_jobs set
    /// Returns the number of jobs cleaned up
    pub async fn cleanup_old_jobs(&self, queue: Option<&str>) -> crate::Result<usize> {
        use tracing::debug;
        use tracing::info;
        use tracing::warn;

        let mut conn = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis connect error: {e}")))?;

        let active_jobs_key = self.key("active_jobs");

        // Get all active jobs
        let job_keys: Vec<String> = conn
            .smembers(&active_jobs_key)
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis SMEMBERS error: {e}")))?;

        if job_keys.is_empty() {
            debug!("No active jobs to clean up");
            return Ok(0);
        }

        warn!("Checking {} active job(s) for cleanup...", job_keys.len());

        let mut cleaned_count = 0;
        let mut cleaned_jobs = Vec::new();

        for job_key in &job_keys {
            // Filter by queue if specified
            if let Some(queue_name) = queue {
                if let Some((job_type, _)) = job_key.split_once(':') {
                    if job_type != queue_name {
                        continue;
                    }
                } else {
                    continue;
                }
            }

            // Check job status
            if let Ok(Some((status, _))) = self.get_job_status(job_key).await {
                // Clean up completed or failed jobs
                if status == "completed" || status == "failed" {
                    cleaned_jobs.push(job_key.clone());
                    cleaned_count += 1;
                    warn!("Marked for cleanup: {} (status: {})", job_key, status);
                    continue;
                }

                // Clean up pending jobs - these are likely orphaned if they've been pending for a while
                // and aren't in the queue anymore
                if status == "pending" {
                    // Check if the job is actually in the queue
                    if let Some((job_type, _)) = job_key.split_once(':') {
                        let queue_key = self.key(&format!("queue:{job_type}"));
                        let queue_len: Result<i64, _> = conn.llen(&queue_key).await;
                        if let Ok(queue_len) = queue_len {
                            // If queue is empty or doesn't contain this job, it's orphaned
                            if queue_len == 0 {
                                cleaned_jobs.push(job_key.clone());
                                cleaned_count += 1;
                                warn!(
                                    "Marked orphaned pending job for cleanup: {} (status: pending, queue empty)",
                                    job_key
                                );
                                continue;
                            }
                            // For queues with items, we can't easily check if this specific job is in the queue
                            // without scanning, so we'll clean it up if it's been pending for too long
                            // Check the status key TTL to estimate how long it's been pending
                            let status_key = self.key(&format!("job_status:{job_key}"));
                            let ttl: Result<i64, _> = conn.ttl(&status_key).await;
                            if let Ok(ttl) = ttl {
                                // Status key has 24 hour TTL (86400 seconds)
                                // If TTL is less than 23 hours (82800 seconds), job has been pending for > 1 hour
                                // Clean it up as it's likely orphaned
                                if ttl > 0 && ttl < 82800 {
                                    cleaned_jobs.push(job_key.clone());
                                    cleaned_count += 1;
                                    warn!(
                                        "Marked old pending job for cleanup: {} (status: pending, TTL: {}s)",
                                        job_key, ttl
                                    );
                                    continue;
                                }
                            }
                        }
                    }
                }

                // Clean up stuck processing jobs (> 1 hour)
                if status == "processing" {
                    let status_key = self.key(&format!("job_status:{job_key}"));
                    let ttl: Result<i64, _> = conn.ttl(&status_key).await;
                    if let Ok(ttl) = ttl {
                        // Status key has 24 hour TTL (86400 seconds)
                        // If TTL is less than 23 hours (82800 seconds), job has been processing for > 1 hour
                        if ttl > 0 && ttl < 82800 {
                            cleaned_jobs.push(job_key.clone());
                            cleaned_count += 1;
                            warn!(
                                "Marked stuck job for cleanup: {} (status: processing, TTL: {}s)",
                                job_key, ttl
                            );
                        }
                    }
                }
            } else {
                // Job has no status - check if it's orphaned
                if let Some((job_type, _)) = job_key.split_once(':') {
                    let queue_key = self.key(&format!("queue:{job_type}"));
                    let queue_len: Result<i64, _> = conn.llen(&queue_key).await;
                    if let Ok(queue_len) = queue_len {
                        // If queue is empty and job has no status, it's likely orphaned
                        if queue_len == 0 {
                            cleaned_jobs.push(job_key.clone());
                            cleaned_count += 1;
                            warn!(
                                "Marked orphaned job for cleanup: {} (no status, queue empty)",
                                job_key
                            );
                        }
                    }
                }
            }
        }

        // Remove all cleaned jobs from active_jobs set
        if cleaned_jobs.is_empty() {
            warn!("✅ No jobs needed cleanup");
        } else {
            for job_key in &cleaned_jobs {
                let _: () = conn
                    .srem(&active_jobs_key, job_key)
                    .await
                    .map_err(|e| crate::SnapRagError::Custom(format!("Redis SREM error: {e}")))?;
            }
            warn!("✅ Cleaned up {} old/stuck job(s)", cleaned_count);
        }

        Ok(cleaned_count)
    }

    /// Re-queue pending jobs that are in active_jobs but not in the queue
    /// This can happen if jobs were marked as active but the queue push failed,
    /// or if the queue was cleared but active_jobs wasn't
    pub async fn requeue_pending_jobs(&self, queue: Option<&str>) -> crate::Result<usize> {
        use tracing::info;
        use tracing::warn;

        let mut conn = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis connect error: {e}")))?;

        let active_jobs_key = self.key("active_jobs");
        let job_keys: Vec<String> = conn
            .smembers(&active_jobs_key)
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis SMEMBERS error: {e}")))?;

        let mut requeued_count = 0;

        for job_key in job_keys {
            // Filter by queue if specified
            if let Some(queue_name) = queue {
                if let Some((job_type, _)) = job_key.split_once(':') {
                    if job_type != queue_name {
                        continue;
                    }
                } else {
                    continue;
                }
            }

            // Check if job status is pending
            if let Ok(Some((status, _))) = self.get_job_status(&job_key).await {
                if status != "pending" {
                    continue; // Skip non-pending jobs
                }

                // Extract queue name and FID from job_key (format: "type:fid")
                if let Some((job_type, fid_str)) = job_key.split_once(':') {
                    let fid: i64 = if let Ok(f) = fid_str.parse() {
                        f
                    } else {
                        warn!("Invalid FID in job_key {}: {}", job_key, fid_str);
                        continue;
                    };

                    // Check if job is already in the queue
                    let queue_key = self.key(&format!("queue:{job_type}"));
                    // We can't easily check if a specific job_id is in the queue without scanning,
                    // so we'll just try to re-push and let push_job handle duplicates

                    // Reconstruct job data
                    let job_data = serde_json::json!({
                        "fid": fid,
                        "type": job_type
                    })
                    .to_string();

                    // Try to push the job (push_job will return None if already active, but we know it's active)
                    // So we need to directly push to the queue and set status
                    // Actually, let's just create a new job_id and push it
                    let job_id = format!("job:{}:{}", job_type, uuid::Uuid::new_v4());

                    // Check if there's already a job with this job_key being processed
                    // by checking if the queue contains any job with matching data
                    // For simplicity, we'll just push a new job entry
                    redis::pipe()
                        .lpush(&queue_key, &job_id)
                        .ignore()
                        .set(self.key(&job_id), &job_data)
                        .ignore()
                        .expire(self.key(&job_id), 3600) // 1 hour TTL
                        .query_async::<()>(&mut conn)
                        .await
                        .map_err(|e| {
                            crate::SnapRagError::Custom(format!("Redis requeue error: {e}"))
                        })?;

                    info!("Re-queued pending job {} (new job_id: {})", job_key, job_id);
                    requeued_count += 1;
                }
            } else {
                // No status found, but job is in active_jobs - might need to be requeued
                // For now, skip these as they might be orphaned
            }
        }

        if requeued_count > 0 {
            info!("Re-queued {} pending job(s)", requeued_count);
        }

        Ok(requeued_count)
    }

    /// Get all active jobs with their status information
    /// Returns a vector of (job_key, status, result, processing_duration)
    pub async fn get_active_jobs_status(
        &self,
        queue_filter: Option<&str>,
    ) -> crate::Result<Vec<(String, String, Option<String>, Option<u64>)>> {
        let mut conn = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis connect error: {e}")))?;

        let active_jobs_key = self.key("active_jobs");

        // Get all active jobs
        let job_keys: Vec<String> = conn
            .smembers(&active_jobs_key)
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis SMEMBERS error: {e}")))?;

        let mut results = Vec::new();

        for job_key in job_keys {
            // Filter by queue if specified
            if let Some(queue_name) = queue_filter {
                if let Some((job_type, _)) = job_key.split_once(':') {
                    if job_type != queue_name {
                        continue;
                    }
                } else {
                    continue;
                }
            }

            // Get job status
            if let Ok(Some((status, result))) = self.get_job_status(&job_key).await {
                // Calculate processing duration if status is "processing"
                let processing_duration = if status == "processing" {
                    let status_key = self.key(&format!("job_status:{job_key}"));
                    let ttl: Result<i64, _> = conn.ttl(&status_key).await;
                    if let Ok(ttl) = ttl {
                        if ttl > 0 {
                            // TTL is remaining time, so processing duration = 86400 - ttl
                            Some((86400 - ttl) as u64)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                results.push((job_key.clone(), status, result, processing_duration));
            } else {
                // Job has no status but is in active_jobs
                results.push((job_key, "unknown".to_string(), None, None));
            }
        }

        // Sort by job_key for consistent output
        results.sort_by_key(|(job_key, _, _, _)| job_key.clone());

        Ok(results)
    }
}
