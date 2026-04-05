//! Prometheus-compatible metrics collector for the Xergon relay.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

/// Fixed exponential bucket boundaries in milliseconds.
const LATENCY_BUCKETS_MS: &[u64] = &[
    1, 2, 5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10000,
];

/// A thread-safe latency histogram with fixed exponential buckets.
///
/// Stores bucket counts in a `Mutex<Vec<u64>>` -- contention is acceptable
/// because recording only happens on the metrics path, not the hot request path.
pub struct LatencyHistogram {
    /// One counter per bucket; `counts[i]` holds observations <= `LATENCY_BUCKETS_MS[i]`.
    counts: Mutex<Vec<u64>>,
    /// Running sum of all recorded values (ms).
    sum: AtomicU64,
    /// Total number of observations.
    total: AtomicU64,
}

impl std::fmt::Debug for LatencyHistogram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LatencyHistogram")
            .field("total", &self.total.load(Ordering::Relaxed))
            .field("sum", &self.sum.load(Ordering::Relaxed))
            .field("buckets", &LATENCY_BUCKETS_MS)
            .finish()
    }
}

impl LatencyHistogram {
    /// Create a new empty histogram.
    pub fn new() -> Self {
        Self {
            counts: Mutex::new(vec![0u64; LATENCY_BUCKETS_MS.len()]),
            sum: AtomicU64::new(0),
            total: AtomicU64::new(0),
        }
    }

    /// Record a latency observation in milliseconds.
    pub fn record(&self, ms: u64) {
        self.sum.fetch_add(ms, Ordering::Relaxed);
        self.total.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut counts) = self.counts.lock() {
            for (i, &boundary) in LATENCY_BUCKETS_MS.iter().enumerate() {
                if ms <= boundary {
                    counts[i] += 1;
                }
            }
        }
    }

    /// Return the total number of observations.
    pub fn count(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }

    /// Return the running sum of all recorded values (ms).
    pub fn sum(&self) -> u64 {
        self.sum.load(Ordering::Relaxed)
    }

    /// Estimate the latency value at the given percentile `p` (0.0 ..= 1.0).
    ///
    /// Uses cumulative bucket counts.  Returns 0 when no observations exist.
    pub fn percentile(&self, p: f64) -> u64 {
        assert!((0.0..=1.0).contains(&p), "percentile must be in [0, 1]");
        let n = self.count();
        if n == 0 {
            return 0;
        }
        let target = (p * n as f64).ceil() as u64; // rank (1-based)
        if let Ok(counts) = self.counts.lock() {
            let mut cumulative: u64 = 0;
            for (i, &boundary) in LATENCY_BUCKETS_MS.iter().enumerate() {
                cumulative += counts[i];
                if cumulative >= target {
                    // Linear interpolation inside the bucket when possible.
                    if i > 0 {
                        let prev_cum = cumulative - counts[i];
                        let prev_boundary = LATENCY_BUCKETS_MS[i - 1];
                        let bucket_count = counts[i];
                        if bucket_count > 0 && cumulative >= target && prev_cum < target {
                            let frac = (target - prev_cum) as f64 / bucket_count as f64;
                            return prev_boundary
                                + ((boundary - prev_boundary) as f64 * frac) as u64;
                        }
                    }
                    return boundary;
                }
            }
            // All buckets overflowed -- return the largest bucket boundary.
            *LATENCY_BUCKETS_MS.last().unwrap()
        } else {
            0
        }
    }

    /// Render the histogram in Prometheus text exposition format.
    ///
    /// Produces `histogram`-type lines with `_bucket`, `_sum`, `_count` for the
    /// given metric name.  Values are converted from ms to seconds.
    pub fn render(&self, metric_name: &str) -> String {
        let mut out = String::with_capacity(512);
        out.push_str(&format!(
            "# HELP {name} Request duration in seconds\n",
            name = metric_name
        ));
        out.push_str(&format!(
            "# TYPE {name} histogram\n",
            name = metric_name
        ));

        if let Ok(counts) = self.counts.lock() {
            let mut cumulative: u64 = 0;
            for (i, &boundary_ms) in LATENCY_BUCKETS_MS.iter().enumerate() {
                cumulative += counts[i];
                let le = boundary_ms as f64 / 1000.0;
                out.push_str(&format!(
                    "{name}_bucket{{le=\"{le}\"}} {cum}\n",
                    name = metric_name,
                    le = le,
                    cum = cumulative,
                ));
            }
            // +Inf bucket
            let total = self.total.load(Ordering::Relaxed);
            out.push_str(&format!(
                "{name}_bucket{{le=\"+Inf\"}} {total}\n",
                name = metric_name,
                total = total,
            ));

            let sum_sec = self.sum.load(Ordering::Relaxed) as f64 / 1000.0;
            out.push_str(&format!(
                "{name}_sum {sum}\n",
                name = metric_name,
                sum = sum_sec,
            ));
            out.push_str(&format!(
                "{name}_count {count}\n",
                name = metric_name,
                count = total,
            ));
        }

        out
    }
}

impl Default for LatencyHistogram {
    fn default() -> Self {
        Self::new()
    }
}

/// Central metrics collector shared across the relay.
#[derive(Debug)]
pub struct RelayMetrics {
    // Counters
    requests_chat: AtomicU64,
    requests_models: AtomicU64,
    requests_gpu: AtomicU64,
    errors_total: AtomicU64,
    errors_4xx: AtomicU64,
    errors_5xx: AtomicU64,
    rate_limited_total: AtomicU64,

    // Gauges
    providers_active: AtomicU64,
    providers_total: AtomicU64,
    active_connections: AtomicU64,

    // Average latency (stored as integer ms)
    avg_latency_ms: AtomicU64,

    // Latency histogram
    request_latency: LatencyHistogram,

    /// Process start time for uptime computation.
    start_time: Instant,
}

#[allow(dead_code)]
impl RelayMetrics {
    pub fn new() -> Self {
        Self {
            requests_chat: AtomicU64::new(0),
            requests_models: AtomicU64::new(0),
            requests_gpu: AtomicU64::new(0),
            errors_total: AtomicU64::new(0),
            errors_4xx: AtomicU64::new(0),
            errors_5xx: AtomicU64::new(0),
            rate_limited_total: AtomicU64::new(0),
            providers_active: AtomicU64::new(0),
            providers_total: AtomicU64::new(0),
            active_connections: AtomicU64::new(0),
            avg_latency_ms: AtomicU64::new(0),
            request_latency: LatencyHistogram::new(),
            start_time: Instant::now(),
        }
    }

    // ---- Increment helpers ----

    pub fn inc_chat_requests(&self) {
        self.requests_chat.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_models_requests(&self) {
        self.requests_models.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_gpu_requests(&self) {
        self.requests_gpu.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_errors(&self, code: &str) {
        self.errors_total.fetch_add(1, Ordering::Relaxed);
        if code.starts_with('4') {
            self.errors_4xx.fetch_add(1, Ordering::Relaxed);
        } else if code.starts_with('5') {
            self.errors_5xx.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn inc_rate_limited(&self) {
        self.rate_limited_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn rate_limited_count(&self) -> u64 {
        self.rate_limited_total.load(Ordering::Relaxed)
    }

    // ---- Gauge setters ----

    pub fn set_providers_active(&self, n: u64) {
        self.providers_active.store(n, Ordering::Relaxed);
    }

    pub fn set_providers_total(&self, n: u64) {
        self.providers_total.store(n, Ordering::Relaxed);
    }

    pub fn set_avg_latency_ms(&self, ms: u64) {
        self.avg_latency_ms.store(ms, Ordering::Relaxed);
    }

    pub fn set_active_connections(&self, n: u64) {
        self.active_connections.store(n, Ordering::Relaxed);
    }

    // ---- Latency histogram ----

    /// Record a request latency observation in milliseconds.
    pub fn observe_request_latency_ms(&self, ms: u64) {
        self.request_latency.record(ms);
    }

    /// Read-only access to the underlying latency histogram (for tests / introspection).
    pub fn request_latency(&self) -> &LatencyHistogram {
        &self.request_latency
    }

    // ---- Read helpers ----

    pub fn uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Render all metrics in Prometheus text exposition format.
    pub fn render_prometheus(&self) -> String {
        let mut out = String::with_capacity(2048);

        out.push_str("# HELP xergon_relay_requests_total Total requests by endpoint\n");
        out.push_str("# TYPE xergon_relay_requests_total counter\n");
        out.push_str(&format!(
            "xergon_relay_requests_total{{endpoint=\"chat\"}} {}\n",
            self.requests_chat.load(Ordering::Relaxed)
        ));
        out.push_str(&format!(
            "xergon_relay_requests_total{{endpoint=\"models\"}} {}\n",
            self.requests_models.load(Ordering::Relaxed)
        ));

        out.push_str("# HELP xergon_relay_providers_active Number of active (healthy) providers\n");
        out.push_str("# TYPE xergon_relay_providers_active gauge\n");
        out.push_str(&format!(
            "xergon_relay_providers_active {}\n",
            self.providers_active.load(Ordering::Relaxed)
        ));

        out.push_str("# HELP xergon_relay_providers_total Total known providers\n");
        out.push_str("# TYPE xergon_relay_providers_total gauge\n");
        out.push_str(&format!(
            "xergon_relay_providers_total {}\n",
            self.providers_total.load(Ordering::Relaxed)
        ));

        out.push_str("# HELP xergon_relay_avg_latency_ms Average request latency in ms\n");
        out.push_str("# TYPE xergon_relay_avg_latency_ms gauge\n");
        out.push_str(&format!(
            "xergon_relay_avg_latency_ms {}\n",
            self.avg_latency_ms.load(Ordering::Relaxed)
        ));

        out.push_str("# HELP xergon_relay_errors_total Total relay errors\n");
        out.push_str("# TYPE xergon_relay_errors_total counter\n");
        out.push_str(&format!(
            "xergon_relay_errors_total{{code=\"503\"}} {}\n",
            self.errors_total.load(Ordering::Relaxed)
        ));

        out.push_str("# HELP xergon_relay_uptime_seconds Relay uptime in seconds\n");
        out.push_str("# TYPE xergon_relay_uptime_seconds gauge\n");
        out.push_str(&format!(
            "xergon_relay_uptime_seconds {}\n",
            self.uptime_seconds()
        ));

        // New metrics
        out.push_str("# HELP xergon_relay_requests_gpu_total Total GPU Bazar requests\n");
        out.push_str("# TYPE xergon_relay_requests_gpu_total counter\n");
        out.push_str(&format!(
            "xergon_relay_requests_gpu_total {}\n",
            self.requests_gpu.load(Ordering::Relaxed)
        ));

        out.push_str("# HELP xergon_relay_errors_4xx_total Total 4xx client errors\n");
        out.push_str("# TYPE xergon_relay_errors_4xx_total counter\n");
        out.push_str(&format!(
            "xergon_relay_errors_4xx_total {}\n",
            self.errors_4xx.load(Ordering::Relaxed)
        ));

        out.push_str("# HELP xergon_relay_errors_5xx_total Total 5xx server errors\n");
        out.push_str("# TYPE xergon_relay_errors_5xx_total counter\n");
        out.push_str(&format!(
            "xergon_relay_errors_5xx_total {}\n",
            self.errors_5xx.load(Ordering::Relaxed)
        ));

        out.push_str("# HELP xergon_relay_active_connections Current HTTP connections\n");
        out.push_str("# TYPE xergon_relay_active_connections gauge\n");
        out.push_str(&format!(
            "xergon_relay_active_connections {}\n",
            self.active_connections.load(Ordering::Relaxed)
        ));

        out.push_str("# HELP xergon_relay_rate_limited_total Total requests rejected by rate limiter\n");
        out.push_str("# TYPE xergon_relay_rate_limited_total counter\n");
        out.push_str(&format!(
            "xergon_relay_rate_limited_total {}\n",
            self.rate_limited_total.load(Ordering::Relaxed)
        ));

        // ---- Latency histogram (Prometheus convention: seconds) ----
        out.push_str(&self.request_latency.render("xergon_relay_request_duration_seconds"));

        // ---- Summary-style percentile gauges ----
        out.push_str("# HELP xergon_relay_request_duration_p50 p50 request latency in ms\n");
        out.push_str("# TYPE xergon_relay_request_duration_p50 gauge\n");
        out.push_str(&format!(
            "xergon_relay_request_duration_p50 {}\n",
            self.request_latency.percentile(0.50)
        ));

        out.push_str("# HELP xergon_relay_request_duration_p95 p95 request latency in ms\n");
        out.push_str("# TYPE xergon_relay_request_duration_p95 gauge\n");
        out.push_str(&format!(
            "xergon_relay_request_duration_p95 {}\n",
            self.request_latency.percentile(0.95)
        ));

        out.push_str("# HELP xergon_relay_request_duration_p99 p99 request latency in ms\n");
        out.push_str("# TYPE xergon_relay_request_duration_p99 gauge\n");
        out.push_str(&format!(
            "xergon_relay_request_duration_p99 {}\n",
            self.request_latency.percentile(0.99)
        ));

        out
    }
}

impl Default for RelayMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relay_metrics_initial() {
        let m = RelayMetrics::new();
        assert_eq!(m.requests_chat.load(Ordering::Relaxed), 0);
        assert_eq!(m.requests_models.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_relay_metrics_increments() {
        let m = RelayMetrics::new();
        m.inc_chat_requests();
        m.inc_chat_requests();
        m.inc_chat_requests();
        m.inc_models_requests();
        m.inc_gpu_requests();
        m.inc_errors("400");
        m.inc_errors("500");
        m.inc_rate_limited();
        assert_eq!(m.requests_chat.load(Ordering::Relaxed), 3);
        assert_eq!(m.requests_models.load(Ordering::Relaxed), 1);
        assert_eq!(m.requests_gpu.load(Ordering::Relaxed), 1);
        assert_eq!(m.errors_total.load(Ordering::Relaxed), 2);
        assert_eq!(m.errors_4xx.load(Ordering::Relaxed), 1);
        assert_eq!(m.errors_5xx.load(Ordering::Relaxed), 1);
        assert_eq!(m.rate_limited_total.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_relay_metrics_gauges() {
        let m = RelayMetrics::new();
        m.set_providers_active(15);
        m.set_providers_total(23);
        m.set_avg_latency_ms(45);
        m.set_active_connections(8);
        assert_eq!(m.providers_active.load(Ordering::Relaxed), 15);
        assert_eq!(m.providers_total.load(Ordering::Relaxed), 23);
        assert_eq!(m.avg_latency_ms.load(Ordering::Relaxed), 45);
        assert_eq!(m.active_connections.load(Ordering::Relaxed), 8);
    }

    #[test]
    fn test_relay_render_prometheus() {
        let m = RelayMetrics::new();
        m.inc_chat_requests();
        m.inc_gpu_requests();
        m.set_providers_active(15);
        m.set_providers_total(23);
        m.set_avg_latency_ms(45);
        m.set_active_connections(8);
        m.inc_errors("503");
        m.inc_rate_limited();

        let output = m.render_prometheus();
        assert!(output.contains("xergon_relay_requests_total{endpoint=\"chat\"} 1"));
        assert!(output.contains("xergon_relay_providers_active 15"));
        assert!(output.contains("xergon_relay_providers_total 23"));
        assert!(output.contains("xergon_relay_avg_latency_ms 45"));
        assert!(output.contains("xergon_relay_uptime_seconds"));
        assert!(output.contains("# TYPE xergon_relay_requests_total counter"));
        assert!(output.contains("xergon_relay_requests_gpu_total 1"));
        assert!(output.contains("xergon_relay_errors_5xx_total 1"));
        assert!(output.contains("xergon_relay_active_connections 8"));
        assert!(output.contains("xergon_relay_rate_limited_total 1"));
    }

    // ---- Latency histogram tests ----

    #[test]
    fn test_histogram_records_correctly() {
        let h = LatencyHistogram::new();
        assert_eq!(h.count(), 0);
        assert_eq!(h.sum(), 0);

        // Record a few values
        h.record(1);  // falls in bucket 0 (<=1)
        h.record(3);  // falls in bucket 1 (<=2) -- wait, 3 > 2, so bucket 2 (<=5)
        h.record(10); // falls in bucket 3 (<=10)
        h.record(50); // falls in bucket 4 (<=20) -- wait, 50 > 20, bucket 5 (<=50)
        h.record(100);
        h.record(5000);

        assert_eq!(h.count(), 6);
        assert_eq!(h.sum(), 1 + 3 + 10 + 50 + 100 + 5000);

        // Verify bucket contents
        let counts = h.counts.lock().unwrap();
        // 1ms: 1 (value 1)
        assert_eq!(counts[0], 1);
        // 2ms: 1 (value 1 -- cumulative; 3 does not fit)
        assert_eq!(counts[1], 1);
        // 5ms: 2 (values 1, 3)
        assert_eq!(counts[2], 2);
        // 10ms: 3 (values 1, 3, 10)
        assert_eq!(counts[3], 3);
        // 20ms: 3 (no new values)
        assert_eq!(counts[4], 3);
        // 50ms: 4 (values 1, 3, 10, 50)
        assert_eq!(counts[5], 4);
        // 100ms: 5 (values 1, 3, 10, 50, 100)
        assert_eq!(counts[6], 5);
        // 200ms: 5
        assert_eq!(counts[7], 5);
        // 500ms: 5
        assert_eq!(counts[8], 5);
        // 1000ms: 5
        assert_eq!(counts[9], 5);
        // 2000ms: 5
        assert_eq!(counts[10], 5);
        // 5000ms: 6 (all values)
        assert_eq!(counts[11], 6);
        // 10000ms: 6
        assert_eq!(counts[12], 6);
    }

    #[test]
    fn test_histogram_percentile() {
        let h = LatencyHistogram::new();

        // Record 100 values: all exactly 10ms
        for _ in 0..100 {
            h.record(10);
        }
        // p50, p95, p99 should all be ~10ms
        let p50 = h.percentile(0.50);
        let p95 = h.percentile(0.95);
        let p99 = h.percentile(0.99);
        assert_eq!(p50, 10);
        assert_eq!(p95, 10);
        assert_eq!(p99, 10);

        // Record 100 more values at 100ms
        for _ in 0..100 {
            h.record(100);
        }
        // Now 200 total. p50 should be around 10ms (first 100), p95 around 100ms.
        let p50 = h.percentile(0.50);
        assert!(p50 <= 10, "p50 should be <= 10ms, got {}", p50);

        let p95 = h.percentile(0.95);
        // rank = ceil(0.95 * 200) = 190. First 100 at 10ms, next 100 at 100ms.
        // Bucket for 100ms has cumulative 200, so p95 should return 100.
        assert!(p95 >= 10, "p95 should be >= 10ms, got {}", p95);

        // Empty histogram returns 0
        let empty = LatencyHistogram::new();
        assert_eq!(empty.percentile(0.50), 0);
        assert_eq!(empty.percentile(0.99), 0);
    }

    #[test]
    fn test_histogram_prometheus_output() {
        let h = LatencyHistogram::new();
        h.record(5);
        h.record(10);
        h.record(50);

        let output = h.render("xergon_relay_request_duration_seconds");

        // Must contain histogram type declaration
        assert!(output.contains("# TYPE xergon_relay_request_duration_seconds histogram"));

        // Must contain bucket lines
        assert!(output.contains("xergon_relay_request_duration_seconds_bucket{le=\""));
        assert!(output.contains("xergon_relay_request_duration_seconds_bucket{le=\"+Inf\"} 3"));

        // Must contain _sum and _count
        assert!(output.contains("xergon_relay_request_duration_seconds_sum 0.065"));
        assert!(output.contains("xergon_relay_request_duration_seconds_count 3"));

        // Bucket boundaries should be in seconds
        assert!(output.contains("le=\"0.001\""));
        assert!(output.contains("le=\"0.01\""));
        assert!(output.contains("le=\"1\""));
        assert!(output.contains("le=\"10\""));
    }
}
