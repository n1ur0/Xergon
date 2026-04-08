//! Token Operations — EIP-4 compliant minting, burning, and transfer
//!
//! Implements the Ergo token standard (EIP-4) and NFT collection standard (EIP-34).
//! Handles token ID derivation, register encoding, minimum box value,
//! and token preservation rules.

use axum::{
    extract::{Path, State},
    Json,
    Router,
    routing::{get, post},
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use sha2::{Sha256, Digest};

// ================================================================
// Types
// ================================================================

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct TokenId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSpec {
    pub token_id: TokenId,
    pub amount: u64,
    pub name: String,
    pub decimals: u8,
    pub description: String,
    pub nft_url: Option<String>,
    pub nft_content_hash: Option<String>,
    pub asset_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBalance {
    pub token_id: String,
    pub name: String,
    pub amount: u64,
    pub decimals: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftCollection {
    pub id: String,
    pub version: i32,
    pub name: String,
    pub description: String,
    pub logo_url: String,
    pub banner_url: String,
    pub category: String,
    pub socials: Vec<(String, String)>,
    pub minting_expiry: i64,
    pub total_nfts: u64,
    pub created_height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftProvenanceEntry {
    pub tx_id: String,
    pub event: String,
    pub from: String,
    pub to: String,
    pub height: u32,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenOperation {
    pub op_type: String,
    pub token_id: String,
    pub amount: u64,
    pub tx_id: String,
    pub height: u32,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TokenError {
    InvalidTokenId(String),
    InsufficientBalance { required: u64, available: u64 },
    Eip4Violation(String),
    TokenNotFound(String),
    CollectionNotFound(String),
    CollectionExpired,
    Internal(String),
}

// ================================================================
// EIP-4 Register Encoding
// ================================================================

/// Encode a Coll[Byte] value for EIP-4 registers.
/// Format: 0x0e + VLQ_length + data_bytes
pub fn encode_coll_byte(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x0e];
    out.extend(encode_vlq(data.len() as u64));
    out.extend_from_slice(data);
    out
}

/// Decode a Coll[Byte] from EIP-4 register format.
pub fn decode_coll_byte(bytes: &[u8]) -> Result<Vec<u8>, TokenError> {
    if bytes.is_empty() || bytes[0] != 0x0e {
        return Err(TokenError::Eip4Violation("Expected 0x0e prefix for Coll[Byte]".into()));
    }
    let (len, consumed) = decode_vlq(&bytes[1..])
        .ok_or_else(|| TokenError::Eip4Violation("Invalid VLQ length".into()))?;
    let data_start = 1 + consumed;
    if data_start + len as usize > bytes.len() {
        return Err(TokenError::Eip4Violation("Data overflows register".into()));
    }
    Ok(bytes[data_start..data_start + len as usize].to_vec())
}

/// Encode EIP-4 metadata registers for a token spec.
/// R4=name, R5=description, R6=decimals, R7=asset_type(NFT), R8=content_hash, R9=url
pub fn encode_eip4_registers(spec: &TokenSpec) -> HashMap<u8, Vec<u8>> {
    let mut regs = HashMap::new();
    regs.insert(4, encode_coll_byte(spec.name.as_bytes()));
    regs.insert(5, encode_coll_byte(spec.description.as_bytes()));
    regs.insert(6, encode_coll_byte(spec.decimals.to_string().as_bytes()));

    // NFT-specific registers (amount == 1, decimals == 0)
    if spec.amount == 1 && spec.decimals == 0 {
        if let Some(ref at) = spec.asset_type {
            regs.insert(7, encode_coll_byte(at.as_bytes()));
        }
        if let Some(ref ch) = spec.nft_content_hash {
            regs.insert(8, encode_coll_byte(ch.as_bytes()));
        }
        if let Some(ref url) = spec.nft_url {
            regs.insert(9, encode_coll_byte(url.as_bytes()));
        }
    }
    regs
}

/// Decode EIP-4 registers back into a TokenSpec.
pub fn decode_eip4_registers(registers: &HashMap<u8, Vec<u8>>) -> TokenSpec {
    let name = registers.get(&4)
        .and_then(|b| decode_coll_byte(b).ok())
        .and_then(|b| String::from_utf8(b).ok())
        .unwrap_or_default();
    let description = registers.get(&5)
        .and_then(|b| decode_coll_byte(b).ok())
        .and_then(|b| String::from_utf8(b).ok())
        .unwrap_or_default();
    let decimals = registers.get(&6)
        .and_then(|b| decode_coll_byte(b).ok())
        .and_then(|b| String::from_utf8(b).ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let asset_type = registers.get(&7)
        .and_then(|b| decode_coll_byte(b).ok())
        .and_then(|b| String::from_utf8(b).ok());
    let content_hash = registers.get(&8)
        .and_then(|b| decode_coll_byte(b).ok())
        .and_then(|b| String::from_utf8(b).ok());
    let nft_url = registers.get(&9)
        .and_then(|b| decode_coll_byte(b).ok())
        .and_then(|b| String::from_utf8(b).ok());

    TokenSpec {
        token_id: TokenId(String::new()),
        amount: if decimals == 0 && asset_type.is_some() { 1 } else { 0 },
        name,
        decimals,
        description,
        nft_url,
        nft_content_hash: content_hash,
        asset_type,
    }
}

/// Validate EIP-4 compliance of a token spec.
pub fn validate_eip4_compliance(spec: &TokenSpec) -> Result<(), TokenError> {
    if spec.name.is_empty() {
        return Err(TokenError::Eip4Violation("Name must not be empty".into()));
    }
    if spec.decimals > 18 {
        return Err(TokenError::Eip4Violation("Decimals must be 0-18".into()));
    }
    // NFT-specific checks
    if spec.amount == 1 && spec.decimals == 0 {
        // NFT: content_hash should be SHA-256 (64 hex chars)
        if let Some(ref ch) = spec.nft_content_hash {
            if ch.len() != 64 {
                return Err(TokenError::Eip4Violation("NFT content_hash must be 64 hex chars (SHA-256)".into()));
            }
        }
    }
    Ok(())
}

// ================================================================
// Token ID Rule
// ================================================================

/// Validate a token ID (must be 64-char hex, 32 bytes).
pub fn validate_token_id(hex: &str) -> Result<(), TokenError> {
    let clean = hex.trim_start_matches("0x");
    if clean.len() != 64 {
        return Err(TokenError::InvalidTokenId(format!("Token ID must be 64 hex chars, got {}", clean.len())));
    }
    hex::decode(clean).map_err(|e| TokenError::InvalidTokenId(format!("Invalid hex: {}", e)))?;
    Ok(())
}

/// Derive token ID from first input box ID.
/// In Ergo, the token ID equals the box ID of the first input.
pub fn derive_token_id(first_input_box_id: &str) -> TokenId {
    TokenId(first_input_box_id.to_string())
}

/// Validate token preservation: sum(inputs) == sum(outputs) + burned.
pub fn validate_token_preservation(
    inputs: &[TokenBalance],
    outputs: &[TokenBalance],
    burned: u64,
) -> Result<(), TokenError> {
    let mut input_map: HashMap<String, u64> = HashMap::new();
    for t in inputs {
        *input_map.entry(t.token_id.clone()).or_insert(0) += t.amount;
    }
    let mut output_map: HashMap<String, u64> = HashMap::new();
    for t in outputs {
        *output_map.entry(t.token_id.clone()).or_insert(0) += t.amount;
    }

    for (token_id, in_amount) in &input_map {
        let out_amount = output_map.get(token_id).copied().unwrap_or(0);
        if *in_amount != out_amount + burned {
            return Err(TokenError::Eip4Violation(format!(
                "Token {} not preserved: inputs={}, outputs={}, burned={}",
                token_id, in_amount, out_amount, burned
            )));
        }
    }
    Ok(())
}

// ================================================================
// Minimum Box Value
// ================================================================

/// Calculate minimum box value in nanoERG.
/// Rule: boxSizeInBytes * 360 nanoERG/byte
pub fn calculate_min_box_value(
    ergotree_size: usize,
    num_tokens: u32,
    register_sizes: &[usize],
) -> u64 {
    let base_size = 4u64;  // basic box overhead
    let tokens_size = if num_tokens > 0 {
        2 + (num_tokens as u64 * 34) // token count + (token_id(32) + amount(2)) per token
    } else { 0 };
    let regs_size: u64 = register_sizes.iter().sum::<usize>() as u64;
    let total_bytes = base_size + ergotree_size as u64 + tokens_size + regs_size;
    total_bytes * 360
}

/// Validate that a box value meets the minimum requirement.
pub fn validate_box_value(value_nanoerg: u64, estimated_box_size: usize) -> Result<(), TokenError> {
    let min = estimated_box_size as u64 * 360;
    if value_nanoerg < min {
        return Err(TokenError::Eip4Violation(format!(
            "Box value {} below minimum {} nanoERG ({} bytes)",
            value_nanoerg, min, estimated_box_size
        )));
    }
    Ok(())
}

// ================================================================
// VLQ Encoding
// ================================================================

fn encode_vlq(mut value: u64) -> Vec<u8> {
    if value == 0 { return vec![0]; }
    let mut bytes = Vec::new();
    while value > 0 {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value > 0 { byte |= 0x80; }
        bytes.push(byte);
    }
    bytes
}

fn decode_vlq(bytes: &[u8]) -> Option<(u64, usize)> {
    if bytes.is_empty() { return None; }
    let mut result: u64 = 0;
    let mut consumed = 0;
    for &byte in bytes {
        consumed += 1;
        result = (result << 7) | (byte & 0x7F) as u64;
        if byte & 0x80 == 0 { break; }
    }
    Some((result, consumed))
}

// ================================================================
// Token Operations Service
// ================================================================

pub struct TokenOperationsState {
    operations: DashMap<String, TokenOperation>,
    token_registry: DashMap<String, TokenSpec>,
    balances: DashMap<String, u64>,
    collections: DashMap<String, NftCollection>,
    collection_nfts: DashMap<String, Vec<String>>,
    provenance: DashMap<String, Vec<NftProvenanceEntry>>,
    op_counter: AtomicU64,
}

impl TokenOperationsState {
    pub fn new() -> Self {
        Self {
            operations: DashMap::new(),
            token_registry: DashMap::new(),
            balances: DashMap::new(),
            collections: DashMap::new(),
            collection_nfts: DashMap::new(),
            provenance: DashMap::new(),
            op_counter: AtomicU64::new(0),
        }
    }

    /// Mint a new token following EIP-4
    pub fn mint_token(
        &self,
        name: String,
        description: String,
        decimals: u8,
        amount: u64,
        nft_url: Option<String>,
        nft_content_hash: Option<String>,
        asset_type: Option<String>,
        recipient: String,
        first_input_box_id: &str,
        height: u32,
    ) -> Result<TokenOperation, TokenError> {
        let token_id = derive_token_id(first_input_box_id);
        let spec = TokenSpec {
            token_id: TokenId(token_id.0.clone()),
            amount,
            name: name.clone(),
            decimals,
            description,
            nft_url,
            nft_content_hash,
            asset_type,
        };
        validate_eip4_compliance(&spec)?;

        let tx_id = format!("tx_{}", self.op_counter.fetch_add(1, Ordering::Relaxed));
        let op = TokenOperation {
            op_type: "mint".into(),
            token_id: token_id.0.clone(),
            amount,
            tx_id: tx_id.clone(),
            height,
            status: "confirmed".into(),
        };

        self.token_registry.insert(token_id.0.clone(), spec);
        *self.balances.entry(token_id.0.clone()).or_insert(0) += amount;
        self.operations.insert(tx_id.clone(), op.clone());

        // Record provenance
        self.provenance.entry(token_id.0.clone())
            .or_insert_with(Vec::new)
            .push(NftProvenanceEntry {
                tx_id: tx_id.clone(),
                event: "Minted".into(),
                from: "mint".into(),
                to: recipient,
                height,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            });

        Ok(op)
    }

    /// Burn tokens
    pub fn burn_token(
        &self,
        token_id: &str,
        amount: u64,
        height: u32,
    ) -> Result<TokenOperation, TokenError> {
        validate_token_id(token_id)?;
        let balance = self.balances.get(token_id)
            .map(|b| *b)
            .ok_or_else(|| TokenError::TokenNotFound(token_id.into()))?;

        if balance < amount {
            return Err(TokenError::InsufficientBalance { required: amount, available: balance });
        }

        let tx_id = format!("tx_{}", self.op_counter.fetch_add(1, Ordering::Relaxed));
        let op = TokenOperation {
            op_type: "burn".into(),
            token_id: token_id.into(),
            amount,
            tx_id: tx_id.clone(),
            height,
            status: "confirmed".into(),
        };

        *self.balances.entry(op.token_id.clone()).or_insert(0) -= amount;
        self.operations.insert(tx_id, op.clone());
        Ok(op)
    }

    /// Transfer tokens
    pub fn transfer_token(
        &self,
        token_id: &str,
        amount: u64,
        recipient: String,
        height: u32,
    ) -> Result<TokenOperation, TokenError> {
        validate_token_id(token_id)?;
        let balance = self.balances.get(token_id)
            .map(|b| *b)
            .ok_or_else(|| TokenError::TokenNotFound(token_id.into()))?;

        if balance < amount {
            return Err(TokenError::InsufficientBalance { required: amount, available: balance });
        }

        let tx_id = format!("tx_{}", self.op_counter.fetch_add(1, Ordering::Relaxed));
        let op = TokenOperation {
            op_type: "transfer".into(),
            token_id: token_id.into(),
            amount,
            tx_id: tx_id.clone(),
            height,
            status: "confirmed".into(),
        };

        self.operations.insert(tx_id.clone(), op.clone());

        self.provenance.entry(op.token_id.clone())
            .or_insert_with(Vec::new)
            .push(NftProvenanceEntry {
                tx_id: tx_id.clone(),
                event: "Transferred".into(),
                from: "self".into(),
                to: recipient,
                height,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            });

        Ok(op)
    }

    pub fn get_token_info(&self, token_id: &str) -> Option<TokenSpec> {
        self.token_registry.get(token_id).map(|r| r.clone())
    }

    pub fn list_tokens(&self) -> Vec<TokenSpec> {
        self.token_registry.iter().map(|r| r.value().clone()).collect()
    }

    pub fn get_balance(&self, token_id: &str) -> u64 {
        self.balances.get(token_id).map(|b| *b).unwrap_or(0)
    }

    pub fn get_operations(&self, limit: usize) -> Vec<TokenOperation> {
        self.operations.iter()
            .map(|r| r.value().clone())
            .take(limit)
            .collect()
    }

    /// Create an NFT collection (EIP-34)
    pub fn create_collection(
        &self,
        name: String,
        description: String,
        logo_url: String,
        banner_url: String,
        category: String,
        socials: Vec<(String, String)>,
        expiry: i64,
        height: u32,
    ) -> Result<NftCollection, TokenError> {
        let id = format!("col_{}", hex::encode(sha2_hash(name.as_bytes()))[..16].to_string());
        let collection = NftCollection {
            id: id.clone(),
            version: 1,
            name,
            description,
            logo_url,
            banner_url,
            category,
            socials,
            minting_expiry: expiry,
            total_nfts: 0,
            created_height: height,
        };
        self.collections.insert(id, collection.clone());
        Ok(collection)
    }

    pub fn get_collection(&self, id: &str) -> Option<NftCollection> {
        self.collections.get(id).map(|r| r.clone())
    }

    pub fn list_collections(&self) -> Vec<NftCollection> {
        self.collections.iter().map(|r| r.value().clone()).collect()
    }

    pub fn get_collection_nfts(&self, collection_id: &str) -> Vec<String> {
        self.collection_nfts.get(collection_id)
            .map(|r| r.clone())
            .unwrap_or_default()
    }

    pub fn get_provenance(&self, token_id: &str) -> Vec<NftProvenanceEntry> {
        self.provenance.get(token_id)
            .map(|r| r.clone())
            .unwrap_or_default()
    }
}

// ================================================================
// REST API
// ================================================================

#[derive(Debug, Deserialize)]
struct MintRequest {
    name: String,
    description: String,
    decimals: u8,
    amount: u64,
    recipient: String,
    nft_url: Option<String>,
    nft_content_hash: Option<String>,
    asset_type: Option<String>,
    first_input_box_id: Option<String>,
    height: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct BurnRequest {
    token_id: String,
    amount: u64,
    height: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct TransferRequest {
    token_id: String,
    amount: u64,
    recipient: String,
    height: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct CreateCollectionRequest {
    name: String,
    description: String,
    logo_url: Option<String>,
    banner_url: Option<String>,
    category: Option<String>,
    socials: Option<Vec<(String, String)>>,
    expiry: Option<i64>,
    height: Option<u32>,
}

#[derive(Debug, Serialize)]
struct OpResponse {
    ok: bool,
    operation: Option<TokenOperation>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct TokensResponse {
    tokens: Vec<TokenSpec>,
    count: usize,
}

#[derive(Debug, Serialize)]
struct BalanceResponse {
    token_id: String,
    balance: u64,
}

#[derive(Debug, Serialize)]
struct Eip4ValidationResponse {
    valid: bool,
    errors: Vec<String>,
    warnings: Vec<String>,
}

// ================================================================
// Handlers
// ================================================================

async fn mint_handler(
    State(state): State<crate::api::AppState>,
    Json(req): Json<MintRequest>,
) -> Json<OpResponse> {
    let ops = match &state.token_operations {
        Some(o) => o,
        None => return Json(OpResponse { ok: false, operation: None, error: Some("Token operations not initialized".into()) }),
    };
    let box_id = req.first_input_box_id.as_deref()
        .unwrap_or("0000000000000000000000000000000000000000000000000000000000000000");
    let height = req.height.unwrap_or(0);
    match ops.mint_token(
        req.name, req.description, req.decimals, req.amount,
        req.nft_url, req.nft_content_hash, req.asset_type,
        req.recipient, box_id, height,
    ) {
        Ok(op) => Json(OpResponse { ok: true, operation: Some(op), error: None }),
        Err(e) => Json(OpResponse { ok: false, operation: None, error: Some(format!("{:?}", e)) }),
    }
}

async fn burn_handler(
    State(state): State<crate::api::AppState>,
    Json(req): Json<BurnRequest>,
) -> Json<OpResponse> {
    let ops = match &state.token_operations {
        Some(o) => o,
        None => return Json(OpResponse { ok: false, operation: None, error: Some("Token operations not initialized".into()) }),
    };
    let height = req.height.unwrap_or(0);
    match ops.burn_token(&req.token_id, req.amount, height) {
        Ok(op) => Json(OpResponse { ok: true, operation: Some(op), error: None }),
        Err(e) => Json(OpResponse { ok: false, operation: None, error: Some(format!("{:?}", e)) }),
    }
}

async fn transfer_handler(
    State(state): State<crate::api::AppState>,
    Json(req): Json<TransferRequest>,
) -> Json<OpResponse> {
    let ops = match &state.token_operations {
        Some(o) => o,
        None => return Json(OpResponse { ok: false, operation: None, error: Some("Token operations not initialized".into()) }),
    };
    let height = req.height.unwrap_or(0);
    match ops.transfer_token(&req.token_id, req.amount, req.recipient, height) {
        Ok(op) => Json(OpResponse { ok: true, operation: Some(op), error: None }),
        Err(e) => Json(OpResponse { ok: false, operation: None, error: Some(format!("{:?}", e)) }),
    }
}

async fn get_token_handler(
    State(state): State<crate::api::AppState>,
    Path(token_id): Path<String>,
) -> Json<serde_json::Value> {
    let ops = match &state.token_operations {
        Some(o) => o,
        None => return Json(serde_json::json!({"ok": false, "error": "not initialized"})),
    };
    match ops.get_token_info(&token_id) {
        Some(spec) => Json(serde_json::json!({"ok": true, "token": spec})),
        None => Json(serde_json::json!({"ok": false, "error": "not found"})),
    }
}

async fn list_tokens_handler(
    State(state): State<crate::api::AppState>,
) -> Json<TokensResponse> {
    let ops = match &state.token_operations {
        Some(o) => o,
        None => return Json(TokensResponse { tokens: vec![], count: 0 }),
    };
    let tokens = ops.list_tokens();
    let count = tokens.len();
    Json(TokensResponse { tokens, count })
}

async fn get_balance_handler(
    State(state): State<crate::api::AppState>,
    Path(token_id): Path<String>,
) -> Json<BalanceResponse> {
    let ops = match &state.token_operations {
        Some(o) => o,
        None => return Json(BalanceResponse { token_id, balance: 0 }),
    };
    Json(BalanceResponse { token_id: token_id.clone(), balance: ops.get_balance(&token_id) })
}

async fn list_operations_handler(
    State(state): State<crate::api::AppState>,
) -> Json<serde_json::Value> {
    let ops = match &state.token_operations {
        Some(o) => o,
        None => return Json(serde_json::json!({"operations": []})),
    };
    Json(serde_json::json!({"operations": ops.get_operations(100)}))
}

async fn create_collection_handler(
    State(state): State<crate::api::AppState>,
    Json(req): Json<CreateCollectionRequest>,
) -> Json<serde_json::Value> {
    let ops = match &state.token_operations {
        Some(o) => o,
        None => return Json(serde_json::json!({"ok": false, "error": "not initialized"})),
    };
    match ops.create_collection(
        req.name, req.description,
        req.logo_url.unwrap_or_default(),
        req.banner_url.unwrap_or_default(),
        req.category.unwrap_or_default(),
        req.socials.unwrap_or_default(),
        req.expiry.unwrap_or(-1),
        req.height.unwrap_or(0),
    ) {
        Ok(col) => Json(serde_json::json!({"ok": true, "collection": col})),
        Err(e) => Json(serde_json::json!({"ok": false, "error": format!("{:?}", e)})),
    }
}

async fn list_collections_handler(
    State(state): State<crate::api::AppState>,
) -> Json<serde_json::Value> {
    let ops = match &state.token_operations {
        Some(o) => o,
        None => return Json(serde_json::json!({"collections": []})),
    };
    Json(serde_json::json!({"collections": ops.list_collections()}))
}

async fn eip4_validate_handler(
    State(state): State<crate::api::AppState>,
    Path(token_id): Path<String>,
) -> Json<Eip4ValidationResponse> {
    let ops = match &state.token_operations {
        Some(o) => o,
        None => return Json(Eip4ValidationResponse { valid: false, errors: vec!["not initialized".into()], warnings: vec![] }),
    };
    match ops.get_token_info(&token_id) {
        Some(spec) => {
            let mut errors = vec![];
            let mut warnings = vec![];
            if let Err(e) = validate_eip4_compliance(&spec) {
                errors.push(format!("{:?}", e));
            }
            if spec.decimals > 8 {
                warnings.push("High decimals value (>8) is uncommon".into());
            }
            Json(Eip4ValidationResponse { valid: errors.is_empty(), errors, warnings })
        }
        None => Json(Eip4ValidationResponse { valid: false, errors: vec!["Token not found".into()], warnings: vec![] }),
    }
}

// ================================================================
// Router
// ================================================================

pub fn build_router(state: crate::api::AppState) -> Router<()> {
    Router::new()
        .route("/api/tokens/mint", post(mint_handler))
        .route("/api/tokens/burn", post(burn_handler))
        .route("/api/tokens/transfer", post(transfer_handler))
        .route("/api/tokens/{token_id}", get(get_token_handler))
        .route("/api/tokens", get(list_tokens_handler))
        .route("/api/tokens/{token_id}/balance", get(get_balance_handler))
        .route("/api/tokens/operations", get(list_operations_handler))
        .route("/api/tokens/collections/create", post(create_collection_handler))
        .route("/api/tokens/collections", get(list_collections_handler))
        .route("/api/tokens/{token_id}/eip4", get(eip4_validate_handler))
        .with_state(state)
}

// ================================================================
// Helpers
// ================================================================

fn sha2_hash(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result);
    out
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_coll_byte() {
        let data = b"hello";
        let encoded = encode_coll_byte(data);
        assert_eq!(encoded[0], 0x0e);
        let decoded = decode_coll_byte(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_eip4_registers_roundtrip() {
        let spec = TokenSpec {
            token_id: TokenId("a".repeat(64)),
            amount: 1_000_000,
            name: "TestToken".into(),
            decimals: 8,
            description: "A test token".into(),
            nft_url: None,
            nft_content_hash: None,
            asset_type: None,
        };
        let regs = encode_eip4_registers(&spec);
        let decoded = decode_eip4_registers(&regs);
        assert_eq!(decoded.name, spec.name);
        assert_eq!(decoded.decimals, spec.decimals);
    }

    #[test]
    fn test_nft_registers() {
        let spec = TokenSpec {
            token_id: TokenId("a".repeat(64)),
            amount: 1,
            name: "MyNFT".into(),
            decimals: 0,
            description: "An NFT".into(),
            nft_url: Some("https://example.com/nft.png".into()),
            nft_content_hash: Some("a".repeat(64)),
            asset_type: Some("0e01".into()),
        };
        let regs = encode_eip4_registers(&spec);
        assert!(regs.contains_key(&7)); // asset_type
        assert!(regs.contains_key(&8)); // content_hash
        assert!(regs.contains_key(&9)); // url
    }

    #[test]
    fn test_validate_eip4_compliance() {
        let mut spec = TokenSpec {
            token_id: TokenId("a".repeat(64)),
            amount: 100,
            name: "Valid".into(),
            decimals: 8,
            description: "OK".into(),
            nft_url: None,
            nft_content_hash: None,
            asset_type: None,
        };
        assert!(validate_eip4_compliance(&spec).is_ok());

        spec.name = "".into();
        assert!(validate_eip4_compliance(&spec).is_err());

        spec.name = "OK".into();
        spec.decimals = 19;
        assert!(validate_eip4_compliance(&spec).is_err());
    }

    #[test]
    fn test_token_id_validation() {
        assert!(validate_token_id(&"a".repeat(64)).is_ok());
        assert!(validate_token_id(&"a".repeat(32)).is_err());
    }

    #[test]
    fn test_min_box_value() {
        let val = calculate_min_box_value(100, 1, &[10, 20]);
        // base(4) + ergotree(100) + tokens(2+34=36) + regs(30) = 170 bytes * 360
        assert_eq!(val, 170 * 360);
    }

    #[test]
    fn test_token_preservation() {
        let inputs = vec![
            TokenBalance { token_id: "abc".repeat(32), name: "T".into(), amount: 100, decimals: 8 },
        ];
        let outputs = vec![
            TokenBalance { token_id: "abc".repeat(32), name: "T".into(), amount: 80, decimals: 8 },
        ];
        assert!(validate_token_preservation(&inputs, &outputs, 20).is_ok());
        assert!(validate_token_preservation(&inputs, &outputs, 10).is_err());
    }

    #[test]
    fn test_mint_and_burn() {
        let state = TokenOperationsState::new();
        let op = state.mint_token(
            "TestToken".into(), "A token".into(), 8, 1000,
            None, None, None, "recipient".into(),
            &"a".repeat(64), 800000,
        ).unwrap();
        assert_eq!(op.op_type, "mint");
        assert_eq!(state.get_balance(&op.token_id), 1000);

        state.burn_token(&op.token_id, 300, 800001).unwrap();
        assert_eq!(state.get_balance(&op.token_id), 700);
    }

    #[test]
    fn test_collection_create() {
        let state = TokenOperationsState::new();
        let col = state.create_collection(
            "Test Collection".into(), "Desc".into(),
            "logo.png".into(), "banner.png".into(),
            "art".into(), vec![("twitter".into(), "https://x.com/test".into())],
            -1, 800000,
        ).unwrap();
        assert_eq!(col.version, 1);
        assert_eq!(col.category, "art");
        let retrieved = state.get_collection(&col.id).unwrap();
        assert_eq!(retrieved.name, "Test Collection");
    }

    #[test]
    fn test_provenance() {
        let state = TokenOperationsState::new();
        let op = state.mint_token(
            "NFT".into(), "An NFT".into(), 0, 1,
            Some("https://x.com/nft".into()),
            Some("a".repeat(64)),
            Some("image".into()),
            "owner".into(),
            &"a".repeat(64), 800000,
        ).unwrap();
        let prov = state.get_provenance(&op.token_id);
        assert_eq!(prov.len(), 1);
        assert_eq!(prov[0].event, "Minted");
    }
}
