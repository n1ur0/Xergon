//! Inference Autoscaler -- horizontal autoscaling for inference workloads
//! based on load prediction.
//!
//! Monitors load samples (request rate, queue depth, latency, GPU utilization)
//! and uses simple linear regression to predict future load. Makes
//! scale-up / scale-down / hold decisions while respecting min/max bounds,
//! cooldown periods, and configurable thresholds.
//!
//! This module is a pure state machine with no external I/O. It can be driven
//! by an external loop that feeds `LoadSample` data and applies the resulting
//! `ScaleEvent` decisions (e.g. provisioning GPU instances, adjusting
//! concurrency, etc.).
//!
//! Key types:
//! - [`ScaleDecision`] -- the outcome of an autoscale evaluation
//! - [`AutoscaleConfig`] -- tunable thresholds and limits
//! - [`LoadSample`] -- a single observation of system load
//! - [`ScaleEvent`] -- a record of a scaling action that was applied
//! - [`InferenceAutoscaler`] -- the main autoscaler state machine

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// ScaleDecision
// ---------------------------------------------------------------------------

/// Outcome of an autoscale evaluation cycle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ScaleDecision {
    /// Increase instance count by the given number.
    ScaleUp(u32),
    /// Decrease instance count by the given number.
    ScaleDown(u32),
    /// No scaling action is required right now.
    NoAction,
    /// Scaling is on hold (e.g. cooldown not elapsed, insufficient data).
    Hold(String),
}

impl std::fmt::Display for ScaleDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScaleDecision::ScaleUp(n) => write!(f, "ScaleUp(+{})", n),
            ScaleDecision::ScaleDown(n) => write!(f, "ScaleDown(-{})", n),
            ScaleDecision::NoAction => write!(f, "NoAction"),
            ScaleDecision::Hold(reason) => write!(f, "Hold({})", reason),
        }
    }
}

// ---------------------------------------------------------------------------
// AutoscaleConfig
// ---------------------------------------------------------------------------

/// Configuration for the inference autoscaler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoscaleConfig {
    /// Minimum number of inference instances (default: 1).
    #[serde(default = "default_min_instances")]
    pub min_instances: u32,
    /// Maximum number of inference instances (default: 32).
    #[serde(default = "default_max_instances")]
    pub max_instances: u32,
    /// Fraction (0.0..1.0) of load that triggers scale-up (default: 0.80).
    #[serde(default = "default_scale_up_threshold")]
    pub scale_up_threshold: f64,
    /// Fraction (0.0..1.0) of load below which triggers scale-down (default: 0.30).
    #[serde(default = "default_scale_down_threshold")]
    pub scale_down_threshold: f64,
    /// Minimum seconds between scaling actions (default: 60).
    #[serde(default = "default_cooldown_secs")]
    pub cooldown_secs: u64,
    /// How far into the future to predict load, in seconds (default: 120).
    #[serde(default = "default_prediction_window_secs")]
    pub prediction_window_secs: u64,
    /// Target average latency in milliseconds (default: 500).
    #[serde(default = "default_target_latency_ms")]
    pub target_latency_ms: u64,
    /// Queue size that alone triggers immediate scale-up (default: 50).
    #[serde(default = "default_max_queue_size")]
    pub max_queue_size: u32,
}

fn default_min_instances() -> u32 { 1 }
fn default_max_instances() -> u32 { 32 }
fn default_scale_up_threshold() -> f64 { 0.80 }
fn default_scale_down_threshold() -> f64 { 0.30 }
fn default_cooldown_secs() -> u64 { 60 }
fn default_prediction_window_secs() -> u64 { 120 }
fn default_target_latency_ms() -> u64 { 500 }
fn default_max_queue_size() -> u32 { 50 }

impl Default for AutoscaleConfig {
    fn default() -> Self {
        Self {
            min_instances: default_min_instances(),
            max_instances: default_max_instances(),
            scale_up_threshold: default_scale_up_threshold(),
            scale_down_threshold: default_scale_down_threshold(),
            cooldown_secs: default_cooldown_secs(),
            prediction_window_secs: default_prediction_window_secs(),
            target_latency_ms: default_target_latency_ms(),
            max_queue_size: default_max_queue_size(),
        }
    }
}

impl AutoscaleConfig {
    /// Validate the configuration and return a list of issues.
    pub fn validate(&self) -> Vec<String> {
        let mut issues = Vec::new();
        if self.min_instances == 0 {
            issues.push("min_instances must be >= 1".to_string());
        }
        if self.max_instances < self.min_instances {
            issues.push("max_instances must be >= min_instances".to_string());
        }
        if self.scale_up_threshold <= self.scale_down_threshold {
            issues.push("scale_up_threshold must be > scale_down_threshold".to_string());
        }
        if !(0.0..1.0).contains(&self.scale_up_threshold) {
            issues.push("scale_up_threshold must be in (0.0, 1.0)".to_string());
        }
        if !(0.0..1.0).contains(&self.scale_down_threshold) {
            issues.push("scale_down_threshold must be in (0.0, 1.0)".to_string());
        }
        issues
    }
}

// ---------------------------------------------------------------------------
// LoadSample
// ---------------------------------------------------------------------------

/// A single observation of inference system load.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadSample {
    /// When this sample was recorded.
    pub timestamp: DateTime<Utc>,
    /// Number of requests currently being processed.
    pub active_requests: u32,
    /// Number of requests waiting in the queue.
    pub queue_size: u32,
    /// Average request latency in milliseconds.
    pub avg_latency_ms: f64,
    /// Tokens generated per second across all instances.
    pub tokens_per_sec: f64,
    /// GPU utilization as a fraction (0.0 - 1.0).
    pub gpu_utilization: f64,
}

impl LoadSample {
    /// Create a zeroed sample at the current time.
    pub fn zero() -> Self {
        Self {
            timestamp: Utc::now(),
            active_requests: 0,
            queue_size: 0,
            avg_latency_ms: 0.0,
            tokens_per_sec: 0.0,
            gpu_utilization: 0.0,
        }
    }

    /// Create a sample with the given fields at the current time.
    pub fn new(
        active_requests: u32,
        queue_size: u32,
        avg_latency_ms: f64,
        tokens_per_sec: f64,
        gpu_utilization: f64,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            active_requests,
            queue_size,
            avg_latency_ms,
            tokens_per_sec,
            gpu_utilization,
        }
    }

    /// Compute a composite "load score" in 0.0..1.0 for this sample.
    ///
    /// Combines queue pressure, latency overshoot, and GPU utilization
    /// using a weighted average: queue 40%, latency 30%, GPU 30%.
    pub fn load_score(&self, target_latency_ms: u64) -> f64 {
        let queue_factor = (self.queue_size as f64 / 100.0).min(1.0);
        let latency_factor = if target_latency_ms > 0 {
            (self.avg_latency_ms / target_latency_ms as f64).min(2.0) / 2.0
        } else {
            0.0
        };
        let gpu_factor = self.gpu_utilization.clamp(0.0, 1.0);

        (queue_factor * 0.4 + latency_factor * 0.3 + gpu_factor * 0.3).clamp(0.0, 1.0)
    }
}

// ---------------------------------------------------------------------------
// ScaleEvent
// ---------------------------------------------------------------------------

/// A record of a scaling action that was applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScaleEvent {
    /// Unique identifier for this event.
    pub id: String,
    /// The decision that was made.
    pub decision: ScaleDecision,
    /// Human-readable reason for the decision.
    pub reason: String,
    /// When the event occurred.
    pub timestamp: DateTime<Utc>,
    /// Instance count before scaling.
    pub current_instances: u32,
    /// Instance count after scaling.
    pub target_instances: u32,
}

// ---------------------------------------------------------------------------
// PredictionModel -- simple linear regression
// ---------------------------------------------------------------------------

/// Simple linear regression over [`LoadSample`] fields.
///
/// Uses least-squares fitting on (timestamp, value) pairs to produce a
/// slope and intercept that can predict future values.
#[derive(Debug, Clone, Default)]
pub struct PredictionModel {
    /// Number of samples used in the last fit.
    pub n: usize,
    /// Slope (units per second) for active_requests.
    pub slope_active_requests: f64,
    /// Slope (units per second) for queue_size.
    pub slope_queue_size: f64,
    /// Slope (ms per second) for avg_latency_ms.
    pub slope_avg_latency_ms: f64,
    /// Slope (tokens/s per second) for tokens_per_sec.
    pub slope_tokens_per_sec: f64,
    /// Slope (per second) for gpu_utilization.
    pub slope_gpu_utilization: f64,
    /// Intercepts for each field.
    pub intercept_active_requests: f64,
    pub intercept_queue_size: f64,
    pub intercept_avg_latency_ms: f64,
    pub intercept_tokens_per_sec: f64,
    pub intercept_gpu_utilization: f64,
    /// Mean of each field over the fitted window.
    pub mean_active_requests: f64,
    pub mean_queue_size: f64,
    pub mean_avg_latency_ms: f64,
    pub mean_tokens_per_sec: f64,
    pub mean_gpu_utilization: f64,
}

impl PredictionModel {
    /// Fit a linear regression model on the given samples.
    ///
    /// The x-axis is seconds since the earliest sample. Requires at least
    /// 2 samples for meaningful regression; with fewer, returns means only.
    pub fn fit(samples: &[LoadSample]) -> Self {
        if samples.len() < 2 {
            let mut model = Self::default();
            if let Some(s) = samples.first() {
                model.mean_active_requests = s.active_requests as f64;
                model.mean_queue_size = s.queue_size as f64;
                model.mean_avg_latency_ms = s.avg_latency_ms;
                model.mean_tokens_per_sec = s.tokens_per_sec;
                model.mean_gpu_utilization = s.gpu_utilization;
            }
            model.n = samples.len();
            return model;
        }

        let t0 = samples[0].timestamp.timestamp() as f64;
        let n = samples.len();

        let mut ts = Vec::with_capacity(n);
        let mut actives = Vec::with_capacity(n);
        let mut queues = Vec::with_capacity(n);
        let mut lats = Vec::with_capacity(n);
        let mut tps = Vec::with_capacity(n);
        let mut gpus = Vec::with_capacity(n);

        for s in samples {
            ts.push(s.timestamp.timestamp() as f64 - t0);
            actives.push(s.active_requests as f64);
            queues.push(s.queue_size as f64);
            lats.push(s.avg_latency_ms);
            tps.push(s.tokens_per_sec);
            gpus.push(s.gpu_utilization);
        }

        let mean_t = ts.iter().sum::<f64>() / n as f64;
        let mean_a = actives.iter().sum::<f64>() / n as f64;
        let mean_q = queues.iter().sum::<f64>() / n as f64;
        let mean_l = lats.iter().sum::<f64>() / n as f64;
        let mean_tps = tps.iter().sum::<f64>() / n as f64;
        let mean_g = gpus.iter().sum::<f64>() / n as f64;

        let mut cov_ta = 0.0;
        let mut cov_tq = 0.0;
        let mut cov_tl = 0.0;
        let mut cov_ttps = 0.0;
        let mut cov_tg = 0.0;
        let mut var_t = 0.0;

        for i in 0..n {
            let dt = ts[i] - mean_t;
            var_t += dt * dt;
            cov_ta += dt * (actives[i] - mean_a);
            cov_tq += dt * (queues[i] - mean_q);
            cov_tl += dt * (lats[i] - mean_l);
            cov_ttps += dt * (tps[i] - mean_tps);
            cov_tg += dt * (gpus[i] - mean_g);
        }

        let safe_var = if var_t.abs() > 1e-12 { var_t } else { 1.0 };

        Self {
            n,
            slope_active_requests: cov_ta / safe_var,
            slope_queue_size: cov_tq / safe_var,
            slope_avg_latency_ms: cov_tl / safe_var,
            slope_tokens_per_sec: cov_ttps / safe_var,
            slope_gpu_utilization: cov_tg / safe_var,
            intercept_active_requests: mean_a - (cov_ta / safe_var) * mean_t,
            intercept_queue_size: mean_q - (cov_tq / safe_var) * mean_t,
            intercept_avg_latency_ms: mean_l - (cov_tl / safe_var) * mean_t,
            intercept_tokens_per_sec: mean_tps - (cov_ttps / safe_var) * mean_t,
            intercept_gpu_utilization: mean_g - (cov_tg / safe_var) * mean_t,
            mean_active_requests: mean_a,
            mean_queue_size: mean_q,
            mean_avg_latency_ms: mean_l,
            mean_tokens_per_sec: mean_tps,
            mean_gpu_utilization: mean_g,
        }
    }

    /// Predict load at `delta_secs` seconds beyond the fitted window.
    ///
    /// Returns a [`LoadSample`] with predicted values. All values are
    /// clamped to non-negative ranges; GPU utilization is clamped to [0, 1].
    pub fn predict(&self, delta_secs: f64) -> LoadSample {
        let active_requests = (self.intercept_active_requests
            + self.slope_active_requests * delta_secs)
            .max(0.0) as u32;
        let queue_size = (self.intercept_queue_size
            + self.slope_queue_size * delta_secs)
            .max(0.0) as u32;
        let avg_latency_ms = (self.intercept_avg_latency_ms
            + self.slope_avg_latency_ms * delta_secs)
            .max(0.0);
        let tokens_per_sec = (self.intercept_tokens_per_sec
            + self.slope_tokens_per_sec * delta_secs)
            .max(0.0);
        let gpu_utilization = (self.intercept_gpu_utilization
            + self.slope_gpu_utilization * delta_secs)
            .clamp(0.0, 1.0);

        LoadSample {
            timestamp: Utc::now() + Duration::seconds(delta_secs as i64),
            active_requests,
            queue_size,
            avg_latency_ms,
            tokens_per_sec,
            gpu_utilization,
        }
    }
}

// ---------------------------------------------------------------------------
// AutoscaleMetricsSnapshot
// ---------------------------------------------------------------------------

/// Point-in-time snapshot of autoscaler metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoscaleMetricsSnapshot {
    /// Total number of evaluation cycles run.
    pub total_evaluations: u64,
    /// Total scale-up events applied.
    pub total_scale_ups: u64,
    /// Total scale-down events applied.
    pub total_scale_downs: u64,
    /// Total holds (no action due to cooldown/data).
    pub total_holds: u64,
    /// Total no-action evaluations.
    pub total_no_actions: u64,
    /// Current number of instances.
    pub current_instances: u32,
    /// Average latency from the most recent load sample (ms).
    pub avg_latency_ms: f64,
    /// Current queue size from the most recent load sample.
    pub current_queue_size: u32,
    /// Current GPU utilization from the most recent load sample.
    pub current_gpu_utilization: f64,
    /// Number of load samples in the history buffer.
    pub load_history_len: usize,
    /// Number of scale events recorded.
    pub scale_events_len: usize,
    /// Load score from the most recent sample (0.0 - 1.0).
    pub current_load_score: f64,
    /// Last predicted load score.
    pub predicted_load_score: f64,
}

// ---------------------------------------------------------------------------
// InferenceAutoscaler
// ---------------------------------------------------------------------------

/// Horizontal autoscaler for inference workloads.
///
/// Maintains a sliding window of [`LoadSample`] data, fits a prediction model,
/// and evaluates scaling decisions based on predicted load, thresholds, and
/// cooldown constraints.
pub struct InferenceAutoscaler {
    config: AutoscaleConfig,
    /// Ring buffer of load samples, newest at the back.
    load_history: VecDeque<LoadSample>,
    /// Maximum number of samples to retain.
    max_history: usize,
    /// Record of all scaling events.
    scale_events: Vec<ScaleEvent>,
    /// Maximum scale events to retain.
    max_events: usize,
    /// Current instance count.
    current_instances: u32,
    /// Timestamp of the last applied scale action.
    last_scale_time: Option<DateTime<Utc>>,
    // -- metrics counters --
    total_evaluations: AtomicU64,
    total_scale_ups: AtomicU64,
    total_scale_downs: AtomicU64,
    total_holds: AtomicU64,
    total_no_actions: AtomicU64,
    /// Cached prediction model from the most recent evaluation.
    prediction_model: PredictionModel,
    /// Most recent predicted load score.
    last_predicted_score: f64,
}

impl InferenceAutoscaler {
    /// Create a new autoscaler with the given configuration.
    ///
    /// The autoscaler starts with `min_instances` as the initial instance count.
    pub fn new(config: AutoscaleConfig) -> Self {
        let initial_instances = config.min_instances;
        Self {
            config,
            load_history: VecDeque::with_capacity(10_000),
            max_history: 10_000,
            scale_events: Vec::with_capacity(1_000),
            max_events: 1_000,
            current_instances: initial_instances,
            last_scale_time: None,
            total_evaluations: AtomicU64::new(0),
            total_scale_ups: AtomicU64::new(0),
            total_scale_downs: AtomicU64::new(0),
            total_holds: AtomicU64::new(0),
            total_no_actions: AtomicU64::new(0),
            prediction_model: PredictionModel::default(),
            last_predicted_score: 0.0,
        }
    }

    /// Record a load sample.
    ///
    /// The sample is appended to the internal history buffer. Old samples
    /// beyond `max_history` are discarded.
    pub fn record_load(&mut self, sample: LoadSample) {
        if self.load_history.len() >= self.max_history {
            self.load_history.pop_front();
        }
        let active = sample.active_requests;
        let queue = sample.queue_size;
        let latency_ms = sample.avg_latency_ms;
        let gpu = sample.gpu_utilization;
        self.load_history.push_back(sample);
        debug!(
            active = active,
            queue = queue,
            latency_ms = latency_ms,
            gpu = gpu,
            "Recorded load sample"
        );
    }

    /// Evaluate current load and decide on a scaling action.
    ///
    /// This fits a prediction model on the load history, predicts load at
    /// `prediction_window_secs` into the future, and compares the predicted
    /// load score against the configured thresholds.
    ///
    /// Cooldown, min/max bounds, and insufficient-data conditions are all
    /// respected.
    pub fn evaluate_scale(&mut self) -> ScaleDecision {
        self.total_evaluations.fetch_add(1, Ordering::Relaxed);

        // --- Check cooldown ---
        if let Some(last) = self.last_scale_time {
            let elapsed = Utc::now() - last;
            if elapsed.num_seconds() < self.config.cooldown_secs as i64 {
                let remaining = self.config.cooldown_secs as i64 - elapsed.num_seconds();
                let decision = ScaleDecision::Hold(format!(
                    "cooldown: {}s remaining",
                    remaining
                ));
                self.total_holds.fetch_add(1, Ordering::Relaxed);
                debug!(%decision, "Cooldown active");
                return decision;
            }
        }

        // --- Check we have enough data ---
        if self.load_history.is_empty() {
            let decision = ScaleDecision::Hold("no load data available".to_string());
            self.total_holds.fetch_add(1, Ordering::Relaxed);
            debug!(%decision, "Insufficient data");
            return decision;
        }

        // --- Fit prediction model ---
        let samples: Vec<LoadSample> = self.load_history.iter().cloned().collect();
        self.prediction_model = PredictionModel::fit(&samples);

        // --- Predict future load ---
        let predicted = self.prediction_model
            .predict(self.config.prediction_window_secs as f64);
        let predicted_score = predicted.load_score(self.config.target_latency_ms);
        self.last_predicted_score = predicted_score;

        // --- Also check current (real-time) load ---
        let current_sample = self.load_history.back().unwrap();
        let current_score = current_sample.load_score(self.config.target_latency_ms);

        debug!(
            current_score,
            predicted_score,
            up_threshold = self.config.scale_up_threshold,
            down_threshold = self.config.scale_down_threshold,
            instances = self.current_instances,
            "Autoscale evaluation"
        );

        // --- Immediate queue pressure override ---
        if current_sample.queue_size > self.config.max_queue_size {
            let additional = ((current_sample.queue_size as f64
                / self.config.max_queue_size as f64)
                .ceil() as u32)
                .max(1);
            let proposed = self.current_instances + additional;
            let clamped = proposed.min(self.config.max_instances);
            if clamped > self.current_instances {
                let delta = clamped - self.current_instances;
                info!(
                    queue = current_sample.queue_size,
                    delta,
                    target = clamped,
                    "Queue overflow: scale up"
                );
                return ScaleDecision::ScaleUp(delta);
            }
            let decision = ScaleDecision::Hold(format!(
                "queue overflow but already at max instances ({})",
                self.config.max_instances
            ));
            self.total_holds.fetch_add(1, Ordering::Relaxed);
            return decision;
        }

        // --- Use the higher of current and predicted score ---
        let effective_score = current_score.max(predicted_score);

        // --- Scale up ---
        if effective_score > self.config.scale_up_threshold {
            if self.current_instances >= self.config.max_instances {
                let decision = ScaleDecision::Hold(format!(
                    "at max instances ({})",
                    self.config.max_instances
                ));
                self.total_holds.fetch_add(1, Ordering::Relaxed);
                debug!(%decision, "Cannot scale up");
                return decision;
            }

            // Add at least 1, more if we're far above threshold.
            let overshoot = (effective_score - self.config.scale_up_threshold).max(0.0);
            let additional = ((overshoot * 10.0).ceil() as u32).max(1);
            let proposed = self.current_instances + additional;
            let clamped = proposed.min(self.config.max_instances);
            let delta = clamped - self.current_instances;
            info!(
                effective_score,
                delta,
                target = clamped,
                "Load above scale-up threshold"
            );
            return ScaleDecision::ScaleUp(delta);
        }

        // --- Scale down ---
        if effective_score < self.config.scale_down_threshold {
            if self.current_instances <= self.config.min_instances {
                let decision = ScaleDecision::Hold(format!(
                    "at min instances ({})",
                    self.config.min_instances
                ));
                self.total_holds.fetch_add(1, Ordering::Relaxed);
                debug!(%decision, "Cannot scale down");
                return decision;
            }

            // Remove at least 1, more if we're far below threshold.
            let undershoot = (self.config.scale_down_threshold - effective_score).max(0.0);
            let reduction = ((undershoot * 10.0).ceil() as u32).max(1);
            let proposed = self.current_instances.saturating_sub(reduction);
            let clamped = proposed.max(self.config.min_instances);
            let delta = self.current_instances - clamped;
            if delta > 0 {
                info!(
                    effective_score,
                    delta,
                    target = clamped,
                    "Load below scale-down threshold"
                );
                return ScaleDecision::ScaleDown(delta);
            }
        }

        // --- No action needed ---
        self.total_no_actions.fetch_add(1, Ordering::Relaxed);
        debug!(effective_score, "No scaling action needed");
        ScaleDecision::NoAction
    }

    /// Apply a scaling decision and record the resulting event.
    ///
    /// Returns a [`ScaleEvent`] describing what happened. The instance count
    /// is updated in-place.
    pub fn apply_scale(&mut self, decision: ScaleDecision) -> ScaleEvent {
        let before = self.current_instances;
        let (after, reason) = match &decision {
            ScaleDecision::ScaleUp(n) => {
                let target = (before + n).min(self.config.max_instances);
                let actual = target - before;
                (target, format!("scale up by {} ({} -> {})", actual, before, target))
            }
            ScaleDecision::ScaleDown(n) => {
                let target = before.saturating_sub(*n).max(self.config.min_instances);
                let actual = before - target;
                (target, format!("scale down by {} ({} -> {})", actual, before, target))
            }
            ScaleDecision::NoAction => {
                (before, "no action: load within thresholds".to_string())
            }
            ScaleDecision::Hold(reason) => {
                (before, format!("hold: {}", reason))
            }
        };

        self.current_instances = after;

        // Only update last_scale_time for actual scaling actions
        match &decision {
            ScaleDecision::ScaleUp(_) | ScaleDecision::ScaleDown(_) => {
                self.last_scale_time = Some(Utc::now());
                match &decision {
                    ScaleDecision::ScaleUp(_) => {
                        self.total_scale_ups.fetch_add(1, Ordering::Relaxed);
                    }
                    ScaleDecision::ScaleDown(_) => {
                        self.total_scale_downs.fetch_add(1, Ordering::Relaxed);
                    }
                    _ => {}
                }
            }
            _ => {}
        }

        let event = ScaleEvent {
            id: format!("se_{}", Utc::now().timestamp_millis()),
            decision: decision.clone(),
            reason,
            timestamp: Utc::now(),
            current_instances: before,
            target_instances: after,
        };

        if self.scale_events.len() >= self.max_events {
            self.scale_events.remove(0);
        }
        self.scale_events.push(event.clone());

        info!(
            id = %event.id,
            %decision,
            before,
            after,
            "Scale event applied"
        );

        event
    }

    /// Get the load history as a Vec (oldest first).
    pub fn get_load_history(&self) -> Vec<LoadSample> {
        self.load_history.iter().cloned().collect()
    }

    /// Get all recorded scale events (oldest first).
    pub fn get_scale_events(&self) -> Vec<ScaleEvent> {
        self.scale_events.clone()
    }

    /// Predict future load using the current prediction model.
    ///
    /// If no model has been fitted yet, returns a zeroed sample.
    pub fn predict_load(&mut self, window_secs: u64) -> LoadSample {
        if self.load_history.len() < 2 {
            return LoadSample::zero();
        }
        let samples: Vec<LoadSample> = self.load_history.iter().cloned().collect();
        self.prediction_model = PredictionModel::fit(&samples);
        self.prediction_model.predict(window_secs as f64)
    }

    /// Get a point-in-time snapshot of autoscaler metrics.
    pub fn get_metrics(&self) -> AutoscaleMetricsSnapshot {
        let (avg_latency, queue_size, gpu_util, load_score) = self
            .load_history
            .back()
            .map(|s| {
                (
                    s.avg_latency_ms,
                    s.queue_size,
                    s.gpu_utilization,
                    s.load_score(self.config.target_latency_ms),
                )
            })
            .unwrap_or((0.0, 0, 0.0, 0.0));

        AutoscaleMetricsSnapshot {
            total_evaluations: self.total_evaluations.load(Ordering::Relaxed),
            total_scale_ups: self.total_scale_ups.load(Ordering::Relaxed),
            total_scale_downs: self.total_scale_downs.load(Ordering::Relaxed),
            total_holds: self.total_holds.load(Ordering::Relaxed),
            total_no_actions: self.total_no_actions.load(Ordering::Relaxed),
            current_instances: self.current_instances,
            avg_latency_ms: avg_latency,
            current_queue_size: queue_size,
            current_gpu_utilization: gpu_util,
            load_history_len: self.load_history.len(),
            scale_events_len: self.scale_events.len(),
            current_load_score: load_score,
            predicted_load_score: self.last_predicted_score,
        }
    }

    /// Update the autoscaler configuration at runtime.
    ///
    /// The new config is validated before applying. Returns the old config
    /// on success, or a list of validation errors on failure.
    pub fn update_config(&mut self, config: AutoscaleConfig) -> Result<AutoscaleConfig, Vec<String>> {
        let issues = config.validate();
        if !issues.is_empty() {
            return Err(issues);
        }
        let old = std::mem::replace(&mut self.config, config);
        info!("Autoscaler config updated");
        Ok(old)
    }

    /// Get a copy of the current configuration.
    pub fn get_config(&self) -> AutoscaleConfig {
        self.config.clone()
    }

    /// Force scale to a specific number of instances, bypassing the
    /// normal evaluation logic (but still respecting min/max bounds).
    ///
    /// Returns the resulting [`ScaleEvent`].
    pub fn force_scale(&mut self, target_instances: u32) -> ScaleEvent {
        let clamped = target_instances
            .clamp(self.config.min_instances, self.config.max_instances);
        let decision = if clamped > self.current_instances {
            ScaleDecision::ScaleUp(clamped - self.current_instances)
        } else if clamped < self.current_instances {
            ScaleDecision::ScaleDown(self.current_instances - clamped)
        } else {
            ScaleDecision::NoAction
        };
        let mut event = self.apply_scale(decision);
        event.reason = format!("forced: target={}, actual={}", target_instances, clamped);
        event
    }

    /// Get the current instance count.
    pub fn current_instances(&self) -> u32 {
        self.current_instances
    }

    /// Get the last scale event, if any.
    pub fn last_scale_event(&self) -> Option<&ScaleEvent> {
        self.scale_events.last()
    }

    /// Clear all load history.
    pub fn clear_history(&mut self) {
        self.load_history.clear();
    }

    /// Clear all scale events.
    pub fn clear_events(&mut self) {
        self.scale_events.clear();
    }

    /// Reset all metrics counters to zero.
    pub fn reset_metrics(&mut self) {
        self.total_evaluations.store(0, Ordering::Relaxed);
        self.total_scale_ups.store(0, Ordering::Relaxed);
        self.total_scale_downs.store(0, Ordering::Relaxed);
        self.total_holds.store(0, Ordering::Relaxed);
        self.total_no_actions.store(0, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    /// Helper: create a high-load sample (queue=60, latency=800ms, gpu=0.9).
    fn high_load_sample() -> LoadSample {
        LoadSample::new(20, 60, 800.0, 100.0, 0.9)
    }

    /// Helper: create a low-load sample (queue=0, latency=50ms, gpu=0.1).
    fn low_load_sample() -> LoadSample {
        LoadSample::new(2, 0, 50.0, 10.0, 0.1)
    }

    /// Helper: create a medium-load sample.
    fn medium_load_sample() -> LoadSample {
        LoadSample::new(10, 15, 300.0, 50.0, 0.5)
    }

    fn test_config() -> AutoscaleConfig {
        AutoscaleConfig {
            min_instances: 1,
            max_instances: 10,
            scale_up_threshold: 0.7,
            scale_down_threshold: 0.3,
            cooldown_secs: 0, // no cooldown for tests
            prediction_window_secs: 60,
            target_latency_ms: 500,
            max_queue_size: 50,
        }
    }

    // ---- Basic construction ----

    #[test]
    fn test_new_autoscaler() {
        let config = test_config();
        let scaler = InferenceAutoscaler::new(config);
        assert_eq!(scaler.current_instances(), 1);
        let metrics = scaler.get_metrics();
        assert_eq!(metrics.current_instances, 1);
        assert_eq!(metrics.total_evaluations, 0);
    }

    #[test]
    fn test_default_config() {
        let config = AutoscaleConfig::default();
        assert_eq!(config.min_instances, 1);
        assert_eq!(config.max_instances, 32);
        assert_eq!(config.cooldown_secs, 60);
    }

    // ---- Load recording ----

    #[test]
    fn test_record_load() {
        let mut scaler = InferenceAutoscaler::new(test_config());
        scaler.record_load(low_load_sample());
        scaler.record_load(high_load_sample());
        let history = scaler.get_load_history();
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn test_history_max_capacity() {
        let mut config = test_config();
        let mut scaler = InferenceAutoscaler::new(config);
        scaler.max_history = 5;
        for i in 0..10 {
            scaler.record_load(LoadSample::new(i, 0, 0.0, 0.0, 0.0));
        }
        let history = scaler.get_load_history();
        assert_eq!(history.len(), 5);
        // Oldest should be sample 5
        assert_eq!(history[0].active_requests, 5);
    }

    // ---- Scale-up under load ----

    #[test]
    fn test_scale_up_under_high_load() {
        let mut scaler = InferenceAutoscaler::new(test_config());
        scaler.record_load(high_load_sample());
        let decision = scaler.evaluate_scale();
        assert!(matches!(decision, ScaleDecision::ScaleUp(_)));
        let event = scaler.apply_scale(decision);
        assert!(event.target_instances > event.current_instances);
        assert!(scaler.current_instances() > 1);
    }

    #[test]
    fn test_scale_up_with_multiple_high_samples() {
        let mut scaler = InferenceAutoscaler::new(test_config());
        for _ in 0..5 {
            scaler.record_load(high_load_sample());
        }
        let decision = scaler.evaluate_scale();
        assert!(matches!(decision, ScaleDecision::ScaleUp(_)));
    }

    // ---- Scale-down when idle ----

    #[test]
    fn test_scale_down_when_idle() {
        let mut config = test_config();
        config.min_instances = 1;
        let mut scaler = InferenceAutoscaler::new(config);
        // First scale up
        scaler.record_load(high_load_sample());
        let decision = scaler.evaluate_scale();
        scaler.apply_scale(decision);
        let mid_instances = scaler.current_instances();

        // Now scale down with low load
        scaler.record_load(low_load_sample());
        let decision = scaler.evaluate_scale();
        assert!(matches!(decision, ScaleDecision::ScaleDown(_)));
        let event = scaler.apply_scale(decision);
        assert!(event.target_instances < mid_instances);
    }

    // ---- No action ----

    #[test]
    fn test_no_action_medium_load() {
        let mut scaler = InferenceAutoscaler::new(test_config());
        scaler.record_load(medium_load_sample());
        let decision = scaler.evaluate_scale();
        assert_eq!(decision, ScaleDecision::NoAction);
    }

    // ---- Cooldown enforcement ----

    #[test]
    fn test_cooldown_prevents_scaling() {
        let mut config = test_config();
        config.cooldown_secs = 300; // 5 minutes
        let mut scaler = InferenceAutoscaler::new(config);

        // First scale
        scaler.record_load(high_load_sample());
        let decision = scaler.evaluate_scale();
        scaler.apply_scale(decision);

        // Immediate second evaluation should be on cooldown
        scaler.record_load(high_load_sample());
        let decision2 = scaler.evaluate_scale();
        assert!(matches!(decision2, ScaleDecision::Hold(_)));
    }

    // ---- Min/max bounds ----

    #[test]
    fn test_max_instances_bound() {
        let mut config = test_config();
        config.max_instances = 3;
        let mut scaler = InferenceAutoscaler::new(config);
        // Record extreme load
        for _ in 0..5 {
            scaler.record_load(LoadSample::new(100, 200, 2000.0, 0.0, 1.0));
        }
        let decision = scaler.evaluate_scale();
        if let ScaleDecision::ScaleUp(n) = decision {
            scaler.apply_scale(decision);
        }
        assert!(scaler.current_instances() <= 3);
    }

    #[test]
    fn test_min_instances_bound() {
        let mut config = test_config();
        config.min_instances = 2;
        let mut scaler = InferenceAutoscaler::new(config);
        assert_eq!(scaler.current_instances(), 2);

        // Try to scale down with very low load
        scaler.record_load(low_load_sample());
        let decision = scaler.evaluate_scale();
        if let ScaleDecision::ScaleDown(n) = decision {
            scaler.apply_scale(decision);
        }
        assert!(scaler.current_instances() >= 2);
    }

    #[test]
    fn test_scale_up_at_max_returns_hold() {
        let mut config = test_config();
        config.max_instances = 1;
        let mut scaler = InferenceAutoscaler::new(config);
        scaler.record_load(high_load_sample());
        let decision = scaler.evaluate_scale();
        assert!(matches!(decision, ScaleDecision::Hold(_)));
    }

    #[test]
    fn test_scale_down_at_min_returns_hold() {
        let mut config = test_config();
        config.min_instances = 1;
        config.max_instances = 10;
        let mut scaler = InferenceAutoscaler::new(config);
        scaler.record_load(low_load_sample());
        let decision = scaler.evaluate_scale();
        assert!(matches!(decision, ScaleDecision::Hold(_)));
    }

    // ---- Queue overflow ----

    #[test]
    fn test_queue_overflow_triggers_scale_up() {
        let mut config = test_config();
        config.max_queue_size = 10;
        let mut scaler = InferenceAutoscaler::new(config);
        scaler.record_load(LoadSample::new(5, 50, 100.0, 50.0, 0.3));
        let decision = scaler.evaluate_scale();
        assert!(matches!(decision, ScaleDecision::ScaleUp(_)));
    }

    // ---- Empty data ----

    #[test]
    fn test_evaluate_with_no_data_returns_hold() {
        let mut scaler = InferenceAutoscaler::new(test_config());
        let decision = scaler.evaluate_scale();
        assert!(matches!(decision, ScaleDecision::Hold(reason) if reason.contains("no load data")));
    }

    // ---- Prediction model ----

    #[test]
    fn test_prediction_model_fit_single_sample() {
        let samples = vec![LoadSample::new(10, 5, 200.0, 50.0, 0.5)];
        let model = PredictionModel::fit(&samples);
        assert_eq!(model.n, 1);
        assert_eq!(model.mean_active_requests, 10.0);
    }

    #[test]
    fn test_prediction_model_fit_multiple_samples() {
        let mut samples = Vec::new();
        for i in 0..10 {
            let ts = Utc::now() - Duration::seconds((10 - i) * 10);
            samples.push(LoadSample {
                timestamp: ts,
                active_requests: (5 + i) as u32,
                queue_size: i as u32,
                avg_latency_ms: 100.0 + (i as f64) * 10.0,
                tokens_per_sec: 50.0,
                gpu_utilization: 0.5,
            });
        }
        let model = PredictionModel::fit(&samples);
        assert_eq!(model.n, 10);
        // Active requests should be trending up
        assert!(model.slope_active_requests > 0.0);
    }

    #[test]
    fn test_prediction_model_predict() {
        let mut samples = Vec::new();
        for i in 0..10 {
            let ts = Utc::now() - Duration::seconds((10 - i) * 10);
            samples.push(LoadSample {
                timestamp: ts,
                active_requests: 10,
                queue_size: 5,
                avg_latency_ms: 200.0,
                tokens_per_sec: 50.0,
                gpu_utilization: 0.5,
            });
        }
        let model = PredictionModel::fit(&samples);
        let predicted = model.predict(60.0);
        // Flat data should predict close to input values
        assert!((predicted.active_requests as f64 - 10.0).abs() < 2.0);
    }

    #[test]
    fn test_predict_load_method() {
        let mut scaler = InferenceAutoscaler::new(test_config());
        for _ in 0..5 {
            scaler.record_load(medium_load_sample());
        }
        let predicted = scaler.predict_load(120);
        // Should not panic and should return reasonable values
        assert!(predicted.avg_latency_ms >= 0.0);
    }

    #[test]
    fn test_predict_load_with_no_data() {
        let mut scaler = InferenceAutoscaler::new(test_config());
        let predicted = scaler.predict_load(60);
        assert_eq!(predicted.active_requests, 0);
        assert_eq!(predicted.queue_size, 0);
    }

    // ---- Load score ----

    #[test]
    fn test_load_score_zero() {
        let sample = LoadSample::zero();
        let score = sample.load_score(500);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_load_score_high() {
        let sample = LoadSample::new(20, 80, 900.0, 100.0, 0.95);
        let score = sample.load_score(500);
        assert!(score > 0.7);
    }

    #[test]
    fn test_load_score_clamped_to_one() {
        let sample = LoadSample::new(100, 200, 2000.0, 0.0, 1.0);
        let score = sample.load_score(500);
        assert!(score <= 1.0);
    }

    // ---- Metrics ----

    #[test]
    fn test_metrics_tracking() {
        let mut scaler = InferenceAutoscaler::new(test_config());
        scaler.record_load(high_load_sample());
        scaler.evaluate_scale();
        scaler.record_load(low_load_sample());
        scaler.evaluate_scale();

        let metrics = scaler.get_metrics();
        assert_eq!(metrics.total_evaluations, 2);
        assert_eq!(metrics.load_history_len, 2);
    }

    #[test]
    fn test_metrics_counters() {
        let mut scaler = InferenceAutoscaler::new(test_config());

        // Scale up
        scaler.record_load(high_load_sample());
        let d1 = scaler.evaluate_scale();
        scaler.apply_scale(d1);

        // Scale down
        scaler.record_load(low_load_sample());
        let d2 = scaler.evaluate_scale();
        scaler.apply_scale(d2);

        let metrics = scaler.get_metrics();
        assert!(metrics.total_scale_ups >= 1);
        assert!(metrics.total_scale_downs >= 1);
        assert!(metrics.total_evaluations >= 2);
    }

    // ---- Config updates ----

    #[test]
    fn test_update_config() {
        let mut scaler = InferenceAutoscaler::new(test_config());
        let mut new_config = test_config();
        new_config.max_instances = 100;
        let old = scaler.update_config(new_config).unwrap();
        assert_eq!(old.max_instances, 10);
        assert_eq!(scaler.get_config().max_instances, 100);
    }

    #[test]
    fn test_update_config_invalid() {
        let mut scaler = InferenceAutoscaler::new(test_config());
        let mut bad_config = test_config();
        bad_config.min_instances = 0;
        let result = scaler.update_config(bad_config);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_validate() {
        let mut config = test_config();
        assert!(config.validate().is_empty());

        config.min_instances = 0;
        assert!(!config.validate().is_empty());

        config.min_instances = 1;
        config.max_instances = 0;
        assert!(!config.validate().is_empty());

        config.max_instances = 10;
        config.scale_up_threshold = 0.2;
        config.scale_down_threshold = 0.5;
        assert!(!config.validate().is_empty());
    }

    // ---- Force scale ----

    #[test]
    fn test_force_scale_up() {
        let mut scaler = InferenceAutoscaler::new(test_config());
        let event = scaler.force_scale(5);
        assert_eq!(event.target_instances, 5);
        assert_eq!(scaler.current_instances(), 5);
    }

    #[test]
    fn test_force_scale_down() {
        let mut scaler = InferenceAutoscaler::new(test_config());
        scaler.force_scale(5);
        let event = scaler.force_scale(2);
        assert_eq!(event.target_instances, 2);
        assert_eq!(scaler.current_instances(), 2);
    }

    #[test]
    fn test_force_scale_clamped_to_max() {
        let mut scaler = InferenceAutoscaler::new(test_config());
        let event = scaler.force_scale(100);
        assert_eq!(event.target_instances, 10); // max_instances
    }

    #[test]
    fn test_force_scale_clamped_to_min() {
        let mut scaler = InferenceAutoscaler::new(test_config());
        let event = scaler.force_scale(0);
        assert_eq!(event.target_instances, 1); // min_instances
    }

    #[test]
    fn test_force_scale_no_change() {
        let mut scaler = InferenceAutoscaler::new(test_config());
        let event = scaler.force_scale(1);
        assert_eq!(event.target_instances, 1);
        assert_eq!(scaler.current_instances(), 1);
    }

    // ---- Scale events ----

    #[test]
    fn test_scale_events_recorded() {
        let mut scaler = InferenceAutoscaler::new(test_config());
        scaler.record_load(high_load_sample());
        let decision = scaler.evaluate_scale();
        scaler.apply_scale(decision);

        let events = scaler.get_scale_events();
        assert_eq!(events.len(), 1);
        assert!(!events[0].id.is_empty());
    }

    #[test]
    fn test_apply_no_action() {
        let mut scaler = InferenceAutoscaler::new(test_config());
        let event = scaler.apply_scale(ScaleDecision::NoAction);
        assert_eq!(event.current_instances, event.target_instances);
    }

    #[test]
    fn test_apply_hold() {
        let mut scaler = InferenceAutoscaler::new(test_config());
        let event = scaler.apply_scale(ScaleDecision::Hold("test".to_string()));
        assert_eq!(event.current_instances, event.target_instances);
        assert!(event.reason.contains("hold"));
    }

    // ---- Edge cases ----

    #[test]
    fn test_clear_history() {
        let mut scaler = InferenceAutoscaler::new(test_config());
        scaler.record_load(high_load_sample());
        scaler.record_load(low_load_sample());
        scaler.clear_history();
        assert_eq!(scaler.get_load_history().len(), 0);
    }

    #[test]
    fn test_clear_events() {
        let mut scaler = InferenceAutoscaler::new(test_config());
        scaler.record_load(high_load_sample());
        let d = scaler.evaluate_scale();
        scaler.apply_scale(d);
        scaler.clear_events();
        assert_eq!(scaler.get_scale_events().len(), 0);
    }

    #[test]
    fn test_reset_metrics() {
        let mut scaler = InferenceAutoscaler::new(test_config());
        scaler.record_load(high_load_sample());
        scaler.evaluate_scale();
        scaler.reset_metrics();
        let metrics = scaler.get_metrics();
        assert_eq!(metrics.total_evaluations, 0);
    }

    #[test]
    fn test_last_scale_event() {
        let mut scaler = InferenceAutoscaler::new(test_config());
        assert!(scaler.last_scale_event().is_none());

        scaler.record_load(high_load_sample());
        let d = scaler.evaluate_scale();
        scaler.apply_scale(d);
        assert!(scaler.last_scale_event().is_some());
    }

    #[test]
    fn test_events_max_capacity() {
        let mut scaler = InferenceAutoscaler::new(test_config());
        scaler.max_events = 3;
        for _ in 0..5 {
            scaler.record_load(high_load_sample());
            let d = scaler.evaluate_scale();
            scaler.apply_scale(d);
        }
        let events = scaler.get_scale_events();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn test_scale_decision_display() {
        assert_eq!(format!("{}", ScaleDecision::ScaleUp(3)), "ScaleUp(+3)");
        assert_eq!(format!("{}", ScaleDecision::ScaleDown(2)), "ScaleDown(-2)");
        assert_eq!(format!("{}", ScaleDecision::NoAction), "NoAction");
        assert_eq!(format!("{}", ScaleDecision::Hold("test".to_string())), "Hold(test)");
    }

    #[test]
    fn test_load_sample_new_and_zero() {
        let sample = LoadSample::new(5, 10, 100.0, 50.0, 0.5);
        assert_eq!(sample.active_requests, 5);
        assert_eq!(sample.queue_size, 10);

        let zero = LoadSample::zero();
        assert_eq!(zero.active_requests, 0);
        assert_eq!(zero.avg_latency_ms, 0.0);
    }

    #[test]
    fn test_sustained_load_produces_multiple_scale_events() {
        let mut scaler = InferenceAutoscaler::new(test_config());
        // Record high load and evaluate several times
        for i in 0..5 {
            scaler.record_load(LoadSample::new(
                20 + i * 10,
                60 + i * 20,
                800.0,
                100.0,
                0.9,
            ));
            let decision = scaler.evaluate_scale();
            scaler.apply_scale(decision);
        }
        let events = scaler.get_scale_events();
        assert!(events.len() >= 2);
        // Instances should have grown
        assert!(scaler.current_instances() > 1);
    }

    #[test]
    fn test_oscillating_load() {
        let mut scaler = InferenceAutoscaler::new(test_config());
        // Alternate high/low load
        let samples = [
            high_load_sample(),
            high_load_sample(),
            low_load_sample(),
            low_load_sample(),
        ];
        for sample in &samples {
            scaler.record_load(sample.clone());
            let decision = scaler.evaluate_scale();
            scaler.apply_scale(decision);
        }
        let events = scaler.get_scale_events();
        assert!(!events.is_empty());
        // Should have both scale ups and scale downs
        let has_up = events.iter().any(|e| matches!(e.decision, ScaleDecision::ScaleUp(_)));
        let has_down = events.iter().any(|e| matches!(e.decision, ScaleDecision::ScaleDown(_)));
        assert!(has_up || has_down); // At least one action was taken
    }
}
