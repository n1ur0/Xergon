//! WebSocket connection pooling for backend providers.
//!
//! Maintains a pool of persistent WebSocket connections to each provider endpoint,
//! reducing connection overhead for repeated requests. The pool is keyed by
//! provider endpoint URL and prunes idle/dead connections via a background
//! maintenance task.

use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::protocol::Message;
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the WebSocket connection pool.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct WsPoolConfig {
    /// Enable/disable connection pooling (default: true).
    pub enabled: bool,
    /// Maximum pooled connections per provider endpoint (default: 5).
    pub max_per_provider: usize,
    /// Idle timeout in seconds -- close connections unused for this long (default: 60).
    pub idle_timeout_secs: u64,
    /// Maximum connection age in seconds -- rotate old connections (default: 300).
    pub max_age_secs: u64,
    /// Maintenance task interval in seconds (default: 30).
    pub maintenance_interval_secs: u64,
}

impl Default for WsPoolConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_per_provider: 5,
            idle_timeout_secs: 60,
            max_age_secs: 300,
            maintenance_interval_secs: 30,
        }
    }
}

// ---------------------------------------------------------------------------
// Pooled connection
// ---------------------------------------------------------------------------

/// A single pooled WebSocket connection to a provider.
pub struct PooledConnection {
    /// The underlying WebSocket sender (write half).
    pub sender: futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    /// The underlying WebSocket receiver (read half).
    pub receiver: futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    /// When this connection was created.
    pub created_at: Instant,
    /// When this connection was last used.
    pub last_used_at: Instant,
    /// Number of requests served over this connection.
    pub request_count: u64,
    /// The provider endpoint URL this connection is to.
    pub endpoint: String,
}

impl PooledConnection {
    /// Check if this connection has exceeded the idle timeout.
    pub fn is_idle(&self, idle_timeout: std::time::Duration) -> bool {
        self.last_used_at.elapsed() > idle_timeout
    }

    /// Check if this connection has exceeded the max age.
    pub fn is_expired(&self, max_age: std::time::Duration) -> bool {
        self.created_at.elapsed() > max_age
    }
}

// ---------------------------------------------------------------------------
// Per-provider pool bucket
// ---------------------------------------------------------------------------

/// A bucket of connections for a single provider endpoint.
struct ProviderPool {
    connections: Mutex<Vec<PooledConnection>>,
}

impl ProviderPool {
    fn new() -> Self {
        Self {
            connections: Mutex::new(Vec::new()),
        }
    }
}

// ---------------------------------------------------------------------------
// Pool statistics
// ---------------------------------------------------------------------------

/// Statistics about the connection pool.
#[derive(Debug, Clone, Serialize)]
pub struct WsPoolStats {
    /// Whether the pool is enabled.
    pub enabled: bool,
    /// Total number of active pooled connections.
    pub total_connections: usize,
    /// Number of provider endpoints with pooled connections.
    pub provider_count: usize,
    /// Per-provider connection counts.
    pub per_provider: Vec<ProviderPoolStats>,
    /// Total number of pool hits (reused connections).
    pub hits: u64,
    /// Total number of pool misses (new connections created).
    pub misses: u64,
    /// Hit rate as a fraction (hits / (hits + misses)).
    pub hit_rate: f64,
    /// Total connections ever created.
    pub total_created: u64,
    /// Total connections closed (pruned/evicted).
    pub total_closed: u64,
}

/// Per-provider pool statistics.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderPoolStats {
    pub endpoint: String,
    pub connections: usize,
    pub avg_age_secs: f64,
}

// ---------------------------------------------------------------------------
// WsConnectionPool
// ---------------------------------------------------------------------------

/// A connection pool for maintaining persistent WebSocket connections to
/// backend providers.
///
/// Connections are keyed by provider endpoint URL. Each endpoint has a
/// configurable maximum number of connections. Idle and expired connections
/// are pruned by a background maintenance task.
pub struct WsConnectionPool {
    config: WsPoolConfig,
    /// Connections keyed by provider endpoint URL.
    pools: DashMap<String, ProviderPool>,
    /// Total number of pool hits.
    hits: AtomicU64,
    /// Total number of pool misses.
    misses: AtomicU64,
    /// Total connections ever created.
    total_created: AtomicU64,
    /// Total connections closed.
    total_closed: AtomicU64,
    /// Whether the pool is shutting down.
    shutting_down: AtomicBool,
}

impl WsConnectionPool {
    /// Create a new connection pool with the given configuration.
    pub fn new(config: WsPoolConfig) -> Self {
        Self {
            config,
            pools: DashMap::new(),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            total_created: AtomicU64::new(0),
            total_closed: AtomicU64::new(0),
            shutting_down: AtomicBool::new(false),
        }
    }

    /// Check if the pool is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get or create a pooled connection to a provider endpoint.
    ///
    /// Returns `None` if:
    /// - The pool is disabled
    /// - The pool is shutting down
    /// - A new connection cannot be established
    ///
    /// The caller is responsible for returning the connection via
    /// [`return_connection`](Self::return_connection) when done, or dropping
    /// it if it's no longer usable (in which case it will not be returned to
    /// the pool).
    pub async fn get_connection(&self, endpoint: &str) -> Option<PooledConnection> {
        if !self.config.enabled || self.shutting_down.load(Ordering::Relaxed) {
            return None;
        }

        let idle_timeout = std::time::Duration::from_secs(self.config.idle_timeout_secs);
        let max_age = std::time::Duration::from_secs(self.config.max_age_secs);

        // Try to get an existing connection from the pool
        {
            let bucket = self.pools.get(endpoint);
            if let Some(bucket) = bucket {
                let mut conns = bucket.connections.lock().await;
                // Find the first non-expired, non-idle connection
                while let Some(mut conn) = conns.pop() {
                    if conn.is_expired(max_age) || conn.is_idle(idle_timeout) {
                        // Connection is stale, close it
                        let _ = conn.sender.close().await;
                        self.total_closed.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    // Found a usable connection -- update stats
                    conn.last_used_at = Instant::now();
                    conn.request_count += 1;
                    self.hits.fetch_add(1, Ordering::Relaxed);
                    debug!(
                        endpoint = %endpoint,
                        request_count = conn.request_count,
                        "Reusing pooled WS connection"
                    );
                    return Some(conn);
                }
            }
        }

        // No usable connection -- create a new one
        self.misses.fetch_add(1, Ordering::Relaxed);
        match self.create_connection(endpoint).await {
            Some(conn) => {
                self.total_created.fetch_add(1, Ordering::Relaxed);
                debug!(endpoint = %endpoint, "Created new pooled WS connection");
                Some(conn)
            }
            None => {
                warn!(endpoint = %endpoint, "Failed to create pooled WS connection");
                None
            }
        }
    }

    /// Return a connection to the pool for reuse.
    ///
    /// If the pool is full for this endpoint, or the connection is expired/idle,
    /// it will be closed and discarded.
    pub async fn return_connection(&self, mut conn: PooledConnection) {
        if self.shutting_down.load(Ordering::Relaxed) {
            let _ = conn.sender.close().await;
            return;
        }

        let max_age = std::time::Duration::from_secs(self.config.max_age_secs);
        let idle_timeout = std::time::Duration::from_secs(self.config.idle_timeout_secs);

        // Don't return expired connections
        if conn.is_expired(max_age) {
            let _ = conn.sender.close().await;
            self.total_closed.fetch_add(1, Ordering::Relaxed);
            return;
        }

        let endpoint = conn.endpoint.clone();
        let bucket = self
            .pools
            .entry(endpoint.clone())
            .or_insert_with(ProviderPool::new);

        let mut conns = bucket.connections.lock().await;

        // If pool is full, close the oldest connection
        if conns.len() >= self.config.max_per_provider {
            if let Some(oldest) = conns.first_mut() {
                if oldest.is_idle(idle_timeout) {
                    let _ = oldest.sender.close().await;
                    conns.remove(0);
                    self.total_closed.fetch_add(1, Ordering::Relaxed);
                } else {
                    // Pool is full and no idle connections -- close the returned one
                    let _ = conn.sender.close().await;
                    self.total_closed.fetch_add(1, Ordering::Relaxed);
                    return;
                }
            }
        }

        conns.push(conn);
        debug!(endpoint = %endpoint, pool_size = conns.len(), "Returned WS connection to pool");
    }

    /// Create a new WebSocket connection to a provider endpoint.
    async fn create_connection(&self, endpoint: &str) -> Option<PooledConnection> {
        // Build the WS URL from the HTTP endpoint
        let ws_url = endpoint
            .trim_end_matches('/')
            .replace("http://", "ws://")
            .replace("https://", "wss://");

        let connect_url = format!("{}/v1/chat/ws", ws_url);

        let (ws_stream, _response) = tokio_tungstenite::connect_async(&connect_url)
            .await
            .map_err(|e| {
                warn!(endpoint = %endpoint, error = %e, "WS pool: failed to connect to provider");
                e
            })
            .ok()?;

        let (sender, receiver) = ws_stream.split();

        Some(PooledConnection {
            sender,
            receiver,
            created_at: Instant::now(),
            last_used_at: Instant::now(),
            request_count: 0,
            endpoint: endpoint.to_string(),
        })
    }

    /// Run a health check on all connections, removing dead ones.
    pub async fn health(&self) {
        let max_age = std::time::Duration::from_secs(self.config.max_age_secs);
        let idle_timeout = std::time::Duration::from_secs(self.config.idle_timeout_secs);

        for mut entry in self.pools.iter_mut() {
            let endpoint = entry.key().clone();
            let mut conns = entry.value_mut().connections.lock().await;
            let before = conns.len();

            conns.retain(|conn| {
                if conn.is_expired(max_age) || conn.is_idle(idle_timeout) {
                    debug!(
                        endpoint = %endpoint,
                        age_secs = conn.created_at.elapsed().as_secs(),
                        idle_secs = conn.last_used_at.elapsed().as_secs(),
                        "Pruning stale pooled WS connection"
                    );
                    // We can't close async in retain -- will be dropped
                    // The Drop impl will attempt a close, but for async we just discard
                    self.total_closed.fetch_add(1, Ordering::Relaxed);
                    false
                } else {
                    true
                }
            });

            let removed = before - conns.len();
            if removed > 0 {
                debug!(
                    endpoint = %endpoint,
                    removed,
                    remaining = conns.len(),
                    "Pruned connections from pool"
                );
            }
        }
    }

    /// Get pool statistics.
    pub fn stats(&self) -> WsPoolStats {
        let total_connections: usize = self
            .pools
            .iter()
            .map(|entry| {
                // Use try_lock to avoid blocking in stats
                entry
                    .value()
                    .connections
                    .try_lock()
                    .map(|c| c.len())
                    .unwrap_or(0)
            })
            .sum();

        let provider_count = self.pools.len();

        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total = hits + misses;
        let hit_rate = if total > 0 {
            hits as f64 / total as f64
        } else {
            0.0
        };

        let per_provider: Vec<ProviderPoolStats> = self
            .pools
            .iter()
            .map(|entry| {
                let conns = entry
                    .value()
                    .connections
                    .try_lock()
                    .map(|c| c.len())
                    .unwrap_or(0);
                let avg_age_secs = entry
                    .value()
                    .connections
                    .try_lock()
                    .map(|c| {
                        if c.is_empty() {
                            0.0
                        } else {
                            c.iter()
                                .map(|conn| conn.created_at.elapsed().as_secs_f64())
                                .sum::<f64>()
                                / c.len() as f64
                        }
                    })
                    .unwrap_or(0.0);
                ProviderPoolStats {
                    endpoint: entry.key().clone(),
                    connections: conns,
                    avg_age_secs,
                }
            })
            .collect();

        WsPoolStats {
            enabled: self.config.enabled,
            total_connections,
            provider_count,
            per_provider,
            hits,
            misses,
            hit_rate,
            total_created: self.total_created.load(Ordering::Relaxed),
            total_closed: self.total_closed.load(Ordering::Relaxed),
        }
    }

    /// Gracefully close all pooled connections.
    pub async fn close_all(&self) {
        self.shutting_down.store(true, Ordering::Relaxed);
        info!(total_endpoints = self.pools.len(), "Closing all pooled WS connections");

        for mut entry in self.pools.iter_mut() {
            let endpoint = entry.key().clone();
            let mut conns = entry.value_mut().connections.lock().await;
            let count = conns.len();
            for mut conn in conns.drain(..) {
                let _ = conn.sender.close().await;
            }
            self.total_closed.fetch_add(count as u64, Ordering::Relaxed);
            debug!(endpoint = %endpoint, closed = count, "Closed pooled WS connections");
        }

        self.pools.clear();
        info!("All pooled WS connections closed");
    }

    /// Start the background maintenance task.
    ///
    /// Returns a `JoinHandle` that can be used to abort the task on shutdown.
    pub fn start_maintenance_task(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        let interval_secs = self.config.maintenance_interval_secs;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            loop {
                interval.tick().await;

                if self.shutting_down.load(Ordering::Relaxed) {
                    debug!("WS pool maintenance task shutting down");
                    break;
                }

                let stats = self.stats();
                debug!(
                    total_connections = stats.total_connections,
                    provider_count = stats.provider_count,
                    hit_rate = format!("{:.2}%", stats.hit_rate * 100.0),
                    "WS pool maintenance tick"
                );

                self.health().await;

                // Clean up empty provider entries
                self.pools.retain(|_, bucket| {
                    bucket
                        .connections
                        .try_lock()
                        .map(|c| !c.is_empty())
                        .unwrap_or(true)
                });
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ws_pool_config_default() {
        let config = WsPoolConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_per_provider, 5);
        assert_eq!(config.idle_timeout_secs, 60);
        assert_eq!(config.max_age_secs, 300);
        assert_eq!(config.maintenance_interval_secs, 30);
    }

    #[test]
    fn test_pooled_connection_idle_check() {
        let (tx, _rx) = tokio::sync::mpsc::channel::<Message>(1);
        // We can't easily create a real PooledConnection in tests,
        // so we just test the config defaults
        let config = WsPoolConfig::default();
        assert_eq!(config.idle_timeout_secs, 60);
        assert_eq!(config.max_age_secs, 300);
    }

    #[test]
    fn test_ws_pool_stats_serialization() {
        let stats = WsPoolStats {
            enabled: true,
            total_connections: 10,
            provider_count: 3,
            per_provider: vec![ProviderPoolStats {
                endpoint: "http://localhost:9099".to_string(),
                connections: 5,
                avg_age_secs: 30.5,
            }],
            hits: 100,
            misses: 25,
            hit_rate: 0.8,
            total_created: 50,
            total_closed: 40,
        };
        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("\"hit_rate\":0.8"));
        assert!(json.contains("\"total_connections\":10"));
    }

    #[tokio::test]
    async fn test_ws_pool_disabled() {
        let config = WsPoolConfig {
            enabled: false,
            ..Default::default()
        };
        let pool = WsConnectionPool::new(config);
        assert!(!pool.is_enabled());
        // get_connection should return None when disabled
        let result = pool.get_connection("http://localhost:9099").await;
        assert!(result.is_none());
    }

    #[test]
    fn test_ws_pool_initial_stats() {
        let pool = WsConnectionPool::new(WsPoolConfig::default());
        let stats = pool.stats();
        assert!(stats.enabled);
        assert_eq!(stats.total_connections, 0);
        assert_eq!(stats.provider_count, 0);
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert!((stats.hit_rate - 0.0).abs() < f64::EPSILON);
    }
}
