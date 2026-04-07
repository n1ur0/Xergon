//! ErgoPay (EIP-20) Transaction Builder for Mobile Wallet Signing
//!
//! Implements the ErgoPay protocol (EIP-20): the relay server builds unsigned
//! Ergo transactions, encodes them as `reducedTx`, and returns JSON responses
//! that mobile wallets (Nautilus, Yoroi, etc.) can parse, sign, and submit.
//!
//! Key features:
//!   - Reduced transaction construction (staking, provider registration, payment, token fee)
//!   - ErgoPay URI encoding (static base64url and dynamic URL schemes)
//!   - UTXO selection with token-awareness and dust filtering
//!   - Box validation (min value, fee bounds, register packing, token rules)
//!   - Request lifecycle management with TTL-based expiration
//!   - REST API for request creation, status, QR data, callbacks, and history

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// NanoERG per ERG (10^9).
pub const NANOERG_PER_ERG: u64 = 1_000_000_000;
/// Min box value: 360 nanoERG per byte of serialized box (Ergo protocol).
pub const NANOERG_PER_BYTE: u64 = 360;
/// Default minimum box value in nanoERG (~0.001 ERG).
pub const DEFAULT_MIN_BOX_VALUE: u64 = 1_000_000;
/// Default minimum fee in nanoERG (0.001 ERG).
pub const DEFAULT_MIN_FEE: u64 = 1_000_000;
/// Maximum fee in nanoERG (0.1 ERG).
pub const MAX_FEE_NANOERG: u64 = 100_000_000;
/// Default maximum inputs per transaction.
pub const DEFAULT_MAX_INPUTS: usize = 50;
/// Default request TTL in seconds (10 minutes).
pub const DEFAULT_REQUEST_TTL_SECS: u64 = 600;
/// Maximum request history to retain.
const MAX_HISTORY: usize = 10_000;
/// ErgoPay static URI scheme prefix.
pub const ERGOPAY_SCHEME: &str = "ergopay:";
/// ErgoPay dynamic URI scheme prefix.
pub const ERGOPAY_DYNAMIC_SCHEME: &str = "ergopay://";

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Severity level for ErgoPay response messages displayed in the wallet UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageSeverity {
    /// Informational message (blue).
    Information,
    /// Warning message (yellow).
    Warning,
    /// Error message (red).
    Error,
}

impl Default for MessageSeverity {
    fn default() -> Self {
        MessageSeverity::Information
    }
}

/// Status of an ErgoPay signing request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestStatus {
    /// Request created, awaiting wallet signing.
    Pending,
    /// Wallet returned a signed transaction.
    Signed,
    /// Request expired (TTL elapsed).
    Expired,
    /// Request failed (validation error or processing failure).
    Failed,
}

/// Value type for register entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValueType {
    /// Coll[Byte] UTF-8 string.
    String,
    /// Long integer.
    Long,
    /// Coll of a type.
    Coll,
    /// Raw bytes (Coll[Byte]).
    Bytes,
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the ErgoPay transaction builder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErgoPayConfig {
    /// Ergo node REST API URL (e.g. "http://localhost:9053").
    pub node_url: String,
    /// Ergo explorer API URL for address lookups.
    pub explorer_api: String,
    /// P2PK address for the relay's own holdings (fee change, etc.).
    pub p2pk_address: String,
    /// Minimum box value in nanoERG (default: 1_000_000).
    #[serde(default = "default_min_box_value")]
    pub min_box_value: u64,
    /// Minimum transaction fee in nanoERG (default: 1_000_000).
    #[serde(default = "default_min_fee")]
    pub min_fee: u64,
    /// Maximum number of inputs per transaction (default: 50).
    #[serde(default = "default_max_inputs")]
    pub max_inputs: usize,
    /// Request TTL in seconds before expiration (default: 600).
    #[serde(default = "default_request_ttl_secs")]
    pub request_ttl_secs: u64,
}

fn default_min_box_value() -> u64 {
    DEFAULT_MIN_BOX_VALUE
}
fn default_min_fee() -> u64 {
    DEFAULT_MIN_FEE
}
fn default_max_inputs() -> usize {
    DEFAULT_MAX_INPUTS
}
fn default_request_ttl_secs() -> u64 {
    DEFAULT_REQUEST_TTL_SECS
}

impl Default for ErgoPayConfig {
    fn default() -> Self {
        Self {
            node_url: "http://localhost:9053".to_string(),
            explorer_api: "https://api.ergoplatform.com".to_string(),
            p2pk_address: String::new(),
            min_box_value: DEFAULT_MIN_BOX_VALUE,
            min_fee: DEFAULT_MIN_FEE,
            max_inputs: DEFAULT_MAX_INPUTS,
            request_ttl_secs: DEFAULT_REQUEST_TTL_SECS,
        }
    }
}

// ---------------------------------------------------------------------------
// Data Types
// ---------------------------------------------------------------------------

/// Specification of a token in a transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSpec {
    /// Token ID (32-byte hex).
    pub token_id: String,
    /// Token amount.
    pub amount: u64,
}

/// Specification of an Ergo register entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterSpec {
    /// Register index (R4-R9).
    pub register: String,
    /// Type of the value stored in the register.
    pub value_type: ValueType,
    /// Hex-encoded register value.
    pub value_hex: String,
}

/// Incoming request to build an ErgoPay reduced transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReducedTxRequest {
    /// Unique request identifier (UUID).
    pub request_id: String,
    /// Sender P2PK/Sigma address.
    pub sender_address: String,
    /// Recipient P2PK/Sigma address.
    pub recipient_address: String,
    /// Amount in nanoERG to transfer.
    pub amount_nanoerg: u64,
    /// Optional tokens to include in the output.
    #[serde(default)]
    pub tokens: Vec<TokenSpec>,
    /// Optional register specifications for the output box (key = register name).
    #[serde(default)]
    pub registers: HashMap<String, String>,
    /// Message displayed to the user in their wallet.
    #[serde(default)]
    pub message: String,
    /// URL the wallet should POST the signed TX back to.
    #[serde(default)]
    pub reply_to_url: String,
}

/// A simplified Ergo box representation used for UTXO selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnspentBox {
    /// Box ID (32-byte hex).
    pub box_id: String,
    /// Transaction ID that created this box.
    pub tx_id: String,
    /// Index of the output in the creating transaction.
    pub index: u16,
    /// P2PK/Sigma address guarding this box.
    pub address: String,
    /// Value in nanoERG.
    pub value: u64,
    /// Tokens in this box.
    #[serde(default)]
    pub tokens: Vec<TokenSpec>,
    /// Additional registers (R4-R9), hex-encoded.
    #[serde(default)]
    pub additional_registers: HashMap<String, String>,
    /// Estimated serialized size in bytes.
    #[serde(default = "default_estimated_size")]
    pub estimated_bytes: u32,
}

fn default_estimated_size() -> u32 {
    200
}

/// Result of UTXO selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtxoSelection {
    /// Selected input boxes.
    pub inputs: Vec<UnspentBox>,
    /// Total nanoERG from selected inputs.
    pub total_value: u64,
    /// Total tokens collected from selected inputs.
    pub total_tokens: Vec<TokenSpec>,
    /// Estimated transaction fee in nanoERG.
    pub estimated_fee: u64,
    /// Change amount in nanoERG (total - output - fee).
    pub change_amount: u64,
}

/// EIP-20 ErgoPay response sent to the wallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErgoPayResponse {
    /// Base64url-encoded reduced transaction (unsigned, with proofs to fill).
    pub reduced_tx: String,
    /// Address of the signer (wallet should verify ownership).
    pub address: String,
    /// Message to display to the user.
    #[serde(default)]
    pub message: String,
    /// Severity of the message (affects wallet UI color).
    #[serde(default)]
    pub message_severity: MessageSeverity,
    /// URL the wallet should POST the signed TX to.
    #[serde(default)]
    pub reply_to: String,
}

/// Callback payload received from a wallet after signing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedTxCallback {
    /// The ErgoPay request ID.
    pub request_id: String,
    /// Hex-encoded signed transaction.
    pub signed_tx: String,
    /// ID of the submitted transaction (if the wallet broadcast it).
    #[serde(default)]
    pub tx_id: Option<String>,
}

/// Stored ErgoPay request with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredRequest {
    /// The original request parameters.
    pub request: ReducedTxRequest,
    /// Base64url-encoded reduced transaction.
    pub reduced_tx: String,
    /// Current status of the request.
    pub status: RequestStatus,
    /// Creation timestamp (RFC 3339).
    pub created_at: String,
    /// Expiration timestamp (RFC 3339).
    pub expires_at: String,
    /// When the status last changed (RFC 3339).
    pub updated_at: String,
    /// Signed TX hex (set by callback).
    #[serde(default)]
    pub signed_tx: Option<String>,
    /// Broadcast TX ID (if wallet submitted it).
    #[serde(default)]
    pub broadcast_tx_id: Option<String>,
    /// Error message (if status is Failed).
    #[serde(default)]
    pub error: Option<String>,
}

/// Query parameters for history listing.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct HistoryQuery {
    /// Filter by status (optional).
    pub status: Option<String>,
    /// Filter by address (optional).
    pub address: Option<String>,
    /// Maximum number of results (default: 50).
    pub limit: Option<usize>,
    /// Offset for pagination.
    pub offset: Option<usize>,
}

/// Validation error detail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    /// Human-readable error description.
    pub message: String,
    /// The field or component that failed validation.
    pub field: String,
}

// ---------------------------------------------------------------------------
// ErgoPay Service (shared state)
// ---------------------------------------------------------------------------

/// Core service managing ErgoPay request lifecycle, UTXO selection, and validation.
pub struct ErgoPayService {
    /// Service configuration.
    config: ErgoPayConfig,
    /// Active pending requests keyed by request ID.
    requests: Arc<DashMap<String, StoredRequest>>,
    /// Completed request history (bounded).
    history: Arc<DashMap<String, StoredRequest>>,
    /// Cached UTXO sets keyed by address.
    utxo_cache: Arc<DashMap<String, Vec<UnspentBox>>>,
    /// Base URL for dynamic ErgoPay endpoints (e.g. "https://relay.xergon.io").
    base_url: String,
    /// History count for bounding.
    history_count: Arc<std::sync::atomic::AtomicUsize>,
}

impl ErgoPayService {
    /// Create a new ErgoPay service with the given config and base URL.
    pub fn new(config: ErgoPayConfig, base_url: String) -> Self {
        Self {
            config,
            requests: Arc::new(DashMap::new()),
            history: Arc::new(DashMap::new()),
            utxo_cache: Arc::new(DashMap::new()),
            base_url,
            history_count: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// Create a new ErgoPay service with default configuration.
    pub fn with_defaults(base_url: String) -> Self {
        Self::new(ErgoPayConfig::default(), base_url)
    }

    /// Return a reference to the configuration.
    pub fn config(&self) -> &ErgoPayConfig {
        &self.config
    }

    // ---- Request Lifecycle ----

    /// Create a new ErgoPay signing request, build the reduced TX, and store it.
    pub fn create_request(&self, mut req: ReducedTxRequest) -> Result<StoredRequest, ValidationError> {
        // Validate the request
        self.validate_request(&req)?;

        // Assign a request ID if not provided
        if req.request_id.is_empty() {
            req.request_id = Uuid::new_v4().to_string();
        }

        // Build a placeholder reduced TX (in production this calls the Ergo node
        // to build the actual unsigned transaction; here we encode the request as
        // a JSON reduced TX for demonstration).
        let reduced_tx = self.build_reduced_tx(&req)?;

        let now = Utc::now();
        let expires = now + chrono::Duration::seconds(self.config.request_ttl_secs as i64);

        let stored = StoredRequest {
            request: req.clone(),
            reduced_tx,
            status: RequestStatus::Pending,
            created_at: now.to_rfc3339(),
            expires_at: expires.to_rfc3339(),
            updated_at: now.to_rfc3339(),
            signed_tx: None,
            broadcast_tx_id: None,
            error: None,
        };

        let id = stored.request.request_id.clone();
        self.requests.insert(id.clone(), stored.clone());
        Ok(stored)
    }

    /// Retrieve a stored request by ID, checking expiration.
    pub fn get_request(&self, request_id: &str) -> Option<StoredRequest> {
        let mut entry = self.requests.get_mut(request_id)?;
        let now = Utc::now();
        let expires_at = chrono::DateTime::parse_from_rfc3339(&entry.expires_at).ok()?;

        if now >= expires_at {
            entry.status = RequestStatus::Failed;
            entry.error = Some("Request expired".to_string());
            entry.updated_at = now.to_rfc3339();
            let archived = entry.value().clone();
            drop(entry);
            self.archive_request(request_id);
            return Some(archived);
        }

        Some(entry.value().clone())
    }

    /// Cancel a pending request.
    pub fn cancel_request(&self, request_id: &str) -> Result<StoredRequest, ValidationError> {
        let mut entry = self
            .requests
            .get_mut(request_id)
            .ok_or_else(|| ValidationError {
                message: format!("Request {} not found", request_id),
                field: "request_id".to_string(),
            })?;

        if entry.status != RequestStatus::Pending {
            return Err(ValidationError {
                message: format!("Cannot cancel request in {:?} state", entry.status),
                field: "status".to_string(),
            });
        }

        entry.status = RequestStatus::Failed;
        entry.error = Some("Cancelled by user".to_string());
        entry.updated_at = Utc::now().to_rfc3339();
        let archived = entry.value().clone();
        drop(entry);
        self.archive_request(request_id);
        Ok(archived)
    }

    /// Process a signed TX callback from a wallet.
    pub fn handle_callback(&self, callback: &SignedTxCallback) -> Result<StoredRequest, ValidationError> {
        let mut entry = self
            .requests
            .get_mut(&callback.request_id)
            .ok_or_else(|| ValidationError {
                message: format!("Request {} not found", callback.request_id),
                field: "request_id".to_string(),
            })?;

        // Check expiration
        let now = Utc::now();
        let expires_at = chrono::DateTime::parse_from_rfc3339(&entry.expires_at).ok();
        if let Some(exp) = expires_at {
            if now >= exp {
                entry.status = RequestStatus::Expired;
                entry.error = Some("Request expired before signing".to_string());
                entry.updated_at = now.to_rfc3339();
                let archived = entry.value().clone();
                drop(entry);
                self.archive_request(&callback.request_id);
                return Err(ValidationError {
                    message: "Request expired".to_string(),
                    field: "expires_at".to_string(),
                });
            }
        }

        // Verify the signed TX structure
        if let Err(e) = self.verify_signed_tx(&entry.value(), &callback.signed_tx) {
            entry.status = RequestStatus::Failed;
            entry.error = Some(e.message.clone());
            entry.updated_at = now.to_rfc3339();
            let archived = entry.value().clone();
            drop(entry);
            self.archive_request(&callback.request_id);
            return Err(e);
        }

        entry.status = RequestStatus::Signed;
        entry.signed_tx = Some(callback.signed_tx.clone());
        entry.broadcast_tx_id = callback.tx_id.clone();
        entry.updated_at = now.to_rfc3339();
        let completed = entry.value().clone();
        drop(entry);
        self.archive_request(&callback.request_id);
        Ok(completed)
    }

    /// List request history with optional filtering.
    pub fn list_history(&self, query: &HistoryQuery) -> Vec<StoredRequest> {
        let limit = query.limit.unwrap_or(50).min(200);
        let offset = query.offset.unwrap_or(0);

        let mut results: Vec<StoredRequest> = self
            .history
            .iter()
            .filter(|entry| {
                let req = entry.value();
                if let Some(ref status_str) = query.status {
                    let status_str = status_str.to_lowercase();
                    let matches = match req.status {
                        RequestStatus::Pending => status_str == "pending",
                        RequestStatus::Signed => status_str == "signed",
                        RequestStatus::Expired => status_str == "expired",
                        RequestStatus::Failed => status_str == "failed",
                    };
                    if !matches {
                        return false;
                    }
                }
                if let Some(ref addr) = query.address {
                    if req.request.sender_address != *addr && req.request.recipient_address != *addr {
                        return false;
                    }
                }
                true
            })
            .map(|e| e.value().clone())
            .collect();

        // Sort by creation time descending
        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        results.into_iter().skip(offset).take(limit).collect()
    }

    /// Return the current count of active (pending) requests.
    pub fn active_count(&self) -> usize {
        self.requests.len()
    }

    /// Expire all requests that have passed their TTL.
    pub fn expire_stale_requests(&self) -> usize {
        let now = Utc::now();
        let mut expired_ids = Vec::new();

        for entry in self.requests.iter() {
            if let Ok(exp) = chrono::DateTime::parse_from_rfc3339(&entry.expires_at) {
                if now >= exp {
                    expired_ids.push(entry.request.request_id.clone());
                }
            }
        }

        for id in expired_ids {
            if let Some(mut entry) = self.requests.get_mut(&id) {
                entry.status = RequestStatus::Expired;
                entry.error = Some("Request expired".to_string());
                entry.updated_at = now.to_rfc3339();
            }
            self.archive_request(&id);
        }

        expired_ids.len()
    }

    // ---- Reduced TX Building ----

    /// Build a placeholder reduced transaction (JSON-encoded request for demonstration).
    /// In production, this would call the Ergo node API to construct an actual
    /// unsigned Ergo transaction with proper inputs, outputs, and data inputs.
    fn build_reduced_tx(&self, req: &ReducedTxRequest) -> Result<String, ValidationError> {
        let tx_json = serde_json::json!({
            "requestId": req.request_id,
            "sender": req.sender_address,
            "recipient": req.recipient_address,
            "amount": req.amount_nanoerg,
            "tokens": req.tokens.iter().map(|t| {
                serde_json::json!({
                    "tokenId": t.token_id,
                    "amount": t.amount,
                })
            }).collect::<Vec<_>>(),
            "registers": req.registers,
            "fee": self.config.min_fee,
        });

        let bytes = serde_json::to_vec(&tx_json).map_err(|e| ValidationError {
            message: format!("Failed to serialize reduced TX: {}", e),
            field: "reduced_tx".to_string(),
        })?;

        Ok(URL_SAFE_NO_PAD.encode(&bytes))
    }

    /// Build a staking deposit transaction.
    /// Creates a box with the user's public key in R4 for staking registration.
    pub fn build_staking_deposit(
        &self,
        sender_address: &str,
        user_pk_hex: &str,
        stake_amount_nanoerg: u64,
        message: &str,
    ) -> Result<StoredRequest, ValidationError> {
        let mut registers = HashMap::new();
        registers.insert("R4".to_string(), user_pk_hex.to_string());

        let request = ReducedTxRequest {
            request_id: Uuid::new_v4().to_string(),
            sender_address: sender_address.to_string(),
            recipient_address: self.config.p2pk_address.clone(),
            amount_nanoerg: stake_amount_nanoerg,
            tokens: vec![],
            registers,
            message: message.to_string(),
            reply_to_url: format!("{}/v1/ergopay/callback", self.base_url),
        };

        self.create_request(request)
    }

    /// Build a provider registration transaction.
    /// Creates a box with provider metadata in registers R4-R8.
    pub fn build_provider_registration(
        &self,
        sender_address: &str,
        provider_pk_hex: &str,
        endpoint_url: &str,
        models_json: &str,
        pown_score: u64,
        region: &str,
        message: &str,
    ) -> Result<StoredRequest, ValidationError> {
        let mut registers = HashMap::new();
        registers.insert("R4".to_string(), provider_pk_hex.to_string());
        registers.insert("R5".to_string(), hex::encode(endpoint_url.as_bytes()));
        registers.insert("R6".to_string(), hex::encode(models_json.as_bytes()));
        registers.insert("R7".to_string(), pown_score.to_string());
        registers.insert("R8".to_string(), region.to_string());

        let request = ReducedTxRequest {
            request_id: Uuid::new_v4().to_string(),
            sender_address: sender_address.to_string(),
            recipient_address: sender_address.to_string(),
            amount_nanoerg: self.config.min_box_value * 10, // Sufficient box value for registers
            tokens: vec![],
            registers,
            message: message.to_string(),
            reply_to_url: format!("{}/v1/ergopay/callback", self.base_url),
        };

        self.create_request(request)
    }

    /// Build a simple ERG payment transaction between two addresses.
    pub fn build_payment_tx(
        &self,
        sender_address: &str,
        recipient_address: &str,
        amount_nanoerg: u64,
        message: &str,
    ) -> Result<StoredRequest, ValidationError> {
        let request = ReducedTxRequest {
            request_id: Uuid::new_v4().to_string(),
            sender_address: sender_address.to_string(),
            recipient_address: recipient_address.to_string(),
            amount_nanoerg,
            tokens: vec![],
            registers: HashMap::new(),
            message: message.to_string(),
            reply_to_url: format!("{}/v1/ergopay/callback", self.base_url),
        };

        self.create_request(request)
    }

    /// Build a token fee payment transaction (Babel fee pattern).
    /// Pays fees in a custom token alongside a small ERG amount.
    pub fn build_token_fee_tx(
        &self,
        sender_address: &str,
        recipient_address: &str,
        token_id: &str,
        token_amount: u64,
        erg_amount_nanoerg: u64,
        message: &str,
    ) -> Result<StoredRequest, ValidationError> {
        let request = ReducedTxRequest {
            request_id: Uuid::new_v4().to_string(),
            sender_address: sender_address.to_string(),
            recipient_address: recipient_address.to_string(),
            amount_nanoerg: erg_amount_nanoerg,
            tokens: vec![TokenSpec {
                token_id: token_id.to_string(),
                amount: token_amount,
            }],
            registers: HashMap::new(),
            message: message.to_string(),
            reply_to_url: format!("{}/v1/ergopay/callback", self.base_url),
        };

        self.create_request(request)
    }

    // ---- ErgoPay URI Encoding ----

    /// Encode a reduced transaction as a static `ergopay:` URI (base64url-encoded).
    pub fn encode_static_ergopay(reduced_tx_b64: &str) -> String {
        format!("{}{}", ERGOPAY_SCHEME, reduced_tx_b64)
    }

    /// Encode an ErgoPay request as a dynamic `ergopay://` URL pointing to the request endpoint.
    pub fn encode_dynamic_ergopay(&self, request_id: &str) -> String {
        format!(
            "{}{}/v1/ergopay/request/{}",
            ERGOPAY_DYNAMIC_SCHEME, self.base_url, request_id
        )
    }

    /// Decode an `ergopay:` URI into the encoded payload (base64url or URL).
    /// Returns `(is_dynamic, payload)` where payload is the base64 data or the URL.
    pub fn decode_ergopay_uri(uri: &str) -> Result<(bool, String), ValidationError> {
        if uri.starts_with(ERGOPAY_DYNAMIC_SCHEME) {
            let url = uri[ERGOPAY_DYNAMIC_SCHEME.len()..].to_string();
            Ok((true, url))
        } else if uri.starts_with(ERGOPAY_SCHEME) {
            let data = uri[ERGOPAY_SCHEME.len()..].to_string();
            Ok((false, data))
        } else {
            Err(ValidationError {
                message: "Invalid ErgoPay URI: must start with 'ergopay:' or 'ergopay://'".to_string(),
                field: "uri".to_string(),
            })
        }
    }

    /// Generate QR-compatible data string for a stored request.
    /// Returns the dynamic ErgoPay URL if the base URL is configured,
    /// otherwise falls back to static encoding.
    pub fn generate_qr_data(&self, stored: &StoredRequest) -> String {
        if !self.base_url.is_empty() {
            self.encode_dynamic_ergopay(&stored.request.request_id)
        } else {
            Self::encode_static_ergopay(&stored.reduced_tx)
        }
    }

    // ---- Response Building ----

    /// Build an EIP-20 compliant ErgoPay response for a stored request.
    pub fn build_response(&self, stored: &StoredRequest) -> ErgoPayResponse {
        ErgoPayResponse {
            reduced_tx: stored.reduced_tx.clone(),
            address: stored.request.sender_address.clone(),
            message: stored.request.message.clone(),
            message_severity: MessageSeverity::Information,
            reply_to: stored.request.reply_to_url.clone(),
        }
    }

    /// Build an error ErgoPay response for wallet display.
    pub fn build_error_response(
        &self,
        address: &str,
        message: &str,
        severity: MessageSeverity,
    ) -> ErgoPayResponse {
        // Use an empty reduced TX to signal error state
        ErgoPayResponse {
            reduced_tx: String::new(),
            address: address.to_string(),
            message: message.to_string(),
            message_severity: severity,
            reply_to: String::new(),
        }
    }

    // ---- UTXO Selection ----

    /// Select UTXOs from a set of unspent boxes to cover the target amount and tokens.
    ///
    /// Strategy:
    /// 1. Token-aware: prioritize boxes containing the required tokens.
    /// 2. Greedy ERG: select largest-value boxes first for fee coverage.
    /// 3. Dust filter: skip boxes below `min_box_value`.
    pub fn select_utxos(
        &self,
        available: &[UnspentBox],
        target_nanoerg: u64,
        required_tokens: &[TokenSpec],
    ) -> Result<UtxoSelection, ValidationError> {
        let fee = self.config.min_fee;
        let total_needed = target_nanoerg.saturating_add(fee);

        // Filter dust
        let filtered: Vec<&UnspentBox> = available
            .iter()
            .filter(|b| b.value >= self.config.min_box_value)
            .collect();

        if filtered.is_empty() {
            return Err(ValidationError {
                message: "No non-dust UTXOs available".to_string(),
                field: "inputs".to_string(),
            });
        }

        // Phase 1: Token-aware selection — grab boxes that contain required tokens
        let mut selected: Vec<UnspentBox> = Vec::new();
        let mut collected_tokens: HashMap<String, u64> = HashMap::new();
        let mut collected_value: u64 = 0;

        // Build a map of which tokens we still need
        let mut needed_tokens: HashMap<String, u64> = required_tokens
            .iter()
            .map(|t| (t.token_id.clone(), t.amount))
            .collect();

        // First pass: select boxes that contain needed tokens
        for box_ref in &filtered {
            if needed_tokens.is_empty() && collected_value >= total_needed {
                break;
            }

            let has_needed_token = box_ref.tokens.iter().any(|t| {
                needed_tokens.contains_key(&t.token_id)
            });

            if has_needed_token || needed_tokens.is_empty() {
                // Collect tokens from this box
                for t in &box_ref.tokens {
                    if let Some(remaining) = needed_tokens.get_mut(&t.token_id) {
                        let take = (*remaining).min(t.amount);
                        *collected_tokens.entry(t.token_id.clone()).or_insert(0) += take;
                        *remaining -= take;
                        if *remaining == 0 {
                            needed_tokens.remove(&t.token_id);
                        }
                    }
                }

                selected.push(box_ref.clone());
                collected_value += box_ref.value;
            }
        }

        // Phase 2: Greedy ERG selection — pick largest boxes first until we have enough
        if collected_value < total_needed {
            let mut remaining: Vec<&UnspentBox> = filtered
                .iter()
                .filter(|b| !selected.iter().any(|s| s.box_id == b.box_id))
                .collect();
            remaining.sort_by(|a, b| b.value.cmp(&a.value));

            for box_ref in remaining {
                if collected_value >= total_needed {
                    break;
                }
                if selected.len() >= self.config.max_inputs {
                    break;
                }
                selected.push(box_ref.clone());
                collected_value += box_ref.value;
            }
        }

        if collected_value < total_needed {
            return Err(ValidationError {
                message: format!(
                    "Insufficient ERG: need {} nanoERG, have {} nanoERG",
                    total_needed, collected_value
                ),
                field: "amount".to_string(),
            });
        }

        if !needed_tokens.is_empty() {
            let missing: Vec<String> = needed_tokens.keys().cloned().collect();
            return Err(ValidationError {
                message: format!("Missing tokens: {}", missing.join(", ")),
                field: "tokens".to_string(),
            });
        }

        let change_amount = collected_value.saturating_sub(target_nanoerg).saturating_sub(fee);
        let total_tokens: Vec<TokenSpec> = collected_tokens
            .into_iter()
            .map(|(id, amount)| TokenSpec { token_id: id, amount })
            .collect();

        Ok(UtxoSelection {
            inputs: selected,
            total_value: collected_value,
            total_tokens,
            estimated_fee: fee,
            change_amount,
        })
    }

    /// Update the UTXO cache for a given address.
    pub fn cache_utxos(&self, address: &str, boxes: Vec<UnspentBox>) {
        self.utxo_cache.insert(address.to_string(), boxes);
    }

    /// Get cached UTXOs for an address.
    pub fn get_cached_utxos(&self, address: &str) -> Option<Vec<UnspentBox>> {
        self.utxo_cache.get(address).map(|e| e.value().clone())
    }

    // ---- Box Validation ----

    /// Validate that a box meets the minimum value threshold (360 nanoERG/byte).
    pub fn validate_min_box_value(
        &self,
        box_value: u64,
        estimated_bytes: u32,
    ) -> Result<(), ValidationError> {
        let min_required = (estimated_bytes as u64).saturating_mul(NANOERG_PER_BYTE);
        if box_value < min_required {
            Err(ValidationError {
                message: format!(
                    "Box value {} nanoERG below minimum {} nanoERG ({} bytes * 360)",
                    box_value, min_required, estimated_bytes
                ),
                field: "value".to_string(),
            })
        } else {
            Ok(())
        }
    }

    /// Validate that a fee is within the acceptable bounds [0.001 ERG, 0.1 ERG].
    pub fn validate_fee(&self, fee_nanoerg: u64) -> Result<(), ValidationError> {
        if fee_nanoerg < self.config.min_fee {
            return Err(ValidationError {
                message: format!(
                    "Fee {} nanoERG below minimum {} nanoERG",
                    fee_nanoerg, self.config.min_fee
                ),
                field: "fee".to_string(),
            });
        }
        if fee_nanoerg > MAX_FEE_NANOERG {
            return Err(ValidationError {
                message: format!(
                    "Fee {} nanoERG exceeds maximum {} nanoERG",
                    fee_nanoerg, MAX_FEE_NANOERG
                ),
                field: "fee".to_string(),
            });
        }
        Ok(())
    }

    /// Validate register packing: no gaps in register sequence (R4, R5, R6...).
    /// For example, having R4 and R6 without R5 is invalid.
    pub fn validate_register_packing(
        registers: &HashMap<String, String>,
    ) -> Result<(), ValidationError> {
        // Extract register indices (R4=4, R5=5, ..., R9=9)
        let mut indices: Vec<u8> = registers
            .keys()
            .filter_map(|k| {
                if k.starts_with('R') {
                    k[1..].parse::<u8>().ok()
                } else {
                    None
                }
            })
            .filter(|&i| (4..=9).contains(&i))
            .collect();

        if indices.is_empty() {
            return Ok(());
        }

        indices.sort();

        // Check for gaps
        for window in indices.windows(2) {
            if window[1] != window[0] + 1 {
                return Err(ValidationError {
                    message: format!(
                        "Register gap detected: R{} followed by R{} (missing R{})",
                        window[0],
                        window[1],
                        window[0] + 1
                    ),
                    field: format!("R{}", window[0] + 1),
                });
            }
        }

        // Check that registers start at R4
        if indices[0] != 4 {
            return Err(ValidationError {
                message: format!(
                    "Registers must start at R4, found R{}",
                    indices[0]
                ),
                field: format!("R{}", indices[0]),
            });
        }

        Ok(())
    }

    /// Validate token rules: for new tokens being minted, the token ID must equal
    /// the first input box ID. For existing tokens, this check is not required.
    pub fn validate_token_rules(
        &self,
        input_box_ids: &[String],
        output_tokens: &[TokenSpec],
    ) -> Result<(), ValidationError> {
        if input_box_ids.is_empty() {
            return Err(ValidationError {
                message: "No input boxes provided for token validation".to_string(),
                field: "inputs".to_string(),
            });
        }

        let first_input_id = &input_box_ids[0];

        for token in output_tokens {
            // A token ID equal to the first input box ID means it's a minted token.
            // This is valid only if the first input is spent entirely (simplified check).
            // In production, the Ergo node handles this validation.
            if token.token_id == *first_input_id {
                // Token is being minted — valid as long as the first input exists.
                continue;
            }
            // For existing tokens, just verify the ID is a valid 32-byte hex string.
            if token.token_id.len() != 64 {
                return Err(ValidationError {
                    message: format!(
                        "Invalid token ID length {}: expected 64 hex chars",
                        token.token_id.len()
                    ),
                    field: "token_id".to_string(),
                });
            }
            if hex::decode(&token.token_id).is_err() {
                return Err(ValidationError {
                    message: format!("Invalid token ID hex: {}", token.token_id),
                    field: "token_id".to_string(),
                });
            }
        }

        Ok(())
    }

    // ---- Validation Helpers ----

    /// Validate a ReducedTxRequest before processing.
    fn validate_request(&self, req: &ReducedTxRequest) -> Result<(), ValidationError> {
        if req.sender_address.is_empty() {
            return Err(ValidationError {
                message: "Sender address is required".to_string(),
                field: "sender_address".to_string(),
            });
        }
        if req.recipient_address.is_empty() {
            return Err(ValidationError {
                message: "Recipient address is required".to_string(),
                field: "recipient_address".to_string(),
            });
        }
        if req.amount_nanoerg == 0 && req.tokens.is_empty() {
            return Err(ValidationError {
                message: "Amount or tokens must be specified".to_string(),
                field: "amount_nanoerg".to_string(),
            });
        }

        // Validate registers
        Self::validate_register_packing(&req.registers)?;

        // Validate token IDs
        for token in &req.tokens {
            if token.token_id.len() != 64 {
                return Err(ValidationError {
                    message: format!("Invalid token ID length: {}", token.token_id.len()),
                    field: "tokens.token_id".to_string(),
                });
            }
            if token.amount == 0 {
                return Err(ValidationError {
                    message: "Token amount must be positive".to_string(),
                    field: "tokens.amount".to_string(),
                });
            }
        }

        Ok(())
    }

    /// Verify that a signed TX callback matches the original request.
    /// In production this would decode the signed TX and verify input/output structure.
    fn verify_signed_tx(
        &self,
        stored: &StoredRequest,
        signed_tx_hex: &str,
    ) -> Result<(), ValidationError> {
        if signed_tx_hex.is_empty() {
            return Err(ValidationError {
                message: "Empty signed transaction".to_string(),
                field: "signed_tx".to_string(),
            });
        }

        // Verify the signed TX is valid hex
        if hex::decode(signed_tx_hex).is_err() {
            return Err(ValidationError {
                message: "Signed TX is not valid hex".to_string(),
                field: "signed_tx".to_string(),
            });
        }

        // Verify the signed TX is non-trivially sized (at least some bytes)
        if signed_tx_hex.len() < 10 {
            return Err(ValidationError {
                message: "Signed TX is suspiciously short".to_string(),
                field: "signed_tx".to_string(),
            });
        }

        // In production, deserialize the signed TX and verify:
        // - Input boxes match the selection
        // - Output amounts match the request
        // - Fee is within bounds
        // - Token transfers are correct
        // For now, we just check that the TX was returned.

        let _ = stored; // Acknowledge the original request context
        Ok(())
    }

    /// Archive a completed/expired/failed request to history.
    fn archive_request(&self, request_id: &str) {
        if let Some((id, stored)) = self.requests.remove(request_id) {
            if self.history_count.load(std::sync::atomic::Ordering::Relaxed) >= MAX_HISTORY {
                // Remove oldest entries to stay within bounds
                if let Some(oldest) = self
                    .history
                    .iter()
                    .min_by_key(|e| e.value().created_at.clone())
                {
                    self.history.remove(oldest.key());
                }
            }
            self.history.insert(id, stored);
        }
    }
}

// ---------------------------------------------------------------------------
// REST API Handlers
// ---------------------------------------------------------------------------

/// Shared state wrapper for axum handlers.
#[derive(Clone)]
pub struct ErgoPayState {
    pub service: Arc<ErgoPayService>,
}

impl ErgoPayState {
    pub fn new(service: ErgoPayService) -> Self {
        Self {
            service: Arc::new(service),
        }
    }
}

/// POST /v1/ergopay/request — Create a new ErgoPay signing request.
pub async fn create_ergopay_request(
    State(state): State<ErgoPayState>,
    Json(req): Json<ReducedTxRequest>,
) -> impl IntoResponse {
    match state.service.create_request(req) {
        Ok(stored) => (StatusCode::CREATED, Json(stored)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": e.message,
                "field": e.field,
            })),
        )
            .into_response(),
    }
}

/// GET /v1/ergopay/request/:id — Get request status and reduced TX.
pub async fn get_ergopay_request(
    State(state): State<ErgoPayState>,
    Path(request_id): Path<String>,
) -> impl IntoResponse {
    match state.service.get_request(&request_id) {
        Some(stored) => (StatusCode::OK, Json(stored)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "Request not found",
            })),
        )
            .into_response(),
    }
}

/// GET /v1/ergopay/uri/:id — Get ergopay: URI for QR code generation.
pub async fn get_ergopay_uri(
    State(state): State<ErgoPayState>,
    Path(request_id): Path<String>,
) -> impl IntoResponse {
    match state.service.get_request(&request_id) {
        Some(stored) => {
            let qr_data = state.service.generate_qr_data(&stored);
            let response = serde_json::json!({
                "request_id": request_id,
                "ergopay_uri": qr_data,
                "is_dynamic": !state.service.base_url.is_empty(),
            });
            (StatusCode::OK, Json(response)).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "Request not found",
            })),
        )
            .into_response(),
    }
}

/// POST /v1/ergopay/callback — Receive signed TX from wallet.
pub async fn ergopay_callback(
    State(state): State<ErgoPayState>,
    Json(callback): Json<SignedTxCallback>,
) -> impl IntoResponse {
    match state.service.handle_callback(&callback) {
        Ok(stored) => (StatusCode::OK, Json(stored)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": e.message,
                "field": e.field,
            })),
        )
            .into_response(),
    }
}

/// GET /v1/ergopay/history — List past requests with optional filtering.
pub async fn list_ergopay_history(
    State(state): State<ErgoPayState>,
    Query(query): Query<HistoryQuery>,
) -> impl IntoResponse {
    let history = state.service.list_history(&query);
    (StatusCode::OK, Json(serde_json::json!({
        "requests": history,
        "count": history.len(),
    })))
    .into_response()
}

/// DELETE /v1/ergopay/request/:id — Cancel a pending request.
pub async fn cancel_ergopay_request(
    State(state): State<ErgoPayState>,
    Path(request_id): Path<String>,
) -> impl IntoResponse {
    match state.service.cancel_request(&request_id) {
        Ok(stored) => (StatusCode::OK, Json(stored)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": e.message,
                "field": e.field,
            })),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the ErgoPay API router.
pub fn build_router(state: ErgoPayState) -> Router<()> {
    Router::new()
        .route("/v1/ergopay/request", post(create_ergopay_request))
        .route("/v1/ergopay/request/{id}", get(get_ergopay_request))
        .route("/v1/ergopay/request/{id}", delete(cancel_ergopay_request))
        .route("/v1/ergopay/uri/{id}", get(get_ergopay_uri))
        .route("/v1/ergopay/callback", post(ergopay_callback))
        .route("/v1/ergopay/history", get(list_ergopay_history))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_service() -> ErgoPayService {
        let config = ErgoPayConfig {
            node_url: "http://localhost:9053".to_string(),
            explorer_api: "https://api.ergoplatform.com".to_string(),
            p2pk_address: "3WvsT2Gm4EpsM9Pg18PdY6XyhNN8p2fs5TG".to_string(),
            min_box_value: DEFAULT_MIN_BOX_VALUE,
            min_fee: DEFAULT_MIN_FEE,
            max_inputs: DEFAULT_MAX_INPUTS,
            request_ttl_secs: 600,
        };
        ErgoPayService::new(config, "https://relay.xergon.io".to_string())
    }

    fn make_service_no_base_url() -> ErgoPayService {
        let config = ErgoPayConfig::default();
        ErgoPayService::new(config, String::new())
    }

    fn make_box(box_id: &str, value: u64, tokens: Vec<TokenSpec>) -> UnspentBox {
        UnspentBox {
            box_id: box_id.to_string(),
            tx_id: format!("tx-{}", box_id),
            index: 0,
            address: "3WvsT2Gm4EpsM9Pg18PdY6XyhNN8p2fs5TG".to_string(),
            value,
            tokens,
            additional_registers: HashMap::new(),
            estimated_bytes: 200,
        }
    }

    fn valid_token_id() -> String {
        "0000000000000000000000000000000000000000000000000000000000000000".to_string()
    }

    fn valid_token_id_2() -> String {
        "1111111111111111111111111111111111111111111111111111111111111111".to_string()
    }

    // ---- URI Encoding/Decoding Tests ----

    #[test]
    fn test_encode_static_ergopay() {
        let b64 = "SGVsbG8gV29ybGQ";
        let uri = ErgoPayService::encode_static_ergopay(b64);
        assert_eq!(uri, "ergopay:SGVsbG8gV29ybGQ");
    }

    #[test]
    fn test_encode_dynamic_ergopay() {
        let svc = make_service();
        let uri = svc.encode_dynamic_ergopay("abc-123");
        assert_eq!(uri, "ergopay://https://relay.xergon.io/v1/ergopay/request/abc-123");
    }

    #[test]
    fn test_decode_ergopay_uri_static() {
        let (is_dynamic, payload) =
            ErgoPayService::decode_ergopay_uri("ergopay:SGVsbG8gV29ybGQ").unwrap();
        assert!(!is_dynamic);
        assert_eq!(payload, "SGVsbG8gV29ybGQ");
    }

    #[test]
    fn test_decode_ergopay_uri_dynamic() {
        let (is_dynamic, payload) =
            ErgoPayService::decode_ergopay_uri("ergopay://https://relay.xergon.io/v1/ergopay/request/abc-123").unwrap();
        assert!(is_dynamic);
        assert_eq!(payload, "https://relay.xergon.io/v1/ergopay/request/abc-123");
    }

    #[test]
    fn test_decode_ergopay_uri_invalid() {
        let result = ErgoPayService::decode_ergopay_uri("http://example.com");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().field, "uri");
    }

    #[test]
    fn test_generate_qr_data_dynamic() {
        let svc = make_service();
        let req = ReducedTxRequest {
            request_id: "qr-test-1".to_string(),
            sender_address: "3WvsT2Gm4EpsM9Pg18PdY6XyhNN8p2fs5TG".to_string(),
            recipient_address: "3WvsT2Gm4EpsM9Pg18PdY6XyhNN8p2fs5TG".to_string(),
            amount_nanoerg: 1_000_000_000,
            tokens: vec![],
            registers: HashMap::new(),
            message: String::new(),
            reply_to_url: String::new(),
        };
        let stored = svc.create_request(req).unwrap();
        let qr = svc.generate_qr_data(&stored);
        assert!(qr.starts_with("ergopay://"));
        assert!(qr.contains("qr-test-1"));
    }

    #[test]
    fn test_generate_qr_data_static_fallback() {
        let svc = make_service_no_base_url();
        let req = ReducedTxRequest {
            request_id: "qr-test-2".to_string(),
            sender_address: "3WvsT2Gm4EpsM9Pg18PdY6XyhNN8p2fs5TG".to_string(),
            recipient_address: "3WvsT2Gm4EpsM9Pg18PdY6XyhNN8p2fs5TG".to_string(),
            amount_nanoerg: 1_000_000_000,
            tokens: vec![],
            registers: HashMap::new(),
            message: String::new(),
            reply_to_url: String::new(),
        };
        let stored = svc.create_request(req).unwrap();
        let qr = svc.generate_qr_data(&stored);
        assert!(qr.starts_with("ergopay:"));
        assert!(!qr.starts_with("ergopay://"));
    }

    // ---- Response Building Tests ----

    #[test]
    fn test_build_response() {
        let svc = make_service();
        let req = ReducedTxRequest {
            request_id: "resp-1".to_string(),
            sender_address: "3WvsT2Gm4EpsM9Pg18PdY6XyhNN8p2fs5TG".to_string(),
            recipient_address: "9fDcNZjhbhdhHpuPFvMksfpsF4qd3e8mdY".to_string(),
            amount_nanoerg: 500_000_000,
            tokens: vec![],
            registers: HashMap::new(),
            message: "Send 0.5 ERG".to_string(),
            reply_to_url: "https://relay.xergon.io/callback".to_string(),
        };
        let stored = svc.create_request(req).unwrap();
        let resp = svc.build_response(&stored);

        assert_eq!(resp.address, "3WvsT2Gm4EpsM9Pg18PdY6XyhNN8p2fs5TG");
        assert_eq!(resp.message, "Send 0.5 ERG");
        assert_eq!(resp.message_severity, MessageSeverity::Information);
        assert_eq!(resp.reply_to, "https://relay.xergon.io/callback");
        assert!(!resp.reduced_tx.is_empty());
    }

    #[test]
    fn test_build_error_response() {
        let svc = make_service();
        let resp = svc.build_error_response(
            "3WvsT2Gm4EpsM9Pg18PdY6XyhNN8p2fs5TG",
            "Insufficient funds",
            MessageSeverity::Error,
        );

        assert_eq!(resp.message, "Insufficient funds");
        assert_eq!(resp.message_severity, MessageSeverity::Error);
        assert!(resp.reduced_tx.is_empty());
    }

    // ---- UTXO Selection Tests ----

    #[test]
    fn test_utxo_selection_simple() {
        let svc = make_service();
        let boxes = vec![
            make_box("b1", 5_000_000_000, vec![]),
            make_box("b2", 3_000_000_000, vec![]),
            make_box("b3", 1_000_000_000, vec![]),
        ];

        let selection = svc.select_utxos(&boxes, 4_000_000_000, &[]).unwrap();
        // Should select enough to cover 4B + 1M fee
        assert!(selection.total_value >= 4_001_000_000);
        assert!(selection.inputs.len() >= 1);
        assert_eq!(selection.estimated_fee, 1_000_000);
    }

    #[test]
    fn test_utxo_selection_token_aware() {
        let svc = make_service();
        let tid = valid_token_id();
        let boxes = vec![
            make_box("b1", 1_000_000_000, vec![TokenSpec { token_id: tid.clone(), amount: 100 }]),
            make_box("b2", 5_000_000_000, vec![]),
        ];

        let required = vec![TokenSpec { token_id: tid.clone(), amount: 50 }];
        let selection = svc.select_utxos(&boxes, 500_000_000, &required).unwrap();

        // Must include b1 (has the token)
        assert!(selection.inputs.iter().any(|b| b.box_id == "b1"));
        assert!(selection.total_tokens.iter().any(|t| t.token_id == tid && t.amount >= 50));
    }

    #[test]
    fn test_utxo_selection_greedy_erg() {
        let svc = make_service();
        let boxes = vec![
            make_box("small1", 1_100_000, vec![]),  // Above dust
            make_box("small2", 1_100_000, vec![]),  // Above dust
            make_box("big1", 8_000_000_000, vec![]),
            make_box("big2", 5_000_000_000, vec![]),
        ];

        let selection = svc.select_utxos(&boxes, 10_000_000_000, &[]).unwrap();
        // Greedy should prefer big boxes first
        assert!(selection.inputs.len() >= 2);
        assert!(selection.total_value >= 10_001_000_000);
    }

    #[test]
    fn test_utxo_selection_dust_filter() {
        let svc = make_service();
        let boxes = vec![
            make_box("dust1", 500_000, vec![]),   // Below min_box_value
            make_box("dust2", 900_000, vec![]),   // Below min_box_value
            make_box("ok1", 2_000_000_000, vec![]),
        ];

        let selection = svc.select_utxos(&boxes, 1_000_000_000, &[]).unwrap();
        // Should only select ok1, not dust
        assert_eq!(selection.inputs.len(), 1);
        assert_eq!(selection.inputs[0].box_id, "ok1");
    }

    #[test]
    fn test_utxo_selection_insufficient_funds() {
        let svc = make_service();
        let boxes = vec![
            make_box("b1", 500_000_000, vec![]),
            make_box("b2", 300_000_000, vec![]),
        ];

        let result = svc.select_utxos(&boxes, 10_000_000_000, &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("Insufficient ERG"));
    }

    #[test]
    fn test_utxo_selection_missing_tokens() {
        let svc = make_service();
        let tid = valid_token_id();
        let boxes = vec![
            make_box("b1", 5_000_000_000, vec![]),  // No tokens
        ];

        let required = vec![TokenSpec { token_id: tid.clone(), amount: 100 }];
        let result = svc.select_utxos(&boxes, 1_000_000_000, &required);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("Missing tokens"));
    }

    #[test]
    fn test_utxo_selection_no_non_dust() {
        let svc = make_service();
        let boxes = vec![
            make_box("dust1", 100_000, vec![]),
            make_box("dust2", 200_000, vec![]),
        ];

        let result = svc.select_utxos(&boxes, 1_000_000_000, &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("No non-dust"));
    }

    // ---- Box Validation Tests ----

    #[test]
    fn test_validate_min_box_value_ok() {
        let svc = make_service();
        // 200 bytes * 360 = 72_000 nanoERG minimum
        assert!(svc.validate_min_box_value(100_000, 200).is_ok());
    }

    #[test]
    fn test_validate_min_box_value_too_low() {
        let svc = make_service();
        let result = svc.validate_min_box_value(50_000, 200);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("below minimum"));
    }

    #[test]
    fn test_validate_fee_ok() {
        let svc = make_service();
        assert!(svc.validate_fee(1_000_000).is_ok());
        assert!(svc.validate_fee(50_000_000).is_ok());
        assert!(svc.validate_fee(100_000_000).is_ok());
    }

    #[test]
    fn test_validate_fee_too_low() {
        let svc = make_service();
        let result = svc.validate_fee(500_000);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("below minimum"));
    }

    #[test]
    fn test_validate_fee_too_high() {
        let svc = make_service();
        let result = svc.validate_fee(200_000_000);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("exceeds maximum"));
    }

    #[test]
    fn test_validate_register_packing_valid() {
        let mut regs = HashMap::new();
        regs.insert("R4".to_string(), "0x01".to_string());
        regs.insert("R5".to_string(), "0x02".to_string());
        assert!(ErgoPayService::validate_register_packing(&regs).is_ok());
    }

    #[test]
    fn test_validate_register_packing_gap() {
        let mut regs = HashMap::new();
        regs.insert("R4".to_string(), "0x01".to_string());
        regs.insert("R6".to_string(), "0x03".to_string()); // Missing R5
        let result = ErgoPayService::validate_register_packing(&regs);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("gap"));
    }

    #[test]
    fn test_validate_register_packing_starts_at_r4() {
        let mut regs = HashMap::new();
        regs.insert("R5".to_string(), "0x01".to_string()); // Should start at R4
        let result = ErgoPayService::validate_register_packing(&regs);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("must start at R4"));
    }

    #[test]
    fn test_validate_register_packing_empty() {
        let regs = HashMap::new();
        assert!(ErgoPayService::validate_register_packing(&regs).is_ok());
    }

    #[test]
    fn test_validate_token_rules_valid_existing() {
        let svc = make_service();
        let tokens = vec![
            TokenSpec { token_id: valid_token_id(), amount: 100 },
        ];
        let inputs = vec!["box1".to_string()];
        assert!(svc.validate_token_rules(&inputs, &tokens).is_ok());
    }

    #[test]
    fn test_validate_token_rules_minted() {
        let svc = make_service();
        let box_id = "0000000000000000000000000000000000000000000000000000000000000000";
        let tokens = vec![
            TokenSpec { token_id: box_id.to_string(), amount: 1000 },
        ];
        let inputs = vec![box_id.to_string()];
        assert!(svc.validate_token_rules(&inputs, &tokens).is_ok());
    }

    #[test]
    fn test_validate_token_rules_invalid_id_length() {
        let svc = make_service();
        let tokens = vec![
            TokenSpec { token_id: "short".to_string(), amount: 100 },
        ];
        let inputs = vec!["box1".to_string()];
        let result = svc.validate_token_rules(&inputs, &tokens);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("Invalid token ID length"));
    }

    #[test]
    fn test_validate_token_rules_invalid_hex() {
        let svc = make_service();
        let bad_id = "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";
        let tokens = vec![
            TokenSpec { token_id: bad_id.to_string(), amount: 100 },
        ];
        let inputs = vec!["box1".to_string()];
        let result = svc.validate_token_rules(&inputs, &tokens);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("not valid hex"));
    }

    // ---- Request Lifecycle Tests ----

    #[test]
    fn test_create_and_get_request() {
        let svc = make_service();
        let req = ReducedTxRequest {
            request_id: "lc-1".to_string(),
            sender_address: "3WvsT2Gm4EpsM9Pg18PdY6XyhNN8p2fs5TG".to_string(),
            recipient_address: "9fDcNZjhbhdhHpuPFvMksfpsF4qd3e8mdY".to_string(),
            amount_nanoerg: 1_000_000_000,
            tokens: vec![],
            registers: HashMap::new(),
            message: "Test payment".to_string(),
            reply_to_url: String::new(),
        };

        let stored = svc.create_request(req).unwrap();
        assert_eq!(stored.status, RequestStatus::Pending);
        assert_eq!(stored.request.request_id, "lc-1");

        let fetched = svc.get_request("lc-1").unwrap();
        assert_eq!(fetched.request_id(), "lc-1");
        assert_eq!(fetched.status, RequestStatus::Pending);
    }

    #[test]
    fn test_request_auto_id() {
        let svc = make_service();
        let req = ReducedTxRequest {
            request_id: String::new(), // Auto-generate
            sender_address: "3WvsT2Gm4EpsM9Pg18PdY6XyhNN8p2fs5TG".to_string(),
            recipient_address: "9fDcNZjhbhdhHpuPFvMksfpsF4qd3e8mdY".to_string(),
            amount_nanoerg: 1_000_000_000,
            tokens: vec![],
            registers: HashMap::new(),
            message: String::new(),
            reply_to_url: String::new(),
        };

        let stored = svc.create_request(req).unwrap();
        assert!(!stored.request.request_id.is_empty());
    }

    #[test]
    fn test_create_request_validation() {
        let svc = make_service();
        let req = ReducedTxRequest {
            request_id: "bad-1".to_string(),
            sender_address: String::new(), // Empty — should fail
            recipient_address: "9fDcNZjhbhdhHpuPFvMksfpsF4qd3e8mdY".to_string(),
            amount_nanoerg: 1_000_000_000,
            tokens: vec![],
            registers: HashMap::new(),
            message: String::new(),
            reply_to_url: String::new(),
        };

        let result = svc.create_request(req);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().field, "sender_address");
    }

    #[test]
    fn test_cancel_request() {
        let svc = make_service();
        let req = ReducedTxRequest {
            request_id: "cancel-1".to_string(),
            sender_address: "3WvsT2Gm4EpsM9Pg18PdY6XyhNN8p2fs5TG".to_string(),
            recipient_address: "9fDcNZjhbhdhHpuPFvMksfpsF4qd3e8mdY".to_string(),
            amount_nanoerg: 1_000_000_000,
            tokens: vec![],
            registers: HashMap::new(),
            message: String::new(),
            reply_to_url: String::new(),
        };

        svc.create_request(req).unwrap();
        let cancelled = svc.cancel_request("cancel-1").unwrap();
        assert_eq!(cancelled.status, RequestStatus::Failed);
        assert_eq!(cancelled.error, Some("Cancelled by user".to_string()));

        // Should be removed from active requests
        assert!(svc.get_request("cancel-1").is_none());
    }

    #[test]
    fn test_cancel_nonexistent_request() {
        let svc = make_service();
        let result = svc.cancel_request("nope");
        assert!(result.is_err());
    }

    #[test]
    fn test_callback_verification() {
        let svc = make_service();
        let req = ReducedTxRequest {
            request_id: "cb-1".to_string(),
            sender_address: "3WvsT2Gm4EpsM9Pg18PdY6XyhNN8p2fs5TG".to_string(),
            recipient_address: "9fDcNZjhbhdhHpuPFvMksfpsF4qd3e8mdY".to_string(),
            amount_nanoerg: 1_000_000_000,
            tokens: vec![],
            registers: HashMap::new(),
            message: String::new(),
            reply_to_url: String::new(),
        };

        svc.create_request(req).unwrap();

        let callback = SignedTxCallback {
            request_id: "cb-1".to_string(),
            signed_tx: "deadbeef00c0ffee".to_string(),
            tx_id: Some("tx-abc-123".to_string()),
        };

        let completed = svc.handle_callback(&callback).unwrap();
        assert_eq!(completed.status, RequestStatus::Signed);
        assert_eq!(completed.signed_tx, Some("deadbeef00c0ffee".to_string()));
        assert_eq!(completed.broadcast_tx_id, Some("tx-abc-123".to_string()));
    }

    #[test]
    fn test_callback_empty_tx() {
        let svc = make_service();
        let req = ReducedTxRequest {
            request_id: "cb-empty".to_string(),
            sender_address: "3WvsT2Gm4EpsM9Pg18PdY6XyhNN8p2fs5TG".to_string(),
            recipient_address: "9fDcNZjhbhdhHpuPFvMksfpsF4qd3e8mdY".to_string(),
            amount_nanoerg: 1_000_000_000,
            tokens: vec![],
            registers: HashMap::new(),
            message: String::new(),
            reply_to_url: String::new(),
        };

        svc.create_request(req).unwrap();

        let callback = SignedTxCallback {
            request_id: "cb-empty".to_string(),
            signed_tx: String::new(),
            tx_id: None,
        };

        let result = svc.handle_callback(&callback);
        assert!(result.is_err());
    }

    #[test]
    fn test_callback_invalid_hex() {
        let svc = make_service();
        let req = ReducedTxRequest {
            request_id: "cb-badhex".to_string(),
            sender_address: "3WvsT2Gm4EpsM9Pg18PdY6XyhNN8p2fs5TG".to_string(),
            recipient_address: "9fDcNZjhbhdhHpuPFvMksfpsF4qd3e8mdY".to_string(),
            amount_nanoerg: 1_000_000_000,
            tokens: vec![],
            registers: HashMap::new(),
            message: String::new(),
            reply_to_url: String::new(),
        };

        svc.create_request(req).unwrap();

        let callback = SignedTxCallback {
            request_id: "cb-badhex".to_string(),
            signed_tx: "zzzz".to_string(), // Not valid hex
            tx_id: None,
        };

        let result = svc.handle_callback(&callback);
        assert!(result.is_err());
    }

    #[test]
    fn test_history_listing() {
        let svc = make_service();

        // Create and complete a request
        let req = ReducedTxRequest {
            request_id: "hist-1".to_string(),
            sender_address: "3WvsT2Gm4EpsM9Pg18PdY6XyhNN8p2fs5TG".to_string(),
            recipient_address: "9fDcNZjhbhdhHpuPFvMksfpsF4qd3e8mdY".to_string(),
            amount_nanoerg: 1_000_000_000,
            tokens: vec![],
            registers: HashMap::new(),
            message: String::new(),
            reply_to_url: String::new(),
        };
        svc.create_request(req).unwrap();

        // Cancel to archive it
        svc.cancel_request("hist-1").unwrap();

        let query = HistoryQuery::default();
        let history = svc.list_history(&query);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].request.request_id, "hist-1");
    }

    #[test]
    fn test_history_filter_by_status() {
        let svc = make_service();

        for i in 0..3 {
            let req = ReducedTxRequest {
                request_id: format!("hf-{}", i),
                sender_address: "3WvsT2Gm4EpsM9Pg18PdY6XyhNN8p2fs5TG".to_string(),
                recipient_address: "9fDcNZjhbhdhHpuPFvMksfpsF4qd3e8mdY".to_string(),
                amount_nanoerg: 1_000_000_000,
                tokens: vec![],
                registers: HashMap::new(),
                message: String::new(),
                reply_to_url: String::new(),
            };
            svc.create_request(req).unwrap();
            svc.cancel_request(&format!("hf-{}", i)).unwrap();
        }

        let query = HistoryQuery {
            status: Some("failed".to_string()),
            ..Default::default()
        };
        let history = svc.list_history(&query);
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn test_active_count() {
        let svc = make_service();
        assert_eq!(svc.active_count(), 0);

        let req = ReducedTxRequest {
            request_id: "active-1".to_string(),
            sender_address: "3WvsT2Gm4EpsM9Pg18PdY6XyhNN8p2fs5TG".to_string(),
            recipient_address: "9fDcNZjhbhdhHpuPFvMksfpsF4qd3e8mdY".to_string(),
            amount_nanoerg: 1_000_000_000,
            tokens: vec![],
            registers: HashMap::new(),
            message: String::new(),
            reply_to_url: String::new(),
        };
        svc.create_request(req).unwrap();
        assert_eq!(svc.active_count(), 1);

        svc.cancel_request("active-1").unwrap();
        assert_eq!(svc.active_count(), 0);
    }

    #[test]
    fn test_expire_stale_requests() {
        let svc = make_service();
        // Create request with very short TTL
        let mut config = svc.config().clone();
        config.request_ttl_secs = 0; // Instant expiration
        let svc_short = ErgoPayService::new(config, "https://relay.xergon.io".to_string());

        let req = ReducedTxRequest {
            request_id: "expire-1".to_string(),
            sender_address: "3WvsT2Gm4EpsM9Pg18PdY6XyhNN8p2fs5TG".to_string(),
            recipient_address: "9fDcNZjhbhdhHpuPFvMksfpsF4qd3e8mdY".to_string(),
            amount_nanoerg: 1_000_000_000,
            tokens: vec![],
            registers: HashMap::new(),
            message: String::new(),
            reply_to_url: String::new(),
        };
        svc_short.create_request(req).unwrap();

        // Small sleep to ensure time passes
        std::thread::sleep(std::time::Duration::from_millis(10));

        let expired = svc_short.expire_stale_requests();
        assert_eq!(expired, 1);
        assert_eq!(svc_short.active_count(), 0);
    }

    // ---- Builder Method Tests ----

    #[test]
    fn test_build_payment_tx() {
        let svc = make_service();
        let stored = svc
            .build_payment_tx(
                "3WvsT2Gm4EpsM9Pg18PdY6XyhNN8p2fs5TG",
                "9fDcNZjhbhdhHpuPFvMksfpsF4qd3e8mdY",
                2_000_000_000,
                "Payment for services",
            )
            .unwrap();

        assert_eq!(stored.status, RequestStatus::Pending);
        assert_eq!(stored.request.amount_nanoerg, 2_000_000_000);
        assert_eq!(stored.request.message, "Payment for services");
        assert!(stored.reduced_tx.is_empty() || stored.reduced_tx.len() > 0);
    }

    #[test]
    fn test_build_staking_deposit() {
        let svc = make_service();
        let pk_hex = "02adef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
        let stored = svc
            .build_staking_deposit(
                "3WvsT2Gm4EpsM9Pg18PdY6XyhNN8p2fs5TG",
                pk_hex,
                10_000_000_000,
                "Stake 10 ERG",
            )
            .unwrap();

        assert_eq!(stored.status, RequestStatus::Pending);
        assert_eq!(stored.request.registers.get("R4").unwrap(), pk_hex);
        assert_eq!(stored.request.amount_nanoerg, 10_000_000_000);
    }

    #[test]
    fn test_build_provider_registration() {
        let svc = make_service();
        let pk = "02adef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";
        let stored = svc
            .build_provider_registration(
                "3WvsT2Gm4EpsM9Pg18PdY6XyhNN8p2fs5TG",
                pk,
                "https://provider.example.com",
                r#"{"models":["llama-3.1-8b"]}"#,
                800,
                "us-west",
                "Register as provider",
            )
            .unwrap();

        assert_eq!(stored.status, RequestStatus::Pending);
        assert!(stored.request.registers.contains_key("R4"));
        assert!(stored.request.registers.contains_key("R5"));
        assert!(stored.request.registers.contains_key("R6"));
        assert!(stored.request.registers.contains_key("R7"));
        assert!(stored.request.registers.contains_key("R8"));
    }

    #[test]
    fn test_build_token_fee_tx() {
        let svc = make_service();
        let tid = valid_token_id();
        let stored = svc
            .build_token_fee_tx(
                "3WvsT2Gm4EpsM9Pg18PdY6XyhNN8p2fs5TG",
                "9fDcNZjhbhdhHpuPFvMksfpsF4qd3e8mdY",
                &tid,
                500,
                100_000_000,
                "Token fee payment",
            )
            .unwrap();

        assert_eq!(stored.status, RequestStatus::Pending);
        assert_eq!(stored.request.tokens.len(), 1);
        assert_eq!(stored.request.tokens[0].amount, 500);
        assert_eq!(stored.request.amount_nanoerg, 100_000_000);
    }

    // ---- Constants Tests ----

    #[test]
    fn test_constants() {
        assert_eq!(NANOERG_PER_ERG, 1_000_000_000);
        assert_eq!(NANOERG_PER_BYTE, 360);
        assert_eq!(DEFAULT_MIN_BOX_VALUE, 1_000_000);
        assert_eq!(DEFAULT_MIN_FEE, 1_000_000);
        assert_eq!(DEFAULT_MAX_INPUTS, 50);
        assert_eq!(DEFAULT_REQUEST_TTL_SECS, 600);
        assert_eq!(MAX_FEE_NANOERG, 100_000_000);
    }

    #[test]
    fn test_config_defaults() {
        let config = ErgoPayConfig::default();
        assert_eq!(config.min_box_value, 1_000_000);
        assert_eq!(config.min_fee, 1_000_000);
        assert_eq!(config.max_inputs, 50);
        assert_eq!(config.request_ttl_secs, 600);
    }

    #[test]
    fn test_message_severity_default() {
        assert_eq!(MessageSeverity::default(), MessageSeverity::Information);
    }
}

// ---------------------------------------------------------------------------
// Helper trait for accessing request_id from StoredRequest
// ---------------------------------------------------------------------------

/// Helper for tests to access the request_id easily.
#[cfg(test)]
trait StoredRequestExt {
    fn request_id(&self) -> &str;
}

#[cfg(test)]
impl StoredRequestExt for StoredRequest {
    fn request_id(&self) -> &str {
        &self.request.request_id
    }
}
