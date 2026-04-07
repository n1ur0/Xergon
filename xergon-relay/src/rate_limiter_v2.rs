//! Advanced rate limiting v2 — multi-algorithm, multi-scope, DashMap-backed
//!
//! Algorithms:
//! - TokenBucket: replenishes tokens over time, supports burst
//! - SlidingWindow: weighted count of requests in a rolling window
//! - FixedWindow: simple counter reset at window boundaries
//! - LeakyBucket: processes requests at a steady rate, queues excess
//!
//! Scopes: per-user, per-provider, per-model, per-ip, global

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Algorithm enum
// ---------------------------------------------------------------------------

/// Rate limiting algorithm to apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateLimitAlgorithm {
    TokenBucket,
    SlidingWindow,
    FixedWindow,
    LeakyBucket,
}

// ---------------------------------------------------------------------------
// Key extraction patterns
// ---------------------------------------------------------------------------

/// Describes how to extract a rate-limit key from a request context.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyExtractorPattern {
    /// Rate limit by user identity (from auth header / API key).
    PerUser,
    /// Rate limit by target provider endpoint.
    PerProvider,
    /// Rate limit by model name.
    PerModel,
    /// Rate limit by client IP address.
    PerIp,
    /// Global rate limit (single shared bucket).
    Global,
}

impl KeyExtractorPattern {
    /// Extract the rate-limit key from available context values.
    pub fn extract_key(
        &self,
        user_id: Option<&str>,
        provider: Option<&str>,
        model: Option<&str>,
        ip: Option<&str>,
    ) -> String {
        match self {
            KeyExtractorPattern::PerUser => {
                format!("user:{}", user_id.unwrap_or("anonymous"))
            }
            KeyExtractorPattern::PerProvider => {
                format!("provider:{}", provider.unwrap_or("unknown"))
            }
            KeyExtractorPattern::PerModel => {
                format!("model:{}", model.unwrap_or("unknown"))
            }
            KeyExtractorPattern::PerIp => {
                format!("ip:{}", ip.unwrap_or("0.0.0.0"))
            }
            KeyExtractorPattern::Global => "global".to_string(),
        }
    }

    /// Return a human-readable scope label.
    pub fn scope_label(&self) -> &'static str {
        match self {
            KeyExtractorPattern::PerUser => "per-user",
            KeyExtractorPattern::PerProvider => "per-provider",
            KeyExtractorPattern::PerModel => "per-model",
            KeyExtractorPattern::PerIp => "per-ip",
            KeyExtractorPattern::Global => "global",
        }
    }
}

// ---------------------------------------------------------------------------
// Rate limit rule
// ---------------------------------------------------------------------------

/// A single rate-limiting rule that can be dynamically added/removed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitRule {
    /// Unique identifier for this rule.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Maximum requests allowed per window.
    pub requests_per_window: u32,
    /// Window duration in seconds.
    pub window_secs: u64,
    /// Burst size (only used by TokenBucket / LeakyBucket).
    #[serde(default = "default_burst")]
    pub burst_size: u32,
    /// Algorithm to apply.
    pub algorithm: RateLimitAlgorithm,
    /// How to extract the key from request context.
    pub key_extractor: KeyExtractorPattern,
    /// Whether this rule is active.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Priority — lower numbers checked first.
    #[serde(default)]
    pub priority: u32,
}

fn default_burst() -> u32 {
    10
}
fn default_enabled() -> bool {
    true
}

impl RateLimitRule {
    /// Create a new rule with the given parameters.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        requests_per_window: u32,
        window_secs: u64,
        algorithm: RateLimitAlgorithm,
        key_extractor: KeyExtractorPattern,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            requests_per_window,
            window_secs,
            burst_size: default_burst(),
            algorithm,
            key_extractor,
            enabled: true,
            priority: 100,
        }
    }
}

// ---------------------------------------------------------------------------
// Rate limit result
// ---------------------------------------------------------------------------

/// Result of a rate limit check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitResult {
    /// Whether the request is allowed.
    pub allowed: bool,
    /// Remaining requests in the current window.
    pub remaining: u32,
    /// When the rate limit window resets (UTC).
    pub reset_at: DateTime<Utc>,
    /// If denied, seconds until the request would be allowed.
    pub retry_after_secs: Option<u64>,
    /// The rule ID that matched (if any).
    pub rule_id: Option<String>,
    /// The scope that was applied.
    pub scope: Option<String>,
    /// The algorithm used.
    pub algorithm: Option<String>,
}

impl RateLimitResult {
    /// Create an allowed result.
    pub fn allowed(remaining: u32, reset_at: DateTime<Utc>) -> Self {
        Self {
            allowed: true,
            remaining,
            reset_at,
            retry_after_secs: None,
            rule_id: None,
            scope: None,
            algorithm: None,
        }
    }

    /// Create a denied result.
    pub fn denied(retry_after_secs: u64, reset_at: DateTime<Utc>) -> Self {
        Self {
            allowed: false,
            remaining: 0,
            reset_at,
            retry_after_secs: Some(retry_after_secs),
            rule_id: None,
            scope: None,
            algorithm: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Per-key counter state (DashMap value)
// ---------------------------------------------------------------------------

/// Internal counter state for a single rate-limit key.
#[derive(Debug)]
struct CounterState {
    /// Number of requests in current window.
    count: AtomicU64,
    /// Tokens remaining (TokenBucket).
    tokens: AtomicU64,
    /// Last refill time (TokenBucket / LeakyBucket).
    last_refill: std::sync::Mutex<Instant>,
    /// Window start time.
    window_start: std::sync::Mutex<Instant>,
    /// Queue depth (LeakyBucket).
    queue_depth: AtomicU64,
    /// Last drain time (LeakyBucket).
    last_drain: std::sync::Mutex<Instant>,
}

impl CounterState {
    fn new(now: Instant) -> Self {
        Self {
            count: AtomicU64::new(0),
            tokens: AtomicU64::new(0),
            last_refill: std::sync::Mutex::new(now),
            window_start: std::sync::Mutex::new(now),
            queue_depth: AtomicU64::new(0),
            last_drain: std::sync::Mutex::new(now),
        }
    }
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

/// Aggregated rate limiter statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RateLimiterStats {
    /// Total number of requests checked.
    pub total_checks: u64,
    /// Total number of requests allowed.
    pub total_allowed: u64,
    /// Total number of requests denied.
    pub total_denied: u64,
    /// Number of active keys being tracked.
    pub active_keys: usize,
    /// Number of active rules.
    pub active_rules: usize,
    /// Per-rule denial counts.
    pub denials_by_rule: HashMap<String, u64>,
    /// Per-scope denial counts.
    pub denials_by_scope: HashMap<String, u64>,
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Global configuration for the rate limiter v2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimiterV2Config {
    /// Whether the rate limiter is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Default window in seconds for new rules.
    #[serde(default = "default_window_secs")]
    pub default_window_secs: u64,
    /// Default requests per window for new rules.
    #[serde(default = "default_requests_per_window")]
    pub default_requests_per_window: u32,
    /// Whether to emit X-RateLimit-* headers on responses.
    #[serde(default = "default_true")]
    pub emit_headers: bool,
    /// Whether to deny by default when no rules match.
    #[serde(default)]
    pub default_deny: bool,
    /// Maximum number of keys to track (LRU eviction).
    #[serde(default = "default_max_keys")]
    pub max_keys: usize,
}

fn default_true() -> bool {
    true
}
fn default_window_secs() -> u64 {
    60
}
fn default_requests_per_window() -> u32 {
    100
}
fn default_max_keys() -> usize {
    100_000
}

impl Default for RateLimiterV2Config {
    fn default() -> Self {
        Self {
            enabled: true,
            default_window_secs: default_window_secs(),
            default_requests_per_window: default_requests_per_window(),
            emit_headers: true,
            default_deny: false,
            max_keys: default_max_keys(),
        }
    }
}

// ---------------------------------------------------------------------------
// RateLimiterV2 — main struct
// ---------------------------------------------------------------------------

/// Advanced rate limiter with multiple algorithms and scopes.
#[derive(Clone)]
pub struct RateLimiterV2 {
    /// Per-key counter state (key = "scope:value").
    counters: Arc<DashMap<String, CounterState>>,
    /// Rules registry, keyed by rule ID.
    rules: Arc<DashMap<String, RateLimitRule>>,
    /// Per-rule denial counters.
    denials_by_rule: Arc<DashMap<String, AtomicU64>>,
    /// Per-scope denial counters.
    denials_by_scope: Arc<DashMap<String, AtomicU64>>,
    /// Global stats counters.
    total_checks: Arc<AtomicU64>,
    total_allowed: Arc<AtomicU64>,
    total_denied: Arc<AtomicU64>,
    /// Configuration.
    config: Arc<std::sync::RwLock<RateLimiterV2Config>>,
}

impl RateLimiterV2 {
    /// Create a new rate limiter with default config.
    pub fn new() -> Self {
        Self::with_config(RateLimiterV2Config::default())
    }

    /// Create a new rate limiter with the given config.
    pub fn with_config(config: RateLimiterV2Config) -> Self {
        Self {
            counters: Arc::new(DashMap::new()),
            rules: Arc::new(DashMap::new()),
            denials_by_rule: Arc::new(DashMap::new()),
            denials_by_scope: Arc::new(DashMap::new()),
            total_checks: Arc::new(AtomicU64::new(0)),
            total_allowed: Arc::new(AtomicU64::new(0)),
            total_denied: Arc::new(AtomicU64::new(0)),
            config: Arc::new(std::sync::RwLock::new(config)),
        }
    }

    /// Check rate limit for the given context.
    ///
    /// Returns the most restrictive result across all matching enabled rules.
    pub fn check_rate_limit(
        &self,
        user_id: Option<&str>,
        provider: Option<&str>,
        model: Option<&str>,
        ip: Option<&str>,
    ) -> RateLimitResult {
        self.total_checks.fetch_add(1, Ordering::Relaxed);

        let cfg = self.config.read().unwrap();
        if !cfg.enabled {
            self.total_allowed.fetch_add(1, Ordering::Relaxed);
            return RateLimitResult::allowed(u32::MAX, Utc::now());
        }

        let now = Instant::now();
        let mut most_restrictive: Option<RateLimitResult> = None;

        // Collect rules sorted by priority (lower = higher priority)
        let mut sorted_rules: Vec<RateLimitRule> = self
            .rules
            .iter()
            .filter(|r| r.value().enabled)
            .map(|r| r.value().clone())
            .collect();
        sorted_rules.sort_by_key(|r| r.priority);

        for rule in &sorted_rules {
            let key = format!(
                "{}:{}",
                rule.key_extractor.scope_label(),
                rule.key_extractor.extract_key(user_id, provider, model, ip)
            );

            let result = self.check_rule(&key, rule, now);
            let allowed = result.allowed;

            if !allowed {
                // Record denial
                self.total_denied.fetch_add(1, Ordering::Relaxed);
                self.denials_by_rule
                    .entry(rule.id.clone())
                    .or_insert_with(|| AtomicU64::new(0))
                    .fetch_add(1, Ordering::Relaxed);
                self.denials_by_scope
                    .entry(rule.key_extractor.scope_label().to_string())
                    .or_insert_with(|| AtomicU64::new(0))
                    .fetch_add(1, Ordering::Relaxed);
            }

            // Track the most restrictive denied result
            if !allowed {
                if most_restrictive.is_none() {
                    most_restrictive = Some(result);
                }
            }
        }

        drop(cfg);

        match most_restrictive {
            Some(mut denied) => denied,
            None => {
                // No rules matched — allow or deny based on config
                let cfg = self.config.read().unwrap();
                if cfg.default_deny && !sorted_rules.is_empty() {
                    // At least rules exist but none matched the scope; allow
                    self.total_allowed.fetch_add(1, Ordering::Relaxed);
                    RateLimitResult::allowed(u32::MAX, Utc::now())
                } else if cfg.default_deny {
                    self.total_denied.fetch_add(1, Ordering::Relaxed);
                    let retry = cfg.default_window_secs;
                    RateLimitResult::denied(retry, Utc::now() + chrono::Duration::seconds(retry as i64))
                } else {
                    self.total_allowed.fetch_add(1, Ordering::Relaxed);
                    RateLimitResult::allowed(u32::MAX, Utc::now())
                }
            }
        }
    }

    /// Check a single rule against a key.
    fn check_rule(&self, key: &str, rule: &RateLimitRule, now: Instant) -> RateLimitResult {
        // Evict old keys if over max
        self.maybe_evict();

        let counter = self
            .counters
            .entry(key.to_string())
            .or_insert_with(|| CounterState::new(now));

        match rule.algorithm {
            RateLimitAlgorithm::TokenBucket => self.check_token_bucket(&counter, rule, now),
            RateLimitAlgorithm::SlidingWindow => self.check_sliding_window(&counter, rule, now),
            RateLimitAlgorithm::FixedWindow => self.check_fixed_window(&counter, rule, now),
            RateLimitAlgorithm::LeakyBucket => self.check_leaky_bucket(&counter, rule, now),
        }
    }

    /// Token bucket: tokens refill at `requests_per_window / window_secs` per second.
    fn check_token_bucket(
        &self,
        counter: &CounterState,
        rule: &RateLimitRule,
        now: Instant,
    ) -> RateLimitResult {
        let mut last_refill = counter.last_refill.lock().unwrap();
        let elapsed = now.duration_since(*last_refill).as_secs_f64();
        let refill_rate = rule.requests_per_window as f64 / rule.window_secs as f64;
        let tokens_to_add = elapsed * refill_rate;
        let current_tokens = counter.tokens.load(Ordering::Relaxed) as f64;
        let burst = rule.burst_size.max(rule.requests_per_window) as f64;
        let new_tokens = (current_tokens + tokens_to_add).min(burst);

        counter.tokens.store(new_tokens as u64, Ordering::Relaxed);
        *last_refill = now;

        if new_tokens >= 1.0 {
            counter.tokens.store(new_tokens as u64 - 1, Ordering::Relaxed);
            let window_end = *last_refill + Duration::from_secs(rule.window_secs);
            let remaining = (new_tokens as u64).saturating_sub(1) as u32;
            let mut result = RateLimitResult::allowed(remaining, Utc::now());
            result.rule_id = Some(rule.id.clone());
            result.scope = Some(rule.key_extractor.scope_label().to_string());
            result.algorithm = Some("token_bucket".to_string());
            result
        } else {
            let retry_secs = ((1.0 - new_tokens) / refill_rate).ceil() as u64;
            let mut result = RateLimitResult::denied(retry_secs, Utc::now() + chrono::Duration::seconds(retry_secs as i64));
            result.rule_id = Some(rule.id.clone());
            result.scope = Some(rule.key_extractor.scope_label().to_string());
            result.algorithm = Some("token_bucket".to_string());
            result
        }
    }

    /// Sliding window: weighted count in a rolling window.
    fn check_sliding_window(
        &self,
        counter: &CounterState,
        rule: &RateLimitRule,
        now: Instant,
    ) -> RateLimitResult {
        let window_duration = Duration::from_secs(rule.window_secs);
        let mut window_start = counter.window_start.lock().unwrap();

        // Reset window if expired
        if now.duration_since(*window_start) >= window_duration {
            counter.count.store(0, Ordering::Relaxed);
            *window_start = now;
        }

        let count = counter.count.load(Ordering::Relaxed);
        let elapsed_fraction = now.duration_since(*window_start).as_secs_f64()
            / window_duration.as_secs_f64();
        // Weighted count for sliding window (simplified: current window weight)
        let weighted_count = if elapsed_fraction > 0.0 {
            (count as f64 * (1.0 - 0.5 * elapsed_fraction)).ceil() as u64
        } else {
            count
        };

        if weighted_count < rule.requests_per_window as u64 {
            counter.count.fetch_add(1, Ordering::Relaxed);
            let remaining = (rule.requests_per_window as u64 - weighted_count - 1) as u32;
            let reset_at = *window_start + window_duration;
            let mut result = RateLimitResult::allowed(remaining, Utc::now() + chrono::Duration::from_std(reset_at - now).unwrap_or_default());
            result.rule_id = Some(rule.id.clone());
            result.scope = Some(rule.key_extractor.scope_label().to_string());
            result.algorithm = Some("sliding_window".to_string());
            result
        } else {
            let retry_secs = window_duration.as_secs()
                - now.duration_since(*window_start).as_secs();
            let retry_secs = retry_secs.max(1);
            let mut result = RateLimitResult::denied(retry_secs, Utc::now() + chrono::Duration::seconds(retry_secs as i64));
            result.rule_id = Some(rule.id.clone());
            result.scope = Some(rule.key_extractor.scope_label().to_string());
            result.algorithm = Some("sliding_window".to_string());
            result
        }
    }

    /// Fixed window: simple counter that resets at window boundaries.
    fn check_fixed_window(
        &self,
        counter: &CounterState,
        rule: &RateLimitRule,
        now: Instant,
    ) -> RateLimitResult {
        let window_duration = Duration::from_secs(rule.window_secs);
        let mut window_start = counter.window_start.lock().unwrap();

        if now.duration_since(*window_start) >= window_duration {
            counter.count.store(0, Ordering::Relaxed);
            *window_start = now;
        }

        let count = counter.count.load(Ordering::Relaxed);
        if count < rule.requests_per_window as u64 {
            counter.count.fetch_add(1, Ordering::Relaxed);
            let remaining = (rule.requests_per_window as u64 - count - 1) as u32;
            let reset_at = *window_start + window_duration;
            let mut result = RateLimitResult::allowed(remaining, Utc::now() + chrono::Duration::from_std(reset_at - now).unwrap_or_default());
            result.rule_id = Some(rule.id.clone());
            result.scope = Some(rule.key_extractor.scope_label().to_string());
            result.algorithm = Some("fixed_window".to_string());
            result
        } else {
            let retry_secs = window_duration.as_secs()
                - now.duration_since(*window_start).as_secs();
            let retry_secs = retry_secs.max(1);
            let mut result = RateLimitResult::denied(retry_secs, Utc::now() + chrono::Duration::seconds(retry_secs as i64));
            result.rule_id = Some(rule.id.clone());
            result.scope = Some(rule.key_extractor.scope_label().to_string());
            result.algorithm = Some("fixed_window".to_string());
            result
        }
    }

    /// Leaky bucket: processes at steady rate, queues excess.
    fn check_leaky_bucket(
        &self,
        counter: &CounterState,
        rule: &RateLimitRule,
        now: Instant,
    ) -> RateLimitResult {
        let leak_rate = rule.requests_per_window as f64 / rule.window_secs as f64;
        let mut last_drain = counter.last_drain.lock().unwrap();
        let elapsed = now.duration_since(*last_drain).as_secs_f64();
        let leaked = (elapsed * leak_rate) as u64;

        let queue = counter.queue_depth.load(Ordering::Relaxed);
        let new_queue = queue.saturating_sub(leaked);
        counter.queue_depth.store(new_queue, Ordering::Relaxed);
        *last_drain = now;

        let burst = rule.burst_size as u64;

        if new_queue < burst {
            counter.queue_depth.fetch_add(1, Ordering::Relaxed);
            let remaining = (burst - new_queue - 1) as u32;
            let mut result = RateLimitResult::allowed(remaining, Utc::now());
            result.rule_id = Some(rule.id.clone());
            result.scope = Some(rule.key_extractor.scope_label().to_string());
            result.algorithm = Some("leaky_bucket".to_string());
            result
        } else {
            let retry_secs = ((new_queue - burst) as f64 / leak_rate).ceil() as u64;
            let retry_secs = retry_secs.max(1);
            let mut result = RateLimitResult::denied(retry_secs, Utc::now() + chrono::Duration::seconds(retry_secs as i64));
            result.rule_id = Some(rule.id.clone());
            result.scope = Some(rule.key_extractor.scope_label().to_string());
            result.algorithm = Some("leaky_bucket".to_string());
            result
        }
    }

    /// Evict excess keys when over max_keys limit.
    fn maybe_evict(&self) {
        let cfg = self.config.read().unwrap();
        if self.counters.len() <= cfg.max_keys {
            return;
        }
        drop(cfg);

        // Simple eviction: remove oldest keys (by approximate staleness)
        let now = Instant::now();
        let mut to_remove = Vec::new();
        for entry in self.counters.iter() {
            let ws = entry.value().window_start.lock().unwrap();
            if now.duration_since(*ws) > Duration::from_secs(300) {
                to_remove.push(entry.key().clone());
                if to_remove.len() >= 100 {
                    break;
                }
            }
        }
        for key in to_remove {
            self.counters.remove(&key);
        }
    }

    /// Add a new rule.
    pub fn add_rule(&self, rule: RateLimitRule) {
        self.rules.insert(rule.id.clone(), rule);
    }

    /// Remove a rule by ID.
    pub fn remove_rule(&self, id: &str) -> bool {
        self.rules.remove(id).is_some()
    }

    /// Get a rule by ID.
    pub fn get_rule(&self, id: &str) -> Option<RateLimitRule> {
        self.rules.get(id).map(|r| r.value().clone())
    }

    /// List all rules.
    pub fn list_rules(&self) -> Vec<RateLimitRule> {
        self.rules.iter().map(|r| r.value().clone()).collect()
    }

    /// Get aggregated statistics.
    pub fn get_stats(&self) -> RateLimiterStats {
        let active_keys = self.counters.len();
        let active_rules = self.rules.iter().filter(|r| r.value().enabled).count();
        let mut denials_by_rule = HashMap::new();
        for entry in self.denials_by_rule.iter() {
            denials_by_rule.insert(entry.key().clone(), entry.value().load(Ordering::Relaxed));
        }
        let mut denials_by_scope = HashMap::new();
        for entry in self.denials_by_scope.iter() {
            denials_by_scope.insert(entry.key().clone(), entry.value().load(Ordering::Relaxed));
        }
        RateLimiterStats {
            total_checks: self.total_checks.load(Ordering::Relaxed),
            total_allowed: self.total_allowed.load(Ordering::Relaxed),
            total_denied: self.total_denied.load(Ordering::Relaxed),
            active_keys,
            active_rules,
            denials_by_rule,
            denials_by_scope,
        }
    }

    /// Reset all counters and stats.
    pub fn reset(&self) {
        self.counters.clear();
        self.total_checks.store(0, Ordering::Relaxed);
        self.total_allowed.store(0, Ordering::Relaxed);
        self.total_denied.store(0, Ordering::Relaxed);
        for entry in self.denials_by_rule.iter() {
            entry.value().store(0, Ordering::Relaxed);
        }
        for entry in self.denials_by_scope.iter() {
            entry.value().store(0, Ordering::Relaxed);
        }
    }

    /// Get the current configuration.
    pub fn get_config(&self) -> RateLimiterV2Config {
        self.config.read().unwrap().clone()
    }

    /// Update the configuration.
    pub fn update_config(&self, new_config: RateLimiterV2Config) {
        let mut cfg = self.config.write().unwrap();
        *cfg = new_config;
    }

    /// Generate X-RateLimit-* headers from a rate limit result.
    pub fn rate_limit_headers(result: &RateLimitResult) -> Vec<(String, String)> {
        let mut headers = Vec::new();
        headers.push(("X-RateLimit-Limit".to_string(), "100".to_string()));
        headers.push((
            "X-RateLimit-Remaining".to_string(),
            result.remaining.to_string(),
        ));
        headers.push((
            "X-RateLimit-Reset".to_string(),
            result.reset_at.to_rfc3339(),
        ));
        if !result.allowed {
            if let Some(retry) = result.retry_after_secs {
                headers.push(("Retry-After".to_string(), retry.to_string()));
            }
        }
        if let Some(ref scope) = result.scope {
            headers.push(("X-RateLimit-Scope".to_string(), scope.clone()));
        }
        if let Some(ref algo) = result.algorithm {
            headers.push(("X-RateLimit-Algorithm".to_string(), algo.clone()));
        }
        headers
    }
}

impl Default for RateLimiterV2 {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// REST API request/response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CheckRateLimitRequest {
    pub user_id: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub ip: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CheckRateLimitResponse {
    pub result: RateLimitResult,
}

#[derive(Debug, Deserialize)]
pub struct AddRuleRequest {
    pub id: String,
    pub name: String,
    pub requests_per_window: u32,
    pub window_secs: u64,
    #[serde(default = "default_burst")]
    pub burst_size: u32,
    pub algorithm: RateLimitAlgorithm,
    pub key_extractor: KeyExtractorPattern,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub priority: u32,
}

#[derive(Debug, Serialize)]
pub struct RuleAddedResponse {
    pub id: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct RuleDeletedResponse {
    pub id: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct RulesListResponse {
    pub rules: Vec<RateLimitRule>,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub stats: RateLimiterStats,
}

#[derive(Debug, Serialize)]
pub struct ConfigResponse {
    pub config: RateLimiterV2Config,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConfigRequest {
    pub enabled: Option<bool>,
    pub emit_headers: Option<bool>,
    pub default_deny: Option<bool>,
    pub max_keys: Option<usize>,
    pub default_window_secs: Option<u64>,
    pub default_requests_per_window: Option<u32>,
}

// ---------------------------------------------------------------------------
// REST handlers
// ---------------------------------------------------------------------------

async fn check_rate_limit_handler(
    State(state): State<AppState>,
    Json(body): Json<CheckRateLimitRequest>,
) -> impl IntoResponse {
    let result = state.rate_limiter_v2.check_rate_limit(
        body.user_id.as_deref(),
        body.provider.as_deref(),
        body.model.as_deref(),
        body.ip.as_deref(),
    );
    (StatusCode::OK, Json(CheckRateLimitResponse { result }))
}

async fn list_rules_handler(State(state): State<AppState>) -> impl IntoResponse {
    let rules = state.rate_limiter_v2.list_rules();
    let count = rules.len();
    (StatusCode::OK, Json(RulesListResponse { rules, count }))
}

async fn add_rule_handler(
    State(state): State<AppState>,
    Json(body): Json<AddRuleRequest>,
) -> impl IntoResponse {
    let rule = RateLimitRule {
        id: body.id.clone(),
        name: body.name,
        requests_per_window: body.requests_per_window,
        window_secs: body.window_secs,
        burst_size: body.burst_size,
        algorithm: body.algorithm,
        key_extractor: body.key_extractor,
        enabled: body.enabled,
        priority: body.priority,
    };
    let id = rule.id.clone();
    state.rate_limiter_v2.add_rule(rule);
    info!(rule_id = %id, "Rate limit rule added");
    (StatusCode::CREATED, Json(RuleAddedResponse { id, status: "created".to_string() }))
}

async fn delete_rule_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let removed = state.rate_limiter_v2.remove_rule(&id);
    if removed {
        info!(rule_id = %id, "Rate limit rule removed");
        (StatusCode::OK, Json(RuleDeletedResponse { id, status: "deleted".to_string() }))
    } else {
        (StatusCode::NOT_FOUND, Json(RuleDeletedResponse { id, status: "not_found".to_string() }))
    }
}

async fn get_stats_handler(State(state): State<AppState>) -> impl IntoResponse {
    let stats = state.rate_limiter_v2.get_stats();
    (StatusCode::OK, Json(StatsResponse { stats }))
}

async fn get_config_handler(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.rate_limiter_v2.get_config();
    (StatusCode::OK, Json(ConfigResponse { config }))
}

async fn update_config_handler(
    State(state): State<AppState>,
    Json(body): Json<UpdateConfigRequest>,
) -> impl IntoResponse {
    let mut config = state.rate_limiter_v2.get_config();
    if let Some(enabled) = body.enabled {
        config.enabled = enabled;
    }
    if let Some(emit_headers) = body.emit_headers {
        config.emit_headers = emit_headers;
    }
    if let Some(default_deny) = body.default_deny {
        config.default_deny = default_deny;
    }
    if let Some(max_keys) = body.max_keys {
        config.max_keys = max_keys;
    }
    if let Some(window) = body.default_window_secs {
        config.default_window_secs = window;
    }
    if let Some(rpw) = body.default_requests_per_window {
        config.default_requests_per_window = rpw;
    }
    state.rate_limiter_v2.update_config(config.clone());
    info!("Rate limiter config updated");
    (StatusCode::OK, Json(ConfigResponse { config }))
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the rate limiter v2 API router.
pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/v1/rate-limit/check", post(check_rate_limit_handler))
        .route("/v1/rate-limit/rules", get(list_rules_handler))
        .route("/v1/rate-limit/rules", post(add_rule_handler))
        .route("/v1/rate-limit/rules/{id}", delete(delete_rule_handler))
        .route("/v1/rate-limit/stats", get(get_stats_handler))
        .route("/v1/rate-limit/config", get(get_config_handler))
        .route("/v1/rate-limit/config", post(update_config_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_empty_limiter() {
        let limiter = RateLimiterV2::new();
        assert_eq!(limiter.counters.len(), 0);
        assert_eq!(limiter.rules.len(), 0);
    }

    #[test]
    fn test_add_and_remove_rule() {
        let limiter = RateLimiterV2::new();
        let rule = RateLimitRule::new(
            "test-1",
            "Test Rule",
            10,
            60,
            RateLimitAlgorithm::FixedWindow,
            KeyExtractorPattern::PerIp,
        );
        limiter.add_rule(rule.clone());
        assert_eq!(limiter.rules.len(), 1);

        let fetched = limiter.get_rule("test-1").unwrap();
        assert_eq!(fetched.name, "Test Rule");

        assert!(limiter.remove_rule("test-1"));
        assert_eq!(limiter.rules.len(), 0);
        assert!(!limiter.remove_rule("nonexistent"));
    }

    #[test]
    fn test_fixed_window_allows_within_limit() {
        let limiter = RateLimiterV2::new();
        limiter.add_rule(RateLimitRule::new(
            "fw-1",
            "Fixed Window",
            5,
            60,
            RateLimitAlgorithm::FixedWindow,
            KeyExtractorPattern::PerIp,
        ));

        for _ in 0..5 {
            let result = limiter.check_rate_limit(None, None, None, Some("1.2.3.4"));
            assert!(result.allowed, "Should allow within limit");
        }
    }

    #[test]
    fn test_fixed_window_denies_over_limit() {
        let limiter = RateLimiterV2::new();
        limiter.add_rule(RateLimitRule::new(
            "fw-2",
            "Fixed Window Deny",
            3,
            60,
            RateLimitAlgorithm::FixedWindow,
            KeyExtractorPattern::Global,
        ));

        for _ in 0..3 {
            let result = limiter.check_rate_limit(None, None, None, None);
            assert!(result.allowed);
        }
        let result = limiter.check_rate_limit(None, None, None, None);
        assert!(!result.allowed);
        assert!(result.retry_after_secs.is_some());
    }

    #[test]
    fn test_token_bucket_with_burst() {
        let limiter = RateLimiterV2::new();
        let mut rule = RateLimitRule::new(
            "tb-1",
            "Token Bucket",
            2,
            60,
            RateLimitAlgorithm::TokenBucket,
            KeyExtractorPattern::PerUser,
        );
        rule.burst_size = 5;
        limiter.add_rule(rule);

        // Burst of 5 should be allowed
        for _ in 0..5 {
            let result = limiter.check_rate_limit(Some("user1"), None, None, None);
            assert!(result.allowed, "Burst should allow");
        }
        // 6th should be denied
        let result = limiter.check_rate_limit(Some("user1"), None, None, None);
        assert!(!result.allowed);
    }

    #[test]
    fn test_leaky_bucket_steady_rate() {
        let limiter = RateLimiterV2::new();
        let mut rule = RateLimitRule::new(
            "lb-1",
            "Leaky Bucket",
            10,
            10,
            RateLimitAlgorithm::LeakyBucket,
            KeyExtractorPattern::PerIp,
        );
        rule.burst_size = 3;
        limiter.add_rule(rule);

        for _ in 0..3 {
            let result = limiter.check_rate_limit(None, None, None, Some("10.0.0.1"));
            assert!(result.allowed, "Within burst");
        }
        let result = limiter.check_rate_limit(None, None, None, Some("10.0.0.1"));
        assert!(!result.allowed, "Over burst");
    }

    #[test]
    fn test_sliding_window_within_limit() {
        let limiter = RateLimiterV2::new();
        limiter.add_rule(RateLimitRule::new(
            "sw-1",
            "Sliding Window",
            10,
            60,
            RateLimitAlgorithm::SlidingWindow,
            KeyExtractorPattern::PerModel,
        ));

        for _ in 0..8 {
            let result = limiter.check_rate_limit(None, None, Some("gpt-4"), None);
            assert!(result.allowed);
        }
    }

    #[test]
    fn test_key_isolation_per_user() {
        let limiter = RateLimiterV2::new();
        limiter.add_rule(RateLimitRule::new(
            "ku-1",
            "Per User",
            2,
            60,
            RateLimitAlgorithm::FixedWindow,
            KeyExtractorPattern::PerUser,
        ));

        // User A uses 2 requests
        for _ in 0..2 {
            let r = limiter.check_rate_limit(Some("user-a"), None, None, None);
            assert!(r.allowed);
        }
        // User A denied
        assert!(!limiter.check_rate_limit(Some("user-a"), None, None, None).allowed);
        // User B still allowed
        assert!(limiter.check_rate_limit(Some("user-b"), None, None, None).allowed);
    }

    #[test]
    fn test_key_isolation_per_ip() {
        let limiter = RateLimiterV2::new();
        limiter.add_rule(RateLimitRule::new(
            "ki-1",
            "Per IP",
            1,
            60,
            RateLimitAlgorithm::FixedWindow,
            KeyExtractorPattern::PerIp,
        ));

        assert!(limiter.check_rate_limit(None, None, None, Some("1.1.1.1")).allowed);
        assert!(!limiter.check_rate_limit(None, None, None, Some("1.1.1.1")).allowed);
        assert!(limiter.check_rate_limit(None, None, None, Some("2.2.2.2")).allowed);
    }

    #[test]
    fn test_disabled_limiter_allows_all() {
        let limiter = RateLimiterV2::new();
        limiter.add_rule(RateLimitRule::new(
            "dis-1",
            "Disabled",
            1,
            60,
            RateLimitAlgorithm::FixedWindow,
            KeyExtractorPattern::Global,
        ));

        // Should work with rule
        assert!(limiter.check_rate_limit(None, None, None, None).allowed);

        // Disable the limiter
        limiter.update_config(RateLimiterV2Config {
            enabled: false,
            ..RateLimiterV2Config::default()
        });

        // Now everything allowed even past rule limits
        for _ in 0..10 {
            assert!(limiter.check_rate_limit(None, None, None, None).allowed);
        }
    }

    #[test]
    fn test_stats_tracking() {
        let limiter = RateLimiterV2::new();
        limiter.add_rule(RateLimitRule::new(
            "stat-1",
            "Stats Test",
            2,
            60,
            RateLimitAlgorithm::FixedWindow,
            KeyExtractorPattern::Global,
        ));

        for _ in 0..2 {
            limiter.check_rate_limit(None, None, None, None);
        }
        limiter.check_rate_limit(None, None, None, None); // denied

        let stats = limiter.get_stats();
        assert_eq!(stats.total_checks, 3);
        assert_eq!(stats.total_allowed, 2);
        assert_eq!(stats.total_denied, 1);
        assert_eq!(stats.active_rules, 1);
    }

    #[test]
    fn test_reset_clears_state() {
        let limiter = RateLimiterV2::new();
        limiter.add_rule(RateLimitRule::new(
            "rst-1",
            "Reset Test",
            1,
            60,
            RateLimitAlgorithm::FixedWindow,
            KeyExtractorPattern::Global,
        ));

        limiter.check_rate_limit(None, None, None, None);
        limiter.check_rate_limit(None, None, None, None); // denied

        limiter.reset();

        let stats = limiter.get_stats();
        assert_eq!(stats.total_checks, 0);
        assert_eq!(stats.total_denied, 0);

        // Should be allowed again after reset
        assert!(limiter.check_rate_limit(None, None, None, None).allowed);
    }

    #[test]
    fn test_rate_limit_headers() {
        let result = RateLimitResult::allowed(42, Utc::now());
        let headers = RateLimiterV2::rate_limit_headers(&result);
        assert!(headers.iter().any(|(k, _)| k == "X-RateLimit-Remaining"));

        let denied = RateLimitResult::denied(10, Utc::now());
        let headers = RateLimiterV2::rate_limit_headers(&denied);
        assert!(headers.iter().any(|(k, _)| k == "Retry-After"));
    }

    #[test]
    fn test_list_rules() {
        let limiter = RateLimiterV2::new();
        limiter.add_rule(RateLimitRule::new(
            "lr-1",
            "Rule 1",
            10,
            60,
            RateLimitAlgorithm::FixedWindow,
            KeyExtractorPattern::PerIp,
        ));
        limiter.add_rule(RateLimitRule::new(
            "lr-2",
            "Rule 2",
            20,
            120,
            RateLimitAlgorithm::TokenBucket,
            KeyExtractorPattern::PerUser,
        ));

        let rules = limiter.list_rules();
        assert_eq!(rules.len(), 2);
    }
}
