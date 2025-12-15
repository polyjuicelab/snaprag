//! Authentication module for API request signing and verification

use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use axum::http::HeaderMap;
use hmac::Hmac;
use hmac::Mac;
use sha2::Sha256;

use crate::config::AuthConfig;

pub mod middleware;
pub mod signature;

pub use middleware::auth_middleware;
pub use signature::build_signature_string;
pub use signature::verify_signature;

/// Authentication state containing token/secret mappings
#[derive(Clone)]
pub struct AuthState {
    /// Token name to secret key mapping
    tokens: Arc<HashMap<String, String>>,
    /// Whether authentication is enabled
    enabled: bool,
    /// Time window for timestamp validation (seconds)
    time_window: u64,
}

impl AuthState {
    /// Create a new AuthState from configuration
    #[must_use]
    pub fn new(config: &AuthConfig) -> Self {
        Self {
            tokens: Arc::new(config.tokens.clone()),
            enabled: config.enabled,
            time_window: config.time_window_secs,
        }
    }

    /// Check if authentication is enabled
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get secret for a token name
    ///
    /// # Returns
    /// - `Some(secret)` if token exists
    /// - `None` if token not found
    #[must_use]
    pub fn get_secret(&self, token: &str) -> Option<&str> {
        self.tokens.get(token).map(String::as_str)
    }

    /// Get time window for timestamp validation
    #[must_use]
    pub const fn time_window(&self) -> u64 {
        self.time_window
    }

    /// Verify timestamp is within valid window
    ///
    /// # Returns
    /// - `Ok(())` if timestamp is valid
    /// - `Err(String)` with error message if invalid
    ///
    /// # Errors
    /// Returns an error if the timestamp is outside the configured time window
    ///
    /// # Panics
    /// This function may panic if the system time is before the Unix epoch (1970-01-01),
    /// which should never happen in practice on modern systems
    pub fn verify_timestamp(&self, timestamp: i64) -> Result<(), String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "System time is before Unix epoch".to_string())?
            .as_secs();

        // Safe cast: u64 to i64 for timestamp comparison
        // Timestamps are positive and well within i64 range
        let now_i64: i64 = now
            .try_into()
            .map_err(|_| "Timestamp overflow".to_string())?;
        let diff: i64 = (timestamp - now_i64).abs();

        // Safe cast: u64 to i64 for window comparison
        let window: i64 =
            i64::try_from(self.time_window).map_err(|_| "Time window overflow".to_string())?;

        if diff > window {
            Err(format!(
                "Timestamp out of window: {diff} seconds (max: {window} seconds)"
            ))
        } else {
            Ok(())
        }
    }
}

/// Extract authentication headers from request
///
/// # Returns
/// - `Ok((token, timestamp, signature))` if all headers present
/// - `Err(String)` with error message if any header missing
///
/// # Errors
/// Returns an error if:
/// - X-Token header is missing or invalid
/// - X-Timestamp header is missing or cannot be parsed as i64
/// - X-Signature header is missing or invalid
pub fn extract_auth_headers(headers: &HeaderMap) -> Result<(String, i64, String), String> {
    let token = headers
        .get("X-Token")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| "Missing X-Token header".to_string())?
        .to_string();

    let timestamp_str = headers
        .get("X-Timestamp")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| "Missing X-Timestamp header".to_string())?;

    let timestamp = timestamp_str
        .parse::<i64>()
        .map_err(|_| format!("Invalid timestamp format: {timestamp_str}"))?;

    let signature = headers
        .get("X-Signature")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| "Missing X-Signature header".to_string())?
        .to_string();

    Ok((token, timestamp, signature))
}
