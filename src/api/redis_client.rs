use std::time::Duration;

use redis::AsyncCommands;

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
        let client = redis::Client::open(config.url.as_str())
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis open error: {e}")))?;

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

        let queue_key = self.key(&format!("queue:{}", queue));
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
        let queue_key = self.key(&format!("queue:{}", queue));
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

    /// Set job status
    pub async fn set_job_status(
        &self,
        job_id: &str,
        status: &str,
        result: Option<&str>,
    ) -> crate::Result<()> {
        let status_key = self.key(&format!("job_status:{}", job_id));
        let mut conn = self
            .client
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| crate::SnapRagError::Custom(format!("Redis connect error: {e}")))?;

        let status_data = if let Some(result_data) = result {
            format!("{}|{}", status, result_data)
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
        let status_key = self.key(&format!("job_status:{}", job_id));
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
            let result = parts.get(1).map(|s| s.to_string());
            Ok(Some((status, result)))
        } else {
            Ok(None)
        }
    }

    /// Check if a job is already in queue or processing (for deduplication)
    /// Uses a simpler approach: check if there's an active job for this key
    pub async fn is_job_active(&self, job_key: &str) -> crate::Result<bool> {
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
}
