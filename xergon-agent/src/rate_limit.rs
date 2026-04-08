//! Token-bucket rate limiting middleware for xergon-agent.
//!
//! Provides per-IP and per-authenticated-key rate limiting using a token-bucket
//! algorithm backed by a `DashMap` for lock-free concurrent access.
//!
//! Configuration is read from `[rate_limit]` in the agent config TOML.

use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::{
    body::Body,
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::warn;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Rate-limiting configuration.
///
/// Deserialized from the `[rate_limit]` section of the agent config.
/// All fields have sensible defaults so the section can be omitted entirely.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RateLimitConfig {
    /// Enable rate limiting (default: true).
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Requests per minute per IP address (default: 60).
    #[serde(default = "default_ip_rpm")]
    pub ip_rpm: u32,

    /// Burst capacity per IP (default: 10).
    #[serde(default = "default_ip_burst")]
    pub ip_burst: u32,

    /// Requests per minute per authenticated API key / provider_pk (default: 300).
    #[serde(default = "default_key_rpm")]
    pub key_rpm: u32,

    /// Burst capacity per authenticated key (default: 30).
    #[serde(default = "default_key_burst")]
    pub key_burst: u32,

    /// Requests per minute for admin endpoints (default: 1000).
    #[serde(default = "default_admin_rpm")]
    pub admin_rpm: u32,

    /// Burst capacity for admin endpoints (default: 100).
    #[serde(default = "default_admin_burst")]
    pub admin_burst: u32,
}

fn default_enabled() -> bool { true }
fn default_ip_rpm() -> u32 { 60 }
fn default_ip_burst() -> u32 { 10 }
fn default_key_rpm() -> u32 { 300 }
fn default_key_burst() -> u32 { 30 }
fn default_admin_rpm() -> u32 { 1000 }
fn default_admin_burst() -> u32 { 100 }

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            ip_rpm: default_ip_rpm(),
            ip_burst: default_ip_burst(),
            key_rpm: default_key_rpm(),
            key_burst: default_key_burst(),
            admin_rpm: default_admin_rpm(),
            admin_burst: default_admin_burst(),
        }
    }
}

// ---------------------------------------------------------------------------
// Token bucket
// ---------------------------------------------------------------------------

/// A single token bucket for one rate-limit key.
struct TokenBucket {
    /// Current number of available tokens.
    tokens: AtomicU32,
    /// Maximum number of tokens (burst capacity).
    max_tokens: u32,
    /// Tokens refilled per second.
    refill_rate: u32,
    /// Last refill timestamp as Unix milliseconds.
    last_refill: AtomicI64,
    /// Last access time (Instant) for staleness tracking.
    last_access: std::sync::atomic::AtomicU64,
}

impl TokenBucket {
    /// Create a new token bucket starting full.
    fn new(max_tokens: u32, refill_rate: u32) -> Self {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        Self {
            tokens: AtomicU32::new(max_tokens),
            max_tokens,
            refill_rate,
            last_refill: AtomicI64::new(now_ms),
            last_access: std::sync::atomic::AtomicU64::new(now_nanos),
        }
    }

    /// Try to consume one token. Returns `true` if allowed, `false` if rate-limited.
    fn try_consume(&self) -> bool {
        self.refill();
        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        self.last_access.store(now_nanos, Ordering::Relaxed);

        loop {
            let current = self.tokens.load(Ordering::Relaxed);
            if current == 0 {
                return false;
            }
            if self.tokens.compare_exchange_weak(current, current - 1, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
                return true;
            }
        }
    }

    /// Refill tokens based on elapsed time since last refill.
    fn refill(&self) {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let last = self.last_refill.load(Ordering::Relaxed);
        let elapsed_ms = (now_ms - last).max(0) as u64;
        let elapsed_secs = elapsed_ms / 1000;

        if elapsed_secs > 0 {
            let tokens_to_add = (elapsed_secs as u32) * self.refill_rate;
            let new_tokens = (self.tokens.load(Ordering::Relaxed) + tokens_to_add).min(self.max_tokens);
            self.tokens.store(new_tokens, Ordering::Relaxed);
            self.last_refill.store(now_ms, Ordering::Relaxed);
        }
    }

    /// Check if this bucket is stale (not accessed for 5 minutes).
    fn is_stale(&self) -> bool {
        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        let last = self.last_access.load(Ordering::Relaxed);
        let stale_nanos: u64 = Duration::from_secs(300).as_nanos() as u64;
        now_nanos.saturating_sub(last) > stale_nanos
    }
}

// ---------------------------------------------------------------------------
// Rate limit state
// ---------------------------------------------------------------------------

/// Shared rate-limiting state.
///
/// Thread-safe via `DashMap`. Can be cloned cheaply (Arc-like) for use in middleware.
#[derive(Clone)]
pub struct RateLimitState {
    /// Per-key token buckets.
    buckets: std::sync::Arc<DashMap<String, TokenBucket>>,
    /// Configuration.
    pub config: RateLimitConfig,
}

impl RateLimitState {
    /// Create a new rate-limit state from config.
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            buckets: std::sync::Arc::new(DashMap::new()),
            config,
        }
    }

    /// Check if a request from the given key is allowed.
    ///
    /// `rpm` is requests per minute, `burst` is the burst capacity.
    /// Returns `true` if the request should be allowed.
    pub fn check(&self, key: &str, rpm: u32, burst: u32) -> bool {
        if !self.config.enabled {
            return true;
        }
        let refill_rate = rpm / 60; // tokens per second
        let bucket = self
            .buckets
            .entry(key.to_string())
            .or_insert_with(|| TokenBucket::new(burst, refill_rate.max(1)));
        bucket.try_consume()
    }

    /// Remove stale entries (not accessed for 5 minutes).
    pub fn cleanup_stale(&self) {
        self.buckets.retain(|_, bucket| !bucket.is_stale());
    }

    /// Start a background cleanup task that runs every 60 seconds.
    pub fn start_cleanup_task(state: std::sync::Arc<RateLimitState>) {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                state.cleanup_stale();
            }
        });
    }

    /// Check rate limit for a given key using the appropriate config values.
    ///
    /// For unauthenticated requests (no provider_pk), uses IP-based limits.
    /// For authenticated requests, uses key-based limits (higher).
    /// Admin paths get even higher limits.
    fn check_request(&self, key: &str, is_admin: bool, authenticated: bool) -> bool {
        let (rpm, burst) = if is_admin {
            (self.config.admin_rpm, self.config.admin_burst)
        } else if authenticated {
            (self.config.key_rpm, self.config.key_burst)
        } else {
            (self.config.ip_rpm, self.config.ip_burst)
        };
        self.check(key, rpm, burst)
    }

    /// Returns true if rate limiting is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

// ---------------------------------------------------------------------------
// Axum middleware
// ---------------------------------------------------------------------------

/// Extract client IP from request headers or connect info.
fn extract_client_ip(req: &Request<Body>) -> String {
    // Try X-Forwarded-For first (set by reverse proxies)
    if let Some(forwarded) = req.headers().get("x-forwarded-for") {
        if let Ok(value) = forwarded.to_str() {
            // X-Forwarded-For can contain multiple IPs; use the first one
            if let Some(first_ip) = value.split(',').next() {
                let ip = first_ip.trim();
                if !ip.is_empty() {
                    return ip.to_string();
                }
            }
        }
    }

    // Try X-Real-IP
    if let Some(real_ip) = req.headers().get("x-real-ip") {
        if let Ok(value) = real_ip.to_str() {
            let ip = value.trim();
            if !ip.is_empty() {
                return ip.to_string();
            }
        }
    }

    // Fallback: unknown
    "unknown".to_string()
}

/// Axum middleware function for rate limiting.
///
/// Extracts the client IP and checks against per-IP or per-key rate limits.
/// For authenticated requests (Authorization header with JWT), attempts to
/// use the provider_pk for higher limits.
pub async fn rate_limit_middleware(
    axum::extract::State(state): axum::extract::State<RateLimitState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    if !state.is_enabled() {
        return next.run(req).await;
    }

    let client_ip = extract_client_ip(&req);
    let path = req.uri().path();
    let is_admin = path.starts_with("/xergon/") || path.starts_with("/api/settlement/");

    // Check if authenticated (has Bearer token)
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let authenticated = auth_header.starts_with("Bearer ") || auth_header.starts_with("bearer ");

    // Use provider_pk as key if we can extract it from the JWT
    // For the rate limit check, we use IP-based key for unauthenticated
    // and "key:{provider_pk}" for authenticated requests.
    // Since we can't decode JWT here without the secret, we use IP as
    // the primary key. Authenticated users still get IP-based limits
    // but with the higher key_rpm/key_burst thresholds.
    let rate_key = if authenticated {
        format!("auth:{}", client_ip)
    } else {
        format!("ip:{}", client_ip)
    };

    if !state.check_request(&rate_key, is_admin, authenticated) {
        warn!(
            path = %path,
            client_ip = %client_ip,
            "Rate limit exceeded"
        );
        let retry_after = 60; // seconds until next minute boundary
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [
                (header::RETRY_AFTER, retry_after.to_string()),
                (header::CONTENT_TYPE, "application/json".to_string()),
            ],
            serde_json::json!({
                "error": {
                    "type": "rate_limit_error",
                    "message": "Too many requests. Please retry later.",
                    "code": 429,
                }
            })
            .to_string(),
        )
            .into_response();
    }

    next.run(req).await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_rate_limiting_allow_burst() {
        let config = RateLimitConfig {
            enabled: true,
            ip_rpm: 60,
            ip_burst: 5,
            key_rpm: 300,
            key_burst: 30,
            admin_rpm: 1000,
            admin_burst: 100,
        };
        let state = RateLimitState::new(config);

        // Should allow burst of 5 requests
        for _ in 0..5 {
            assert!(state.check("ip:1.2.3.4", 60, 5), "should allow within burst");
        }
        // 6th should be denied
        assert!(!state.check("ip:1.2.3.4", 60, 5), "should deny after burst exhausted");
    }

    #[test]
    fn test_different_keys_have_separate_buckets() {
        let config = RateLimitConfig {
            enabled: true,
            ip_rpm: 60,
            ip_burst: 2,
            key_rpm: 300,
            key_burst: 30,
            admin_rpm: 1000,
            admin_burst: 100,
        };
        let state = RateLimitState::new(config);

        // Exhaust bucket for key A
        assert!(state.check("ip:1.1.1.1", 60, 2));
        assert!(state.check("ip:1.1.1.1", 60, 2));
        assert!(!state.check("ip:1.1.1.1", 60, 2));

        // Key B should still be allowed
        assert!(state.check("ip:2.2.2.2", 60, 2));
        assert!(state.check("ip:2.2.2.2", 60, 2));
    }

    #[test]
    fn test_token_refill_over_time() {
        let config = RateLimitConfig {
            enabled: true,
            ip_rpm: 600, // 10 per second
            ip_burst: 2,
            key_rpm: 300,
            key_burst: 30,
            admin_rpm: 1000,
            admin_burst: 100,
        };
        let state = RateLimitState::new(config);

        // Exhaust bucket
        assert!(state.check("ip:3.3.3.3", 600, 2));
        assert!(state.check("ip:3.3.3.3", 600, 2));
        assert!(!state.check("ip:3.3.3.3", 600, 2));

        // Simulate time passing by manipulating the bucket's last_refill
        {
            let bucket = state.buckets.get("ip:3.3.3.3").unwrap();
            // Set last_refill to 2 seconds ago
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            bucket.last_refill.store(now_ms - 2000, Ordering::Relaxed);
        }

        // After 2 seconds, refill_rate = 600/60 = 10 tokens/sec, so 20 tokens should refill
        // But capped at max_tokens = 2
        assert!(state.check("ip:3.3.3.3", 600, 2));
    }

    #[test]
    fn test_cleanup_stale_entries() {
        let config = RateLimitConfig::default();
        let state = RateLimitState::new(config);

        state.check("ip:stale:1", 60, 10);
        state.check("ip:stale:2", 60, 10);
        state.check("ip:stale:3", 60, 10);

        assert_eq!(state.buckets.len(), 3);

        // Make entries stale by setting last_access far in the past
        for entry in state.buckets.iter_mut() {
            let now_nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64;
            let stale_nanos: u64 = Duration::from_secs(600).as_nanos() as u64;
            entry.last_access.store(now_nanos - stale_nanos, Ordering::Relaxed);
        }

        state.cleanup_stale();
        assert_eq!(state.buckets.len(), 0, "all stale entries should be removed");
    }

    #[test]
    fn test_ip_vs_authenticated_key_limits() {
        let config = RateLimitConfig {
            enabled: true,
            ip_rpm: 60,
            ip_burst: 2,
            key_rpm: 300,
            key_burst: 10,
            admin_rpm: 1000,
            admin_burst: 100,
        };
        let state = RateLimitState::new(config);

        // IP-based: burst of 2
        assert!(state.check_request("ip:1.1.1.1", false, false));
        assert!(state.check_request("ip:1.1.1.1", false, false));
        assert!(!state.check_request("ip:1.1.1.1", false, false));

        // Auth-based: burst of 10 (different key prefix)
        for _ in 0..10 {
            assert!(state.check_request("auth:1.1.1.1", false, true));
        }
        assert!(!state.check_request("auth:1.1.1.1", false, true));
    }

    #[test]
    fn test_disabled_rate_limit_allows_all() {
        let config = RateLimitConfig {
            enabled: false,
            ip_rpm: 60,
            ip_burst: 2,
            key_rpm: 300,
            key_burst: 30,
            admin_rpm: 1000,
            admin_burst: 100,
        };
        let state = RateLimitState::new(config);

        // Even with burst=2, all requests should pass when disabled
        for _ in 0..100 {
            assert!(state.check("ip:1.2.3.4", 2, 2));
        }
    }

    #[test]
    fn test_admin_gets_higher_limits() {
        let config = RateLimitConfig {
            enabled: true,
            ip_rpm: 60,
            ip_burst: 2,
            key_rpm: 300,
            key_burst: 10,
            admin_rpm: 6000,
            admin_burst: 50,
        };
        let state = RateLimitState::new(config);

        // Admin: burst of 50
        for _ in 0..50 {
            assert!(state.check_request("ip:admin-test", true, false));
        }
        assert!(!state.check_request("ip:admin-test", true, false));
    }

    #[test]
    fn test_default_config_values() {
        let config = RateLimitConfig::default();
        assert!(config.enabled);
        assert_eq!(config.ip_rpm, 60);
        assert_eq!(config.ip_burst, 10);
        assert_eq!(config.key_rpm, 300);
        assert_eq!(config.key_burst, 30);
        assert_eq!(config.admin_rpm, 1000);
        assert_eq!(config.admin_burst, 100);
    }

    #[test]
    fn test_config_deserialize_defaults() {
        let config: RateLimitConfig = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(config.enabled);
        assert_eq!(config.ip_rpm, 60);
    }

    #[test]
    fn test_config_deserialize_custom() {
        let config: RateLimitConfig = serde_json::from_value(serde_json::json!({
            "enabled": false,
            "ip_rpm": 120,
            "ip_burst": 20,
            "key_rpm": 600,
            "key_burst": 60,
            "admin_rpm": 2000,
            "admin_burst": 200,
        }))
        .unwrap();
        assert!(!config.enabled);
        assert_eq!(config.ip_rpm, 120);
        assert_eq!(config.ip_burst, 20);
        assert_eq!(config.key_rpm, 600);
        assert_eq!(config.key_burst, 60);
        assert_eq!(config.admin_rpm, 2000);
        assert_eq!(config.admin_burst, 200);
    }
}
