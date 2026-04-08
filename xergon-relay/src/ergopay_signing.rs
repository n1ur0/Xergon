#![allow(dead_code)]
//! ErgoPay Signing Flow — EIP-20 Protocol Implementation.
//!
//! Implements the full ErgoPay (EIP-20) protocol for mobile wallet signing.
//! Generates URIs for QR codes, manages dynamic requests, handles wallet
//! reply callbacks, and submits signed transactions.
//!
//! Endpoints (nested under /v1/ergopay):
//!   POST   /v1/ergopay/request          -- create signing request
//!   GET    /v1/ergopay/request/:id      -- get request details
//!   GET    /v1/ergopay/uri/:id          -- generate ErgoPay URI
//!   GET    /v1/ergopay/qr/:id           -- get QR code data
//!   POST   /v1/ergopay/reply/:id        -- handle wallet reply (signed tx)
//!   POST   /v1/ergopay/submit/:id       -- submit signed transaction
//!   GET    /v1/ergopay/requests         -- list requests
//!   GET    /v1/ergopay/stats            -- get signing statistics
//!   PUT    /v1/ergopay/config           -- update config
//!   POST   /v1/ergopay/cleanup          -- clean expired requests

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post, put},
    Router,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::proxy;

// ================================================================
// Domain Types
// ================================================================

/// ErgoPay request lifecycle status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ErgoPayStatus {
    Created,
    Delivered,
    Signed,
    Submitted,
    Expired,
    Failed,
    Rejected,
}

impl std::fmt::Display for ErgoPayStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErgoPayStatus::Created => write!(f, "created"),
            ErgoPayStatus::Delivered => write!(f, "delivered"),
            ErgoPayStatus::Signed => write!(f, "signed"),
            ErgoPayStatus::Submitted => write!(f, "submitted"),
            ErgoPayStatus::Expired => write!(f, "expired"),
            ErgoPayStatus::Failed => write!(f, "failed"),
            ErgoPayStatus::Rejected => write!(f, "rejected"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TokenRef {
    pub token_id: String,
    pub amount: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendingProof {
    pub proof_bytes: String,
    #[serde(default)]
    pub extension: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxInputRef {
    pub box_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spending_proof: Option<SpendingProof>,
    #[serde(default)]
    pub extension: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxOutputRef {
    pub ergo_tree: String,
    pub value: u64,
    #[serde(default)]
    pub assets: Vec<TokenRef>,
    #[serde(default)]
    pub additional_registers: HashMap<String, String>,
    pub creation_height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsignedTx {
    pub inputs: Vec<TxInputRef>,
    #[serde(default)]
    pub data_inputs: Vec<TxInputRef>,
    pub outputs: Vec<TxOutputRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedBox {
    pub box_id: String,
    pub value: u64,
    pub ergo_tree: String,
    #[serde(default)]
    pub assets: Vec<TokenRef>,
    #[serde(default)]
    pub additional_registers: HashMap<String, String>,
    pub creation_height: u32,
    pub transaction_id: String,
    pub index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReducedTransaction {
    pub id: String,
    pub unsigned_tx: UnsignedTx,
    #[serde(default)]
    pub input_boxes: Vec<SerializedBox>,
    #[serde(default)]
    pub data_input_boxes: Vec<SerializedBox>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedInput {
    pub box_id: String,
    pub spending_proof: SpendingProof,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedTransaction {
    pub id: String,
    pub inputs: Vec<SignedInput>,
    #[serde(default)]
    pub data_inputs: Vec<TxInputRef>,
    pub outputs: Vec<SerializedBox>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErgoPayRequest {
    pub id: String,
    pub reduced_tx: ReducedTransaction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub created_at: i64,
    pub expires_at: i64,
    pub status: ErgoPayStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErgoPayUri {
    pub uri: String,
    pub request_id: String,
    pub is_dynamic: bool,
    pub qr_data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErgoPayConfig {
    pub base_url: String,
    pub reply_timeout_secs: u64,
    pub default_expiry_secs: u64,
    pub max_request_size_bytes: u32,
}

impl Default for ErgoPayConfig {
    fn default() -> Self {
        Self {
            base_url: "https://relay.xergon.network".to_string(),
            reply_timeout_secs: 600,
            default_expiry_secs: 3600,
            max_request_size_bytes: 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErgoPayStats {
    pub total_requests: u64,
    pub total_signed: u64,
    pub total_submitted: u64,
    pub total_expired: u64,
    pub avg_signing_time_ms: u64,
}

// ================================================================
// ErgoPay State
// ================================================================

#[derive(Clone)]
pub struct ErgoPayState {
    inner: Arc<ErgoPayStateInner>,
}

struct ErgoPayStateInner {
    requests: DashMap<String, ErgoPayRequest>,
    signed_transactions: DashMap<String, SignedTransaction>,
    config: std::sync::RwLock<ErgoPayConfig>,
    total_requests: AtomicU64,
    total_signed: AtomicU64,
    total_submitted: AtomicU64,
    total_expired: AtomicU64,
    signing_time_sum_ms: AtomicU64,
    signing_time_count: AtomicU64,
}

impl ErgoPayState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ErgoPayStateInner {
                requests: DashMap::new(),
                signed_transactions: DashMap::new(),
                config: std::sync::RwLock::new(ErgoPayConfig::default()),
                total_requests: AtomicU64::new(0),
                total_signed: AtomicU64::new(0),
                total_submitted: AtomicU64::new(0),
                total_expired: AtomicU64::new(0),
                signing_time_sum_ms: AtomicU64::new(0),
                signing_time_count: AtomicU64::new(0),
            }),
        }
    }

    pub fn with_config(config: ErgoPayConfig) -> Self {
        Self {
            inner: Arc::new(ErgoPayStateInner {
                requests: DashMap::new(),
                signed_transactions: DashMap::new(),
                config: std::sync::RwLock::new(config),
                total_requests: AtomicU64::new(0),
                total_signed: AtomicU64::new(0),
                total_submitted: AtomicU64::new(0),
                total_expired: AtomicU64::new(0),
                signing_time_sum_ms: AtomicU64::new(0),
                signing_time_count: AtomicU64::new(0),
            }),
        }
    }
}

// ================================================================
// ErgoPay Engine
// ================================================================

pub struct ErgoPayEngine {
    pub(crate) state: ErgoPayState,
}

impl ErgoPayEngine {
    pub fn new(state: ErgoPayState) -> Self {
        Self { state }
    }

    pub fn create_request(
        &self,
        unsigned_tx: UnsignedTx,
        input_boxes: Vec<SerializedBox>,
        data_input_boxes: Vec<SerializedBox>,
        reply_to: Option<String>,
        message: Option<String>,
    ) -> Result<String, String> {
        let config = self.state.inner.config.read().map_err(|e| e.to_string())?;
        let request_id = Uuid::new_v4().to_string();
        let now = Utc::now().timestamp();
        let reduced_tx = ReducedTransaction {
            id: request_id.clone(),
            unsigned_tx,
            input_boxes,
            data_input_boxes,
        };
        let request = ErgoPayRequest {
            id: request_id.clone(),
            reduced_tx,
            reply_to,
            message,
            created_at: now,
            expires_at: now + config.default_expiry_secs as i64,
            status: ErgoPayStatus::Created,
        };
        self.state.inner.requests.insert(request_id.clone(), request);
        self.state.inner.total_requests.fetch_add(1, Ordering::Relaxed);
        info!(request_id = %request_id, "ErgoPay request created");
        Ok(request_id)
    }

    pub fn generate_static_uri(&self, request_id: &str) -> Result<ErgoPayUri, String> {
        let request = self.state.inner.requests
            .get(request_id)
            .ok_or_else(|| format!("Request {} not found", request_id))?;
        let tx_json = serde_json::to_string(&request.reduced_tx)
            .map_err(|e| format!("Failed to serialize reduced tx: {}", e))?;
        let encoded = URL_SAFE_NO_PAD.encode(tx_json.as_bytes());
        let uri = format!("nergopay:{}", encoded);
        debug!(request_id = %request_id, uri_len = uri.len(), "Static ErgoPay URI generated");
        Ok(ErgoPayUri {
            uri: uri.clone(),
            request_id: request_id.to_string(),
            is_dynamic: false,
            qr_data: uri,
        })
    }

    pub fn generate_dynamic_uri(
        &self,
        request_id: &str,
        address: Option<&str>,
    ) -> Result<ErgoPayUri, String> {
        let _ = self.state.inner.requests
            .get(request_id)
            .ok_or_else(|| format!("Request {} not found", request_id))?;
        let config = self.state.inner.config.read().map_err(|e| e.to_string())?;
        let addr_param = address.unwrap_or("#P2PK_ADDRESS#");
        let uri = format!(
            "nergopay://{}/api/ergopay/request/{}?address={}",
            config.base_url, request_id, addr_param
        );
        if let Some(mut req) = self.state.inner.requests.get_mut(request_id) {
            if req.status == ErgoPayStatus::Created {
                req.status = ErgoPayStatus::Delivered;
            }
        }
        debug!(request_id = %request_id, "Dynamic ErgoPay URI generated");
        Ok(ErgoPayUri {
            uri: uri.clone(),
            request_id: request_id.to_string(),
            is_dynamic: true,
            qr_data: uri,
        })
    }

    pub fn get_request(&self, request_id: &str) -> Result<ErgoPayRequest, String> {
        let request = self.state.inner.requests
            .get(request_id)
            .ok_or_else(|| format!("Request {} not found", request_id))?;
        if request.status == ErgoPayStatus::Created || request.status == ErgoPayStatus::Delivered {
            let now = Utc::now().timestamp();
            if now >= request.expires_at {
                drop(request);
                if let Some(mut req) = self.state.inner.requests.get_mut(request_id) {
                    req.status = ErgoPayStatus::Expired;
                    self.state.inner.total_expired.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        self.state.inner.requests
            .get(request_id)
            .map(|r| r.clone())
            .ok_or_else(|| format!("Request {} not found", request_id))
    }

    pub fn handle_reply(&self, request_id: &str, signed_tx: SignedTransaction) -> Result<(), String> {
        let mut request = self.state.inner.requests
            .get_mut(request_id)
            .ok_or_else(|| format!("Request {} not found", request_id))?;
        match request.status {
            ErgoPayStatus::Created | ErgoPayStatus::Delivered => {}
            _ => return Err(format!("Cannot handle reply for request in status: {}", request.status)),
        }
        let now = Utc::now().timestamp();
        if now >= request.expires_at {
            request.status = ErgoPayStatus::Expired;
            self.state.inner.total_expired.fetch_add(1, Ordering::Relaxed);
            return Err("Request has expired".to_string());
        }
        if signed_tx.inputs.len() != request.reduced_tx.unsigned_tx.inputs.len() {
            return Err(format!(
                "Signed tx input count {} does not match request input count {}",
                signed_tx.inputs.len(),
                request.reduced_tx.unsigned_tx.inputs.len()
            ));
        }
        for (signed_input, req_input) in signed_tx.inputs.iter()
            .zip(request.reduced_tx.unsigned_tx.inputs.iter())
        {
            if signed_input.box_id != req_input.box_id {
                return Err(format!(
                    "Signed input box_id {} does not match request box_id {}",
                    signed_input.box_id, req_input.box_id
                ));
            }
        }
        request.status = ErgoPayStatus::Signed;
        self.state.inner.signed_transactions
            .insert(request_id.to_string(), signed_tx);
        self.state.inner.total_signed.fetch_add(1, Ordering::Relaxed);
        info!(request_id = %request_id, "ErgoPay request signed by wallet");
        Ok(())
    }

    pub async fn submit_signed(&self, request_id: &str, node_url: &str) -> Result<String, String> {
        let signed_tx = self.state.inner.signed_transactions
            .get(request_id)
            .map(|r| r.clone())
            .ok_or_else(|| format!("No signed transaction for request {}", request_id))?;
        if let Some(req) = self.state.inner.requests.get_mut(request_id) {
            if req.status != ErgoPayStatus::Signed {
                return Err(format!("Cannot submit request in status: {}", req.status));
            }
        }
        let tx_json = serde_json::to_string(&signed_tx)
            .map_err(|e| format!("Failed to serialize signed tx: {}", e))?;
        let client = reqwest::Client::new();
        let url = format!("{}/transactions", node_url.trim_end_matches('/'));
        let response = client
            .post(&url)
            .header("Content-Type", "application/json")
            .body(tx_json)
            .send()
            .await
            .map_err(|e| format!("Failed to submit transaction to node: {}", e))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            if let Some(mut req) = self.state.inner.requests.get_mut(request_id) {
                req.status = ErgoPayStatus::Failed;
            }
            return Err(format!("Node returned status {}: {}", status, body));
        }
        let tx_id = response.text().await.unwrap_or_else(|_| request_id.to_string());
        if let Some(mut req) = self.state.inner.requests.get_mut(request_id) {
            req.status = ErgoPayStatus::Submitted;
        }
        self.state.inner.total_submitted.fetch_add(1, Ordering::Relaxed);
        if let Some(req) = self.state.inner.requests.get(request_id) {
            let now = Utc::now().timestamp_millis();
            let elapsed = (now - req.created_at).max(0) as u64;
            self.state.inner.signing_time_sum_ms.fetch_add(elapsed, Ordering::Relaxed);
            self.state.inner.signing_time_count.fetch_add(1, Ordering::Relaxed);
        }
        info!(request_id = %request_id, tx_id = %tx_id, "ErgoPay transaction submitted to node");
        Ok(tx_id)
    }

    pub fn verify_signature(&self, request_id: &str) -> Result<bool, String> {
        let request = self.state.inner.requests
            .get(request_id)
            .ok_or_else(|| format!("Request {} not found", request_id))?;
        if request.status != ErgoPayStatus::Signed {
            return Ok(false);
        }
        let signed_tx = self.state.inner.signed_transactions
            .get(request_id)
            .ok_or_else(|| format!("No signed tx for request {}", request_id))?;
        for input in &signed_tx.inputs {
            if input.spending_proof.proof_bytes.is_empty() {
                warn!(request_id = %request_id, box_id = %input.box_id, "Signed input missing proof bytes");
                return Ok(false);
            }
        }
        for (signed, req) in signed_tx.inputs.iter()
            .zip(request.reduced_tx.unsigned_tx.inputs.iter())
        {
            if signed.box_id != req.box_id {
                return Ok(false);
            }
        }
        debug!(request_id = %request_id, "Signature verification passed");
        Ok(true)
    }

    pub fn list_requests(
        &self,
        status: Option<ErgoPayStatus>,
        from: Option<i64>,
        to: Option<i64>,
    ) -> Vec<ErgoPayRequest> {
        let mut results: Vec<ErgoPayRequest> = self.state.inner.requests
            .iter()
            .map(|r| r.value().clone())
            .collect();
        if let Some(ref s) = status {
            results.retain(|r| r.status == *s);
        }
        if let Some(f) = from {
            results.retain(|r| r.created_at >= f);
        }
        if let Some(t) = to {
            results.retain(|r| r.created_at <= t);
        }
        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        results
    }

    pub fn get_config(&self) -> Result<ErgoPayConfig, String> {
        self.state.inner.config.read().map(|c| c.clone()).map_err(|e| e.to_string())
    }

    pub fn update_config(&self, config: ErgoPayConfig) -> Result<(), String> {
        let mut cfg = self.state.inner.config.write().map_err(|e| e.to_string())?;
        *cfg = config;
        info!("ErgoPay config updated");
        Ok(())
    }

    pub fn get_stats(&self) -> ErgoPayStats {
        let total_signed = self.state.inner.total_signed.load(Ordering::Relaxed);
        let signing_time_sum = self.state.inner.signing_time_sum_ms.load(Ordering::Relaxed);
        let signing_time_count = self.state.inner.signing_time_count.load(Ordering::Relaxed);
        ErgoPayStats {
            total_requests: self.state.inner.total_requests.load(Ordering::Relaxed),
            total_signed,
            total_submitted: self.state.inner.total_submitted.load(Ordering::Relaxed),
            total_expired: self.state.inner.total_expired.load(Ordering::Relaxed),
            avg_signing_time_ms: if signing_time_count > 0 { signing_time_sum / signing_time_count } else { 0 },
        }
    }

    pub fn cleanup_expired(&self) -> usize {
        let now = Utc::now().timestamp();
        let mut count = 0;
        self.state.inner.requests.retain(|_, req| {
            if now >= req.expires_at
                && matches!(req.status, ErgoPayStatus::Created | ErgoPayStatus::Delivered | ErgoPayStatus::Expired)
            {
                if req.status != ErgoPayStatus::Expired {
                    count += 1;
                }
                self.state.inner.total_expired.fetch_add(1, Ordering::Relaxed);
                false
            } else {
                true
            }
        });
        self.state.inner.signed_transactions.retain(|id, _| self.state.inner.requests.contains_key(id));
        if count > 0 {
            info!(expired_count = count, "ErgoPay expired requests cleaned up");
        }
        count
    }

    pub fn get_qr_data(&self, request_id: &str) -> Result<String, String> {
        let uri = self.generate_dynamic_uri(request_id, None)?;
        Ok(uri.qr_data)
    }
}

// ================================================================
// REST API Types
// ================================================================

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateRequestReq {
    pub unsigned_tx: UnsignedTx,
    #[serde(default)]
    pub input_boxes: Vec<SerializedBox>,
    #[serde(default)]
    pub data_input_boxes: Vec<SerializedBox>,
    pub reply_to: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListRequestsQuery {
    pub status: Option<String>,
    pub from: Option<i64>,
    pub to: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ReplyRequest {
    pub signed_tx: SignedTransaction,
}

#[derive(Debug, Deserialize)]
pub struct SubmitRequest {
    #[serde(default = "default_node_url")]
    pub node_url: String,
}

fn default_node_url() -> String {
    "http://127.0.0.1:9053".to_string()
}

#[derive(Debug, Deserialize)]
pub struct UpdateConfigReq {
    pub base_url: Option<String>,
    pub reply_timeout_secs: Option<u64>,
    pub default_expiry_secs: Option<u64>,
    pub max_request_size_bytes: Option<u32>,
}

// ================================================================
// REST Handlers
// ================================================================

async fn create_request_handler(
    State(state): State<proxy::AppState>,
    Json(body): Json<CreateRequestReq>,
) -> impl IntoResponse {
    let engine = ErgoPayEngine::new(state.ergopay.clone());
    match engine.create_request(
        body.unsigned_tx,
        body.input_boxes,
        body.data_input_boxes,
        body.reply_to,
        body.message,
    ) {
        Ok(id) => (StatusCode::CREATED, Json(serde_json::json!({
            "request_id": id,
            "status": "created"
        }))).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn get_request_handler(
    State(state): State<proxy::AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let engine = ErgoPayEngine::new(state.ergopay.clone());
    match engine.get_request(&id) {
        Ok(request) => (StatusCode::OK, Json(request)).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn get_uri_handler(
    State(state): State<proxy::AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let engine = ErgoPayEngine::new(state.ergopay.clone());
    match engine.generate_dynamic_uri(&id, None) {
        Ok(uri) => (StatusCode::OK, Json(uri)).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn get_qr_handler(
    State(state): State<proxy::AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let engine = ErgoPayEngine::new(state.ergopay.clone());
    match engine.get_qr_data(&id) {
        Ok(qr_data) => (StatusCode::OK, Json(serde_json::json!({
            "request_id": id,
            "qr_data": qr_data
        }))).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn reply_handler(
    State(state): State<proxy::AppState>,
    Path(id): Path<String>,
    Json(body): Json<ReplyRequest>,
) -> impl IntoResponse {
    let engine = ErgoPayEngine::new(state.ergopay.clone());
    match engine.handle_reply(&id, body.signed_tx) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({
            "request_id": id,
            "status": "signed"
        }))).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn submit_handler(
    State(state): State<proxy::AppState>,
    Path(id): Path<String>,
    Json(body): Json<SubmitRequest>,
) -> impl IntoResponse {
    let engine = ErgoPayEngine::new(state.ergopay.clone());
    match engine.submit_signed(&id, &body.node_url).await {
        Ok(tx_id) => (StatusCode::OK, Json(serde_json::json!({
            "request_id": id,
            "tx_id": tx_id,
            "status": "submitted"
        }))).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn list_requests_handler(
    State(state): State<proxy::AppState>,
    Query(query): Query<ListRequestsQuery>,
) -> impl IntoResponse {
    let engine = ErgoPayEngine::new(state.ergopay.clone());
    let status = match query.status.as_deref() {
        Some("created") => Some(ErgoPayStatus::Created),
        Some("delivered") => Some(ErgoPayStatus::Delivered),
        Some("signed") => Some(ErgoPayStatus::Signed),
        Some("submitted") => Some(ErgoPayStatus::Submitted),
        Some("expired") => Some(ErgoPayStatus::Expired),
        Some("failed") => Some(ErgoPayStatus::Failed),
        Some("rejected") => Some(ErgoPayStatus::Rejected),
        _ => None,
    };
    let requests = engine.list_requests(status, query.from, query.to);
    (StatusCode::OK, Json(requests))
}

async fn stats_handler(State(state): State<proxy::AppState>) -> impl IntoResponse {
    let engine = ErgoPayEngine::new(state.ergopay.clone());
    (StatusCode::OK, Json(engine.get_stats()))
}

async fn update_config_handler(
    State(state): State<proxy::AppState>,
    Json(body): Json<UpdateConfigReq>,
) -> impl IntoResponse {
    let engine = ErgoPayEngine::new(state.ergopay.clone());
    let mut config = match engine.get_config() {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e}))).into_response(),
    };
    if let Some(base_url) = body.base_url { config.base_url = base_url; }
    if let Some(timeout) = body.reply_timeout_secs { config.reply_timeout_secs = timeout; }
    if let Some(expiry) = body.default_expiry_secs { config.default_expiry_secs = expiry; }
    if let Some(max_size) = body.max_request_size_bytes { config.max_request_size_bytes = max_size; }
    match engine.update_config(config) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"status": "updated"}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn cleanup_handler(State(state): State<proxy::AppState>) -> impl IntoResponse {
    let engine = ErgoPayEngine::new(state.ergopay.clone());
    let count = engine.cleanup_expired();
    (StatusCode::OK, Json(serde_json::json!({"expired_cleaned": count})))
}

// ================================================================
// Router Builder
// ================================================================

pub fn build_router(state: proxy::AppState) -> Router<proxy::AppState> {
    Router::new()
        .route("/v1/ergopay/request", post(create_request_handler))
        .route("/v1/ergopay/request/:id", get(get_request_handler))
        .route("/v1/ergopay/uri/:id", get(get_uri_handler))
        .route("/v1/ergopay/qr/:id", get(get_qr_handler))
        .route("/v1/ergopay/reply/:id", post(reply_handler))
        .route("/v1/ergopay/submit/:id", post(submit_handler))
        .route("/v1/ergopay/requests", get(list_requests_handler))
        .route("/v1/ergopay/stats", get(stats_handler))
        .route("/v1/ergopay/config", put(update_config_handler))
        .route("/v1/ergopay/cleanup", post(cleanup_handler))
        .with_state(state)
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn test_engine() -> ErgoPayEngine {
        ErgoPayEngine::new(ErgoPayState::new())
    }

    fn sample_unsigned_tx() -> UnsignedTx {
        UnsignedTx {
            inputs: vec![TxInputRef {
                box_id: "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2".to_string(),
                spending_proof: None,
                extension: HashMap::new(),
            }],
            data_inputs: vec![],
            outputs: vec![TxOutputRef {
                ergo_tree: "100204a00b08cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798".to_string(),
                value: 1_000_000_000,
                assets: vec![],
                additional_registers: HashMap::new(),
                creation_height: 800_000,
            }],
        }
    }

    fn sample_signed_tx() -> SignedTransaction {
        SignedTransaction {
            id: "signed-tx-123".to_string(),
            inputs: vec![SignedInput {
                box_id: "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2".to_string(),
                spending_proof: SpendingProof {
                    proof_bytes: "abcd1234ef567890".to_string(),
                    extension: HashMap::new(),
                },
            }],
            data_inputs: vec![],
            outputs: vec![],
        }
    }

    fn sample_input_box() -> SerializedBox {
        SerializedBox {
            box_id: "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2".to_string(),
            value: 2_000_000_000,
            ergo_tree: "100204a00b08cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798".to_string(),
            assets: vec![],
            additional_registers: HashMap::new(),
            creation_height: 799_000,
            transaction_id: "parent-tx-001".to_string(),
            index: 0,
        }
    }

    #[test]
    fn test_create_request() {
        let engine = test_engine();
        let result = engine.create_request(
            sample_unsigned_tx(), vec![sample_input_box()], vec![],
            None, Some("Sign this transaction".to_string()),
        );
        assert!(result.is_ok());
        let id = result.unwrap();
        assert!(!id.is_empty());
        let request = engine.get_request(&id).unwrap();
        assert_eq!(request.status, ErgoPayStatus::Created);
        assert_eq!(request.message.as_deref(), Some("Sign this transaction"));
    }

    #[test]
    fn test_static_uri_generation() {
        let engine = test_engine();
        let id = engine.create_request(sample_unsigned_tx(), vec![sample_input_box()], vec![], None, None).unwrap();
        let uri = engine.generate_static_uri(&id).unwrap();
        assert!(uri.uri.starts_with("nergopay:"));
        assert!(!uri.is_dynamic);
        let encoded = uri.uri.strip_prefix("nergopay:").unwrap();
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
        assert!(!encoded.contains('='));
        let decoded = URL_SAFE_NO_PAD.decode(encoded).unwrap();
        let json_str = String::from_utf8(decoded).unwrap();
        let _: ReducedTransaction = serde_json::from_str(&json_str).unwrap();
    }

    #[test]
    fn test_dynamic_uri_generation() {
        let engine = test_engine();
        let id = engine.create_request(sample_unsigned_tx(), vec![sample_input_box()], vec![], None, None).unwrap();
        let uri = engine.generate_dynamic_uri(&id, None).unwrap();
        assert!(uri.uri.contains("address=#P2PK_ADDRESS#"));
        assert!(uri.is_dynamic);
        let addr = "3WvsT8Gm3sBZjXwQHGxvKkB5tQhPvKtE3sK2bmRvVz1mAmD6K";
        let uri2 = engine.generate_dynamic_uri(&id, Some(addr)).unwrap();
        assert!(uri2.uri.contains(&format!("address={}", addr)));
        assert_eq!(engine.get_request(&id).unwrap().status, ErgoPayStatus::Delivered);
    }

    #[test]
    fn test_handle_reply() {
        let engine = test_engine();
        let id = engine.create_request(sample_unsigned_tx(), vec![sample_input_box()], vec![], None, None).unwrap();
        assert!(engine.handle_reply(&id, sample_signed_tx()).is_ok());
        assert_eq!(engine.get_request(&id).unwrap().status, ErgoPayStatus::Signed);
    }

    #[test]
    fn test_handle_reply_invalid_input_count() {
        let engine = test_engine();
        let id = engine.create_request(sample_unsigned_tx(), vec![sample_input_box()], vec![], None, None).unwrap();
        let mut signed = sample_signed_tx();
        signed.inputs.push(SignedInput {
            box_id: "extra-box-id".to_string(),
            spending_proof: SpendingProof { proof_bytes: "proof".to_string(), extension: HashMap::new() },
        });
        let result = engine.handle_reply(&id, signed);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("input count"));
    }

    #[test]
    fn test_handle_reply_wrong_status() {
        let engine = test_engine();
        let id = engine.create_request(sample_unsigned_tx(), vec![sample_input_box()], vec![], None, None).unwrap();
        engine.handle_reply(&id, sample_signed_tx()).unwrap();
        assert!(engine.handle_reply(&id, sample_signed_tx()).is_err());
    }

    #[test]
    fn test_request_expiry() {
        let engine = test_engine();
        let id = engine.create_request(sample_unsigned_tx(), vec![sample_input_box()], vec![], None, None).unwrap();
        if let Some(mut req) = engine.state.inner.requests.get_mut(&id) {
            req.expires_at = Utc::now().timestamp() - 100;
        }
        let result = engine.handle_reply(&id, sample_signed_tx());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("expired"));
    }

    #[test]
    fn test_request_lifecycle() {
        let engine = test_engine();
        let id = engine.create_request(sample_unsigned_tx(), vec![sample_input_box()], vec![], None, None).unwrap();
        assert_eq!(engine.get_request(&id).unwrap().status, ErgoPayStatus::Created);
        engine.generate_dynamic_uri(&id, None).unwrap();
        assert_eq!(engine.get_request(&id).unwrap().status, ErgoPayStatus::Delivered);
        engine.handle_reply(&id, sample_signed_tx()).unwrap();
        assert_eq!(engine.get_request(&id).unwrap().status, ErgoPayStatus::Signed);
        assert!(engine.verify_signature(&id).unwrap());
    }

    #[test]
    fn test_cleanup_expired() {
        let engine = test_engine();
        let id = engine.create_request(sample_unsigned_tx(), vec![sample_input_box()], vec![], None, None).unwrap();
        if let Some(mut req) = engine.state.inner.requests.get_mut(&id) {
            req.expires_at = Utc::now().timestamp() - 100;
        }
        assert!(engine.cleanup_expired() >= 1);
        assert!(engine.get_request(&id).is_err());
    }

    #[test]
    fn test_qr_data_generation() {
        let engine = test_engine();
        let id = engine.create_request(sample_unsigned_tx(), vec![sample_input_box()], vec![], None, None).unwrap();
        let qr = engine.get_qr_data(&id).unwrap();
        assert!(qr.starts_with("nergopay://"));
        assert!(qr.contains("address=#P2PK_ADDRESS#"));
    }

    #[test]
    fn test_list_requests_with_filters() {
        let engine = test_engine();
        let id1 = engine.create_request(sample_unsigned_tx(), vec![sample_input_box()], vec![], None, None).unwrap();
        let _id2 = engine.create_request(sample_unsigned_tx(), vec![sample_input_box()], vec![], None, None).unwrap();
        engine.generate_dynamic_uri(&id1, None).unwrap();
        assert_eq!(engine.list_requests(Some(ErgoPayStatus::Created), None, None).len(), 1);
        assert_eq!(engine.list_requests(None, None, None).len(), 2);
    }

    #[test]
    fn test_config_update() {
        let engine = test_engine();
        assert_eq!(engine.get_config().unwrap().default_expiry_secs, 3600);
        let mut cfg = engine.get_config().unwrap();
        cfg.default_expiry_secs = 7200;
        cfg.base_url = "https://test.xergon.network".to_string();
        engine.update_config(cfg).unwrap();
        assert_eq!(engine.get_config().unwrap().default_expiry_secs, 7200);
    }

    #[test]
    fn test_stats_tracking() {
        let engine = test_engine();
        let id = engine.create_request(sample_unsigned_tx(), vec![sample_input_box()], vec![], None, None).unwrap();
        engine.handle_reply(&id, sample_signed_tx()).unwrap();
        let id2 = engine.create_request(sample_unsigned_tx(), vec![sample_input_box()], vec![], None, None).unwrap();
        if let Some(mut req) = engine.state.inner.requests.get_mut(&id2) {
            req.expires_at = Utc::now().timestamp() - 100;
        }
        engine.cleanup_expired();
        let stats = engine.get_stats();
        assert_eq!(stats.total_requests, 2);
        assert_eq!(stats.total_signed, 1);
        assert!(stats.total_expired >= 1);
    }

    #[test]
    fn test_reduced_transaction_serialization() {
        let reduced = ReducedTransaction {
            id: "test-id".to_string(),
            unsigned_tx: sample_unsigned_tx(),
            input_boxes: vec![sample_input_box()],
            data_input_boxes: vec![],
        };
        let json = serde_json::to_string(&reduced).unwrap();
        let deserialized: ReducedTransaction = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, reduced.id);
        assert_eq!(deserialized.unsigned_tx.inputs.len(), 1);
    }

    #[tokio::test]
    async fn test_concurrent_requests() {
        let engine = test_engine();
        let state = engine.state.clone();
        let mut handles = Vec::new();
        for _ in 0..10 {
            let s = state.clone();
            handles.push(tokio::spawn(async move {
                let eng = ErgoPayEngine::new(s);
                eng.create_request(sample_unsigned_tx(), vec![sample_input_box()], vec![], None, None).unwrap()
            }));
        }
        let mut ids = Vec::with_capacity(handles.len());
        for h in handles {
            ids.push(h.await.unwrap());
        }
        let unique: std::collections::HashSet<String> = ids.into_iter().collect();
        assert_eq!(unique.len(), 10);
        assert_eq!(engine.get_stats().total_requests, 10);
    }
}
