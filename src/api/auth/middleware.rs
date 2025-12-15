//! Authentication middleware for Axum

use axum::body::Body;
use axum::extract::Request;
use axum::extract::State;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::response::Response;
use tracing::debug;
use tracing::warn;

use super::extract_auth_headers;
use super::signature::build_signature_string;
use super::signature::extract_and_hash_body;
use super::signature::verify_signature;
use super::AuthState;

/// Authentication middleware
///
/// This middleware verifies request signatures to prevent replay attacks and unauthorized access.
/// It expects the following headers:
/// - `X-Token`: Token identifier
/// - `X-Timestamp`: Unix timestamp in seconds
/// - `X-Signature`: Base64-encoded HMAC-SHA256 signature
///
/// # Returns
/// - `Ok(Response)` if authentication succeeds
/// - `Err(StatusCode)` with 401 Unauthorized if authentication fails
///
/// # Errors
/// Returns `StatusCode::UNAUTHORIZED` if authentication fails (missing headers, invalid signature, etc.)
/// Returns `StatusCode::BAD_REQUEST` if request body cannot be read
pub async fn auth_middleware(
    State(auth_state): State<AuthState>,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Skip authentication if disabled
    if !auth_state.is_enabled() {
        debug!("Authentication disabled, skipping verification");
        return Ok(next.run(request).await);
    }

    // Skip authentication for OPTIONS requests (CORS preflight)
    if request.method() == axum::http::Method::OPTIONS {
        debug!("Skipping authentication for OPTIONS request (CORS preflight)");
        return Ok(next.run(request).await);
    }

    // Extract authentication headers
    let (token, timestamp, provided_signature) = match extract_auth_headers(request.headers()) {
        Ok(headers) => headers,
        Err(e) => {
            warn!("Authentication failed: {}", e);
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    // Verify timestamp (replay protection)
    if let Err(e) = auth_state.verify_timestamp(timestamp) {
        warn!("Timestamp validation failed: {}", e);
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Get secret for token
    let Some(secret) = auth_state.get_secret(&token) else {
        warn!("Token not found: {}", token);
        return Err(StatusCode::UNAUTHORIZED);
    };

    // Extract and hash request body
    // For GET requests and requests without body, use empty hash
    let body_hash = if request.method() == axum::http::Method::GET
        || request.method() == axum::http::Method::HEAD
        || request.method() == axum::http::Method::DELETE
    {
        String::new()
    } else {
        match extract_and_hash_body(&mut request).await {
            Ok(hash) => hash,
            Err(e) => {
                warn!("Failed to extract body hash: {}", e);
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    };

    // Build signature string
    // Note: build_signature_string strips /api prefix to match client-side signing
    let signature_string =
        build_signature_string(request.method(), request.uri(), &body_hash, timestamp);

    // Debug logging for signature verification
    debug!("Request URI path: {:?}", request.uri().path());
    debug!(
        "Signature verification - Token: {}, Timestamp: {}, Body hash: '{}', Sig string: {:?}",
        token, timestamp, body_hash, signature_string
    );
    debug!(
        "Provided signature: {}",
        &provided_signature.chars().take(30).collect::<String>()
    );

    // Verify signature
    if let Err(e) = verify_signature(secret, &signature_string, &provided_signature) {
        warn!("Signature verification failed: {}", e);
        warn!("Expected signature string: {:?}", signature_string);
        return Err(StatusCode::UNAUTHORIZED);
    }

    debug!("Authentication successful for token: {}", token);

    // Authentication successful, proceed with request
    Ok(next.run(request).await)
}
