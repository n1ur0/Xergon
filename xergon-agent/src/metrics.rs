//! Prometheus-compatible metrics collector for the Xergon agent.
//!
//! Thread-safe counters and gauges backed by atomics.
//! Wire into AppState and increment on inference/settlement/rollup events.
//!
//! Also includes a `MetricsStore` — a DashMap-backed label-aware metrics
//! system used for HTTP request tracking and other dynamic metrics.

use std::collections::BTreeMap;
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
    storage_rent_check_total: AtomicU64,
    storage_rent_auto_topup_total: AtomicU64,
    gossip_messages_sent_total: AtomicU64,
    gossip_messages_received_total: AtomicU64,
    reputation_interactions_total: AtomicU64,

    // Gauges (can go up and down)
    gpu_rentals_active: AtomicU64,
    p2p_peers_known: AtomicU64,
    inference_latency_ms: AtomicU64,
    chain_sync_height: AtomicU64,
    chain_node_peers: AtomicU64,
    wallet_balance_nanoerg: AtomicU64,
    relay_connections: AtomicU64,
    gossip_peers_connected: AtomicU64,
    box_balance_nanoerg: AtomicU64,
    active_requests: AtomicU64,

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
            storage_rent_check_total: AtomicU64::new(0),
            storage_rent_auto_topup_total: AtomicU64::new(0),
            gossip_messages_sent_total: AtomicU64::new(0),
            gossip_messages_received_total: AtomicU64::new(0),
            reputation_interactions_total: AtomicU64::new(0),
            gpu_rentals_active: AtomicU64::new(0),
            p2p_peers_known: AtomicU64::new(0),
            inference_latency_ms: AtomicU64::new(0),
            chain_sync_height: AtomicU64::new(0),
            chain_node_peers: AtomicU64::new(0),
            wallet_balance_nanoerg: AtomicU64::new(0),
            relay_connections: AtomicU64::new(0),
            gossip_peers_connected: AtomicU64::new(0),
            box_balance_nanoerg: AtomicU64::new(0),
            active_requests: AtomicU64::new(0),
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

    /// Record a storage rent check.
    pub fn inc_storage_rent_check_total(&self, n: u64) {
        self.storage_rent_check_total.fetch_add(n, Ordering::Relaxed);
    }

    /// Record a storage rent auto-topup.
    pub fn inc_storage_rent_auto_topup_total(&self, n: u64) {
        self.storage_rent_auto_topup_total.fetch_add(n, Ordering::Relaxed);
    }

    /// Record gossip messages sent.
    pub fn inc_gossip_messages_sent_total(&self, n: u64) {
        self.gossip_messages_sent_total.fetch_add(n, Ordering::Relaxed);
    }

    /// Record gossip messages received.
    pub fn inc_gossip_messages_received_total(&self, n: u64) {
        self.gossip_messages_received_total.fetch_add(n, Ordering::Relaxed);
    }

    /// Record reputation interactions.
    pub fn inc_reputation_interactions_total(&self, n: u64) {
        self.reputation_interactions_total.fetch_add(n, Ordering::Relaxed);
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

    /// Set the number of gossip peers connected.
    pub fn set_gossip_peers_connected(&self, n: u64) {
        self.gossip_peers_connected.store(n, Ordering::Relaxed);
    }

    /// Set the box balance in nanoERG.
    pub fn set_box_balance_nanoerg(&self, nanoerg: u64) {
        self.box_balance_nanoerg.store(nanoerg, Ordering::Relaxed);
    }

    // ---- Active requests gauge (increment/decrement) ----

    /// Increment the active HTTP requests gauge.
    pub fn inc_active_requests(&self) {
        self.active_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement the active HTTP requests gauge.
    pub fn dec_active_requests(&self) {
        self.active_requests.fetch_sub(1, Ordering::Relaxed);
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

    pub fn storage_rent_check_total(&self) -> u64 {
        self.storage_rent_check_total.load(Ordering::Relaxed)
    }

    pub fn storage_rent_auto_topup_total(&self) -> u64 {
        self.storage_rent_auto_topup_total.load(Ordering::Relaxed)
    }

    pub fn gossip_messages_sent_total(&self) -> u64 {
        self.gossip_messages_sent_total.load(Ordering::Relaxed)
    }

    pub fn gossip_messages_received_total(&self) -> u64 {
        self.gossip_messages_received_total.load(Ordering::Relaxed)
    }

    pub fn reputation_interactions_total(&self) -> u64 {
        self.reputation_interactions_total.load(Ordering::Relaxed)
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
        self.wallet_balance_nanoerg.load(Ordering::Relaxed)
    }

    pub fn wallet_balance_erg(&self) -> f64 {
        self.wallet_balance_nanoerg() as f64 / 1_000_000_000.0
    }

    pub fn relay_connections(&self) -> u64 {
        self.relay_connections.load(Ordering::Relaxed)
    }

    pub fn gossip_peers_connected(&self) -> u64 {
        self.gossip_peers_connected.load(Ordering::Relaxed)
    }

    pub fn box_balance_nanoerg(&self) -> u64 {
        self.box_balance_nanoerg.load(Ordering::Relaxed)
    }

    pub fn active_requests(&self) -> u64 {
        self.active_requests.load(Ordering::Relaxed)
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
        let mut out = String::with_capacity(2048);

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

        out.push_str("# HELP xergon_agent_inference_errors_total Total failed inference requests (5xx)\n");
        out.push_str("# TYPE xergon_agent_inference_errors_total counter\n");
        out.push_str(&format!(
            "xergon_agent_inference_errors_total {}\n",
            self.inference_errors()
        ));

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

        // New built-in metrics (Phase 33)
        out.push_str("# HELP xergon_agent_http_requests_total Total HTTP requests\n");
        out.push_str("# TYPE xergon_agent_http_requests_total counter\n");
        out.push_str(&format!(
            "xergon_agent_http_requests_total {}\n",
            self.http_requests_total()
        ));

        out.push_str("# HELP xergon_agent_http_request_duration_seconds Total HTTP request duration\n");
        out.push_str("# TYPE xergon_agent_http_request_duration_seconds counter\n");
        out.push_str(&format!(
            "xergon_agent_http_request_duration_seconds {}\n",
            self.http_request_duration_secs()
        ));

        out.push_str("# HELP xergon_agent_active_requests Current active HTTP requests\n");
        out.push_str("# TYPE xergon_agent_active_requests gauge\n");
        out.push_str(&format!(
            "xergon_agent_active_requests {}\n",
            self.active_requests()
        ));

        out.push_str("# HELP xergon_agent_storage_rent_check_total Total storage rent checks\n");
        out.push_str("# TYPE xergon_agent_storage_rent_check_total counter\n");
        out.push_str(&format!(
            "xergon_agent_storage_rent_check_total {}\n",
            self.storage_rent_check_total()
        ));

        out.push_str("# HELP xergon_agent_storage_rent_auto_topup_total Total storage rent auto-topups\n");
        out.push_str("# TYPE xergon_agent_storage_rent_auto_topup_total counter\n");
        out.push_str(&format!(
            "xergon_agent_storage_rent_auto_topup_total {}\n",
            self.storage_rent_auto_topup_total()
        ));

        out.push_str("# HELP xergon_agent_gossip_messages_sent_total Total gossip messages sent\n");
        out.push_str("# TYPE xergon_agent_gossip_messages_sent_total counter\n");
        out.push_str(&format!(
            "xergon_agent_gossip_messages_sent_total {}\n",
            self.gossip_messages_sent_total()
        ));

        out.push_str("# HELP xergon_agent_gossip_messages_received_total Total gossip messages received\n");
        out.push_str("# TYPE xergon_agent_gossip_messages_received_total counter\n");
        out.push_str(&format!(
            "xergon_agent_gossip_messages_received_total {}\n",
            self.gossip_messages_received_total()
        ));

        out.push_str("# HELP xergon_agent_gossip_peers_connected Currently connected gossip peers\n");
        out.push_str("# TYPE xergon_agent_gossip_peers_connected gauge\n");
        out.push_str(&format!(
            "xergon_agent_gossip_peers_connected {}\n",
            self.gossip_peers_connected()
        ));

        out.push_str("# HELP xergon_agent_reputation_interactions_total Total reputation interactions\n");
        out.push_str("# TYPE xergon_agent_reputation_interactions_total counter\n");
        out.push_str(&format!(
            "xergon_agent_reputation_interactions_total {}\n",
            self.reputation_interactions_total()
        ));

        out.push_str("# HELP xergon_agent_blockchain_height Current blockchain height\n");
        out.push_str("# TYPE xergon_agent_blockchain_height gauge\n");
        out.push_str(&format!(
            "xergon_agent_blockchain_height {}\n",
            self.chain_sync_height()
        ));

        out.push_str("# HELP xergon_agent_box_balance_nanoerg Protocol box balance in nanoERG\n");
        out.push_str("# TYPE xergon_agent_box_balance_nanoerg gauge\n");
        out.push_str(&format!(
            "xergon_agent_box_balance_nanoerg {}\n",
            self.box_balance_nanoerg()
        ));

        out
    }

    /// Render all metrics as a JSON object.
    pub fn render_json(
        &self,
        pown_total: u64,
        pown_node: u64,
        pown_network: u64,
        pown_ai: u64,
    ) -> serde_json::Value {
        serde_json::json!({
            "xergon_pown_score": {
                "total": pown_total,
                "node": pown_node,
                "network": pown_network,
                "ai": pown_ai,
            },
            "xergon_inference_requests_total": self.inference_requests(),
            "xergon_inference_tokens_total": self.inference_tokens(),
            "xergon_settlement_earned_erg_total": self.settlement_earned_erg(),
            "xergon_uptime_seconds": self.uptime_seconds(),
            "xergon_gpu_rentals_active": self.gpu_rentals_active(),
            "xergon_p2p_peers_known": self.p2p_peers_known(),
            "xergon_rollup_commitments_total": self.rollup_commitments(),
            "xergon_agent_inference_errors_total": self.inference_errors(),
            "xergon_agent_inference_latency_ms": self.inference_latency_ms(),
            "xergon_agent_chain_sync_height": self.chain_sync_height(),
            "xergon_agent_chain_node_peers": self.chain_node_peers(),
            "xergon_agent_wallet_balance_nanoerg": self.wallet_balance_nanoerg(),
            "xergon_agent_relay_connections": self.relay_connections(),
            "xergon_agent_http_requests_total": self.http_requests_total(),
            "xergon_agent_http_request_duration_seconds": self.http_request_duration_secs(),
            "xergon_agent_active_requests": self.active_requests(),
            "xergon_agent_storage_rent_check_total": self.storage_rent_check_total(),
            "xergon_agent_storage_rent_auto_topup_total": self.storage_rent_auto_topup_total(),
            "xergon_agent_gossip_messages_sent_total": self.gossip_messages_sent_total(),
            "xergon_agent_gossip_messages_received_total": self.gossip_messages_received_total(),
            "xergon_agent_gossip_peers_connected": self.gossip_peers_connected(),
            "xergon_agent_reputation_interactions_total": self.reputation_interactions_total(),
            "xergon_agent_blockchain_height": self.chain_sync_height(),
            "xergon_agent_box_balance_nanoerg": self.box_balance_nanoerg(),
        })
    }

    // ---- HTTP metrics (from MetricsStore) ----

    /// Total HTTP requests tracked by the MetricsStore.
    pub fn http_requests_total(&self) -> u64 {
        // This is a pass-through to the MetricsStore if available;
        // for the atomic collector, we expose it as a simple counter.
        // The middleware increments this directly.
        0 // placeholder; actual tracking is via MetricsStore
    }

    /// Total HTTP request duration in seconds (sum of all durations).
    pub fn http_request_duration_secs(&self) -> f64 {
        0.0 // placeholder; actual tracking is via MetricsStore
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// MetricsStore: DashMap-backed, label-aware metrics for HTTP request tracking
// ---------------------------------------------------------------------------

use serde::{Deserialize, Serialize};

/// Metric type enumeration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetricType {
    Counter,
    Gauge,
    Histogram,
}

/// Metric descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metric {
    pub name: String,
    pub help: String,
    pub metric_type: MetricType,
    pub labels: Vec<String>,
}

/// A single metric data point with label values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricValue {
    pub labels: Vec<(String, String)>,
    pub value: f64,
}

/// Full metric data: descriptor + all recorded values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricData {
    pub metric: Metric,
    pub values: Vec<MetricValue>,
}

/// Key for the MetricsStore DashMap: metric name + sorted label values.
fn make_key(name: &str, label_values: &[(String, String)]) -> String {
    if label_values.is_empty() {
        return name.to_string();
    }
    let mut parts: Vec<String> = label_values.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
    parts.sort();
    format!("{}{{{}}}", name, parts.join(","))
}

/// Thread-safe metrics store backed by DashMap.
///
/// Supports counters, gauges, and histograms with arbitrary label sets.
pub struct MetricsStore {
    /// metric key -> (Metric descriptor, value)
    data: dashmap::DashMap<String, (Metric, f64)>,
    /// metric name -> Metric descriptor (for enumeration)
    descriptors: dashmap::DashMap<String, Metric>,
}

impl std::fmt::Debug for MetricsStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MetricsStore").finish()
    }
}

impl MetricsStore {
    /// Create a new empty metrics store.
    pub fn new() -> Self {
        Self {
            data: dashmap::DashMap::new(),
            descriptors: dashmap::DashMap::new(),
        }
    }

    /// Register a metric descriptor.
    pub fn register(&self, metric: Metric) {
        self.descriptors.insert(metric.name.clone(), metric);
    }

    /// Increment a counter by value. Creates the metric if it doesn't exist.
    pub fn counter_inc(&self, name: &str, help: &str, labels: &[(String, String)], value: f64) {
        let key = make_key(name, labels);
        self.descriptors
            .entry(name.to_string())
            .or_insert_with(|| Metric {
                name: name.to_string(),
                help: help.to_string(),
                metric_type: MetricType::Counter,
                labels: labels.iter().map(|(k, _)| k.clone()).collect(),
            });
        self.data
            .entry(key)
            .and_modify(|(_, v)| *v += value)
            .or_insert_with(|| {
                (
                    Metric {
                        name: name.to_string(),
                        help: help.to_string(),
                        metric_type: MetricType::Counter,
                        labels: labels.iter().map(|(k, _)| k.clone()).collect(),
                    },
                    value,
                )
            });
    }

    /// Set a gauge to a specific value.
    pub fn gauge_set(&self, name: &str, help: &str, labels: &[(String, String)], value: f64) {
        let key = make_key(name, labels);
        self.descriptors
            .entry(name.to_string())
            .or_insert_with(|| Metric {
                name: name.to_string(),
                help: help.to_string(),
                metric_type: MetricType::Gauge,
                labels: labels.iter().map(|(k, _)| k.clone()).collect(),
            });
        self.data.insert(
            key,
            (
                Metric {
                    name: name.to_string(),
                    help: help.to_string(),
                    metric_type: MetricType::Gauge,
                    labels: labels.iter().map(|(k, _)| k.clone()).collect(),
                },
                value,
            ),
        );
    }

    /// Increment a gauge by value.
    pub fn gauge_inc(&self, name: &str, help: &str, labels: &[(String, String)], value: f64) {
        let key = make_key(name, labels);
        self.descriptors
            .entry(name.to_string())
            .or_insert_with(|| Metric {
                name: name.to_string(),
                help: help.to_string(),
                metric_type: MetricType::Gauge,
                labels: labels.iter().map(|(k, _)| k.clone()).collect(),
            });
        self.data
            .entry(key)
            .and_modify(|(_, v)| *v += value)
            .or_insert_with(|| {
                (
                    Metric {
                        name: name.to_string(),
                        help: help.to_string(),
                        metric_type: MetricType::Gauge,
                        labels: labels.iter().map(|(k, _)| k.clone()).collect(),
                    },
                    value,
                )
            });
    }

    /// Record a histogram observation (stored as a counter of total sum + count).
    pub fn histogram_observe(
        &self,
        name: &str,
        help: &str,
        labels: &[(String, String)],
        value: f64,
    ) {
        // Store _sum and _count as counters
        let sum_labels: Vec<(String, String)> = labels.to_vec();
        let count_labels: Vec<(String, String)> = labels.to_vec();

        self.counter_inc(
            &format!("{}_sum", name),
            &format!("{} (sum)", help),
            &sum_labels,
            value,
        );
        self.counter_inc(
            &format!("{}_count", name),
            &format!("{} (count)", help),
            &count_labels,
            1.0,
        );

        // Register the histogram descriptor
        self.descriptors
            .entry(name.to_string())
            .or_insert_with(|| Metric {
                name: name.to_string(),
                help: help.to_string(),
                metric_type: MetricType::Histogram,
                labels: labels.iter().map(|(k, _)| k.clone()).collect(),
            });
    }

    /// Get all metric data.
    pub fn get_all(&self) -> Vec<MetricData> {
        // Group by metric name
        let mut by_name: BTreeMap<String, (Metric, Vec<MetricValue>)> = BTreeMap::new();

        for entry in self.data.iter() {
            let (key, (metric, value)) = entry.pair();
            // Parse label values from key
            let label_values = parse_labels_from_key(key, &metric.name);
            by_name
                .entry(metric.name.clone())
                .or_insert_with(|| (metric.clone(), Vec::new()))
                .1
                .push(MetricValue {
                    labels: label_values,
                    value: *value,
                });
        }

        by_name
            .into_iter()
            .map(|(_, (metric, values))| MetricData { metric, values })
            .collect()
    }

    /// Format all metrics in Prometheus text exposition format.
    pub fn format_prometheus(&self) -> String {
        let all = self.get_all();
        let mut out = String::with_capacity(1024);

        for md in &all {
            out.push_str(&format!("# HELP {} {}\n", md.metric.name, md.metric.help));
            out.push_str(&format!(
                "# TYPE {} {}\n",
                md.metric.name,
                match md.metric.metric_type {
                    MetricType::Counter => "counter",
                    MetricType::Gauge => "gauge",
                    MetricType::Histogram => "histogram",
                }
            ));
            for v in &md.values {
                if v.labels.is_empty() {
                    out.push_str(&format!("{} {}\n", md.metric.name, v.value));
                } else {
                    let labels_str = v
                        .labels
                        .iter()
                        .map(|(k, val)| format!("{}=\"{}\"", k, val))
                        .collect::<Vec<_>>()
                        .join(",");
                    out.push_str(&format!("{}{{{}}} {}\n", md.metric.name, labels_str, v.value));
                }
            }
        }

        out
    }

    /// Format all metrics as JSON.
    pub fn format_json(&self) -> serde_json::Value {
        let all = self.get_all();
        serde_json::json!({ "metrics": all })
    }

    /// Clear all metrics (useful for tests).
    #[cfg(test)]
    pub fn clear(&self) {
        self.data.clear();
        self.descriptors.clear();
    }
}

impl Default for MetricsStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse label key=value pairs from a DashMap key like "name{key1=val1,key2=val2}".
fn parse_labels_from_key(key: &str, metric_name: &str) -> Vec<(String, String)> {
    if key == metric_name {
        return Vec::new();
    }
    if let Some(rest) = key.strip_prefix(&format!("{}{{", metric_name)) {
        if let Some(inner) = rest.strip_suffix('}') {
            return inner
                .split(',')
                .filter_map(|pair| {
                    let mut parts = pair.splitn(2, '=');
                    let k = parts.next()?.to_string();
                    let v = parts.next()?.to_string();
                    Some((k, v))
                })
                .collect();
        }
    }
    Vec::new()
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
        assert_eq!(m.active_requests(), 0);
        assert_eq!(m.storage_rent_check_total(), 0);
        assert_eq!(m.storage_rent_auto_topup_total(), 0);
        assert_eq!(m.gossip_messages_sent_total(), 0);
        assert_eq!(m.gossip_messages_received_total(), 0);
        assert_eq!(m.reputation_interactions_total(), 0);
        assert_eq!(m.gossip_peers_connected(), 0);
        assert_eq!(m.box_balance_nanoerg(), 0);
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
    fn test_new_builtin_counters() {
        let m = MetricsCollector::new();
        m.inc_storage_rent_check_total(10);
        m.inc_storage_rent_auto_topup_total(3);
        m.inc_gossip_messages_sent_total(42);
        m.inc_gossip_messages_received_total(38);
        m.inc_reputation_interactions_total(7);

        assert_eq!(m.storage_rent_check_total(), 10);
        assert_eq!(m.storage_rent_auto_topup_total(), 3);
        assert_eq!(m.gossip_messages_sent_total(), 42);
        assert_eq!(m.gossip_messages_received_total(), 38);
        assert_eq!(m.reputation_interactions_total(), 7);
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
    fn test_new_builtin_gauges() {
        let m = MetricsCollector::new();
        m.set_gossip_peers_connected(8);
        m.set_box_balance_nanoerg(1_500_000_000);

        assert_eq!(m.gossip_peers_connected(), 8);
        assert_eq!(m.box_balance_nanoerg(), 1_500_000_000);
    }

    #[test]
    fn test_active_requests_gauge() {
        let m = MetricsCollector::new();
        m.inc_active_requests();
        m.inc_active_requests();
        m.inc_active_requests();
        assert_eq!(m.active_requests(), 3);
        m.dec_active_requests();
        assert_eq!(m.active_requests(), 2);
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
        m.inc_storage_rent_check_total(5);
        m.inc_gossip_messages_sent_total(20);
        m.set_gossip_peers_connected(4);
        m.set_box_balance_nanoerg(1_000_000_000);
        m.inc_active_requests();

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

        // New built-in metrics
        assert!(output.contains("# TYPE xergon_agent_storage_rent_check_total counter"));
        assert!(output.contains("xergon_agent_storage_rent_check_total 5"));
        assert!(output.contains("# TYPE xergon_agent_gossip_messages_sent_total counter"));
        assert!(output.contains("xergon_agent_gossip_messages_sent_total 20"));
        assert!(output.contains("# TYPE xergon_agent_gossip_peers_connected gauge"));
        assert!(output.contains("xergon_agent_gossip_peers_connected 4"));
        assert!(output.contains("# TYPE xergon_agent_active_requests gauge"));
        assert!(output.contains("xergon_agent_active_requests 1"));
        assert!(output.contains("# TYPE xergon_agent_box_balance_nanoerg gauge"));
        assert!(output.contains("xergon_agent_box_balance_nanoerg 1000000000"));
    }

    #[test]
    fn test_render_json_format() {
        let m = MetricsCollector::new();
        m.inc_inference_requests(10);
        m.set_gpu_rentals_active(3);

        let json = m.render_json(100, 40, 30, 30);
        assert_eq!(json["xergon_inference_requests_total"], 10);
        assert_eq!(json["xergon_gpu_rentals_active"], 3);
        assert_eq!(json["xergon_pown_score"]["total"], 100);
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

    // ---- MetricsStore tests ----

    #[test]
    fn test_store_counter_increment() {
        let store = MetricsStore::new();
        store.counter_inc(
            "http_requests_total",
            "Total HTTP requests",
            &[
                ("method".to_string(), "GET".to_string()),
                ("path".to_string(), "/health".to_string()),
            ],
            1.0,
        );
        store.counter_inc(
            "http_requests_total",
            "Total HTTP requests",
            &[
                ("method".to_string(), "GET".to_string()),
                ("path".to_string(), "/health".to_string()),
            ],
            1.0,
        );

        let all = store.get_all();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].metric.name, "http_requests_total");
        assert_eq!(all[0].metric.metric_type, MetricType::Counter);
        assert_eq!(all[0].values.len(), 1);
        assert_eq!(all[0].values[0].value, 2.0);
        assert_eq!(all[0].values[0].labels.len(), 2);
    }

    #[test]
    fn test_store_gauge_set() {
        let store = MetricsStore::new();
        store.gauge_set(
            "active_requests",
            "Active HTTP requests",
            &[],
            5.0,
        );
        store.gauge_set(
            "active_requests",
            "Active HTTP requests",
            &[],
            3.0,
        );

        let all = store.get_all();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].values[0].value, 3.0);
        assert_eq!(all[0].metric.metric_type, MetricType::Gauge);
    }

    #[test]
    fn test_store_gauge_inc() {
        let store = MetricsStore::new();
        store.gauge_inc("peers", "Connected peers", &[], 3.0);
        store.gauge_inc("peers", "Connected peers", &[], 2.0);

        let all = store.get_all();
        assert_eq!(all[0].values[0].value, 5.0);
    }

    #[test]
    fn test_store_histogram_observe() {
        let store = MetricsStore::new();
        store.histogram_observe(
            "request_duration_seconds",
            "Request duration",
            &[
                ("method".to_string(), "GET".to_string()),
                ("path".to_string(), "/api".to_string()),
            ],
            0.1,
        );
        store.histogram_observe(
            "request_duration_seconds",
            "Request duration",
            &[
                ("method".to_string(), "GET".to_string()),
                ("path".to_string(), "/api".to_string()),
            ],
            0.2,
        );

        let all = store.get_all();
        // histogram registers _sum and _count as separate counters
        assert!(all.len() >= 2);

        // Check the _sum and _count
        let sum_metric = all.iter().find(|m| m.metric.name == "request_duration_seconds_sum");
        let count_metric = all.iter().find(|m| m.metric.name == "request_duration_seconds_count");

        assert!(sum_metric.is_some());
        assert!((sum_metric.unwrap().values[0].value - 0.3).abs() < 1e-9);
        assert!(count_metric.is_some());
        assert_eq!(count_metric.unwrap().values[0].value, 2.0);
    }

    #[test]
    fn test_store_prometheus_format() {
        let store = MetricsStore::new();
        store.counter_inc(
            "http_requests_total",
            "Total HTTP requests",
            &[
                ("method".to_string(), "GET".to_string()),
                ("path".to_string(), "/health".to_string()),
                ("status".to_string(), "200".to_string()),
            ],
            42.0,
        );

        let output = store.format_prometheus();
        assert!(output.contains("# HELP http_requests_total Total HTTP requests"));
        assert!(output.contains("# TYPE http_requests_total counter"));
        assert!(output.contains("http_requests_total{"));
        assert!(output.contains("method=\"GET\""));
        assert!(output.contains("path=\"/health\""));
        assert!(output.contains("status=\"200\""));
        assert!(output.contains(" 42"));
    }

    #[test]
    fn test_store_json_format() {
        let store = MetricsStore::new();
        store.counter_inc(
            "test_counter",
            "A test counter",
            &[("label".to_string(), "value".to_string())],
            7.0,
        );

        let json = store.format_json();
        let metrics = json["metrics"].as_array().unwrap();
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0]["metric"]["name"], "test_counter");
        assert_eq!(metrics[0]["values"][0]["value"], 7.0);
    }

    #[test]
    fn test_store_multiple_label_sets() {
        let store = MetricsStore::new();
        store.counter_inc("requests", "Total", &[("method".to_string(), "GET".to_string())], 10.0);
        store.counter_inc("requests", "Total", &[("method".to_string(), "POST".to_string())], 5.0);

        let all = store.get_all();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].values.len(), 2);
    }
}
