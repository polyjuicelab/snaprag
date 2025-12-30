//! Session management for interactive chat

use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;

use serde::Deserialize;
use serde::Serialize;
use uuid::Uuid;

/// Chat message in conversation history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // "user" or "assistant"
    pub content: String,
    pub timestamp: u64,
}

/// Chat session data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub session_id: String,
    pub fid: i64,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub conversation_history: Vec<ChatMessage>,
    pub created_at: u64,
    pub last_activity: u64,
    pub context_limit: usize,
    pub temperature: f32,
}

impl ChatSession {
    /// Create a new chat session
    ///
    /// # Panics
    /// Panics if the system time is before UNIX_EPOCH (1970-01-01), which is impossible on modern systems
    #[must_use]
    pub fn new(
        fid: i64,
        username: Option<String>,
        display_name: Option<String>,
        context_limit: usize,
        temperature: f32,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            session_id: Uuid::new_v4().to_string(),
            fid,
            username,
            display_name,
            conversation_history: Vec::new(),
            created_at: now,
            last_activity: now,
            context_limit,
            temperature,
        }
    }

    /// Add a message to the conversation history
    ///
    /// # Panics
    /// Panics if the system time is before UNIX_EPOCH (1970-01-01), which is impossible on modern systems
    pub fn add_message(&mut self, role: &str, content: String) {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        self.conversation_history.push(ChatMessage {
            role: role.to_string(),
            content,
            timestamp,
        });

        self.last_activity = timestamp;

        // Keep only last context_limit messages to avoid context overflow
        if self.conversation_history.len() > self.context_limit {
            let excess = self.conversation_history.len() - self.context_limit;
            self.conversation_history.drain(0..excess);
        }
    }

    /// Check if the session has expired
    ///
    /// # Panics
    /// Panics if the system time is before UNIX_EPOCH (1970-01-01), which is impossible on modern systems
    #[must_use]
    pub fn is_expired(&self, timeout_secs: u64) -> bool {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        now - self.last_activity > timeout_secs
    }
}

/// Session manager with Redis backend
pub struct SessionManager {
    redis_client: Arc<crate::api::redis_client::RedisClient>,
    session_timeout: Duration,
}

impl SessionManager {
    /// Create new SessionManager with Redis client
    ///
    /// # Arguments
    ///
    /// * `session_timeout_secs` - Session timeout in seconds (default: 3600 = 1 hour)
    /// * `redis_client` - Redis client for session storage
    pub fn new(
        session_timeout_secs: u64,
        redis_client: Arc<crate::api::redis_client::RedisClient>,
    ) -> Self {
        let session_timeout = Duration::from_secs(session_timeout_secs);

        // Redis TTL handles expiration automatically, but we can log stats periodically
        let redis_client_clone = redis_client.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(300)).await; // Check every 5 minutes
                                                                    // Redis TTL handles expiration automatically
                                                                    // Could add session statistics logging here if needed
                let _ = redis_client_clone;
            }
        });

        Self {
            redis_client,
            session_timeout,
        }
    }

    /// Create session and store in Redis
    ///
    /// # Errors
    ///
    /// Returns error if Redis operation fails or session serialization fails
    pub async fn create_session(
        &self,
        fid: i64,
        username: Option<String>,
        display_name: Option<String>,
        context_limit: usize,
        temperature: f32,
    ) -> crate::Result<ChatSession> {
        let session = ChatSession::new(fid, username, display_name, context_limit, temperature);
        let session_key = format!("session:{}", session.session_id);
        let session_json = serde_json::to_string(&session).map_err(|e| {
            crate::SnapRagError::Custom(format!("Failed to serialize session: {}", e))
        })?;

        self.redis_client
            .set_json_with_ttl(&session_key, &session_json, Some(self.session_timeout))
            .await?;

        Ok(session)
    }

    /// Get session from Redis
    ///
    /// # Errors
    ///
    /// Returns error if Redis operation fails or session deserialization fails
    pub async fn get_session(&self, session_id: &str) -> crate::Result<Option<ChatSession>> {
        let session_key = format!("session:{}", session_id);
        match self.redis_client.get_json(&session_key).await? {
            Some(json) => {
                let session: ChatSession = serde_json::from_str(&json).map_err(|e| {
                    crate::SnapRagError::Custom(format!("Failed to deserialize session: {}", e))
                })?;
                Ok(Some(session))
            }
            None => Ok(None),
        }
    }

    /// Update session in Redis (refreshes TTL)
    ///
    /// # Errors
    ///
    /// Returns error if Redis operation fails or session serialization fails
    pub async fn update_session(&self, session: ChatSession) -> crate::Result<()> {
        let session_key = format!("session:{}", session.session_id);
        let session_json = serde_json::to_string(&session).map_err(|e| {
            crate::SnapRagError::Custom(format!("Failed to serialize session: {}", e))
        })?;

        self.redis_client
            .set_json_with_ttl(&session_key, &session_json, Some(self.session_timeout))
            .await?;

        Ok(())
    }

    /// Delete session from Redis
    ///
    /// # Errors
    ///
    /// Returns error if Redis operation fails
    pub async fn delete_session(&self, session_id: &str) -> crate::Result<()> {
        let session_key = format!("session:{}", session_id);
        self.redis_client.delete(&session_key).await?;
        Ok(())
    }

    /// Get session count (approximate)
    ///
    /// Note: This is expensive as it scans Redis keys, use sparingly
    ///
    /// # Errors
    ///
    /// Returns error if Redis operation fails
    pub async fn session_count(&self) -> crate::Result<usize> {
        // For now, return 0 as counting requires scanning all keys
        // Could implement a counter in Redis if needed
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    /// Helper to create a test Redis client
    fn create_test_redis_client() -> Option<Arc<crate::api::redis_client::RedisClient>> {
        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());

        let config = crate::config::RedisConfig {
            url: redis_url,
            namespace: "test:snaprag:session:".to_string(),
            default_ttl_secs: 3600,
            stale_threshold_secs: 300,
            refresh_channel: "test:snaprag.refresh".to_string(),
        };

        match crate::api::redis_client::RedisClient::connect(&config) {
            Ok(client) => Some(Arc::new(client)),
            Err(_) => {
                eprintln!("⚠️  Redis not available, skipping Redis session tests");
                None
            }
        }
    }

    /// Helper to create test SessionManager
    fn create_test_session_manager() -> Option<SessionManager> {
        create_test_redis_client().map(|redis| SessionManager::new(3600, redis))
    }

    /// Helper to cleanup test session
    async fn cleanup_test_session(manager: &SessionManager, session_id: &str) {
        let _ = manager.delete_session(session_id).await;
    }

    #[test]
    fn test_session_creation() {
        let session = ChatSession::new(
            99,
            Some("jesse.base.eth".to_string()),
            Some("Jesse Pollak".to_string()),
            20,
            0.7,
        );

        assert_eq!(session.fid, 99);
        assert_eq!(session.conversation_history.len(), 0);
        assert!(!session.session_id.is_empty());
    }

    #[test]
    fn test_add_message() {
        let mut session = ChatSession::new(99, None, None, 20, 0.7);

        session.add_message("user", "Hello".to_string());
        session.add_message("assistant", "Hi there!".to_string());

        assert_eq!(session.conversation_history.len(), 2);
        assert_eq!(session.conversation_history[0].role, "user");
        assert_eq!(session.conversation_history[1].role, "assistant");
    }

    #[test]
    fn test_message_limit() {
        let mut session = ChatSession::new(99, None, None, 20, 0.7);

        // Add 25 messages
        for i in 0..25 {
            session.add_message("user", format!("Message {i}"));
        }

        // Should only keep last 20
        assert_eq!(session.conversation_history.len(), 20);
    }

    #[tokio::test]
    #[ignore] // Requires Redis instance
    async fn test_session_create_and_get() {
        let manager = match create_test_session_manager() {
            Some(m) => m,
            None => return,
        };

        let session = manager
            .create_session(
                99,
                Some("test".to_string()),
                Some("Test".to_string()),
                20,
                0.7,
            )
            .await
            .expect("Failed to create session");

        let retrieved = manager
            .get_session(&session.session_id)
            .await
            .expect("Failed to get session")
            .expect("Session not found");

        assert_eq!(retrieved.session_id, session.session_id);
        assert_eq!(retrieved.fid, 99);
        assert_eq!(retrieved.username, Some("test".to_string()));

        cleanup_test_session(&manager, &session.session_id).await;
    }

    #[tokio::test]
    #[ignore] // Requires Redis instance
    async fn test_session_update() {
        let manager = match create_test_session_manager() {
            Some(m) => m,
            None => return,
        };

        let mut session = manager
            .create_session(99, None, None, 20, 0.7)
            .await
            .expect("Failed to create session");

        session.add_message("user", "Hello".to_string());

        manager
            .update_session(session.clone())
            .await
            .expect("Failed to update session");

        let retrieved = manager
            .get_session(&session.session_id)
            .await
            .expect("Failed to get session")
            .expect("Session not found");

        assert_eq!(retrieved.conversation_history.len(), 1);
        assert_eq!(retrieved.conversation_history[0].content, "Hello");

        cleanup_test_session(&manager, &session.session_id).await;
    }

    #[tokio::test]
    #[ignore] // Requires Redis instance
    async fn test_session_delete() {
        let manager = match create_test_session_manager() {
            Some(m) => m,
            None => return,
        };

        let session = manager
            .create_session(99, None, None, 20, 0.7)
            .await
            .expect("Failed to create session");

        manager
            .delete_session(&session.session_id)
            .await
            .expect("Failed to delete session");

        let retrieved = manager
            .get_session(&session.session_id)
            .await
            .expect("Failed to get session");

        assert!(retrieved.is_none());
    }

    #[tokio::test]
    #[ignore] // Requires Redis instance
    async fn test_session_expiration() {
        let manager = match create_test_session_manager() {
            Some(m) => m,
            None => return,
        };

        // Create session with short TTL (1 second)
        let redis_client = create_test_redis_client().expect("Redis not available");
        let short_timeout_manager = SessionManager::new(1, redis_client);

        let session = short_timeout_manager
            .create_session(99, None, None, 20, 0.7)
            .await
            .expect("Failed to create session");

        // Wait for expiration
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let retrieved = short_timeout_manager
            .get_session(&session.session_id)
            .await
            .expect("Failed to get session");

        assert!(retrieved.is_none(), "Session should have expired");
    }

    #[tokio::test]
    #[ignore] // Requires Redis instance
    async fn test_session_conversation_history() {
        let manager = match create_test_session_manager() {
            Some(m) => m,
            None => return,
        };

        let mut session = manager
            .create_session(99, None, None, 20, 0.7)
            .await
            .expect("Failed to create session");

        // Add multiple messages
        session.add_message("user", "Message 1".to_string());
        session.add_message("assistant", "Response 1".to_string());
        session.add_message("user", "Message 2".to_string());

        manager
            .update_session(session.clone())
            .await
            .expect("Failed to update session");

        let retrieved = manager
            .get_session(&session.session_id)
            .await
            .expect("Failed to get session")
            .expect("Session not found");

        assert_eq!(retrieved.conversation_history.len(), 3);
        assert_eq!(retrieved.conversation_history[0].content, "Message 1");
        assert_eq!(retrieved.conversation_history[1].content, "Response 1");
        assert_eq!(retrieved.conversation_history[2].content, "Message 2");

        cleanup_test_session(&manager, &session.session_id).await;
    }

    #[tokio::test]
    #[ignore] // Requires Redis instance
    async fn test_concurrent_session_updates() {
        let manager = match create_test_session_manager() {
            Some(m) => m,
            None => return,
        };

        let session = manager
            .create_session(99, None, None, 20, 0.7)
            .await
            .expect("Failed to create session");

        // Simulate concurrent updates
        let mut handles = Vec::new();
        for i in 0..5 {
            let manager_clone = SessionManager {
                redis_client: manager.redis_client.clone(),
                session_timeout: manager.session_timeout,
            };
            let session_id = session.session_id.clone();
            let handle = tokio::spawn(async move {
                let mut s = manager_clone
                    .get_session(&session_id)
                    .await
                    .expect("Failed to get session")
                    .expect("Session not found");
                s.add_message("user", format!("Concurrent message {i}"));
                manager_clone.update_session(s).await
            });
            handles.push(handle);
        }

        // Wait for all updates
        for handle in handles {
            handle.await.expect("Task failed").expect("Update failed");
        }

        let retrieved = manager
            .get_session(&session.session_id)
            .await
            .expect("Failed to get session")
            .expect("Session not found");

        // Should have 5 messages (last write wins for the final state)
        assert!(retrieved.conversation_history.len() >= 5);

        cleanup_test_session(&manager, &session.session_id).await;
    }
}
