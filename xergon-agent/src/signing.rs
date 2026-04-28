//! Request signing for Xergon relay API authentication
//!
//! Signs requests using HMAC-SHA256 with the wallet's secret key.
//!
//! Signing scheme:
//!   signature = HMAC-SHA256(secret_key, timestamp + method + path + body_hash)
//!
//! Headers sent to relay:
//!   X-Xergon-Timestamp: <unix_ms>
//!   X-Xergon-Public-Key: <hex>
//!   X-Xergon-Signature: <hex>

use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Compute the signature for a request.
///
/// # Arguments
/// * `secret_key_hex` - The wallet's secret key as hex string
/// * `timestamp` - Unix timestamp in milliseconds
/// * `method` - HTTP method (e.g., "GET", "POST")
/// * `path` - URL path (e.g., "/v1/chat/completions")
/// * `body` - Request body bytes (empty for GET)
///
/// # Returns
/// The signature as a hex string
pub fn sign_request(
    secret_key_hex: &str,
    timestamp: u64,
    method: &str,
    path: &str,
    body: &[u8],
) -> String {
    let secret_key_bytes = hex::decode(secret_key_hex)
        .expect("Invalid secret key hex");

    // Compute body hash
    let body_hash = if body.is_empty() {
        hex::encode(sha256_hash(b""))
    } else {
        hex::encode(sha256_hash(body))
    };

    // Build the signing payload
    let payload = format!("{}{}{}{}", timestamp, method, path, body_hash);

    // HMAC-SHA256
    let mut mac = HmacSha256::new_from_slice(&secret_key_bytes)
        .expect("HMAC key error");
    mac.update(payload.as_bytes());
    let result = mac.finalize();
    hex::encode(result.into_bytes())
}

/// Generate a signed API token for use with OpenAI-compatible SDKs.
///
/// Token format: `xergon_{public_key}.{signature}.{timestamp}`
///
/// The token is valid for `expiry_secs` seconds from `timestamp`.
pub fn generate_token(
    secret_key_hex: &str,
    public_key_hex: &str,
    expiry_secs: u64,
) -> (String, u64) {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let expiry_timestamp = timestamp + (expiry_secs * 1000);

    // Sign: token:{timestamp}:{expiry_timestamp}
    let message = format!("token:{}:{}", timestamp, expiry_timestamp);
    let message_bytes = message.as_bytes();

    let secret_key_bytes = hex::decode(secret_key_hex)
        .expect("Invalid secret key hex");

    let mut mac = HmacSha256::new_from_slice(&secret_key_bytes)
        .expect("HMAC key error");
    mac.update(message_bytes);
    let result = mac.finalize();
    let signature = hex::encode(result.into_bytes());

    let token = format!("xergon_{}.{}.{}", public_key_hex, signature, timestamp);

    (token, timestamp)
}

/// SHA-256 hash (32 bytes).
fn sha256_hash(data: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_request_deterministic() {
        let secret = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let ts = 1700000000000u64;

        let sig1 = sign_request(secret, ts, "GET", "/v1/models", b"");
        let sig2 = sign_request(secret, ts, "GET", "/v1/models", b"");

        assert_eq!(sig1, sig2);
        assert!(!sig1.is_empty());
    }

    #[test]
    fn test_sign_request_different_methods() {
        let secret = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let ts = 1700000000000u64;

        let sig_get = sign_request(secret, ts, "GET", "/v1/models", b"");
        let sig_post = sign_request(secret, ts, "POST", "/v1/models", b"{}");

        assert_ne!(sig_get, sig_post);
    }

    #[test]
    fn test_sign_request_with_body() {
        let secret = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";
        let ts = 1700000000000u64;
        let body = br#"{"model":"qwen","messages":[{"role":"user","content":"hi"}]}"#;

        let sig = sign_request(secret, ts, "POST", "/v1/chat/completions", body);
        assert!(!sig.is_empty());
        // Signature should be 64 hex chars (32 bytes)
        assert_eq!(sig.len(), 64);
    }

    #[test]
    fn test_generate_token_format() {
        let secret = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let pubkey = "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789";

        let (token, ts) = generate_token(secret, pubkey, 3600);

        // Format: xergon_{pubkey}.{signature}.{timestamp}
        assert!(token.starts_with("xergon_"));
        assert!(token.contains(&pubkey));
        assert!(token.ends_with(&ts.to_string()));
        assert!(ts > 0);
    }
}
