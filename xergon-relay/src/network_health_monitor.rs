//! Network health monitoring for the Xergon Network relay.
//!
//! Tracks provider heartbeats, network topology, latency matrices,
//! anomaly detection, and health scoring. Standalone module with its own
//! state (no AppState field required).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json, Router,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{atomic::AtomicU64, LazyLock};
use uuid::Uuid;

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Provider health status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderStatus {
    Healthy,
    Degraded,
    Down,
    Unknown,
}

impl Default for ProviderStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Network node type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    Relay,
    Provider,
    Agent,
    Marketplace,
}

impl Default for NodeType {
    fn default() -> Self {
        Self::Relay
    }
}

/// Connection status between nodes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Active,
    Degraded,
    Down,
}

impl Default for ConnectionStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// Anomaly type classification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnomalyType {
    HighLatency,
    ProviderDown,
    ConsensusFailure,
    UnusualLoad,
    NetworkPartition,
    ProofRejection,
}

/// Anomaly severity level.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl Default for Severity {
    fn default() -> Self {
        Self::Medium
    }
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Heartbeat record from a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderHeartbeat {
    pub provider_id: String,
    pub status: ProviderStatus,
    pub last_heartbeat: DateTime<Utc>,
    pub latency_ms: u64,
    pub consecutive_failures: u32,
    pub region: String,
    pub models_served: Vec<String>,
    #[serde(deserialize_with = "deserialize_load")]
    pub load: f64,
}

fn deserialize_load<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v: f64 = serde::Deserialize::deserialize(deserializer)?;
    Ok(v.clamp(0.0, 1.0))
}

impl ProviderHeartbeat {
    pub fn new(provider_id: String) -> Self {
        Self {
            provider_id,
            status: ProviderStatus::Unknown,
            last_heartbeat: Utc::now(),
            latency_ms: 0,
            consecutive_failures: 0,
            region: String::new(),
            models_served: Vec::new(),
            load: 0.0,
        }
    }
}

/// A node in the network topology.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkNode {
    pub node_id: String,
    pub node_type: NodeType,
    pub address: String,
    pub region: String,
    pub status: ProviderStatus,
    pub uptime_ms: u64,
    pub version: String,
}

impl NetworkNode {
    pub fn new(node_id: String, node_type: NodeType, address: String) -> Self {
        Self {
            node_id,
            node_type,
            address,
            region: String::new(),
            status: ProviderStatus::Unknown,
            uptime_ms: 0,
            version: String::new(),
        }
    }
}

/// A connection between two network nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub source_id: String,
    pub target_id: String,
    pub latency_ms: u64,
    pub bandwidth: f64,
    pub status: ConnectionStatus,
}

impl Connection {
    pub fn connection_key(source_id: &str, target_id: &str) -> String {
        format!("{source_id}:{target_id}")
    }
}

/// Latency measurement between a pair of providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyPair {
    pub provider_a: String,
    pub provider_b: String,
    pub latency_ms: u64,
    pub jitter_ms: u64,
    #[serde(deserialize_with = "deserialize_load")]
    pub packet_loss: f64,
}

impl LatencyPair {
    pub fn pair_key(a: &str, b: &str) -> String {
        if a < b {
            format!("{a}:{b}")
        } else {
            format!("{b}:{a}")
        }
    }
}

/// Full latency matrix snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyMatrix {
    pub matrix_id: String,
    pub provider_pairs: Vec<LatencyPair>,
    pub updated_at: DateTime<Utc>,
    pub average_latency_ms: f64,
}

/// An anomaly detected in the network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyRecord {
    pub id: String,
    pub anomaly_type: AnomalyType,
    pub severity: Severity,
    pub description: String,
    pub affected_providers: Vec<String>,
    pub detected_at: DateTime<Utc>,
    pub resolved: bool,
    pub resolved_at: Option<DateTime<Utc>>,
}

impl AnomalyRecord {
    pub fn new(anomaly_type: AnomalyType, severity: Severity, description: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            anomaly_type,
            severity,
            description,
            affected_providers: Vec::new(),
            detected_at: Utc::now(),
            resolved: false,
            resolved_at: None,
        }
    }
}

/// Health score for a component or provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthScore {
    pub component: String,
    pub score: f64,
    pub factors: Vec<(String, f64)>,
    pub updated_at: DateTime<Utc>,
}

impl HealthScore {
    pub fn new(component: String, score: f64) -> Self {
        Self {
            component,
            score: score.clamp(0.0, 100.0),
            factors: Vec::new(),
            updated_at: Utc::now(),
        }
    }
}

/// Network topology snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkTopology {
    pub nodes: Vec<NetworkNode>,
    pub connections: Vec<Connection>,
    pub total_nodes: usize,
    pub active_nodes: usize,
}

/// Overall network health summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkHealthSummary {
    pub overall_score: f64,
    pub total_providers: usize,
    pub healthy_providers: usize,
    pub degraded_providers: usize,
    pub down_providers: usize,
    pub active_anomalies: usize,
    pub average_latency_ms: f64,
    pub total_nodes: usize,
    pub active_nodes: usize,
    pub timestamp: DateTime<Utc>,
}

/// Monitoring statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorStats {
    pub heartbeats_recorded: u64,
    pub nodes_registered: usize,
    pub connections_tracked: usize,
    pub latency_measurements: u64,
    pub anomalies_detected: usize,
    pub active_anomalies: usize,
    pub resolved_anomalies: usize,
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct HeartbeatRequest {
    pub provider_id: String,
    pub status: Option<ProviderStatus>,
    pub latency_ms: Option<u64>,
    pub region: Option<String>,
    pub models_served: Option<Vec<String>>,
    pub load: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct ProviderStatusResponse {
    pub provider_id: String,
    pub status: ProviderStatus,
    pub last_heartbeat: DateTime<Utc>,
    pub latency_ms: u64,
    pub consecutive_failures: u32,
    pub region: String,
    pub load: f64,
}

#[derive(Debug, Deserialize)]
pub struct RegisterNodeRequest {
    pub node_id: String,
    pub node_type: Option<NodeType>,
    pub address: String,
    pub region: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LatencyRecordRequest {
    pub provider_a: String,
    pub provider_b: String,
    pub latency_ms: u64,
    pub jitter_ms: Option<u64>,
    pub packet_loss: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct AnomalyResolveResponse {
    pub id: String,
    pub resolved: bool,
    pub resolved_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct GenericMessage {
    pub message: String,
}

// ---------------------------------------------------------------------------
// NetworkHealthMonitor
// ---------------------------------------------------------------------------

/// Core health monitoring engine.
pub struct NetworkHealthMonitor {
    heartbeats: DashMap<String, ProviderHeartbeat>,
    topology: DashMap<String, NetworkNode>,
    connections: DashMap<String, Connection>,
    anomalies: DashMap<String, AnomalyRecord>,
    latency_pairs: DashMap<String, LatencyPair>,
    health_scores: DashMap<String, HealthScore>,
    heartbeats_counter: AtomicU64,
    latency_counter: AtomicU64,
}

impl NetworkHealthMonitor {
    /// Create a new health monitor instance.
    pub fn new() -> Self {
        Self {
            heartbeats: DashMap::new(),
            topology: DashMap::new(),
            connections: DashMap::new(),
            anomalies: DashMap::new(),
            latency_pairs: DashMap::new(),
            health_scores: DashMap::new(),
            heartbeats_counter: AtomicU64::new(0),
            latency_counter: AtomicU64::new(0),
        }
    }

    // ---- Heartbeats ----

    /// Record a provider heartbeat, returning the previous status for anomaly detection.
    pub fn record_heartbeat(&self, req: &HeartbeatRequest) -> ProviderStatus {
        let prev_status = self
            .heartbeats
            .get(&req.provider_id)
            .map(|h| h.status.clone())
            .unwrap_or(ProviderStatus::Unknown);

        let mut entry = self
            .heartbeats
            .entry(req.provider_id.clone())
            .or_insert_with(|| ProviderHeartbeat::new(req.provider_id.clone()));

        let new_status = req.status.clone().unwrap_or(ProviderStatus::Healthy);

        // Track consecutive failures
        if new_status == ProviderStatus::Down || new_status == ProviderStatus::Degraded {
            entry.consecutive_failures += 1;
        } else {
            entry.consecutive_failures = 0;
        }

        // Detect status transition from healthy to down
        if prev_status == ProviderStatus::Healthy
            && (new_status == ProviderStatus::Down || new_status == ProviderStatus::Degraded)
        {
            let mut anomaly = AnomalyRecord::new(
                AnomalyType::ProviderDown,
                Severity::High,
                format!(
                    "Provider {} transitioned from {:?} to {:?}",
                    req.provider_id, prev_status, new_status
                ),
            );
            anomaly.affected_providers.push(req.provider_id.clone());
            self.anomalies.insert(anomaly.id.clone(), anomaly);
        }

        entry.status = new_status;
        entry.last_heartbeat = Utc::now();
        if let Some(lat) = req.latency_ms {
            entry.latency_ms = lat;
        }
        if let Some(ref region) = req.region {
            entry.region = region.clone();
        }
        if let Some(ref models) = req.models_served {
            entry.models_served = models.clone();
        }
        if let Some(load) = req.load {
            entry.load = load.clamp(0.0, 1.0);
        }

        self.heartbeats_counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        prev_status
    }

    /// Get heartbeat for a specific provider.
    pub fn get_heartbeat(&self, provider_id: &str) -> Option<ProviderHeartbeat> {
        self.heartbeats.get(provider_id).map(|h| h.clone())
    }

    /// List all provider heartbeats.
    pub fn list_heartbeats(&self) -> Vec<ProviderHeartbeat> {
        self.heartbeats.iter().map(|h| h.value().clone()).collect()
    }

    /// Get a provider's current status.
    pub fn get_provider_status(&self, provider_id: &str) -> Option<ProviderStatusResponse> {
        self.heartbeats.get(provider_id).map(|h| ProviderStatusResponse {
            provider_id: h.provider_id.clone(),
            status: h.status.clone(),
            last_heartbeat: h.last_heartbeat,
            latency_ms: h.latency_ms,
            consecutive_failures: h.consecutive_failures,
            region: h.region.clone(),
            load: h.load,
        })
    }

    // ---- Topology ----

    /// Register a new network node.
    pub fn register_node(&self, req: &RegisterNodeRequest) {
        let mut node = NetworkNode::new(
            req.node_id.clone(),
            req.node_type.clone().unwrap_or(NodeType::Relay),
            req.address.clone(),
        );
        if let Some(ref region) = req.region {
            node.region = region.clone();
        }
        if let Some(ref version) = req.version {
            node.version = version.clone();
        }
        node.status = ProviderStatus::Healthy;
        self.topology.insert(req.node_id.clone(), node);
    }

    /// Unregister a network node.
    pub fn unregister_node(&self, node_id: &str) -> bool {
        self.topology.remove(node_id).is_some()
    }

    /// List all registered nodes.
    pub fn list_nodes(&self) -> Vec<NetworkNode> {
        self.topology.iter().map(|n| n.value().clone()).collect()
    }

    /// Update a node's status.
    pub fn update_node_status(&self, node_id: &str, status: ProviderStatus) -> bool {
        if let Some(mut node) = self.topology.get_mut(node_id) {
            node.status = status;
            true
        } else {
            false
        }
    }

    /// Get full network topology snapshot.
    pub fn get_topology(&self) -> NetworkTopology {
        let nodes: Vec<NetworkNode> = self.list_nodes();
        let connections: Vec<Connection> = self.list_connections();
        let total_nodes = nodes.len();
        let active_nodes = nodes
            .iter()
            .filter(|n| n.status == ProviderStatus::Healthy)
            .count();
        NetworkTopology {
            nodes,
            connections,
            total_nodes,
            active_nodes,
        }
    }

    // ---- Connections ----

    /// Add a connection between two nodes.
    pub fn add_connection(&self, conn: Connection) {
        let key = Connection::connection_key(&conn.source_id, &conn.target_id);
        self.connections.insert(key, conn);
    }

    /// Remove a connection.
    pub fn remove_connection(&self, source_id: &str, target_id: &str) -> bool {
        let key = Connection::connection_key(source_id, target_id);
        self.connections.remove(&key).is_some()
    }

    /// List all connections.
    pub fn list_connections(&self) -> Vec<Connection> {
        self.connections.iter().map(|c| c.value().clone()).collect()
    }

    // ---- Latency ----

    /// Record a latency measurement between two providers.
    pub fn record_latency(&self, req: &LatencyRecordRequest) {
        let key = LatencyPair::pair_key(&req.provider_a, &req.provider_b);
        let jitter = req.jitter_ms.unwrap_or(0);
        let packet_loss = req.packet_loss.unwrap_or(0.0).clamp(0.0, 1.0);

        self.latency_pairs.insert(
            key,
            LatencyPair {
                provider_a: req.provider_a.clone(),
                provider_b: req.provider_b.clone(),
                latency_ms: req.latency_ms,
                jitter_ms: jitter,
                packet_loss,
            },
        );

        self.latency_counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Anomaly detection for high latency
        if req.latency_ms > 2000 {
            let mut anomaly = AnomalyRecord::new(
                AnomalyType::HighLatency,
                Severity::Medium,
                format!(
                    "High latency of {}ms between {} and {}",
                    req.latency_ms, req.provider_a, req.provider_b
                ),
            );
            anomaly.affected_providers = vec![
                req.provider_a.clone(),
                req.provider_b.clone(),
            ];
            self.anomalies.insert(anomaly.id.clone(), anomaly);
        }

        // Anomaly detection for high packet loss
        if packet_loss > 0.1 {
            let mut anomaly = AnomalyRecord::new(
                AnomalyType::NetworkPartition,
                Severity::High,
                format!(
                    "High packet loss ({:.1}%) between {} and {}",
                    packet_loss * 100.0,
                    req.provider_a,
                    req.provider_b
                ),
            );
            anomaly.affected_providers = vec![
                req.provider_a.clone(),
                req.provider_b.clone(),
            ];
            self.anomalies.insert(anomaly.id.clone(), anomaly);
        }
    }

    /// Get latency between two providers.
    pub fn get_latency(&self, provider_a: &str, provider_b: &str) -> Option<LatencyPair> {
        let key = LatencyPair::pair_key(provider_a, provider_b);
        self.latency_pairs.get(&key).map(|p| p.clone())
    }

    /// Get the full latency matrix snapshot.
    pub fn get_latency_matrix(&self) -> LatencyMatrix {
        let pairs: Vec<LatencyPair> = self
            .latency_pairs
            .iter()
            .map(|p| p.value().clone())
            .collect();
        let avg = pairs.iter().map(|p| p.latency_ms as f64).sum::<f64>()
            / pairs.len().max(1) as f64;
        LatencyMatrix {
            matrix_id: Uuid::new_v4().to_string(),
            provider_pairs: pairs,
            updated_at: Utc::now(),
            average_latency_ms: avg,
        }
    }

    /// Get average latency across all provider pairs.
    pub fn get_average_latency(&self) -> f64 {
        let pairs: Vec<u64> = self
            .latency_pairs
            .iter()
            .map(|p| p.value().latency_ms)
            .collect();
        if pairs.is_empty() {
            0.0
        } else {
            pairs.iter().sum::<u64>() as f64 / pairs.len() as f64
        }
    }

    // ---- Anomalies ----

    /// Run anomaly detection across the current state.
    /// Checks for missed heartbeats, high latency, and unusual load.
    pub fn detect_anomalies(&self) -> Vec<AnomalyRecord> {
        let mut new_anomalies = Vec::new();
        let now = Utc::now();
        let stale_threshold = chrono::Duration::seconds(120); // 2 minutes

        // Check for stale heartbeats
        for entry in self.heartbeats.iter() {
            let hb = entry.value();
            let elapsed = now.signed_duration_since(hb.last_heartbeat);
            if elapsed > stale_threshold && hb.status != ProviderStatus::Down {
                let mut anomaly = AnomalyRecord::new(
                    AnomalyType::ProviderDown,
                    Severity::High,
                    format!(
                        "Provider {} missed heartbeat (last seen {}s ago)",
                        hb.provider_id,
                        elapsed.num_seconds()
                    ),
                );
                anomaly.affected_providers.push(hb.provider_id.clone());
                new_anomalies.push(anomaly);
            }
        }

        // Check for unusual load
        for entry in self.heartbeats.iter() {
            let hb = entry.value();
            if hb.load > 0.95 && hb.status == ProviderStatus::Healthy {
                let mut anomaly = AnomalyRecord::new(
                    AnomalyType::UnusualLoad,
                    Severity::Medium,
                    format!("Provider {} load is critical at {:.1}%", hb.provider_id, hb.load * 100.0),
                );
                anomaly.affected_providers.push(hb.provider_id.clone());
                new_anomalies.push(anomaly);
            }
        }

        // Check for consecutive failures indicating systemic issues
        for entry in self.heartbeats.iter() {
            let hb = entry.value();
            if hb.consecutive_failures > 5 {
                let mut anomaly = AnomalyRecord::new(
                    AnomalyType::ConsensusFailure,
                    Severity::Critical,
                    format!(
                        "Provider {} has {} consecutive failures",
                        hb.provider_id, hb.consecutive_failures
                    ),
                );
                anomaly.affected_providers.push(hb.provider_id.clone());
                new_anomalies.push(anomaly);
            }
        }

        // Persist new anomalies (avoid duplicates by description)
        for anomaly in &new_anomalies {
            let is_dup = self.anomalies.iter().any(|existing| {
                existing.value().description == anomaly.description && !existing.value().resolved
            });
            if !is_dup {
                self.anomalies
                    .insert(anomaly.id.clone(), anomaly.clone());
            }
        }

        new_anomalies
    }

    /// Resolve an anomaly by ID.
    pub fn resolve_anomaly(&self, id: &str) -> Option<AnomalyRecord> {
        if let Some(mut anomaly) = self.anomalies.get_mut(id) {
            anomaly.resolved = true;
            anomaly.resolved_at = Some(Utc::now());
            Some(anomaly.clone())
        } else {
            None
        }
    }

    /// List all anomalies.
    pub fn list_anomalies(&self) -> Vec<AnomalyRecord> {
        self.anomalies.iter().map(|a| a.value().clone()).collect()
    }

    /// List only active (unresolved) anomalies.
    pub fn get_active_anomalies(&self) -> Vec<AnomalyRecord> {
        self.anomalies
            .iter()
            .filter(|a| !a.value().resolved)
            .map(|a| a.value().clone())
            .collect()
    }

    // ---- Health Scores ----

    /// Compute and store a health score for a given component.
    pub fn compute_health_score(&self, component: &str) -> HealthScore {
        let (score, factors) = match component {
            "providers" => {
                let total = self.heartbeats.len();
                let healthy = self
                    .heartbeats
                    .iter()
                    .filter(|h| h.value().status == ProviderStatus::Healthy)
                    .count();
                let score = if total > 0 {
                    (healthy as f64 / total as f64) * 100.0
                } else {
                    100.0
                };
                let factors = vec![
                    ("healthy_ratio".to_string(), score / 100.0),
                    ("total_providers".to_string(), total as f64),
                ];
                (score, factors)
            }
            "latency" => {
                let avg = self.get_average_latency();
                // Score degrades as latency increases: 0ms=100, 1000ms=0
                let score = (1.0 - (avg / 1000.0).min(1.0)) * 100.0;
                let factors = vec![
                    ("average_latency_ms".to_string(), avg),
                    ("latency_score".to_string(), score / 100.0),
                ];
                (score, factors)
            }
            "topology" => {
                let total = self.topology.len();
                let active = self
                    .topology
                    .iter()
                    .filter(|n| n.value().status == ProviderStatus::Healthy)
                    .count();
                let score = if total > 0 {
                    (active as f64 / total as f64) * 100.0
                } else {
                    100.0
                };
                let factors = vec![
                    ("active_ratio".to_string(), score / 100.0),
                    ("total_nodes".to_string(), total as f64),
                ];
                (score, factors)
            }
            "anomalies" => {
                let active = self.get_active_anomalies().len();
                let critical = self
                    .anomalies
                    .iter()
                    .filter(|a| {
                        !a.value().resolved && a.value().severity == Severity::Critical
                    })
                    .count();
                // Start at 100, subtract for each anomaly
                let score = (100.0 - (active as f64 * 10.0) - (critical as f64 * 30.0)).max(0.0);
                let factors = vec![
                    ("active_anomalies".to_string(), active as f64),
                    ("critical_anomalies".to_string(), critical as f64),
                ];
                (score, factors)
            }
            _ => {
                // Generic component - return neutral score
                (50.0, vec![("unknown_component".to_string(), 0.0)])
            }
        };

        let hs = HealthScore {
            component: component.to_string(),
            score,
            factors,
            updated_at: Utc::now(),
        };
        self.health_scores
            .insert(component.to_string(), hs.clone());
        hs
    }

    /// Get health score for a component.
    pub fn get_health_score(&self, component: &str) -> Option<HealthScore> {
        self.health_scores.get(component).map(|s| s.clone())
    }

    /// Compute overall network health as a weighted average.
    pub fn get_overall_health(&self) -> f64 {
        let components = ["providers", "latency", "topology", "anomalies"];
        let weights = [0.35, 0.25, 0.20, 0.20]; // Total = 1.0
        let scores: Vec<(f64, f64)> = components
            .iter()
            .zip(weights.iter())
            .map(|(c, w)| (self.compute_health_score(c).score, *w))
            .collect();
        let total: f64 = scores.iter().map(|(s, w)| s * w).sum();
        total
    }

    /// Get full health report with per-component scores and overall.
    pub fn get_health_report(&self) -> HashMap<String, HealthScore> {
        let components = ["providers", "latency", "topology", "anomalies"];
        let mut report = HashMap::new();
        for c in &components {
            report.insert(c.to_string(), self.compute_health_score(c));
        }
        report
    }

    // ---- Stats & Summary ----

    /// Get monitoring statistics.
    pub fn get_stats(&self) -> MonitorStats {
        let anomalies = self.list_anomalies();
        let active = anomalies.iter().filter(|a| !a.resolved).count();
        let resolved = anomalies.len() - active;
        MonitorStats {
            heartbeats_recorded: self
                .heartbeats_counter
                .load(std::sync::atomic::Ordering::Relaxed),
            nodes_registered: self.topology.len(),
            connections_tracked: self.connections.len(),
            latency_measurements: self
                .latency_counter
                .load(std::sync::atomic::Ordering::Relaxed),
            anomalies_detected: anomalies.len(),
            active_anomalies: active,
            resolved_anomalies: resolved,
        }
    }

    /// Get a full network health summary.
    pub fn get_network_summary(&self) -> NetworkHealthSummary {
        let heartbeats = self.list_heartbeats();
        let healthy = heartbeats
            .iter()
            .filter(|h| h.status == ProviderStatus::Healthy)
            .count();
        let degraded = heartbeats
            .iter()
            .filter(|h| h.status == ProviderStatus::Degraded)
            .count();
        let down = heartbeats
            .iter()
            .filter(|h| h.status == ProviderStatus::Down)
            .count();
        let topo = self.get_topology();

        NetworkHealthSummary {
            overall_score: self.get_overall_health(),
            total_providers: heartbeats.len(),
            healthy_providers: healthy,
            degraded_providers: degraded,
            down_providers: down,
            active_anomalies: self.get_active_anomalies().len(),
            average_latency_ms: self.get_average_latency(),
            total_nodes: topo.total_nodes,
            active_nodes: topo.active_nodes,
            timestamp: Utc::now(),
        }
    }
}

// ---------------------------------------------------------------------------
// Global static state
// ---------------------------------------------------------------------------

static MONITOR: LazyLock<NetworkHealthMonitor> = LazyLock::new(NetworkHealthMonitor::new);

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the network health monitor router.
pub fn build_router() -> Router<AppState> {
    Router::new()
        .route("/api/health/heartbeat", post(handle_record_heartbeat))
        .route("/api/health/heartbeats", get(handle_list_heartbeats))
        .route("/api/health/providers/{id}", get(handle_provider_status))
        .route("/api/health/topology/nodes", post(handle_register_node))
        .route("/api/health/topology", get(handle_get_topology))
        .route(
            "/api/health/topology/connections",
            get(handle_list_connections),
        )
        .route("/api/health/latency", post(handle_record_latency))
        .route("/api/health/latency/matrix", get(handle_latency_matrix))
        .route("/api/health/anomalies", get(handle_list_anomalies))
        .route(
            "/api/health/anomalies/{id}/resolve",
            post(handle_resolve_anomaly),
        )
        .route("/api/health/scores", get(handle_health_scores))
        .route("/api/health/summary", get(handle_network_summary))
        .route("/api/health/stats", get(handle_stats))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn handle_record_heartbeat(
    State(_state): State<AppState>,
    Json(req): Json<HeartbeatRequest>,
) -> (StatusCode, Json<GenericMessage>) {
    MONITOR.record_heartbeat(&req);
    (
        StatusCode::OK,
        Json(GenericMessage {
            message: format!("Heartbeat recorded for {}", req.provider_id),
        }),
    )
}

async fn handle_list_heartbeats(
    State(_state): State<AppState>,
) -> (StatusCode, Json<Vec<ProviderHeartbeat>>) {
    let heartbeats = MONITOR.list_heartbeats();
    (StatusCode::OK, Json(heartbeats))
}

async fn handle_provider_status(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match MONITOR.get_provider_status(&id) {
        Some(status) => (StatusCode::OK, Json(serde_json::to_value(status).unwrap())),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Provider not found"})),
        ),
    }
}

async fn handle_register_node(
    State(_state): State<AppState>,
    Json(req): Json<RegisterNodeRequest>,
) -> (StatusCode, Json<GenericMessage>) {
    MONITOR.register_node(&req);
    (
        StatusCode::CREATED,
        Json(GenericMessage {
            message: format!("Node {} registered", req.node_id),
        }),
    )
}

async fn handle_get_topology(
    State(_state): State<AppState>,
) -> (StatusCode, Json<NetworkTopology>) {
    let topo = MONITOR.get_topology();
    (StatusCode::OK, Json(topo))
}

async fn handle_list_connections(
    State(_state): State<AppState>,
) -> (StatusCode, Json<Vec<Connection>>) {
    let conns = MONITOR.list_connections();
    (StatusCode::OK, Json(conns))
}

async fn handle_record_latency(
    State(_state): State<AppState>,
    Json(req): Json<LatencyRecordRequest>,
) -> (StatusCode, Json<GenericMessage>) {
    MONITOR.record_latency(&req);
    (
        StatusCode::OK,
        Json(GenericMessage {
            message: format!(
                "Latency recorded: {} <-> {} = {}ms",
                req.provider_a, req.provider_b, req.latency_ms
            ),
        }),
    )
}

async fn handle_latency_matrix(
    State(_state): State<AppState>,
) -> (StatusCode, Json<LatencyMatrix>) {
    let matrix = MONITOR.get_latency_matrix();
    (StatusCode::OK, Json(matrix))
}

async fn handle_list_anomalies(
    State(_state): State<AppState>,
) -> (StatusCode, Json<Vec<AnomalyRecord>>) {
    let anomalies = MONITOR.list_anomalies();
    (StatusCode::OK, Json(anomalies))
}

async fn handle_resolve_anomaly(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match MONITOR.resolve_anomaly(&id) {
        Some(anomaly) => (
            StatusCode::OK,
            Json(serde_json::to_value(AnomalyResolveResponse {
                id: anomaly.id,
                resolved: anomaly.resolved,
                resolved_at: anomaly.resolved_at.unwrap(),
            })
            .unwrap()),
        ),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Anomaly not found"})),
        ),
    }
}

async fn handle_health_scores(
    State(_state): State<AppState>,
) -> (StatusCode, Json<HashMap<String, HealthScore>>) {
    let report = MONITOR.get_health_report();
    (StatusCode::OK, Json(report))
}

async fn handle_network_summary(
    State(_state): State<AppState>,
) -> (StatusCode, Json<NetworkHealthSummary>) {
    let summary = MONITOR.get_network_summary();
    (StatusCode::OK, Json(summary))
}

async fn handle_stats(
    State(_state): State<AppState>,
) -> (StatusCode, Json<MonitorStats>) {
    let stats = MONITOR.get_stats();
    (StatusCode::OK, Json(stats))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_monitor() -> NetworkHealthMonitor {
        NetworkHealthMonitor::new()
    }

    // -- Heartbeat tests --

    #[test]
    fn test_record_heartbeat_basic() {
        let m = fresh_monitor();
        let req = HeartbeatRequest {
            provider_id: "prov-1".to_string(),
            status: Some(ProviderStatus::Healthy),
            latency_ms: Some(42),
            region: Some("us-east".to_string()),
            models_served: Some(vec!["llama-3".to_string()]),
            load: Some(0.5),
        };
        m.record_heartbeat(&req);

        let hb = m.get_heartbeat("prov-1").unwrap();
        assert_eq!(hb.provider_id, "prov-1");
        assert_eq!(hb.status, ProviderStatus::Healthy);
        assert_eq!(hb.latency_ms, 42);
        assert_eq!(hb.region, "us-east");
        assert_eq!(hb.models_served.len(), 1);
        assert!((hb.load - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_heartbeat_updates_existing() {
        let m = fresh_monitor();
        let req1 = HeartbeatRequest {
            provider_id: "prov-2".into(),
            status: Some(ProviderStatus::Healthy),
            latency_ms: Some(100),
            region: None,
            models_served: None,
            load: Some(0.3),
        };
        m.record_heartbeat(&req1);

        let req2 = HeartbeatRequest {
            provider_id: "prov-2".into(),
            status: Some(ProviderStatus::Degraded),
            latency_ms: Some(500),
            region: Some("eu-west".into()),
            models_served: None,
            load: Some(0.9),
        };
        m.record_heartbeat(&req2);

        let hb = m.get_heartbeat("prov-2").unwrap();
        assert_eq!(hb.status, ProviderStatus::Degraded);
        assert_eq!(hb.latency_ms, 500);
        assert_eq!(hb.region, "eu-west");
        assert_eq!(hb.consecutive_failures, 1); // degraded counts as failure
    }

    #[test]
    fn test_consecutive_failures_reset_on_healthy() {
        let m = fresh_monitor();

        // Simulate 3 degraded heartbeats
        for _ in 0..3 {
            m.record_heartbeat(&HeartbeatRequest {
                provider_id: "prov-3".into(),
                status: Some(ProviderStatus::Degraded),
                latency_ms: None,
                region: None,
                models_served: None,
                load: None,
            });
        }
        assert_eq!(
            m.get_heartbeat("prov-3").unwrap().consecutive_failures,
            3
        );

        // Healthy heartbeat resets counter
        m.record_heartbeat(&HeartbeatRequest {
            provider_id: "prov-3".into(),
            status: Some(ProviderStatus::Healthy),
            latency_ms: None,
            region: None,
            models_served: None,
            load: None,
        });
        assert_eq!(
            m.get_heartbeat("prov-3").unwrap().consecutive_failures,
            0
        );
    }

    #[test]
    fn test_heartbeat_status_transition_creates_anomaly() {
        let m = fresh_monitor();

        // First heartbeat: healthy
        m.record_heartbeat(&HeartbeatRequest {
            provider_id: "prov-4".into(),
            status: Some(ProviderStatus::Healthy),
            latency_ms: Some(10),
            region: None,
            models_served: None,
            load: None,
        });

        // Second heartbeat: down (transition healthy -> down)
        m.record_heartbeat(&HeartbeatRequest {
            provider_id: "prov-4".into(),
            status: Some(ProviderStatus::Down),
            latency_ms: None,
            region: None,
            models_served: None,
            load: None,
        });

        let anomalies = m.list_anomalies();
        assert_eq!(anomalies.len(), 1);
        assert_eq!(anomalies[0].anomaly_type, AnomalyType::ProviderDown);
        assert_eq!(anomalies[0].affected_providers, vec!["prov-4"]);
    }

    #[test]
    fn test_list_heartbeats_empty() {
        let m = fresh_monitor();
        assert!(m.list_heartbeats().is_empty());
    }

    #[test]
    fn test_get_provider_status_not_found() {
        let m = fresh_monitor();
        assert!(m.get_provider_status("nonexistent").is_none());
    }

    // -- Topology tests --

    #[test]
    fn test_register_and_list_nodes() {
        let m = fresh_monitor();
        m.register_node(&RegisterNodeRequest {
            node_id: "node-1".into(),
            node_type: Some(NodeType::Provider),
            address: "https://node1.example.com".into(),
            region: Some("us-west".into()),
            version: Some("1.0.0".into()),
        });
        m.register_node(&RegisterNodeRequest {
            node_id: "node-2".into(),
            node_type: Some(NodeType::Relay),
            address: "https://node2.example.com".into(),
            region: None,
            version: None,
        });

        let nodes = m.list_nodes();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].node_id, "node-1");
        assert_eq!(nodes[0].node_type, NodeType::Provider);
        assert_eq!(nodes[0].region, "us-west");
        assert_eq!(nodes[0].version, "1.0.0");
    }

    #[test]
    fn test_unregister_node() {
        let m = fresh_monitor();
        m.register_node(&RegisterNodeRequest {
            node_id: "node-x".into(),
            node_type: None,
            address: "addr".into(),
            region: None,
            version: None,
        });
        assert_eq!(m.list_nodes().len(), 1);
        assert!(m.unregister_node("node-x"));
        assert!(m.list_nodes().is_empty());
        assert!(!m.unregister_node("node-x")); // already removed
    }

    #[test]
    fn test_update_node_status() {
        let m = fresh_monitor();
        m.register_node(&RegisterNodeRequest {
            node_id: "node-s".into(),
            node_type: None,
            address: "addr".into(),
            region: None,
            version: None,
        });
        assert!(m.update_node_status("node-s", ProviderStatus::Degraded));
        let node = m.list_nodes().into_iter().next().unwrap();
        assert_eq!(node.status, ProviderStatus::Degraded);
        assert!(!m.update_node_status("nonexistent", ProviderStatus::Down));
    }

    #[test]
    fn test_get_topology() {
        let m = fresh_monitor();
        m.register_node(&RegisterNodeRequest {
            node_id: "n1".into(),
            node_type: Some(NodeType::Provider),
            address: "a1".into(),
            region: None,
            version: None,
        });
        m.add_connection(Connection {
            source_id: "n1".into(),
            target_id: "n2".into(),
            latency_ms: 50,
            bandwidth: 1000.0,
            status: ConnectionStatus::Active,
        });

        let topo = m.get_topology();
        assert_eq!(topo.total_nodes, 1);
        assert_eq!(topo.active_nodes, 1); // newly registered = Healthy
        assert_eq!(topo.connections.len(), 1);
    }

    // -- Connection tests --

    #[test]
    fn test_add_and_remove_connection() {
        let m = fresh_monitor();
        m.add_connection(Connection {
            source_id: "a".into(),
            target_id: "b".into(),
            latency_ms: 100,
            bandwidth: 500.0,
            status: ConnectionStatus::Active,
        });
        assert_eq!(m.list_connections().len(), 1);
        assert!(m.remove_connection("a", "b"));
        assert!(m.list_connections().is_empty());
    }

    // -- Latency tests --

    #[test]
    fn test_record_and_get_latency() {
        let m = fresh_monitor();
        m.record_latency(&LatencyRecordRequest {
            provider_a: "pa".into(),
            provider_b: "pb".into(),
            latency_ms: 75,
            jitter_ms: Some(5),
            packet_loss: Some(0.01),
        });

        let pair = m.get_latency("pa", "pb").unwrap();
        assert_eq!(pair.latency_ms, 75);
        assert_eq!(pair.jitter_ms, 5);
        assert!((pair.packet_loss - 0.01).abs() < f64::EPSILON);

        // Key normalization: order shouldn't matter
        let pair_rev = m.get_latency("pb", "pa").unwrap();
        assert_eq!(pair_rev.latency_ms, 75);
    }

    #[test]
    fn test_latency_matrix() {
        let m = fresh_monitor();
        m.record_latency(&LatencyRecordRequest {
            provider_a: "a".into(),
            provider_b: "b".into(),
            latency_ms: 100,
            jitter_ms: None,
            packet_loss: None,
        });
        m.record_latency(&LatencyRecordRequest {
            provider_a: "b".into(),
            provider_b: "c".into(),
            latency_ms: 200,
            jitter_ms: None,
            packet_loss: None,
        });

        let matrix = m.get_latency_matrix();
        assert_eq!(matrix.provider_pairs.len(), 2);
        assert!((matrix.average_latency_ms - 150.0).abs() < f64::EPSILON);
        assert!(!matrix.matrix_id.is_empty());
    }

    #[test]
    fn test_high_latency_creates_anomaly() {
        let m = fresh_monitor();
        m.record_latency(&LatencyRecordRequest {
            provider_a: "slow-a".into(),
            provider_b: "slow-b".into(),
            latency_ms: 5000, // way above 2000ms threshold
            jitter_ms: None,
            packet_loss: None,
        });
        let anomalies = m.list_anomalies();
        assert!(anomalies.iter().any(|a| a.anomaly_type == AnomalyType::HighLatency));
    }

    #[test]
    fn test_high_packet_loss_creates_anomaly() {
        let m = fresh_monitor();
        m.record_latency(&LatencyRecordRequest {
            provider_a: "lossy-a".into(),
            provider_b: "lossy-b".into(),
            latency_ms: 50,
            jitter_ms: None,
            packet_loss: Some(0.5), // 50% packet loss
        });
        let anomalies = m.list_anomalies();
        assert!(anomalies
            .iter()
            .any(|a| a.anomaly_type == AnomalyType::NetworkPartition));
    }

    #[test]
    fn test_average_latency_empty() {
        let m = fresh_monitor();
        assert!((m.get_average_latency() - 0.0).abs() < f64::EPSILON);
    }

    // -- Anomaly tests --

    #[test]
    fn test_resolve_anomaly() {
        let m = fresh_monitor();
        let anomaly = AnomalyRecord::new(
            AnomalyType::ProofRejection,
            Severity::Low,
            "test anomaly".into(),
        );
        let id = anomaly.id.clone();
        m.anomalies.insert(id.clone(), anomaly);

        let resolved = m.resolve_anomaly(&id).unwrap();
        assert!(resolved.resolved);
        assert!(resolved.resolved_at.is_some());

        // Should no longer appear in active anomalies
        let active = m.get_active_anomalies();
        assert!(active.is_empty());
    }

    #[test]
    fn test_resolve_nonexistent_anomaly() {
        let m = fresh_monitor();
        assert!(m.resolve_anomaly("no-such-id").is_none());
    }

    #[test]
    fn test_detect_anomalies_consecutive_failures() {
        let m = fresh_monitor();
        // Simulate 7 consecutive degraded heartbeats
        for _ in 0..7 {
            m.record_heartbeat(&HeartbeatRequest {
                provider_id: "fail-provider".into(),
                status: Some(ProviderStatus::Degraded),
                latency_ms: None,
                region: None,
                models_served: None,
                load: None,
            });
        }

        let detected = m.detect_anomalies();
        assert!(detected
            .iter()
            .any(|a| a.anomaly_type == AnomalyType::ConsensusFailure));
    }

    #[test]
    fn test_detect_anomalies_unusual_load() {
        let m = fresh_monitor();
        m.record_heartbeat(&HeartbeatRequest {
            provider_id: "loaded".into(),
            status: Some(ProviderStatus::Healthy),
            latency_ms: None,
            region: None,
            models_served: None,
            load: Some(0.98),
        });

        let detected = m.detect_anomalies();
        assert!(detected
            .iter()
            .any(|a| a.anomaly_type == AnomalyType::UnusualLoad));
    }

    // -- Health score tests --

    #[test]
    fn test_compute_health_score_providers() {
        let m = fresh_monitor();
        m.record_heartbeat(&HeartbeatRequest {
            provider_id: "h1".into(),
            status: Some(ProviderStatus::Healthy),
            latency_ms: None,
            region: None,
            models_served: None,
            load: None,
        });
        m.record_heartbeat(&HeartbeatRequest {
            provider_id: "h2".into(),
            status: Some(ProviderStatus::Degraded),
            latency_ms: None,
            region: None,
            models_served: None,
            load: None,
        });

        let score = m.compute_health_score("providers");
        assert!((score.score - 50.0).abs() < f64::EPSILON);
        assert!(!score.factors.is_empty());
    }

    #[test]
    fn test_compute_health_score_unknown_component() {
        let m = fresh_monitor();
        let score = m.compute_health_score("nonexistent");
        assert!((score.score - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_get_overall_health() {
        let m = fresh_monitor();
        // No data -> all components default high scores
        let overall = m.get_overall_health();
        assert!(overall >= 0.0 && overall <= 100.0);
    }

    #[test]
    fn test_get_health_report() {
        let m = fresh_monitor();
        let report = m.get_health_report();
        assert!(report.contains_key("providers"));
        assert!(report.contains_key("latency"));
        assert!(report.contains_key("topology"));
        assert!(report.contains_key("anomalies"));
    }

    // -- Stats & summary tests --

    #[test]
    fn test_get_stats() {
        let m = fresh_monitor();
        m.record_heartbeat(&HeartbeatRequest {
            provider_id: "s1".into(),
            status: Some(ProviderStatus::Healthy),
            latency_ms: None,
            region: None,
            models_served: None,
            load: None,
        });
        m.record_latency(&LatencyRecordRequest {
            provider_a: "a".into(),
            provider_b: "b".into(),
            latency_ms: 10,
            jitter_ms: None,
            packet_loss: None,
        });

        let stats = m.get_stats();
        assert_eq!(stats.heartbeats_recorded, 1);
        assert_eq!(stats.latency_measurements, 1);
        assert_eq!(stats.nodes_registered, 0);
    }

    #[test]
    fn test_get_network_summary() {
        let m = fresh_monitor();
        m.record_heartbeat(&HeartbeatRequest {
            provider_id: "p1".into(),
            status: Some(ProviderStatus::Healthy),
            latency_ms: None,
            region: None,
            models_served: None,
            load: None,
        });

        let summary = m.get_network_summary();
        assert_eq!(summary.total_providers, 1);
        assert_eq!(summary.healthy_providers, 1);
        assert_eq!(summary.degraded_providers, 0);
        assert!(summary.overall_score >= 0.0 && summary.overall_score <= 100.0);
    }

    // -- Serialization tests --

    #[test]
    fn test_heartbeat_serialization() {
        let hb = ProviderHeartbeat::new("test-prov".into());
        let json = serde_json::to_string(&hb).unwrap();
        assert!(json.contains("\"provider_id\":\"test-prov\""));
        let deserialized: ProviderHeartbeat = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.provider_id, "test-prov");
    }

    #[test]
    fn test_anomaly_serialization() {
        let anomaly = AnomalyRecord::new(
            AnomalyType::NetworkPartition,
            Severity::Critical,
            "Network split detected".into(),
        );
        let json = serde_json::to_string(&anomaly).unwrap();
        assert!(json.contains("\"anomaly_type\":\"network_partition\""));
        assert!(json.contains("\"severity\":\"critical\""));
    }

    #[test]
    fn test_enum_rename_all() {
        let status = ProviderStatus::Degraded;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"degraded\"");

        let node_type = NodeType::Marketplace;
        let json = serde_json::to_string(&node_type).unwrap();
        assert_eq!(json, "\"marketplace\"");

        let severity = Severity::High;
        let json = serde_json::to_string(&severity).unwrap();
        assert_eq!(json, "\"high\"");
    }

    // -- Concurrent access tests --

    #[test]
    fn test_concurrent_heartbeat_recording() {
        use std::thread;

        let m = std::sync::Arc::new(fresh_monitor());
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let m = m.clone();
                thread::spawn(move || {
                    for j in 0..50 {
                        m.record_heartbeat(&HeartbeatRequest {
                            provider_id: format!("concurrent-{i}"),
                            status: Some(ProviderStatus::Healthy),
                            latency_ms: Some(j),
                            region: None,
                            models_served: None,
                            load: None,
                        });
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        let heartbeats = m.list_heartbeats();
        assert_eq!(heartbeats.len(), 10);
    }

    #[test]
    fn test_concurrent_topology_and_latency() {
        use std::thread;

        let m = std::sync::Arc::new(fresh_monitor());

        // Register nodes concurrently
        let node_handles: Vec<_> = (0..5)
            .map(|i| {
                let m = m.clone();
                thread::spawn(move || {
                    m.register_node(&RegisterNodeRequest {
                        node_id: format!("tnode-{i}"),
                        node_type: Some(NodeType::Relay),
                        address: format!("addr-{i}"),
                        region: None,
                        version: None,
                    });
                })
            })
            .collect();

        // Record latency concurrently
        let lat_handles: Vec<_> = (0..5)
            .map(|i| {
                let m = m.clone();
                thread::spawn(move || {
                    m.record_latency(&LatencyRecordRequest {
                        provider_a: format!("lat-a-{i}"),
                        provider_b: format!("lat-b-{i}"),
                        latency_ms: 10 + i,
                        jitter_ms: None,
                        packet_loss: None,
                    });
                })
            })
            .collect();

        for h in node_handles.into_iter().chain(lat_handles.into_iter()) {
            h.join().unwrap();
        }

        assert_eq!(m.list_nodes().len(), 5);
        assert_eq!(m.latency_pairs.len(), 5);
    }

    #[test]
    fn test_load_clamping() {
        let m = fresh_monitor();
        // Load > 1.0 should be clamped
        m.record_heartbeat(&HeartbeatRequest {
            provider_id: "clamp".into(),
            status: Some(ProviderStatus::Healthy),
            latency_ms: None,
            region: None,
            models_served: None,
            load: Some(5.0),
        });
        let hb = m.get_heartbeat("clamp").unwrap();
        assert!(hb.load <= 1.0);

        // Negative load should be clamped
        m.record_heartbeat(&HeartbeatRequest {
            provider_id: "clamp".into(),
            status: Some(ProviderStatus::Healthy),
            latency_ms: None,
            region: None,
            models_served: None,
            load: Some(-1.0),
        });
        let hb = m.get_heartbeat("clamp").unwrap();
        assert!(hb.load >= 0.0);
    }
}
