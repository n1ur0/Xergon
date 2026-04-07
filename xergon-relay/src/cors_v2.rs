//! Enhanced CORS configuration v2 — per-path rules, wildcard subdomains
//!
//! Features:
//! - Per-path CORS rules with origin whitelisting
//! - Preflight request handling with caching headers
//! - Wildcard subdomain support (e.g., *.example.com)
//! - Dynamic rule management via REST API

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// CORS origin configuration
// ---------------------------------------------------------------------------

/// Configuration for a single allowed origin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsOrigin {
    /// The origin pattern (e.g., "https://app.example.com" or "*.example.com").
    pub origin: String,
    /// HTTP methods allowed for this origin.
    #[serde(default = "default_methods")]
    pub allowed_methods: Vec<String>,
    /// Headers allowed in requests from this origin.
    #[serde(default = "default_headers")]
    pub allowed_headers: Vec<String>,
    /// Max age for preflight cache (seconds).
    #[serde(default = "default_max_age")]
    pub max_age: u64,
    /// Whether to allow credentials (cookies, auth headers).
    #[serde(default)]
    pub allow_credentials: bool,
    /// Headers the browser is allowed to access in the response.
    #[serde(default = "default_exposed_headers")]
    pub exposed_headers: Vec<String>,
}

fn default_methods() -> Vec<String> {
    vec![
        "GET".to_string(),
        "POST".to_string(),
        "PUT".to_string(),
        "DELETE".to_string(),
        "OPTIONS".to_string(),
        "PATCH".to_string(),
    ]
}

fn default_headers() -> Vec<String> {
    vec![
        "Content-Type".to_string(),
        "Authorization".to_string(),
        "X-Request-Id".to_string(),
        "X-API-Key".to_string(),
    ]
}

fn default_max_age() -> u64 {
    3600
}

fn default_exposed_headers() -> Vec<String> {
    vec![
        "X-Request-Id".to_string(),
        "X-RateLimit-Remaining".to_string(),
    ]
}

impl CorsOrigin {
    /// Create a new CORS origin with defaults.
    pub fn new(origin: impl Into<String>) -> Self {
        Self {
            origin: origin.into(),
            allowed_methods: default_methods(),
            allowed_headers: default_headers(),
            max_age: default_max_age(),
            allow_credentials: false,
            exposed_headers: default_exposed_headers(),
        }
    }

    /// Create a wildcard origin (matches any subdomain).
    pub fn wildcard(domain: impl Into<String>) -> Self {
        Self::new(format!("*.{}", domain.into()))
    }

    /// Check if this origin matches the given request origin.
    pub fn matches(&self, request_origin: &str) -> bool {
        let pattern = &self.origin;
        if pattern == "*" {
            return true;
        }
        if pattern == request_origin {
            return true;
        }
        // Wildcard subdomain matching: *.example.com matches sub.example.com
        if let Some(wildcard_domain) = pattern.strip_prefix("*.") {
            if let Some(dot_pos) = request_origin.find("://") {
                let after_scheme = &request_origin[dot_pos + 3..];
                if let Some(host_end) = after_scheme.find('/') {
                    let host = &after_scheme[..host_end];
                    return host.ends_with(wildcard_domain)
                        && host.len() > wildcard_domain.len()
                        && host.as_bytes()[host.len() - wildcard_domain.len() - 1] == b'.';
                } else {
                    return after_scheme.ends_with(wildcard_domain)
                        && after_scheme.len() > wildcard_domain.len()
                        && after_scheme.as_bytes()[after_scheme.len() - wildcard_domain.len() - 1]
                            == b'.';
                }
            }
        }
        false
    }
}

// ---------------------------------------------------------------------------
// CORS rule
// ---------------------------------------------------------------------------

/// A CORS rule that applies to specific paths with a set of allowed origins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsRule {
    /// Unique identifier for this rule.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// List of allowed origins for this rule.
    pub origins: Vec<CorsOrigin>,
    /// Path patterns this rule applies to (e.g., ["/v1/*", "/api/*"]).
    #[serde(default)]
    pub paths: Vec<String>,
    /// Whether this rule is active.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Priority — lower numbers checked first.
    #[serde(default)]
    pub priority: u32,
}

fn default_true() -> bool {
    true
}

impl CorsRule {
    /// Create a new CORS rule.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            origins: Vec::new(),
            paths: vec!["/*".to_string()],
            enabled: true,
            priority: 100,
        }
    }

    /// Add an origin to this rule.
    pub fn with_origin(mut self, origin: CorsOrigin) -> Self {
        self.origins.push(origin);
        self
    }

    /// Add a path pattern to this rule.
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.paths.push(path.into());
        self
    }

    /// Check if this rule matches a given request path.
    pub fn matches_path(&self, request_path: &str) -> bool {
        for pattern in &self.paths {
            if glob_match(pattern, request_path) {
                return true;
            }
        }
        false
    }

    /// Find a matching origin for the given request origin.
    pub fn find_matching_origin(&self, request_origin: &str) -> Option<&CorsOrigin> {
        self.origins.iter().find(|o| o.matches(request_origin))
    }
}

/// Simple glob matching: * matches any sequence of characters.
fn glob_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" || pattern == "/*" {
        return true;
    }
    if pattern == text {
        return true;
    }
    // Handle /v1/* style patterns
    if let Some(prefix) = pattern.strip_suffix("/*") {
        return text.starts_with(prefix) && (text.len() == prefix.len() || text[prefix.len()..].starts_with('/'));
    }
    // Handle /v1/chat/* style patterns
    if let Some(prefix) = pattern.strip_suffix('*') {
        return text.starts_with(prefix);
    }
    false
}

// ---------------------------------------------------------------------------
// CORS configuration
// ---------------------------------------------------------------------------

/// Global CORS manager configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsConfig {
    /// Default origins to allow when no rule matches.
    #[serde(default = "default_allow_origins")]
    pub default_allow_origins: Vec<String>,
    /// Default max age for preflight responses (seconds).
    #[serde(default = "default_preflight_max_age")]
    pub default_max_age: u64,
    /// Preflight max age (seconds).
    #[serde(default = "default_preflight_max_age")]
    pub preflight_max_age: u64,
    /// Whether to support wildcard subdomains.
    #[serde(default = "default_true")]
    pub wildcard_subdomains: bool,
    /// Whether CORS is globally enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_allow_origins() -> Vec<String> {
    vec!["*".to_string()]
}

fn default_preflight_max_age() -> u64 {
    3600
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            default_allow_origins: default_allow_origins(),
            default_max_age: 3600,
            preflight_max_age: 3600,
            wildcard_subdomains: true,
            enabled: true,
        }
    }
}

// ---------------------------------------------------------------------------
// CorsManagerV2 — main struct
// ---------------------------------------------------------------------------

/// Enhanced CORS manager with per-path rules and wildcard support.
#[derive(Clone)]
pub struct CorsManagerV2 {
    /// Rules registry, keyed by rule ID.
    rules: Arc<DashMap<String, CorsRule>>,
    /// Configuration.
    config: Arc<std::sync::RwLock<CorsConfig>>,
}

impl CorsManagerV2 {
    /// Create a new CORS manager with default config.
    pub fn new() -> Self {
        Self::with_config(CorsConfig::default())
    }

    /// Create a new CORS manager with the given config.
    pub fn with_config(config: CorsConfig) -> Self {
        Self {
            rules: Arc::new(DashMap::new()),
            config: Arc::new(std::sync::RwLock::new(config)),
        }
    }

    /// Check if a request origin is allowed for the given path.
    ///
    /// Returns the matching origin config if found.
    pub fn check_cors(&self, request_origin: &str, request_path: &str) -> Option<CorsOrigin> {
        let cfg = self.config.read().unwrap();
        if !cfg.enabled {
            return None;
        }
        drop(cfg);

        // Check rules in priority order
        let mut sorted_rules: Vec<CorsRule> = self
            .rules
            .iter()
            .filter(|r| r.value().enabled)
            .map(|r| r.value().clone())
            .collect();
        sorted_rules.sort_by_key(|r| r.priority);

        for rule in &sorted_rules {
            if rule.matches_path(request_path) {
                if let Some(origin) = rule.find_matching_origin(request_origin) {
                    return Some(origin.clone());
                }
            }
        }

        // Check default origins
        let cfg = self.config.read().unwrap();
        for default_origin in &cfg.default_allow_origins {
            let cors_origin = CorsOrigin::new(default_origin.as_str());
            if cors_origin.matches(request_origin) {
                return Some(cors_origin);
            }
        }

        None
    }

    /// Add a CORS rule.
    pub fn add_rule(&self, rule: CorsRule) {
        self.rules.insert(rule.id.clone(), rule);
    }

    /// Remove a CORS rule by ID.
    pub fn remove_rule(&self, id: &str) -> bool {
        self.rules.remove(id).is_some()
    }

    /// Get a rule by ID.
    pub fn get_rule(&self, id: &str) -> Option<CorsRule> {
        self.rules.get(id).map(|r| r.value().clone())
    }

    /// List all CORS rules.
    pub fn list_rules(&self) -> Vec<CorsRule> {
        let mut rules: Vec<CorsRule> = self.rules.iter().map(|r| r.value().clone()).collect();
        rules.sort_by_key(|r| r.priority);
        rules
    }

    /// Generate a preflight response for the given origin and path.
    pub fn get_preflight_response(
        &self,
        request_origin: &str,
        request_path: &str,
        request_method: &str,
        request_headers: &[String],
    ) -> Option<Response> {
        let origin_config = self.check_cors(request_origin, request_path)?;

        // Check if the requested method is allowed
        let method_upper = request_method.to_uppercase();
        if !origin_config.allowed_methods.contains(&method_upper)
            && !origin_config.allowed_methods.contains(&"*".to_string())
        {
            return None;
        }

        // Build preflight response headers
        let mut headers = vec![
            ("Access-Control-Allow-Origin".to_string(), request_origin.to_string()),
            (
                "Access-Control-Allow-Methods".to_string(),
                origin_config.allowed_methods.join(", "),
            ),
            (
                "Access-Control-Allow-Headers".to_string(),
                origin_config.allowed_headers.join(", "),
            ),
            (
                "Access-Control-Max-Age".to_string(),
                origin_config.max_age.to_string(),
            ),
        ];

        if origin_config.allow_credentials {
            headers.push((
                "Access-Control-Allow-Credentials".to_string(),
                "true".to_string(),
            ));
        }

        if !origin_config.exposed_headers.is_empty() {
            headers.push((
                "Access-Control-Expose-Headers".to_string(),
                origin_config.exposed_headers.join(", "),
            ));
        }

        let mut response = StatusCode::NO_CONTENT.into_response();
        for (key, value) in headers {
            if let (Ok(k), Ok(v)) = (
                key.parse::<axum::http::HeaderName>(),
                value.parse::<axum::http::HeaderValue>(),
            ) {
                response.headers_mut().insert(k, v);
            }
        }

        Some(response)
    }

    /// Generate CORS headers for a non-preflight response.
    pub fn get_cors_headers(
        &self,
        request_origin: &str,
        request_path: &str,
    ) -> Option<Vec<(String, String)>> {
        let origin_config = self.check_cors(request_origin, request_path)?;

        let mut headers = vec![
            ("Access-Control-Allow-Origin".to_string(), request_origin.to_string()),
        ];

        if origin_config.allow_credentials {
            headers.push((
                "Access-Control-Allow-Credentials".to_string(),
                "true".to_string(),
            ));
        }

        if !origin_config.exposed_headers.is_empty() {
            headers.push((
                "Access-Control-Expose-Headers".to_string(),
                origin_config.exposed_headers.join(", "),
            ));
        }

        Some(headers)
    }

    /// Get the current configuration.
    pub fn get_config(&self) -> CorsConfig {
        self.config.read().unwrap().clone()
    }

    /// Update the configuration.
    pub fn update_config(&self, new_config: CorsConfig) {
        let mut cfg = self.config.write().unwrap();
        *cfg = new_config;
        info!("CORS config updated");
    }
}

impl Default for CorsManagerV2 {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// REST request/response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct AddRuleRequest {
    pub id: String,
    pub name: String,
    pub origins: Vec<CorsOrigin>,
    #[serde(default = "vec_default_paths")]
    pub paths: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub priority: u32,
}

fn vec_default_paths() -> Vec<String> {
    vec!["/*".to_string()]
}

#[derive(Debug, Serialize)]
pub struct RuleAddedResponse {
    pub id: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct RuleDeletedResponse {
    pub id: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct RulesListResponse {
    pub rules: Vec<CorsRule>,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct CorsConfigResponse {
    pub config: CorsConfig,
}

#[derive(Debug, Deserialize)]
pub struct PreflightRequest {
    pub origin: String,
    pub path: String,
    pub method: String,
    #[serde(default)]
    pub headers: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CorsCheckResponse {
    pub allowed: bool,
    pub origin: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConfigRequest {
    pub enabled: Option<bool>,
    pub default_allow_origins: Option<Vec<String>>,
    pub default_max_age: Option<u64>,
    pub preflight_max_age: Option<u64>,
    pub wildcard_subdomains: Option<bool>,
}

// ---------------------------------------------------------------------------
// REST handlers
// ---------------------------------------------------------------------------

async fn list_rules_handler(State(state): State<AppState>) -> impl IntoResponse {
    let rules = state.cors_manager_v2.list_rules();
    let count = rules.len();
    (StatusCode::OK, Json(RulesListResponse { rules, count }))
}

async fn add_rule_handler(
    State(state): State<AppState>,
    Json(body): Json<AddRuleRequest>,
) -> impl IntoResponse {
    let rule = CorsRule {
        id: body.id.clone(),
        name: body.name,
        origins: body.origins,
        paths: body.paths,
        enabled: body.enabled,
        priority: body.priority,
    };
    let id = rule.id.clone();
    state.cors_manager_v2.add_rule(rule);
    info!(rule_id = %id, "CORS rule added");
    (StatusCode::CREATED, Json(RuleAddedResponse { id, status: "created".to_string() }))
}

async fn delete_rule_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let removed = state.cors_manager_v2.remove_rule(&id);
    if removed {
        info!(rule_id = %id, "CORS rule removed");
        (StatusCode::OK, Json(RuleDeletedResponse { id, status: "deleted".to_string() }))
    } else {
        (StatusCode::NOT_FOUND, Json(RuleDeletedResponse { id, status: "not_found".to_string() }))
    }
}

async fn preflight_handler(
    State(state): State<AppState>,
    Json(body): Json<PreflightRequest>,
) -> impl IntoResponse {
    match state.cors_manager_v2.get_preflight_response(
        &body.origin,
        &body.path,
        &body.method,
        &body.headers,
    ) {
        Some(response) => response,
        None => (StatusCode::FORBIDDEN, Json(serde_json::json!({ "error": "Origin not allowed" }))).into_response(),
    }
}

async fn check_cors_handler(
    State(state): State<AppState>,
    Json(body): Json<PreflightRequest>,
) -> impl IntoResponse {
    let result = state.cors_manager_v2.check_cors(&body.origin, &body.path);
    let allowed = result.is_some();
    let origin = result.map(|o| o.origin);
    (StatusCode::OK, Json(CorsCheckResponse { allowed, origin }))
}

async fn get_config_handler(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.cors_manager_v2.get_config();
    (StatusCode::OK, Json(CorsConfigResponse { config }))
}

async fn update_config_handler(
    State(state): State<AppState>,
    Json(body): Json<UpdateConfigRequest>,
) -> impl IntoResponse {
    let mut config = state.cors_manager_v2.get_config();
    if let Some(enabled) = body.enabled {
        config.enabled = enabled;
    }
    if let Some(origins) = body.default_allow_origins {
        config.default_allow_origins = origins;
    }
    if let Some(max_age) = body.default_max_age {
        config.default_max_age = max_age;
    }
    if let Some(preflight) = body.preflight_max_age {
        config.preflight_max_age = preflight;
    }
    if let Some(wildcard) = body.wildcard_subdomains {
        config.wildcard_subdomains = wildcard;
    }
    state.cors_manager_v2.update_config(config.clone());
    info!("CORS config updated via API");
    (StatusCode::OK, Json(CorsConfigResponse { config }))
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the CORS v2 API router.
pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/v1/cors/rules", get(list_rules_handler))
        .route("/v1/cors/rules", post(add_rule_handler))
        .route("/v1/cors/rules/{id}", delete(delete_rule_handler))
        .route("/v1/cors/preflight", post(preflight_handler))
        .route("/v1/cors/check", post(check_cors_handler))
        .route("/v1/cors/config", get(get_config_handler))
        .route("/v1/cors/config", put(update_config_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_empty_manager() {
        let manager = CorsManagerV2::new();
        assert_eq!(manager.list_rules().len(), 0);
    }

    #[test]
    fn test_add_and_remove_rule() {
        let manager = CorsManagerV2::new();
        let rule = CorsRule::new("rule-1", "Test Rule")
            .with_origin(CorsOrigin::new("https://app.example.com"))
            .with_path("/v1/*");
        manager.add_rule(rule);
        assert_eq!(manager.list_rules().len(), 1);

        assert!(manager.remove_rule("rule-1"));
        assert_eq!(manager.list_rules().len(), 0);
        assert!(!manager.remove_rule("nonexistent"));
    }

    #[test]
    fn test_exact_origin_match() {
        let origin = CorsOrigin::new("https://app.example.com");
        assert!(origin.matches("https://app.example.com"));
        assert!(!origin.matches("https://other.example.com"));
    }

    #[test]
    fn test_wildcard_origin_match() {
        let origin = CorsOrigin::wildcard("example.com");
        assert!(origin.matches("https://app.example.com"));
        assert!(origin.matches("https://api.example.com"));
        assert!(origin.matches("https://sub.sub.example.com"));
        assert!(!origin.matches("https://example.com")); // bare domain
        assert!(!origin.matches("https://other.com"));
    }

    #[test]
    fn test_star_origin_matches_all() {
        let origin = CorsOrigin::new("*");
        assert!(origin.matches("https://anything.com"));
        assert!(origin.matches("http://localhost:3000"));
    }

    #[test]
    fn test_path_matching() {
        let rule = CorsRule::new("path-test", "Path Test")
            .with_origin(CorsOrigin::new("https://app.example.com"))
            .with_path("/v1/*");

        assert!(rule.matches_path("/v1/chat/completions"));
        assert!(rule.matches_path("/v1/models"));
        assert!(!rule.matches_path("/api/models"));
        assert!(!rule matches_path("/v1extra"));
    }

    #[test]
    fn test_check_cors_with_rule() {
        let manager = CorsManagerV2::new();
        let rule = CorsRule::new("cors-1", "CORS Test")
            .with_origin(CorsOrigin::new("https://app.example.com"))
            .with_path("/v1/*");
        manager.add_rule(rule);

        assert!(manager.check_cors("https://app.example.com", "/v1/chat").is_some());
        assert!(manager.check_cors("https://evil.com", "/v1/chat").is_none());
    }

    #[test]
    fn test_check_cors_default_origin() {
        let manager = CorsManagerV2::new();
        // Default config allows "*" which matches everything
        assert!(manager.check_cors("https://anything.com", "/any/path").is_some());
    }

    #[test]
    fn test_preflight_response() {
        let manager = CorsManagerV2::new();
        let rule = CorsRule::new("preflight-1", "Preflight Test")
            .with_origin(CorsOrigin::new("https://app.example.com"))
            .with_path("/v1/*");
        manager.add_rule(rule);

        let response = manager.get_preflight_response(
            "https://app.example.com",
            "/v1/chat",
            "POST",
            &["Content-Type".to_string()],
        );
        assert!(response.is_some());

        let response = response.unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[test]
    fn test_preflight_rejected_for_disallowed_method() {
        let manager = CorsManagerV2::new();
        let mut origin = CorsOrigin::new("https://app.example.com");
        origin.allowed_methods = vec!["GET".to_string()];
        let rule = CorsRule::new("method-test", "Method Test")
            .with_origin(origin)
            .with_path("/v1/*");
        manager.add_rule(rule);

        let response = manager.get_preflight_response(
            "https://app.example.com",
            "/v1/chat",
            "DELETE",
            &[],
        );
        assert!(response.is_none());
    }

    #[test]
    fn test_cors_headers_for_response() {
        let manager = CorsManagerV2::new();
        let headers = manager.get_cors_headers("https://app.example.com", "/v1/chat");
        assert!(headers.is_some());

        let headers = headers.unwrap();
        assert!(headers.iter().any(|(k, _)| k == "Access-Control-Allow-Origin"));
    }

    #[test]
    fn test_disabled_cors_blocks_all() {
        let manager = CorsManagerV2::new();
        manager.update_config(CorsConfig {
            enabled: false,
            ..CorsConfig::default()
        });

        assert!(manager.check_cors("https://anything.com", "/any/path").is_none());
    }
}
