//! Provider Health Scoring module.
//!
//! Maintains per-provider health scores based on latency, reliability,
//! availability, throughput, error rate, and reputation. Scores are
//! computed over a configurable sliding window with exponential decay.

use dashmap::DashMap;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tracing::{debug, trace};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Weights for the individual health score components.
/// All weights should sum to 1.0.
#[derive(Debug, Clone, Serialize)]
pub struct HealthWeights {
    pub latency: f64,
    pub reliability: f64,
    pub availability: f64,
    pub throughput: f64,
    pub error_rate: f64,
    pub reputation: f64,
}

impl Default for HealthWeights {
    fn default() -> Self {
        Self {
            latency: 0.25,
            reliability: 0.30,
            availability: 0.20,
            throughput: 0.10,
            error_rate: 0.10,
            reputation: 0.05,
        }
    }
}

impl HealthWeights {
    /// Validate that weights sum to approximately 1.0 (within 0.01 tolerance).
    pub fn is_valid(&self) -> bool {
        let sum = self.latency + self.reliability + self.availability
            + self.throughput + self.error_rate + self.reputation;
        (sum - 1.0).abs() < 0.01
    }
}

/// Configuration for the health scoring system.
#[derive(Debug, Clone, Serialize)]
pub struct HealthScoringConfig {
    pub weights: HealthWeights,
    pub window_size_secs: u64,
    pub min_samples: usize,
    pub latency_p50_target_ms: u64,
    pub latency_p99_target_ms: u64,
    pub reliability_target: f64,
    pub decay_factor: f64,
    pub max_latency_samples: usize,
}

impl Default for HealthScoringConfig {
    fn default() -> Self {
        Self {
            weights: HealthWeights::default(),
            window_size_secs: 300,
            min_samples: 10,
            latency_p50_target_ms: 500,
            latency_p99_target_ms: 2000,
            reliability_target: 0.999,
            decay_factor: 0.95,
            max_latency_samples: 100,
        }
    }
}

// ---------------------------------------------------------------------------
// Provider Metrics Snapshot (for testing)
// ---------------------------------------------------------------------------

#[cfg(test)]
pub struct ProviderMetricsSnapshot {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub total_latency_ms: u64,
    pub consecutive_failures: u32,
    pub latency_samples_len: usize,
}

// ---------------------------------------------------------------------------
// Provider Metrics (raw data)
// ---------------------------------------------------------------------------

/// Raw metrics collected per provider, used to compute health scores.
pub struct ProviderMetrics {
    pub total_requests: AtomicU64,
    pub successful_requests: AtomicU64,
    pub failed_requests: AtomicU64,
    pub total_latency_ms: AtomicU64,
    /// Bounded circular buffer of recent latency samples (ms).
    pub latency_samples: std::sync::Mutex<VecDeque<u64>>,
    /// Recent errors: (error_type, timestamp_ms_epoch)
    pub error_samples: std::sync::Mutex<VecDeque<(String, u64)>>,
    pub last_success: std::sync::Mutex<Option<Instant>>,
    pub last_failure: std::sync::Mutex<Option<Instant>>,
    pub consecutive_failures: AtomicU32,
    pub circuit_opens: AtomicU32,
    pub created_at: Instant,
}

impl ProviderMetrics {
    pub fn new() -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            successful_requests: AtomicU64::new(0),
            failed_requests: AtomicU64::new(0),
            total_latency_ms: AtomicU64::new(0),
            latency_samples: std::sync::Mutex::new(VecDeque::new()),
            error_samples: std::sync::Mutex::new(VecDeque::new()),
            last_success: std::sync::Mutex::new(None),
            last_failure: std::sync::Mutex::new(None),
            consecutive_failures: AtomicU32::new(0),
            circuit_opens: AtomicU32::new(0),
            created_at: Instant::now(),
        }
    }
}

impl Default for ProviderMetrics {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Health Score (computed result)
// ---------------------------------------------------------------------------

/// Computed health score for a single provider.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderHealthScore {
    pub provider_pk: String,
    pub overall_score: f64,
    pub latency_score: f64,
    pub reliability_score: f64,
    pub availability_score: f64,
    pub throughput_score: f64,
    pub error_rate_score: f64,
    pub reputation_score: f64,
    pub region_match_score: f64,
    pub last_updated: u64, // ms since epoch
    pub window_size_secs: u64,
    pub sample_count: u64,
}

// ---------------------------------------------------------------------------
// Scorer Stats
// ---------------------------------------------------------------------------

/// Aggregate statistics for the health scorer.
#[derive(Debug, Clone, Serialize)]
pub struct HealthScorerStats {
    pub providers_scored: usize,
    pub providers_insufficient_data: usize,
    pub total_requests_recorded: u64,
    pub avg_overall_score: f64,
}

// ---------------------------------------------------------------------------
// HealthScorer
// ---------------------------------------------------------------------------

/// Central health scoring engine.
///
/// Tracks per-provider metrics, computes multi-dimensional health scores,
/// and provides ranked provider lists for routing decisions.
pub struct HealthScorer {
    scores: DashMap<String, ProviderHealthScore>,
    metrics: DashMap<String, ProviderMetrics>,
    config: HealthScoringConfig,
    /// External reputation scores keyed by provider_pk (0.0-1.0).
    /// Populated from on-chain PoNW scores or other reputation sources.
    reputation_scores: DashMap<String, f64>,
}

impl HealthScorer {
    /// Create a new health scorer with the given configuration.
    pub fn new(config: HealthScoringConfig) -> Self {
        Self {
            scores: DashMap::new(),
            metrics: DashMap::new(),
            config,
            reputation_scores: DashMap::new(),
        }
    }

    /// Create a new health scorer with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(HealthScoringConfig::default())
    }

    /// Record a successful request for a provider.
    pub fn record_success(&self, provider_pk: &str, latency_ms: u64) {
        let entry = self
            .metrics
            .entry(provider_pk.to_string())
            .or_insert_with(ProviderMetrics::new);

        entry.total_requests.fetch_add(1, Ordering::Relaxed);
        entry.successful_requests.fetch_add(1, Ordering::Relaxed);
        entry.total_latency_ms.fetch_add(latency_ms, Ordering::Relaxed);
        entry.consecutive_failures.store(0, Ordering::Relaxed);

        // Record latency sample
        {
            let mut samples = entry.latency_samples.lock().unwrap();
            samples.push_back(latency_ms);
            if samples.len() > self.config.max_latency_samples {
                samples.pop_front();
            }
        }

        // Update last success time
        {
            let mut last = entry.last_success.lock().unwrap();
            *last = Some(Instant::now());
        }

        // Recompute score for this provider
        self.recompute_score(provider_pk);
    }

    /// Record a failed request for a provider.
    pub fn record_failure(&self, provider_pk: &str, error_type: &str) {
        let entry = self
            .metrics
            .entry(provider_pk.to_string())
            .or_insert_with(ProviderMetrics::new);

        entry.total_requests.fetch_add(1, Ordering::Relaxed);
        entry.failed_requests.fetch_add(1, Ordering::Relaxed);
        entry.consecutive_failures.fetch_add(1, Ordering::Relaxed);

        // Record error sample
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        {
            let mut errors = entry.error_samples.lock().unwrap();
            errors.push_back((error_type.to_string(), now_ms));
            if errors.len() > self.config.max_latency_samples {
                errors.pop_front();
            }
        }

        // Update last failure time
        {
            let mut last = entry.last_failure.lock().unwrap();
            *last = Some(Instant::now());
        }

        // Recompute score
        self.recompute_score(provider_pk);
    }

    /// Get the health score for a specific provider.
    pub fn get_score(&self, provider_pk: &str) -> Option<ProviderHealthScore> {
        self.scores.get(provider_pk).map(|r| r.value().clone())
    }

    /// Get all scores sorted by overall score (descending).
    pub fn get_all_scores(&self) -> Vec<ProviderHealthScore> {
        let mut scores: Vec<_> = self.scores.iter().map(|r| r.value().clone()).collect();
        scores.sort_by(|a, b| b.overall_score.partial_cmp(&a.overall_score).unwrap_or(std::cmp::Ordering::Equal));
        scores
    }

    /// Get the top N providers by health score.
    pub fn get_top_providers(&self, n: usize) -> Vec<ProviderHealthScore> {
        let all = self.get_all_scores();
        all.into_iter().take(n).collect()
    }

    /// Get health score for a specific provider with region preference bonus.
    pub fn get_score_with_region(
        &self,
        provider_pk: &str,
        _preferred_region: &str,
    ) -> Option<ProviderHealthScore> {
        // For now, region match is always 1.0 (no region data stored in scorer).
        // The AdaptiveRouter handles region matching via GeoRouter.
        let mut score = self.get_score(provider_pk)?;
        score.region_match_score = 1.0;
        Some(score)
    }

    /// Decay old metrics — removes stale samples and updates scores.
    /// Should be called periodically (e.g., every 60 seconds).
    pub fn decay_metrics(&self) {
        let window_ms = self.config.window_size_secs * 1000;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        for entry in self.metrics.iter() {
            let pk = entry.key().clone();
            let m = entry.value();

            // Prune stale error samples
            {
                let mut errors = m.error_samples.lock().unwrap();
                while let Some(&(_, ts)) = errors.front() {
                    if now_ms.saturating_sub(ts) > window_ms {
                        errors.pop_front();
                    } else {
                        break;
                    }
                }
            }

            // Apply exponential decay to counters
            let factor = self.config.decay_factor;
            let total = m.total_requests.load(Ordering::Relaxed) as f64;
            if total > 0.0 {
                let decayed = (total * factor) as u64;
                m.total_requests.store(decayed.max(1), Ordering::Relaxed);

                let success_ratio = m.successful_requests.load(Ordering::Relaxed) as f64 / total;
                m.successful_requests.store((decayed as f64 * success_ratio) as u64, Ordering::Relaxed);
                m.failed_requests.store(decayed.saturating_sub(m.successful_requests.load(Ordering::Relaxed)), Ordering::Relaxed);

                let latency_ratio = m.total_latency_ms.load(Ordering::Relaxed) as f64 / total;
                m.total_latency_ms.store((decayed as f64 * latency_ratio) as u64, Ordering::Relaxed);
            }

            // Re-decode latency samples by trimming old ones
            {
                let mut samples = m.latency_samples.lock().unwrap();
                // Keep only the most recent window worth (approximate by max_samples)
                let max = self.config.max_latency_samples;
                while samples.len() > max {
                    samples.pop_front();
                }
            }

            self.recompute_score(&pk);
        }

        debug!("Health metrics decayed");
    }

    /// Get aggregate statistics.
    pub fn get_stats(&self) -> HealthScorerStats {
        let scored = self.scores.len();
        let total_providers = self.metrics.len();
        let insufficient = total_providers.saturating_sub(scored);

        let total_requests: u64 = self
            .metrics
            .iter()
            .map(|r| r.value().total_requests.load(Ordering::Relaxed))
            .sum();

        let avg_score = if scored > 0 {
            let sum: f64 = self.scores.iter().map(|r| r.value().overall_score).sum();
            sum / scored as f64
        } else {
            0.0
        };

        HealthScorerStats {
            providers_scored: scored,
            providers_insufficient_data: insufficient,
            total_requests_recorded: total_requests,
            avg_overall_score: avg_score,
        }
    }

    /// Get raw metrics for a provider (for testing/debugging).
    #[cfg(test)]
    pub fn get_metrics_snapshot(&self, provider_pk: &str) -> Option<ProviderMetricsSnapshot> {
        self.metrics.get(provider_pk).map(|r| {
            let m = r.value();
            ProviderMetricsSnapshot {
                total_requests: m.total_requests.load(Ordering::Relaxed),
                successful_requests: m.successful_requests.load(Ordering::Relaxed),
                failed_requests: m.failed_requests.load(Ordering::Relaxed),
                total_latency_ms: m.total_latency_ms.load(Ordering::Relaxed),
                consecutive_failures: m.consecutive_failures.load(Ordering::Relaxed),
                latency_samples_len: m.latency_samples.lock().unwrap().len(),
            }
        })
    }

    /// Register a provider (pre-populate so we track from the start).
    pub fn register_provider(&self, provider_pk: &str) {
        self.metrics
            .entry(provider_pk.to_string())
            .or_insert_with(ProviderMetrics::new);
    }

    /// Update the reputation score for a provider (0.0-1.0 range).
    /// Called from external systems (e.g., chain sync) to feed on-chain reputation.
    pub fn update_reputation(&self, provider_pk: &str, score: f64) {
        self.reputation_scores
            .insert(provider_pk.to_string(), score.clamp(0.0, 1.0));
        // Recompute score so the new reputation is reflected immediately
        self.recompute_score(provider_pk);
    }

    /// Update reputation from on-chain PoNW score (0-1000 integer).
    /// Converts to 0.0-1.0 range and stores it.
    pub fn update_reputation_from_pown(&self, provider_pk: &str, pown_score: i32) {
        let normalized = (pown_score.clamp(0, 1000) as f64) / 1000.0;
        self.update_reputation(provider_pk, normalized);
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    fn recompute_score(&self, provider_pk: &str) {
        let metrics = match self.metrics.get(provider_pk) {
            Some(m) => m,
            None => return,
        };

        let total_requests = metrics.total_requests.load(Ordering::Relaxed);
        let successful = metrics.successful_requests.load(Ordering::Relaxed);
        let failed = metrics.failed_requests.load(Ordering::Relaxed);
        let consecutive = metrics.consecutive_failures.load(Ordering::Relaxed);

        // Need minimum samples before scoring
        if total_requests < self.config.min_samples as u64 {
            trace!(
                provider = provider_pk,
                total_requests,
                min = self.config.min_samples,
                "Skipping score computation — insufficient samples"
            );
            // Remove any existing score if we drop below threshold
            self.scores.remove(provider_pk);
            return;
        }

        let latency_samples: Vec<u64> = {
            let guard = metrics.latency_samples.lock().unwrap();
            guard.iter().copied().collect()
        };

        // --- Latency Score ---
        let latency_score = self.compute_latency_score(&latency_samples);

        // --- Reliability Score ---
        let reliability_score = self.compute_reliability_score(successful, total_requests);

        // --- Availability Score ---
        let availability_score = self.compute_availability_score(
            &metrics.last_success,
            &metrics.last_failure,
            consecutive,
        );

        // --- Throughput Score ---
        let throughput_score = self.compute_throughput_score(total_requests, &metrics.created_at);

        // --- Error Rate Score ---
        let error_rate_score = self.compute_error_rate_score(failed, total_requests);

        // --- Reputation Score ---
        // Look up externally-set reputation (from on-chain PoNW or other sources).
        // Defaults to 1.0 (neutral) if no reputation data has been set.
        let reputation_score = self
            .reputation_scores
            .get(provider_pk)
            .map(|r| *r.value())
            .unwrap_or(1.0);

        // --- Combine ---
        let w = &self.config.weights;
        let overall_score = (w.latency * latency_score
            + w.reliability * reliability_score
            + w.availability * availability_score
            + w.throughput * throughput_score
            + w.error_rate * error_rate_score
            + w.reputation * reputation_score)
            .clamp(0.0, 1.0);

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let score = ProviderHealthScore {
            provider_pk: provider_pk.to_string(),
            overall_score,
            latency_score,
            reliability_score,
            availability_score,
            throughput_score,
            error_rate_score,
            reputation_score,
            region_match_score: 0.0,
            last_updated: now_ms,
            window_size_secs: self.config.window_size_secs,
            sample_count: total_requests,
        };

        self.scores.insert(provider_pk.to_string(), score);
    }

    /// Compute latency score using sigmoid-like function.
    /// Maps p50 latency to 0.0-1.0 based on target.
    fn compute_latency_score(&self, samples: &[u64]) -> f64 {
        if samples.is_empty() {
            return 0.5; // neutral
        }

        let p50 = percentile(samples, 0.50);
        let p99 = percentile(samples, 0.99);

        // Sigmoid: 1 / (1 + e^((latency - target) / steepness))
        let p50_score = 1.0 / (1.0 + ((p50 as f64 - self.config.latency_p50_target_ms as f64) / 200.0).exp());
        let p99_score = 1.0 / (1.0 + ((p99 as f64 - self.config.latency_p99_target_ms as f64) / 500.0).exp());

        // Weighted combination (p50 is more important for routing)
        (0.7 * p50_score + 0.3 * p99_score).clamp(0.0, 1.0)
    }

    /// Compute reliability score based on success rate vs target.
    fn compute_reliability_score(&self, successful: u64, total: u64) -> f64 {
        if total == 0 {
            return 0.5;
        }
        let rate = successful as f64 / total as f64;
        // Scale relative to target: meeting target = 1.0, below = proportional
        (rate / self.config.reliability_target).clamp(0.0, 1.0)
    }

    /// Compute availability score based on recent success/failure pattern.
    fn compute_availability_score(
        &self,
        last_success: &std::sync::Mutex<Option<Instant>>,
        last_failure: &std::sync::Mutex<Option<Instant>>,
        consecutive_failures: u32,
    ) -> f64 {
        let last_success = last_success.lock().unwrap();
        let last_failure = last_failure.lock().unwrap();

        let now = Instant::now();

        // Base score from recency of last success
        let success_recency = match *last_success {
            Some(t) => {
                let elapsed_secs = now.duration_since(t).as_secs_f64();
                // Exponential decay over window
                (-elapsed_secs / self.config.window_size_secs as f64).exp()
            }
            None => 0.0,
        };

        // Penalty for consecutive failures
        let failure_penalty = if consecutive_failures > 0 {
            (0.8_f64).powi(consecutive_failures as i32)
        } else {
            1.0
        };

        // Bonus if last failure was long ago
        let recovery_bonus = match *last_failure {
            Some(t) => {
                let elapsed_secs = now.duration_since(t).as_secs_f64();
                if elapsed_secs > self.config.window_size_secs as f64 {
                    1.0 // fully recovered
                } else {
                    0.5 + 0.5 * (elapsed_secs / self.config.window_size_secs as f64)
                }
            }
            None => 1.0, // never failed
        };

        (success_recency * failure_penalty * recovery_bonus).clamp(0.0, 1.0)
    }

    /// Compute throughput score based on requests per minute.
    fn compute_throughput_score(&self, total_requests: u64, created_at: &Instant) -> f64 {
        let uptime_secs = created_at.elapsed().as_secs().max(1);
        let rpm = (total_requests as f64 / uptime_secs as f64) * 60.0;

        // Higher throughput = better (logarithmic scale, capped at 100 rpm)
        if rpm < 1.0 {
            0.1 // minimal
        } else {
            (1.0 - (-rpm / 100.0).exp()).clamp(0.0, 1.0)
        }
    }

    /// Compute error rate score (inverse of error rate).
    fn compute_error_rate_score(&self, failed: u64, total: u64) -> f64 {
        if total == 0 {
            return 1.0; // no errors if no requests
        }
        let error_rate = failed as f64 / total as f64;
        (1.0 - error_rate).clamp(0.0, 1.0)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Compute the p-th percentile from a sorted slice.
fn percentile(sorted_or_unsorted: &[u64], p: f64) -> u64 {
    if sorted_or_unsorted.is_empty() {
        return 0;
    }
    if sorted_or_unsorted.len() == 1 {
        return sorted_or_unsorted[0];
    }

    let mut data = sorted_or_unsorted.to_vec();
    data.sort();

    let idx = ((p * (data.len() - 1) as f64).round() as usize).min(data.len() - 1);
    data[idx]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_scorer(min_samples: usize) -> HealthScorer {
        let config = HealthScoringConfig {
            min_samples,
            ..Default::default()
        };
        HealthScorer::new(config)
    }

    #[test]
    fn test_health_score_calculation_latency() {
        let scorer = make_scorer(5);
        let pk = "provider-latency";

        // Record 10 successful requests with low latency
        for i in 0..10 {
            scorer.record_success(pk, 50); // 50ms — fast
        }

        let score = scorer.get_score(pk).expect("should have score");
        assert!(score.latency_score > 0.8, "latency_score should be high for 50ms: {}", score.latency_score);
        assert!(score.overall_score > 0.5, "overall should be decent: {}", score.overall_score);
    }

    #[test]
    fn test_health_score_with_weights() {
        let scorer = HealthScorer::new(HealthScoringConfig {
            weights: HealthWeights {
                latency: 0.0,       // disable latency
                reliability: 1.0,   // only reliability matters
                availability: 0.0,
                throughput: 0.0,
                error_rate: 0.0,
                reputation: 0.0,
            },
            min_samples: 3,
            ..Default::default()
        });

        let pk = "provider-weights";
        for _ in 0..5 {
            scorer.record_success(pk, 5000); // slow but reliable
        }

        let score = scorer.get_score(pk).unwrap();
        assert!(score.reliability_score > 0.9);
        // Overall should equal reliability since it's the only weight
        assert!((score.overall_score - score.reliability_score).abs() < 0.01,
            "overall ({}) should equal reliability ({})", score.overall_score, score.reliability_score);
    }

    #[test]
    fn test_health_score_decay() {
        let scorer = make_scorer(5);
        let pk = "provider-decay";

        for _ in 0..10 {
            scorer.record_success(pk, 100);
        }

        let score_before = scorer.get_score(pk).unwrap();
        let total_before = scorer.get_stats().total_requests_recorded;

        // Decay metrics
        scorer.decay_metrics();

        let score_after = scorer.get_score(pk).unwrap();
        let total_after = scorer.get_stats().total_requests_recorded;

        // After decay, total requests should decrease
        assert!(total_after <= total_before, "total should decrease after decay");
        // Score should still exist
        assert!(score_after.overall_score >= 0.0);
    }

    #[test]
    fn test_record_success_and_failure() {
        let scorer = make_scorer(5);
        let pk = "provider-success-failure";

        scorer.record_success(pk, 100);
        scorer.record_success(pk, 200);
        scorer.record_failure(pk, "timeout");
        scorer.record_success(pk, 150);
        scorer.record_success(pk, 120);
        scorer.record_success(pk, 180);

        let score = scorer.get_score(pk).unwrap();
        assert_eq!(score.sample_count, 6);
        assert!(score.overall_score > 0.0);

        let stats = scorer.get_stats();
        assert_eq!(stats.providers_scored, 1);
        assert_eq!(stats.total_requests_recorded, 6);
    }

    #[test]
    fn test_min_samples_threshold() {
        let scorer = make_scorer(10);
        let pk = "provider-min-samples";

        // Record only 5 successes (below threshold of 10)
        for _ in 0..5 {
            scorer.record_success(pk, 100);
        }

        // Should not have a score yet
        assert!(scorer.get_score(pk).is_none(), "should not score below min_samples");

        // Record 5 more
        for _ in 0..5 {
            scorer.record_success(pk, 100);
        }

        // Now should have a score
        assert!(scorer.get_score(pk).is_some());
    }

    #[test]
    fn test_get_all_scores_sorted() {
        let scorer = make_scorer(5);

        // Provider A: fast
        for _ in 0..10 {
            scorer.record_success("provider-a", 50);
        }

        // Provider B: slow with failures
        for _ in 0..7 {
            scorer.record_success("provider-b", 2000);
        }
        for _ in 0..3 {
            scorer.record_failure("provider-b", "timeout");
        }

        let scores = scorer.get_all_scores();
        assert_eq!(scores.len(), 2);
        // Provider A should be first (better score)
        assert_eq!(scores[0].provider_pk, "provider-a");
    }

    #[test]
    fn test_get_top_providers() {
        let scorer = make_scorer(5);

        for _ in 0..10 {
            scorer.record_success("p1", 50);
            scorer.record_success("p2", 100);
            scorer.record_success("p3", 150);
        }

        let top = scorer.get_top_providers(2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].provider_pk, "p1"); // fastest
    }

    #[test]
    fn test_weights_default_valid() {
        let w = HealthWeights::default();
        assert!(w.is_valid(), "default weights should sum to ~1.0");
    }

    #[test]
    fn test_register_provider() {
        let scorer = make_scorer(5);
        scorer.register_provider("new-provider");
        // Should have metrics but no score yet
        let metrics = scorer.get_metrics_snapshot("new-provider");
        assert!(metrics.is_some());
        assert!(scorer.get_score("new-provider").is_none());
    }

    #[test]
    fn test_consecutive_failures_degrade_availability() {
        let scorer = make_scorer(5);
        let pk = "provider-consecutive";

        for _ in 0..5 {
            scorer.record_success(pk, 100);
        }
        let score_ok = scorer.get_score(pk).unwrap().availability_score;

        // Reset and add failures
        scorer.record_failure(pk, "timeout");
        scorer.record_failure(pk, "timeout");
        scorer.record_failure(pk, "timeout");
        scorer.record_failure(pk, "timeout");
        scorer.record_failure(pk, "timeout");

        let score_bad = scorer.get_score(pk).unwrap().availability_score;
        assert!(score_bad < score_ok, "consecutive failures should lower availability: {} < {}", score_bad, score_ok);
    }
}
