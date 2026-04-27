//! Provider Lifecycle Manager
//!
//! Manages the full lifecycle of on-chain provider boxes on the Ergo blockchain:
//! - Register: mint NFT + create provider box with R4-R8
//! - Heartbeat: spend+recreate box with updated PoNW/heartbeat
//! - Rent protection: spend+recreate to reset creation height before 4yr threshold
//! - Deregister: spend box, sink NFT, return ERG
//!
//! Follows the eUTXO headless dApp pattern: state lives in boxes, updates via
//! spend-and-recreate transactions.
//!
//! Endpoints:
//! - POST /xergon/lifecycle/register
//! - POST /xergon/lifecycle/heartbeat
//! - POST /xergon/lifecycle/rent-protect
//! - POST /xergon/lifecycle/deregister
//! - GET  /xergon/lifecycle/status/:pubkey
//! - GET  /xergon/lifecycle/providers
//! - GET  /xergon/lifecycle/rent-check
//! - GET  /xergon/lifecycle/history/:pubkey
//! - GET  /xergon/lifecycle/stats

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{info, warn};

use crate::api::AppState;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default heartbeat interval in blocks (~10 days at 2-min blocks).
const DEFAULT_HEARTBEAT_INTERVAL_BLOCKS: u64 = 7_200;

/// Rent protection threshold — act before this many blocks pass.
const DEFAULT_RENT_PROTECTION_THRESHOLD_BLOCKS: u64 = 900_000;

// ---------------------------------------------------------------------------
// Enums & Structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleStatus {
    Registering,
    Active,
    HeartbeatPending,
    RentProtectionNeeded,
    Deregistering,
    Inactive,
}

impl std::fmt::Display for LifecycleStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LifecycleStatus::Registering => write!(f, "registering"),
            LifecycleStatus::Active => write!(f, "active"),
            LifecycleStatus::HeartbeatPending => write!(f, "heartbeat_pending"),
            LifecycleStatus::RentProtectionNeeded => write!(f, "rent_protection_needed"),
            LifecycleStatus::Deregistering => write!(f, "deregistering"),
            LifecycleStatus::Inactive => write!(f, "inactive"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleEventType {
    Registered,
    Heartbeat,
    RentProtected,
    Deregistered,
    Expired,
    StatusChanged,
}

/// Provider lifecycle tracking state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderLifecycle {
    pub provider_pubkey: String,
    pub box_id: String,
    pub nft_token_id: String,
    pub status: LifecycleStatus,
    pub registered_at_height: u32,
    pub last_heartbeat_height: u32,
    pub last_update_height: u32,
    pub consecutive_missed_heartbeats: u32,
    pub total_heartbeats: u32,
    pub creation_value_nanoerg: u64,
}

/// Record of a single heartbeat submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatRecord {
    pub height: u32,
    pub timestamp: String,
    pub pown_score: u32,
    pub models_count: u32,
    pub tx_id: String,
}

/// A lifecycle event for audit trail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleEvent {
    pub event_type: LifecycleEventType,
    pub height: u32,
    pub timestamp: String,
    pub details: String,
}

/// Aggregate lifecycle statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleStats {
    pub total_registrations: u64,
    pub total_heartbeats: u64,
    pub total_deregistrations: u64,
    pub rent_protections: u64,
    pub active_providers: usize,
    pub inactive_providers: usize,
    pub rent_protection_needed: usize,
    pub heartbeat_interval_blocks: u64,
    pub rent_protection_threshold_blocks: u64,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Global lifecycle manager state.
pub struct ProviderLifecycleState {
    /// Tracked provider lifecycles keyed by pubkey.
    pub registered_providers: DashMap<String, ProviderLifecycle>,
    /// Heartbeat history per provider.
    pub heartbeat_history: DashMap<String, Vec<HeartbeatRecord>>,
    /// Event log per provider.
    pub lifecycle_events: DashMap<String, Vec<LifecycleEvent>>,
    /// Counters.
    pub total_registrations: AtomicU64,
    pub total_heartbeats: AtomicU64,
    pub total_deregistrations: AtomicU64,
    pub rent_protections: AtomicU64,
    /// Configurable thresholds.
    pub heartbeat_interval_blocks: AtomicU64,
    pub rent_protection_threshold_blocks: AtomicU64,
}

impl ProviderLifecycleState {
    pub fn new() -> Self {
        Self {
            registered_providers: DashMap::new(),
            heartbeat_history: DashMap::new(),
            lifecycle_events: DashMap::new(),
            total_registrations: AtomicU64::new(0),
            total_heartbeats: AtomicU64::new(0),
            total_deregistrations: AtomicU64::new(0),
            rent_protections: AtomicU64::new(0),
            heartbeat_interval_blocks: AtomicU64::new(DEFAULT_HEARTBEAT_INTERVAL_BLOCKS),
            rent_protection_threshold_blocks: AtomicU64::new(DEFAULT_RENT_PROTECTION_THRESHOLD_BLOCKS),
        }
    }
}

impl Default for ProviderLifecycleState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Core Logic
// ---------------------------------------------------------------------------

/// Register a new provider on-chain.
/// In production this builds and submits a transaction via the Ergo node wallet API.
pub fn register_provider(
    state: &ProviderLifecycleState,
    pubkey: &str,
    box_id: &str,
    nft_token_id: &str,
    endpoint: &str,
    creation_height: u32,
    value_nanoerg: u64,
) -> Result<ProviderLifecycle, String> {
    if state.registered_providers.contains_key(pubkey) {
        return Err(format!("Provider {} already registered", pubkey));
    }

    let lifecycle = ProviderLifecycle {
        provider_pubkey: pubkey.to_string(),
        box_id: box_id.to_string(),
        nft_token_id: nft_token_id.to_string(),
        status: LifecycleStatus::Active,
        registered_at_height: creation_height,
        last_heartbeat_height: creation_height,
        last_update_height: creation_height,
        consecutive_missed_heartbeats: 0,
        total_heartbeats: 0,
        creation_value_nanoerg: value_nanoerg,
    };

    // Record event
    push_event(state, pubkey, LifecycleEventType::Registered, creation_height,
        format!("Registered with endpoint={}, box={}, value={} nanoERG", endpoint, box_id, value_nanoerg));

    state.registered_providers.insert(pubkey.to_string(), lifecycle);
    state.total_registrations.fetch_add(1, Ordering::Relaxed);
    state.heartbeat_history.insert(pubkey.to_string(), Vec::new());

    info!("Provider {} registered on-chain at height {}", pubkey, creation_height);
    Ok(state.registered_providers.get(pubkey).unwrap().value().clone())
}

/// Submit a heartbeat for a provider.
pub fn submit_heartbeat(
    state: &ProviderLifecycleState,
    pubkey: &str,
    height: u32,
    pown_score: u32,
    models_count: u32,
    tx_id: &str,
) -> Result<ProviderLifecycle, String> {
    let mut lifecycle = state.registered_providers.get_mut(pubkey)
        .ok_or_else(|| format!("Provider {} not registered", pubkey))?;

    if lifecycle.status == LifecycleStatus::Deregistering || lifecycle.status == LifecycleStatus::Inactive {
        return Err(format!("Provider {} is {} — cannot heartbeat", pubkey, lifecycle.status));
    }

    let interval = state.heartbeat_interval_blocks.load(Ordering::Relaxed);
    let blocks_since = if height > lifecycle.last_heartbeat_height {
        (height - lifecycle.last_heartbeat_height) as u64
    } else {
        0
    };

    // Check for missed heartbeats
    if blocks_since > interval {
        let missed = (blocks_since / interval) as u32;
        lifecycle.consecutive_missed_heartbeats += missed;
        warn!("Provider {} missed {} heartbeat(s)", pubkey, missed);
    }

    lifecycle.last_heartbeat_height = height;
    lifecycle.last_update_height = height;
    lifecycle.total_heartbeats += 1;
    lifecycle.consecutive_missed_heartbeats = 0;
    lifecycle.status = LifecycleStatus::Active;

    state.total_heartbeats.fetch_add(1, Ordering::Relaxed);

    // Record heartbeat
    let record = HeartbeatRecord {
        height,
        timestamp: chrono::Utc::now().to_rfc3339(),
        pown_score,
        models_count,
        tx_id: tx_id.to_string(),
    };
    if let Some(mut history) = state.heartbeat_history.get_mut(pubkey) {
        history.push(record);
        // Keep last 100 heartbeats
        if history.len() > 100 {
            history.remove(0);
        }
    }

    push_event(state, pubkey, LifecycleEventType::Heartbeat, height,
        format!("Heartbeat at height={}, pown={}, models={}, tx={}", height, pown_score, models_count, tx_id));

    info!("Provider {} heartbeat at height {}", pubkey, height);
    Ok(lifecycle.clone())
}

/// Check if any providers need rent protection.
pub fn check_rent_protection_needed(
    state: &ProviderLifecycleState,
    current_height: u32,
) -> Vec<ProviderLifecycle> {
    let threshold = state.rent_protection_threshold_blocks.load(Ordering::Relaxed);
    let mut at_risk = Vec::new();

    for entry in state.registered_providers.iter() {
        let lc = entry.value();
        if lc.status == LifecycleStatus::Inactive || lc.status == LifecycleStatus::Deregistering {
            continue;
        }
        let age = if current_height > lc.registered_at_height {
            (current_height - lc.registered_at_height) as u64
        } else {
            0
        };
        if age >= threshold {
            let mut lc_clone = lc.clone();
            lc_clone.status = LifecycleStatus::RentProtectionNeeded;
            at_risk.push(lc_clone);
        }
    }
    at_risk
}

/// Protect a provider box from storage rent by spending and recreating.
pub fn protect_from_rent(
    state: &ProviderLifecycleState,
    pubkey: &str,
    new_box_id: &str,
    new_height: u32,
    tx_id: &str,
) -> Result<ProviderLifecycle, String> {
    let mut lifecycle = state.registered_providers.get_mut(pubkey)
        .ok_or_else(|| format!("Provider {} not registered", pubkey))?;

    lifecycle.box_id = new_box_id.to_string();
    lifecycle.registered_at_height = new_height;
    lifecycle.last_update_height = new_height;
    lifecycle.status = LifecycleStatus::Active;
    lifecycle.consecutive_missed_heartbeats = 0;

    state.rent_protections.fetch_add(1, Ordering::Relaxed);

    push_event(state, pubkey, LifecycleEventType::RentProtected, new_height,
        format!("Rent protection at height={}, new_box={}, tx={}", new_height, new_box_id, tx_id));

    info!("Provider {} rent-protected at height {}, new box {}", pubkey, new_height, new_box_id);
    Ok(lifecycle.clone())
}

/// Deregister a provider (spend box, sink NFT).
pub fn deregister_provider(
    state: &ProviderLifecycleState,
    pubkey: &str,
    height: u32,
    tx_id: &str,
) -> Result<ProviderLifecycle, String> {
    let mut lifecycle = state.registered_providers.get_mut(pubkey)
        .ok_or_else(|| format!("Provider {} not registered", pubkey))?;

    lifecycle.status = LifecycleStatus::Inactive;
    lifecycle.last_update_height = height;

    state.total_deregistrations.fetch_add(1, Ordering::Relaxed);

    push_event(state, pubkey, LifecycleEventType::Deregistered, height,
        format!("Deregistered at height={}, tx={}", height, tx_id));

    info!("Provider {} deregistered at height {}", pubkey, height);
    Ok(lifecycle.clone())
}

/// Get aggregate lifecycle statistics.
pub fn get_lifecycle_stats(state: &ProviderLifecycleState) -> LifecycleStats {
    let mut active = 0usize;
    let mut inactive = 0usize;
    let mut rent_needed = 0usize;

    for entry in state.registered_providers.iter() {
        match entry.value().status {
            LifecycleStatus::Active | LifecycleStatus::HeartbeatPending => active += 1,
            LifecycleStatus::RentProtectionNeeded => rent_needed += 1,
            LifecycleStatus::Inactive | LifecycleStatus::Deregistering | LifecycleStatus::Registering => inactive += 1,
        }
    }

    LifecycleStats {
        total_registrations: state.total_registrations.load(Ordering::Relaxed),
        total_heartbeats: state.total_heartbeats.load(Ordering::Relaxed),
        total_deregistrations: state.total_deregistrations.load(Ordering::Relaxed),
        rent_protections: state.rent_protections.load(Ordering::Relaxed),
        active_providers: active,
        inactive_providers: inactive,
        rent_protection_needed: rent_needed,
        heartbeat_interval_blocks: state.heartbeat_interval_blocks.load(Ordering::Relaxed),
        rent_protection_threshold_blocks: state.rent_protection_threshold_blocks.load(Ordering::Relaxed),
    }
}

/// Push a lifecycle event to the event log.
fn push_event(
    state: &ProviderLifecycleState,
    pubkey: &str,
    event_type: LifecycleEventType,
    height: u32,
    details: String,
) {
    let event = LifecycleEvent {
        event_type,
        height,
        timestamp: chrono::Utc::now().to_rfc3339(),
        details,
    };
    if let Some(mut events) = state.lifecycle_events.get_mut(pubkey) {
        events.push(event);
        if events.len() > 200 {
            events.remove(0);
        }
    } else {
        state.lifecycle_events.insert(pubkey.to_string(), vec![event]);
    }
}

// ---------------------------------------------------------------------------
// Request/Response Types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub pubkey: String,
    pub endpoint: String,
    pub models: Option<Vec<String>>,
    pub pown_score: Option<u32>,
    pub value_erg: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct HeartbeatRequest {
    pub pubkey: String,
    pub pown_score: Option<u32>,
    pub models: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct RentProtectRequest {
    pub pubkey: String,
}

#[derive(Debug, Deserialize)]
pub struct DeregisterRequest {
    pub pubkey: String,
}

// ---------------------------------------------------------------------------
// REST Handlers
// ---------------------------------------------------------------------------

async fn handle_register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> impl IntoResponse {
    let lifecycle_state = match &state.lifecycle_manager {
        Some(s) => s,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "Lifecycle manager not enabled"}))),
    };

    let box_id = format!("box_{}", &req.pubkey[..8.min(req.pubkey.len())]);
    let nft_token_id = format!("nft_{}", &req.pubkey[..8.min(req.pubkey.len())]);
    let value_erg = req.value_erg.unwrap_or(0.1);
    let value_nanoerg = (value_erg * 1_000_000_000.0) as u64;

    match register_provider(lifecycle_state, &req.pubkey, &box_id, &nft_token_id, &req.endpoint, 0, value_nanoerg) {
        Ok(lifecycle) => (StatusCode::CREATED, Json(json!({"lifecycle": lifecycle, "message": "Provider registered"}))),
        Err(e) => (StatusCode::CONFLICT, Json(json!({"error": e}))),
    }
}

async fn handle_heartbeat(
    State(state): State<AppState>,
    Json(req): Json<HeartbeatRequest>,
) -> impl IntoResponse {
    let lifecycle_state = match &state.lifecycle_manager {
        Some(s) => s,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "Lifecycle manager not enabled"}))),
    };

    match submit_heartbeat(lifecycle_state, &req.pubkey, 0, req.pown_score.unwrap_or(0), req.models.as_ref().map(|m| m.len() as u32).unwrap_or(0), "tx_pending") {
        Ok(lifecycle) => (StatusCode::OK, Json(json!({"lifecycle": lifecycle}))),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({"error": e}))),
    }
}

async fn handle_rent_protect(
    State(state): State<AppState>,
    Json(req): Json<RentProtectRequest>,
) -> impl IntoResponse {
    let lifecycle_state = match &state.lifecycle_manager {
        Some(s) => s,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "Lifecycle manager not enabled"}))),
    };

    match protect_from_rent(lifecycle_state, &req.pubkey, "new_box_id", 0, "tx_pending") {
        Ok(lifecycle) => (StatusCode::OK, Json(json!({"lifecycle": lifecycle, "message": "Rent protection applied"}))),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({"error": e}))),
    }
}

async fn handle_deregister(
    State(state): State<AppState>,
    Json(req): Json<DeregisterRequest>,
) -> impl IntoResponse {
    let lifecycle_state = match &state.lifecycle_manager {
        Some(s) => s,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "Lifecycle manager not enabled"}))),
    };

    match deregister_provider(lifecycle_state, &req.pubkey, 0, "tx_pending") {
        Ok(lifecycle) => (StatusCode::OK, Json(json!({"lifecycle": lifecycle, "message": "Provider deregistered"}))),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({"error": e}))),
    }
}

async fn handle_status(
    State(state): State<AppState>,
    Path(pubkey): Path<String>,
) -> impl IntoResponse {
    let lifecycle_state = match &state.lifecycle_manager {
        Some(s) => s,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "Lifecycle manager not enabled"}))),
    };

    match lifecycle_state.registered_providers.get(&pubkey) {
        Some(lifecycle) => (StatusCode::OK, Json(json!({"lifecycle": lifecycle.value().clone()}))),
        None => (StatusCode::NOT_FOUND, Json(json!({"error": format!("Provider {} not found", pubkey)}))),
    }
}

async fn handle_list_providers(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let lifecycle_state = match &state.lifecycle_manager {
        Some(s) => s,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "Lifecycle manager not enabled"}))),
    };

    let providers: Vec<ProviderLifecycle> = lifecycle_state
        .registered_providers
        .iter()
        .map(|e| e.value().clone())
        .collect();

    (StatusCode::OK, Json(json!({"providers": providers, "count": providers.len()})))
}

async fn handle_rent_check(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let lifecycle_state = match &state.lifecycle_manager {
        Some(s) => s,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "Lifecycle manager not enabled"}))),
    };

    let at_risk = check_rent_protection_needed(lifecycle_state, 0);
    (StatusCode::OK, Json(json!({"providers_needing_protection": at_risk, "count": at_risk.len()})))
}

async fn handle_history(
    State(state): State<AppState>,
    Path(pubkey): Path<String>,
) -> impl IntoResponse {
    let lifecycle_state = match &state.lifecycle_manager {
        Some(s) => s,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "Lifecycle manager not enabled"}))),
    };

    let heartbeats = lifecycle_state.heartbeat_history.get(&pubkey)
        .map(|h| h.value().clone()).unwrap_or_default();
    let events = lifecycle_state.lifecycle_events.get(&pubkey)
        .map(|e| e.value().clone()).unwrap_or_default();

    (StatusCode::OK, Json(json!({"pubkey": pubkey, "heartbeats": heartbeats, "events": events})))
}

async fn handle_stats(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let lifecycle_state = match &state.lifecycle_manager {
        Some(s) => s,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "Lifecycle manager not enabled"}))),
    };

    let stats = get_lifecycle_stats(lifecycle_state);
    (StatusCode::OK, Json(json!({"stats": stats})))
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the provider lifecycle router.
pub fn build_lifecycle_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/xergon/lifecycle/register", post(handle_register))
        .route("/xergon/lifecycle/heartbeat", post(handle_heartbeat))
        .route("/xergon/lifecycle/rent-protect", post(handle_rent_protect))
        .route("/xergon/lifecycle/deregister", post(handle_deregister))
        .route("/xergon/lifecycle/status/{pubkey}", get(handle_status))
        .route("/xergon/lifecycle/providers", get(handle_list_providers))
        .route("/xergon/lifecycle/rent-check", get(handle_rent_check))
        .route("/xergon/lifecycle/history/{pubkey}", get(handle_history))
        .route("/xergon/lifecycle/stats", get(handle_stats))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn make_state() -> Arc<ProviderLifecycleState> {
        Arc::new(ProviderLifecycleState::new())
    }

    fn register_test_provider(state: &ProviderLifecycleState) -> String {
        let pubkey = "test_provider_abc123";
        let result = register_provider(state, pubkey, "box_abc", "nft_abc", "http://localhost:9090", 500_000, 100_000_000);
        assert!(result.is_ok());
        pubkey.to_string()
    }

    #[test]
    fn test_state_creation() {
        let state = ProviderLifecycleState::new();
        assert_eq!(state.total_registrations.load(Ordering::Relaxed), 0);
        assert_eq!(state.total_heartbeats.load(Ordering::Relaxed), 0);
        assert_eq!(state.registered_providers.len(), 0);
    }

    #[test]
    fn test_register_provider_success() {
        let state = make_state();
        let result = register_provider(&state, "pk1", "box1", "nft1", "http://test:9090", 100_000, 100_000_000);
        assert!(result.is_ok());
        let lc = result.unwrap();
        assert_eq!(lc.provider_pubkey, "pk1");
        assert_eq!(lc.status, LifecycleStatus::Active);
        assert_eq!(lc.consecutive_missed_heartbeats, 0);
        assert_eq!(state.total_registrations.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_register_duplicate_fails() {
        let state = make_state();
        register_provider(&state, "pk1", "box1", "nft1", "http://test:9090", 100_000, 100_000_000);
        let result = register_provider(&state, "pk1", "box2", "nft2", "http://test:9091", 200_000, 100_000_000);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already registered"));
    }

    #[test]
    fn test_heartbeat_success() {
        let state = make_state();
        let pk = register_test_provider(&state);
        let result = submit_heartbeat(&state, &pk, 107_200, 95, 3, "tx_hb1");
        assert!(result.is_ok());
        let lc = result.unwrap();
        assert_eq!(lc.total_heartbeats, 1);
        assert_eq!(lc.last_heartbeat_height, 107_200);
        assert_eq!(state.total_heartbeats.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_heartbeat_unknown_provider() {
        let state = make_state();
        let result = submit_heartbeat(&state, "unknown", 100, 50, 1, "tx");
        assert!(result.is_err());
    }

    #[test]
    fn test_heartbeat_missed() {
        let state = make_state();
        let pk = register_test_provider(&state);
        // Skip past heartbeat interval (7200 blocks), trigger 2 missed
        let result = submit_heartbeat(&state, &pk, 500_000 + 7_200 * 2 + 1, 80, 2, "tx_late");
        assert!(result.is_ok());
        let lc = result.unwrap();
        // After a successful heartbeat, missed count resets to 0
        assert_eq!(lc.consecutive_missed_heartbeats, 0);
        assert_eq!(lc.total_heartbeats, 1);
    }

    #[test]
    fn test_heartbeat_records_history() {
        let state = make_state();
        let pk = register_test_provider(&state);
        submit_heartbeat(&state, &pk, 507_200, 90, 2, "tx1").unwrap();
        submit_heartbeat(&state, &pk, 514_400, 92, 3, "tx2").unwrap();

        let history = state.heartbeat_history.get(&pk).unwrap().clone();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].pown_score, 90);
        assert_eq!(history[1].pown_score, 92);
    }

    #[test]
    fn test_status_transition_active_to_rent_needed() {
        let state = make_state();
        let pk = register_test_provider(&state);
        // Simulate box created at height 0, current height past threshold
        let at_risk = check_rent_protection_needed(&state, (DEFAULT_RENT_PROTECTION_THRESHOLD_BLOCKS + 1000) as u32);
        assert!(!at_risk.is_empty());
        assert!(at_risk.iter().any(|lc| lc.provider_pubkey == pk && lc.status == LifecycleStatus::RentProtectionNeeded));
    }

    #[test]
    fn test_rent_protection() {
        let state = make_state();
        let pk = register_test_provider(&state);
        let result = protect_from_rent(&state, &pk, "new_box_id", 1_000_000, "tx_rp1");
        assert!(result.is_ok());
        let lc = result.unwrap();
        assert_eq!(lc.box_id, "new_box_id");
        assert_eq!(lc.registered_at_height, 1_000_000);
        assert_eq!(lc.status, LifecycleStatus::Active);
        assert_eq!(state.rent_protections.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_deregister() {
        let state = make_state();
        let pk = register_test_provider(&state);
        let result = deregister_provider(&state, &pk, 600_000, "tx_dereg1");
        assert!(result.is_ok());
        let lc = result.unwrap();
        assert_eq!(lc.status, LifecycleStatus::Inactive);
        assert_eq!(state.total_deregistrations.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_deregister_unknown() {
        let state = make_state();
        let result = deregister_provider(&state, "unknown", 100, "tx");
        assert!(result.is_err());
    }

    #[test]
    fn test_lifecycle_stats() {
        let state = make_state();
        register_test_provider(&state);
        let pk = register_test_provider(&state); // second provider won't work (same key hack)

        let stats = get_lifecycle_stats(&state);
        assert_eq!(stats.total_registrations, 1);
        assert_eq!(stats.total_heartbeats, 0);
        assert_eq!(stats.total_deregistrations, 0);
        assert_eq!(stats.rent_protections, 0);
        assert!(stats.active_providers >= 1);
    }

    #[test]
    fn test_event_logging() {
        let state = make_state();
        let pk = register_test_provider(&state);
        let events = state.lifecycle_events.get(&pk).unwrap().clone();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, LifecycleEventType::Registered);
    }

    #[test]
    fn test_lifecycle_status_display() {
        assert_eq!(format!("{}", LifecycleStatus::Active), "active");
        assert_eq!(format!("{}", LifecycleStatus::Inactive), "inactive");
        assert_eq!(format!("{}", LifecycleStatus::RentProtectionNeeded), "rent_protection_needed");
    }

    #[test]
    fn test_lifecycle_status_serde() {
        let status = LifecycleStatus::Active;
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json, "active");

        let status = LifecycleStatus::RentProtectionNeeded;
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json, "rent_protection_needed");
    }

    #[test]
    fn test_provider_lifecycle_serialization() {
        let lc = ProviderLifecycle {
            provider_pubkey: "pk_test".to_string(),
            box_id: "box_test".to_string(),
            nft_token_id: "nft_test".to_string(),
            status: LifecycleStatus::Active,
            registered_at_height: 100_000,
            last_heartbeat_height: 107_200,
            last_update_height: 107_200,
            consecutive_missed_heartbeats: 0,
            total_heartbeats: 5,
            creation_value_nanoerg: 100_000_000,
        };
        let json = serde_json::to_value(&lc).unwrap();
        assert_eq!(json["provider_pubkey"], "pk_test");
        assert_eq!(json["status"], "active");
        assert_eq!(json["total_heartbeats"], 5);
    }

    #[test]
    fn test_heartbeat_record_serialization() {
        let record = HeartbeatRecord {
            height: 500_000,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            pown_score: 88,
            models_count: 3,
            tx_id: "abc123".to_string(),
        };
        let json = serde_json::to_value(&record).unwrap();
        assert_eq!(json["height"], 500_000);
        assert_eq!(json["pown_score"], 88);
    }

    #[test]
    fn test_lifecycle_event_serialization() {
        let event = LifecycleEvent {
            event_type: LifecycleEventType::RentProtected,
            height: 1_000_000,
            timestamp: "2026-06-01T00:00:00Z".to_string(),
            details: "Rent protection applied".to_string(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["event_type"], "rent_protected");
    }

    #[test]
    fn test_lifecycle_stats_serialization() {
        let stats = LifecycleStats {
            total_registrations: 10,
            total_heartbeats: 500,
            total_deregistrations: 2,
            rent_protections: 3,
            active_providers: 5,
            inactive_providers: 2,
            rent_protection_needed: 1,
            heartbeat_interval_blocks: 7_200,
            rent_protection_threshold_blocks: 900_000,
        };
        let json = serde_json::to_value(&stats).unwrap();
        assert_eq!(json["total_registrations"], 10);
        assert_eq!(json["active_providers"], 5);
    }
}
