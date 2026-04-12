//! Enhanced connection pooling with health tracking, circuit-breaker integration,
//! adaptive sizing, and connection prewarming.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Pooled connection
// ---------------------------------------------------------------------------

/// A single tracked connection in the pool.
#[derive(Debug, Clone)]
pub struct PooledConnection {
    /// Unique ID for this connection instance.
    pub id: String,
    /// Provider this connection targets.
    pub provider_id: String,
    /// Endpoint URL.
    pub endpoint: String,
    /// When this connection was created.
    pub created_at: Instant,
    /// When this connection was last used.
    pub last_used: Instant,
    /// Total number of requests served over this connection.
    pub request_count: u64,
    /// Number of consecutive errors observed.
    pub error_count: u64,
    /// Current health score (0.0 = dead, 1.0 = perfect).
    pub health_score: f64,
    /// Whether this connection is currently checked out / in use.
    pub in_use: bool,
    /// Pool group this connection belongs to.
    pub pool_id: String,
}

impl PooledConnection {
    /// Create a new pooled connection.
    pub fn new(provider_id: &str, endpoint: &str, pool_id: &str) -> Self {
        let now = Instant::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            provider_id: provider_id.to_string(),
            endpoint: endpoint.to_string(),
            created_at: now,
            last_used: now,
            request_count: 0,
            error_count: 0,
            health_score: 1.0,
            in_use: false,
            pool_id: pool_id.to_string(),
        }
    }

    /// Record a successful request.
    pub fn record_success(&mut self) {
        self.request_count += 1;
        self.error_count = 0; // reset on success
        self.health_score = (self.health_score + 0.05).min(1.0);
        self.last_used = Instant::now();
    }

    /// Record a failed request.
    pub fn record_error(&mut self) {
        self.request_count += 1;
        self.error_count += 1;
        self.health_score = (self.health_score - 0.15).max(0.0);
        self.last_used = Instant::now();
    }

    /// Check if this connection is expired by lifetime.
    pub fn is_expired(&self, max_lifetime: Duration) -> bool {
        self.created_at.elapsed() > max_lifetime
    }

    /// Check if this connection has been idle too long.
    pub fn is_idle(&self, idle_timeout: Duration) -> bool {
        !self.in_use && self.last_used.elapsed() > idle_timeout
    }

    /// Check if the connection is unhealthy (circuit-breaker threshold).
    pub fn is_unhealthy(&self) -> bool {
        self.health_score < 0.2 || self.error_count >= 5
    }
}

// ---------------------------------------------------------------------------
// Pool configuration
// ---------------------------------------------------------------------------

/// Configuration for the connection pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    /// Maximum connections per provider.
    pub max_connections_per_provider: usize,
    /// Idle timeout before a connection is pruned.
    pub idle_timeout_secs: u64,
    /// Maximum lifetime of a connection.
    pub max_lifetime_secs: u64,
    /// Health check interval in seconds.
    pub health_check_interval_secs: u64,
    /// Number of connections to prewarm per provider.
    pub prewarm_count: usize,
    /// Whether adaptive pool sizing is enabled.
    pub adaptive_sizing: bool,
    /// Minimum pool size per provider when adaptive sizing is on.
    pub min_pool_size: usize,
    /// Maximum pool size per provider when adaptive sizing is on.
    pub max_pool_size: usize,
    /// Circuit-breaker error threshold (consecutive errors before trip).
    pub circuit_breaker_threshold: u64,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections_per_provider: 10,
            idle_timeout_secs: 300,
            max_lifetime_secs: 3600,
            health_check_interval_secs: 60,
            prewarm_count: 2,
            adaptive_sizing: false,
            min_pool_size: 2,
            max_pool_size: 20,
            circuit_breaker_threshold: 5,
        }
    }
}

// ---------------------------------------------------------------------------
// Provider pool statistics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPoolStats {
    pub provider_id: String,
    pub total_connections: usize,
    pub active_connections: usize,
    pub idle_connections: usize,
    pub avg_health_score: f64,
    pub total_requests: u64,
    pub total_errors: u64,
}

// ---------------------------------------------------------------------------
// Global pool statistics
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStats {
    pub total_providers: usize,
    pub total_connections: usize,
    pub active_connections: usize,
    pub idle_connections: usize,
    pub avg_health_score: f64,
    pub total_requests: u64,
    pub total_errors: u64,
    pub evicted_count: u64,
    pub pruned_idle_count: u64,
}

// ---------------------------------------------------------------------------
// Connection pool v2
// ---------------------------------------------------------------------------

/// Enhanced connection pool with per-provider connection groups, health tracking,
/// circuit-breaker integration, and adaptive sizing.
pub struct ConnectionPoolV2 {
    /// connection_id -> PooledConnection
    connections: DashMap<String, PooledConnection>,
    /// provider_id -> Vec<connection_id>
    provider_connections: DashMap<String, Vec<String>>,
    /// Configuration.
    config: PoolConfig,
    /// Total number of connections evicted (for stats).
    evicted_count: AtomicU64,
    /// Total number of idle connections pruned.
    pruned_idle_count: AtomicU64,
    /// Per-provider demand tracker for adaptive sizing.
    demand_counts: DashMap<String, AtomicU64>,
}

impl std::fmt::Debug for ConnectionPoolV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionPoolV2")
            .field("connections", &self.connections.len())
            .field("providers", &self.provider_connections.len())
            .finish()
    }
}

impl ConnectionPoolV2 {
    /// Create a new pool with the given configuration.
    pub fn new(config: PoolConfig) -> Self {
        info!(
            max_per_provider = config.max_connections_per_provider,
            idle_timeout_secs = config.idle_timeout_secs,
            max_lifetime_secs = config.max_lifetime_secs,
            "ConnectionPoolV2 initialized"
        );
        Self {
            connections: DashMap::new(),
            provider_connections: DashMap::new(),
            config,
            evicted_count: AtomicU64::new(0),
            pruned_idle_count: AtomicU64::new(0),
            demand_counts: DashMap::new(),
        }
    }

    /// Get or create a connection for the given provider.
    /// Returns None if the pool is at capacity for this provider.
    pub fn get_connection(&self, provider_id: &str, endpoint: &str) -> Option<PooledConnection> {
        self.demand_counts
            .entry(provider_id.to_string())
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);

        // Try to find an existing idle connection
        if let Some(mut conn_ids) = self.provider_connections.get_mut(provider_id) {
            for conn_id in conn_ids.value_mut().iter() {
                if let Some(mut conn) = self.connections.get_mut(conn_id) {
                    let c = conn.value_mut();
                    if !c.in_use && !c.is_unhealthy() {
                        c.in_use = true;
                        c.last_used = Instant::now();
                        debug!(
                            conn_id = %c.id,
                            provider_id = %provider_id,
                            "Reusing existing connection"
                        );
                        return Some(c.clone());
                    }
                }
            }
        }

        // Check capacity
        let max = if self.config.adaptive_sizing {
            self.adaptive_max_for_provider(provider_id)
        } else {
            self.config.max_connections_per_provider
        };

        let current_count = self
            .provider_connections
            .get(provider_id)
            .map(|e| e.value().len())
            .unwrap_or(0);

        if current_count >= max {
            warn!(
                provider_id = %provider_id,
                current = current_count,
                max = max,
                "Pool at capacity for provider"
            );
            return None;
        }

        // Create new connection
        let conn = PooledConnection::new(provider_id, endpoint, provider_id);
        let conn_id = conn.id.clone();

        self.connections.insert(conn_id.clone(), conn.clone());

        self.provider_connections
            .entry(provider_id.to_string())
            .or_insert_with(Vec::new)
            .value_mut()
            .push(conn_id.clone());

        debug!(
            conn_id = %conn_id,
            provider_id = %provider_id,
            "Created new pooled connection"
        );
        Some(conn)
    }

    /// Return a connection to the pool (mark as no longer in use).
    pub fn return_connection(&self, conn_id: &str, success: bool) {
        if let Some(mut conn) = self.connections.get_mut(conn_id) {
            let c = conn.value_mut();
            if success {
                c.record_success();
            } else {
                c.record_error();
            }
            c.in_use = false;
            debug!(
                conn_id = %conn_id,
                success = success,
                health = c.health_score,
                "Returned connection to pool"
            );
        }
    }

    /// Run health checks on all connections, removing unhealthy ones.
    /// Returns the number of connections removed.
    pub fn health_check(&self) -> usize {
        let mut removed = 0;
        let max_lifetime = Duration::from_secs(self.config.max_lifetime_secs);

        let unhealthy_ids: Vec<String> = self
            .connections
            .iter()
            .filter(|e| {
                let c = e.value();
                c.is_unhealthy() || c.is_expired(max_lifetime)
            })
            .map(|e| e.key().clone())
            .collect();

        for conn_id in &unhealthy_ids {
            if let Some((conn_id, conn)) = self.connections.remove(conn_id) {
                self.remove_from_provider_map(&conn.provider_id, &conn_id);
                removed += 1;
            }
        }

        if removed > 0 {
            info!(removed = removed, "Health check removed unhealthy connections");
        }
        removed
    }

    /// Prune idle connections that have exceeded the idle timeout.
    /// Returns the number of connections pruned.
    pub fn prune_idle(&self) -> usize {
        let idle_timeout = Duration::from_secs(self.config.idle_timeout_secs);
        let mut pruned = 0;

        let idle_ids: Vec<String> = self
            .connections
            .iter()
            .filter(|e| e.value().is_idle(idle_timeout))
            .map(|e| e.key().clone())
            .collect();

        for conn_id in &idle_ids {
            if let Some((conn_id, conn)) = self.connections.remove(conn_id) {
                self.remove_from_provider_map(&conn.provider_id, &conn_id);
                pruned += 1;
            }
        }

        self.pruned_idle_count.fetch_add(pruned as u64, Ordering::Relaxed);
        if pruned > 0 {
            debug!(pruned = pruned, "Pruned idle connections");
        }
        pruned
    }

    /// Get overall pool statistics.
    pub fn stats(&self) -> PoolStats {
        let mut total_connections = 0usize;
        let mut active_connections = 0usize;
        let mut idle_connections = 0usize;
        let mut health_sum = 0.0f64;
        let mut total_requests = 0u64;
        let mut total_errors = 0u64;

        for entry in self.connections.iter() {
            let c = entry.value();
            total_connections += 1;
            if c.in_use {
                active_connections += 1;
            } else {
                idle_connections += 1;
            }
            health_sum += c.health_score;
            total_requests += c.request_count;
            total_errors += c.error_count;
        }

        let avg_health = if total_connections > 0 {
            health_sum / total_connections as f64
        } else {
            0.0
        };

        PoolStats {
            total_providers: self.provider_connections.len(),
            total_connections,
            active_connections,
            idle_connections,
            avg_health_score: avg_health,
            total_requests,
            total_errors,
            evicted_count: self.evicted_count.load(Ordering::Relaxed),
            pruned_idle_count: self.pruned_idle_count.load(Ordering::Relaxed),
        }
    }

    /// Get statistics for a specific provider.
    pub fn provider_stats(&self, provider_id: &str) -> Option<ProviderPoolStats> {
        let conn_ids = self.provider_connections.get(provider_id)?;

        let mut total = 0usize;
        let mut active = 0usize;
        let mut idle = 0usize;
        let mut health_sum = 0.0f64;
        let mut requests = 0u64;
        let mut errors = 0u64;

        for conn_id in conn_ids.value().iter() {
            if let Some(conn) = self.connections.get(conn_id) {
                let c = conn.value();
                total += 1;
                if c.in_use {
                    active += 1;
                } else {
                    idle += 1;
                }
                health_sum += c.health_score;
                requests += c.request_count;
                errors += c.error_count;
            }
        }

        let avg_health = if total > 0 {
            health_sum / total as f64
        } else {
            0.0
        };

        Some(ProviderPoolStats {
            provider_id: provider_id.to_string(),
            total_connections: total,
            active_connections: active,
            idle_connections: idle,
            avg_health_score: avg_health,
            total_requests: requests,
            total_errors: errors,
        })
    }

    /// Evict all connections for a specific provider.
    pub fn evict_provider(&self, provider_id: &str) -> usize {
        let conn_ids = match self.provider_connections.remove(provider_id) {
            Some((_, ids)) => ids,
            None => return 0,
        };

        let count = conn_ids.len();
        for conn_id in conn_ids {
            self.connections.remove(&conn_id);
        }
        self.evicted_count.fetch_add(count as u64, Ordering::Relaxed);
        info!(
            provider_id = %provider_id,
            evicted = count,
            "Evicted all connections for provider"
        );
        count
    }

    /// Prewarm the pool for a provider by creating the configured number of connections.
    pub fn prewarm(&self, provider_id: &str, endpoint: &str) -> usize {
        // First evict any existing connections for clean prewarm
        self.evict_provider(provider_id);

        let count = self.config.prewarm_count;
        for _ in 0..count {
            let conn = PooledConnection::new(provider_id, endpoint, provider_id);
            let conn_id = conn.id.clone();
            self.connections.insert(conn_id.clone(), conn);
            self.provider_connections
                .entry(provider_id.to_string())
                .or_insert_with(Vec::new)
                .value_mut()
                .push(conn_id);
        }

        info!(
            provider_id = %provider_id,
            prewarmed = count,
            "Prewarmed connection pool"
        );
        count
    }

    /// Get the pool configuration.
    pub fn config(&self) -> &PoolConfig {
        &self.config
    }

    /// Get a list of all providers with active connections.
    pub fn list_providers(&self) -> Vec<String> {
        self.provider_connections
            .iter()
            .map(|e| e.key().clone())
            .collect()
    }

    // -- internal helpers --

    fn remove_from_provider_map(&self, provider_id: &str, conn_id: &str) {
        if let Some(mut entry) = self.provider_connections.get_mut(provider_id) {
            entry.value_mut().retain(|id| id != conn_id);
            if entry.value().is_empty() {
                drop(entry);
                self.provider_connections.remove(provider_id);
            }
        }
    }

    fn adaptive_max_for_provider(&self, provider_id: &str) -> usize {
        let demand = self
            .demand_counts
            .get(provider_id)
            .map(|d| d.load(Ordering::Relaxed))
            .unwrap_or(0);

        let adaptive_size = (demand as usize / 10)
            .clamp(self.config.min_pool_size, self.config.max_pool_size);

        adaptive_size.min(self.config.max_connections_per_provider)
    }

    /// Reset demand counters (called periodically to decay demand).
    pub fn decay_demand(&self) {
        for entry in self.demand_counts.iter() {
            let current = entry.value().load(Ordering::Relaxed);
            entry.value().store(current / 2, Ordering::Relaxed);
        }
    }
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

/// GET /v1/pool/stats — global pool statistics
async fn stats_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let stats = state.connection_pool_v2.stats();
    (StatusCode::OK, Json(stats))
}

/// GET /v1/pool/providers — list all providers with active connections
async fn providers_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let providers = state.connection_pool_v2.list_providers();
    (StatusCode::OK, Json(providers))
}

/// POST /v1/pool/prewarm/{provider_id} — prewarm connections for a provider
async fn prewarm_handler(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> axum::response::Response {
    let endpoint = body.get("endpoint")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if endpoint.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "endpoint is required"})),
        ).into_response();
    }

    let count = state.connection_pool_v2.prewarm(&provider_id, endpoint);
    (StatusCode::OK, Json(serde_json::json!({
        "provider_id": provider_id,
        "prewarmed": count,
    }))).into_response()
}

/// DELETE /v1/pool/evict/{provider_id} — evict all connections for a provider
async fn evict_handler(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> impl IntoResponse {
    let evicted = state.connection_pool_v2.evict_provider(&provider_id);
    (StatusCode::OK, Json(serde_json::json!({
        "provider_id": provider_id,
        "evicted": evicted,
    })))
}

/// POST /v1/pool/health-check — trigger health check
async fn health_check_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let removed = state.connection_pool_v2.health_check();
    (StatusCode::OK, Json(serde_json::json!({
        "removed": removed,
    })))
}

/// GET /v1/pool/config — get pool configuration
async fn config_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let config = state.connection_pool_v2.config().clone();
    (StatusCode::OK, Json(config))
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/v1/pool/stats", get(stats_handler))
        .route("/v1/pool/providers", get(providers_handler))
        .route("/v1/pool/prewarm/{provider_id}", post(prewarm_handler))
        .route("/v1/pool/evict/{provider_id}", delete(evict_handler))
        .route("/v1/pool/health-check", post(health_check_handler))
        .route("/v1/pool/config", get(config_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pool() -> ConnectionPoolV2 {
        ConnectionPoolV2::new(PoolConfig {
            max_connections_per_provider: 5,
            idle_timeout_secs: 60,
            max_lifetime_secs: 300,
            health_check_interval_secs: 10,
            prewarm_count: 2,
            adaptive_sizing: false,
            min_pool_size: 1,
            max_pool_size: 10,
            circuit_breaker_threshold: 5,
        })
    }

    #[test]
    fn test_pool_get_and_return() {
        let pool = make_pool();
        let conn = pool.get_connection("prov-1", "https://prov-1.test").unwrap();
        assert!(conn.in_use);
        assert_eq!(conn.request_count, 0);
        assert_eq!(conn.health_score, 1.0);

        pool.return_connection(&conn.id, true);
        let stats = pool.stats();
        assert_eq!(stats.total_connections, 1);
        assert_eq!(stats.active_connections, 0);
        assert_eq!(stats.idle_connections, 1);
        assert_eq!(stats.total_requests, 1);
    }

    #[test]
    fn test_pool_reuses_connection() {
        let pool = make_pool();
        let conn1 = pool.get_connection("prov-1", "https://prov-1.test").unwrap();
        pool.return_connection(&conn1.id, true);

        let conn2 = pool.get_connection("prov-1", "https://prov-1.test").unwrap();
        assert_eq!(conn1.id, conn2.id); // should reuse
    }

    #[test]
    fn test_max_connections() {
        let pool = make_pool();
        let mut conns = Vec::new();
        for _ in 0..5 {
            let conn = pool.get_connection("prov-1", "https://prov-1.test").unwrap();
            conns.push(conn);
        }
        // At capacity, should return None
        let overflow = pool.get_connection("prov-1", "https://prov-1.test");
        assert!(overflow.is_none());

        // Return one, should be able to get again
        pool.return_connection(&conns[0].id, true);
        let reuse = pool.get_connection("prov-1", "https://prov-1.test");
        assert!(reuse.is_some());
    }

    #[test]
    fn test_health_check_removes_unhealthy() {
        let pool = make_pool();
        let conn = pool.get_connection("prov-1", "https://prov-1.test").unwrap();

        // Simulate 5 errors (threshold)
        for _ in 0..5 {
            pool.return_connection(&conn.id, false);
            // Re-acquire to record next error
            if pool.get_connection("prov-1", "https://prov-1.test").is_none() {
                break;
            }
        }

        let removed = pool.health_check();
        // Connection should be removed due to high error count
        assert!(removed >= 1);
    }

    #[test]
    fn test_prewarm() {
        let pool = make_pool();
        let count = pool.prewarm("prov-x", "https://prov-x.test");
        assert_eq!(count, 2);

        let stats = pool.provider_stats("prov-x").unwrap();
        assert_eq!(stats.total_connections, 2);
        assert_eq!(stats.idle_connections, 2);
    }

    #[test]
    fn test_evict_provider() {
        let pool = make_pool();
        pool.prewarm("prov-y", "https://prov-y.test");
        pool.get_connection("prov-y", "https://prov-y.test");

        let evicted = pool.evict_provider("prov-y");
        assert_eq!(evicted, 3); // 2 prewarmed + 1 new
        assert!(pool.provider_stats("prov-y").is_none());
    }

    #[test]
    fn test_stats() {
        let pool = make_pool();
        pool.prewarm("prov-a", "https://prov-a.test");
        pool.prewarm("prov-b", "https://prov-b.test");

        let stats = pool.stats();
        assert_eq!(stats.total_providers, 2);
        assert_eq!(stats.total_connections, 4);
        assert_eq!(stats.active_connections, 0);
        assert_eq!(stats.idle_connections, 4);
    }

    #[test]
    fn test_error_recording() {
        let pool = make_pool();
        let conn = pool.get_connection("prov-1", "https://prov-1.test").unwrap();
        pool.return_connection(&conn.id, false);

        let fetched = pool.connections.get(&conn.id).unwrap();
        assert_eq!(fetched.value().error_count, 1);
        assert!(fetched.value().health_score < 1.0);
    }

    #[test]
    fn test_success_increments_request_count() {
        let pool = make_pool();
        let conn = pool.get_connection("prov-1", "https://prov-1.test").unwrap();
        pool.return_connection(&conn.id, true);
        pool.get_connection("prov-1", "https://prov-1.test").unwrap();
        pool.return_connection(&conn.id, true);

        let fetched = pool.connections.get(&conn.id).unwrap();
        assert_eq!(fetched.value().request_count, 3); // 2 returns + 1 get
    }

    #[test]
    fn test_idle_timeout_prune() {
        let pool = ConnectionPoolV2::new(PoolConfig {
            idle_timeout_secs: 0, // immediate
            ..PoolConfig::default()
        });
        pool.prewarm("prov-z", "https://prov-z.test");
        let pruned = pool.prune_idle();
        assert!(pruned >= 1);
    }

    #[test]
    fn test_provider_stats() {
        let pool = make_pool();
        pool.prewarm("prov-s", "https://prov-s.test");

        let stats = pool.provider_stats("prov-s").unwrap();
        assert_eq!(stats.provider_id, "prov-s");
        assert_eq!(stats.total_connections, 2);

        // Non-existent provider
        assert!(pool.provider_stats("nonexistent").is_none());
    }
}
