//! API Gateway — Request routing, upstream proxying, auth gating, rate limiting,
//! circuit breaking, and stats tracking.
//!
//! REST endpoints:
//! - GET    /v1/gateway/routes       — list all routes
//! - POST   /v1/gateway/routes       — add a route
//! - DELETE /v1/gateway/routes/{id}  — remove a route
//! - GET    /v1/gateway/stats        — gateway statistics
//! - GET    /v1/gateway/config       — current configuration
//! - PUT    /v1/gateway/config       — update configuration

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{info};

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// A single gateway route definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayRoute {
    pub route_id: String,
    pub path_pattern: String,
    pub upstream_url: String,
    pub methods: Vec<String>,
    pub auth_required: bool,
    pub rate_limit: Option<u32>,
    pub timeout_ms: u64,
    pub retry_count: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub enabled: bool,
}

/// Gateway configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    pub default_timeout_ms: u64,
    pub max_retries: u32,
    pub circuit_breaker_threshold: u32,
    pub enable_compression: bool,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            default_timeout_ms: 30_000,
            max_retries: 3,
            circuit_breaker_threshold: 5,
            enable_compression: false,
        }
    }
}

/// Gateway statistics.
#[derive(Debug, Clone, Serialize)]
pub struct GatewayStats {
    pub total_requests: u64,
    pub active_connections: u64,
    pub error_count: u64,
    pub success_count: u64,
    pub total_latency_ms: u64,
    pub avg_latency_ms: f64,
    pub requests_per_route: HashMap<String, u64>,
}

/// Request body for adding a route.
#[derive(Debug, Deserialize)]
pub struct AddRouteRequest {
    pub path_pattern: String,
    pub upstream_url: String,
    #[serde(default = "default_methods")]
    pub methods: Vec<String>,
    #[serde(default)]
    pub auth_required: bool,
    pub rate_limit: Option<u32>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    #[serde(default = "default_retry")]
    pub retry_count: u32,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_methods() -> Vec<String> {
    vec!["GET".to_string(), "POST".to_string(), "PUT".to_string(), "DELETE".to_string()]
}

fn default_timeout() -> u64 {
    30_000
}

fn default_retry() -> u32 {
    3
}

fn default_true() -> bool {
    true
}

/// Request body for updating config.
#[derive(Debug, Deserialize)]
pub struct UpdateGatewayConfigRequest {
    pub default_timeout_ms: Option<u64>,
    pub max_retries: Option<u32>,
    pub circuit_breaker_threshold: Option<u32>,
    pub enable_compression: Option<bool>,
}

impl Default for UpdateGatewayConfigRequest {
    fn default() -> Self {
        Self {
            default_timeout_ms: None,
            max_retries: None,
            circuit_breaker_threshold: None,
            enable_compression: None,
        }
    }
}

/// Route match result.
#[derive(Debug, Clone)]
pub struct RouteMatch {
    pub route: GatewayRoute,
    pub path_params: HashMap<String, String>,
}

/// Routed request record for stats.
#[derive(Debug, Clone)]
pub struct RoutedRequest {
    pub route_id: String,
    pub method: String,
    pub path: String,
    pub status_code: u16,
    pub latency_ms: u64,
    pub success: bool,
}

// ---------------------------------------------------------------------------
// ApiGateway
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ApiGateway {
    /// route_id -> GatewayRoute
    routes: Arc<DashMap<String, GatewayRoute>>,
    /// Statistics atomics
    total_requests: Arc<AtomicU64>,
    active_connections: Arc<AtomicU64>,
    error_count: Arc<AtomicU64>,
    success_count: Arc<AtomicU64>,
    total_latency_ms: Arc<AtomicU64>,
    /// Per-route request counts
    requests_per_route: Arc<DashMap<String, AtomicU64>>,
    /// Configuration
    config: Arc<std::sync::RwLock<GatewayConfig>>,
    /// Circuit breaker: consecutive failures per route
    route_failures: Arc<DashMap<String, AtomicU64>>,
}

impl Default for ApiGateway {
    fn default() -> Self {
        Self::new()
    }
}

impl ApiGateway {
    pub fn new() -> Self {
        Self {
            routes: Arc::new(DashMap::new()),
            total_requests: Arc::new(AtomicU64::new(0)),
            active_connections: Arc::new(AtomicU64::new(0)),
            error_count: Arc::new(AtomicU64::new(0)),
            success_count: Arc::new(AtomicU64::new(0)),
            total_latency_ms: Arc::new(AtomicU64::new(0)),
            requests_per_route: Arc::new(DashMap::new()),
            config: Arc::new(std::sync::RwLock::new(GatewayConfig::default())),
            route_failures: Arc::new(DashMap::new()),
        }
    }

    // -- Configuration ------------------------------------------------------

    pub fn get_config(&self) -> GatewayConfig {
        self.config.read().unwrap().clone()
    }

    pub fn update_config(&self, update: UpdateGatewayConfigRequest) {
        let mut cfg = self.config.write().unwrap();
        if let Some(v) = update.default_timeout_ms {
            cfg.default_timeout_ms = v;
        }
        if let Some(v) = update.max_retries {
            cfg.max_retries = v;
        }
        if let Some(v) = update.circuit_breaker_threshold {
            cfg.circuit_breaker_threshold = v;
        }
        if let Some(v) = update.enable_compression {
            cfg.enable_compression = v;
        }
    }

    // -- Route management ---------------------------------------------------

    /// Add a new route. Returns the route ID.
    pub fn add_route(&self, req: AddRouteRequest) -> String {
        let route_id = uuid::Uuid::new_v4().to_string();
        let route = GatewayRoute {
            route_id: route_id.clone(),
            path_pattern: req.path_pattern,
            upstream_url: req.upstream_url,
            methods: req.methods,
            auth_required: req.auth_required,
            rate_limit: req.rate_limit,
            timeout_ms: req.timeout_ms,
            retry_count: req.retry_count,
            created_at: chrono::Utc::now(),
            enabled: req.enabled,
        };
        info!(route_id = %route_id, path = %route.path_pattern, "Gateway route added");
        self.routes.insert(route_id.clone(), route);
        self.requests_per_route
            .insert(route_id.clone(), AtomicU64::new(0));
        self.route_failures
            .insert(route_id.clone(), AtomicU64::new(0));
        route_id
    }

    /// Remove a route by ID.
    pub fn remove_route(&self, route_id: &str) -> bool {
        let removed = self.routes.remove(route_id).is_some();
        if removed {
            self.requests_per_route.remove(route_id);
            self.route_failures.remove(route_id);
            info!(route_id = %route_id, "Gateway route removed");
        }
        removed
    }

    /// List all routes.
    pub fn list_routes(&self) -> Vec<GatewayRoute> {
        self.routes.iter().map(|v| v.value().clone()).collect()
    }

    /// Get a specific route.
    pub fn get_route(&self, route_id: &str) -> Option<GatewayRoute> {
        self.routes.get(route_id).map(|v| v.value().clone())
    }

    // -- Pattern matching ---------------------------------------------------

    /// Match a request path + method against registered routes.
    /// Supports wildcards: `/api/*` matches `/api/anything`.
    pub fn match_route(&self, method: &str, path: &str) -> Option<RouteMatch> {
        for entry in self.routes.iter() {
            let route = entry.value();
            if !route.enabled {
                continue;
            }
            if !route.methods.iter().any(|m| m.eq_ignore_ascii_case(method)) {
                continue;
            }
            if self.path_matches(&route.path_pattern, path) {
                let path_params = self.extract_path_params(&route.path_pattern, path);
                return Some(RouteMatch {
                    route: route.clone(),
                    path_params,
                });
            }
        }
        None
    }

    // -- Routing ------------------------------------------------------------

    /// Record a routed request (for stats).
    pub fn route_request(&self, record: RoutedRequest) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        if record.success {
            self.success_count.fetch_add(1, Ordering::Relaxed);
            // Reset failure count on success
            if let Some(failures) = self.route_failures.get(&record.route_id) {
                failures.store(0, Ordering::Relaxed);
            }
        } else {
            self.error_count.fetch_add(1, Ordering::Relaxed);
            // Increment failure count for circuit breaker
            if let Some(failures) = self.route_failures.get(&record.route_id) {
                failures.fetch_add(1, Ordering::Relaxed);
            }
        }
        self.total_latency_ms
            .fetch_add(record.latency_ms, Ordering::Relaxed);
        if let Some(counter) = self.requests_per_route.get(&record.route_id) {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Check if a route's circuit breaker is open.
    pub fn is_circuit_open(&self, route_id: &str) -> bool {
        let cfg = self.get_config();
        if let Some(failures) = self.route_failures.get(route_id) {
            failures.load(Ordering::Relaxed) >= cfg.circuit_breaker_threshold as u64
        } else {
            false
        }
    }

    // -- Stats --------------------------------------------------------------

    pub fn get_stats(&self) -> GatewayStats {
        let total = self.total_requests.load(Ordering::Relaxed);
        let active = self.active_connections.load(Ordering::Relaxed);
        let errors = self.error_count.load(Ordering::Relaxed);
        let successes = self.success_count.load(Ordering::Relaxed);
        let latency = self.total_latency_ms.load(Ordering::Relaxed);

        let requests_per_route: HashMap<String, u64> = self
            .requests_per_route
            .iter()
            .map(|e| (e.key().clone(), e.value().load(Ordering::Relaxed)))
            .collect();

        let avg_latency = if total > 0 {
            latency as f64 / total as f64
        } else {
            0.0
        };

        GatewayStats {
            total_requests: total,
            active_connections: active,
            error_count: errors,
            success_count: successes,
            total_latency_ms: latency,
            avg_latency_ms: avg_latency,
            requests_per_route,
        }
    }

    /// Increment active connections.
    pub fn inc_connections(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement active connections.
    pub fn dec_connections(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    // -- Internal -----------------------------------------------------------

    /// Check if a path matches a pattern with wildcard support.
    /// Patterns like `/api/*` match `/api/anything`.
    fn path_matches(&self, pattern: &str, path: &str) -> bool {
        if pattern == path {
            return true;
        }
        // Handle wildcard suffix
        if let Some(prefix) = pattern.strip_suffix("/*") {
            if path.starts_with(prefix) {
                let rest = &path[prefix.len()..];
                // Must have at least "/" after the prefix
                return rest.starts_with('/') || rest.is_empty();
            }
        }
        // Handle single-segment wildcards: /api/:id
        let pattern_parts: Vec<&str> = pattern.split('/').collect();
        let path_parts: Vec<&str> = path.split('/').collect();
        if pattern_parts.len() != path_parts.len() {
            return false;
        }
        for (pp, tp) in pattern_parts.iter().zip(path_parts.iter()) {
            if pp.starts_with(':') {
                continue; // wildcard segment
            }
            if pp != tp {
                return false;
            }
        }
        true
    }

    /// Extract path parameters from a pattern like `/api/:id/users/:uid`.
    fn extract_path_params(&self, pattern: &str, path: &str) -> HashMap<String, String> {
        let mut params = HashMap::new();
        let pattern_parts: Vec<&str> = pattern.split('/').collect();
        let path_parts: Vec<&str> = path.split('/').collect();
        for (pp, tp) in pattern_parts.iter().zip(path_parts.iter()) {
            if pp.starts_with(':') {
                let key = pp.trim_start_matches(':').to_string();
                params.insert(key, tp.to_string());
            }
        }
        params
    }
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

pub fn build_gateway_router() -> Router<AppState> {
    Router::new()
        .route("/v1/gateway/routes", get(list_routes_handler))
        .route("/v1/gateway/routes", post(add_route_handler))
        .route(
            "/v1/gateway/routes/{id}",
            delete(remove_route_handler),
        )
        .route("/v1/gateway/stats", get(gateway_stats_handler))
        .route("/v1/gateway/config", get(gateway_config_handler))
        .route("/v1/gateway/config", put(update_gateway_config_handler))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /v1/gateway/routes
async fn list_routes_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let routes = state.api_gateway.list_routes();
    (
        StatusCode::OK,
        Json(serde_json::json!({ "routes": routes })),
    )
}

/// POST /v1/gateway/routes
async fn add_route_handler(
    State(state): State<AppState>,
    Json(body): Json<AddRouteRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let route_id = state.api_gateway.add_route(body);
    (
        StatusCode::CREATED,
        Json(serde_json::json!({ "route_id": route_id })),
    )
}

/// DELETE /v1/gateway/routes/{id}
async fn remove_route_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let removed = state.api_gateway.remove_route(&id);
    if removed {
        (StatusCode::OK, Json(serde_json::json!({ "removed": true })))
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "route not found" })),
        )
    }
}

/// GET /v1/gateway/stats
async fn gateway_stats_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let stats = state.api_gateway.get_stats();
    (StatusCode::OK, Json(serde_json::json!(stats)))
}

/// GET /v1/gateway/config
async fn gateway_config_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let config = state.api_gateway.get_config();
    (StatusCode::OK, Json(serde_json::json!(config)))
}

/// PUT /v1/gateway/config
async fn update_gateway_config_handler(
    State(state): State<AppState>,
    Json(body): Json<UpdateGatewayConfigRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    state.api_gateway.update_config(body);
    let config = state.api_gateway.get_config();
    (StatusCode::OK, Json(serde_json::json!(config)))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> ApiGateway {
        ApiGateway::new()
    }

    #[test]
    fn test_add_and_list_routes() {
        let gw = setup();
        let id = gw.add_route(AddRouteRequest {
            path_pattern: "/api/v1/*".into(),
            upstream_url: "http://backend:8080".into(),
            methods: vec!["GET".into()],
            auth_required: false,
            rate_limit: None,
            timeout_ms: 5000,
            retry_count: 2,
            enabled: true,
        });
        let routes = gw.list_routes();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].route_id, id);
    }

    #[test]
    fn test_remove_route() {
        let gw = setup();
        let id = gw.add_route(AddRouteRequest {
            path_pattern: "/test".into(),
            upstream_url: "http://test:80".into(),
            methods: vec!["GET".into()],
            auth_required: false,
            rate_limit: None,
            timeout_ms: 3000,
            retry_count: 1,
            enabled: true,
        });
        assert!(gw.remove_route(&id));
        assert!(gw.list_routes().is_empty());
    }

    #[test]
    fn test_remove_nonexistent_route() {
        let gw = setup();
        assert!(!gw.remove_route("nope"));
    }

    #[test]
    fn test_exact_path_match() {
        let gw = setup();
        gw.add_route(AddRouteRequest {
            path_pattern: "/api/health".into(),
            upstream_url: "http://backend:8080".into(),
            methods: vec!["GET".into()],
            auth_required: false,
            rate_limit: None,
            timeout_ms: 5000,
            retry_count: 1,
            enabled: true,
        });
        let matched = gw.match_route("GET", "/api/health");
        assert!(matched.is_some());
        assert_eq!(matched.unwrap().route.upstream_url, "http://backend:8080");
    }

    #[test]
    fn test_wildcard_path_match() {
        let gw = setup();
        gw.add_route(AddRouteRequest {
            path_pattern: "/api/*".into(),
            upstream_url: "http://backend:8080".into(),
            methods: vec!["GET".into(), "POST".into()],
            auth_required: false,
            rate_limit: None,
            timeout_ms: 5000,
            retry_count: 1,
            enabled: true,
        });
        assert!(gw.match_route("GET", "/api/users").is_some());
        assert!(gw.match_route("POST", "/api/data").is_some());
        assert!(gw.match_route("DELETE", "/api/data").is_none()); // method not allowed
        assert!(gw.match_route("GET", "/other").is_none());
    }

    #[test]
    fn test_param_path_match() {
        let gw = setup();
        gw.add_route(AddRouteRequest {
            path_pattern: "/users/:id".into(),
            upstream_url: "http://backend:8080".into(),
            methods: vec!["GET".into()],
            auth_required: false,
            rate_limit: None,
            timeout_ms: 5000,
            retry_count: 1,
            enabled: true,
        });
        let m = gw.match_route("GET", "/users/42").unwrap();
        assert_eq!(m.path_params.get("id").unwrap(), "42");
    }

    #[test]
    fn test_disabled_route_not_matched() {
        let gw = setup();
        gw.add_route(AddRouteRequest {
            path_pattern: "/disabled".into(),
            upstream_url: "http://backend:8080".into(),
            methods: vec!["GET".into()],
            auth_required: false,
            rate_limit: None,
            timeout_ms: 5000,
            retry_count: 1,
            enabled: false,
        });
        assert!(gw.match_route("GET", "/disabled").is_none());
    }

    #[test]
    fn test_route_request_stats() {
        let gw = setup();
        let route_id = gw.add_route(AddRouteRequest {
            path_pattern: "/test".into(),
            upstream_url: "http://backend:8080".into(),
            methods: vec!["GET".into()],
            auth_required: false,
            rate_limit: None,
            timeout_ms: 5000,
            retry_count: 1,
            enabled: true,
        });
        gw.route_request(RoutedRequest {
            route_id: route_id.clone(),
            method: "GET".into(),
            path: "/test".into(),
            status_code: 200,
            latency_ms: 50,
            success: true,
        });
        gw.route_request(RoutedRequest {
            route_id: route_id.clone(),
            method: "GET".into(),
            path: "/test".into(),
            status_code: 500,
            latency_ms: 100,
            success: false,
        });
        let stats = gw.get_stats();
        assert_eq!(stats.total_requests, 2);
        assert_eq!(stats.success_count, 1);
        assert_eq!(stats.error_count, 1);
        assert_eq!(stats.total_latency_ms, 150);
        assert!((stats.avg_latency_ms - 75.0).abs() < 0.01);
        assert_eq!(stats.requests_per_route.get(&route_id).unwrap(), &2);
    }

    #[test]
    fn test_config_update() {
        let gw = setup();
        assert_eq!(gw.get_config().default_timeout_ms, 30_000);
        gw.update_config(UpdateGatewayConfigRequest {
            default_timeout_ms: Some(10_000),
            max_retries: None,
            circuit_breaker_threshold: None,
            enable_compression: None,
        });
        assert_eq!(gw.get_config().default_timeout_ms, 10_000);
    }

    #[test]
    fn test_circuit_breaker() {
        let gw = setup();
        gw.update_config(UpdateGatewayConfigRequest {
            circuit_breaker_threshold: Some(3),
            ..Default::default()
        });
        let route_id = gw.add_route(AddRouteRequest {
            path_pattern: "/cb".into(),
            upstream_url: "http://backend:8080".into(),
            methods: vec!["GET".into()],
            auth_required: false,
            rate_limit: None,
            timeout_ms: 5000,
            retry_count: 1,
            enabled: true,
        });
        assert!(!gw.is_circuit_open(&route_id));
        for _ in 0..3 {
            gw.route_request(RoutedRequest {
                route_id: route_id.clone(),
                method: "GET".into(),
                path: "/cb".into(),
                status_code: 500,
                latency_ms: 10,
                success: false,
            });
        }
        assert!(gw.is_circuit_open(&route_id));
        // Success resets the breaker
        gw.route_request(RoutedRequest {
            route_id: route_id.clone(),
            method: "GET".into(),
            path: "/cb".into(),
            status_code: 200,
            latency_ms: 5,
            success: true,
        });
        assert!(!gw.is_circuit_open(&route_id));
    }
}
