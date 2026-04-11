#![allow(dead_code)]
//! On-Chain Provider Box Scanner
//!
//! Scans the Ergo blockchain for Xergon provider boxes, validates their
//! singleton NFT, parses register state, monitors storage rent, and syncs
//! on-chain state into the relay's routing decisions.
//!
//! Provider box layout (eUTXO headless dApp pattern):
//!   Token[0]: Singleton NFT (supply=1) — provider identity
//!   R4: Provider endpoint URL (Coll[Byte])
//!   R5: PoNW score (Int)
//!   R6: Models served (Coll[Byte] — JSON-encoded Vec<String>)
//!   R7: Metadata URL (Coll[Byte])
//!   R8: Last heartbeat height (Int)
//!   R9: Reserved
//!
//! Endpoints:
//! - GET /v1/chain/providers          — list all on-chain provider boxes
//! - GET /v1/chain/providers/:pubkey — get specific provider's box
//! - GET /v1/chain/providers/:pubkey/rent — storage rent status
//! - GET /v1/chain/scan/stats        — scanner metrics
//! - GET /v1/chain/scan/trigger      — force a scan
//! - GET /v1/chain/diffs             — chain vs in-memory discrepancies

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{info, warn, error;};

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Storage rent threshold: 1,051,200 blocks (~4 years at 2-min blocks).
const STORAGE_RENT_THRESHOLD_BLOCKS: u64 = 1_051_200;

/// Approximate rent fee per 4-year cycle for a minimal box (in ERG).
const RENT_FEE_PER_CYCLE_ERG: f64 = 0.14;

/// Blocks per Ergo epoch (2 minutes).
const BLOCKS_PER_MINUTE: f64 = 0.5;

/// Risk threshold — flag boxes within this many blocks of rent collection.
const RENT_RISK_THRESHOLD_BLOCKS: u64 = 100_000;

/// Default scan interval in seconds.
const DEFAULT_SCAN_INTERVAL_SECS: u64 = 300;

// ---------------------------------------------------------------------------
// Data Types
// ---------------------------------------------------------------------------

/// Parsed registers from an on-chain provider box.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderBoxRegisters {
    /// R4: Provider endpoint URL.
    #[serde(default)]
    pub endpoint: String,
    /// R5: PoNW score.
    #[serde(default)]
    pub pown_score: u32,
    /// R6: Models served (JSON-encoded list).
    #[serde(default)]
    pub models_served: Vec<String>,
    /// R7: Metadata URL.
    #[serde(default)]
    pub metadata_url: String,
    /// R8: Last heartbeat height.
    #[serde(default)]
    pub last_heartbeat_height: u32,
}

/// Storage rent status for a provider box.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RentStatus {
    /// Blocks remaining until storage rent collection.
    pub blocks_until_rent: u64,
    /// Whether the box is at risk (within RENT_RISK_THRESHOLD_BLOCKS).
    pub is_at_risk: bool,
    /// Estimated rent fee in ERG for the current cycle.
    pub estimated_rent_erg: f64,
    /// Years remaining until rent.
    pub years_until_rent: f64,
    /// Risk level description.
    pub risk_level: String,
}

/// Full parsed on-chain provider box.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderBoxInfo {
    /// Box ID (hex).
    pub box_id: String,
    /// Provider public key.
    pub provider_pubkey: String,
    /// Singleton NFT token ID.
    pub nft_token_id: String,
    /// ErgoTree hex.
    pub ergo_tree: String,
    /// Box value in nanoERG.
    pub value_nanoerg: u64,
    /// Block height when this box was created.
    pub creation_height: u32,
    /// Parsed registers.
    pub registers: ProviderBoxRegisters,
    /// Whether the box passed all validation checks.
    pub is_valid: bool,
    /// Validation failure reason (if invalid).
    pub validation_error: Option<String>,
    /// Storage rent status.
    pub rent_status: RentStatus,
    /// Current chain height when last seen.
    pub last_seen_height: u32,
    /// Transaction ID that created this box.
    pub tx_id: String,
}

/// Chain vs in-memory discrepancy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainDiff {
    pub provider_pubkey: String,
    pub diff_type: String,
    pub chain_value: String,
    pub memory_value: String,
    pub detected_at: String,
}

/// Scanner metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanStats {
    pub total_scans: u64,
    pub scan_height: u64,
    pub known_boxes: usize,
    pub boxes_at_risk: u64,
    pub invalid_boxes: usize,
    pub last_scan_time: String,
    pub scan_interval_secs: u64,
    pub diffs_detected: usize,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Scanner state for tracking on-chain provider boxes.
pub struct ProviderBoxScannerState {
    /// Known provider boxes keyed by box_id.
    pub known_boxes: DashMap<String, ProviderBoxInfo>,
    /// Provider boxes indexed by pubkey for fast lookup.
    pub pubkey_index: DashMap<String, String>, // pubkey -> box_id
    /// Current scan height.
    pub scan_height: AtomicU64,
    /// Total scans performed.
    pub total_scans: AtomicU64,
    /// Number of boxes at rent risk.
    pub boxes_at_risk: AtomicU64,
    /// Last scan timestamp per provider (pubkey -> unix ms).
    pub last_scan_time: DashMap<String, i64>,
    /// Invalid boxes (box_id -> reason).
    pub invalid_boxes: DashMap<String, String>,
    /// Detected chain vs in-memory diffs.
    pub chain_diffs: DashMap<String, ChainDiff>,
    /// Scan interval in seconds.
    pub scan_interval_secs: AtomicU64,
}

impl ProviderBoxScannerState {
    pub fn new() -> Self {
        Self {
            known_boxes: DashMap::new(),
            pubkey_index: DashMap::new(),
            scan_height: AtomicU64::new(0),
            total_scans: AtomicU64::new(0),
            boxes_at_risk: AtomicU64::new(0),
            last_scan_time: DashMap::new(),
            invalid_boxes: DashMap::new(),
            chain_diffs: DashMap::new(),
            scan_interval_secs: AtomicU64::new(DEFAULT_SCAN_INTERVAL_SECS),
        }
    }
}

impl Default for ProviderBoxScannerState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Core Logic
// ---------------------------------------------------------------------------

/// Calculate storage rent status for a box.
pub fn calculate_rent_status(creation_height: u32, current_height: u32) -> RentStatus {
    let age_blocks = if current_height > creation_height {
        (current_height - creation_height) as u64
    } else {
        0
    };

    let blocks_until_rent = if STORAGE_RENT_THRESHOLD_BLOCKS > age_blocks {
        STORAGE_RENT_THRESHOLD_BLOCKS - age_blocks
    } else {
        0
    };

    let is_at_risk = blocks_until_rent <= RENT_RISK_THRESHOLD_BLOCKS;
    let years_until_rent = blocks_until_rent as f64 * BLOCKS_PER_MINUTE / (60.0 * 24.0 * 365.25);
    let estimated_rent_erg = RENT_FEE_PER_CYCLE_ERG;

    let risk_level = if blocks_until_rent == 0 {
        "CRITICAL — rent collection active".to_string()
    } else if is_at_risk {
        format!("WARNING — {} blocks until rent", blocks_until_rent)
    } else if years_until_rent < 1.0 {
        format!("NOTICE — {:.1} years until rent", years_until_rent)
    } else {
        format!("OK — {:.1} years until rent", years_until_rent)
    };

    RentStatus {
        blocks_until_rent,
        is_at_risk,
        estimated_rent_erg,
        years_until_rent,
        risk_level,
    }
}

/// Validate that a provider box has the required structure.
pub fn validate_provider_box(
    has_singleton_nft: bool,
    has_endpoint: bool,
    has_ergo_tree: bool,
    value_nanoerg: u64,
    min_value: u64,
) -> Result<(), String> {
    if !has_ergo_tree {
        return Err("Missing ErgoTree".to_string());
    }
    if !has_singleton_nft {
        return Err("Missing singleton NFT (token[0] with supply=1)".to_string());
    }
    if !has_endpoint {
        return Err("Missing endpoint in R4".to_string());
    }
    if value_nanoerg < min_value {
        return Err(format!(
            "Insufficient box value: {} nanoERG < {} minimum",
            value_nanoerg, min_value
        ));
    }
    Ok(())
}

/// Parse a simulated box JSON response into ProviderBoxInfo.
/// In production this would parse the actual Ergo node / explorer API response.
pub fn parse_box_from_json(box_json: &Value, current_height: u32) -> ProviderBoxInfo {
    let box_id = box_json["boxId"].as_str().unwrap_or("unknown").to_string();
    let tx_id = box_json["transactionId"].as_str().unwrap_or("unknown").to_string();
    let ergo_tree = box_json["ergoTree"].as_str().unwrap_or("").to_string();
    let value_nanoerg = box_json["value"].as_u64().unwrap_or(0);
    let creation_height = box_json["creationHeight"].as_u64().unwrap_or(0) as u32;

    // Parse singleton NFT from tokens
    let tokens = box_json["assets"].as_array().cloned().unwrap_or_default();
    let (has_nft, nft_token_id) = if !tokens.is_empty() {
        let first_token = &tokens[0];
        let token_id = first_token["tokenId"].as_str().unwrap_or("").to_string();
        let amount = first_token["amount"].as_u64().unwrap_or(0);
        (amount == 1, token_id)
    } else {
        (false, String::new())
    };

    // Parse additionalRegisters (R4-R9)
    let registers_json = box_json["additionalRegisters"].as_object().cloned().unwrap_or_default();

    let r4_value = registers_json
        .get("R4")
        .and_then(|v| v["value"].as_str())
        .unwrap_or("");

    // Parse registers — in real Ergo, these are Sigma-type encoded.
    // For now, extract string content from the serialized STypeColl/STypeString.
    let endpoint = extract_string_from_register(r4_value);

    let r5_raw = registers_json
        .get("R5")
        .and_then(|v| v["value"].as_str())
        .unwrap_or("0");
    let pown_score = extract_int_from_register(r5_raw) as u32;

    let r6_raw = registers_json
        .get("R6")
        .and_then(|v| v["value"].as_str())
        .unwrap_or("[]");
    let models_served = extract_string_list_from_register(r6_raw);

    let r7_raw = registers_json
        .get("R7")
        .and_then(|v| v["value"].as_str())
        .unwrap_or("");
    let metadata_url = extract_string_from_register(r7_raw);

    let r8_raw = registers_json
        .get("R8")
        .and_then(|v| v["value"].as_str())
        .unwrap_or("0");
    let last_heartbeat_height = extract_int_from_register(r8_raw) as u32;

    let registers = ProviderBoxRegisters {
        endpoint: endpoint.clone(),
        pown_score,
        models_served,
        metadata_url,
        last_heartbeat_height,
    };

    let rent_status = calculate_rent_status(creation_height, current_height);

    // Validate
    let validation = validate_provider_box(has_nft, !endpoint.is_empty(), !ergo_tree.is_empty(), value_nanoerg, 1_000_000);
    let (is_valid, validation_error) = match validation {
        Ok(()) => (true, None),
        Err(e) => (false, Some(e)),
    };

    // Derive provider pubkey from the box (in production, from R4 or a separate mapping).
    // Here we use the first 20 chars of box_id as a mock pubkey.
    let provider_pubkey = if box_id.len() >= 20 {
        box_id[..20].to_string()
    } else {
        box_id.clone()
    };

    ProviderBoxInfo {
        box_id: box_id.clone(),
        provider_pubkey,
        nft_token_id,
        ergo_tree,
        value_nanoerg,
        creation_height,
        registers,
        is_valid,
        validation_error,
        rent_status,
        last_seen_height: current_height,
        tx_id,
    }
}

/// Extract a UTF-8 string from a register value (handles both raw and STypeColl encoded).
fn extract_string_from_register(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    // If the raw value looks like a hex-encoded string (starts with 0e followed by length),
    // decode it. Otherwise return as-is.
    if raw.starts_with("0e") && raw.len() > 4 {
        let len_hex = &raw[2..4];
        if let Ok(len) = u8::from_str_radix(len_hex, 16) {
            let start = 4;
            let end = start + (len as usize) * 2;
            if end <= raw.len() {
                let hex_bytes = &raw[start..end];
                return decode_hex_string(hex_bytes).unwrap_or_else(|_| raw.to_string());
            }
        }
    }
    raw.to_string()
}

/// Extract an integer from a register value (handles STypeInt encoding).
fn extract_int_from_register(raw: &str) -> i64 {
    if raw.is_empty() {
        return 0;
    }
    // Ergo STypeInt is encoded as the hex representation of a signed 64-bit big-endian integer.
    if raw.len() == 16 && raw.chars().all(|c| c.is_ascii_hexdigit()) {
        if let Ok(bytes) = hex_decode(raw) {
            if bytes.len() == 8 {
                let arr: [u8; 8] = bytes.try_into().unwrap_or([0u8; 8]);
                return i64::from_be_bytes(arr);
            }
        }
    }
    raw.parse::<i64>().unwrap_or(0)
}

/// Extract a list of strings from a register (handles STypeColl encoded JSON).
fn extract_string_list_from_register(raw: &str) -> Vec<String> {
    let inner = extract_string_from_register(raw);
    if inner.is_empty() {
        return Vec::new();
    }
    // Try to parse as JSON array
    if inner.starts_with('[') {
        serde_json::from_str(&inner).unwrap_or_else(|_| vec![inner])
    } else {
        inner.split(',').map(|s| s.trim().to_string()).collect()
    }
}

/// Decode hex string to UTF-8.
fn decode_hex_string(hex: &str) -> Result<String, String> {
    let bytes = hex_decode(hex).map_err(|e| format!("Hex decode error: {:?}", e))?;
    String::from_utf8(bytes).map_err(|e| format!("UTF-8 decode error: {}", e))
}

/// Decode hex string to bytes.
fn hex_decode(hex: &str) -> Result<Vec<u8>, hex::FromHexError> {
    hex::FromHex::from_hex(hex)
}

/// Count boxes currently at rent risk in the scanner state.
pub fn count_boxes_at_risk(state: &ProviderBoxScannerState) -> u64 {
    let mut count = 0u64;
    for entry in state.known_boxes.iter() {
        if entry.value().rent_status.is_at_risk {
            count += 1;
        }
    }
    count
}

/// Detect discrepancies between on-chain state and in-memory provider registry.
pub fn detect_chain_diffs(
    scanner_state: &ProviderBoxScannerState,
    memory_endpoints: &DashMap<String, String>,
) -> Vec<ChainDiff> {
    let mut diffs = Vec::new();
    for entry in scanner_state.known_boxes.iter() {
        let box_info = entry.value();
        if let Some(mem_endpoint) = memory_endpoints.get(&box_info.provider_pubkey) {
            if mem_endpoint.value() != &box_info.registers.endpoint {
                diffs.push(ChainDiff {
                    provider_pubkey: box_info.provider_pubkey.clone(),
                    diff_type: "endpoint_mismatch".to_string(),
                    chain_value: box_info.registers.endpoint.clone(),
                    memory_value: mem_endpoint.value().clone(),
                    detected_at: chrono::Utc::now().to_rfc3339(),
                });
            }
        }
    }
    diffs
}

// ---------------------------------------------------------------------------
// REST Handlers
// ---------------------------------------------------------------------------

/// GET /v1/chain/providers — list all on-chain provider boxes
async fn list_chain_providers(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let scanner = &state.provider_box_scanner;
    let mut providers: Vec<ProviderBoxInfo> = scanner
        .known_boxes
        .iter()
        .filter(|e| e.value().is_valid)
        .map(|e| e.value().clone())
        .collect();
    providers.sort_by(|a, b| b.registers.pown_score.cmp(&a.registers.pown_score));
    (StatusCode::OK, Json(json!({ "providers": providers, "count": providers.len() })))
}

/// GET /v1/chain/providers/:pubkey — get specific provider's box
async fn get_chain_provider(
    State(state): State<AppState>,
    Path(pubkey): Path<String>,
) -> impl IntoResponse {
    let scanner = &state.provider_box_scanner;
    if let Some(box_id) = scanner.pubkey_index.get(&pubkey) {
        if let Some(box_info) = scanner.known_boxes.get(box_id.value()) {
            return (
                StatusCode::OK,
                Json(json!({ "provider": box_info.value().clone() })),
            );
        }
    }
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": format!("Provider {} not found on-chain", pubkey) })),
    )
}

/// GET /v1/chain/providers/:pubkey/rent — storage rent status
async fn get_provider_rent_status(
    State(state): State<AppState>,
    Path(pubkey): Path<String>,
) -> impl IntoResponse {
    let scanner = &state.provider_box_scanner;
    if let Some(box_id) = scanner.pubkey_index.get(&pubkey) {
        if let Some(box_info) = scanner.known_boxes.get(box_id.value()) {
            let rent = &box_info.value().rent_status;
            return (
                StatusCode::OK,
                Json(json!({
                    "provider_pubkey": pubkey,
                    "box_id": box_id.value(),
                    "rent_status": rent,
                })),
            );
        }
    }
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": format!("Provider {} not found", pubkey) })),
    )
}

/// GET /v1/chain/scan/stats — scanner metrics
async fn get_scan_stats(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let scanner = &state.provider_box_scanner;
    let stats = ScanStats {
        total_scans: scanner.total_scans.load(Ordering::Relaxed),
        scan_height: scanner.scan_height.load(Ordering::Relaxed),
        known_boxes: scanner.known_boxes.len(),
        boxes_at_risk: count_boxes_at_risk(scanner),
        invalid_boxes: scanner.invalid_boxes.len(),
        last_scan_time: chrono::Utc::now().to_rfc3339(),
        scan_interval_secs: scanner.scan_interval_secs.load(Ordering::Relaxed),
        diffs_detected: scanner.chain_diffs.len(),
    };
    (StatusCode::OK, Json(json!({ "stats": stats })))
}

/// GET /v1/chain/scan/trigger — force a scan (placeholder)
async fn trigger_scan(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let scanner = &state.provider_box_scanner;
    scanner.total_scans.fetch_add(1, Ordering::Relaxed);
    info!("Manual scan triggered via API");
    (
        StatusCode::OK,
        Json(json!({
            "message": "Scan triggered",
            "total_scans": scanner.total_scans.load(Ordering::Relaxed),
        })),
    )
}

/// GET /v1/chain/diffs — chain vs in-memory discrepancies
async fn get_chain_diffs(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let scanner = &state.provider_box_scanner;
    let diffs: Vec<ChainDiff> = scanner
        .chain_diffs
        .iter()
        .map(|e| e.value().clone())
        .collect();
    (StatusCode::OK, Json(json!({ "diffs": diffs, "count": diffs.len() })))
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the provider box scanner router.
pub fn build_provider_box_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/v1/chain/providers", get(list_chain_providers))
        .route("/v1/chain/providers/{pubkey}", get(get_chain_provider))
        .route("/v1/chain/providers/{pubkey}/rent", get(get_provider_rent_status))
        .route("/v1/chain/scan/stats", get(get_scan_stats))
        .route("/v1/chain/scan/trigger", get(trigger_scan))
        .route("/v1/chain/diffs", get(get_chain_diffs))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rent_status_new_box() {
        let status = calculate_rent_status(1_000_000, 1_000_001);
        assert_eq!(status.blocks_until_rent, STORAGE_RENT_THRESHOLD_BLOCKS - 1);
        assert!(!status.is_at_risk);
        assert!(status.years_until_rent > 3.0);
        assert!(status.risk_level.contains("OK"));
    }

    #[test]
    fn test_rent_status_at_risk() {
        let current = 1_000_000;
        let creation = (current as u64 - RENT_RISK_THRESHOLD_BLOCKS + 1000) as u32;
        let status = calculate_rent_status(creation, current);
        assert!(status.is_at_risk);
        assert!(status.risk_level.contains("WARNING"));
    }

    #[test]
    fn test_rent_status_expired() {
        let creation = 1u32;
        let current = (STORAGE_RENT_THRESHOLD_BLOCKS + 1000) as u32;
        let status = calculate_rent_status(creation, current);
        assert_eq!(status.blocks_until_rent, 0);
        assert!(status.is_at_risk);
        assert!(status.risk_level.contains("CRITICAL"));
    }

    #[test]
    fn test_rent_status_future_height() {
        let status = calculate_rent_status(1000, 500);
        assert_eq!(status.blocks_until_rent, STORAGE_RENT_THRESHOLD_BLOCKS);
        assert!(!status.is_at_risk);
    }

    #[test]
    fn test_validate_box_valid() {
        let result = validate_provider_box(true, true, true, 5_000_000, 1_000_000);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_box_missing_nft() {
        let result = validate_provider_box(false, true, true, 5_000_000, 1_000_000);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("NFT"));
    }

    #[test]
    fn test_validate_box_missing_endpoint() {
        let result = validate_provider_box(true, false, true, 5_000_000, 1_000_000);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("endpoint"));
    }

    #[test]
    fn test_validate_box_insufficient_value() {
        let result = validate_provider_box(true, true, true, 500_000, 1_000_000);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Insufficient"));
    }

    #[test]
    fn test_validate_box_missing_ergo_tree() {
        let result = validate_provider_box(true, true, false, 5_000_000, 1_000_000);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("ErgoTree"));
    }

    #[test]
    fn test_extract_int_from_register_hex() {
        // 64-bit signed int for value 42
        let hex = "000000000000002a";
        let val = extract_int_from_register(hex);
        assert_eq!(val, 42);
    }

    #[test]
    fn test_extract_int_from_register_empty() {
        let val = extract_int_from_register("");
        assert_eq!(val, 0);
    }

    #[test]
    fn test_extract_int_from_register_decimal() {
        let val = extract_int_from_register("123");
        assert_eq!(val, 123);
    }

    #[test]
    fn test_extract_string_from_register_plain() {
        let val = extract_string_from_register("https://example.com");
        assert_eq!(val, "https://example.com");
    }

    #[test]
    fn test_extract_string_from_register_empty() {
        let val = extract_string_from_register("");
        assert_eq!(val, "");
    }

    #[test]
    fn test_extract_string_list_from_register_json() {
        let val = extract_string_list_from_register(r#"["llama3","qwen2"]"#);
        assert_eq!(val, vec!["llama3", "qwen2"]);
    }

    #[test]
    fn test_extract_string_list_from_register_comma() {
        let val = extract_string_list_from_register("llama3,qwen2");
        assert_eq!(val, vec!["llama3", "qwen2"]);
    }

    #[test]
    fn test_provider_box_scanner_state_new() {
        let state = ProviderBoxScannerState::new();
        assert_eq!(state.total_scans.load(Ordering::Relaxed), 0);
        assert_eq!(state.known_boxes.len(), 0);
        assert_eq!(state.scan_interval_secs.load(Ordering::Relaxed), DEFAULT_SCAN_INTERVAL_SECS);
    }

    #[test]
    fn test_count_boxes_at_risk() {
        let state = ProviderBoxScannerState::new();
        let box_info = ProviderBoxInfo {
            box_id: "test1".to_string(),
            provider_pubkey: "pk1".to_string(),
            nft_token_id: "nft1".to_string(),
            ergo_tree: "tree1".to_string(),
            value_nanoerg: 5_000_000,
            creation_height: 1,
            registers: ProviderBoxRegisters {
                endpoint: "http://test".to_string(),
                pown_score: 100,
                models_served: vec![],
                metadata_url: String::new(),
                last_heartbeat_height: 0,
            },
            is_valid: true,
            validation_error: None,
            rent_status: RentStatus {
                blocks_until_rent: 50000,
                is_at_risk: true,
                estimated_rent_erg: 0.14,
                years_until_rent: 0.95,
                risk_level: "WARNING".to_string(),
            },
            last_seen_height: 1_000_000,
            tx_id: "tx1".to_string(),
        };
        state.known_boxes.insert("test1".to_string(), box_info);
        assert_eq!(count_boxes_at_risk(&state), 1);
    }

    #[test]
    fn test_detect_chain_diffs() {
        let scanner = ProviderBoxScannerState::new();
        let memory = DashMap::new();
        memory.insert("pk1".to_string(), "http://old-endpoint".to_string());

        let box_info = ProviderBoxInfo {
            box_id: "test1".to_string(),
            provider_pubkey: "pk1".to_string(),
            nft_token_id: "nft1".to_string(),
            ergo_tree: "tree1".to_string(),
            value_nanoerg: 5_000_000,
            creation_height: 1,
            registers: ProviderBoxRegisters {
                endpoint: "http://new-endpoint".to_string(),
                pown_score: 100,
                models_served: vec![],
                metadata_url: String::new(),
                last_heartbeat_height: 0,
            },
            is_valid: true,
            validation_error: None,
            rent_status: RentStatus {
                blocks_until_rent: 1_000_000,
                is_at_risk: false,
                estimated_rent_erg: 0.14,
                years_until_rent: 3.8,
                risk_level: "OK".to_string(),
            },
            last_seen_height: 1_000_000,
            tx_id: "tx1".to_string(),
        };
        scanner.known_boxes.insert("test1".to_string(), box_info);
        scanner.pubkey_index.insert("pk1".to_string(), "test1".to_string());

        let diffs = detect_chain_diffs(&scanner, &memory);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].diff_type, "endpoint_mismatch");
        assert_eq!(diffs[0].chain_value, "http://new-endpoint");
    }

    #[test]
    fn test_detect_chain_diffs_no_diff() {
        let scanner = ProviderBoxScannerState::new();
        let memory = DashMap::new();
        memory.insert("pk1".to_string(), "http://same".to_string());

        let box_info = ProviderBoxInfo {
            box_id: "test1".to_string(),
            provider_pubkey: "pk1".to_string(),
            nft_token_id: "nft1".to_string(),
            ergo_tree: "tree1".to_string(),
            value_nanoerg: 5_000_000,
            creation_height: 1,
            registers: ProviderBoxRegisters {
                endpoint: "http://same".to_string(),
                pown_score: 100,
                models_served: vec![],
                metadata_url: String::new(),
                last_heartbeat_height: 0,
            },
            is_valid: true,
            validation_error: None,
            rent_status: RentStatus {
                blocks_until_rent: 1_000_000,
                is_at_risk: false,
                estimated_rent_erg: 0.14,
                years_until_rent: 3.8,
                risk_level: "OK".to_string(),
            },
            last_seen_height: 1_000_000,
            tx_id: "tx1".to_string(),
        };
        scanner.known_boxes.insert("test1".to_string(), box_info);
        scanner.pubkey_index.insert("pk1".to_string(), "test1".to_string());

        let diffs = detect_chain_diffs(&scanner, &memory);
        assert_eq!(diffs.len(), 0);
    }

    #[test]
    fn test_scan_stats_serialization() {
        let stats = ScanStats {
            total_scans: 5,
            scan_height: 1_000_000,
            known_boxes: 3,
            boxes_at_risk: 1,
            invalid_boxes: 0,
            last_scan_time: "2026-01-01T00:00:00Z".to_string(),
            scan_interval_secs: 300,
            diffs_detected: 2,
        };
        let json = serde_json::to_value(&stats).unwrap();
        assert_eq!(json["total_scans"], 5);
        assert_eq!(json["known_boxes"], 3);
    }

    #[test]
    fn test_rent_status_serialization() {
        let status = RentStatus {
            blocks_until_rent: 500_000,
            is_at_risk: false,
            estimated_rent_erg: 0.14,
            years_until_rent: 1.9,
            risk_level: "OK — 1.9 years until rent".to_string(),
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["is_at_risk"], false);
        assert_eq!(json["blocks_until_rent"], 500_000);
    }

    #[test]
    fn test_provider_box_info_serialization() {
        let info = ProviderBoxInfo {
            box_id: "abc123".to_string(),
            provider_pubkey: "pk_test".to_string(),
            nft_token_id: "nft_test".to_string(),
            ergo_tree: "tree_hex".to_string(),
            value_nanoerg: 10_000_000,
            creation_height: 500_000,
            registers: ProviderBoxRegisters {
                endpoint: "http://provider:9090".to_string(),
                pown_score: 85,
                models_served: vec!["llama3:70b".to_string()],
                metadata_url: "https://meta.example.com".to_string(),
                last_heartbeat_height: 999_000,
            },
            is_valid: true,
            validation_error: None,
            rent_status: RentStatus {
                blocks_until_rent: 551_200,
                is_at_risk: false,
                estimated_rent_erg: 0.14,
                years_until_rent: 2.1,
                risk_level: "OK".to_string(),
            },
            last_seen_height: 1_000_000,
            tx_id: "tx_abc".to_string(),
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["box_id"], "abc123");
        assert_eq!(json["registers"]["pown_score"], 85);
        assert_eq!(json["is_valid"], true);
        assert!(json["validation_error"].is_null());
    }
}
