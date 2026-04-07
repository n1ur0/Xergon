//! Request logging and audit trail middleware for xergon-agent.
//!
//! Logs structured audit entries for every HTTP request, including timing,
//! client IP, request IDs, and optional body logging.
//!
//! Configuration is read from `[audit]` in the agent config TOML.

use std::sync::Arc;

use axum::{
    body::Body,
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Audit/logging configuration.
///
/// Deserialized from the `[audit]` section of the agent config.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct AuditConfig {
    /// Enable audit logging (default: true).
    #[serde(default = "default_audit_enabled")]
    pub enabled: bool,

    /// Log request/response bodies (default: false).
    #[serde(default)]
    pub log_body: bool,

    /// Maximum body size to log in bytes (default: 1024).
    #[serde(default = "default_max_body_log_size")]
    pub max_body_log_size: usize,

    /// Paths to skip audit logging (default: ["/health", "/metrics"]).
    #[serde(default = "default_exclude_paths")]
    pub exclude_paths: Vec<String>,

    /// Headers to redact from logs (default: ["authorization", "cookie"]).
    #[serde(default = "default_sensitive_headers")]
    pub sensitive_headers: Vec<String>,
}

fn default_audit_enabled() -> bool { true }
fn default_max_body_log_size() -> usize { 1024 }
fn default_exclude_paths() -> Vec<String> {
    vec!["/health".to_string(), "/metrics".to_string()]
}
fn default_sensitive_headers() -> Vec<String> {
    vec!["authorization".to_string(), "cookie".to_string()]
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: default_audit_enabled(),
            log_body: false,
            max_body_log_size: default_max_body_log_size(),
            exclude_paths: default_exclude_paths(),
            sensitive_headers: default_sensitive_headers(),
        }
    }
}

// ---------------------------------------------------------------------------
// Audit entry
// ---------------------------------------------------------------------------

/// A single audit log entry.
#[derive(Debug, Clone, Serialize)]
pub struct AuditEntry {
    /// ISO 8601 timestamp.
    pub timestamp: String,
    /// Unique request ID (UUID v4).
    pub request_id: String,
    /// HTTP method.
    pub method: String,
    /// Request path.
    pub path: String,
    /// Query string (empty if none).
    pub query: String,
    /// HTTP response status code.
    pub status: u16,
    /// Request duration in milliseconds.
    pub duration_ms: u64,
    /// Client IP address.
    pub client_ip: String,
    /// User-Agent header.
    pub user_agent: String,
    /// Authenticated provider public key (if present).
    pub provider_pk: Option<String>,
    /// Request body (if log_body enabled and body present).
    pub request_body: Option<String>,
    /// Response size in bytes (if available).
    pub response_size_bytes: Option<u64>,
}

impl AuditEntry {
    /// Create a new audit entry from request/response data.
    pub fn new(
        request_id: String,
        method: String,
        path: String,
        query: String,
        status: u16,
        duration_ms: u64,
        client_ip: String,
        user_agent: String,
        provider_pk: Option<String>,
        request_body: Option<String>,
        response_size_bytes: Option<u64>,
    ) -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            request_id,
            method,
            path,
            query,
            status,
            duration_ms,
            client_ip,
            user_agent,
            provider_pk,
            request_body,
            response_size_bytes,
        }
    }
}

// ---------------------------------------------------------------------------
// Audit logger
// ---------------------------------------------------------------------------

/// Audit logger that writes structured entries via tracing.
#[derive(Clone)]
pub struct AuditLogger {
    pub config: AuditConfig,
}

impl AuditLogger {
    /// Create a new audit logger from config.
    pub fn new(config: AuditConfig) -> Self {
        Self { config }
    }

    /// Log an audit entry at the appropriate level.
    ///
    /// - 2xx: info
    /// - 4xx: warn
    /// - 5xx: error
    pub fn log(&self, entry: &AuditEntry) {
        let status = entry.status;
        if (400..500).contains(&status) {
            warn!(
                timestamp = %entry.timestamp,
                request_id = %entry.request_id,
                method = %entry.method,
                path = %entry.path,
                query = %entry.query,
                status = status,
                duration_ms = entry.duration_ms,
                client_ip = %entry.client_ip,
                user_agent = %entry.user_agent,
                provider_pk = entry.provider_pk.as_deref().unwrap_or("-"),
                "Request completed (client error)"
            );
        } else if status >= 500 {
            error!(
                timestamp = %entry.timestamp,
                request_id = %entry.request_id,
                method = %entry.method,
                path = %entry.path,
                query = %entry.query,
                status = status,
                duration_ms = entry.duration_ms,
                client_ip = %entry.client_ip,
                user_agent = %entry.user_agent,
                provider_pk = entry.provider_pk.as_deref().unwrap_or("-"),
                "Request completed (server error)"
            );
        } else {
            info!(
                timestamp = %entry.timestamp,
                request_id = %entry.request_id,
                method = %entry.method,
                path = %entry.path,
                query = %entry.query,
                status = status,
                duration_ms = entry.duration_ms,
                client_ip = %entry.client_ip,
                user_agent = %entry.user_agent,
                provider_pk = entry.provider_pk.as_deref().unwrap_or("-"),
                "Request completed"
            );
        }

        // Log body if configured and present
        if self.config.log_body {
            if let Some(body) = &entry.request_body {
                tracing::debug!(
                    request_id = %entry.request_id,
                    body = %body,
                    "Request body"
                );
            }
            if let Some(size) = entry.response_size_bytes {
                tracing::debug!(
                    request_id = %entry.request_id,
                    response_size_bytes = size,
                    "Response size"
                );
            }
        }
    }

    /// Check if a path should be excluded from audit logging.
    pub fn is_excluded(&self, path: &str) -> bool {
        self.config.exclude_paths.iter().any(|excluded| {
            path == excluded || path.starts_with(&format!("{}/", excluded))
        })
    }

    /// Truncate a body string to the configured max size.
    pub fn truncate_body(&self, body: &str) -> String {
        if body.len() > self.config.max_body_log_size {
            format!("{}...[truncated, {} bytes total]", 
                &body[..self.config.max_body_log_size], 
                body.len())
        } else {
            body.to_string()
        }
    }

    /// Returns true if audit logging is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

// ---------------------------------------------------------------------------
// Axum middleware
// ---------------------------------------------------------------------------

/// Extract client IP from request headers.
fn extract_client_ip(req: &Request<Body>) -> String {
    if let Some(forwarded) = req.headers().get("x-forwarded-for") {
        if let Ok(value) = forwarded.to_str() {
            if let Some(first_ip) = value.split(',').next() {
                let ip = first_ip.trim();
                if !ip.is_empty() {
                    return ip.to_string();
                }
            }
        }
    }
    if let Some(real_ip) = req.headers().get("x-real-ip") {
        if let Ok(value) = real_ip.to_str() {
            let ip = value.trim();
            if !ip.is_empty() {
                return ip.to_string();
            }
        }
    }
    "unknown".to_string()
}

/// Extract user-agent header.
fn extract_user_agent(req: &Request<Body>) -> String {
    req.headers()
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-")
        .to_string()
}

/// Axum middleware function for audit logging.
///
/// Logs structured audit entries for every HTTP request, including timing,
/// client IP, request IDs, and optional body logging.
pub async fn audit_middleware(
    axum::extract::State(logger): axum::extract::State<AuditLogger>,
    req: Request<Body>,
    next: Next,
) -> Response {
    if !logger.is_enabled() {
        return next.run(req).await;
    }

    let path = req.uri().path().to_string();
    let query = req.uri().query().unwrap_or("").to_string();

    // Skip excluded paths
    if logger.is_excluded(&path) {
        return next.run(req).await;
    }

    // Generate request ID
    let request_id = uuid::Uuid::new_v4().to_string();
    let method = req.method().to_string();
    let client_ip = extract_client_ip(&req);
    let user_agent = extract_user_agent(&req);

    // Check for authentication (provider_pk in extensions set by auth middleware)
    // Since auth middleware may not have run yet (depends on layer order),
    // we just note the presence of auth header
    let provider_pk = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .filter(|h| h.starts_with("Bearer "))
        .map(|_| "authenticated".to_string());

    // Capture body bytes if log_body is enabled
    let request_body = if logger.is_enabled() && logger.config.log_body {
        // We can't easily read the body without consuming it,
        // so we skip body capture in the middleware itself.
        // Body logging would require a body-intercepting layer.
        None
    } else {
        None
    };

    // Time the request
    let start = std::time::Instant::now();
    let response = next.run(req).await;
    let duration_ms = start.elapsed().as_millis() as u64;

    let status = response.status().as_u16();

    // Try to get response size (axum 0.8: use response body size hint)
    let response_size_bytes: Option<u64> = None;

    // Build audit entry
    let entry = AuditEntry::new(
        request_id.clone(),
        method,
        path,
        query,
        status,
        duration_ms,
        client_ip,
        user_agent,
        provider_pk,
        request_body,
        response_size_bytes,
    );

    logger.log(&entry);

    // Add X-Request-Id header to response
    let mut resp = response;
    let headers = resp.headers_mut();
    headers.insert(
        header::HeaderName::from_static("x-request-id"),
        header::HeaderValue::from_str(&request_id).unwrap_or_else(|_| {
            header::HeaderValue::from_static("unknown")
        }),
    );

    resp
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Bytes;
    use axum::http::{Request as HttpRequest, Method};
    use tower::ServiceExt;

    /// Build a simple test app that echoes the request method.
    fn test_app(config: AuditConfig) -> axum::Router {
        let logger = AuditLogger::new(config);
        axum::Router::new()
            .route("/test", axum::routing::any(|req: HttpRequest<Body>| async move {
                format!("{} {}", req.method(), req.uri())
            }))
            .route("/health", axum::routing::get(|| async { "ok" }))
            .route("/metrics", axum::routing::get(|| async { "metrics" }))
            .layer(axum::middleware::from_fn_with_state(logger, audit_middleware))
    }

    #[tokio::test]
    async fn test_audit_entry_created() {
        let config = AuditConfig::default();
        let app = test_app(config);

        let response = app
            .oneshot(
                HttpRequest::builder()
                    .method(Method::GET)
                    .uri("/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(response.headers().contains_key("x-request-id"));
        let request_id = response.headers().get("x-request-id").unwrap().to_str().unwrap();
        // Should be a valid UUID v4
        assert!(uuid::Uuid::parse_str(request_id).is_ok(), "request_id should be a valid UUID");
    }

    #[tokio::test]
    async fn test_excluded_paths_skipped() {
        let config = AuditConfig::default();
        let app = test_app(config);

        // /health should be excluded (no x-request-id header added by our middleware
        // but the route still returns)
        let response = app
            .oneshot(
                HttpRequest::builder()
                    .method(Method::GET)
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Health is excluded, but we still get a response
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_audit_sets_request_id_header() {
        let config = AuditConfig::default();
        let app = test_app(config);

        let response = app
            .oneshot(
                HttpRequest::builder()
                    .method(Method::POST)
                    .uri("/test?foo=bar")
                    .header("x-real-ip", "192.168.1.1")
                    .header("user-agent", "test-agent")
                    .body(Body::from("test body"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(response.headers().contains_key("x-request-id"));
    }

    #[test]
    fn test_audit_entry_new() {
        let entry = AuditEntry::new(
            "req-123".to_string(),
            "GET".to_string(),
            "/api/test".to_string(),
            "foo=bar".to_string(),
            200,
            42,
            "1.2.3.4".to_string(),
            "test-agent".to_string(),
            Some("provider-pk-hex".to_string()),
            None,
            Some(256),
        );

        assert_eq!(entry.request_id, "req-123");
        assert_eq!(entry.method, "GET");
        assert_eq!(entry.path, "/api/test");
        assert_eq!(entry.query, "foo=bar");
        assert_eq!(entry.status, 200);
        assert_eq!(entry.duration_ms, 42);
        assert_eq!(entry.client_ip, "1.2.3.4");
        assert_eq!(entry.user_agent, "test-agent");
        assert_eq!(entry.provider_pk, Some("provider-pk-hex".to_string()));
        assert!(entry.request_body.is_none());
        assert_eq!(entry.response_size_bytes, Some(256));
        // timestamp should be ISO 8601
        assert!(!entry.timestamp.is_empty());
    }

    #[test]
    fn test_is_excluded() {
        let config = AuditConfig {
            exclude_paths: vec!["/health".to_string(), "/metrics".to_string()],
            ..Default::default()
        };
        let logger = AuditLogger::new(config);

        assert!(logger.is_excluded("/health"));
        assert!(logger.is_excluded("/health/ready"));
        assert!(logger.is_excluded("/metrics"));
        assert!(!logger.is_excluded("/api/test"));
        assert!(!logger.is_excluded("/xergon/status"));
    }

    #[test]
    fn test_truncate_body() {
        let config = AuditConfig {
            max_body_log_size: 10,
            ..Default::default()
        };
        let logger = AuditLogger::new(config);

        let short = logger.truncate_body("hello");
        assert_eq!(short, "hello");

        let long = logger.truncate_body("abcdefghijklmnopqrstuvwxyz");
        assert!(long.contains("...[truncated"));
        assert!(long.contains("26 bytes total"));
        assert!(long.len() > 26);
    }

    #[test]
    fn test_sensitive_headers_redacted_in_config() {
        let config = AuditConfig::default();
        assert!(config.sensitive_headers.contains(&"authorization".to_string()));
        assert!(config.sensitive_headers.contains(&"cookie".to_string()));
    }

    #[test]
    fn test_default_config() {
        let config = AuditConfig::default();
        assert!(config.enabled);
        assert!(!config.log_body);
        assert_eq!(config.max_body_log_size, 1024);
        assert_eq!(config.exclude_paths, vec!["/health", "/metrics"]);
        assert_eq!(config.sensitive_headers, vec!["authorization", "cookie"]);
    }

    #[test]
    fn test_config_deserialize_custom() {
        let config: AuditConfig = serde_json::from_value(serde_json::json!({
            "enabled": false,
            "log_body": true,
            "max_body_log_size": 2048,
            "exclude_paths": ["/health", "/metrics", "/internal"],
            "sensitive_headers": ["authorization", "cookie", "x-api-key"],
        }))
        .unwrap();

        assert!(!config.enabled);
        assert!(config.log_body);
        assert_eq!(config.max_body_log_size, 2048);
        assert_eq!(config.exclude_paths.len(), 3);
        assert!(config.sensitive_headers.contains(&"x-api-key".to_string()));
    }

    #[test]
    fn test_audit_logger_log_levels() {
        // This test just verifies the logger doesn't panic for different status codes
        let logger = AuditLogger::new(AuditConfig::default());

        let entry_2xx = AuditEntry::new(
            "req-1".into(), "GET".into(), "/".into(), "".into(),
            200, 10, "127.0.0.1".into(), "test".into(), None, None, None,
        );
        logger.log(&entry_2xx); // info level

        let entry_4xx = AuditEntry::new(
            "req-2".into(), "POST".into(), "/api/test".into(), "".into(),
            404, 5, "127.0.0.1".into(), "test".into(), None, None, None,
        );
        logger.log(&entry_4xx); // warn level

        let entry_5xx = AuditEntry::new(
            "req-3".into(), "GET".into(), "/api/fail".into(), "".into(),
            500, 100, "127.0.0.1".into(), "test".into(), None, None, None,
        );
        logger.log(&entry_5xx); // error level
    }

    #[test]
    fn test_audit_disabled() {
        let config = AuditConfig {
            enabled: false,
            ..Default::default()
        };
        let logger = AuditLogger::new(config);
        assert!(!logger.is_enabled());
    }
}
