//! Prometheus-compatible metrics collector for the Xergon agent.
//!
//! Thread-safe counters and gauges backed by atomics.
//! Wire into AppState and increment on inference/settlement/rollup events.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// Central metrics collector shared across the agent.
#[derive(Debug)]
pub struct MetricsCollector {
    // Counters (monotonically increasing)
    inference_requests: AtomicU64,
    inference_tokens: AtomicU64,
    inference_errors: AtomicU64,
    settlement_earned_nanoerg: AtomicU64,
    rollup_commitments: AtomicU64,

    // Gauges (can go up and down)
    gpu_rentals_active: AtomicU64,
    p2p_peers_known: AtomicU64,
    inference_latency_ms: AtomicU64,
    chain_sync_height: AtomicU64,
    chain_node_peers: AtomicU64,
    wallet_balance_nanoerg: AtomicU64,
    relay_connections: AtomicU64,

    /// Process start time for uptime computation.
    start_time: Instant,
}

impl MetricsCollector {
    /// Create a new metrics collector (all counters start at 0).
    pub fn new() -> Self {
        Self {
            inference_requests: AtomicU64::new(0),
            inference_tokens: AtomicU64::new(0),
            inference_errors: AtomicU64::new(0),
            settlement_earned_nanoerg: AtomicU64::new(0),
            rollup_commitments: AtomicU64::new(0),
            gpu_rentals_active: AtomicU64::new(0),
            p2p_peers_known: AtomicU64::new(0),
            inference_latency_ms: AtomicU64::new(0),
            chain_sync_height: AtomicU64::new(0),
            chain_node_peers: AtomicU64::new(0),
            wallet_balance_nanoerg: AtomicU64::new(0),
            relay_connections: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }

    // ---- Increment helpers ----

    /// Record a proxied inference request.
    pub fn inc_inference_requests(&self, n: u64) {
        self.inference_requests.fetch_add(n, Ordering::Relaxed);
    }

    /// Record tokens processed by the inference backend.
    pub fn inc_inference_tokens(&self, n: u64) {
        self.inference_tokens.fetch_add(n, Ordering::Relaxed);
    }

    /// Record a failed inference request (5xx from backend).
    pub fn inc_inference_errors(&self, n: u64) {
        self.inference_errors.fetch_add(n, Ordering::Relaxed);
    }

    /// Record ERG earned from settlements (in nanoERG).
    pub fn inc_settlement_earned_nanoerg(&self, nanoerg: u64) {
        self.settlement_earned_nanoerg
            .fetch_add(nanoerg, Ordering::Relaxed);
    }

    /// Record a rollup commitment submitted.
    pub fn inc_rollup_commitments(&self, n: u64) {
        self.rollup_commitments.fetch_add(n, Ordering::Relaxed);
    }

    // ---- Gauge setters ----

    /// Set the number of active GPU rentals.
    pub fn set_gpu_rentals_active(&self, n: u64) {
        self.gpu_rentals_active.store(n, Ordering::Relaxed);
    }

    /// Set the number of known P2P peers.
    pub fn set_p2p_peers_known(&self, n: u64) {
        self.p2p_peers_known.store(n, Ordering::Relaxed);
    }

    /// Set the last inference latency in ms.
    pub fn set_inference_latency_ms(&self, ms: u64) {
        self.inference_latency_ms.store(ms, Ordering::Relaxed);
    }

    /// Set the current Ergo chain height from node.
    pub fn set_chain_sync_height(&self, height: u64) {
        self.chain_sync_height.store(height, Ordering::Relaxed);
    }

    /// Set the number of connected Ergo node peers.
    pub fn set_chain_node_peers(&self, peers: u64) {
        self.chain_node_peers.store(peers, Ordering::Relaxed);
    }

    /// Set the wallet ERG balance in nanoERG.
    pub fn set_wallet_balance_nanoerg(&self, nanoerg: u64) {
        self.wallet_balance_nanoerg.store(nanoerg, Ordering::Relaxed);
    }

    /// Set the number of connected relays.
    pub fn set_relay_connections(&self, n: u64) {
        self.relay_connections.store(n, Ordering::Relaxed);
    }

    // ---- Read helpers ----

    pub fn inference_requests(&self) -> u64 {
        self.inference_requests.load(Ordering::Relaxed)
    }

    pub fn inference_tokens(&self) -> u64 {
        self.inference_tokens.load(Ordering::Relaxed)
    }

    pub fn inference_errors(&self) -> u64 {
        self.inference_errors.load(Ordering::Relaxed)
    }

    pub fn settlement_earned_nanoerg(&self) -> u64 {
        self.settlement_earned_nanoerg.load(Ordering::Relaxed)
    }

    /// Earned ERG as a human-readable float.
    pub fn settlement_earned_erg(&self) -> f64 {
        self.settlement_earned_nanoerg() as f64 / 1_000_000_000.0
    }

    pub fn rollup_commitments(&self) -> u64 {
        self.rollup_commitments.load(Ordering::Relaxed)
    }

    pub fn gpu_rentals_active(&self) -> u64 {
        self.gpu_rentals_active.load(Ordering::Relaxed)
    }

    pub fn p2p_peers_known(&self) -> u64 {
        self.p2p_peers_known.load(Ordering::Relaxed)
    }

    pub fn inference_latency_ms(&self) -> u64 {
        self.inference_latency_ms.load(Ordering::Relaxed)
    }

    pub fn chain_sync_height(&self) -> u64 {
        self.chain_sync_height.load(Ordering::Relaxed)
    }

    pub fn chain_node_peers(&self) -> u64 {
        self.chain_node_peers.load(Ordering::Relaxed)
    }

    pub fn wallet_balance_nanoerg(&self) -> u64 {
        self.wallet_balance_nanoerg
            .load(Ordering::Relaxed)
    }

    pub fn wallet_balance_erg(&self) -> f64 {
        self.wallet_balance_nanoerg() as f64 / 1_000_000_000.0
    }

    pub fn relay_connections(&self) -> u64 {
        self.relay_connections.load(Ordering::Relaxed)
    }

    pub fn uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Render all metrics in Prometheus text exposition format.
    ///
    /// Callers should supply the current PoNW score components.
    pub fn render_prometheus(
        &self,
        pown_total: u64,
        pown_node: u64,
        pown_network: u64,
        pown_ai: u64,
    ) -> String {
        let mut out = String::with_capacity(1024);

        // PoNW score gauges
        out.push_str("# HELP xergon_pown_score Current PoNW score\n");
        out.push_str("# TYPE xergon_pown_score gauge\n");
        out.push_str(&format!(
            "xergon_pown_score{{component=\"total\"}} {}\n",
            pown_total
        ));
        out.push_str(&format!(
            "xergon_pown_score{{component=\"node\"}} {}\n",
            pown_node
        ));
        out.push_str(&format!(
            "xergon_pown_score{{component=\"network\"}} {}\n",
            pown_network
        ));
        out.push_str(&format!(
            "xergon_pown_score{{component=\"ai\"}} {}\n",
            pown_ai
        ));

        // Counters
        out.push_str("# HELP xergon_inference_requests_total Total inference requests proxied\n");
        out.push_str("# TYPE xergon_inference_requests_total counter\n");
        out.push_str(&format!(
            "xergon_inference_requests_total {}\n",
            self.inference_requests()
        ));

        out.push_str("# HELP xergon_inference_tokens_total Total tokens processed\n");
        out.push_str("# TYPE xergon_inference_tokens_total counter\n");
        out.push_str(&format!(
            "xergon_inference_tokens_total {}\n",
            self.inference_tokens()
        ));

        out.push_str("# HELP xergon_settlement_earned_erg_total Total ERG earned from settlements\n");
        out.push_str("# TYPE xergon_settlement_earned_erg_total counter\n");
        out.push_str(&format!(
            "xergon_settlement_earned_erg_total {:.4}\n",
            self.settlement_earned_erg()
        ));

        // Gauges
        out.push_str("# HELP xergon_uptime_seconds Uptime in seconds\n");
        out.push_str("# TYPE xergon_uptime_seconds gauge\n");
        out.push_str(&format!("xergon_uptime_seconds {}\n", self.uptime_seconds()));

        out.push_str("# HELP xergon_gpu_rentals_active Active GPU rentals\n");
        out.push_str("# TYPE xergon_gpu_rentals_active gauge\n");
        out.push_str(&format!(
            "xergon_gpu_rentals_active {}\n",
            self.gpu_rentals_active()
        ));

        out.push_str("# HELP xergon_p2p_peers_known Known P2P peers\n");
        out.push_str("# TYPE xergon_p2p_peers_known gauge\n");
        out.push_str(&format!(
            "xergon_p2p_peers_known {}\n",
            self.p2p_peers_known()
        ));

        out.push_str("# HELP xergon_rollup_commitments_total Total rollup commitments submitted\n");
        out.push_str("# TYPE xergon_rollup_commitments_total counter\n");
        out.push_str(&format!(
            "xergon_rollup_commitments_total {}\n",
            self.rollup_commitments()
        ));

        // New counters
        out.push_str("# HELP xergon_agent_inference_errors_total Total failed inference requests (5xx)\n");
        out.push_str("# TYPE xergon_agent_inference_errors_total counter\n");
        out.push_str(&format!(
            "xergon_agent_inference_errors_total {}\n",
            self.inference_errors()
        ));

        // New gauges
        out.push_str("# HELP xergon_agent_inference_latency_ms Last inference latency in ms\n");
        out.push_str("# TYPE xergon_agent_inference_latency_ms gauge\n");
        out.push_str(&format!(
            "xergon_agent_inference_latency_ms {}\n",
            self.inference_latency_ms()
        ));

        out.push_str("# HELP xergon_agent_chain_sync_height Current Ergo chain height from node\n");
        out.push_str("# TYPE xergon_agent_chain_sync_height gauge\n");
        out.push_str(&format!(
            "xergon_agent_chain_sync_height {}\n",
            self.chain_sync_height()
        ));

        out.push_str("# HELP xergon_agent_chain_node_peers Connected Ergo node peers\n");
        out.push_str("# TYPE xergon_agent_chain_node_peers gauge\n");
        out.push_str(&format!(
            "xergon_agent_chain_node_peers {}\n",
            self.chain_node_peers()
        ));

        out.push_str("# HELP xergon_agent_wallet_balance_nanoerg Wallet ERG balance in nanoERG\n");
        out.push_str("# TYPE xergon_agent_wallet_balance_nanoerg gauge\n");
        out.push_str(&format!(
            "xergon_agent_wallet_balance_nanoerg {}\n",
            self.wallet_balance_nanoerg()
        ));

        out.push_str("# HELP xergon_agent_relay_connections Number of connected relays\n");
        out.push_str("# TYPE xergon_agent_relay_connections gauge\n");
        out.push_str(&format!(
            "xergon_agent_relay_connections {}\n",
            self.relay_connections()
        ));

        out
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collector_initial_state() {
        let m = MetricsCollector::new();
        assert_eq!(m.inference_requests(), 0);
        assert_eq!(m.inference_tokens(), 0);
        assert_eq!(m.inference_errors(), 0);
        assert_eq!(m.settlement_earned_nanoerg(), 0);
        assert_eq!(m.rollup_commitments(), 0);
        assert_eq!(m.gpu_rentals_active(), 0);
        assert_eq!(m.p2p_peers_known(), 0);
        assert_eq!(m.inference_latency_ms(), 0);
        assert_eq!(m.chain_sync_height(), 0);
        assert_eq!(m.chain_node_peers(), 0);
        assert_eq!(m.wallet_balance_nanoerg(), 0);
        assert_eq!(m.relay_connections(), 0);
    }

    #[test]
    fn test_counter_increments() {
        let m = MetricsCollector::new();
        m.inc_inference_requests(5);
        m.inc_inference_tokens(1000);
        m.inc_inference_errors(2);
        m.inc_settlement_earned_nanoerg(500_000_000); // 0.5 ERG
        m.inc_rollup_commitments(2);

        assert_eq!(m.inference_requests(), 5);
        assert_eq!(m.inference_tokens(), 1000);
        assert_eq!(m.inference_errors(), 2);
        assert_eq!(m.settlement_earned_erg(), 0.5);
        assert_eq!(m.rollup_commitments(), 2);
    }

    #[test]
    fn test_gauge_setters() {
        let m = MetricsCollector::new();
        m.set_gpu_rentals_active(3);
        m.set_p2p_peers_known(12);
        m.set_inference_latency_ms(450);
        m.set_chain_sync_height(1_234_567);
        m.set_chain_node_peers(25);
        m.set_wallet_balance_nanoerg(50_000_000_000);
        m.set_relay_connections(3);

        assert_eq!(m.gpu_rentals_active(), 3);
        assert_eq!(m.p2p_peers_known(), 12);
        assert_eq!(m.inference_latency_ms(), 450);
        assert_eq!(m.chain_sync_height(), 1_234_567);
        assert_eq!(m.chain_node_peers(), 25);
        assert_eq!(m.wallet_balance_nanoerg(), 50_000_000_000);
        assert_eq!(m.wallet_balance_erg(), 50.0);
        assert_eq!(m.relay_connections(), 3);

        // Can go down
        m.set_gpu_rentals_active(1);
        assert_eq!(m.gpu_rentals_active(), 1);
    }

    #[test]
    fn test_render_prometheus_format() {
        let m = MetricsCollector::new();
        m.inc_inference_requests(1234);
        m.inc_inference_tokens(567890);
        m.inc_inference_errors(3);
        m.set_gpu_rentals_active(2);
        m.set_p2p_peers_known(12);
        m.set_inference_latency_ms(450);
        m.set_chain_sync_height(999888);
        m.set_chain_node_peers(10);
        m.set_wallet_balance_nanoerg(25_000_000_000);
        m.set_relay_connections(2);

        let output = m.render_prometheus(847, 340, 280, 227);

        // Check that key lines exist
        assert!(output.contains("# HELP xergon_pown_score Current PoNW score"));
        assert!(output.contains("# TYPE xergon_pown_score gauge"));
        assert!(output.contains("xergon_pown_score{component=\"total\"} 847"));
        assert!(output.contains("xergon_pown_score{component=\"node\"} 340"));
        assert!(output.contains("# TYPE xergon_inference_requests_total counter"));
        assert!(output.contains("xergon_inference_requests_total 1234"));
        assert!(output.contains("xergon_inference_tokens_total 567890"));
        assert!(output.contains("xergon_gpu_rentals_active 2"));
        assert!(output.contains("xergon_p2p_peers_known 12"));
        assert!(output.contains("# TYPE xergon_uptime_seconds gauge"));
        assert!(output.contains("xergon_uptime_seconds"));
        assert!(output.contains("xergon_agent_inference_errors_total 3"));
        assert!(output.contains("xergon_agent_inference_latency_ms 450"));
        assert!(output.contains("xergon_agent_chain_sync_height 999888"));
        assert!(output.contains("xergon_agent_chain_node_peers 10"));
        assert!(output.contains("xergon_agent_wallet_balance_nanoerg 25000000000"));
        assert!(output.contains("xergon_agent_relay_connections 2"));
    }

    #[test]
    fn test_erg_formatting_precision() {
        let m = MetricsCollector::new();
        m.inc_settlement_earned_nanoerg(125_300_000_000); // 125.3 ERG

        let output = m.render_prometheus(0, 0, 0, 0);
        assert!(output.contains("xergon_settlement_earned_erg_total 125.3000"));
    }

    #[test]
    fn test_default_trait() {
        let m = MetricsCollector::default();
        assert_eq!(m.inference_requests(), 0);
    }
}
