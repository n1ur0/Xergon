//! Distributed inference for the Xergon agent.
//!
//! Routes inference requests to remote nodes in the mesh.
//! Supports multiple distribution strategies (round-robin, least-loaded,
//! proximity-based, cost-optimized), health checking, and automatic failover.
//!
//! API:
//! - GET  /api/distributed/nodes   -- list connected nodes
//! - GET  /api/distributed/status  -- cluster status
//! - POST /api/distributed/forward -- manual forward request
//! - GET  /api/distributed/metrics -- node utilization metrics

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Distribution strategy for routing inference requests.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DistributionStrategy {
    RoundRobin,
    LeastLoaded,
    Proximity,
    CostOptimized,
}

impl Default for DistributionStrategy {
    fn default() -> Self {
        Self::LeastLoaded
    }
}

/// Health status of an inference node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NodeHealth {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

/// A remote inference node in the cluster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceNode {
    pub id: String,
    pub endpoint: String,
    pub models: Vec<String>,
    pub capacity: u32,
    pub load: f64,
    pub health: NodeHealth,
    pub latency_ms: f64,
    pub cost_per_token: f64,
    pub last_heartbeat: DateTime<Utc>,
    pub total_requests: u64,
    pub failed_requests: u64,
}

/// Status of a distributed request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DistributedRequestStatus {
    Pending,
    Forwarded,
    Completed,
    Failed,
    TimedOut,
}

/// A request forwarded to a remote node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedRequest {
    pub id: String,
    pub model: String,
    pub prompt: String,
    pub assigned_node: String,
    pub status: DistributedRequestStatus,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub result: Option<String>,
    pub error: Option<String>,
}

/// Request to forward inference to a remote node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardRequest {
    pub model: String,
    pub prompt: String,
    pub preferred_node: Option<String>,
    pub max_retries: Option<u32>,
    pub timeout_secs: Option<u64>,
}

/// Result from a forwarded inference request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardResult {
    pub request_id: String,
    pub node_id: String,
    pub status: DistributedRequestStatus,
    pub result: Option<String>,
    pub error: Option<String>,
    pub latency_ms: f64,
    pub retries_used: u32,
}

/// Cluster-wide status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterStatus {
    pub enabled: bool,
    pub strategy: DistributionStrategy,
    pub total_nodes: usize,
    pub healthy_nodes: usize,
    pub total_requests: u64,
    pub active_requests: usize,
    pub average_load: f64,
}

/// Metrics for the distributed inference system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedMetrics {
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub average_latency_ms: f64,
    pub p50_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub node_metrics: Vec<NodeMetrics>,
}

/// Per-node metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMetrics {
    pub node_id: String,
    pub endpoint: String,
    pub health: NodeHealth,
    pub load: f64,
    pub latency_ms: f64,
    pub total_requests: u64,
    pub failed_requests: u64,
    pub success_rate: f64,
}

// ---------------------------------------------------------------------------
// Distributed Inference Manager
// ---------------------------------------------------------------------------

/// Manages distributed inference across a cluster of remote nodes.
pub struct DistributedInferenceManager {
    /// All registered inference nodes
    nodes: RwLock<Vec<InferenceNode>>,
    /// Strategy for routing requests
    strategy: RwLock<DistributionStrategy>,
    /// All distributed requests
    requests: DashMap<String, DistributedRequest>,
    /// Latency history for percentile calculations
    latency_history: DashMap<String, Vec<f64>>,
    /// Request counter
    request_counter: AtomicU64,
    /// Round-robin index
    round_robin_index: AtomicU64,
    /// Whether distributed inference is enabled
    enabled: RwLock<bool>,
}

impl DistributedInferenceManager {
    /// Create a new distributed inference manager.
    pub fn new() -> Self {
        Self {
            nodes: RwLock::new(Vec::new()),
            strategy: RwLock::new(DistributionStrategy::default()),
            requests: DashMap::new(),
            latency_history: DashMap::new(),
            request_counter: AtomicU64::new(0),
            round_robin_index: AtomicU64::new(0),
            enabled: RwLock::new(false),
        }
    }

    /// Add a new inference node to the cluster.
    pub async fn add_node(&self, node: InferenceNode) {
        let mut nodes = self.nodes.write().await;
        // Remove existing node with same ID
        nodes.retain(|n| n.id != node.id);
        nodes.push(node);
        info!("Inference node added to cluster");
    }

    /// Remove a node from the cluster.
    pub async fn remove_node(&self, node_id: &str) -> bool {
        let mut nodes = self.nodes.write().await;
        let len_before = nodes.len();
        nodes.retain(|n| n.id != node_id);
        nodes.len() < len_before
    }

    /// Select the best node for a model based on the current strategy.
    pub async fn select_node(&self, model: &str) -> Result<InferenceNode, String> {
        let nodes = self.nodes.read().await;
        let strategy = self.strategy.read().await;

        // Filter nodes that serve the requested model and are healthy
        let candidates: Vec<&InferenceNode> = nodes
            .iter()
            .filter(|n| {
                n.models.contains(&model.to_string())
                    && (n.health == NodeHealth::Healthy || n.health == NodeHealth::Degraded)
            })
            .collect();

        if candidates.is_empty() {
            // Fallback: any healthy node
            let any_healthy: Vec<&InferenceNode> = nodes
                .iter()
                .filter(|n| n.health == NodeHealth::Healthy || n.health == NodeHealth::Degraded)
                .collect();

            if any_healthy.is_empty() {
                return Err("No healthy nodes available for inference".into());
            }
            return Ok(any_healthy[0].clone());
        }

        let selected = match &*strategy {
            DistributionStrategy::RoundRobin => {
                let idx = self.round_robin_index.fetch_add(1, Ordering::Relaxed) as usize;
                &candidates[idx % candidates.len()]
            }
            DistributionStrategy::LeastLoaded => {
                candidates
                    .iter()
                    .min_by(|a, b| a.load.partial_cmp(&b.load).unwrap_or(std::cmp::Ordering::Equal))
                    .unwrap()
            }
            DistributionStrategy::Proximity => {
                candidates
                    .iter()
                    .min_by(|a, b| a.latency_ms.partial_cmp(&b.latency_ms).unwrap_or(std::cmp::Ordering::Equal))
                    .unwrap()
            }
            DistributionStrategy::CostOptimized => {
                candidates
                    .iter()
                    .min_by(|a, b| a.cost_per_token.partial_cmp(&b.cost_per_token).unwrap_or(std::cmp::Ordering::Equal))
                    .unwrap()
            }
        };

        Ok((*selected).clone())
    }

    /// Forward an inference request to a remote node.
    pub async fn forward_request(&self, request: ForwardRequest) -> Result<ForwardResult, String> {
        let request_id = format!(
            "dist-{}",
            self.request_counter.fetch_add(1, Ordering::Relaxed)
        );

        let max_retries = request.max_retries.unwrap_or(3);
        let _timeout_secs = request.timeout_secs.unwrap_or(30);

        // Try preferred node first, then fall back to selection
        let node = if let Some(ref preferred) = request.preferred_node {
            let nodes = self.nodes.read().await;
            nodes
                .iter()
                .find(|n| n.id == *preferred)
                .cloned()
                .ok_or_else(|| {
                    format!("Preferred node '{}' not found", preferred)
                })?
        } else {
            self.select_node(&request.model).await?
        };

        // Create the distributed request
        let dist_request = DistributedRequest {
            id: request_id.clone(),
            model: request.model.clone(),
            prompt: request.prompt.clone(),
            assigned_node: node.id.clone(),
            status: DistributedRequestStatus::Forwarded,
            created_at: Utc::now(),
            completed_at: None,
            result: None,
            error: None,
        };
        self.requests.insert(request_id.clone(), dist_request);

        // Simulate inference with latency tracking
        let start = std::time::Instant::now();
        let simulated_latency = node.latency_ms + (node.load * 50.0);
        let elapsed_ms = simulated_latency;

        // Record latency
        self.latency_history
            .entry(node.id.clone())
            .or_default()
            .push(elapsed_ms);

        let mut retries_used: u32 = 0;
        let (status, result, error) = if node.health == NodeHealth::Healthy {
            (
                DistributedRequestStatus::Completed,
                Some(format!(
                    "[distributed] Simulated inference on node '{}' for model '{}'",
                    node.id, request.model
                )),
                None,
            )
        } else if retries_used < max_retries {
            retries_used += 1;
            (
                DistributedRequestStatus::Completed,
                Some(format!(
                    "[distributed] Inference completed on degraded node '{}' (retry {})",
                    node.id, retries_used
                )),
                None,
            )
        } else {
            (
                DistributedRequestStatus::Failed,
                None,
                Some(format!("Node '{}' failed after {} retries", node.id, retries_used)),
            )
        };

        // Update request
        if let Some(mut req) = self.requests.get_mut(&request_id) {
            req.status = status.clone();
            req.completed_at = Some(Utc::now());
            req.result = result.clone();
            req.error = error.clone();
        }

        debug!(
            request_id = %request_id,
            node_id = %node.id,
            latency_ms = elapsed_ms,
            ?status,
            "Distributed request completed"
        );

        let _ = start; // used for latency in real impl

        Ok(ForwardResult {
            request_id,
            node_id: node.id,
            status,
            result,
            error,
            latency_ms: elapsed_ms,
            retries_used,
        })
    }

    /// Aggregate results from multiple nodes (for ensemble inference).
    pub async fn aggregate_results(&self, request_ids: &[String]) -> Result<String, String> {
        let mut results = Vec::new();
        for id in request_ids {
            if let Some(req) = self.requests.get(id) {
                if let Some(ref result) = req.result {
                    results.push(result.clone());
                }
            }
        }

        if results.is_empty() {
            return Err("No results to aggregate".into());
        }

        Ok(format!(
            "[ensemble] Aggregated {} results from {} requests",
            results.len(),
            request_ids.len()
        ))
    }

    /// Perform a health check on all nodes.
    pub async fn health_check(&self) {
        let mut nodes = self.nodes.write().await;
        for node in nodes.iter_mut() {
            // In production, this would actually ping the node endpoint
            if node.last_heartbeat < Utc::now() - chrono::Duration::seconds(300) {
                node.health = NodeHealth::Unhealthy;
                warn!(node_id = %node.id, "Node marked unhealthy (no heartbeat)");
            } else if node.load > 0.9 {
                node.health = NodeHealth::Degraded;
            } else {
                node.health = NodeHealth::Healthy;
            }
        }
    }

    /// Update the distribution strategy.
    pub async fn set_strategy(&self, strategy: DistributionStrategy) {
        let mut s = self.strategy.write().await;
        *s = strategy.clone();
        info!(?strategy, "Distribution strategy updated");
    }

    /// Enable or disable distributed inference.
    pub async fn set_enabled(&self, enabled: bool) {
        let mut e = self.enabled.write().await;
        *e = enabled;
    }

    /// List all registered nodes.
    pub async fn list_nodes(&self) -> Vec<InferenceNode> {
        self.nodes.read().await.clone()
    }

    /// Get cluster status.
    pub async fn get_cluster_status(&self) -> ClusterStatus {
        let nodes = self.nodes.read().await;
        let strategy = self.strategy.read().await;
        let enabled = *self.enabled.read().await;

        let healthy_count = nodes
            .iter()
            .filter(|n| n.health == NodeHealth::Healthy)
            .count();

        let total_requests: u64 = nodes.iter().map(|n| n.total_requests).sum();
        let active_requests = self.requests
            .iter()
            .filter(|r| r.value().status == DistributedRequestStatus::Pending || r.value().status == DistributedRequestStatus::Forwarded)
            .count();

        let avg_load = if nodes.is_empty() {
            0.0
        } else {
            nodes.iter().map(|n| n.load).sum::<f64>() / nodes.len() as f64
        };

        ClusterStatus {
            enabled,
            strategy: strategy.clone(),
            total_nodes: nodes.len(),
            healthy_nodes: healthy_count,
            total_requests,
            active_requests,
            average_load: avg_load,
        }
    }

    /// Get distributed inference metrics.
    pub async fn get_metrics(&self) -> DistributedMetrics {
        let nodes = self.nodes.read().await;
        let mut total_requests: u64 = 0;
        let mut successful: u64 = 0;
        let mut failed: u64 = 0;
        let mut all_latencies: Vec<f64> = Vec::new();
        let mut node_metrics = Vec::new();

        for node in nodes.iter() {
            total_requests += node.total_requests;
            failed += node.failed_requests;
            let node_success = node.total_requests.saturating_sub(node.failed_requests);
            successful += node_success;

            if let Some(hist) = self.latency_history.get(&node.id) {
                all_latencies.extend(hist.iter().cloned());
            }

            let success_rate = if node.total_requests > 0 {
                node_success as f64 / node.total_requests as f64
            } else {
                1.0
            };

            node_metrics.push(NodeMetrics {
                node_id: node.id.clone(),
                endpoint: node.endpoint.clone(),
                health: node.health.clone(),
                load: node.load,
                latency_ms: node.latency_ms,
                total_requests: node.total_requests,
                failed_requests: node.failed_requests,
                success_rate,
            });
        }

        all_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let avg_latency = if all_latencies.is_empty() {
            0.0
        } else {
            all_latencies.iter().sum::<f64>() / all_latencies.len() as f64
        };
        let p50 = percentile(&all_latencies, 50.0);
        let p99 = percentile(&all_latencies, 99.0);

        DistributedMetrics {
            total_requests,
            successful_requests: successful,
            failed_requests: failed,
            average_latency_ms: avg_latency,
            p50_latency_ms: p50,
            p99_latency_ms: p99,
            node_metrics,
        }
    }
}

/// Calculate a percentile from a sorted slice of values.
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    /// Helper: create a healthy inference node.
    fn healthy_node(id: &str, models: &[&str], load: f64) -> InferenceNode {
        InferenceNode {
            id: id.to_string(),
            endpoint: format!("http://localhost:8080/{}", id),
            models: models.iter().map(|s| s.to_string()).collect(),
            capacity: 100,
            load,
            health: NodeHealth::Healthy,
            latency_ms: 10.0,
            cost_per_token: 1.0,
            last_heartbeat: Utc::now(),
            total_requests: 0,
            failed_requests: 0,
        }
    }

    /// Helper: create a degraded inference node.
    fn degraded_node(id: &str, models: &[&str], load: f64) -> InferenceNode {
        let mut n = healthy_node(id, models, load);
        n.health = NodeHealth::Degraded;
        n
    }

    /// Helper: create an unhealthy inference node.
    fn unhealthy_node(id: &str) -> InferenceNode {
        let mut n = healthy_node(id, &[], 0.0);
        n.health = NodeHealth::Unhealthy;
        n.last_heartbeat = Utc::now() - chrono::Duration::seconds(600);
        n
    }

    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_new_manager() {
        let mgr = DistributedInferenceManager::new();
        let nodes = mgr.list_nodes().await;
        assert!(nodes.is_empty());
        let status = mgr.get_cluster_status().await;
        assert!(!status.enabled);
        assert_eq!(status.total_nodes, 0);
    }

    // -----------------------------------------------------------------------
    // Register / list / remove nodes
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_add_and_list_nodes() {
        let mgr = DistributedInferenceManager::new();
        mgr.add_node(healthy_node("node-1", &["llama-3"], 0.3)).await;
        mgr.add_node(healthy_node("node-2", &["mistral"], 0.5)).await;
        let nodes = mgr.list_nodes().await;
        assert_eq!(nodes.len(), 2);
    }

    #[tokio::test]
    async fn test_add_node_replaces_existing() {
        let mgr = DistributedInferenceManager::new();
        mgr.add_node(healthy_node("node-1", &["model-a"], 0.1)).await;
        mgr.add_node(healthy_node("node-1", &["model-b"], 0.8)).await;
        let nodes = mgr.list_nodes().await;
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].models, vec!["model-b"]);
        assert!((nodes[0].load - 0.8).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_remove_node() {
        let mgr = DistributedInferenceManager::new();
        mgr.add_node(healthy_node("node-1", &["m"], 0.0)).await;
        mgr.add_node(healthy_node("node-2", &["m"], 0.0)).await;
        let removed = mgr.remove_node("node-1").await;
        assert!(removed);
        let nodes = mgr.list_nodes().await;
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].id, "node-2");
    }

    #[tokio::test]
    async fn test_remove_nonexistent_node() {
        let mgr = DistributedInferenceManager::new();
        let removed = mgr.remove_node("ghost").await;
        assert!(!removed);
    }

    // -----------------------------------------------------------------------
    // Forward: round-robin
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_forward_round_robin() {
        let mgr = DistributedInferenceManager::new();
        mgr.set_strategy(DistributionStrategy::RoundRobin).await;
        mgr.set_enabled(true).await;
        mgr.add_node(healthy_node("rr-1", &["llama"], 0.0)).await;
        mgr.add_node(healthy_node("rr-2", &["llama"], 0.0)).await;

        let r1 = mgr.forward_request(ForwardRequest {
            model: "llama".into(),
            prompt: "hello".into(),
            preferred_node: None,
            max_retries: None,
            timeout_secs: None,
        }).await.unwrap();
        let r2 = mgr.forward_request(ForwardRequest {
            model: "llama".into(),
            prompt: "world".into(),
            preferred_node: None,
            max_retries: None,
            timeout_secs: None,
        }).await.unwrap();
        // Round-robin should alternate between nodes
        assert_ne!(r1.node_id, r2.node_id);
        assert_eq!(r1.status, DistributedRequestStatus::Completed);
    }

    // -----------------------------------------------------------------------
    // Forward: least-loaded
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_forward_least_loaded() {
        let mgr = DistributedInferenceManager::new();
        mgr.set_strategy(DistributionStrategy::LeastLoaded).await;
        mgr.set_enabled(true).await;
        mgr.add_node(healthy_node("heavy", &["mistral"], 0.9)).await;
        mgr.add_node(healthy_node("light", &["mistral"], 0.1)).await;

        let result = mgr.forward_request(ForwardRequest {
            model: "mistral".into(),
            prompt: "test".into(),
            preferred_node: None,
            max_retries: None,
            timeout_secs: None,
        }).await.unwrap();
        assert_eq!(result.node_id, "light");
    }

    // -----------------------------------------------------------------------
    // Forward: proximity (lowest latency)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_forward_proximity() {
        let mgr = DistributedInferenceManager::new();
        mgr.set_strategy(DistributionStrategy::Proximity).await;
        mgr.set_enabled(true).await;
        mgr.add_node(healthy_node("far", &["gpt"], 0.0)).await;
        // Update latency for "near" node
        let mut near = healthy_node("near", &["gpt"], 0.0);
        near.latency_ms = 2.0;
        mgr.add_node(near).await;

        let result = mgr.forward_request(ForwardRequest {
            model: "gpt".into(),
            prompt: "test".into(),
            preferred_node: None,
            max_retries: None,
            timeout_secs: None,
        }).await.unwrap();
        assert_eq!(result.node_id, "near");
    }

    // -----------------------------------------------------------------------
    // Forward: cost-optimized
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_forward_cost_optimized() {
        let mgr = DistributedInferenceManager::new();
        mgr.set_strategy(DistributionStrategy::CostOptimized).await;
        mgr.set_enabled(true).await;
        let mut expensive = healthy_node("premium", &["phi"], 0.0);
        expensive.cost_per_token = 10.0;
        mgr.add_node(expensive).await;
        let mut cheap = healthy_node("budget", &["phi"], 0.0);
        cheap.cost_per_token = 0.5;
        mgr.add_node(cheap).await;

        let result = mgr.forward_request(ForwardRequest {
            model: "phi".into(),
            prompt: "test".into(),
            preferred_node: None,
            max_retries: None,
            timeout_secs: None,
        }).await.unwrap();
        assert_eq!(result.node_id, "budget");
    }

    // -----------------------------------------------------------------------
    // Forward: preferred node
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_forward_preferred_node() {
        let mgr = DistributedInferenceManager::new();
        mgr.set_enabled(true).await;
        mgr.add_node(healthy_node("target", &["llama"], 0.0)).await;
        mgr.add_node(healthy_node("other", &["llama"], 0.0)).await;

        let result = mgr.forward_request(ForwardRequest {
            model: "llama".into(),
            prompt: "test".into(),
            preferred_node: Some("target".into()),
            max_retries: None,
            timeout_secs: None,
        }).await.unwrap();
        assert_eq!(result.node_id, "target");
    }

    #[tokio::test]
    async fn test_forward_preferred_node_not_found() {
        let mgr = DistributedInferenceManager::new();
        mgr.set_enabled(true).await;
        mgr.add_node(healthy_node("exists", &["m"], 0.0)).await;

        let result = mgr.forward_request(ForwardRequest {
            model: "m".into(),
            prompt: "test".into(),
            preferred_node: Some("ghost".into()),
            max_retries: None,
            timeout_secs: None,
        }).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    // -----------------------------------------------------------------------
    // Cluster status
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_cluster_status() {
        let mgr = DistributedInferenceManager::new();
        mgr.set_enabled(true).await;
        mgr.set_strategy(DistributionStrategy::RoundRobin).await;
        mgr.add_node(healthy_node("h1", &["m"], 0.2)).await;
        mgr.add_node(unhealthy_node("u1")).await;

        let status = mgr.get_cluster_status().await;
        assert!(status.enabled);
        assert_eq!(status.total_nodes, 2);
        assert_eq!(status.healthy_nodes, 1);
        assert_eq!(status.strategy, DistributionStrategy::RoundRobin);
    }

    // -----------------------------------------------------------------------
    // Metrics
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_get_metrics_empty() {
        let mgr = DistributedInferenceManager::new();
        let metrics = mgr.get_metrics().await;
        assert_eq!(metrics.total_requests, 0);
        assert_eq!(metrics.successful_requests, 0);
        assert_eq!(metrics.failed_requests, 0);
        assert!(metrics.node_metrics.is_empty());
    }

    #[tokio::test]
    async fn test_get_metrics_after_forward() {
        let mgr = DistributedInferenceManager::new();
        mgr.set_enabled(true).await;
        let mut node = healthy_node("m1", &["test-model"], 0.0);
        node.total_requests = 5;
        node.failed_requests = 1;
        mgr.add_node(node).await;

        mgr.forward_request(ForwardRequest {
            model: "test-model".into(),
            prompt: "hi".into(),
            preferred_node: None,
            max_retries: None,
            timeout_secs: None,
        }).await.unwrap();

        let metrics = mgr.get_metrics().await;
        assert_eq!(metrics.node_metrics.len(), 1);
        assert_eq!(metrics.node_metrics[0].node_id, "m1");
    }

    // -----------------------------------------------------------------------
    // Health checking
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_health_check_marks_unhealthy() {
        let mgr = DistributedInferenceManager::new();
        // Node with stale heartbeat
        mgr.add_node(unhealthy_node("stale")).await;
        mgr.health_check().await;
        let nodes = mgr.list_nodes().await;
        assert_eq!(nodes[0].health, NodeHealth::Unhealthy);
    }

    #[tokio::test]
    async fn test_health_check_marks_degraded() {
        let mgr = DistributedInferenceManager::new();
        let mut node = healthy_node("overloaded", &["m"], 0.0);
        node.load = 0.95; // over 0.9 threshold
        node.last_heartbeat = Utc::now();
        mgr.add_node(node).await;
        mgr.health_check().await;
        let nodes = mgr.list_nodes().await;
        assert_eq!(nodes[0].health, NodeHealth::Degraded);
    }

    #[tokio::test]
    async fn test_health_check_restores_healthy() {
        let mgr = DistributedInferenceManager::new();
        let mut node = degraded_node("recovering", &["m"], 0.3);
        node.last_heartbeat = Utc::now();
        mgr.add_node(node).await;
        mgr.health_check().await;
        let nodes = mgr.list_nodes().await;
        assert_eq!(nodes[0].health, NodeHealth::Healthy);
    }

    // -----------------------------------------------------------------------
    // Failover: unhealthy nodes excluded from selection
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_failover_skips_unhealthy() {
        let mgr = DistributedInferenceManager::new();
        mgr.set_strategy(DistributionStrategy::LeastLoaded).await;
        mgr.set_enabled(true).await;
        mgr.add_node(unhealthy_node("dead")).await;
        mgr.add_node(healthy_node("alive", &["llama"], 0.1)).await;

        let result = mgr.forward_request(ForwardRequest {
            model: "llama".into(),
            prompt: "test".into(),
            preferred_node: None,
            max_retries: None,
            timeout_secs: None,
        }).await.unwrap();
        assert_eq!(result.node_id, "alive");
    }

    // -----------------------------------------------------------------------
    // Empty node list handling
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_forward_no_nodes() {
        let mgr = DistributedInferenceManager::new();
        let result = mgr.forward_request(ForwardRequest {
            model: "any".into(),
            prompt: "test".into(),
            preferred_node: None,
            max_retries: None,
            timeout_secs: None,
        }).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No healthy nodes"));
    }

    #[tokio::test]
    async fn test_select_node_no_healthy_nodes() {
        let mgr = DistributedInferenceManager::new();
        mgr.add_node(unhealthy_node("dead-1")).await;
        mgr.add_node(unhealthy_node("dead-2")).await;
        let result = mgr.select_node("any-model").await;
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Concurrent forwards
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_concurrent_forwards() {
        let mgr = DistributedInferenceManager::new();
        mgr.set_enabled(true).await;
        for i in 0..5 {
            mgr.add_node(healthy_node(
                &format!("conc-{}", i),
                &["shared-model"],
                0.0,
            ))
            .await;
        }

        // Since DistributedInferenceManager doesn't implement Clone, we use
        // Arc to share it across tasks.
        let mgr = Arc::new(mgr);
        let mut handles: Vec<tokio::task::JoinHandle<ForwardResult>> = Vec::new();
        for _ in 0..20 {
            let mgr = mgr.clone();
            handles.push(tokio::spawn(async move {
                mgr.forward_request(ForwardRequest {
                    model: "shared-model".into(),
                    prompt: "concurrent".into(),
                    preferred_node: None,
                    max_retries: None,
                    timeout_secs: None,
                })
                .await
                .unwrap()
            }));
        }

        let mut results = Vec::new();
        for h in handles {
            results.push(h.await.unwrap());
        }
        assert_eq!(results.len(), 20);
        for r in &results {
            assert_eq!(r.status, DistributedRequestStatus::Completed);
        }
    }

    // -----------------------------------------------------------------------
    // Aggregate results
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_aggregate_results() {
        let mgr = DistributedInferenceManager::new();
        mgr.set_enabled(true).await;
        mgr.add_node(healthy_node("agg-1", &["m"], 0.0)).await;

        let r1 = mgr.forward_request(ForwardRequest {
            model: "m".into(),
            prompt: "a".into(),
            preferred_node: None,
            max_retries: None,
            timeout_secs: None,
        }).await.unwrap();
        let r2 = mgr.forward_request(ForwardRequest {
            model: "m".into(),
            prompt: "b".into(),
            preferred_node: None,
            max_retries: None,
            timeout_secs: None,
        }).await.unwrap();

        let aggregated = mgr
            .aggregate_results(&[r1.request_id, r2.request_id])
            .await
            .unwrap();
        assert!(aggregated.contains("Aggregated 2 results"));
    }

    #[tokio::test]
    async fn test_aggregate_no_results() {
        let mgr = DistributedInferenceManager::new();
        let result = mgr.aggregate_results(&["fake-id".into()]).await;
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Strategy setter
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_set_strategy() {
        let mgr = DistributedInferenceManager::new();
        mgr.set_strategy(DistributionStrategy::CostOptimized).await;
        let status = mgr.get_cluster_status().await;
        assert_eq!(status.strategy, DistributionStrategy::CostOptimized);
    }

    // -----------------------------------------------------------------------
    // Enable / disable
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_enable_disable() {
        let mgr = DistributedInferenceManager::new();
        assert!(!mgr.get_cluster_status().await.enabled);
        mgr.set_enabled(true).await;
        assert!(mgr.get_cluster_status().await.enabled);
        mgr.set_enabled(false).await;
        assert!(!mgr.get_cluster_status().await.enabled);
    }

    // -----------------------------------------------------------------------
    // Fallback to any healthy node when no model-specific node
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_fallback_to_any_healthy() {
        let mgr = DistributedInferenceManager::new();
        mgr.set_strategy(DistributionStrategy::LeastLoaded).await;
        mgr.set_enabled(true).await;
        // No node serves "unknown-model" but both are healthy
        mgr.add_node(healthy_node("fb-1", &["other-model"], 0.0)).await;
        mgr.add_node(healthy_node("fb-2", &["other-model"], 0.0)).await;

        let result = mgr.forward_request(ForwardRequest {
            model: "unknown-model".into(),
            prompt: "test".into(),
            preferred_node: None,
            max_retries: None,
            timeout_secs: None,
        }).await.unwrap();
        // Should still succeed via fallback
        assert_eq!(result.status, DistributedRequestStatus::Completed);
    }
}
