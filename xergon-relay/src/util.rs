//! Shared utility functions used across handlers.

use axum::http::HeaderMap;
use sha2::{Digest, Sha256};

/// Extract client IP from headers (works behind reverse proxy).
///
/// Checks `x-forwarded-for` (last entry) first, then `x-real-ip`,
/// falling back to `"unknown"`.
pub fn extract_client_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next_back())
        .map(|s| s.trim().to_string())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string())
}

/// Hash an IP address with SHA-256 for privacy-safe storage.
pub fn hash_ip(ip: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ip.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn test_extract_client_ip_forwarded_for_multiple() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("1.2.3.4, 10.0.0.1, 192.168.1.100"),
        );
        assert_eq!(extract_client_ip(&headers), "192.168.1.100");
    }

    #[test]
    fn test_extract_client_ip_forwarded_for_single() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("10.0.0.1"));
        assert_eq!(extract_client_ip(&headers), "10.0.0.1");
    }

    #[test]
    fn test_extract_client_ip_forwarded_for_with_spaces() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("  1.2.3.4 ,  10.0.0.5  "),
        );
        assert_eq!(extract_client_ip(&headers), "10.0.0.5");
    }

    #[test]
    fn test_extract_client_ip_no_headers_returns_unknown() {
        let headers = HeaderMap::new();
        assert_eq!(extract_client_ip(&headers), "unknown");
    }

    #[test]
    fn test_extract_client_ip_falls_back_to_x_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", HeaderValue::from_static("172.16.0.1"));
        assert_eq!(extract_client_ip(&headers), "172.16.0.1");
    }

    #[test]
    fn test_extract_client_ip_forwarded_for_takes_priority_over_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", HeaderValue::from_static("10.0.0.1"));
        headers.insert("x-real-ip", HeaderValue::from_static("172.16.0.1"));
        assert_eq!(extract_client_ip(&headers), "10.0.0.1");
    }

    #[test]
    fn test_hash_ip_deterministic() {
        let h1 = hash_ip("192.168.1.1");
        let h2 = hash_ip("192.168.1.1");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_ip_different_inputs_different_outputs() {
        let h1 = hash_ip("192.168.1.1");
        let h2 = hash_ip("10.0.0.1");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_hash_ip_empty_string() {
        let h = hash_ip("");
        // SHA-256 of empty string is a known constant (64 hex chars)
        assert_eq!(h.len(), 64);
        assert_eq!(
            h,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_hash_ip_output_is_hex() {
        let h = hash_ip("1.2.3.4");
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(h.len(), 64);
    }
}
