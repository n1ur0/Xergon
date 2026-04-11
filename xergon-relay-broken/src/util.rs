//! Shared utility functions used across handlers.

use axum::http::HeaderMap;

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
}
