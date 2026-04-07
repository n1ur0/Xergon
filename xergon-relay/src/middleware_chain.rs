//! Middleware chain v2 — request processing pipeline with ordered middleware
//!
//! Provides a pluggable middleware pipeline for request processing:
//! - Before: pre-processing before the main handler
//! - After: post-processing after the main handler
//! - Around: wraps the entire request lifecycle
//!
//! Built-in middlewares:
//! - RequestLogger: logs request metadata
//! - RequestTimer: measures request duration
//! - RequestIdInjector: adds unique request IDs
//! - UserAgentParser: extracts and normalizes user agent info
//! - RequestValidator: validates request structure

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Middleware ordering
// ---------------------------------------------------------------------------

/// Defines when a middleware executes relative to the main handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MiddlewareOrder {
    /// Execute before the main handler.
    Before,
    /// Execute after the main handler.
    After,
    /// Wrap around the entire handler (before + after).
    Around,
}

// ---------------------------------------------------------------------------
// Middleware result
// ---------------------------------------------------------------------------

/// Result returned by a middleware's process method.
pub enum MiddlewareResult {
    /// Continue to the next middleware or handler.
    Next,
    /// Stop processing and return a response immediately.
    Stop(Response),
    /// An error occurred; stop processing.
    Error(String),
}

// ---------------------------------------------------------------------------
// Middleware context
// ---------------------------------------------------------------------------

/// Shared context passed through the middleware chain.
#[derive(Debug, Clone)]
pub struct MiddlewareContext {
    /// Unique request identifier.
    pub request_id: String,
    /// Timestamp when the request entered the chain.
    pub timestamp: DateTime<Utc>,
    /// Custom attributes set by middlewares (key-value store).
    pub attributes: HashMap<String, String>,
    /// Additional metadata (structured data).
    pub metadata: HashMap<String, serde_json::Value>,
}

impl MiddlewareContext {
    /// Create a new middleware context with the given request ID.
    pub fn new(request_id: impl Into<String>) -> Self {
        Self {
            request_id: request_id.into(),
            timestamp: Utc::now(),
            attributes: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    /// Set an attribute.
    pub fn set_attribute(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.attributes.insert(key.into(), value.into());
    }

    /// Get an attribute.
    pub fn get_attribute(&self, key: &str) -> Option<&str> {
        self.attributes.get(key).map(|s| s.as_str())
    }

    /// Set metadata.
    pub fn set_metadata(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.metadata.insert(key.into(), value);
    }

    /// Get metadata.
    pub fn get_metadata(&self, key: &str) -> Option<&serde_json::Value> {
        self.metadata.get(key)
    }
}

// ---------------------------------------------------------------------------
// Middleware trait
// ---------------------------------------------------------------------------

/// Trait that all middlewares must implement.
pub trait Middleware: Send + Sync {
    /// Unique name of this middleware.
    fn name(&self) -> &str;

    /// Execution order.
    fn order(&self) -> MiddlewareOrder;

    /// Process the request through this middleware.
    ///
    /// Receives mutable context and request reference, returns a result
    /// indicating whether to continue, stop, or report an error.
    fn process(
        &self,
        ctx: &mut MiddlewareContext,
        request: &Request<Body>,
    ) -> MiddlewareResult;

    /// Whether this middleware is currently enabled.
    fn is_enabled(&self) -> bool {
        true
    }

    /// Set enabled state (default: no-op, chain manages externally).
    fn set_enabled(&self, _enabled: bool) {}
}

// ---------------------------------------------------------------------------
// Built-in: RequestLogger
// ---------------------------------------------------------------------------

/// Logs request method, path, and key headers.
#[derive(Debug, Clone)]
pub struct RequestLogger {
    enabled: Arc<AtomicBool>,
}

impl RequestLogger {
    pub fn new() -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(true)),
        }
    }
}

impl Default for RequestLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl Middleware for RequestLogger {
    fn name(&self) -> &str {
        "request_logger"
    }

    fn order(&self) -> MiddlewareOrder {
        MiddlewareOrder::Before
    }

    fn process(&self, ctx: &mut MiddlewareContext, request: &Request<Body>) -> MiddlewareResult {
        let method = request.method();
        let uri = request.uri();
        let user_agent = request
            .headers()
            .get("user-agent")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown");

        ctx.set_attribute("method", method.to_string());
        ctx.set_attribute("uri", uri.to_string());
        ctx.set_attribute("user_agent", user_agent.to_string());

        info!(
            request_id = %ctx.request_id,
            method = %method,
            uri = %uri,
            user_agent = %user_agent,
            "Request logged"
        );

        MiddlewareResult::Next
    }

    fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Built-in: RequestTimer
// ---------------------------------------------------------------------------

/// Measures and records request processing duration.
#[derive(Debug, Clone)]
pub struct RequestTimer {
    enabled: Arc<AtomicBool>,
}

impl RequestTimer {
    pub fn new() -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(true)),
        }
    }
}

impl Default for RequestTimer {
    fn default() -> Self {
        Self::new()
    }
}

impl Middleware for RequestTimer {
    fn name(&self) -> &str {
        "request_timer"
    }

    fn order(&self) -> MiddlewareOrder {
        MiddlewareOrder::Around
    }

    fn process(&self, ctx: &mut MiddlewareContext, _request: &Request<Body>) -> MiddlewareResult {
        let start = Instant::now();
        ctx.set_attribute("timer_start", start.elapsed().as_nanos().to_string());

        // Store the start instant in metadata for later use
        ctx.set_metadata(
            "timer_start_ns",
            serde_json::json!(start.elapsed().as_nanos()),
        );

        MiddlewareResult::Next
    }

    fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Built-in: RequestIdInjector
// ---------------------------------------------------------------------------

/// Injects or validates a unique request ID header.
#[derive(Debug, Clone)]
pub struct RequestIdInjector {
    enabled: Arc<AtomicBool>,
    header_name: String,
}

impl RequestIdInjector {
    pub fn new() -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(true)),
            header_name: "X-Request-Id".to_string(),
        }
    }

    pub fn with_header(header: impl Into<String>) -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(true)),
            header_name: header.into(),
        }
    }
}

impl Default for RequestIdInjector {
    fn default() -> Self {
        Self::new()
    }
}

impl Middleware for RequestIdInjector {
    fn name(&self) -> &str {
        "request_id_injector"
    }

    fn order(&self) -> MiddlewareOrder {
        MiddlewareOrder::Before
    }

    fn process(&self, ctx: &mut MiddlewareContext, request: &Request<Body>) -> MiddlewareResult {
        let existing = request
            .headers()
            .get(&self.header_name)
            .and_then(|v| v.to_str().ok());

        if let Some(id) = existing {
            ctx.set_attribute("request_id_source", "header");
            debug!(request_id = %id, "Using existing request ID from header");
        } else {
            ctx.set_attribute("request_id_source", "generated");
            debug!(request_id = %ctx.request_id, "Generated new request ID");
        }

        MiddlewareResult::Next
    }

    fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Built-in: UserAgentParser
// ---------------------------------------------------------------------------

/// Parses and normalizes the User-Agent header into structured data.
#[derive(Debug, Clone)]
pub struct UserAgentParser {
    enabled: Arc<AtomicBool>,
}

impl UserAgentParser {
    pub fn new() -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(true)),
        }
    }
}

impl Default for UserAgentParser {
    fn default() -> Self {
        Self::new()
    }
}

impl Middleware for UserAgentParser {
    fn name(&self) -> &str {
        "user_agent_parser"
    }

    fn order(&self) -> MiddlewareOrder {
        MiddlewareOrder::Before
    }

    fn process(&self, ctx: &mut MiddlewareContext, request: &Request<Body>) -> MiddlewareResult {
        let ua = request
            .headers()
            .get("user-agent")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("unknown/0.0");

        // Simple parsing: take first segment as browser/client name
        let parts: Vec<&str> = ua.split('/').collect();
        let client_name = parts.first().unwrap_or(&"unknown").to_string();
        let client_version = parts.get(1).unwrap_or(&"0.0").to_string();

        // Detect common bots/crawlers
        let is_bot = ua.contains("bot")
            || ua.contains("crawler")
            || ua.contains("spider")
            || ua.contains("scraper");

        ctx.set_attribute("client_name", client_name.clone());
        ctx.set_attribute("client_version", client_version.clone());
        ctx.set_attribute("is_bot", is_bot.to_string());

        ctx.set_metadata(
            "user_agent",
            serde_json::json!({
                "raw": ua,
                "client": client_name,
                "version": client_version,
                "is_bot": is_bot,
            }),
        );

        debug!(
            client = %client_name,
            version = %client_version,
            is_bot = is_bot,
            "User agent parsed"
        );

        MiddlewareResult::Next
    }

    fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Built-in: RequestValidator
// ---------------------------------------------------------------------------

/// Validates request structure (method, content-type, body size).
#[derive(Debug, Clone)]
pub struct RequestValidator {
    enabled: Arc<AtomicBool>,
    max_body_size_bytes: u64,
    allowed_methods: Vec<String>,
}

impl RequestValidator {
    pub fn new() -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(true)),
            max_body_size_bytes: 10 * 1024 * 1024, // 10 MB
            allowed_methods: vec![
                "GET".to_string(),
                "POST".to_string(),
                "PUT".to_string(),
                "DELETE".to_string(),
                "OPTIONS".to_string(),
                "PATCH".to_string(),
            ],
        }
    }

    pub fn with_max_body_size(bytes: u64) -> Self {
        Self {
            max_body_size_bytes: bytes,
            ..Self::new()
        }
    }
}

impl Default for RequestValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl Middleware for RequestValidator {
    fn name(&self) -> &str {
        "request_validator"
    }

    fn order(&self) -> MiddlewareOrder {
        MiddlewareOrder::Before
    }

    fn process(&self, ctx: &mut MiddlewareContext, request: &Request<Body>) -> MiddlewareResult {
        // Validate HTTP method
        let method_str = request.method().as_str().to_uppercase();
        if !self.allowed_methods.contains(&method_str) {
            warn!(
                request_id = %ctx.request_id,
                method = %method_str,
                "Blocked request with disallowed method"
            );
            return MiddlewareResult::Stop(
                StatusCode::METHOD_NOT_ALLOWED.into_response(),
            );
        }

        // Validate content-length for requests with bodies
        if let Some(cl) = request.headers().get("content-length") {
            if let Ok(size_str) = cl.to_str() {
                if let Ok(size) = size_str.parse::<u64>() {
                    if size > self.max_body_size_bytes {
                        warn!(
                            request_id = %ctx.request_id,
                            size = size,
                            max = self.max_body_size_bytes,
                            "Request body too large"
                        );
                        return MiddlewareResult::Stop(
                            StatusCode::PAYLOAD_TOO_LARGE.into_response(),
                        );
                    }
                }
            }
        }

        ctx.set_attribute("validation_passed", "true");
        debug!(request_id = %ctx.request_id, "Request validated successfully");
        MiddlewareResult::Next
    }

    fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Middleware chain entry
// ---------------------------------------------------------------------------

/// A single entry in the middleware chain.
struct ChainEntry {
    /// The middleware instance (type-erased via Arc<dyn Middleware>).
    middleware: Arc<dyn Middleware>,
    /// Position in the chain (0-based).
    position: usize,
    /// Whether this entry is enabled.
    enabled: Arc<AtomicBool>,
}

// ---------------------------------------------------------------------------
// Middleware chain configuration
// ---------------------------------------------------------------------------

/// Configuration for the middleware chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiddlewareChainConfig {
    /// Maximum number of middlewares allowed in the chain.
    #[serde(default = "default_max_middlewares")]
    pub max_middlewares: usize,
    /// Whether to continue processing on middleware error.
    #[serde(default = "default_true")]
    pub continue_on_error: bool,
    /// Request timeout in seconds (0 = no timeout).
    #[serde(default)]
    pub request_timeout_secs: u64,
}

fn default_max_middlewares() -> usize {
    50
}
fn default_true() -> bool {
    true
}

impl Default for MiddlewareChainConfig {
    fn default() -> Self {
        Self {
            max_middlewares: default_max_middlewares(),
            continue_on_error: true,
            request_timeout_secs: 30,
        }
    }
}

// ---------------------------------------------------------------------------
// MiddlewareChain — main struct
// ---------------------------------------------------------------------------

/// Ordered middleware processing pipeline.
#[derive(Clone)]
pub struct MiddlewareChain {
    /// Ordered list of middleware entries.
    entries: Arc<DashMap<String, ChainEntry>>,
    /// Processing order (names in execution order).
    order: Arc<std::sync::RwLock<Vec<String>>>,
    /// Configuration.
    config: Arc<std::sync::RwLock<MiddlewareChainConfig>>,
    /// Total requests processed.
    total_processed: Arc<AtomicU64>,
    /// Total errors encountered.
    total_errors: Arc<AtomicU64>,
    /// Total requests stopped by middleware.
    total_stopped: Arc<AtomicU64>,
}

impl MiddlewareChain {
    /// Create a new empty middleware chain.
    pub fn new() -> Self {
        Self::with_config(MiddlewareChainConfig::default())
    }

    /// Create a new middleware chain with the given config.
    pub fn with_config(config: MiddlewareChainConfig) -> Self {
        Self {
            entries: Arc::new(DashMap::new()),
            order: Arc::new(std::sync::RwLock::new(Vec::new())),
            config: Arc::new(std::sync::RwLock::new(config)),
            total_processed: Arc::new(AtomicU64::new(0)),
            total_errors: Arc::new(AtomicU64::new(0)),
            total_stopped: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Add a middleware to the chain.
    pub fn add_middleware(&self, middleware: Arc<dyn Middleware>) -> Result<(), String> {
        let name = middleware.name().to_string();
        let cfg = self.config.read().unwrap();

        if self.entries.len() >= cfg.max_middlewares {
            return Err(format!(
                "Maximum number of middlewares ({}) reached",
                cfg.max_middlewares
            ));
        }

        if self.entries.contains_key(&name) {
            return Err(format!("Middleware '{}' already exists", name));
        }
        drop(cfg);

        let mut order = self.order.write().unwrap();
        let position = order.len();
        order.push(name.clone());

        self.entries.insert(
            name.clone(),
            ChainEntry {
                middleware,
                position,
                enabled: Arc::new(AtomicBool::new(true)),
            },
        );

        info!(name = %name, position = position, "Middleware added to chain");
        Ok(())
    }

    /// Remove a middleware by name.
    pub fn remove_middleware(&self, name: &str) -> bool {
        let removed = self.entries.remove(name).is_some();
        if removed {
            let mut order = self.order.write().unwrap();
            order.retain(|n| n != name);
            // Re-index positions
            for (i, n) in order.iter().enumerate() {
                if let Some(mut entry) = self.entries.get_mut(n) {
                    entry.position = i;
                }
            }
            info!(name = %name, "Middleware removed from chain");
        }
        removed
    }

    /// Process a request through the middleware chain.
    ///
    /// Returns the context after all middlewares have executed, and optionally
    /// a stop response if any middleware returned `MiddlewareResult::Stop`.
    pub fn process(
        &self,
        ctx: &mut MiddlewareContext,
        request: &Request<Body>,
    ) -> Option<Response> {
        self.total_processed.fetch_add(1, Ordering::Relaxed);

        let order = self.order.read().unwrap().clone();
        let cfg = self.config.read().unwrap();
        let continue_on_error = cfg.continue_on_error;
        drop(cfg);

        for name in &order {
            let entry = match self.entries.get(name) {
                Some(e) => e,
                None => continue,
            };

            if !entry.enabled.load(Ordering::Relaxed) {
                continue;
            }

            if !entry.middleware.is_enabled() {
                continue;
            }

            match entry.middleware.process(ctx, request) {
                MiddlewareResult::Next => continue,
                MiddlewareResult::Stop(response) => {
                    self.total_stopped.fetch_add(1, Ordering::Relaxed);
                    debug!(middleware = %name, "Request stopped by middleware");
                    return Some(response);
                }
                MiddlewareResult::Error(err) => {
                    self.total_errors.fetch_add(1, Ordering::Relaxed);
                    warn!(
                        middleware = %name,
                        error = %err,
                        "Middleware error"
                    );
                    if !continue_on_error {
                        return Some(StatusCode::INTERNAL_SERVER_ERROR.into_response());
                    }
                }
            }
        }

        None
    }

    /// Get the current chain order (names in execution order).
    pub fn get_chain(&self) -> Vec<String> {
        self.order.read().unwrap().clone()
    }

    /// Get detailed info about each middleware in the chain.
    pub fn get_chain_info(&self) -> Vec<MiddlewareInfo> {
        let order = self.order.read().unwrap();
        order
            .iter()
            .filter_map(|name| {
                self.entries.get(name).map(|entry| MiddlewareInfo {
                    name: name.clone(),
                    position: entry.position,
                    order: entry.middleware.order(),
                    enabled: entry.enabled.load(Ordering::Relaxed),
                    middleware_enabled: entry.middleware.is_enabled(),
                })
            })
            .collect()
    }

    /// Reorder the chain by providing a new ordered list of middleware names.
    pub fn reorder(&self, new_order: Vec<String>) -> Result<(), String> {
        // Validate all names exist
        for name in &new_order {
            if !self.entries.contains_key(name) {
                return Err(format!("Unknown middleware: {}", name));
            }
        }

        // Check all existing middlewares are included
        let existing: std::collections::HashSet<_> =
            self.entries.iter().map(|e| e.key().clone()).collect();
        let new_set: std::collections::HashSet<_> = new_order.iter().cloned().collect();
        if existing != new_set {
            return Err("Reorder must include all existing middlewares".to_string());
        }

        // Update positions and order
        let mut order = self.order.write().unwrap();
        for (i, name) in new_order.iter().enumerate() {
            if let Some(mut entry) = self.entries.get_mut(name) {
                entry.position = i;
            }
        }
        *order = new_order;

        info!("Middleware chain reordered");
        Ok(())
    }

    /// Enable a middleware by name.
    pub fn enable(&self, name: &str) -> bool {
        if let Some(entry) = self.entries.get(name) {
            entry.enabled.store(true, Ordering::Relaxed);
            entry.middleware.set_enabled(true);
            info!(name = %name, "Middleware enabled");
            true
        } else {
            false
        }
    }

    /// Disable a middleware by name.
    pub fn disable(&self, name: &str) -> bool {
        if let Some(entry) = self.entries.get(name) {
            entry.enabled.store(false, Ordering::Relaxed);
            entry.middleware.set_enabled(false);
            info!(name = %name, "Middleware disabled");
            true
        } else {
            false
        }
    }

    /// Get chain statistics.
    pub fn get_stats(&self) -> ChainStats {
        ChainStats {
            total_processed: self.total_processed.load(Ordering::Relaxed),
            total_errors: self.total_errors.load(Ordering::Relaxed),
            total_stopped: self.total_stopped.load(Ordering::Relaxed),
            middleware_count: self.entries.len(),
            chain_order: self.get_chain(),
        }
    }

    /// Get the chain configuration.
    pub fn get_config(&self) -> MiddlewareChainConfig {
        self.config.read().unwrap().clone()
    }

    /// Update the chain configuration.
    pub fn update_config(&self, new_config: MiddlewareChainConfig) {
        let mut cfg = self.config.write().unwrap();
        *cfg = new_config;
    }
}

impl Default for MiddlewareChain {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Info / stats types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiddlewareInfo {
    pub name: String,
    pub position: usize,
    pub order: MiddlewareOrder,
    pub enabled: bool,
    pub middleware_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainStats {
    pub total_processed: u64,
    pub total_errors: u64,
    pub total_stopped: u64,
    pub middleware_count: usize,
    pub chain_order: Vec<String>,
}

// ---------------------------------------------------------------------------
// REST request/response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ChainResponse {
    pub chain: Vec<MiddlewareInfo>,
    pub count: usize,
}

#[derive(Debug, Deserialize)]
pub struct ReorderRequest {
    pub order: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ReorderResponse {
    pub status: String,
    pub chain: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ToggleRequest {
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub struct ToggleResponse {
    pub name: String,
    pub enabled: bool,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct ChainConfigResponse {
    pub config: MiddlewareChainConfig,
}

#[derive(Debug, Serialize)]
pub struct ChainStatsResponse {
    pub stats: ChainStats,
}

// ---------------------------------------------------------------------------
// REST handlers
// ---------------------------------------------------------------------------

async fn get_chain_handler(State(state): State<AppState>) -> impl IntoResponse {
    let chain = state.middleware_chain.get_chain_info();
    let count = chain.len();
    (StatusCode::OK, Json(ChainResponse { chain, count }))
}

async fn reorder_chain_handler(
    State(state): State<AppState>,
    Json(body): Json<ReorderRequest>,
) -> impl IntoResponse {
    match state.middleware_chain.reorder(body.order.clone()) {
        Ok(()) => {
            info!("Middleware chain reordered via API");
            (
                StatusCode::OK,
                Json(ReorderResponse {
                    status: "reordered".to_string(),
                    chain: body.order,
                }),
            )
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ReorderResponse {
                status: format!("error: {}", e),
                chain: body.order,
            }),
        ),
    }
}

async fn toggle_middleware_handler(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<ToggleRequest>,
) -> impl IntoResponse {
    let success = if body.enabled {
        state.middleware_chain.enable(&name)
    } else {
        state.middleware_chain.disable(&name)
    };

    (
        StatusCode::OK,
        Json(ToggleResponse {
            name,
            enabled: success,
            status: if success { "updated".to_string() } else { "not_found".to_string() },
        }),
    )
}

async fn get_config_handler(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.middleware_chain.get_config();
    (StatusCode::OK, Json(ChainConfigResponse { config }))
}

async fn get_stats_handler(State(state): State<AppState>) -> impl IntoResponse {
    let stats = state.middleware_chain.get_stats();
    (StatusCode::OK, Json(ChainStatsResponse { stats }))
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the middleware chain API router.
pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/v1/middleware/chain", get(get_chain_handler))
        .route("/v1/middleware/chain/reorder", post(reorder_chain_handler))
        .route("/v1/middleware/{name}/enable", post(toggle_middleware_handler))
        .route("/v1/middleware/config", get(get_config_handler))
        .route("/v1/middleware/stats", get(get_stats_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_request(method: &str, uri: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .unwrap()
    }

    fn make_request_with_ua(method: &str, uri: &str, ua: &str) -> Request<Body> {
        Request::builder()
            .method(method)
            .header("user-agent", ua)
            .uri(uri)
            .body(Body::empty())
            .unwrap()
    }

    #[test]
    fn test_new_creates_empty_chain() {
        let chain = MiddlewareChain::new();
        assert_eq!(chain.get_chain().len(), 0);
        assert_eq!(chain.get_chain_info().len(), 0);
    }

    #[test]
    fn test_add_middleware() {
        let chain = MiddlewareChain::new();
        let mw = Arc::new(RequestLogger::new());
        assert!(chain.add_middleware(mw).is_ok());
        assert_eq!(chain.get_chain().len(), 1);
    }

    #[test]
    fn test_add_duplicate_middleware_fails() {
        let chain = MiddlewareChain::new();
        let mw = Arc::new(RequestLogger::new());
        assert!(chain.add_middleware(mw.clone()).is_ok());
        assert!(chain.add_middleware(mw).is_err());
    }

    #[test]
    fn test_remove_middleware() {
        let chain = MiddlewareChain::new();
        let mw = Arc::new(RequestLogger::new());
        chain.add_middleware(mw).unwrap();
        assert!(chain.remove_middleware("request_logger"));
        assert_eq!(chain.get_chain().len(), 0);
        assert!(!chain.remove_middleware("nonexistent"));
    }

    #[test]
    fn test_process_with_logger() {
        let chain = MiddlewareChain::new();
        chain.add_middleware(Arc::new(RequestLogger::new())).unwrap();
        let mut ctx = MiddlewareContext::new("test-123");
        let req = make_request("GET", "/v1/models");

        let result = chain.process(&mut ctx, &req);
        assert!(result.is_none(), "Logger should not stop the request");
        assert_eq!(ctx.get_attribute("method"), Some("GET"));
        assert_eq!(ctx.get_attribute("uri"), Some("/v1/models"));
    }

    #[test]
    fn test_process_with_validator_blocks_bad_method() {
        let chain = MiddlewareChain::new();
        chain.add_middleware(Arc::new(RequestValidator::new())).unwrap();
        let mut ctx = MiddlewareContext::new("test-456");
        let req = make_request("TRACE", "/v1/models");

        let result = chain.process(&mut ctx, &req);
        assert!(result.is_some(), "Validator should block TRACE method");
    }

    #[test]
    fn test_user_agent_parser() {
        let chain = MiddlewareChain::new();
        chain.add_middleware(Arc::new(UserAgentParser::new())).unwrap();
        let mut ctx = MiddlewareContext::new("test-ua");
        let req = make_request_with_ua("GET", "/v1/chat", "curl/8.0");

        chain.process(&mut ctx, &req);
        assert_eq!(ctx.get_attribute("client_name"), Some("curl"));
        assert_eq!(ctx.get_attribute("client_version"), Some("8.0"));
        assert_eq!(ctx.get_attribute("is_bot"), Some("false"));
    }

    #[test]
    fn test_user_agent_parser_detects_bot() {
        let chain = MiddlewareChain::new();
        chain.add_middleware(Arc::new(UserAgentParser::new())).unwrap();
        let mut ctx = MiddlewareContext::new("test-bot");
        let req = make_request_with_ua("GET", "/", "GoogleBot/2.1");

        chain.process(&mut ctx, &req);
        assert_eq!(ctx.get_attribute("is_bot"), Some("true"));
    }

    #[test]
    fn test_enable_disable_middleware() {
        let chain = MiddlewareChain::new();
        let mw = Arc::new(RequestLogger::new());
        chain.add_middleware(mw).unwrap();

        assert!(chain.disable("request_logger"));
        let info = chain.get_chain_info();
        assert!(!info[0].enabled);

        assert!(chain.enable("request_logger"));
        let info = chain.get_chain_info();
        assert!(info[0].enabled);

        assert!(!chain.enable("nonexistent"));
    }

    #[test]
    fn test_reorder_chain() {
        let chain = MiddlewareChain::new();
        chain.add_middleware(Arc::new(RequestLogger::new())).unwrap();
        chain.add_middleware(Arc::new(UserAgentParser::new())).unwrap();
        chain.add_middleware(Arc::new(RequestValidator::new())).unwrap();

        let original = chain.get_chain();
        let mut reversed = original.clone();
        reversed.reverse();

        assert!(chain.reorder(reversed.clone()).is_ok());
        assert_eq!(chain.get_chain(), reversed);
    }

    #[test]
    fn test_reorder_with_unknown_fails() {
        let chain = MiddlewareChain::new();
        chain.add_middleware(Arc::new(RequestLogger::new())).unwrap();
        assert!(chain.reorder(vec!["nonexistent".to_string()]).is_err());
    }

    #[test]
    fn test_chain_stats() {
        let chain = MiddlewareChain::new();
        chain.add_middleware(Arc::new(RequestLogger::new())).unwrap();
        let mut ctx = MiddlewareContext::new("stat-test");
        let req = make_request("GET", "/");

        chain.process(&mut ctx, &req);
        chain.process(&mut ctx, &req);

        let stats = chain.get_stats();
        assert_eq!(stats.total_processed, 2);
        assert_eq!(stats.total_stopped, 0);
        assert_eq!(stats.middleware_count, 1);
    }
}
