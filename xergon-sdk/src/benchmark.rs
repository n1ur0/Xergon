use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::RwLock;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for a single benchmark run.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BenchmarkConfig {
    /// Target inference endpoint URL.
    pub target_url: String,
    /// Model identifier to benchmark.
    pub model: String,
    /// Number of inference requests to send.
    pub num_requests: u32,
    /// Number of concurrent requests.
    pub concurrency: u32,
    /// Per-request timeout in milliseconds.
    pub timeout_ms: u64,
    /// Number of warm-up requests (discarded from results).
    pub warmup_requests: u32,
    /// Simulated prompt token count.
    pub prompt_tokens: u32,
    /// Simulated max output token count.
    pub max_tokens: u32,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            target_url: String::from("http://localhost:8080/v1/chat/completions"),
            model: String::from("llama-3-8b"),
            num_requests: 10,
            concurrency: 1,
            timeout_ms: 30_000,
            warmup_requests: 2,
            prompt_tokens: 100,
            max_tokens: 256,
        }
    }
}

// ---------------------------------------------------------------------------
// Latency statistics
// ---------------------------------------------------------------------------

/// Percentile latency statistics computed from a benchmark run.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct LatencyStats {
    pub min_ms: f64,
    pub max_ms: f64,
    pub mean_ms: f64,
    pub median_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub std_dev_ms: f64,
}

// ---------------------------------------------------------------------------
// Throughput statistics
// ---------------------------------------------------------------------------

/// Throughput measurements from a benchmark run.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ThroughputStats {
    pub requests_per_sec: f64,
    pub tokens_per_sec: f64,
    pub total_tokens: u64,
    pub total_requests: u32,
}

// ---------------------------------------------------------------------------
// Benchmark result
// ---------------------------------------------------------------------------

/// Complete result of a single benchmark execution.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BenchmarkResult {
    pub config: BenchmarkConfig,
    pub latency: LatencyStats,
    pub throughput: ThroughputStats,
    pub errors: u32,
    pub error_rate: f64,
    pub duration_ms: u64,
    pub timestamp: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Benchmark suite
// ---------------------------------------------------------------------------

/// Aggregated results from multiple benchmark runs.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct BenchmarkSuite {
    pub results: Vec<BenchmarkResult>,
    pub configs: Vec<BenchmarkConfig>,
}

// ---------------------------------------------------------------------------
// Simple LCG PRNG (deterministic, no external dependency)
// ---------------------------------------------------------------------------

struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        // Ensure non-zero seed.
        Self {
            state: if seed == 0 { 1 } else { seed },
        }
    }

    /// Returns the next pseudo-random u64 in [0, u64::MAX].
    fn next_u64(&mut self) -> u64 {
        // Numerical Recipes LCG parameters.
        self.state = self.state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        self.state
    }

    /// Returns a pseudo-random f64 in [0, 1).
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Box-Muller transform: returns two normally-distributed f64 values
    /// with the given mean and standard deviation.
    fn next_normal(&mut self, mean: f64, std_dev: f64) -> f64 {
        let u1 = self.next_f64().max(1e-30);
        let u2 = self.next_f64();
        let z0 = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
        mean + z0 * std_dev
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Compute latency statistics from a mutable vector of latency values (ms).
/// The vector is sorted in-place.
pub fn compute_latency_stats(latencies: &mut Vec<f64>) -> LatencyStats {
    if latencies.is_empty() {
        return LatencyStats::default();
    }

    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let n = latencies.len();
    let min_ms = latencies[0];
    let max_ms = latencies[n - 1];
    let sum: f64 = latencies.iter().sum();
    let mean_ms = sum / n as f64;

    let median_ms = if n % 2 == 0 {
        (latencies[n / 2 - 1] + latencies[n / 2]) / 2.0
    } else {
        latencies[n / 2]
    };

    let p95_ms = percentile(latencies, 0.95);
    let p99_ms = percentile(latencies, 0.99);

    let variance = if n > 1 {
        let mean_sq: f64 = latencies.iter().map(|v| (v - mean_ms).powi(2)).sum();
        mean_sq / (n - 1) as f64
    } else {
        0.0
    };
    let std_dev_ms = variance.sqrt();

    LatencyStats {
        min_ms,
        max_ms,
        mean_ms,
        median_ms,
        p95_ms,
        p99_ms,
        std_dev_ms,
    }
}

/// Linear-interpolation percentile (nearest-rank with interpolation).
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let idx = p * (sorted.len() - 1) as f64;
    let lo = idx.floor() as usize;
    let hi = idx.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        let frac = idx - lo as f64;
        sorted[lo] * (1.0 - frac) + sorted[hi] * frac
    }
}

/// Compute throughput statistics.
pub fn compute_throughput(total_tokens: u64, duration_secs: f64, num_requests: u32) -> ThroughputStats {
    if duration_secs <= 0.0 {
        return ThroughputStats {
            requests_per_sec: 0.0,
            tokens_per_sec: 0.0,
            total_tokens,
            total_requests: num_requests,
        };
    }
    ThroughputStats {
        requests_per_sec: num_requests as f64 / duration_secs,
        tokens_per_sec: total_tokens as f64 / duration_secs,
        total_tokens,
        total_requests: num_requests,
    }
}

// ---------------------------------------------------------------------------
// BenchmarkRunner
// ---------------------------------------------------------------------------

/// Drives benchmark execution against an inference endpoint.
pub struct BenchmarkRunner {
    config: BenchmarkConfig,
}

impl BenchmarkRunner {
    /// Create a new runner with the given configuration.
    pub fn new(config: BenchmarkConfig) -> Self {
        Self { config }
    }

    /// Run a single benchmark using synthetic (mock) latency data.
    ///
    /// Latencies are generated from a normal distribution centred at 50 ms
    /// with a standard deviation of 10 ms, clamped to [1, 500] ms.
    pub fn run(&self) -> BenchmarkResult {
        let seed = Utc::now().timestamp_millis() as u64;
        let mut rng = Lcg::new(seed);

        let total_requests = self.config.num_requests;
        let warmup = self.config.warmup_requests;
        let measured_requests = total_requests.saturating_sub(warmup);

        // Generate all latencies.
        let mut latencies: Vec<f64> = Vec::with_capacity(measured_requests as usize);
        let mut errors: u32 = 0;

        for i in 0..total_requests {
            let latency = rng.next_normal(50.0, 10.0).clamp(1.0, 500.0);
            if i >= warmup {
                latencies.push(latency);
            }
            // Simulate occasional errors (~3%).
            if rng.next_f64() < 0.03 {
                errors += 1;
            }
        }

        let mut latency_stats = compute_latency_stats(&mut latencies);

        // Edge: if all were warmup, stats stay default.
        if measured_requests == 0 {
            latency_stats = LatencyStats::default();
        }

        let total_tokens = (self.config.prompt_tokens + self.config.max_tokens) as u64
            * measured_requests as u64;
        let duration_ms = latencies.iter().sum::<f64>() as u64;
        let duration_secs = duration_ms as f64 / 1000.0;

        let throughput = compute_throughput(total_tokens, duration_secs, measured_requests);

        let error_rate = if measured_requests > 0 {
            errors as f64 / measured_requests as f64
        } else {
            0.0
        };

        BenchmarkResult {
            config: self.config.clone(),
            latency: latency_stats,
            throughput,
            errors,
            error_rate,
            duration_ms,
            timestamp: Utc::now(),
        }
    }

    /// Run a suite of benchmarks across multiple configurations.
    pub fn run_suite(configs: Vec<BenchmarkConfig>) -> BenchmarkSuite {
        let results: Vec<BenchmarkResult> = configs
            .iter()
            .map(|c| {
                let runner = Self::new(c.clone());
                runner.run()
            })
            .collect();
        BenchmarkSuite {
            results,
            configs,
        }
    }
}

// ---------------------------------------------------------------------------
// Thread-safe benchmark runner (RwLock-based)
// ---------------------------------------------------------------------------

/// A shared, thread-safe handle to a benchmark runner.
///
/// Internally uses `std::sync::RwLock` so callers can read the config
/// concurrently while a benchmark is in progress.
pub struct SharedBenchmarkRunner {
    inner: RwLock<BenchmarkRunner>,
}

impl SharedBenchmarkRunner {
    /// Create a new shared runner.
    pub fn new(config: BenchmarkConfig) -> Self {
        Self {
            inner: RwLock::new(BenchmarkRunner::new(config)),
        }
    }

    /// Run the benchmark, taking a read lock on the runner.
    pub fn run(&self) -> BenchmarkResult {
        let guard = self.inner.read().expect("benchmark lock poisoned");
        guard.run()
    }

    /// Update the configuration, taking a write lock.
    pub fn update_config(&self, config: BenchmarkConfig) {
        let mut guard = self.inner.write().expect("benchmark lock poisoned");
        *guard = BenchmarkRunner::new(config);
    }

    /// Get a snapshot of the current configuration.
    pub fn config(&self) -> BenchmarkConfig {
        let guard = self.inner.read().expect("benchmark lock poisoned");
        guard.config.clone()
    }
}

// ---------------------------------------------------------------------------
// Queue-based concurrent request simulation (uses RwLock)
// ---------------------------------------------------------------------------

/// Simulates a concurrent request queue backed by a `RwLock<VecDeque>`.
pub struct ConcurrentSimulator {
    queue: RwLock<VecDeque<f64>>,
}

impl ConcurrentSimulator {
    pub fn new() -> Self {
        Self {
            queue: RwLock::new(VecDeque::new()),
        }
    }

    /// Push a latency value onto the queue (write lock).
    pub fn push(&self, latency_ms: f64) {
        let mut q = self.queue.write().expect("simulator lock poisoned");
        q.push_back(latency_ms);
    }

    /// Drain all latency values from the queue (write lock).
    pub fn drain(&self) -> Vec<f64> {
        let mut q = self.queue.write().expect("simulator lock poisoned");
        q.drain(..).collect()
    }

    /// Read the current queue length (read lock).
    pub fn len(&self) -> usize {
        let q = self.queue.read().expect("simulator lock poisoned");
        q.len()
    }
}

impl Default for ConcurrentSimulator {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // BenchmarkConfig defaults
    // -----------------------------------------------------------------------

    #[test]
    fn config_default_values() {
        let cfg = BenchmarkConfig::default();
        assert_eq!(cfg.num_requests, 10);
        assert_eq!(cfg.concurrency, 1);
        assert_eq!(cfg.timeout_ms, 30_000);
        assert_eq!(cfg.warmup_requests, 2);
        assert_eq!(cfg.prompt_tokens, 100);
        assert_eq!(cfg.max_tokens, 256);
        assert!(!cfg.target_url.is_empty());
        assert!(!cfg.model.is_empty());
    }

    #[test]
    fn config_clone_and_debug() {
        let cfg = BenchmarkConfig::default();
        let _ = cfg.clone();
        let debug_str = format!("{:?}", cfg);
        assert!(debug_str.contains("llama-3-8b"));
    }

    #[test]
    fn config_serialize_deserialize_roundtrip() {
        let cfg = BenchmarkConfig::default();
        let json = serde_json::to_string(&cfg).expect("serialize");
        let restored: BenchmarkConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.target_url, cfg.target_url);
        assert_eq!(restored.model, cfg.model);
        assert_eq!(restored.num_requests, cfg.num_requests);
        assert_eq!(restored.concurrency, cfg.concurrency);
        assert_eq!(restored.timeout_ms, cfg.timeout_ms);
        assert_eq!(restored.warmup_requests, cfg.warmup_requests);
        assert_eq!(restored.prompt_tokens, cfg.prompt_tokens);
        assert_eq!(restored.max_tokens, cfg.max_tokens);
    }

    #[test]
    fn config_custom_values() {
        let cfg = BenchmarkConfig {
            target_url: "https://example.com/v1".into(),
            model: "gpt-4".into(),
            num_requests: 100,
            concurrency: 5,
            timeout_ms: 60_000,
            warmup_requests: 10,
            prompt_tokens: 512,
            max_tokens: 1024,
        };
        assert_eq!(cfg.num_requests, 100);
        assert_eq!(cfg.concurrency, 5);
        assert_eq!(cfg.max_tokens, 1024);
    }

    // -----------------------------------------------------------------------
    // LatencyStats
    // -----------------------------------------------------------------------

    #[test]
    fn latency_stats_default_is_zero() {
        let stats = LatencyStats::default();
        assert_eq!(stats.min_ms, 0.0);
        assert_eq!(stats.max_ms, 0.0);
        assert_eq!(stats.mean_ms, 0.0);
    }

    #[test]
    fn latency_stats_serialize_deserialize() {
        let stats = LatencyStats {
            min_ms: 10.0,
            max_ms: 200.0,
            mean_ms: 50.0,
            median_ms: 45.0,
            p95_ms: 150.0,
            p99_ms: 190.0,
            std_dev_ms: 30.0,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let restored: LatencyStats = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.p95_ms, 150.0);
    }

    // -----------------------------------------------------------------------
    // ThroughputStats
    // -----------------------------------------------------------------------

    #[test]
    fn throughput_stats_default() {
        let stats = ThroughputStats::default();
        assert_eq!(stats.total_tokens, 0);
        assert_eq!(stats.total_requests, 0);
        assert_eq!(stats.requests_per_sec, 0.0);
    }

    #[test]
    fn throughput_stats_serialize_deserialize() {
        let stats = ThroughputStats {
            requests_per_sec: 10.5,
            tokens_per_sec: 2500.0,
            total_tokens: 500_000,
            total_requests: 100,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let restored: ThroughputStats = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.total_tokens, 500_000);
        assert_eq!(restored.requests_per_sec, 10.5);
    }

    // -----------------------------------------------------------------------
    // compute_latency_stats
    // -----------------------------------------------------------------------

    #[test]
    fn compute_latency_empty() {
        let mut empty: Vec<f64> = vec![];
        let stats = compute_latency_stats(&mut empty);
        assert_eq!(stats.min_ms, 0.0);
        assert_eq!(stats.max_ms, 0.0);
        assert_eq!(stats.mean_ms, 0.0);
        assert_eq!(stats.std_dev_ms, 0.0);
    }

    #[test]
    fn compute_latency_single_value() {
        let mut vals = vec![42.0];
        let stats = compute_latency_stats(&mut vals);
        assert_eq!(stats.min_ms, 42.0);
        assert_eq!(stats.max_ms, 42.0);
        assert_eq!(stats.mean_ms, 42.0);
        assert_eq!(stats.median_ms, 42.0);
        assert_eq!(stats.p95_ms, 42.0);
        assert_eq!(stats.p99_ms, 42.0);
        assert_eq!(stats.std_dev_ms, 0.0); // sample std_dev with n=1 is 0
    }

    #[test]
    fn compute_latency_two_values() {
        let mut vals = vec![10.0, 20.0];
        let stats = compute_latency_stats(&mut vals);
        assert_eq!(stats.min_ms, 10.0);
        assert_eq!(stats.max_ms, 20.0);
        assert_eq!(stats.mean_ms, 15.0);
        assert_eq!(stats.median_ms, 15.0);
    }

    #[test]
    fn compute_latency_known_dataset() {
        // 11 values => median is the 6th value after sort.
        let mut vals: Vec<f64> = (1..=11).map(|i| i as f64 * 10.0).collect();
        let stats = compute_latency_stats(&mut vals);
        assert_eq!(stats.min_ms, 10.0);
        assert_eq!(stats.max_ms, 110.0);
        assert_eq!(stats.mean_ms, 60.0);
        assert_eq!(stats.median_ms, 60.0);
        assert_eq!(stats.p95_ms, 105.0); // index 9.5 => 100 + 0.5*10 = 105
        assert_eq!(stats.p99_ms, 109.0); // index 9.9 => 100 + 0.9*10 = 109
    }

    #[test]
    fn compute_latency_sorts_in_place() {
        let mut vals = vec![50.0, 10.0, 30.0];
        compute_latency_stats(&mut vals);
        assert_eq!(vals, vec![10.0, 30.0, 50.0]);
    }

    #[test]
    fn compute_latency_std_dev() {
        // Values: 40, 50, 60 => mean=50, sample std_dev=10
        let mut vals = vec![40.0, 50.0, 60.0];
        let stats = compute_latency_stats(&mut vals);
        assert!((stats.std_dev_ms - 10.0).abs() < 1e-6);
    }

    // -----------------------------------------------------------------------
    // compute_throughput
    // -----------------------------------------------------------------------

    #[test]
    fn compute_throughput_basic() {
        let stats = compute_throughput(1000, 2.0, 10);
        assert!((stats.requests_per_sec - 5.0).abs() < 1e-9);
        assert!((stats.tokens_per_sec - 500.0).abs() < 1e-9);
        assert_eq!(stats.total_tokens, 1000);
        assert_eq!(stats.total_requests, 10);
    }

    #[test]
    fn compute_throughput_zero_duration() {
        let stats = compute_throughput(500, 0.0, 5);
        assert_eq!(stats.requests_per_sec, 0.0);
        assert_eq!(stats.tokens_per_sec, 0.0);
        assert_eq!(stats.total_tokens, 500);
    }

    #[test]
    fn compute_throughput_negative_duration() {
        let stats = compute_throughput(100, -1.0, 3);
        assert_eq!(stats.requests_per_sec, 0.0);
        assert_eq!(stats.tokens_per_sec, 0.0);
    }

    #[test]
    fn compute_throughput_zero_requests() {
        let stats = compute_throughput(0, 1.0, 0);
        assert!((stats.requests_per_sec - 0.0).abs() < 1e-9);
        assert_eq!(stats.total_tokens, 0);
    }

    // -----------------------------------------------------------------------
    // LCG PRNG
    // -----------------------------------------------------------------------

    #[test]
    fn lcg_deterministic() {
        let mut a = Lcg::new(42);
        let mut b = Lcg::new(42);
        assert_eq!(a.next_u64(), b.next_u64());
        assert_eq!(a.next_u64(), b.next_u64());
        assert_eq!(a.next_f64(), b.next_f64());
    }

    #[test]
    fn lcg_different_seeds_differ() {
        let mut a = Lcg::new(1);
        let mut b = Lcg::new(2);
        // It's astronomically unlikely they produce the same sequence.
        let a_vals: Vec<u64> = (0..10).map(|_| a.next_u64()).collect();
        let b_vals: Vec<u64> = (0..10).map(|_| b.next_u64()).collect();
        assert_ne!(a_vals, b_vals);
    }

    #[test]
    fn lcg_zero_seed_handled() {
        let mut rng = Lcg::new(0);
        let v = rng.next_u64();
        assert_ne!(v, 0); // should produce valid output even with seed=0
    }

    #[test]
    fn lcg_normal_distribution_produces_variety() {
        let mut rng = Lcg::new(12345);
        let vals: Vec<f64> = (0..1000).map(|_| rng.next_normal(50.0, 10.0)).collect();
        let mean = vals.iter().sum::<f64>() / vals.len() as f64;
        // Should be roughly centred at 50.
        assert!(mean > 40.0 && mean < 60.0);
    }

    // -----------------------------------------------------------------------
    // BenchmarkRunner
    // -----------------------------------------------------------------------

    #[test]
    fn runner_creation() {
        let cfg = BenchmarkConfig::default();
        let _runner = BenchmarkRunner::new(cfg);
    }

    #[test]
    fn runner_run_produces_result() {
        let cfg = BenchmarkConfig {
            num_requests: 20,
            warmup_requests: 5,
            ..Default::default()
        };
        let runner = BenchmarkRunner::new(cfg.clone());
        let result = runner.run();

        assert_eq!(result.config.target_url, cfg.target_url);
        assert_eq!(result.config.model, cfg.model);
        assert!(result.latency.min_ms > 0.0);
        assert!(result.latency.max_ms >= result.latency.min_ms);
        assert!(result.latency.mean_ms > 0.0);
        assert!(result.duration_ms > 0);
        assert!(result.timestamp.timestamp() > 0);
    }

    #[test]
    fn runner_measured_equals_num_minus_warmup() {
        let cfg = BenchmarkConfig {
            num_requests: 10,
            warmup_requests: 2,
            ..Default::default()
        };
        let runner = BenchmarkRunner::new(cfg);
        let result = runner.run();
        assert_eq!(result.throughput.total_requests, 8);
    }

    #[test]
    fn runner_all_warmup_produces_empty_stats() {
        let cfg = BenchmarkConfig {
            num_requests: 5,
            warmup_requests: 5,
            ..Default::default()
        };
        let runner = BenchmarkRunner::new(cfg);
        let result = runner.run();
        assert_eq!(result.latency, LatencyStats::default());
        assert_eq!(result.throughput.total_requests, 0);
        assert_eq!(result.error_rate, 0.0);
    }

    #[test]
    fn runner_result_serializable() {
        let runner = BenchmarkRunner::new(BenchmarkConfig::default());
        let result = runner.run();
        let json = serde_json::to_string(&result).expect("serialize result");
        assert!(json.contains("latency"));
        assert!(json.contains("throughput"));
        assert!(json.contains("timestamp"));
    }

    // -----------------------------------------------------------------------
    // BenchmarkSuite
    // -----------------------------------------------------------------------

    #[test]
    fn suite_default() {
        let suite = BenchmarkSuite::default();
        assert!(suite.results.is_empty());
        assert!(suite.configs.is_empty());
    }

    #[test]
    fn suite_run_multiple_configs() {
        let configs = vec![
            BenchmarkConfig {
                num_requests: 5,
                warmup_requests: 0,
                model: "model-a".into(),
                ..Default::default()
            },
            BenchmarkConfig {
                num_requests: 5,
                warmup_requests: 0,
                model: "model-b".into(),
                ..Default::default()
            },
        ];
        let suite = BenchmarkRunner::run_suite(configs.clone());
        assert_eq!(suite.results.len(), 2);
        assert_eq!(suite.configs.len(), 2);
        assert_eq!(suite.results[0].config.model, "model-a");
        assert_eq!(suite.results[1].config.model, "model-b");
    }

    #[test]
    fn suite_serialize_deserialize() {
        let suite = BenchmarkRunner::run_suite(vec![BenchmarkConfig {
            num_requests: 3,
            warmup_requests: 0,
            ..Default::default()
        }]);
        let json = serde_json::to_string(&suite).unwrap();
        let restored: BenchmarkSuite = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.results.len(), 1);
        assert_eq!(restored.configs.len(), 1);
    }

    #[test]
    fn suite_empty_configs() {
        let suite = BenchmarkRunner::run_suite(vec![]);
        assert!(suite.results.is_empty());
        assert!(suite.configs.is_empty());
    }

    // -----------------------------------------------------------------------
    // SharedBenchmarkRunner (RwLock)
    // -----------------------------------------------------------------------

    #[test]
    fn shared_runner_creation_and_run() {
        let shared = SharedBenchmarkRunner::new(BenchmarkConfig::default());
        let result = shared.run();
        assert!(result.latency.mean_ms > 0.0);
    }

    #[test]
    fn shared_runner_update_config() {
        let shared = SharedBenchmarkRunner::new(BenchmarkConfig::default());
        let new_cfg = BenchmarkConfig {
            model: "updated-model".into(),
            num_requests: 5,
            warmup_requests: 0,
            ..Default::default()
        };
        shared.update_config(new_cfg.clone());
        let snap = shared.config();
        assert_eq!(snap.model, "updated-model");
        assert_eq!(snap.num_requests, 5);
    }

    #[test]
    fn shared_runner_config_snapshot() {
        let cfg = BenchmarkConfig {
            target_url: "https://custom.host".into(),
            ..Default::default()
        };
        let shared = SharedBenchmarkRunner::new(cfg.clone());
        let snap = shared.config();
        assert_eq!(snap.target_url, "https://custom.host");
    }

    // -----------------------------------------------------------------------
    // ConcurrentSimulator (RwLock)
    // -----------------------------------------------------------------------

    #[test]
    fn simulator_push_and_drain() {
        let sim = ConcurrentSimulator::new();
        assert_eq!(sim.len(), 0);
        sim.push(10.0);
        sim.push(20.0);
        sim.push(30.0);
        assert_eq!(sim.len(), 3);
        let drained = sim.drain();
        assert_eq!(drained, vec![10.0, 20.0, 30.0]);
        assert_eq!(sim.len(), 0);
    }

    #[test]
    fn simulator_default() {
        let sim = ConcurrentSimulator::default();
        assert_eq!(sim.len(), 0);
    }

    #[test]
    fn simulator_drain_empty() {
        let sim = ConcurrentSimulator::new();
        let drained = sim.drain();
        assert!(drained.is_empty());
    }

    // -----------------------------------------------------------------------
    // BenchmarkResult
    // -----------------------------------------------------------------------

    #[test]
    fn benchmark_result_clone_and_debug() {
        let runner = BenchmarkRunner::new(BenchmarkConfig::default());
        let result = runner.run();
        let _cloned = result.clone();
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("BenchmarkResult"));
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn large_concurrency_config() {
        let cfg = BenchmarkConfig {
            concurrency: 10_000,
            num_requests: 100_000,
            ..Default::default()
        };
        assert_eq!(cfg.concurrency, 10_000);
        assert_eq!(cfg.num_requests, 100_000);
    }

    #[test]
    fn zero_timeout_config() {
        let cfg = BenchmarkConfig {
            timeout_ms: 0,
            ..Default::default()
        };
        assert_eq!(cfg.timeout_ms, 0);
    }

    #[test]
    fn runner_latency_within_bounds() {
        let cfg = BenchmarkConfig {
            num_requests: 50,
            warmup_requests: 0,
            ..Default::default()
        };
        let runner = BenchmarkRunner::new(cfg);
        let result = runner.run();
        // Mock clamps to [1, 500].
        assert!(result.latency.min_ms >= 1.0);
        assert!(result.latency.max_ms <= 500.0);
    }

    #[test]
    fn percentile_function_directly() {
        assert_eq!(percentile(&[], 0.5), 0.0);
        assert_eq!(percentile(&[42.0], 0.5), 42.0);
        assert_eq!(percentile(&[10.0, 20.0, 30.0], 0.5), 20.0);
        // p95 of [10,20,30] => idx = 0.95*2 = 1.9 => 20*0.1 + 30*0.9 = 29.0
        assert!((percentile(&[10.0, 20.0, 30.0], 0.95) - 29.0).abs() < 1e-9);
    }
}
