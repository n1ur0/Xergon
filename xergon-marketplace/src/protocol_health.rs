use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use axum::{extract::State, response::Json};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// HealthLevel
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum HealthLevel {
    Healthy,
    Degraded,
    Down,
}

impl HealthLevel {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Healthy => "Healthy",
            Self::Degraded => "Degraded",
            Self::Down => "Down",
        }
    }
}

impl std::fmt::Display for HealthLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ---------------------------------------------------------------------------
// ComponentHealth
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ComponentHealth {
    pub status: HealthLevel,
    pub latency_ms: f64,
    pub error_rate: f64,
    pub last_error: Option<String>,
    pub details: HashMap<String, String>,
}

impl Default for ComponentHealth {
    fn default() -> Self {
        Self {
            status: HealthLevel::Healthy,
            latency_ms: 0.0,
            error_rate: 0.0,
            last_error: None,
            details: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// HealthStatus
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HealthStatus {
    pub overall: HealthLevel,
    pub components: HashMap<String, ComponentHealth>,
    pub last_checked: DateTime<Utc>,
    pub uptime_seconds: u64,
}

// ---------------------------------------------------------------------------
// HealthHistoryEntry
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HealthHistoryEntry {
    pub timestamp: DateTime<Utc>,
    pub overall: HealthLevel,
    pub components: HashMap<String, ComponentHealth>,
}

// ---------------------------------------------------------------------------
// Component identifiers
// ---------------------------------------------------------------------------

pub const COMPONENT_RELAY_NODE: &str = "relay_node";
pub const COMPONENT_BLOCKCHAIN_SYNC: &str = "blockchain_sync";
pub const COMPONENT_STAKING_POOL: &str = "staking_pool";
pub const COMPONENT_ORACLE_FEEDS: &str = "oracle_feeds";
pub const COMPONENT_MARKETPLACE_API: &str = "marketplace_api";
pub const COMPONENT_STORAGE_RENT_MONITOR: &str = "storage_rent_monitor";

const ALL_COMPONENTS: &[&str] = &[
    COMPONENT_RELAY_NODE,
    COMPONENT_BLOCKCHAIN_SYNC,
    COMPONENT_STAKING_POOL,
    COMPONENT_ORACLE_FEEDS,
    COMPONENT_MARKETPLACE_API,
    COMPONENT_STORAGE_RENT_MONITOR,
];

const HISTORY_CAPACITY: usize = 24;

// ---------------------------------------------------------------------------
// Ring buffer for health history
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct HealthRingBuffer {
    entries: DashMap<usize, HealthHistoryEntry>,
    write_index: AtomicU64,
    capacity: usize,
}

impl HealthRingBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            entries: DashMap::new(),
            write_index: AtomicU64::new(0),
            capacity,
        }
    }

    fn push(&self, entry: HealthHistoryEntry) {
        let idx = self.write_index.fetch_add(1, Ordering::Relaxed) as usize % self.capacity;
        self.entries.insert(idx, entry);
    }

    fn read_all(&self) -> Vec<HealthHistoryEntry> {
        let total_written = self.write_index.load(Ordering::Relaxed) as usize;
        let count = total_written.min(self.capacity);
        let start = if total_written > self.capacity {
            total_written % self.capacity
        } else {
            0
        };

        let mut result = Vec::with_capacity(count);
        for i in 0..count {
            let idx = (start + i) % self.capacity;
            if let Some(entry) = self.entries.get(&idx) {
                result.push(entry.clone());
            }
        }
        result
    }

    fn len(&self) -> usize {
        let total = self.write_index.load(Ordering::Relaxed) as usize;
        total.min(self.capacity)
    }
}

// ---------------------------------------------------------------------------
// HealthChecker
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct HealthChecker {
    current_status: DashMap<String, ComponentHealth>,
    history: HealthRingBuffer,
    started_at: Instant,
    check_count: AtomicU64,
    relay_api_url: String,
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthChecker {
    pub fn new() -> Self {
        Self {
            current_status: DashMap::new(),
            history: HealthRingBuffer::new(HISTORY_CAPACITY),
            started_at: Instant::now(),
            check_count: AtomicU64::new(0),
            relay_api_url: "https://ergo-node.xergon.network".to_string(),
        }
    }

    /// Run a full health check cycle and record results.
    pub fn run_health_check(&self) -> HealthStatus {
        let mut components = HashMap::new();

        // Check each component
        components.insert(
            COMPONENT_RELAY_NODE.to_string(),
            self.check_relay_health(),
        );
        components.insert(
            COMPONENT_BLOCKCHAIN_SYNC.to_string(),
            self.check_chain_sync(),
        );
        components.insert(
            COMPONENT_STAKING_POOL.to_string(),
            self.check_staking_pool(),
        );
        components.insert(
            COMPONENT_ORACLE_FEEDS.to_string(),
            self.check_oracle_feeds(),
        );
        components.insert(
            COMPONENT_MARKETPLACE_API.to_string(),
            self.check_marketplace_api(),
        );
        components.insert(
            COMPONENT_STORAGE_RENT_MONITOR.to_string(),
            self.check_storage_rent_monitor(),
        );

        // Update current status
        for (name, health) in &components {
            self.current_status.insert(name.clone(), health.clone());
        }

        // Compute overall health
        let overall = self.compute_overall(&components);

        let uptime_seconds = self.started_at.elapsed().as_secs();

        let status = HealthStatus {
            overall: overall.clone(),
            components: components.clone(),
            last_checked: Utc::now(),
            uptime_seconds,
        };

        // Record history
        self.history.push(HealthHistoryEntry {
            timestamp: Utc::now(),
            overall,
            components,
        });

        self.check_count.fetch_add(1, Ordering::Relaxed);

        status
    }

    /// Get the current cached health status without running a new check.
    pub fn get_current_status(&self) -> HealthStatus {
        let mut components = HashMap::new();
        for entry in self.current_status.iter() {
            components.insert(entry.key().clone(), entry.value().clone());
        }

        let overall = self.compute_overall(&components);

        HealthStatus {
            overall,
            components,
            last_checked: Utc::now(),
            uptime_seconds: self.started_at.elapsed().as_secs(),
        }
    }

    /// Get per-component health.
    pub fn get_components(&self) -> HashMap<String, ComponentHealth> {
        let mut components = HashMap::new();
        for entry in self.current_status.iter() {
            components.insert(entry.key().clone(), entry.value().clone());
        }
        components
    }

    /// Get historical health data (last 24h, hourly).
    pub fn get_history(&self) -> Vec<HealthHistoryEntry> {
        self.history.read_all()
    }

    /// Start a periodic health checker in the background.
    pub fn start_periodic(interval_secs: u64) -> Arc<Self> {
        let checker = Arc::new(Self::new());
        let c = checker.clone();
        tokio::spawn(async move {
            loop {
                c.run_health_check();
                tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
            }
        });
        checker
    }

    // --- Individual component checks ---

    /// Ping relay API and measure latency.
    fn check_relay_health(&self) -> ComponentHealth {
        let start = Instant::now();
        // In production this would make an HTTP request to the relay API.
        // For now, simulate a healthy response.
        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

        ComponentHealth {
            status: HealthLevel::Healthy,
            latency_ms,
            error_rate: 0.0,
            last_error: None,
            details: {
                let mut d = HashMap::new();
                d.insert("url".to_string(), self.relay_api_url.clone());
                d.insert("method".to_string(), "GET /info".to_string());
                d
            },
        }
    }

    /// Check block height vs node.
    fn check_chain_sync(&self) -> ComponentHealth {
        // In production, compare local block height with node-reported height.
        // Simulated: fully synced.
        ComponentHealth {
            status: HealthLevel::Healthy,
            latency_ms: 5.0,
            error_rate: 0.0,
            last_error: None,
            details: {
                let mut d = HashMap::new();
                d.insert("local_height".to_string(), "999000".to_string());
                d.insert("node_height".to_string(), "999000".to_string());
                d.insert("blocks_behind".to_string(), "0".to_string());
                d
            },
        }
    }

    /// Verify staking pool box exists and has sufficient stake.
    fn check_staking_pool(&self) -> ComponentHealth {
        // In production, query the Ergo node for the staking pool box.
        // Simulated: pool box found with sufficient stake.
        ComponentHealth {
            status: HealthLevel::Healthy,
            latency_ms: 12.0,
            error_rate: 0.0,
            last_error: None,
            details: {
                let mut d = HashMap::new();
                d.insert("pool_box_found".to_string(), "true".to_string());
                d.insert("stake_nanoerg".to_string(), "50000000000".to_string());
                d.insert("min_required".to_string(), "10000000000".to_string());
                d
            },
        }
    }

    /// Verify oracle box values are fresh (< 100 blocks old).
    fn check_oracle_feeds(&self) -> ComponentHealth {
        // In production, fetch oracle boxes and check creation height.
        // Simulated: oracles fresh.
        ComponentHealth {
            status: HealthLevel::Healthy,
            latency_ms: 8.0,
            error_rate: 0.0,
            last_error: None,
            details: {
                let mut d = HashMap::new();
                d.insert("oracle_count".to_string(), "3".to_string());
                d.insert("max_age_blocks".to_string(), "45".to_string());
                d.insert("threshold_blocks".to_string(), "100".to_string());
                d
            },
        }
    }

    /// Check marketplace API responsiveness.
    fn check_marketplace_api(&self) -> ComponentHealth {
        let start = Instant::now();
        // In production, call a lightweight marketplace endpoint.
        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

        ComponentHealth {
            status: HealthLevel::Healthy,
            latency_ms,
            error_rate: 0.0,
            last_error: None,
            details: {
                let mut d = HashMap::new();
                d.insert("endpoint".to_string(), "/v1/health".to_string());
                d.insert("status_code".to_string(), "200".to_string());
                d
            },
        }
    }

    /// Check storage rent monitor status.
    fn check_storage_rent_monitor(&self) -> ComponentHealth {
        // In production, verify rent payment tracking is active.
        ComponentHealth {
            status: HealthLevel::Healthy,
            latency_ms: 3.0,
            error_rate: 0.0,
            last_error: None,
            details: {
                let mut d = HashMap::new();
                d.insert("monitored_boxes".to_string(), "42".to_string());
                d.insert("overdue_rent".to_string(), "0".to_string());
                d.insert("next_payment_height".to_string(), "999050".to_string());
                d
            },
        }
    }

    /// Compute overall health from component statuses.
    fn compute_overall(&self, components: &HashMap<String, ComponentHealth>) -> HealthLevel {
        if components.is_empty() {
            return HealthLevel::Down;
        }

        let mut has_down = false;
        let mut has_degraded = false;

        for health in components.values() {
            match health.status {
                HealthLevel::Down => has_down = true,
                HealthLevel::Degraded => has_degraded = true,
                HealthLevel::Healthy => {}
            }
        }

        if has_down {
            HealthLevel::Down
        } else if has_degraded {
            HealthLevel::Degraded
        } else {
            HealthLevel::Healthy
        }
    }

    /// Simulate a degraded component (for testing).
    pub fn simulate_degraded(&self, component: &str, error_msg: &str) {
        let mut health = ComponentHealth::default();
        health.status = HealthLevel::Degraded;
        health.latency_ms = 500.0;
        health.error_rate = 0.15;
        health.last_error = Some(error_msg.to_string());
        health.details.insert("simulated".to_string(), "true".to_string());
        self.current_status.insert(component.to_string(), health);
    }

    /// Simulate a down component (for testing).
    pub fn simulate_down(&self, component: &str, error_msg: &str) {
        let mut health = ComponentHealth::default();
        health.status = HealthLevel::Down;
        health.latency_ms = f64::INFINITY;
        health.error_rate = 1.0;
        health.last_error = Some(error_msg.to_string());
        health.details.insert("simulated".to_string(), "true".to_string());
        self.current_status.insert(component.to_string(), health);
    }

    /// Reset a component to healthy status.
    pub fn reset_component(&self, component: &str) {
        self.current_status.remove(component);
    }
}

// ---------------------------------------------------------------------------
// REST Handlers
// ---------------------------------------------------------------------------

pub async fn health_overall_handler(
    State(state): State<super::proxy::AppState>,
) -> Json<serde_json::Value> {
    let status = state.health_checker.run_health_check();
    Json(serde_json::to_value(status).unwrap_or_default())
}

pub async fn health_components_handler(
    State(state): State<super::proxy::AppState>,
) -> Json<serde_json::Value> {
    let components = state.health_checker.get_components();
    Json(serde_json::json!({
        "components": components,
        "total": components.len(),
    }))
}

pub async fn health_history_handler(
    State(state): State<super::proxy::AppState>,
) -> Json<serde_json::Value> {
    let history = state.health_checker.get_history();
    Json(serde_json::json!({
        "history": history,
        "count": history.len(),
        "period_hours": 24,
    }))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_checker() -> HealthChecker {
        HealthChecker::new()
    }

    #[test]
    fn test_health_level_display() {
        assert_eq!(HealthLevel::Healthy.as_str(), "Healthy");
        assert_eq!(HealthLevel::Degraded.as_str(), "Degraded");
        assert_eq!(HealthLevel::Down.as_str(), "Down");
        assert_eq!(format!("{}", HealthLevel::Healthy), "Healthy");
    }

    #[test]
    fn test_component_health_default() {
        let ch = ComponentHealth::default();
        assert_eq!(ch.status, HealthLevel::Healthy);
        assert_eq!(ch.latency_ms, 0.0);
        assert_eq!(ch.error_rate, 0.0);
        assert!(ch.last_error.is_none());
        assert!(ch.details.is_empty());
    }

    #[test]
    fn test_run_health_check_returns_all_components() {
        let checker = make_checker();
        let status = checker.run_health_check();
        assert_eq!(status.components.len(), ALL_COMPONENTS.len());
        for comp in ALL_COMPONENTS {
            assert!(
                status.components.contains_key(*comp),
                "Missing component: {}",
                comp
            );
        }
    }

    #[test]
    fn test_run_health_check_overall_healthy() {
        let checker = make_checker();
        let status = checker.run_health_check();
        assert_eq!(status.overall, HealthLevel::Healthy);
        assert!(status.uptime_seconds >= 0);
    }

    #[test]
    fn test_run_health_check_records_history() {
        let checker = make_checker();
        checker.run_health_check();
        checker.run_health_check();
        let history = checker.get_history();
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn test_history_ring_buffer_capacity() {
        let checker = make_checker();
        for _ in 0..30 {
            checker.run_health_check();
        }
        let history = checker.get_history();
        assert_eq!(history.len(), HISTORY_CAPACITY);
    }

    #[test]
    fn test_simulate_degraded() {
        let checker = make_checker();
        checker.run_health_check();
        checker.simulate_degraded(COMPONENT_RELAY_NODE, "High latency detected");
        let status = checker.get_current_status();
        assert_eq!(status.overall, HealthLevel::Degraded);
        assert_eq!(
            status.components[COMPONENT_RELAY_NODE].status,
            HealthLevel::Degraded
        );
        assert_eq!(
            status.components[COMPONENT_RELAY_NODE].last_error.as_deref(),
            Some("High latency detected")
        );
    }

    #[test]
    fn test_simulate_down() {
        let checker = make_checker();
        checker.run_health_check();
        checker.simulate_down(COMPONENT_BLOCKCHAIN_SYNC, "Connection refused");
        let status = checker.get_current_status();
        assert_eq!(status.overall, HealthLevel::Down);
        assert_eq!(
            status.components[COMPONENT_BLOCKCHAIN_SYNC].status,
            HealthLevel::Down
        );
    }

    #[test]
    fn test_reset_component() {
        let checker = make_checker();
        checker.run_health_check();
        checker.simulate_down(COMPONENT_ORACLE_FEEDS, "Timeout");
        let status_before = checker.get_current_status();
        assert_eq!(
            status_before.components[COMPONENT_ORACLE_FEEDS].status,
            HealthLevel::Down
        );

        checker.reset_component(COMPONENT_ORACLE_FEEDS);
        // After reset, component is removed from current_status.
        // get_current_status rebuilds from remaining entries only.
        let status_after = checker.get_current_status();
        assert!(!status_after.components.contains_key(COMPONENT_ORACLE_FEEDS));
    }

    #[test]
    fn test_compute_overall_empty() {
        let checker = make_checker();
        let overall = checker.compute_overall(&HashMap::new());
        assert_eq!(overall, HealthLevel::Down);
    }

    #[test]
    fn test_get_components() {
        let checker = make_checker();
        checker.run_health_check();
        let components = checker.get_components();
        assert_eq!(components.len(), ALL_COMPONENTS.len());
    }

    #[test]
    fn test_check_count_increments() {
        let checker = make_checker();
        assert_eq!(checker.check_count.load(Ordering::Relaxed), 0);
        checker.run_health_check();
        assert_eq!(checker.check_count.load(Ordering::Relaxed), 1);
        checker.run_health_check();
        checker.run_health_check();
        assert_eq!(checker.check_count.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let checker = make_checker();
        let status = checker.run_health_check();
        let json = serde_json::to_value(&status).unwrap();
        let deserialized: HealthStatus = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.overall, status.overall);
        assert_eq!(deserialized.components.len(), status.components.len());
    }

    #[test]
    fn test_health_history_entry_serialization() {
        let entry = HealthHistoryEntry {
            timestamp: Utc::now(),
            overall: HealthLevel::Healthy,
            components: HashMap::new(),
        };
        let json = serde_json::to_value(&entry).unwrap();
        let deserialized: HealthHistoryEntry = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.overall, HealthLevel::Healthy);
    }
}
