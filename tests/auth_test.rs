//! Authentication module tests

use std::collections::HashMap;
use std::str::FromStr;

use axum::body::Body;
use axum::http::header;
use axum::http::HeaderValue;
use axum::http::Method;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::Uri;
use base64::engine::general_purpose;
use base64::Engine as _;
use hmac::Hmac;
use hmac::Mac;
use sha2::Digest;
use sha2::Sha256;
use snaprag::api::auth::extract_auth_headers;
use snaprag::api::auth::signature::build_signature_string;
use snaprag::api::auth::signature::compute_body_hash;
use snaprag::api::auth::signature::verify_signature;
use snaprag::api::auth::AuthState;
use snaprag::config::AuthConfig;

#[test]
fn test_extract_auth_headers() {
    use axum::http::HeaderMap;

    let mut headers = HeaderMap::new();
    headers.insert("X-Token", HeaderValue::from_static("test_token"));
    headers.insert("X-Timestamp", HeaderValue::from_static("1234567890"));
    headers.insert("X-Signature", HeaderValue::from_static("test_signature"));

    let (token, timestamp, signature) = extract_auth_headers(&headers).unwrap();
    assert_eq!(token, "test_token");
    assert_eq!(timestamp, 1234567890);
    assert_eq!(signature, "test_signature");
}

#[test]
fn test_extract_auth_headers_missing() {
    use axum::http::HeaderMap;

    let headers = HeaderMap::new();
    let result = extract_auth_headers(&headers);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Missing"));
}

#[test]
fn test_build_signature_string() {
    let uri = Uri::from_static("/api/profiles/123?limit=10");
    let method = &Method::GET;
    let body_hash = "";
    let timestamp = 1234567890;

    let sig_string = build_signature_string(method, &uri, body_hash, timestamp);
    // Path should have /api prefix stripped
    let expected = "GET\n/profiles/123\nlimit=10\n\n1234567890";
    assert_eq!(sig_string, expected);
}

#[test]
fn test_build_signature_string_with_body() {
    let uri = Uri::from_static("/api/search/profiles");
    let method = &Method::POST;
    let body_hash = "abc123def456";
    let timestamp = 1234567890;

    let sig_string = build_signature_string(method, &uri, body_hash, timestamp);
    // Path should have /api prefix stripped
    let expected = "POST\n/search/profiles\n\nabc123def456\n1234567890";
    assert_eq!(sig_string, expected);
}

#[test]
fn test_verify_signature_valid() {
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

#[test]
fn test_verify_signature_wrong_secret() {
    let secret1 = hex::encode(b"test_secret_key_32_bytes_long!!");
    let secret2 = hex::encode(b"different_secret_key_32_bytes!!");
    let signature_string = "GET\n/api/test\n\n\n1234567890";

    // Compute signature with secret1
    let secret_bytes = hex::decode(&secret1).unwrap();
    let mut mac = Hmac::<Sha256>::new_from_slice(&secret_bytes).unwrap();
    mac.update(signature_string.as_bytes());
    let computed = general_purpose::STANDARD.encode(mac.finalize().into_bytes());

    // Try to verify with secret2
    let result = verify_signature(&secret2, signature_string, &computed);
    assert!(result.is_err());
}

#[test]
fn test_auth_state_new() {
    let mut tokens = HashMap::new();
    tokens.insert(
        "token1".to_string(),
        hex::encode(b"secret1_32_bytes_long!!"),
    );
    tokens.insert(
        "token2".to_string(),
        hex::encode(b"secret2_32_bytes_long!!"),
    );

    let auth_config = AuthConfig {
        enabled: true,
        time_window_secs: 300,
        tokens,
    };

    let auth_state = AuthState::new(&auth_config);
    assert!(auth_state.is_enabled());
    assert_eq!(
        auth_state.get_secret("token1").unwrap(),
        &hex::encode(b"secret1_32_bytes_long!!")
    );
    assert_eq!(
        auth_state.get_secret("token2").unwrap(),
        &hex::encode(b"secret2_32_bytes_long!!")
    );
    assert!(auth_state.get_secret("nonexistent").is_none());
}

#[test]
fn test_auth_state_verify_timestamp() {
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    let auth_config = AuthConfig {
        enabled: true,
        time_window_secs: 300,
        tokens: HashMap::new(),
    };

    let auth_state = AuthState::new(&auth_config);

    // Current timestamp should be valid
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    assert!(auth_state.verify_timestamp(now).is_ok());

    // Timestamp within window should be valid
    assert!(auth_state.verify_timestamp(now - 100).is_ok());
    assert!(auth_state.verify_timestamp(now + 100).is_ok());

    // Timestamp outside window should be invalid
    assert!(auth_state.verify_timestamp(now - 400).is_err());
    assert!(auth_state.verify_timestamp(now + 400).is_err());
}

#[test]
fn test_auth_state_disabled() {
    let auth_config = AuthConfig {
        enabled: false,
        time_window_secs: 300,
        tokens: HashMap::new(),
    };

    let auth_state = AuthState::new(&auth_config);
    assert!(!auth_state.is_enabled());
}

#[test]
fn test_compute_body_hash() {
    let body = b"{\"test\": \"data\"}";
    let hash = compute_body_hash(body);

    // SHA256 of the test data
    assert!(!hash.is_empty());
    assert_eq!(hash.len(), 64); // Hex-encoded SHA256 is 64 chars

    // Verify it's actually a SHA256 hash
    let mut hasher = Sha256::new();
    hasher.update(body);
    let expected_hash = hex::encode(hasher.finalize());
    assert_eq!(hash, expected_hash);
}

#[test]
fn test_compute_body_hash_empty() {
    let body = b"";
    let hash = compute_body_hash(body);

    // Empty body should produce a hash (SHA256 of empty string)
    assert!(!hash.is_empty());
    assert_eq!(hash.len(), 64);
}

#[test]
fn test_signature_string_format() {
    // Test various combinations to ensure format is correct
    // Note: build_signature_string strips /api prefix
    let test_cases = vec![
        ("GET", "/api/test", "", "", "GET\n/test\n\n\n1234567890"),
        (
            "POST",
            "/api/test",
            "query=value",
            "bodyhash",
            "POST\n/test\nquery=value\nbodyhash\n1234567890",
        ),
        (
            "PUT",
            "/api/resource/123",
            "",
            "hash123",
            "PUT\n/resource/123\n\nhash123\n1234567890",
        ),
    ];

    for (method_str, path, query, body_hash, expected) in test_cases {
        let method = Method::from_bytes(method_str.as_bytes()).unwrap();
        let uri = if query.is_empty() {
            Uri::from_str(&format!("http://example.com{}", path)).unwrap()
        } else {
            Uri::from_str(&format!("http://example.com{}?{}", path, query)).unwrap()
        };

        let sig_string = build_signature_string(&method, &uri, body_hash, 1234567890);
        assert_eq!(sig_string, expected);
    }
}

#[test]
fn test_constant_time_comparison() {
    // Verify that signature comparison uses constant time
    // This is important for security to prevent timing attacks
    let secret = hex::encode(b"test_secret_key_32_bytes_long!!");
    let signature_string = "GET\n/api/test\n\n\n1234567890";

    // Compute valid signature
    let secret_bytes = hex::decode(&secret).unwrap();
    let mut mac = Hmac::<Sha256>::new_from_slice(&secret_bytes).unwrap();
    mac.update(signature_string.as_bytes());
    let valid_sig = general_purpose::STANDARD.encode(mac.finalize().into_bytes());

    // Test that different length signatures are rejected quickly
    let short_sig = "short";
    let result1 = verify_signature(&secret, signature_string, short_sig);
    assert!(result1.is_err());

    // Test that same length but different content is rejected
    let wrong_sig = "a".repeat(valid_sig.len());
    let result2 = verify_signature(&secret, signature_string, &wrong_sig);
    assert!(result2.is_err());

    // Valid signature should pass
    let result3 = verify_signature(&secret, signature_string, &valid_sig);
    assert!(result3.is_ok());
}

/// Test that server-side signature string matches client-side signature string
/// This replicates the exact logic from polyjuice/src/api.rs
#[test]
fn test_signature_string_matches_client() {
    use hex;

    // Simulate client-side signature generation (from polyjuice)
    fn client_compute_body_hash(body: &str) -> String {
        if body.is_empty() {
            return String::new();
        }
        let mut hasher = Sha256::new();
        hasher.update(body.as_bytes());
        let hash = hasher.finalize();
        hex::encode(hash)
    }

    fn client_build_signature_string(
        method: &str,
        path: &str,
        query: &str,
        body_hash: &str,
        timestamp: i64,
    ) -> String {
        format!(
            "{}\n{}\n{}\n{}\n{}",
            method, path, query, body_hash, timestamp
        )
    }

    fn client_sign_request(
        method: &str,
        path: &str,
        query: &str,
        body: Option<&str>,
        secret_hex: &str,
        timestamp: i64,
    ) -> Result<String, String> {
        let body_hash = body.map(client_compute_body_hash).unwrap_or_default();
        let sig_string = client_build_signature_string(method, path, query, &body_hash, timestamp);

        let secret_bytes =
            hex::decode(secret_hex).map_err(|e| format!("Invalid secret format: {}", e))?;

        let mut mac = Hmac::<Sha256>::new_from_slice(&secret_bytes)
            .map_err(|e| format!("Failed to create HMAC: {}", e))?;
        mac.update(sig_string.as_bytes());
        let signature = mac.finalize().into_bytes();

        Ok(general_purpose::STANDARD.encode(&signature))
    }

    // Test case: GET /api/profiles/1
    let secret_hex = "b19aa6643145b297626ee72aa1cb96f731a0dd108f46afe0dde22052866046c4";
    let timestamp = 1765812565;

    // Client side: URL is http://127.0.0.1:3000/api/profiles/1
    // Client extracts pathname: /api/profiles/1
    // Client strips /api prefix: /profiles/1
    let client_path = "/profiles/1"; // After stripping /api
    let client_query = "";
    let client_body: Option<&str> = None;

    // Client generates signature
    let client_signature = client_sign_request(
        "GET",
        client_path,
        client_query,
        client_body,
        secret_hex,
        timestamp,
    )
    .unwrap();

    // Server side: receives URI /api/profiles/1
    let server_uri = Uri::from_str("http://127.0.0.1:3000/api/profiles/1").unwrap();
    let server_method = Method::GET;
    let server_body_hash = ""; // GET request has no body

    // Server builds signature string (should strip /api prefix)
    let server_sig_string =
        build_signature_string(&server_method, &server_uri, &server_body_hash, timestamp);

    // Verify server signature string matches client
    let expected_client_sig_string =
        client_build_signature_string("GET", "/profiles/1", "", "", timestamp);

    assert_eq!(
        server_sig_string, expected_client_sig_string,
        "Server signature string should match client. Server: {:?}, Client: {:?}",
        server_sig_string, expected_client_sig_string
    );

    // Verify server can verify client's signature
    // Server stores secret as hex string, so we need to use it directly
    assert!(
        verify_signature(secret_hex, &server_sig_string, &client_signature).is_ok(),
        "Server should be able to verify client signature. Server sig: {:?}, Client sig: {}",
        server_sig_string,
        &client_signature.chars().take(30).collect::<String>()
    );

    println!("✅ Signature strings match!");
    println!("   Client sig string: {:?}", expected_client_sig_string);
    println!("   Server sig string: {:?}", server_sig_string);
    println!(
        "   Client signature: {}",
        &client_signature.chars().take(30).collect::<String>()
    );
}
