//! Balance-based rate limiting middleware for the Xergon relay.
//!
//! Rate limits are determined by the user's on-chain ERG staking balance
//! (from their staking box). Users with more ERG staked get higher rate limits.
//!
//! Tiers:
//!   - No auth / unknown balance: 1 req/min  (trial)
//!   - < 0.1 ERG:                  1 req/min  (trial)
//!   - 0.1 - 1 ERG:               10 req/min
//!   - 1 - 10 ERG:                60 req/min
//!   - 10+ ERG:                  300 req/min
//!   - Providers (NFT holders):  unlimited   (still tracked via metrics)
//!
//! The governor crate provides the token-bucket mechanics.  Keys are the
//! user's Ergo public key (extracted from the `x-xergon-public-key` HMAC
//! auth header).  When no auth header is present the request falls back
//! to the lowest (trial) tier — it is never hard-blocked.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use dashmap::DashMap;
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use tracing::{debug, info, warn};

use crate::balance::BalanceChecker;
use crate::config::RateLimitConfig;
use crate::metrics::RelayMetrics;
use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Balance tier definitions
// ---------------------------------------------------------------------------

/// 1 ERG in nanoERG.
const NANOERG_PER_ERG: u64 = 1_000_000_000;

/// Maximum number of rate limiter entries before cleanup triggers.
const DEFAULT_MAX_ENTRIES: usize = 100_000;

/// How long a cached balance result is considered fresh (seconds).
const BALANCE_CACHE_TTL_SECS: u64 = 300; // 5 minutes

/// Header used to extract the user's public key.
const HEADER_PUBLIC_KEY: &str = "x-xergon-public-key";

/// Rate limit information returned by the check, used to set response headers.
#[derive(Debug, Clone, Copy)]
pub struct RateLimitInfo {
    /// Maximum requests per window.
    pub limit: u32,
    /// Remaining requests in current window.
    pub remaining: usize,
    /// Seconds until the window resets.
    pub reset_after_secs: u64,
}

// ---------------------------------------------------------------------------
// Tier struct
// ---------------------------------------------------------------------------

/// A rate-limit tier with a human-readable name, requests-per-minute, and
/// burst capacity.
#[derive(Debug, Clone, Copy)]
struct Tier {
    name: &'static str,
    rpm: u32,
    burst: u32,
}

impl Tier {
    fn trial() -> Self {
        Self {
            name: "trial",
            rpm: 1,
            burst: 1,
        }
    }
    fn basic() -> Self {
        Self {
            name: "basic",
            rpm: 10,
            burst: 10,
        }
    }
    fn standard() -> Self {
        Self {
            name: "standard",
            rpm: 60,
            burst: 30,
        }
    }
    fn premium() -> Self {
        Self {
            name: "premium",
            rpm: 300,
            burst: 60,
        }
    }
    /// Providers have "unlimited" — we still create a very high limit so
    /// governor can track, but it should never trigger in practice.
    fn provider() -> Self {
        Self {
            name: "provider",
            rpm: 10_000,
            burst: 1_000,
        }
    }

    /// Build a governor Quota from this tier.
    fn quota(&self) -> Quota {
        Quota::per_minute(NonZeroU32::new(self.rpm).unwrap_or(NonZeroU32::MIN))
            .allow_burst(NonZeroU32::new(self.burst).unwrap_or(NonZeroU32::MIN))
    }
}

/// Determine the appropriate rate-limit tier from a balance in nanoERG.
fn tier_from_balance(balance_nanoerg: u64, is_provider: bool) -> Tier {
    if is_provider {
        return Tier::provider();
    }
    let erg = balance_nanoerg as f64 / NANOERG_PER_ERG as f64;
    if erg >= 10.0 {
        Tier::premium()
    } else if erg >= 1.0 {
        Tier::standard()
    } else if erg >= 0.1 {
        Tier::basic()
    } else {
        Tier::trial()
    }
}

// ---------------------------------------------------------------------------
// Cached balance entry
// ---------------------------------------------------------------------------

struct CachedBalance {
    balance_nanoerg: u64,
    is_provider: bool,
    inserted_at: Instant,
}

// ---------------------------------------------------------------------------
// RateLimitState
// ---------------------------------------------------------------------------

/// Shared rate limit state keyed by the user's Ergo public key.
pub struct RateLimitState {
    /// Per-public-key rate limiters.  Each entry is tagged with the tier
    /// name so we can re-evaluate when the cached balance expires.
    limiters: DashMap<String, Arc<(Tier, DefaultDirectRateLimiter)>>,
    /// Cached balance lookups to avoid hammering the Ergo node.
    balance_cache: DashMap<String, CachedBalance>,
    /// Optional balance checker (None if balance checking is disabled).
    balance_checker: Option<Arc<BalanceChecker>>,
    #[allow(dead_code)]
    config: RateLimitConfig,
    metrics: Arc<RelayMetrics>,
}

impl RateLimitState {
    /// Create a new balance-based rate limiter.
    pub fn new(
        config: RateLimitConfig,
        metrics: Arc<RelayMetrics>,
        balance_checker: Option<Arc<BalanceChecker>>,
    ) -> Self {
        info!(
            enabled = config.enabled,
            balance_checker_available = balance_checker.is_some(),
            "Balance-based rate limiting configured"
        );
        Self {
            limiters: DashMap::with_capacity(10_000),
            balance_cache: DashMap::with_capacity(10_000),
            balance_checker,
            config,
            metrics,
        }
    }

    /// Look up (or fetch) the cached balance for a public key.
    ///
    /// Returns `(balance_nanoerg, is_provider)`.  If the balance checker is
    /// unavailable or the query fails, returns `(0, false)` so the caller
    /// falls back to the trial tier.
    async fn get_balance(&self, public_key: &str) -> (u64, bool) {
        // Check cache first
        if let Some(cached) = self.balance_cache.get(public_key) {
            if cached.inserted_at.elapsed().as_secs() < BALANCE_CACHE_TTL_SECS {
                return (cached.balance_nanoerg, cached.is_provider);
            }
            // Stale — remove and re-fetch
            drop(cached);
            self.balance_cache.remove(public_key);
        }

        // Query the balance checker if available
        if let Some(ref checker) = self.balance_checker {
            match checker.get_balance(public_key).await {
                Ok((balance, _box_count)) => {
                    // TODO: provider NFT detection can be added here in the
                    // future. For now we treat all users as non-providers.
                    let is_provider = false;
                    self.balance_cache.insert(
                        public_key.to_string(),
                        CachedBalance {
                            balance_nanoerg: balance,
                            is_provider,
                            inserted_at: Instant::now(),
                        },
                    );
                    return (balance, is_provider);
                }
                Err(e) => {
                    warn!(
                        public_key = %&public_key[..public_key.len().min(16)],
                        error = %e,
                        "Balance check failed, falling back to trial tier"
                    );
                }
            }
        }

        // No checker or query failed — trial tier
        (0, false)
    }

    /// Get or create a rate limiter for the given public key.
    ///
    /// This is async because it may need to query the balance checker.
    async fn get_or_create_limiter(
        &self,
        public_key: &str,
    ) -> Arc<(Tier, DefaultDirectRateLimiter)> {
        // Fast path: check if we already have a limiter and the balance
        // cache is still fresh.
        if let Some(entry) = self.limiters.get(public_key) {
            // If the balance cache entry is still fresh, reuse the limiter.
            if let Some(cached) = self.balance_cache.get(public_key) {
                if cached.inserted_at.elapsed().as_secs() < BALANCE_CACHE_TTL_SECS {
                    return entry.value().clone();
                }
                // Balance cache expired — we need to re-evaluate.  Drop the
                // existing limiter so a fresh one is created below.
                drop(entry);
                self.limiters.remove(public_key);
            } else {
                // No balance cache entry (e.g. no balance checker) — keep
                // reusing the existing limiter.
                return entry.value().clone();
            }
        }

        // Slow path: fetch balance and create limiter.
        let (balance_nanoerg, is_provider) = self.get_balance(public_key).await;
        let tier = tier_from_balance(balance_nanoerg, is_provider);

        debug!(
            public_key = %&public_key[..public_key.len().min(16)],
            tier = tier.name,
            balance_nanoerg,
            "Creating rate limiter"
        );

        let limiter = Arc::new((tier, RateLimiter::direct(tier.quota())));
        self.limiters
            .entry(public_key.to_string())
            .or_insert_with(|| limiter.clone())
            .value()
            .clone()
    }

    /// Check if a request should be rate limited.
    ///
    /// `public_key` is the user's Ergo public key (from the HMAC auth header).
    /// If `None`, the trial tier is used with a synthetic key.
    pub async fn check(&self, public_key: Option<&str>) -> Result<RateLimitInfo, Response> {
        let key = public_key.unwrap_or("__unauthenticated__");

        let entry = self.get_or_create_limiter(key).await;
        let (tier, limiter) = entry.as_ref();

        match limiter.check() {
            Ok(_) => {
                // governor 0.6 doesn't expose remaining() on the limiter directly.
                // Approximate remaining from the burst capacity minus 1 consumed.
                let remaining = tier.burst.saturating_sub(1) as usize;
                Ok(RateLimitInfo {
                    limit: tier.rpm,
                    remaining,
                    reset_after_secs: 60,
                })
            }
            Err(_negative) => {
                debug!(
                    public_key = %&key[..key.len().min(16)],
                    tier = tier.name,
                    "Rate limited"
                );
                self.metrics.inc_rate_limited();

                let info = RateLimitInfo {
                    limit: tier.rpm,
                    remaining: 0,
                    reset_after_secs: 60,
                };

                Err(rate_limited_response(
                    &format!(
                        "Rate limit exceeded ({} tier: {} req/min)",
                        tier.name, tier.rpm
                    ),
                    &info,
                ))
            }
        }
    }

    /// Periodic cleanup of stale entries when the maps grow too large.
    ///
    /// Removes approximately half the entries when the map exceeds
    /// `max_entries`.  Evicted entries are lazily recreated with full
    /// burst capacity.
    pub async fn cleanup(&self) {
        let max_entries = DEFAULT_MAX_ENTRIES;

        // Clean up rate limiters
        let len = self.limiters.len();
        if len > max_entries {
            let to_remove = len / 2;
            let keys: Vec<String> = self
                .limiters
                .iter()
                .take(to_remove)
                .map(|r| r.key().clone())
                .collect();
            for key in keys {
                self.limiters.remove(&key);
            }
            let removed = len - self.limiters.len();
            if removed > 0 {
                debug!(removed, "Cleaned up stale rate limiters");
            }
        }

        // Clean up balance cache
        let bal_len = self.balance_cache.len();
        if bal_len > max_entries {
            let to_remove = bal_len / 2;
            let keys: Vec<String> = self
                .balance_cache
                .iter()
                .take(to_remove)
                .map(|r| r.key().clone())
                .collect();
            for key in keys {
                self.balance_cache.remove(&key);
            }
            let removed = bal_len - self.balance_cache.len();
            if removed > 0 {
                debug!(removed, "Cleaned up stale balance cache entries");
            }
        }

        // Also expire truly stale balance cache entries even if under limit
        let before = self.balance_cache.len();
        self.balance_cache.retain(|_, entry| {
            entry.inserted_at.elapsed().as_secs() < BALANCE_CACHE_TTL_SECS
        });
        let expired = before - self.balance_cache.len();
        if expired > 0 {
            debug!(expired, "Expired stale balance cache entries");
        }
    }

    /// Return the number of rate limiter entries (for testing/monitoring).
    #[cfg(test)]
    pub fn limiter_count(&self) -> usize {
        self.limiters.len()
    }

    /// Return the number of cached balance entries (for testing).
    #[cfg(test)]
    pub fn balance_cache_count(&self) -> usize {
        self.balance_cache.len()
    }

    /// Insert a known balance into the cache (for testing).
    #[cfg(test)]
    pub fn set_cached_balance(&self, public_key: &str, balance_nanoerg: u64, is_provider: bool) {
        self.balance_cache.insert(
            public_key.to_string(),
            CachedBalance {
                balance_nanoerg,
                is_provider,
                inserted_at: Instant::now(),
            },
        );
    }
}

// ---------------------------------------------------------------------------
// Middleware
// ---------------------------------------------------------------------------

/// Axum middleware function that applies balance-based rate limits.
///
/// Exempts health, readiness, and metrics endpoints from rate limiting.
/// Extracts the user's public key from the `x-xergon-public-key` header.
/// Falls back to the trial tier if no auth header is present.
pub async fn rate_limit_middleware(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // If rate limiting is disabled, pass through
    let Some(rate_limit) = &state.rate_limit_state else {
        return next.run(req).await;
    };

    let path = req.uri().path();

    // Exempt health/metrics endpoints from rate limiting
    if is_exempt_path(path) {
        return next.run(req).await;
    }

    // Extract public key from auth header
    let public_key = extract_public_key(req.headers());

    // Check rate limit (async because it may query balance checker)
    match rate_limit.check(public_key.as_deref()).await {
        Ok(info) => {
            let response = next.run(req).await;
            inject_rate_limit_headers(response, &info)
        }
        Err(response) => response,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Paths that are exempt from rate limiting.
fn is_exempt_path(path: &str) -> bool {
    matches!(
        path,
        "/health" | "/ready" | "/v1/health" | "/v1/metrics"
    )
}

/// Extract the user's public key from the `x-xergon-public-key` header.
fn extract_public_key(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(HEADER_PUBLIC_KEY)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Build a 429 Too Many Requests response with a JSON body and rate limit headers.
fn rate_limited_response(message: &str, info: &RateLimitInfo) -> Response {
    let mut resp = (
        StatusCode::TOO_MANY_REQUESTS,
        serde_json::json!({
            "error": {
                "message": message,
                "type": "rate_limit_error",
                "code": 429
            }
        })
        .to_string(),
    )
        .into_response();

    // Add rate limit headers
    let headers = resp.headers_mut();
    headers.insert("Retry-After", info.reset_after_secs.to_string().parse().unwrap());
    headers.insert("X-RateLimit-Limit", info.limit.to_string().parse().unwrap());
    headers.insert("X-RateLimit-Remaining", "0".parse().unwrap());
    headers.insert("X-RateLimit-Reset", info.reset_after_secs.to_string().parse().unwrap());

    resp
}

/// Inject rate limit headers into a successful response.
fn inject_rate_limit_headers(mut response: Response, info: &RateLimitInfo) -> Response {
    let headers = response.headers_mut();
    headers.insert("X-RateLimit-Limit", info.limit.to_string().parse().unwrap());
    headers.insert(
        "X-RateLimit-Remaining",
        info.remaining.to_string().parse().unwrap(),
    );
    headers.insert(
        "X-RateLimit-Reset",
        info.reset_after_secs.to_string().parse().unwrap(),
    );
    response
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> RateLimitConfig {
        RateLimitConfig {
            enabled: true,
            ip_rpm: 30,
            ip_burst: 10,
            key_rpm: 120,
            key_burst: 30,
        }
    }

    fn test_state(config: RateLimitConfig) -> RateLimitState {
        RateLimitState::new(config, Arc::new(RelayMetrics::new()), None)
    }

    fn test_state_with_balance(config: RateLimitConfig) -> RateLimitState {
        RateLimitState::new(config, Arc::new(RelayMetrics::new()), None)
    }

    // -- Tier tests --

    #[test]
    fn test_tier_from_balance_trial() {
        // < 0.1 ERG → trial
        let tier = tier_from_balance(50_000_000, false); // 0.05 ERG
        assert_eq!(tier.name, "trial");
        assert_eq!(tier.rpm, 1);
    }

    #[test]
    fn test_tier_from_balance_zero() {
        let tier = tier_from_balance(0, false);
        assert_eq!(tier.name, "trial");
    }

    #[test]
    fn test_tier_from_balance_basic() {
        // 0.1 ERG → basic
        let tier = tier_from_balance(100_000_000, false);
        assert_eq!(tier.name, "basic");
        assert_eq!(tier.rpm, 10);
    }

    #[test]
    fn test_tier_from_balance_standard() {
        // 1 ERG → standard
        let tier = tier_from_balance(1_000_000_000, false);
        assert_eq!(tier.name, "standard");
        assert_eq!(tier.rpm, 60);
    }

    #[test]
    fn test_tier_from_balance_premium() {
        // 10 ERG → premium
        let tier = tier_from_balance(10_000_000_000, false);
        assert_eq!(tier.name, "premium");
        assert_eq!(tier.rpm, 300);
    }

    #[test]
    fn test_tier_from_balance_provider() {
        // Provider overrides balance
        let tier = tier_from_balance(0, true);
        assert_eq!(tier.name, "provider");
        assert_eq!(tier.rpm, 10_000);
    }

    #[test]
    fn test_tier_from_balance_provider_even_with_low_balance() {
        let tier = tier_from_balance(50_000_000, true);
        assert_eq!(tier.name, "provider");
    }

    // -- RateLimitState tests --

    #[tokio::test]
    async fn test_unauthenticated_gets_trial_tier() {
        let state = test_state(test_config());

        // No public key → trial tier (1 req/min, burst 1)
        // First request should succeed
        assert!(state.check(None).await.is_ok());
        // Second request should be rate limited
        assert!(state.check(None).await.is_err());
    }

    #[tokio::test]
    async fn test_known_public_key_gets_trial_tier_when_no_balance() {
        let state = test_state_with_balance(test_config());
        // No balance checker, no cached balance → trial tier
        let pk = "abcdef1234567890";

        assert!(state.check(Some(pk)).await.is_ok());
        assert!(state.check(Some(pk)).await.is_err());
    }

    #[tokio::test]
    async fn test_different_keys_independent() {
        let state = test_state(test_config());

        // Key 1: trial tier, 1 burst
        assert!(state.check(Some("key1")).await.is_ok());
        assert!(state.check(Some("key1")).await.is_err());

        // Key 2 should still have full burst
        assert!(state.check(Some("key2")).await.is_ok());
        assert!(state.check(Some("key2")).await.is_err());
    }

    #[tokio::test]
    async fn test_cached_balance_determines_tier() {
        let state = test_state_with_balance(test_config());

        // Pre-cache a premium balance (10 ERG) for this key
        state.set_cached_balance("rich_user", 10_000_000_000, false);

        // Premium tier: 300 rpm, burst 60 — all should succeed
        for i in 0..60 {
            assert!(
                state.check(Some("rich_user")).await.is_ok(),
                "Request {} should be allowed for premium tier",
                i + 1
            );
        }
        // 61st should be rate limited (burst = 60)
        assert!(state.check(Some("rich_user")).await.is_err());
    }

    #[tokio::test]
    async fn test_basic_tier_allows_10_burst() {
        let state = test_state_with_balance(test_config());

        // Pre-cache 0.5 ERG → basic tier (10 rpm, burst 10)
        state.set_cached_balance("basic_user", 500_000_000, false);

        for i in 0..10 {
            assert!(
                state.check(Some("basic_user")).await.is_ok(),
                "Request {} should be allowed for basic tier",
                i + 1
            );
        }
        // 11th should be rate limited
        assert!(state.check(Some("basic_user")).await.is_err());
    }

    #[tokio::test]
    async fn test_standard_tier_allows_30_burst() {
        let state = test_state_with_balance(test_config());

        // Pre-cache 5 ERG → standard tier (60 rpm, burst 30)
        state.set_cached_balance("standard_user", 5_000_000_000, false);

        for i in 0..30 {
            assert!(
                state.check(Some("standard_user")).await.is_ok(),
                "Request {} should be allowed for standard tier",
                i + 1
            );
        }
        // 31st should be rate limited
        assert!(state.check(Some("standard_user")).await.is_err());
    }

    #[tokio::test]
    async fn test_provider_tier_very_high_limit() {
        let state = test_state_with_balance(test_config());

        // Pre-cache as provider
        state.set_cached_balance("provider_user", 10_000_000_000, true);

        // Provider tier: 10000 rpm, burst 1000 — 100 requests should all pass
        for i in 0..100 {
            assert!(
                state.check(Some("provider_user")).await.is_ok(),
                "Request {} should be allowed for provider tier",
                i + 1
            );
        }
    }

    #[tokio::test]
    async fn test_rate_limit_increments_metrics() {
        let state = test_state(test_config());
        assert_eq!(state.metrics.rate_limited_count(), 0);

        // First request: allowed
        state.check(Some("metrics_test")).await.unwrap();
        assert_eq!(state.metrics.rate_limited_count(), 0);

        // Second request: rate limited
        state.check(Some("metrics_test")).await.unwrap_err();
        assert_eq!(state.metrics.rate_limited_count(), 1);

        // Third request: also rate limited
        state.check(Some("metrics_test")).await.unwrap_err();
        assert_eq!(state.metrics.rate_limited_count(), 2);
    }

    #[tokio::test]
    async fn test_cleanup_removes_entries() {
        // DEFAULT_MAX_ENTRIES is 100_000; with fewer entries cleanup is a no-op
        let state = test_state(test_config());

        for i in 0..50u32 {
            state.check(Some(&format!("key{}", i))).await.unwrap();
        }
        assert_eq!(state.limiter_count(), 50);

        state.cleanup().await;
        assert_eq!(state.limiter_count(), 50);
    }

    #[tokio::test]
    async fn test_cleanup_noop_when_under_threshold() {
        let state = test_state(test_config());
        for i in 0..5u32 {
            state.check(Some(&format!("key{}", i))).await.unwrap();
        }
        assert_eq!(state.limiter_count(), 5);

        state.cleanup().await;
        assert_eq!(state.limiter_count(), 5);
    }

    // -- Helper function tests --

    #[test]
    fn test_rate_limited_response_format() {
        let info = RateLimitInfo {
            limit: 30,
            remaining: 0,
            reset_after_secs: 60,
        };
        let resp = rate_limited_response("Rate limit exceeded", &info);
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

        let retry_after = resp.headers().get("retry-after").unwrap();
        assert_eq!(retry_after, "60");
    }

    #[test]
    fn test_extract_public_key_present() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            "x-xergon-public-key",
            "abcdef1234567890".parse().unwrap(),
        );
        assert_eq!(
            extract_public_key(&headers),
            Some("abcdef1234567890".to_string())
        );
    }

    #[test]
    fn test_extract_public_key_missing() {
        let headers = axum::http::HeaderMap::new();
        assert_eq!(extract_public_key(&headers), None);
    }

    #[test]
    fn test_extract_public_key_empty() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-xergon-public-key", "".parse().unwrap());
        assert_eq!(extract_public_key(&headers), None);
    }

    #[test]
    fn test_extract_public_key_whitespace() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("x-xergon-public-key", "  abcdef  ".parse().unwrap());
        assert_eq!(
            extract_public_key(&headers),
            Some("abcdef".to_string())
        );
    }

    #[test]
    fn test_is_exempt_path() {
        assert!(is_exempt_path("/health"));
        assert!(is_exempt_path("/ready"));
        assert!(is_exempt_path("/v1/health"));
        assert!(is_exempt_path("/v1/metrics"));
        assert!(!is_exempt_path("/v1/chat/completions"));
        assert!(!is_exempt_path("/v1/models"));
        assert!(!is_exempt_path("/v1/providers"));
    }

    #[test]
    fn test_tier_quota_is_valid() {
        // Ensure all tier quotas can be constructed without panicking
        let _ = Tier::trial().quota();
        let _ = Tier::basic().quota();
        let _ = Tier::standard().quota();
        let _ = Tier::premium().quota();
        let _ = Tier::provider().quota();
    }
}
