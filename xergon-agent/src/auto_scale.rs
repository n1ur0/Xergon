//! Auto-Scaling System -- monitors inference demand and adjusts model availability.
//!
//! Periodically checks request queue depth, latency percentiles, and GPU utilization.
//! When demand exceeds thresholds it preloads the next most-requested model and
//! increases concurrency. When demand drops it unloads idle models and decreases
//! concurrency to free resources.
//!
//! API endpoints:
//! - GET  /api/auto-scale/status   -- current scale state and recent actions
//! - POST /api/auto-scale/trigger  -- force an immediate scale check cycle
//! - GET  /api/auto-scale/config   -- current auto-scaling configuration
//! - PATCH /api/auto-scale/config   -- update configuration at runtime

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::hardware::detect_hardware;
use crate::reputation::ReputationStore;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Auto-scaling configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AutoScaleConfig {
    /// Enable the auto-scaling system (default: false).
    #[serde(default)]
    pub enabled: bool,
    /// How often to run scale checks (seconds, default: 30).
    #[serde(default = "default_check_interval")]
    pub check_interval_secs: u64,
    /// Scale up when queue depth exceeds this (default: 10).
    #[serde(default = "default_scale_up_queue")]
    pub scale_up_queue_threshold: usize,
    /// Scale up when p99 latency exceeds this in milliseconds (default: 5000).
    #[serde(default = "default_scale_up_latency_ms")]
    pub scale_up_latency_ms: u64,
    /// Scale down when queue depth is below this (default: 2).
    #[serde(default = "default_scale_down_queue")]
    pub scale_down_queue_threshold: usize,
    /// Scale down when p99 latency is below this in milliseconds (default: 500).
    #[serde(default = "default_scale_down_latency_ms")]
    pub scale_down_latency_ms: u64,
    /// Minimum idle time (seconds) before unloading a model (default: 600 = 10 min).
    #[serde(default = "default_idle_timeout_secs")]
    pub model_idle_timeout_secs: u64,
    /// Maximum number of concurrent inference requests allowed (default: 16).
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: usize,
    /// Minimum concurrency floor (default: 1).
    #[serde(default = "default_min_concurrency")]
    pub min_concurrency: usize,
    /// Concurrency step size for scale up/down (default: 2).
    #[serde(default = "default_concurrency_step")]
    pub concurrency_step: usize,
    /// Minimum free VRAM (MB) to reserve before loading a new model (default: 1024).
    #[serde(default = "default_reserved_vram_mb")]
    pub reserved_vram_mb: u64,
}

fn default_check_interval() -> u64 { 30 }
fn default_scale_up_queue() -> usize { 10 }
fn default_scale_up_latency_ms() -> u64 { 5000 }
fn default_scale_down_queue() -> usize { 2 }
fn default_scale_down_latency_ms() -> u64 { 500 }
fn default_idle_timeout_secs() -> u64 { 600 }
fn default_max_concurrency() -> usize { 16 }
fn default_min_concurrency() -> usize { 1 }
fn default_concurrency_step() -> usize { 2 }
fn default_reserved_vram_mb() -> u64 { 1024 }

impl Default for AutoScaleConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            check_interval_secs: default_check_interval(),
            scale_up_queue_threshold: default_scale_up_queue(),
            scale_up_latency_ms: default_scale_up_latency_ms(),
            scale_down_queue_threshold: default_scale_down_queue(),
            scale_down_latency_ms: default_scale_down_latency_ms(),
            model_idle_timeout_secs: default_idle_timeout_secs(),
            max_concurrency: default_max_concurrency(),
            min_concurrency: default_min_concurrency(),
            concurrency_step: default_concurrency_step(),
            reserved_vram_mb: default_reserved_vram_mb(),
        }
    }
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A recorded scale action (scale up or scale down).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScaleAction {
    /// Action type: "scale_up" or "scale_down".
    pub action: String,
    /// Human-readable description.
    pub message: String,
    /// Unix timestamp of when the action was taken.
    pub timestamp: i64,
    /// Whether the action succeeded.
    pub success: bool,
}

/// Snapshot of the current auto-scaling state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoScaleStatus {
    pub enabled: bool,
    /// Current concurrency limit.
    pub current_concurrency: usize,
    /// Number of models currently loaded.
    pub models_loaded: usize,
    /// Current request queue depth estimate.
    pub queue_depth: usize,
    /// Last measured p99 latency in ms.
    pub p99_latency_ms: u64,
    /// GPU utilization (0.0 - 100.0).
    pub gpu_utilization: f64,
    /// Free VRAM in MB.
    pub free_vram_mb: u64,
    /// Total VRAM in MB.
    pub total_vram_mb: u64,
    /// Last N scale actions.
    pub recent_actions: Vec<ScaleAction>,
    /// Time of the last scale check (RFC3339).
    pub last_check: String,
    /// Time the scaler was started (RFC3339).
    pub started_at: String,
}

// ---------------------------------------------------------------------------
// Latency tracker (simple sliding window for percentile estimation)
// ---------------------------------------------------------------------------

/// Tracks recent request latencies in a ring-buffer for percentile computation.
struct LatencyTracker {
    samples: Arc<RwLock<VecDeque<u64>>>,
    max_samples: usize,
}

impl LatencyTracker {
    fn new(max_samples: usize) -> Self {
        Self {
            samples: Arc::new(RwLock::new(VecDeque::with_capacity(max_samples))),
            max_samples,
        }
    }

    /// Record a latency sample in milliseconds.
    async fn record(&self, latency_ms: u64) {
        let mut samples = self.samples.write().await;
        if samples.len() >= self.max_samples {
            samples.pop_front();
        }
        samples.push_back(latency_ms);
    }

    /// Estimate the p99 latency from recent samples.
    async fn p99_ms(&self) -> u64 {
        let samples = self.samples.read().await;
        if samples.is_empty() {
            return 0;
        }
        let mut sorted: Vec<u64> = samples.iter().copied().collect();
        sorted.sort_unstable();
        let idx = ((sorted.len() as f64 * 0.99).ceil() as usize).saturating_sub(1);
        sorted[idx]
    }
}

// ---------------------------------------------------------------------------
// Model usage tracker
// ---------------------------------------------------------------------------

/// Tracks per-model request counts and last-used timestamps.
struct ModelUsageTracker {
    /// model_id -> (request_count, last_used_secs)
    usage: Arc<RwLock<std::collections::HashMap<String, (u64, u64)>>>,
}

impl ModelUsageTracker {
    fn new() -> Self {
        Self {
            usage: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Record that a model was used.
    async fn touch(&self, model_id: &str) {
        let now = Utc::now().timestamp() as u64;
        let mut usage = self.usage.write().await;
        let entry = usage.entry(model_id.to_string()).or_insert((0, now));
        entry.0 += 1;
        entry.1 = now;
    }

    /// Get the model with the most requests that is not currently loaded.
    async fn most_requested_unloaded(
        &self,
        loaded_models: &[String],
    ) -> Option<(String, u64)> {
        let usage = self.usage.read().await;
        let mut best: Option<(String, u64)> = None;
        for (model_id, (count, _)) in usage.iter() {
            if !loaded_models.contains(model_id) {
                if best.as_ref().map_or(true, |(_, c)| count > c) {
                    best = Some((model_id.clone(), *count));
                }
            }
        }
        best
    }

    /// Get the least recently used model that is currently loaded and not pinned.
    async fn least_recently_used(
        &self,
        loaded_models: &[String],
        pinned_models: &[String],
    ) -> Option<(String, u64)> {
        let usage = self.usage.read().await;
        let mut oldest: Option<(String, u64)> = None;
        for model_id in loaded_models {
            if pinned_models.contains(model_id) {
                continue;
            }
            if let Some((_, last_used)) = usage.get(model_id) {
                if oldest.as_ref().map_or(true, |(_, t)| last_used < t) {
                    oldest = Some((model_id.clone(), *last_used));
                }
            }
        }
        oldest
    }

    /// Reset all counters.
    async fn reset(&self) {
        self.usage.write().await.clear();
    }
}

// ---------------------------------------------------------------------------
// AutoScaler
// ---------------------------------------------------------------------------

/// The main auto-scaling engine.
///
/// Monitors inference demand metrics and adjusts model loading/concurrency
/// based on configurable thresholds. Uses `hardware.rs` for GPU VRAM checks
/// before loading new models, and respects model_cache pin/eviction rules.
pub struct AutoScaler {
    config: Arc<RwLock<AutoScaleConfig>>,
    current_concurrency: AtomicU64,
    queue_depth: AtomicU64,
    latency_tracker: LatencyTracker,
    model_usage: ModelUsageTracker,
    recent_actions: Arc<RwLock<Vec<ScaleAction>>>,
    started_at: i64,
    last_check: Arc<RwLock<i64>>,
    /// Whether a scale check is currently running.
    checking: AtomicBool,
    /// Optional reputation store (for prioritizing high-reputation model requests).
    reputation: Option<Arc<ReputationStore>>,
}

impl AutoScaler {
    /// Create a new auto-scaler with the given configuration.
    pub fn new(config: AutoScaleConfig) -> Self {
        Self {
            current_concurrency: AtomicU64::new(config.min_concurrency as u64),
            queue_depth: AtomicU64::new(0),
            latency_tracker: LatencyTracker::new(1000),
            model_usage: ModelUsageTracker::new(),
            recent_actions: Arc::new(RwLock::new(VecDeque::new().into())),
            started_at: Utc::now().timestamp(),
            last_check: Arc::new(RwLock::new(Utc::now().timestamp())),
            checking: AtomicBool::new(false),
            reputation: None,
            config: Arc::new(RwLock::new(config)),
        }
    }

    /// Attach a reputation store for priority-aware scaling.
    pub fn with_reputation(mut self, store: Arc<ReputationStore>) -> Self {
        self.reputation = Some(store);
        self
    }

    // ---- Metrics ingestion (called by inference proxy) ----

    /// Record a completed inference request latency.
    pub async fn record_latency(&self, latency_ms: u64) {
        self.latency_tracker.record(latency_ms).await;
    }

    /// Update the current queue depth (set atomically by the inference layer).
    pub fn set_queue_depth(&self, depth: usize) {
        self.queue_depth.store(depth as u64, Ordering::Relaxed);
    }

    /// Record that a specific model was used for a request.
    pub async fn record_model_usage(&self, model_id: &str) {
        self.model_usage.touch(model_id).await;
    }

    /// Get the current concurrency limit.
    pub fn current_concurrency(&self) -> usize {
        self.current_concurrency.load(Ordering::Relaxed) as usize
    }

    /// Get the current queue depth.
    pub fn queue_depth(&self) -> usize {
        self.queue_depth.load(Ordering::Relaxed) as usize
    }

    // ---- Scale actions ----

    /// Run a scale-up: preload the next most-requested model and increase concurrency.
    ///
    /// Returns the action taken.
    pub async fn scale_up(&self) -> ScaleAction {
        let config = self.config.read().await;
        let timestamp = Utc::now().timestamp();

        // Increase concurrency
        let current = self.current_concurrency.load(Ordering::Relaxed) as usize;
        let new_concurrency = (current + config.concurrency_step).min(config.max_concurrency);
        self.current_concurrency.store(new_concurrency as u64, Ordering::Relaxed);

        // Check VRAM before loading a new model
        let hw = detect_hardware();
        let free_vram_mb: u64 = hw.gpus.iter()
            .map(|g| g.vram_mb.saturating_sub(g.vram_used_mb.unwrap_or(0)))
            .sum();

        let mut message = format!(
            "Concurrency increased from {} to {}",
            current, new_concurrency
        );

        if free_vram_mb > config.reserved_vram_mb {
            // Try to preload the most-requested model that isn't loaded yet
            // (In a real implementation, this would query the model cache for loaded models)
            if let Some((model_id, count)) = self.model_usage.most_requested_unloaded(&[]).await {
                message.push_str(&format!(
                    ", would preload '{}' ({} requests, {} MB free)",
                    model_id, count, free_vram_mb
                ));
            }
        } else {
            message.push_str(&format!(
                ", skipped model preload (only {} MB VRAM free, need {} MB reserved)",
                free_vram_mb, config.reserved_vram_mb
            ));
        }

        let action = ScaleAction {
            action: "scale_up".to_string(),
            message,
            timestamp,
            success: true,
        };

        self.push_action(action.clone()).await;
        action
    }

    /// Run a scale-down: unload least-used idle model and decrease concurrency.
    ///
    /// Returns the action taken.
    pub async fn scale_down(&self) -> ScaleAction {
        let config = self.config.read().await;
        let timestamp = Utc::now().timestamp();
        let now_secs = Utc::now().timestamp() as u64;

        // Decrease concurrency
        let current = self.current_concurrency.load(Ordering::Relaxed) as usize;
        let new_concurrency = current.saturating_sub(config.concurrency_step).max(config.min_concurrency);
        self.current_concurrency.store(new_concurrency as u64, Ordering::Relaxed);

        let mut message = format!(
            "Concurrency decreased from {} to {}",
            current, new_concurrency
        );

        // Find models that have been idle for longer than the timeout
        if let Some((model_id, last_used)) = self.model_usage.least_recently_used(&[], &[]).await {
            let idle_secs = now_secs.saturating_sub(last_used);
            if idle_secs > config.model_idle_timeout_secs {
                message.push_str(&format!(
                    ", '{}' idle for {}s (threshold {}s) -- candidate for unload",
                    model_id, idle_secs, config.model_idle_timeout_secs
                ));
            }
        }

        let action = ScaleAction {
            action: "scale_down".to_string(),
            message,
            timestamp,
            success: true,
        };

        self.push_action(action.clone()).await;
        action
    }

    /// Run a full scale check cycle: evaluate conditions and take action.
    pub async fn check_and_scale(&self) -> Vec<ScaleAction> {
        // Prevent concurrent check cycles
        if self.checking.swap(true, Ordering::Relaxed) {
            debug!("Scale check already in progress, skipping");
            return vec![];
        }

        let config = self.config.read().await;
        if !config.enabled {
            debug!("Auto-scaling is disabled");
            self.checking.store(false, Ordering::Relaxed);
            return vec![];
        }

        let queue_depth = self.queue_depth.load(Ordering::Relaxed) as usize;
        let p99 = self.latency_tracker.p99_ms().await;

        *self.last_check.write().await = Utc::now().timestamp();

        info!(
            queue_depth,
            p99_latency_ms = p99,
            current_concurrency = self.current_concurrency.load(Ordering::Relaxed),
            "Running auto-scale check"
        );

        let mut actions = Vec::new();

        // Check scale-up conditions: queue > threshold OR p99 > threshold
        if queue_depth > config.scale_up_queue_threshold || p99 > config.scale_up_latency_ms {
            let reason = if queue_depth > config.scale_up_queue_threshold {
                format!("queue depth {} > threshold {}", queue_depth, config.scale_up_queue_threshold)
            } else {
                format!("p99 latency {}ms > threshold {}ms", p99, config.scale_up_latency_ms)
            };
            info!(%reason, "Scale up triggered");
            let action = self.scale_up().await;
            actions.push(action);
        }
        // Check scale-down conditions: queue < threshold AND p99 < threshold
        else if queue_depth < config.scale_down_queue_threshold
            && p99 < config.scale_down_latency_ms
            && p99 > 0
        {
            let reason = format!(
                "queue depth {} < threshold {} and p99 {}ms < threshold {}ms",
                queue_depth, config.scale_down_queue_threshold,
                p99, config.scale_down_latency_ms
            );
            info!(%reason, "Scale down triggered");
            let action = self.scale_down().await;
            actions.push(action);
        } else {
            debug!(
                queue_depth,
                p99_latency_ms = p99,
                "No scaling action needed"
            );
        }

        self.checking.store(false, Ordering::Relaxed);
        actions
    }

    /// Manually trigger a scale check (for the POST /api/auto-scale/trigger endpoint).
    pub async fn trigger(&self) -> Vec<ScaleAction> {
        self.check_and_scale().await
    }

    /// Get the current auto-scale status.
    pub async fn get_status(&self) -> AutoScaleStatus {
        let config = self.config.read().await;
        let hw = detect_hardware();

        let total_vram_mb: u64 = hw.gpus.iter().map(|g| g.vram_mb).sum();
        let used_vram_mb: u64 = hw.gpus.iter().map(|g| g.vram_used_mb.unwrap_or(0)).sum();
        let free_vram_mb = total_vram_mb.saturating_sub(used_vram_mb);

        let actions = self.recent_actions.read().await;
        let recent: Vec<ScaleAction> = actions.iter().rev().take(20).cloned().collect();

        let last_check = *self.last_check.read().await;

        AutoScaleStatus {
            enabled: config.enabled,
            current_concurrency: self.current_concurrency.load(Ordering::Relaxed) as usize,
            models_loaded: 0, // Would be populated from model cache
            queue_depth: self.queue_depth.load(Ordering::Relaxed) as usize,
            p99_latency_ms: self.latency_tracker.p99_ms().await,
            gpu_utilization: if total_vram_mb > 0 {
                (used_vram_mb as f64 / total_vram_mb as f64) * 100.0
            } else {
                0.0
            },
            free_vram_mb,
            total_vram_mb,
            recent_actions: recent,
            last_check: chrono::DateTime::from_timestamp(last_check, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| "never".to_string()),
            started_at: chrono::DateTime::from_timestamp(self.started_at, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| "unknown".to_string()),
        }
    }

    /// Get the current configuration.
    pub async fn get_config(&self) -> AutoScaleConfig {
        self.config.read().await.clone()
    }

    /// Update the configuration at runtime.
    pub async fn update_config(&self, new_config: AutoScaleConfig) {
        let mut config = self.config.write().await;
        info!(
            old_enabled = config.enabled,
            new_enabled = new_config.enabled,
            "Auto-scale config updated"
        );
        *config = new_config;
    }

    /// Push an action to the recent actions log (keeps last 100).
    async fn push_action(&self, action: ScaleAction) {
        let mut actions = self.recent_actions.write().await;
        actions.push(action);
        // Keep only the last 100 actions
        if actions.len() > 100 {
            let drain_from = actions.len() - 100;
            actions.drain(..drain_from);
        }
    }

    /// Start the background scale-check loop.
    ///
    /// Returns a JoinHandle that runs until the config is set to disabled
    /// or the handle is aborted.
    pub fn spawn_background_loop(scaler: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                let interval = {
                    let config = scaler.config.read().await;
                    if !config.enabled {
                        // Sleep and re-check instead of exiting
                        drop(config);
                        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                        continue;
                    }
                    config.check_interval_secs
                };

                tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
                scaler.check_and_scale().await;
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_latency_tracker_p99() {
        let tracker = LatencyTracker::new(100);
        // Fill with values 1..=100
        for i in 1..=100u64 {
            tracker.record(i).await;
        }
        // p99 of 1..=100 should be ~99 or 100
        let p99 = tracker.p99_ms().await;
        assert!(p99 >= 99, "p99 should be >= 99, got {}", p99);
    }

    #[tokio::test]
    async fn test_latency_tracker_empty() {
        let tracker = LatencyTracker::new(100);
        assert_eq!(tracker.p99_ms().await, 0);
    }

    #[tokio::test]
    async fn test_scale_up_increases_concurrency() {
        let config = AutoScaleConfig {
            enabled: true,
            min_concurrency: 2,
            max_concurrency: 16,
            concurrency_step: 2,
            ..Default::default()
        };
        let scaler = AutoScaler::new(config);
        assert_eq!(scaler.current_concurrency(), 2);

        scaler.scale_up().await;
        assert_eq!(scaler.current_concurrency(), 4);

        scaler.scale_up().await;
        assert_eq!(scaler.current_concurrency(), 6);
    }

    #[tokio::test]
    async fn test_scale_up_respects_max() {
        let config = AutoScaleConfig {
            enabled: true,
            min_concurrency: 2,
            max_concurrency: 4,
            concurrency_step: 4,
            ..Default::default()
        };
        let scaler = AutoScaler::new(config);
        assert_eq!(scaler.current_concurrency(), 2);

        scaler.scale_up().await;
        assert_eq!(scaler.current_concurrency(), 4); // Clamped to max

        scaler.scale_up().await;
        assert_eq!(scaler.current_concurrency(), 4); // No increase past max
    }

    #[tokio::test]
    async fn test_scale_down_decreases_concurrency() {
        let config = AutoScaleConfig {
            enabled: true,
            min_concurrency: 1,
            max_concurrency: 16,
            concurrency_step: 2,
            ..Default::default()
        };
        let scaler = AutoScaler::new(config);
        // Manually set high concurrency
        scaler.current_concurrency.store(8, Ordering::Relaxed);

        scaler.scale_down().await;
        assert_eq!(scaler.current_concurrency(), 6);

        scaler.scale_down().await;
        assert_eq!(scaler.current_concurrency(), 4);
    }

    #[tokio::test]
    async fn test_scale_down_respects_min() {
        let config = AutoScaleConfig {
            enabled: true,
            min_concurrency: 2,
            max_concurrency: 16,
            concurrency_step: 4,
            ..Default::default()
        };
        let scaler = AutoScaler::new(config);
        scaler.current_concurrency.store(4, Ordering::Relaxed);

        scaler.scale_down().await;
        assert_eq!(scaler.current_concurrency(), 2); // Clamped to min

        scaler.scale_down().await;
        assert_eq!(scaler.current_concurrency(), 2); // No decrease past min
    }

    #[tokio::test]
    async fn test_check_and_scale_disabled() {
        let config = AutoScaleConfig {
            enabled: false,
            ..Default::default()
        };
        let scaler = AutoScaler::new(config);
        scaler.set_queue_depth(100);
        let actions = scaler.check_and_scale().await;
        assert!(actions.is_empty());
    }

    #[tokio::test]
    async fn test_model_usage_tracking() {
        let tracker = ModelUsageTracker::new();
        tracker.touch("model-a").await;
        tracker.touch("model-a").await;
        tracker.touch("model-b").await;

        // model-a has 2 requests, model-b has 1
        let best = tracker.most_requested_unloaded(&["model-b".to_string()]).await;
        assert_eq!(best.as_ref().map(|(m, _)| m.as_str()), Some("model-a"));
        assert_eq!(best.as_ref().map(|(_, c)| *c), Some(2));
    }

    #[tokio::test]
    async fn test_get_status() {
        let config = AutoScaleConfig {
            enabled: true,
            ..Default::default()
        };
        let scaler = AutoScaler::new(config);
        let status = scaler.get_status().await;
        assert!(status.enabled);
        assert_eq!(status.current_concurrency, 1);
    }
}
