//! Signature generation and verification for authentication

use axum::body::Body;
use axum::extract::Request;
use axum::http::Method;
use axum::http::Uri;
use base64::engine::general_purpose;
use base64::Engine as _;
use hmac::Hmac;
use hmac::Mac;
use sha2::Digest;
use sha2::Sha256;

/// Build the signature string from request components
///
/// Format: `{method}\n{path}\n{query_string}\n{body_hash}\n{timestamp}`
///
/// # Arguments
/// - `method`: HTTP method (GET, POST, etc.)
/// - `uri`: Request URI (includes path and query)
/// - `body_hash`: SHA256 hash of request body (empty string if no body)
/// - `timestamp`: Unix timestamp in seconds
///
/// # Returns
/// Signature string ready for HMAC signing
pub fn build_signature_string(
    method: &Method,
    uri: &Uri,
    body_hash: &str,
    timestamp: i64,
) -> String {
    let path = if uri.path().starts_with("/api") {
        uri.path()
            .strip_prefix("/api")
            .unwrap_or_else(|| uri.path())
            .to_string()
    } else {
        uri.path().to_string()
    };
    let query = uri.query().unwrap_or("");

    format!("{method}\n{path}\n{query}\n{body_hash}\n{timestamp}")
}

/// Compute SHA256 hash of request body
///
/// # Arguments
/// - `body`: Request body bytes
///
/// # Returns
/// Hex-encoded SHA256 hash
#[must_use]
pub fn compute_body_hash(body: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body);
    let hash = hasher.finalize();
    hex::encode(hash)
}

/// Verify request signature
///
/// # Arguments
/// - `secret`: Secret key for HMAC
/// - `signature_string`: The string to sign (from `build_signature_string`)
/// - `provided_signature`: Base64-encoded signature from request header
///
/// # Returns
/// - `Ok(())` if signature is valid
/// - `Err(String)` with error message if invalid
///
/// # Errors
/// Returns an error if:
/// - The secret format is invalid (not hex-encoded)
/// - HMAC creation fails
/// - Signature length doesn't match
/// - Signature doesn't match the computed signature
pub fn verify_signature(
    secret: &str,
    signature_string: &str,
    provided_signature: &str,
) -> Result<(), String> {
    // Decode secret (expecting hex-encoded)
    let secret_bytes = hex::decode(secret).map_err(|e| format!("Invalid secret format: {e}"))?;

    // Compute HMAC-SHA256
    let mut mac = Hmac::<Sha256>::new_from_slice(&secret_bytes)
        .map_err(|e| format!("Failed to create HMAC: {e}"))?;
    mac.update(signature_string.as_bytes());
    let computed_signature = mac.finalize().into_bytes();

    // Encode to base64
    let computed_b64 = general_purpose::STANDARD.encode(computed_signature);

    // Constant-time comparison to prevent timing attacks
    if computed_b64.len() != provided_signature.len() {
        return Err("Signature length mismatch".to_string());
    }

    let mut result = 0u8;
    for (a, b) in computed_b64.bytes().zip(provided_signature.bytes()) {
        result |= a ^ b;
    }

    if result == 0 {
        Ok(())
    } else {
        Err("Invalid signature".to_string())
    }
}

/// Extract and hash request body
///
/// This function reads the request body and computes its hash.
/// Note: The body can only be read once, so this should be called
/// before the body is consumed by handlers.
///
/// # Errors
/// Returns an error if the request body cannot be read or hashing fails
pub async fn extract_and_hash_body(request: &mut Request) -> Result<String, String> {
    // Take ownership of the body to read it
    let body = std::mem::replace(request.body_mut(), Body::empty());

    // Use axum's to_bytes helper to read the entire body
    let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(e) => {
            // Restore body on error
            *request.body_mut() = Body::empty();
            return Err(format!("Failed to read request body: {e}"));
        }
    };

    let hash = if body_bytes.is_empty() {
        String::new()
    } else {
        compute_body_hash(&body_bytes)
    };

    // Reconstruct the body for downstream handlers
    *request.body_mut() = Body::from(body_bytes);

    Ok(hash)
}

#[cfg(test)]
mod tests {
    use axum::http::Method;

    use super::*;

    #[test]
    fn test_build_signature_string() {
        let uri = Uri::from_static("/api/profiles/123?limit=10");
        let method = &Method::GET;
        let body_hash = "";
        let timestamp = 1_234_567_890;

        let sig_string = build_signature_string(method, &uri, body_hash, timestamp);
        let expected = "GET\n/api/profiles/123\nlimit=10\n\n1234567890";
        assert_eq!(sig_string, expected);
    }

    #[test]
    fn test_compute_body_hash() {
        let body = b"{\"test\": \"data\"}";
        let hash = compute_body_hash(body);
        // SHA256 of the test data
        assert!(!hash.is_empty());
        assert_eq!(hash.len(), 64); // Hex-encoded SHA256 is 64 chars
    }

    #[test]
    fn test_verify_signature() {
        let secret = hex::encode(b"test_secret_key_32_bytes_long!!");
        let signature_string = "GET\n/api/test\n\n\n1234567890";

        // Compute expected signature
        let secret_bytes = hex::decode(&secret).unwrap();
        let mut mac = Hmac::<Sha256>::new_from_slice(&secret_bytes).unwrap();
        mac.update(signature_string.as_bytes());
        let computed = general_purpose::STANDARD.encode(mac.finalize().into_bytes());

        // Verify it matches
        let result = verify_signature(&secret, signature_string, &computed);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_signature_invalid() {
        let secret = hex::encode(b"test_secret_key_32_bytes_long!!");
        let signature_string = "GET\n/api/test\n\n\n1234567890";
        let invalid_signature = "invalid_signature";

        let result = verify_signature(&secret, signature_string, invalid_signature);
        assert!(result.is_err());
    }
}
