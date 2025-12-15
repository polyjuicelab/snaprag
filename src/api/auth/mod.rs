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
            time_window: 300, // 5 minutes default
        }
    }

    /// Check if authentication is enabled
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get secret for a token name
    ///
    /// # Returns
    /// - `Some(secret)` if token exists
    /// - `None` if token not found
    pub fn get_secret(&self, token: &str) -> Option<&String> {
        self.tokens.get(token)
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
    pub fn verify_timestamp(&self, timestamp: i64) -> Result<(), String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let diff = (timestamp - now).abs();
        let window = self.time_window as i64;

        if diff > window {
            Err(format!(
                "Timestamp out of window: {} seconds (max: {} seconds)",
                diff, window
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
        .map_err(|_| format!("Invalid timestamp format: {}", timestamp_str))?;

    let signature = headers
        .get("X-Signature")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| "Missing X-Signature header".to_string())?
        .to_string();

    Ok((token, timestamp, signature))
}
