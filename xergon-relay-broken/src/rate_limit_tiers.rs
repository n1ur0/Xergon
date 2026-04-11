//! Multi-tier rate limiting for the Xergon relay.
//!
//! Builds on the existing balance-based rate limiting in `rate_limit.rs` by
//! providing an explicit tier system that maps API keys (public keys) to
//! predefined rate-limit tiers: Free, Basic, Pro, Enterprise.
//!
//! The TierManager is an optional layer that can override the default
//! balance-based tier assignment, allowing admins to manually upgrade (or
//! downgrade) specific keys.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// Tier enum
// ---------------------------------------------------------------------------

/// Rate-limit tiers from least to most permissive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum RateLimitTier {
    Free,
    Basic,
    Pro,
    Enterprise,
}

impl std::fmt::Display for RateLimitTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RateLimitTier::Free => write!(f, "free"),
            RateLimitTier::Basic => write!(f, "basic"),
            RateLimitTier::Pro => write!(f, "pro"),
            RateLimitTier::Enterprise => write!(f, "enterprise"),
        }
    }
}

impl RateLimitTier {
    /// Parse a tier name (case-insensitive).
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "basic" => RateLimitTier::Basic,
            "pro" => RateLimitTier::Pro,
            "enterprise" => RateLimitTier::Enterprise,
            _ => RateLimitTier::Free,
        }
    }
}

// ---------------------------------------------------------------------------
// TierConfig
// ---------------------------------------------------------------------------

/// Configuration for a single rate-limit tier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierConfig {
    pub tier: RateLimitTier,
    /// Maximum requests per minute.
    pub requests_per_minute: u32,
    /// Maximum requests per day.
    pub requests_per_day: u32,
    /// Maximum concurrent requests.
    pub max_concurrent: u32,
    /// Maximum input tokens per request.
    pub max_input_tokens: u32,
    /// Which models are available (empty = all).
    pub models: Vec<String>,
    /// Queue priority (lower = higher priority).
    pub priority: u8,
}

impl Default for TierConfig {
    fn default() -> Self {
        Self::free()
    }
}

impl TierConfig {
    /// Free tier: 3 req/min, 100 req/day
    pub fn free() -> Self {
        Self {
            tier: RateLimitTier::Free,
            requests_per_minute: 3,
            requests_per_day: 100,
            max_concurrent: 1,
            max_input_tokens: 4096,
            models: vec![],
            priority: 3,
        }
    }

    /// Basic tier: 30 req/min, 10K req/day
    pub fn basic() -> Self {
        Self {
            tier: RateLimitTier::Basic,
            requests_per_minute: 30,
            requests_per_day: 10_000,
            max_concurrent: 5,
            max_input_tokens: 32_768,
            models: vec![],
            priority: 2,
        }
    }

    /// Pro tier: 120 req/min, 100K req/day
    pub fn pro() -> Self {
        Self {
            tier: RateLimitTier::Pro,
            requests_per_minute: 120,
            requests_per_day: 100_000,
            max_concurrent: 20,
            max_input_tokens: 128_000,
            models: vec![],
            priority: 1,
        }
    }

    /// Enterprise tier: effectively unlimited
    pub fn enterprise() -> Self {
        Self {
            tier: RateLimitTier::Enterprise,
            requests_per_minute: 10_000,
            requests_per_day: 10_000_000,
            max_concurrent: 100,
            max_input_tokens: 1_000_000,
            models: vec![],
            priority: 0,
        }
    }

    /// Get config for a given tier.
    pub fn for_tier(tier: RateLimitTier) -> Self {
        match tier {
            RateLimitTier::Free => Self::free(),
            RateLimitTier::Basic => Self::basic(),
            RateLimitTier::Pro => Self::pro(),
            RateLimitTier::Enterprise => Self::enterprise(),
        }
    }
}

// ---------------------------------------------------------------------------
// Per-key usage tracking
// ---------------------------------------------------------------------------

/// Tracks request counts for a single API key.
///
/// Uses `std::sync::Mutex` for the window timestamps because `Instant` is
/// not atomic. The lock is held only briefly during counter resets.
struct KeyUsage {
    /// Minute-level request counter (resets every minute).
    minute_requests: AtomicU32,
    /// Day-level request counter (resets every day).
    day_requests: AtomicU32,
    /// Current concurrent request count.
    concurrent: AtomicU32,
    /// Window start timestamps (protected by mutex).
    windows: std::sync::Mutex<WindowStarts>,
}

/// Start timestamps for rate-limit windows.
struct WindowStarts {
    minute_start: Instant,
    day_start: Instant,
}

impl KeyUsage {
    fn new() -> Self {
        Self {
            minute_requests: AtomicU32::new(0),
            day_requests: AtomicU32::new(0),
            concurrent: AtomicU32::new(0),
            windows: std::sync::Mutex::new(WindowStarts {
                minute_start: Instant::now(),
                day_start: Instant::now(),
            }),
        }
    }

    /// Check if a request is within limits, incrementing counters if allowed.
    fn check_and_increment(&self, config: &TierConfig) -> TierCheckResult {
        let now = Instant::now();

        {
            let mut wins = self.windows.lock().unwrap();

            // Reset minute counter if window expired
            if now.duration_since(wins.minute_start).as_secs() >= 60 {
                self.minute_requests.store(0, Ordering::Relaxed);
                wins.minute_start = now;
            }

            // Reset day counter if window expired
            if now.duration_since(wins.day_start).as_secs() >= 86400 {
                self.day_requests.store(0, Ordering::Relaxed);
                wins.day_start = now;
            }
        }

        // Check minute limit
        let minute_count = self.minute_requests.fetch_add(1, Ordering::Relaxed);
        if minute_count >= config.requests_per_minute {
            // Rollback
            self.minute_requests.fetch_sub(1, Ordering::Relaxed);
            return TierCheckResult::RateLimited {
                reason: format!(
                    "Minute rate limit exceeded: {}/{}",
                    minute_count + 1,
                    config.requests_per_minute
                ),
                limit: config.requests_per_minute,
                remaining: 0,
                reset_after_secs: 60,
            };
        }

        // Check day limit
        let day_count = self.day_requests.fetch_add(1, Ordering::Relaxed);
        if day_count >= config.requests_per_day {
            self.minute_requests.fetch_sub(1, Ordering::Relaxed);
            self.day_requests.fetch_sub(1, Ordering::Relaxed);
            return TierCheckResult::RateLimited {
                reason: format!(
                    "Daily rate limit exceeded: {}/{}",
                    day_count + 1,
                    config.requests_per_day
                ),
                limit: config.requests_per_day,
                remaining: 0,
                reset_after_secs: 86400,
            };
        }

        // Check concurrent limit
        let concurrent = self.concurrent.fetch_add(1, Ordering::Relaxed);
        if concurrent >= config.max_concurrent {
            self.minute_requests.fetch_sub(1, Ordering::Relaxed);
            self.day_requests.fetch_sub(1, Ordering::Relaxed);
            self.concurrent.fetch_sub(1, Ordering::Relaxed);
            return TierCheckResult::RateLimited {
                reason: format!(
                    "Concurrent request limit exceeded: {}/{}",
                    concurrent + 1,
                    config.max_concurrent
                ),
                limit: config.max_concurrent,
                remaining: 0,
                reset_after_secs: 0,
            };
        }

        TierCheckResult::Allowed {
            remaining_minute: config.requests_per_minute.saturating_sub(minute_count + 1),
            remaining_day: config.requests_per_day.saturating_sub(day_count + 1),
        }
    }

    /// Decrement concurrent counter when a request completes.
    fn decrement_concurrent(&self) {
        self.concurrent.fetch_sub(1, Ordering::Relaxed);
    }
}

// Workaround: minute_start/day_start as Instant can't use Atomic directly.
// We use a separate approach with parking_lot or just accept a small race.
// For simplicity, we store start times as plain Instant and handle resets
// with a CAS-like pattern using a Mutex-free approach.

// ---------------------------------------------------------------------------
// TierCheckResult
// ---------------------------------------------------------------------------

/// Result of a tier-based rate limit check.
#[derive(Debug, Clone)]
pub enum TierCheckResult {
    Allowed {
        remaining_minute: u32,
        remaining_day: u32,
    },
    RateLimited {
        reason: String,
        limit: u32,
        remaining: u32,
        reset_after_secs: u64,
    },
}

// ---------------------------------------------------------------------------
// TierManager
// ---------------------------------------------------------------------------

/// Manages per-key tier assignments and rate limit enforcement.
pub struct TierManager {
    /// Maps API keys (public keys) to their assigned tier.
    tier_assignments: DashMap<String, RateLimitTier>,
    /// Per-key usage tracking.
    usage: DashMap<String, KeyUsage>,
    /// Predefined tier configs.
    configs: std::collections::HashMap<RateLimitTier, TierConfig>,
}

impl TierManager {
    /// Create a new tier manager with default tier configs.
    pub fn new() -> Self {
        let mut configs = std::collections::HashMap::new();
        configs.insert(RateLimitTier::Free, TierConfig::free());
        configs.insert(RateLimitTier::Basic, TierConfig::basic());
        configs.insert(RateLimitTier::Pro, TierConfig::pro());
        configs.insert(RateLimitTier::Enterprise, TierConfig::enterprise());

        Self {
            tier_assignments: DashMap::new(),
            usage: DashMap::new(),
            configs,
        }
    }

    /// Create a new tier manager with custom tier configs.
    pub fn with_configs(configs: std::collections::HashMap<RateLimitTier, TierConfig>) -> Self {
        Self {
            tier_assignments: DashMap::new(),
            usage: DashMap::new(),
            configs,
        }
    }

    /// Get the tier for a given API key. Falls back to Free.
    pub fn get_tier(&self, api_key: &str) -> RateLimitTier {
        self.tier_assignments
            .get(api_key)
            .map(|r| *r.value())
            .unwrap_or(RateLimitTier::Free)
    }

    /// Get the tier config for a given API key.
    pub fn get_tier_config(&self, api_key: &str) -> TierConfig {
        let tier = self.get_tier(api_key);
        self.configs
            .get(&tier)
            .cloned()
            .unwrap_or_else(TierConfig::default)
    }

    /// Check if a request from the given API key is within tier limits.
    ///
    /// If allowed, increments the counters. Call `decrement_concurrent()` when
    /// the request completes.
    pub fn check_tier_limit(&self, api_key: &str) -> TierCheckResult {
        let config = self.get_tier_config(api_key);

        // Enterprise is effectively unlimited
        if config.tier == RateLimitTier::Enterprise {
            return TierCheckResult::Allowed {
                remaining_minute: u32::MAX,
                remaining_day: u32::MAX,
            };
        }

        let usage = self
            .usage
            .entry(api_key.to_string())
            .or_insert_with(KeyUsage::new);

        usage.check_and_increment(&config)
    }

    /// Decrement the concurrent request counter for a key (call on request completion).
    pub fn decrement_concurrent(&self, api_key: &str) {
        if let Some(usage) = self.usage.get(api_key) {
            usage.decrement_concurrent();
        }
    }

    /// Get tier info for a given API key (for the /v1/tier endpoint).
    pub fn get_tier_info(&self, api_key: &str) -> serde_json::Value {
        let tier = self.get_tier(api_key);
        let config = self.get_tier_config(api_key);

        serde_json::json!({
            "tier": format!("{}", tier),
            "limits": {
                "requests_per_minute": config.requests_per_minute,
                "requests_per_day": config.requests_per_day,
                "max_concurrent": config.max_concurrent,
                "max_input_tokens": config.max_input_tokens,
                "priority": config.priority,
            },
            "models": config.models,
        })
    }

    /// Upgrade (or change) a key's tier. Returns the previous tier.
    pub fn set_tier(&self, api_key: &str, tier: RateLimitTier) -> RateLimitTier {
        let previous = self.get_tier(api_key);
        self.tier_assignments.insert(api_key.to_string(), tier);
        info!(
            api_key = %&api_key[..api_key.len().min(16)],
            old_tier = %previous,
            new_tier = %tier,
            "Tier assignment updated"
        );
        previous
    }

    /// Remove a key's tier assignment (reverts to Free).
    pub fn remove_tier(&self, api_key: &str) -> bool {
        self.tier_assignments.remove(api_key).is_some()
    }

    /// Get all tier assignments (for admin purposes).
    pub fn list_assignments(&self) -> Vec<(String, RateLimitTier)> {
        self.tier_assignments
            .iter()
            .map(|r| (r.key().clone(), *r.value()))
            .collect()
    }

    /// Cleanup stale usage entries (call periodically).
    pub fn cleanup(&self) {
        let before = self.usage.len();
        self.usage.retain(|_, usage| {
            let wins = usage.windows.lock().unwrap();
            // Remove entries whose day window has expired (48h+)
            wins.day_start.elapsed().as_secs() < 172800
        });
        let removed = before - self.usage.len();
        if removed > 0 {
            debug!(removed, "Cleaned up stale tier usage entries");
        }
    }

    /// Return the number of tracked keys.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.tier_assignments.len()
    }

    /// Return true if no tier assignments exist.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.tier_assignments.is_empty()
    }
}

impl Default for TierManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Axum handler: GET /v1/tier
// ---------------------------------------------------------------------------

use axum::http::HeaderMap;

/// GET /v1/tier -- Return current user's tier info.
pub async fn tier_info_handler(
    axum::extract::State(state): axum::extract::State<crate::proxy::AppState>,
    headers: HeaderMap,
) -> axum::response::Response {
    let api_key = headers
        .get("x-xergon-public-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "anonymous".to_string());

    let info = state.tier_manager.get_tier_info(&api_key);

    axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(axum::body::Body::from(serde_json::to_string(&info).unwrap()))
        .unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_tiers() {
        let free = TierConfig::free();
        assert_eq!(free.requests_per_minute, 3);
        assert_eq!(free.requests_per_day, 100);

        let basic = TierConfig::basic();
        assert_eq!(basic.requests_per_minute, 30);
        assert_eq!(basic.requests_per_day, 10_000);

        let pro = TierConfig::pro();
        assert_eq!(pro.requests_per_minute, 120);
        assert_eq!(pro.requests_per_day, 100_000);

        let enterprise = TierConfig::enterprise();
        assert_eq!(enterprise.requests_per_minute, 10_000);
    }

    #[test]
    fn test_tier_manager_default_tier() {
        let tm = TierManager::new();
        assert_eq!(tm.get_tier("unknown_key"), RateLimitTier::Free);
    }

    #[test]
    fn test_tier_manager_set_tier() {
        let tm = TierManager::new();
        assert_eq!(tm.get_tier("key1"), RateLimitTier::Free);

        tm.set_tier("key1", RateLimitTier::Pro);
        assert_eq!(tm.get_tier("key1"), RateLimitTier::Pro);
    }

    #[test]
    fn test_tier_manager_remove_tier() {
        let tm = TierManager::new();
        tm.set_tier("key1", RateLimitTier::Pro);
        assert_eq!(tm.get_tier("key1"), RateLimitTier::Pro);

        tm.remove_tier("key1");
        assert_eq!(tm.get_tier("key1"), RateLimitTier::Free);
    }

    #[test]
    fn test_tier_check_allowed() {
        let tm = TierManager::new();
        // Free tier allows 3/min
        for _ in 0..3 {
            assert!(matches!(tm.check_tier_limit("user1"), TierCheckResult::Allowed { .. }));
        }
        // 4th should be rate limited
        assert!(matches!(tm.check_tier_limit("user1"), TierCheckResult::RateLimited { .. }));
    }

    #[test]
    fn test_enterprise_unlimited() {
        let tm = TierManager::new();
        tm.set_tier("vip", RateLimitTier::Enterprise);

        for _ in 0..100 {
            assert!(matches!(
                tm.check_tier_limit("vip"),
                TierCheckResult::Allowed { .. }
            ));
        }
    }

    #[test]
    fn test_tier_from_str() {
        assert_eq!(RateLimitTier::from_str_lossy("free"), RateLimitTier::Free);
        assert_eq!(RateLimitTier::from_str_lossy("BASIC"), RateLimitTier::Basic);
        assert_eq!(RateLimitTier::from_str_lossy("Pro"), RateLimitTier::Pro);
        assert_eq!(RateLimitTier::from_str_lossy("ENTERPRISE"), RateLimitTier::Enterprise);
        assert_eq!(RateLimitTier::from_str_lossy("unknown"), RateLimitTier::Free);
    }
}
