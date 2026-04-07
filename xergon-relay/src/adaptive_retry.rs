//! Adaptive Retry v2 -- token-bucket budgeted exponential backoff with jitter.
//!
//! Features:
//! - Exponential backoff with configurable multiplier and jitter
//! - Retry budget (token bucket) to prevent retry storms
//! - Per-provider budget tracking to prevent noisy-neighbor problems
//! - Adaptive budget: reduce on sustained failures, increase on success
//! - Error classification: retryable (429, 502, 503, 504, timeout) vs non-retryable
//! - Stats: attempts, successful retries, exhausted, budget-exhausted

use dashmap::DashMap;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the adaptive retry system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveRetryConfig {
    /// Maximum number of retries per request (default: 3).
    #[serde(default = "default_max_retries")]
    pub max_retries: usize,

    /// Base delay before first retry (default: 100ms).
    #[serde(default = "default_base_delay_ms")]
    pub base_delay_ms: u64,

    /// Maximum backoff delay cap (default: 30s).
    #[serde(default = "default_max_delay_ms")]
    pub max_delay_ms: u64,

    /// Exponential backoff multiplier (default: 2.0).
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,

    /// Jitter range 0.0–1.0; random factor added to delay (default: 0.5).
    #[serde(default = "default_jitter_range")]
    pub jitter_range: f64,

    /// Retry budget (token bucket) configuration.
    #[serde(default)]
    pub retry_budget: RetryBudgetConfig,

    /// Whether to maintain a separate budget per provider (default: true).
    #[serde(default = "default_per_provider_budget")]
    pub per_provider_budget: bool,
}

fn default_max_retries() -> usize {
    3
}
fn default_base_delay_ms() -> u64 {
    100
}
fn default_max_delay_ms() -> u64 {
    30_000
}
fn default_backoff_multiplier() -> f64 {
    2.0
}
fn default_jitter_range() -> f64 {
    0.5
}
fn default_per_provider_budget() -> bool {
    true
}

impl Default for AdaptiveRetryConfig {
    fn default() -> Self {
        Self {
            max_retries: default_max_retries(),
            base_delay_ms: default_base_delay_ms(),
            max_delay_ms: default_max_delay_ms(),
            backoff_multiplier: default_backoff_multiplier(),
            jitter_range: default_jitter_range(),
            retry_budget: RetryBudgetConfig::default(),
            per_provider_budget: default_per_provider_budget(),
        }
    }
}

/// Token-bucket budget configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryBudgetConfig {
    /// Tokens added per second (default: 10).
    #[serde(default = "default_token_rate")]
    pub token_rate: f64,

    /// Maximum token budget (default: 100).
    #[serde(default = "default_max_tokens")]
    pub max_tokens: f64,
}

fn default_token_rate() -> f64 {
    10.0
}
fn default_max_tokens() -> f64 {
    100.0
}

impl Default for RetryBudgetConfig {
    fn default() -> Self {
        Self {
            token_rate: default_token_rate(),
            max_tokens: default_max_tokens(),
        }
    }
}

// ---------------------------------------------------------------------------
// Error classification
// ---------------------------------------------------------------------------

/// Classifies an error as retryable or not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Retryable {
    /// The error is retryable (429, 502, 503, 504, timeout, connect).
    Yes,
    /// The error is not retryable (400, 401, 403, 404, etc.).
    No,
}

impl Retryable {
    /// Classify an HTTP status code.
    pub fn from_status(status: u16) -> Self {
        match status {
            429 => Retryable::Yes,
            502 | 503 | 504 => Retryable::Yes,
            _ if (500..600).contains(&status) => Retryable::Yes,
            _ => Retryable::No,
        }
    }

    /// Classify a reqwest error kind.
    pub fn from_reqwest_error(err: &reqwest::Error) -> Self {
        if err.is_timeout() || err.is_connect() {
            Retryable::Yes
        } else if let Some(status) = err.status() {
            Self::from_status(status.as_u16())
        } else {
            Retryable::No
        }
    }
}

// ---------------------------------------------------------------------------
// Retry stats
// ---------------------------------------------------------------------------

/// Atomic counters for retry statistics.
#[derive(Debug, Default)]
pub struct RetryStats {
    pub total_attempts: AtomicU64,
    pub successful_retries: AtomicU64,
    pub exhausted_retries: AtomicU64,
    pub budget_exhausted: AtomicU64,
    pub current_budget: AtomicU64,
}

impl RetryStats {
    pub fn snapshot(&self) -> RetryStatsSnapshot {
        RetryStatsSnapshot {
            total_attempts: self.total_attempts.load(Ordering::Relaxed),
            successful_retries: self.successful_retries.load(Ordering::Relaxed),
            exhausted_retries: self.exhausted_retries.load(Ordering::Relaxed),
            budget_exhausted: self.budget_exhausted.load(Ordering::Relaxed),
            current_budget: self.current_budget.load(Ordering::Relaxed),
        }
    }
}

/// A point-in-time snapshot of retry stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryStatsSnapshot {
    pub total_attempts: u64,
    pub successful_retries: u64,
    pub exhausted_retries: u64,
    pub budget_exhausted: u64,
    pub current_budget: u64,
}

// ---------------------------------------------------------------------------
// Per-provider budget bucket
// ---------------------------------------------------------------------------

/// A single token bucket for retry budgeting.
struct TokenBucket {
    tokens: std::sync::Mutex<f64>,
    max_tokens: f64,
    token_rate: f64,
    last_refill: std::sync::Mutex<Instant>,
}

impl TokenBucket {
    fn new(config: &RetryBudgetConfig) -> Self {
        Self {
            tokens: std::sync::Mutex::new(config.max_tokens),
            max_tokens: config.max_tokens,
            token_rate: config.token_rate,
            last_refill: std::sync::Mutex::new(Instant::now()),
        }
    }

    /// Try to consume one token. Returns true if allowed.
    fn try_consume(&self) -> bool {
        self.refill();
        let mut tokens = self.tokens.lock().unwrap();
        if *tokens >= 1.0 {
            *tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Refill tokens based on elapsed time.
    fn refill(&self) {
        let now = Instant::now();
        let mut last = self.last_refill.lock().unwrap();
        let elapsed = now.duration_since(*last).as_secs_f64();
        if elapsed > 0.0 {
            let mut tokens = self.tokens.lock().unwrap();
            *tokens = (*tokens + self.token_rate * elapsed).min(self.max_tokens);
            *last = now;
        }
    }

    /// Current token count (approximate, for stats).
    fn current(&self) -> f64 {
        self.refill();
        *self.tokens.lock().unwrap()
    }

    /// Reset bucket to max.
    fn reset(&self) {
        let mut tokens = self.tokens.lock().unwrap();
        *tokens = self.max_tokens;
        *self.last_refill.lock().unwrap() = Instant::now();
    }
}

// ---------------------------------------------------------------------------
// Adaptive retry engine
// ---------------------------------------------------------------------------

/// The core adaptive retry engine.
pub struct AdaptiveRetry {
    config: std::sync::RwLock<AdaptiveRetryConfig>,
    global_bucket: Mutex<TokenBucket>,
    per_provider_buckets: DashMap<String, TokenBucket>,
    stats: RetryStats,
}

impl AdaptiveRetry {
    /// Create a new adaptive retry engine with the given config.
    pub fn new(config: AdaptiveRetryConfig) -> Self {
        let global_bucket = TokenBucket::new(&config.retry_budget);
        let stats = RetryStats::default();
        // Initialize current_budget stat to max_tokens
        stats
            .current_budget
            .store(config.retry_budget.max_tokens as u64, Ordering::Relaxed);
        Self {
            config: std::sync::RwLock::new(config),
            global_bucket: Mutex::new(global_bucket),
            per_provider_buckets: DashMap::new(),
            stats,
        }
    }

    /// Determine if a retry should be attempted for the given provider and error.
    ///
    /// Returns `Some(delay)` if the retry should proceed (with the backoff delay),
    /// or `None` if the error is non-retryable, budget is exhausted, or retries
    /// are exceeded.
    pub fn should_retry(
        &self,
        provider_id: &str,
        attempt: usize, // 0-based attempt index (0 = first attempt, 1 = first retry, ...)
        error: &Retryable,
    ) -> Option<Duration> {
        self.stats.total_attempts.fetch_add(1, Ordering::Relaxed);

        // Classify the error
        if *error != Retryable::Yes {
            debug!(
                provider = provider_id,
                attempt = attempt,
                "Error is non-retryable, skipping retry"
            );
            return None;
        }

        let config = self.config.read().unwrap();

        // Check retry count
        if attempt >= config.max_retries {
            self.stats.exhausted_retries.fetch_add(1, Ordering::Relaxed);
            debug!(
                provider = provider_id,
                attempt = attempt,
                max = config.max_retries,
                "Max retries exceeded"
            );
            return None;
        }

        // Check global budget
        if !self.global_bucket.lock().unwrap().try_consume() {
            self.stats.budget_exhausted.fetch_add(1, Ordering::Relaxed);
            warn!(
                provider = provider_id,
                "Global retry budget exhausted"
            );
            return None;
        }

        // Check per-provider budget if enabled
        if config.per_provider_budget {
            let provider_bucket = self
                .per_provider_buckets
                .entry(provider_id.to_string())
                .or_insert_with(|| TokenBucket::new(&config.retry_budget));

            if !provider_bucket.try_consume() {
                self.stats.budget_exhausted.fetch_add(1, Ordering::Relaxed);
                warn!(
                    provider = provider_id,
                    "Per-provider retry budget exhausted"
                );
                return None;
            }
        }

        // Compute backoff delay
        let delay = self.compute_backoff(attempt, &config);
        self.stats
            .successful_retries
            .fetch_add(1, Ordering::Relaxed);

        debug!(
            provider = provider_id,
            attempt = attempt,
            delay_ms = delay.as_millis(),
            "Retry allowed"
        );

        Some(delay)
    }

    /// Record a successful outcome (adaptive budget increase).
    pub fn record_success(&self, provider_id: &str) {
        let config = self.config.read().unwrap();
        if config.per_provider_budget {
            if let Some(mut entry) = self.per_provider_buckets.get_mut(provider_id) {
                let bucket = entry.value_mut();
                let mut tokens = bucket.tokens.lock().unwrap();
                // Add a bonus token on success (up to max)
                *tokens = (*tokens + 0.5).min(bucket.max_tokens);
            }
        }
    }

    /// Record a failure outcome (adaptive budget reduction).
    pub fn record_failure(&self, provider_id: &str) {
        let config = self.config.read().unwrap();
        if config.per_provider_budget {
            if let Some(mut entry) = self.per_provider_buckets.get_mut(provider_id) {
                let bucket = entry.value_mut();
                let mut tokens = bucket.tokens.lock().unwrap();
                // Reduce tokens on failure
                *tokens = (*tokens - 1.0).max(0.0);
            }
        }
    }

    /// Compute exponential backoff delay with jitter.
    fn compute_backoff(&self, attempt: usize, config: &AdaptiveRetryConfig) -> Duration {
        let base_delay = Duration::from_millis(config.base_delay_ms);
        let max_delay = Duration::from_millis(config.max_delay_ms);

        // Exponential: base * multiplier^attempt
        let raw_ms = base_delay.as_millis() as f64
            * config.backoff_multiplier.powi(attempt as i32);

        // Add jitter: raw * (1 + jitter_range * random(0..1))
        let jitter_factor = if config.jitter_range > 0.0 {
            1.0 + config.jitter_range * rand::rng().random_range(0.0..1.0)
        } else {
            1.0
        };

        let final_ms = (raw_ms * jitter_factor) as u64;
        let final_delay = Duration::from_millis(final_ms);

        final_delay.min(max_delay)
    }

    /// Get current configuration.
    pub fn get_config(&self) -> AdaptiveRetryConfig {
        self.config.read().unwrap().clone()
    }

    /// Update configuration.
    pub fn set_config(&self, new_config: AdaptiveRetryConfig) {
        let mut config = self.config.write().unwrap();
        *config = new_config.clone();

        // Reset global bucket with new config
        *self.global_bucket.lock().unwrap() = TokenBucket::new(&new_config.retry_budget);
        self.stats
            .current_budget
            .store(new_config.retry_budget.max_tokens as u64, Ordering::Relaxed);

        // Clear per-provider buckets so they get recreated with new config
        self.per_provider_buckets.clear();

        info!(
            max_retries = new_config.max_retries,
            base_delay_ms = new_config.base_delay_ms,
            max_delay_ms = new_config.max_delay_ms,
            backoff_multiplier = new_config.backoff_multiplier,
            jitter_range = new_config.jitter_range,
            token_rate = new_config.retry_budget.token_rate,
            max_tokens = new_config.retry_budget.max_tokens,
            per_provider = new_config.per_provider_budget,
            "Adaptive retry config updated"
        );
    }

    /// Get retry statistics.
    pub fn get_stats(&self) -> RetryStatsSnapshot {
        let mut snapshot = self.stats.snapshot();
        // Update current_budget from global bucket
        snapshot.current_budget = self.global_bucket.lock().unwrap().current() as u64;
        self.stats
            .current_budget
            .store(snapshot.current_budget, Ordering::Relaxed);
        snapshot
    }

    /// Reset all retry budgets to max.
    pub fn reset_budget(&self) {
        self.global_bucket.lock().unwrap().reset();
        for entry in self.per_provider_buckets.iter() {
            entry.value().reset();
        }
        self.stats
            .current_budget
            .store(
                self.config.read().unwrap().retry_budget.max_tokens as u64,
                Ordering::Relaxed,
            );
        info!("Retry budgets reset to max");
    }

    /// Get budget status (global + per-provider).
    pub fn get_budget_status(&self) -> serde_json::Value {
        let config = self.config.read().unwrap();
        let global_current = self.global_bucket.lock().unwrap().current();

        let mut per_provider = serde_json::Map::new();
        if config.per_provider_budget {
            for entry in self.per_provider_buckets.iter() {
                per_provider.insert(
                    entry.key().clone(),
                    serde_json::json!({
                        "current_tokens": entry.value().current(),
                        "max_tokens": entry.value().max_tokens,
                    }),
                );
            }
        }

        serde_json::json!({
            "global": {
                "current_tokens": global_current,
                "max_tokens": config.retry_budget.max_tokens,
                "token_rate": config.retry_budget.token_rate,
            },
            "per_provider": per_provider,
            "per_provider_budget_enabled": config.per_provider_budget,
        })
    }
}

// ---------------------------------------------------------------------------
// API handlers
// ---------------------------------------------------------------------------

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use crate::proxy::AppState;

/// GET /api/retry/stats
pub async fn retry_stats_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let stats = state.adaptive_retry.get_stats();
    Json(serde_json::json!({
        "total_attempts": stats.total_attempts,
        "successful_retries": stats.successful_retries,
        "exhausted_retries": stats.exhausted_retries,
        "budget_exhausted": stats.budget_exhausted,
        "current_budget": stats.current_budget,
    }))
}

/// GET /api/retry/config
pub async fn retry_config_handler(
    State(state): State<AppState>,
) -> Json<AdaptiveRetryConfig> {
    Json(state.adaptive_retry.get_config())
}

/// PATCH /api/retry/config
pub async fn retry_config_update_handler(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let current = state.adaptive_retry.get_config();

    // Merge: only overwrite fields present in the body
    let updated = AdaptiveRetryConfig {
        max_retries: body
            .get("max_retries")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(current.max_retries),
        base_delay_ms: body
            .get("base_delay_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(current.base_delay_ms),
        max_delay_ms: body
            .get("max_delay_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(current.max_delay_ms),
        backoff_multiplier: body
            .get("backoff_multiplier")
            .and_then(|v| v.as_f64())
            .unwrap_or(current.backoff_multiplier),
        jitter_range: body
            .get("jitter_range")
            .and_then(|v| v.as_f64())
            .unwrap_or(current.jitter_range),
        retry_budget: {
            let budget_body = body.get("retry_budget");
            let cur_budget = &current.retry_budget;
            RetryBudgetConfig {
                token_rate: budget_body
                    .and_then(|b| b.get("token_rate"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(cur_budget.token_rate),
                max_tokens: budget_body
                    .and_then(|b| b.get("max_tokens"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(cur_budget.max_tokens),
            }
        },
        per_provider_budget: body
            .get("per_provider_budget")
            .and_then(|v| v.as_bool())
            .unwrap_or(current.per_provider_budget),
    };

    state.adaptive_retry.set_config(updated.clone());

    (StatusCode::OK, Json(updated))
}

/// POST /api/retry/budget/reset
pub async fn retry_budget_reset_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    state.adaptive_retry.reset_budget();
    (StatusCode::OK, "retry budgets reset")
}

/// GET /api/retry/budget
pub async fn retry_budget_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    Json(state.adaptive_retry.get_budget_status())
}

/// Build the retry API router.
pub fn build_retry_router() -> Router<AppState> {
    Router::new()
        .route("/api/retry/stats", get(retry_stats_handler))
        .route("/api/retry/config", get(retry_config_handler))
        .route("/api/retry/config", axum::routing::patch(retry_config_update_handler))
        .route("/api/retry/budget/reset", post(retry_budget_reset_handler))
        .route("/api/retry/budget", get(retry_budget_handler))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_classification_retryable() {
        assert_eq!(Retryable::from_status(429), Retryable::Yes);
        assert_eq!(Retryable::from_status(502), Retryable::Yes);
        assert_eq!(Retryable::from_status(503), Retryable::Yes);
        assert_eq!(Retryable::from_status(504), Retryable::Yes);
        assert_eq!(Retryable::from_status(500), Retryable::Yes);
        assert_eq!(Retryable::from_status(503), Retryable::Yes);
    }

    #[test]
    fn test_error_classification_non_retryable() {
        assert_eq!(Retryable::from_status(400), Retryable::No);
        assert_eq!(Retryable::from_status(401), Retryable::No);
        assert_eq!(Retryable::from_status(403), Retryable::No);
        assert_eq!(Retryable::from_status(404), Retryable::No);
        assert_eq!(Retryable::from_status(200), Retryable::No);
    }

    #[test]
    fn test_should_retry_non_retryable() {
        let retry = AdaptiveRetry::new(AdaptiveRetryConfig::default());
        assert!(retry.should_retry("p1", 0, &Retryable::No).is_none());
    }

    #[test]
    fn test_should_retry_within_budget() {
        let retry = AdaptiveRetry::new(AdaptiveRetryConfig::default());
        let result = retry.should_retry("p1", 0, &Retryable::Yes);
        assert!(result.is_some());
        // Delay should be at least base_delay (100ms) with jitter
        assert!(result.unwrap() >= Duration::from_millis(100));
    }

    #[test]
    fn test_should_retry_exhausted_attempts() {
        let config = AdaptiveRetryConfig {
            max_retries: 2,
            ..Default::default()
        };
        let retry = AdaptiveRetry::new(config);
        assert!(retry.should_retry("p1", 0, &Retryable::Yes).is_some());
        assert!(retry.should_retry("p1", 1, &Retryable::Yes).is_some());
        assert!(retry.should_retry("p1", 2, &Retryable::Yes).is_none());
    }

    #[test]
    fn test_backoff_increases() {
        let config = AdaptiveRetryConfig {
            jitter_range: 0.0, // disable jitter for deterministic test
            ..Default::default()
        };
        let retry = AdaptiveRetry::new(config);
        let d0 = retry.should_retry("p1", 0, &Retryable::Yes).unwrap();
        let d1 = retry.should_retry("p2", 1, &Retryable::Yes).unwrap();
        assert!(d1 > d0, "backoff should increase: {:?} > {:?}", d1, d0);
    }

    #[test]
    fn test_budget_reset() {
        let retry = AdaptiveRetry::new(AdaptiveRetryConfig::default());
        // Drain budget
        for _ in 0..200 {
            retry.should_retry("p1", 0, &Retryable::Yes);
        }
        // Budget should be exhausted
        assert!(retry.should_retry("p1", 0, &Retryable::Yes).is_none());
        // Reset
        retry.reset_budget();
        assert!(retry.should_retry("p1", 0, &Retryable::Yes).is_some());
    }

    #[test]
    fn test_adaptive_success_failure() {
        let retry = AdaptiveRetry::new(AdaptiveRetryConfig::default());
        // Initial retry should work
        assert!(retry.should_retry("p1", 0, &Retryable::Yes).is_some());

        // Record many failures to drain per-provider budget
        for _ in 0..200 {
            retry.record_failure("p1");
        }

        // After failures, per-provider budget should be drained
        // (global budget still has tokens but per-provider is 0)
        let result = retry.should_retry("p1", 0, &Retryable::Yes);
        assert!(result.is_none(), "per-provider budget should be drained");

        // Record success to replenish
        for _ in 0..300 {
            retry.record_success("p1");
        }

        // Should work again
        assert!(retry.should_retry("p1", 0, &Retryable::Yes).is_some());
    }

    #[test]
    fn test_config_update() {
        let retry = AdaptiveRetry::new(AdaptiveRetryConfig::default());
        let new_config = AdaptiveRetryConfig {
            max_retries: 5,
            base_delay_ms: 200,
            ..Default::default()
        };
        retry.set_config(new_config.clone());
        let read_back = retry.get_config();
        assert_eq!(read_back.max_retries, 5);
        assert_eq!(read_back.base_delay_ms, 200);
    }
}
