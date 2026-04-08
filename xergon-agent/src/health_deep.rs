//! Deep health checks for the Xergon agent.
//!
//! Provides per-component health status including ergo_node, storage_rent,
//! gossip, reputation, and chain connectivity. Used by `/api/health/deep`.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// Status of a single health check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "healthy"),
            HealthStatus::Degraded => write!(f, "degraded"),
            HealthStatus::Unhealthy => write!(f, "unhealthy"),
        }
    }
}

/// Result of a single component health check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub component: String,
    pub status: HealthStatus,
    pub message: String,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl HealthCheckResult {
    /// Convenience constructor.
    pub fn new(component: &str, status: HealthStatus, message: &str, duration_ms: u64) -> Self {
        Self {
            component: component.to_string(),
            status,
            message: message.to_string(),
            duration_ms,
            details: None,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

/// Response for GET /api/health/deep
#[derive(Debug, Serialize)]
pub struct DeepHealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_secs: u64,
    pub checks: Vec<HealthCheckResult>,
}

/// Aggregate overall status from a list of check results.
///
/// - If any is Unhealthy -> "unhealthy"
/// - Else if any is Degraded -> "degraded"
/// - Else -> "healthy"
pub fn aggregate_status(checks: &[HealthCheckResult]) -> String {
    if checks.iter().any(|c| c.status == HealthStatus::Unhealthy) {
        "unhealthy".to_string()
    } else if checks.iter().any(|c| c.status == HealthStatus::Degraded) {
        "degraded".to_string()
    } else {
        "healthy".to_string()
    }
}

/// Deep health checker.
///
/// Each method checks a specific subsystem and returns a `HealthCheckResult`.
/// Methods accept references to shared state so they can be called without
/// requiring ownership of the full `AppState`.
pub struct HealthChecker;

impl HealthChecker {
    /// Check the Ergo node via the cached `NodeHealthState`.
    pub async fn check_ergo_node(
        node_health: &Arc<RwLock<crate::node_health::NodeHealthState>>,
    ) -> HealthCheckResult {
        let start = Instant::now();
        let health = node_health.read().await;

        let (status, message) = if health.is_synced && health.peer_count > 0 {
            (
                HealthStatus::Healthy,
                format!(
                    "Height: {}, Synced: true, Peers: {}",
                    health.node_height, health.peer_count
                ),
            )
        } else if health.is_synced {
            (
                HealthStatus::Degraded,
                format!(
                    "Height: {}, Synced: true, Peers: {} (low)",
                    health.node_height, health.peer_count
                ),
            )
        } else {
            (
                HealthStatus::Unhealthy,
                format!(
                    "Height: {}, Synced: false, Peers: {}",
                    health.node_height, health.peer_count
                ),
            )
        };

        HealthCheckResult::new("ergo_node", status, &message, start.elapsed().as_millis() as u64)
            .with_details(serde_json::json!({
                "node_height": health.node_height,
                "is_synced": health.is_synced,
                "peer_count": health.peer_count,
                "ergo_address": health.ergo_address,
            }))
    }

    /// Check storage rent status.
    ///
    /// Since storage rent runs in a background task, we report based on the
    /// metrics collector. If storage rent monitoring is enabled, the gauge
    /// should be positive. Otherwise we report degraded.
    pub async fn check_storage_rent(
        metrics: &Arc<crate::metrics::MetricsCollector>,
    ) -> HealthCheckResult {
        let start = Instant::now();

        // We use the storage_rent_check_total counter as a proxy.
        // If the agent has performed at least one rent check, the subsystem is alive.
        let checks = metrics.storage_rent_check_total();

        let (status, message) = if checks > 0 {
            (
                HealthStatus::Healthy,
                format!("Rent checks performed: {}", checks),
            )
        } else {
            (
                HealthStatus::Degraded,
                "Storage rent monitoring not yet run".to_string(),
            )
        };

        HealthCheckResult::new("storage_rent", status, &message, start.elapsed().as_millis() as u64)
            .with_details(serde_json::json!({
                "checks_total": checks,
                "auto_topup_total": metrics.storage_rent_auto_topup_total(),
            }))
    }

    /// Check gossip subsystem health.
    pub async fn check_gossip(
        metrics: &Arc<crate::metrics::MetricsCollector>,
    ) -> HealthCheckResult {
        let start = Instant::now();

        let peers = metrics.gossip_peers_connected();
        let sent = metrics.gossip_messages_sent_total();
        let received = metrics.gossip_messages_received_total();

        let (status, message) = if peers > 0 {
            (
                HealthStatus::Healthy,
                format!(
                    "Peers: {}, Sent: {}, Received: {}",
                    peers, sent, received
                ),
            )
        } else if sent > 0 || received > 0 {
            (
                HealthStatus::Degraded,
                format!("No connected peers, but messages sent: {}, received: {}", sent, received),
            )
        } else {
            (
                HealthStatus::Degraded,
                "Gossip subsystem idle (no peers, no messages)".to_string(),
            )
        };

        HealthCheckResult::new("gossip", status, &message, start.elapsed().as_millis() as u64)
            .with_details(serde_json::json!({
                "peers_connected": peers,
                "messages_sent": sent,
                "messages_received": received,
            }))
    }

    /// Check reputation store health.
    pub async fn check_reputation(
        metrics: &Arc<crate::metrics::MetricsCollector>,
    ) -> HealthCheckResult {
        let start = Instant::now();

        let interactions = metrics.reputation_interactions_total();

        let (status, message) = if interactions > 0 {
            (
                HealthStatus::Healthy,
                format!("Reputation interactions tracked: {}", interactions),
            )
        } else {
            (
                HealthStatus::Degraded,
                "No reputation interactions recorded yet".to_string(),
            )
        };

        HealthCheckResult::new("reputation", status, &message, start.elapsed().as_millis() as u64)
            .with_details(serde_json::json!({
                "interactions_total": interactions,
            }))
    }

    /// Check blockchain connectivity.
    pub async fn check_blockchain(
        node_health: &Arc<RwLock<crate::node_health::NodeHealthState>>,
        metrics: &Arc<crate::metrics::MetricsCollector>,
    ) -> HealthCheckResult {
        let start = Instant::now();
        let health = node_health.read().await;

        let height = health.node_height;
        let balance = metrics.box_balance_nanoerg();
        let balance_erg = balance as f64 / 1_000_000_000.0;

        let (status, message) = if health.is_synced {
            (
                HealthStatus::Healthy,
                format!(
                    "Height: {}, Box balance: {:.4} ERG",
                    height, balance_erg
                ),
            )
        } else {
            (
                HealthStatus::Degraded,
                format!(
                    "Height: {} (not synced), Box balance: {:.4} ERG",
                    height, balance_erg
                ),
            )
        };

        HealthCheckResult::new("blockchain", status, &message, start.elapsed().as_millis() as u64)
            .with_details(serde_json::json!({
                "height": height,
                "box_balance_nanoerg": balance,
                "box_balance_erg": balance_erg,
                "is_synced": health.is_synced,
            }))
    }

    /// Run all deep health checks.
    pub async fn check_all(
        node_health: &Arc<RwLock<crate::node_health::NodeHealthState>>,
        metrics: &Arc<crate::metrics::MetricsCollector>,
    ) -> Vec<HealthCheckResult> {
        let mut checks = Vec::with_capacity(5);

        checks.push(Self::check_ergo_node(node_health).await);
        checks.push(Self::check_storage_rent(metrics).await);
        checks.push(Self::check_gossip(metrics).await);
        checks.push(Self::check_reputation(metrics).await);
        checks.push(Self::check_blockchain(node_health, metrics).await);

        checks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_display() {
        assert_eq!(HealthStatus::Healthy.to_string(), "healthy");
        assert_eq!(HealthStatus::Degraded.to_string(), "degraded");
        assert_eq!(HealthStatus::Unhealthy.to_string(), "unhealthy");
    }

    #[test]
    fn test_health_check_result_construction() {
        let r = HealthCheckResult::new("test", HealthStatus::Healthy, "all good", 5);
        assert_eq!(r.component, "test");
        assert_eq!(r.status, HealthStatus::Healthy);
        assert_eq!(r.message, "all good");
        assert_eq!(r.duration_ms, 5);
        assert!(r.details.is_none());
    }

    #[test]
    fn test_health_check_result_with_details() {
        let r = HealthCheckResult::new("test", HealthStatus::Unhealthy, "bad", 10)
            .with_details(serde_json::json!({"key": "value"}));
        assert_eq!(r.status, HealthStatus::Unhealthy);
        assert!(r.details.is_some());
        assert_eq!(r.details.unwrap()["key"], "value");
    }

    #[test]
    fn test_aggregate_status_all_healthy() {
        let checks = vec![
            HealthCheckResult::new("a", HealthStatus::Healthy, "ok", 1),
            HealthCheckResult::new("b", HealthStatus::Healthy, "ok", 1),
        ];
        assert_eq!(aggregate_status(&checks), "healthy");
    }

    #[test]
    fn test_aggregate_status_one_degraded() {
        let checks = vec![
            HealthCheckResult::new("a", HealthStatus::Healthy, "ok", 1),
            HealthCheckResult::new("b", HealthStatus::Degraded, "meh", 1),
        ];
        assert_eq!(aggregate_status(&checks), "degraded");
    }

    #[test]
    fn test_aggregate_status_one_unhealthy() {
        let checks = vec![
            HealthCheckResult::new("a", HealthStatus::Healthy, "ok", 1),
            HealthCheckResult::new("b", HealthStatus::Unhealthy, "bad", 1),
            HealthCheckResult::new("c", HealthStatus::Degraded, "meh", 1),
        ];
        assert_eq!(aggregate_status(&checks), "unhealthy");
    }

    #[test]
    fn test_aggregate_status_empty() {
        let checks: Vec<HealthCheckResult> = vec![];
        assert_eq!(aggregate_status(&checks), "healthy");
    }

    #[tokio::test]
    async fn test_deep_health_returns_all_components() {
        let metrics = Arc::new(crate::metrics::MetricsCollector::new());
        let node_health = Arc::new(RwLock::new(crate::node_health::NodeHealthState {
            best_height_local: 100,
            ergo_address: "test".to_string(),
            is_synced: true,
            last_header_id: None,
            node_height: 100,
            node_id: "test".to_string(),
            peer_count: 5,
            timestamp: chrono::Utc::now().timestamp(),
        }));

        let checks = HealthChecker::check_all(&node_health, &metrics).await;
        assert_eq!(checks.len(), 5);

        let components: Vec<&str> = checks.iter().map(|c| c.component.as_str()).collect();
        assert!(components.contains(&"ergo_node"));
        assert!(components.contains(&"storage_rent"));
        assert!(components.contains(&"gossip"));
        assert!(components.contains(&"reputation"));
        assert!(components.contains(&"blockchain"));
    }
}
