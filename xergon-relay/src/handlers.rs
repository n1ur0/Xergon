use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::{get, post},
    Router,
};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::auth::{AuthManager, RateLimiter};
use crate::config::Config;
use crate::heartbeat::{HeartbeatRequest, HeartbeatResponse, ProviderStatus};
use crate::provider::{build_providers, ProviderMap};
use crate::registration::{ProviderRegistry, ProviderRegistration};
use crate::settlement::{SettlementManager, UserBalance};
use crate::types::{ChatCompletionRequest, ChatCompletionResponse, SettlementRequest, SettlementResponse, UsageProof};

pub struct AppState {
    pub config: Config,
    pub providers: ProviderMap,
    pub registry: Arc<RwLock<ProviderRegistry>>,
    pub auth_manager: Arc<AuthManager>,
    pub rate_limiter: Arc<RwLock<RateLimiter>>,
    pub settlement: Arc<RwLock<SettlementManager>>,
}

pub fn create_router(config: Config) -> Router {
    let providers = build_providers(&config.providers);
    let registry = Arc::new(RwLock::new(ProviderRegistry::new()));
    let auth_manager = Arc::new(AuthManager::new());
    let rate_limiter = Arc::new(RwLock::new(RateLimiter::new(60))); // 60 second window
    let db_path = std::env::var("SETTLEMENT_DB_PATH").unwrap_or_else(|_| "data/settlement.db".to_string());
    let settlement = match SettlementManager::new(&db_path) {
        Ok(manager) => Arc::new(RwLock::new(manager)),
        Err(e) => {
            tracing::error!("Failed to initialize settlement manager: {}", e);
            panic!("Failed to initialize settlement manager");
        }
    };

    let state = Arc::new(AppState {
        config,
        providers,
        registry,
        auth_manager,
        rate_limiter,
        settlement,
    });

    Router::new()
        .route("/v1/chat/completions", post(chat_completions))
        .route("/health", get(get_health))
        .route("/register", post(register_provider))
        .route("/heartbeat", post(heartbeat))
        .route("/providers", get(list_providers))
        .route("/settlement/batch", post(submit_settlement_batch))
        .route("/settlement/summary", get(get_settlement_summary))
        .with_state(state)
}

// Provider Registration Endpoint
async fn register_provider(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ProviderRegistration>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let mut registry = state.registry.write().await;
    let response = registry.register(req);

    if response.success {
        Ok(Json(json!({
            "success": true,
            "provider_id": response.provider_id,
            "message": response.message,
            "registered_at": response.registered_at,
            "endpoint": "/heartbeat",
            "heartbeat_interval": "30s"
        })))
    } else {
        Err((StatusCode::BAD_REQUEST, response.message))
    }
}

// Heartbeat Endpoint
async fn heartbeat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<HeartbeatRequest>,
) -> Result<Json<HeartbeatResponse>, (StatusCode, String)> {
    let mut registry = state.registry.write().await;
    let received_at = crate::types::get_current_timestamp();

    let success = registry.update_heartbeat(&req.provider_id, req.pown_score);

    if !success {
        return Err((StatusCode::NOT_FOUND, "Provider not found. Please register first.".to_string()));
    }

    let provider = registry
        .get_provider(&req.provider_id)
        .ok_or_else(|| {
            tracing::error!("Provider not found: {}", req.provider_id);
            (StatusCode::NOT_FOUND, "Provider not found. Please register first.".to_string())
        })?;
    
    Ok(Json(HeartbeatResponse {
        status: "ok".to_string(),
        received_at: Some(received_at),
        provider_status: Some(ProviderStatus {
            provider_id: req.provider_id,
            health_status: format!("{:?}", provider.health_status),
            last_seen: received_at,
            pown_score: req.pown_score,
        }),
    }))
}

// List Providers Endpoint
async fn list_providers(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, String)> {
    let registry = state.registry.read().await;
    let providers: Vec<serde_json::Value> = registry
        .list_providers()
        .iter()
        .map(|p| {
            json!({
                "provider_id": p.provider_id,
                "ergo_address": p.ergo_address,
                "region": p.region,
                "models": p.models,
                "capacity_gpus": p.capacity_gpus,
                "max_concurrent_requests": p.max_concurrent_requests,
                "registered_at": p.registered_at,
                "last_heartbeat": p.last_heartbeat,
                "health_status": format!("{:?}", p.health_status),
                "pown_score": p.pown_score,
            })
        })
        .collect();

    Ok(Json(providers))
}

async fn chat_completions(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<ChatCompletionRequest>,
) -> Result<Json<ChatCompletionResponse>, (StatusCode, String)> {
    // Check rate limit - require API key, no default
    let api_key = headers
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "Missing API key".to_string()))?;

    let mut rate_limiter = state.rate_limiter.write().await;
    let api_key_obj = state.auth_manager.get_api_key(api_key).ok_or_else(|| {
        (StatusCode::UNAUTHORIZED, "Invalid API key".to_string())
    })?;

    if !rate_limiter.check_limit(api_key, api_key_obj.rate_limit) {
        return Err((StatusCode::TOO_MANY_REQUESTS, "Rate limit exceeded".to_string()));
    }

    let model_name = request.model.clone();
    
    // Simple routing: use first available provider
    let provider_id = state.providers.keys().next().cloned();

    let provider_id = match provider_id {
        Some(id) => id,
        None => {
            return Err((StatusCode::SERVICE_UNAVAILABLE, "No providers available".to_string()));
        }
    };

    let provider = state
        .providers
        .get(&provider_id)
        .ok_or_else(|| {
            tracing::error!("Provider not found: {}", provider_id);
            (StatusCode::SERVICE_UNAVAILABLE, "No providers available".to_string())
        })?;

    match provider.chat_completions(request).await {
        Ok(response) => {
            let response_obj: ChatCompletionResponse = serde_json::from_value(response.clone()).unwrap_or_else(|_| {
                ChatCompletionResponse {
                    id: "relay-error".to_string(),
                    object: "chat.completion".to_string(),
                    created: 0,
                    model: model_name.clone(),
                    choices: vec![],
                }
            });
            
            // Record usage for settlement after successful inference
            let tokens_input: u32 = response_obj.choices.iter()
                .map(|c| c.message.content.len() as u32 / 4) // Approximate: 4 chars per token
                .sum();
            let tokens_output: u32 = 100; // Placeholder - would come from actual token counts
            
            let settlement = state.settlement.read().await;
            let _ = settlement.record_usage(api_key, tokens_input, tokens_output, &model_name).await;
            
            Ok(Json(response_obj))
        }
        Err(e) => Err((StatusCode::BAD_GATEWAY, e.to_string())),
    }
}

async fn get_health() -> Json<serde_json::Value> {
    Json(json!({
        "status": "healthy",
        "service": "xergon-relay",
        "version": "0.1.0",
        "features": ["registration", "heartbeat", "authentication", "rate-limiting", "settlement"]
    }))
}

// Settlement Batch Submission Endpoint
async fn submit_settlement_batch(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(request): Json<SettlementRequest>,
) -> Result<Json<SettlementResponse>, (StatusCode, String)> {
    let api_key = headers
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "Missing API key".to_string()))?;

    // Verify signature if provided
    if let Some(signature) = &request.provider_signature {
        let payload = serde_json::to_string(&request.proofs).map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to serialize proofs: {}", e))
        })?;
        
        let auth_manager = &state.auth_manager;
        match auth_manager.verify_signature(api_key, &payload, signature) {
            Ok(true) => {},
            Ok(false) => return Err((StatusCode::FORBIDDEN, "Invalid signature".to_string())),
            Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Signature verification failed: {}", e))),
        }
    }

    let mut settlement = state.settlement.write().await;
    
    // Process each proof
    let mut processed_count = 0;
    for proof in &request.proofs {
        // Mark as settled in the database
        match settlement.mark_settled(&proof.provider_id, "pending-on-chain").await {
            Ok(count) if count > 0 => processed_count += count,
            Ok(_) => {},
            Err(e) => {
                eprintln!("Failed to mark proof as settled: {}", e);
            }
        }
    }

    Ok(Json(SettlementResponse {
        success: true,
        transaction_id: Some("batch-processed".to_string()), // Would be actual tx_id after on-chain submission
        message: format!("Successfully processed {} usage proofs", processed_count),
        batch_size: request.proofs.len(),
    }))
}

// Get Settlement Summary Endpoint
async fn get_settlement_summary(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let api_key = headers
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .ok_or((StatusCode::UNAUTHORIZED, "Missing API key".to_string()))?;

    let settlement = state.settlement.read().await;
    
    match settlement.get_settlement_summary(api_key).await {
        Ok(summary) => Ok(Json(json!({
            "success": true,
            "api_key": api_key,
            "total_records": summary.total_records,
            "pending_records": summary.pending_records,
            "settled_records": summary.settled_records,
            "total_tokens_input": summary.total_tokens_input,
            "total_tokens_output": summary.total_tokens_output,
        }))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get summary: {}", e))),
    }
}
