//! Inference request gateway with load balancing, failover, and health tracking.
//!
//! Provides:
//! - Multiple load balancing strategies (round-robin, weighted, least connections,
//!   latency-based, cost-optimized)
//! - Automatic failover to healthy providers
//! - Retry with exponential backoff
//! - Provider health tracking
//! - REST API endpoints for gateway management

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A provider that can serve inference requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayProvider {
    /// Unique provider identifier.
    pub provider_id: String,
    /// The endpoint URL for inference requests.
    pub endpoint: String,
    /// Load balancing weight (higher = more traffic).
    pub weight: f64,
    /// Health score (0.0-1.0, 1.0 = fully healthy).
    pub health_score: f64,
    /// Whether the provider is active and eligible for routing.
    pub active: bool,
    /// Provider region (e.g., "us-east", "eu-west").
    pub region: String,
    /// Capabilities this provider supports (e.g., ["llama-3.1-8b", "mixtral"]).
    pub capabilities: Vec<String>,
    /// Total requests handled.
    pub total_requests: u64,
    /// Total successful requests.
    pub successful_requests: u64,
    /// Total failed requests.
    pub failed_requests: u64,
    /// Average latency in milliseconds.
    pub avg_latency_ms: f64,
    /// Current active connections.
    pub active_connections: u64,
    /// Cost per 1M tokens.
    pub cost_per_million_tokens: f64,
}

/// Load balancing strategy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LoadBalanceStrategy {
    /// Distribute requests sequentially across providers.
    RoundRobin,
    /// Distribute based on provider weights.
    Weighted,
    /// Route to provider with fewest active connections.
    LeastConnections,
    /// Route to provider with lowest average latency.
    LatencyBased,
    /// Route to provider with lowest cost.
    CostOptimized,
}

/// Retry policy for failed requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts.
    pub max_retries: u32,
    /// Initial backoff delay in milliseconds.
    pub backoff_ms: u64,
    /// Backoff multiplier for each retry.
    pub backoff_multiplier: f64,
    /// Error codes/types that are retryable.
    pub retryable_errors: Vec<String>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            backoff_ms: 100,
            backoff_multiplier: 2.0,
            retryable_errors: vec![
                "timeout".to_string(),
                "connection_error".to_string(),
                "server_error".to_string(),
                "rate_limited".to_string(),
            ],
        }
    }
}

/// An inference request to be routed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayRequest {
    /// Unique request identifier.
    pub request_id: String,
    /// The model to use for inference.
    pub model_id: String,
    /// The prompt text.
    pub prompt: String,
    /// Additional inference parameters.
    pub parameters: HashMap<String, serde_json::Value>,
    /// Request timeout in milliseconds.
    pub timeout_ms: u64,
    /// Request priority (lower = higher priority).
    pub priority: i32,
}

/// Response from an inference provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayResponse {
    /// The request ID.
    pub request_id: String,
    /// The provider that served the request.
    pub provider_id: String,
    /// The inference output.
    pub output: String,
    /// Latency in milliseconds.
    pub latency_ms: f64,
    /// Tokens used (prompt + completion).
    pub tokens_used: u64,
    /// Cost of the request.
    pub cost: f64,
}

/// Gateway statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayStats {
    /// Total providers.
    pub total_providers: usize,
    /// Active providers.
    pub active_providers: usize,
    /// Current load balancing strategy.
    pub strategy: LoadBalanceStrategy,
    /// Total requests routed.
    pub total_requests: u64,
    /// Total successful requests.
    pub successful_requests: u64,
    /// Total failed requests.
    pub failed_requests: u64,
    /// Average latency across all providers.
    pub avg_latency_ms: f64,
    /// Retry policy.
    pub retry_policy: RetryPolicy,
}

/// Health information for a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderHealth {
    pub provider_id: String,
    pub endpoint: String,
    pub health_score: f64,
    pub active: bool,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub avg_latency_ms: f64,
    pub active_connections: u64,
    pub success_rate: f64,
}

// ---------------------------------------------------------------------------
// InferenceGateway
// ---------------------------------------------------------------------------

/// DashMap-backed inference gateway with load balancing and failover.
#[derive(Debug, Clone)]
pub struct InferenceGateway {
    /// Registered providers keyed by provider_id.
    providers: Arc<DashMap<String, GatewayProvider>>,
    /// Current load balancing strategy.
    strategy: Arc<std::sync::RwLock<LoadBalanceStrategy>>,
    /// Round-robin counter.
    rr_counter: Arc<AtomicU64>,
    /// Retry policy.
    retry_policy: Arc<std::sync::RwLock<RetryPolicy>>,
    /// Total requests routed.
    total_requests: Arc<AtomicU64>,
    /// Total successful requests.
    successful_requests: Arc<AtomicU64>,
    /// Total failed requests.
    failed_requests: Arc<AtomicU64>,
}

impl InferenceGateway {
    /// Create a new inference gateway with default settings.
    pub fn new() -> Self {
        Self {
            providers: Arc::new(DashMap::new()),
            strategy: Arc::new(std::sync::RwLock::new(LoadBalanceStrategy::RoundRobin)),
            rr_counter: Arc::new(AtomicU64::new(0)),
            retry_policy: Arc::new(std::sync::RwLock::new(RetryPolicy::default())),
            total_requests: Arc::new(AtomicU64::new(0)),
            successful_requests: Arc::new(AtomicU64::new(0)),
            failed_requests: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Create a new inference gateway with a specific strategy.
    pub fn with_strategy(strategy: LoadBalanceStrategy) -> Self {
        let gw = Self::new();
        *gw.strategy.write().unwrap() = strategy;
        gw
    }

    /// Add a provider to the gateway.
    pub fn add_provider(&self, provider: GatewayProvider) -> Result<(), String> {
        if self.providers.contains_key(&provider.provider_id) {
            return Err(format!("Provider '{}' already exists", provider.provider_id));
        }

        self.providers.insert(provider.provider_id.clone(), provider);
        Ok(())
    }

    /// Remove a provider from the gateway.
    pub fn remove_provider(&self, provider_id: &str) -> Result<(), String> {
        if self.providers.remove(provider_id).is_none() {
            return Err(format!("Provider '{}' not found", provider_id));
        }
        Ok(())
    }

    /// Update a provider's health score.
    pub fn update_health(&self, provider_id: &str, health_score: f64) -> Result<(), String> {
        let mut provider = self
            .providers
            .get_mut(provider_id)
            .ok_or_else(|| format!("Provider '{}' not found", provider_id))?;

        provider.health_score = health_score.clamp(0.0, 1.0);
        provider.active = health_score > 0.3; // Auto-deactivate below 0.3

        Ok(())
    }

    /// Get the current load balancing strategy.
    pub fn get_strategy(&self) -> LoadBalanceStrategy {
        self.strategy.read().unwrap().clone()
    }

    /// Update the load balancing strategy.
    pub fn update_strategy(&self, strategy: LoadBalanceStrategy) {
        *self.strategy.write().unwrap() = strategy;
    }

    /// Get the retry policy.
    pub fn get_retry_policy(&self) -> RetryPolicy {
        self.retry_policy.read().unwrap().clone()
    }

    /// Update the retry policy.
    pub fn update_retry_policy(&self, policy: RetryPolicy) {
        *self.retry_policy.write().unwrap() = policy;
    }

    /// Select a provider using the current load balancing strategy.
    fn select_provider(&self, model_id: &str) -> Result<String, String> {
        let strategy = self.strategy.read().unwrap().clone();

        // Filter to active providers that support the model
        let eligible: Vec<(String, GatewayProvider)> = self
            .providers
            .iter()
            .filter(|entry| {
                let p = entry.value();
                p.active
                    && (p.capabilities.is_empty() || p.capabilities.contains(&model_id.to_string()))
            })
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();

        if eligible.is_empty() {
            return Err("No eligible providers available".to_string());
        }

        match strategy {
            LoadBalanceStrategy::RoundRobin => {
                let idx = self.rr_counter.fetch_add(1, Ordering::SeqCst) as usize;
                let provider = &eligible[idx % eligible.len()];
                Ok(provider.0.clone())
            }
            LoadBalanceStrategy::Weighted => {
                let total_weight: f64 = eligible.iter().map(|(_, p)| p.weight).sum();
                if total_weight <= 0.0 {
                    // Fallback to round-robin if all weights are 0
                    let idx = self.rr_counter.fetch_add(1, Ordering::SeqCst) as usize;
                    return Ok(eligible[idx % eligible.len()].0.clone());
                }

                let hash = self.rr_counter.fetch_add(1, Ordering::SeqCst);
                let threshold = (hash as f64 % 10000.0) / 10000.0 * total_weight;

                let mut cumulative = 0.0;
                for (id, provider) in &eligible {
                    cumulative += provider.weight;
                    if cumulative >= threshold {
                        return Ok(id.clone());
                    }
                }
                Ok(eligible.last().unwrap().0.clone())
            }
            LoadBalanceStrategy::LeastConnections => {
                let best = eligible
                    .iter()
                    .min_by_key(|(_, p)| p.active_connections)
                    .unwrap();
                Ok(best.0.clone())
            }
            LoadBalanceStrategy::LatencyBased => {
                let best = eligible
                    .iter()
                    .filter(|(_, p)| p.avg_latency_ms > 0.0)
                    .min_by(|(_, a), (_, b)| {
                        a.avg_latency_ms
                            .partial_cmp(&b.avg_latency_ms)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .or_else(|| eligible.first());

                match best {
                    Some((id, _)) => Ok(id.clone()),
                    None => Err("No providers with latency data".to_string()),
                }
            }
            LoadBalanceStrategy::CostOptimized => {
                let best = eligible
                    .iter()
                    .filter(|(_, p)| p.cost_per_million_tokens > 0.0)
                    .min_by(|(_, a), (_, b)| {
                        a.cost_per_million_tokens
                            .partial_cmp(&b.cost_per_million_tokens)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .or_else(|| eligible.first());

                match best {
                    Some((id, _)) => Ok(id.clone()),
                    None => Err("No providers with cost data".to_string()),
                }
            }
        }
    }

    /// Record a successful request for a provider.
    fn record_success(&self, provider_id: &str, latency_ms: f64) {
        self.successful_requests.fetch_add(1, Ordering::SeqCst);

        if let Some(mut provider) = self.providers.get_mut(provider_id) {
            provider.successful_requests += 1;
            provider.total_requests += 1;
            // Update moving average latency
            let n = provider.successful_requests as f64;
            provider.avg_latency_ms =
                (provider.avg_latency_ms * (n - 1.0) + latency_ms) / n;
        }
    }

    /// Record a failed request for a provider.
    fn record_failure(&self, provider_id: &str) {
        self.failed_requests.fetch_add(1, Ordering::SeqCst);

        if let Some(mut provider) = self.providers.get_mut(provider_id) {
            provider.failed_requests += 1;
            provider.total_requests += 1;
            // Degrade health on failure
            provider.health_score = (provider.health_score - 0.05).max(0.0);
            if provider.health_score < 0.3 {
                provider.active = false;
            }
        }
    }

    /// Route an inference request to an appropriate provider.
    ///
    /// Handles provider selection, failover, and retries.
    pub fn route_request(&self, request: &GatewayRequest) -> Result<GatewayResponse, String> {
        self.total_requests.fetch_add(1, Ordering::SeqCst);

        let retry_policy = self.retry_policy.read().unwrap().clone();
        let mut last_error = String::new();

        // Collect eligible providers sorted by health
        let mut providers: Vec<String> = self
            .providers
            .iter()
            .filter(|entry| {
                let p = entry.value();
                p.active
                    && (p.capabilities.is_empty()
                        || p.capabilities.contains(&request.model_id))
            })
            .map(|entry| entry.key().clone())
            .collect();

        // Sort by health score descending (try healthiest first)
        providers.sort_by(|a, b| {
            let ha = self
                .providers
                .get(a)
                .map(|p| p.health_score)
                .unwrap_or(0.0);
            let hb = self
                .providers
                .get(b)
                .map(|p| p.health_score)
                .unwrap_or(0.0);
            hb.partial_cmp(&ha).unwrap_or(std::cmp::Ordering::Equal)
        });

        for attempt in 0..=retry_policy.max_retries {
            // Select provider
            let provider_id = match self.select_provider(&request.model_id) {
                Ok(id) => id,
                Err(e) => {
                    last_error = e;
                    continue;
                }
            };

            // Increment active connections
            if let Some(mut provider) = self.providers.get_mut(&provider_id) {
                provider.active_connections += 1;
            }

            // Simulate routing (in a real impl, this would make an HTTP request)
            // For now, check if provider is healthy enough
            let health = self
                .providers
                .get(&provider_id)
                .map(|p| p.health_score)
                .unwrap_or(0.0);

            let result = if health > 0.2 {
                // Simulate success with latency
                let latency = self
                    .providers
                    .get(&provider_id)
                    .map(|p| p.avg_latency_ms)
                    .unwrap_or(50.0);
                let cost = self
                    .providers
                    .get(&provider_id)
                    .map(|p| p.cost_per_million_tokens / 1_000_000.0)
                    .unwrap_or(0.001);

                Ok(GatewayResponse {
                    request_id: request.request_id.clone(),
                    provider_id: provider_id.clone(),
                    output: format!("Response for model {} from {}", request.model_id, provider_id),
                    latency_ms: latency,
                    tokens_used: 100,
                    cost,
                })
            } else {
                Err("Provider health too low".to_string())
            };

            // Decrement active connections
            if let Some(mut provider) = self.providers.get_mut(&provider_id) {
                provider.active_connections = provider.active_connections.saturating_sub(1);
            }

            match result {
                Ok(response) => {
                    self.record_success(&provider_id, response.latency_ms);
                    return Ok(response);
                }
                Err(e) => {
                    last_error = e;
                    self.record_failure(&provider_id);

                    // Check if error is retryable
                    let is_retryable = retry_policy
                        .retryable_errors
                        .iter()
                        .any(|re| last_error.to_lowercase().contains(re));

                    if !is_retryable || attempt as u32 >= retry_policy.max_retries {
                        break;
                    }

                    // Calculate backoff delay (would sleep in real impl)
                    let _backoff_ms = retry_policy.backoff_ms as f64
                        * retry_policy.backoff_multiplier.powi(attempt as i32);
                    // In production: tokio::time::sleep(Duration::from_millis(backoff_ms as u64)).await;
                }
            }
        }

        Err(format!(
            "All retry attempts exhausted. Last error: {}",
            last_error
        ))
    }

    /// Manually failover a request from one provider to another.
    pub fn failover(&self, from_provider_id: &str, request: &GatewayRequest) -> Result<GatewayResponse, String> {
        // Mark the failing provider as degraded
        if let Some(mut provider) = self.providers.get_mut(from_provider_id) {
            provider.health_score = (provider.health_score - 0.2).max(0.0);
            if provider.health_score < 0.3 {
                provider.active = false;
            }
        }

        // Try to route to a different provider
        self.route_request(request)
    }

    /// Get health information for a specific provider.
    pub fn get_provider_health(&self, provider_id: &str) -> Result<ProviderHealth, String> {
        let provider = self
            .providers
            .get(provider_id)
            .ok_or_else(|| format!("Provider '{}' not found", provider_id))?;

        let success_rate = if provider.total_requests > 0 {
            provider.successful_requests as f64 / provider.total_requests as f64
        } else {
            1.0
        };

        Ok(ProviderHealth {
            provider_id: provider.provider_id.clone(),
            endpoint: provider.endpoint.clone(),
            health_score: provider.health_score,
            active: provider.active,
            total_requests: provider.total_requests,
            successful_requests: provider.successful_requests,
            failed_requests: provider.failed_requests,
            avg_latency_ms: provider.avg_latency_ms,
            active_connections: provider.active_connections,
            success_rate,
        })
    }

    /// Get health information for all providers.
    pub fn get_all_provider_health(&self) -> Vec<ProviderHealth> {
        self.providers
            .iter()
            .filter_map(|entry| self.get_provider_health(entry.key()).ok())
            .collect()
    }

    /// Get gateway statistics.
    pub fn get_stats(&self) -> GatewayStats {
        let total_providers = self.providers.len();
        let active_providers = self.providers.iter().filter(|p| p.value().active).count();

        let total_reqs = self.total_requests.load(Ordering::SeqCst);
        let success_reqs = self.successful_requests.load(Ordering::SeqCst);
        let failed_reqs = self.failed_requests.load(Ordering::SeqCst);

        let avg_latency: f64 = {
            let providers: Vec<f64> = self
                .providers
                .iter()
                .filter_map(|p| {
                    if p.value().total_requests > 0 {
                        Some(p.value().avg_latency_ms)
                    } else {
                        None
                    }
                })
                .collect();
            if providers.is_empty() {
                0.0
            } else {
                providers.iter().sum::<f64>() / providers.len() as f64
            }
        };

        GatewayStats {
            total_providers,
            active_providers,
            strategy: self.get_strategy(),
            total_requests: total_reqs,
            successful_requests: success_reqs,
            failed_requests: failed_reqs,
            avg_latency_ms: avg_latency,
            retry_policy: self.get_retry_policy(),
        }
    }

    /// List all providers.
    pub fn list_providers(&self) -> Vec<GatewayProvider> {
        self.providers.iter().map(|p| p.value().clone()).collect()
    }

    /// Get a specific provider.
    pub fn get_provider(&self, provider_id: &str) -> Option<GatewayProvider> {
        self.providers.get(provider_id).map(|p| p.clone())
    }
}

impl Default for InferenceGateway {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// REST API
// ---------------------------------------------------------------------------

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Json},
    routing::{delete, get, post, put},
    Router,
};

/// Build the inference gateway router.
pub fn build_gateway_router(state: crate::api::AppState) -> Router {
    Router::new()
        .route("/v1/gateway/inference", post(route_inference_handler))
        .route("/v1/gateway/providers", get(list_providers_handler))
        .route("/v1/gateway/providers", post(add_provider_handler))
        .route("/v1/gateway/providers/{id}", delete(remove_provider_handler))
        .route("/v1/gateway/strategy", get(get_strategy_handler))
        .route("/v1/gateway/strategy", put(update_strategy_handler))
        .route("/v1/gateway/stats", get(get_gateway_stats_handler))
        .with_state(state)
}

#[derive(Debug, Deserialize)]
struct InferenceRequest {
    model_id: String,
    prompt: String,
    #[serde(default)]
    parameters: HashMap<String, serde_json::Value>,
    #[serde(default = "default_timeout")]
    timeout_ms: u64,
    #[serde(default = "default_priority")]
    priority: i32,
}

fn default_timeout() -> u64 {
    30000
}

fn default_priority() -> i32 {
    0
}

#[derive(Debug, Deserialize)]
struct StrategyUpdateRequest {
    strategy: LoadBalanceStrategy,
}

async fn route_inference_handler(
    State(state): State<crate::api::AppState>,
    axum::Json(req): axum::Json<InferenceRequest>,
) -> impl IntoResponse {
    let gateway_req = GatewayRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        model_id: req.model_id,
        prompt: req.prompt,
        parameters: req.parameters,
        timeout_ms: req.timeout_ms,
        priority: req.priority,
    };

    match state.inference_gateway.route_request(&gateway_req) {
        Ok(response) => axum::Json(response).into_response(),
        Err(e) => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

async fn list_providers_handler(
    State(state): State<crate::api::AppState>,
) -> impl IntoResponse {
    axum::Json(state.inference_gateway.list_providers())
}

async fn add_provider_handler(
    State(state): State<crate::api::AppState>,
    axum::Json(provider): axum::Json<GatewayProvider>,
) -> impl IntoResponse {
    match state.inference_gateway.add_provider(provider) {
        Ok(()) => (
            axum::http::StatusCode::CREATED,
            axum::Json(serde_json::json!({"added": true})),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::CONFLICT,
            axum::Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

async fn remove_provider_handler(
    State(state): State<crate::api::AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.inference_gateway.remove_provider(&id) {
        Ok(()) => (
            axum::http::StatusCode::OK,
            axum::Json(serde_json::json!({"removed": true})),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

async fn get_strategy_handler(
    State(state): State<crate::api::AppState>,
) -> impl IntoResponse {
    axum::Json(serde_json::json!({
        "strategy": state.inference_gateway.get_strategy(),
        "retry_policy": state.inference_gateway.get_retry_policy(),
    }))
}

async fn update_strategy_handler(
    State(state): State<crate::api::AppState>,
    axum::Json(req): axum::Json<StrategyUpdateRequest>,
) -> impl IntoResponse {
    state.inference_gateway.update_strategy(req.strategy);
    axum::Json(serde_json::json!({"updated": true}))
}

async fn get_gateway_stats_handler(
    State(state): State<crate::api::AppState>,
) -> impl IntoResponse {
    axum::Json(state.inference_gateway.get_stats())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn create_gateway() -> InferenceGateway {
        InferenceGateway::new()
    }

    fn test_provider(id: &str, weight: f64, health: f64) -> GatewayProvider {
        GatewayProvider {
            provider_id: id.to_string(),
            endpoint: format!("http://localhost:{}", id),
            weight,
            health_score: health,
            active: true,
            region: "us-east".to_string(),
            capabilities: vec!["llama-3.1-8b".to_string()],
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            avg_latency_ms: 50.0,
            active_connections: 0,
            cost_per_million_tokens: 1.0,
        }
    }

    #[test]
    fn test_add_provider() {
        let gw = create_gateway();
        let p = test_provider("p1", 1.0, 1.0);
        gw.add_provider(p.clone()).unwrap();
        assert_eq!(gw.list_providers().len(), 1);
    }

    #[test]
    fn test_add_duplicate_provider_fails() {
        let gw = create_gateway();
        let p = test_provider("p1", 1.0, 1.0);
        gw.add_provider(p.clone()).unwrap();
        assert!(gw.add_provider(p).is_err());
    }

    #[test]
    fn test_remove_provider() {
        let gw = create_gateway();
        gw.add_provider(test_provider("p1", 1.0, 1.0)).unwrap();
        gw.remove_provider("p1").unwrap();
        assert_eq!(gw.list_providers().len(), 0);
    }

    #[test]
    fn test_remove_nonexistent_provider_fails() {
        let gw = create_gateway();
        assert!(gw.remove_provider("nope").is_err());
    }

    #[test]
    fn test_route_request_round_robin() {
        let gw = create_gateway();
        gw.add_provider(test_provider("p1", 1.0, 1.0)).unwrap();
        gw.add_provider(test_provider("p2", 1.0, 1.0)).unwrap();

        let req = GatewayRequest {
            request_id: "r1".into(),
            model_id: "llama-3.1-8b".into(),
            prompt: "Hello".into(),
            parameters: HashMap::new(),
            timeout_ms: 30000,
            priority: 0,
        };

        let r1 = gw.route_request(&req).unwrap();
        let r2 = gw.route_request(&GatewayRequest { request_id: "r2".into(), ..req.clone() }).unwrap();

        // Round-robin should alternate
        assert_ne!(r1.provider_id, r2.provider_id);
    }

    #[test]
    fn test_route_request_no_providers() {
        let gw = create_gateway();
        let req = GatewayRequest {
            request_id: "r1".into(),
            model_id: "llama-3.1-8b".into(),
            prompt: "Hello".into(),
            parameters: HashMap::new(),
            timeout_ms: 30000,
            priority: 0,
        };

        assert!(gw.route_request(&req).is_err());
    }

    #[test]
    fn test_weighted_strategy() {
        let gw = InferenceGateway::with_strategy(LoadBalanceStrategy::Weighted);
        gw.add_provider(test_provider("heavy", 10.0, 1.0)).unwrap();
        gw.add_provider(test_provider("light", 1.0, 1.0)).unwrap();

        let req = GatewayRequest {
            request_id: "r1".into(),
            model_id: "llama-3.1-8b".into(),
            prompt: "Hello".into(),
            parameters: HashMap::new(),
            timeout_ms: 30000,
            priority: 0,
        };

        let mut heavy_count = 0;
        for i in 0..100 {
            let r = gw
                .route_request(&GatewayRequest {
                    request_id: format!("r{}", i),
                    ..req.clone()
                })
                .unwrap();
            if r.provider_id == "heavy" {
                heavy_count += 1;
            }
        }

        // Heavy provider should get significantly more traffic
        assert!(
            heavy_count > 60,
            "Expected heavy provider to get >60%% of traffic, got {}%%",
            heavy_count
        );
    }

    #[test]
    fn test_least_connections_strategy() {
        let gw = InferenceGateway::with_strategy(LoadBalanceStrategy::LeastConnections);
        gw.add_provider(test_provider("busy", 1.0, 1.0)).unwrap();
        gw.add_provider(test_provider("idle", 1.0, 1.0)).unwrap();

        // Simulate busy provider having connections
        if let Some(mut p) = gw.providers.get_mut("busy") {
            p.active_connections = 10;
        }

        let req = GatewayRequest {
            request_id: "r1".into(),
            model_id: "llama-3.1-8b".into(),
            prompt: "Hello".into(),
            parameters: HashMap::new(),
            timeout_ms: 30000,
            priority: 0,
        };

        let r = gw.route_request(&req).unwrap();
        assert_eq!(r.provider_id, "idle");
    }

    #[test]
    fn test_latency_based_strategy() {
        let gw = InferenceGateway::with_strategy(LoadBalanceStrategy::LatencyBased);
        gw.add_provider(test_provider("slow", 1.0, 1.0)).unwrap();
        gw.add_provider(test_provider("fast", 1.0, 1.0)).unwrap();

        if let Some(mut p) = gw.providers.get_mut("slow") {
            p.avg_latency_ms = 500.0;
        }
        if let Some(mut p) = gw.providers.get_mut("fast") {
            p.avg_latency_ms = 10.0;
        }

        let req = GatewayRequest {
            request_id: "r1".into(),
            model_id: "llama-3.1-8b".into(),
            prompt: "Hello".into(),
            parameters: HashMap::new(),
            timeout_ms: 30000,
            priority: 0,
        };

        let r = gw.route_request(&req).unwrap();
        assert_eq!(r.provider_id, "fast");
    }

    #[test]
    fn test_cost_optimized_strategy() {
        let gw = InferenceGateway::with_strategy(LoadBalanceStrategy::CostOptimized);
        gw.add_provider(test_provider("expensive", 1.0, 1.0)).unwrap();
        gw.add_provider(test_provider("cheap", 1.0, 1.0)).unwrap();

        if let Some(mut p) = gw.providers.get_mut("expensive") {
            p.cost_per_million_tokens = 10.0;
        }
        if let Some(mut p) = gw.providers.get_mut("cheap") {
            p.cost_per_million_tokens = 0.5;
        }

        let req = GatewayRequest {
            request_id: "r1".into(),
            model_id: "llama-3.1-8b".into(),
            prompt: "Hello".into(),
            parameters: HashMap::new(),
            timeout_ms: 30000,
            priority: 0,
        };

        let r = gw.route_request(&req).unwrap();
        assert_eq!(r.provider_id, "cheap");
    }

    #[test]
    fn test_provider_health_degradation() {
        let gw = create_gateway();
        gw.add_provider(test_provider("p1", 1.0, 1.0)).unwrap();

        // Record several failures
        for _ in 0..15 {
            gw.record_failure("p1");
        }

        let health = gw.get_provider_health("p1").unwrap();
        assert!(health.health_score < 0.3);
        assert!(!health.active);
    }

    #[test]
    fn test_failover() {
        let gw = create_gateway();
        gw.add_provider(test_provider("p1", 1.0, 1.0)).unwrap();
        gw.add_provider(test_provider("p2", 1.0, 1.0)).unwrap();

        let req = GatewayRequest {
            request_id: "r1".into(),
            model_id: "llama-3.1-8b".into(),
            prompt: "Hello".into(),
            parameters: HashMap::new(),
            timeout_ms: 30000,
            priority: 0,
        };

        let result = gw.failover("p1", &req);
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_stats() {
        let gw = create_gateway();
        gw.add_provider(test_provider("p1", 1.0, 1.0)).unwrap();
        gw.add_provider(test_provider("p2", 1.0, 0.1)).unwrap(); // inactive due to low health

        let stats = gw.get_stats();
        assert_eq!(stats.total_providers, 2);
        assert_eq!(stats.active_providers, 1);
        assert_eq!(stats.strategy, LoadBalanceStrategy::RoundRobin);
    }

    #[test]
    fn test_update_strategy() {
        let gw = create_gateway();
        assert_eq!(gw.get_strategy(), LoadBalanceStrategy::RoundRobin);
        gw.update_strategy(LoadBalanceStrategy::Weighted);
        assert_eq!(gw.get_strategy(), LoadBalanceStrategy::Weighted);
    }

    #[test]
    fn test_capability_filtering() {
        let gw = create_gateway();

        let mut p1 = test_provider("p1", 1.0, 1.0);
        p1.capabilities = vec!["llama-3.1-8b".to_string()];

        let mut p2 = test_provider("p2", 1.0, 1.0);
        p2.capabilities = vec!["mixtral".to_string()];

        gw.add_provider(p1).unwrap();
        gw.add_provider(p2).unwrap();

        // Request for llama should only use p1
        let req = GatewayRequest {
            request_id: "r1".into(),
            model_id: "llama-3.1-8b".into(),
            prompt: "Hello".into(),
            parameters: HashMap::new(),
            timeout_ms: 30000,
            priority: 0,
        };

        let r = gw.route_request(&req).unwrap();
        assert_eq!(r.provider_id, "p1");
    }
}
