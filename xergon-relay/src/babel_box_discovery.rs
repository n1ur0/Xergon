#![allow(dead_code)]
//! Babel Box Discovery — EIP-0031 Babel fee box discovery and token fee swap
//!
//! Enables users to pay inference fees in tokens (not ERG) using Babel boxes on Ergo.
//! Babel boxes are on-chain UTXOs that swap tokens for ERG to cover miner fees.
//!
//! Features:
//! - Construct Babel ErgoTrees for both compact and with-size headers
//! - Discover available Babel boxes via Ergo Explorer API
//! - Decode token prices from Sigma-serialized registers (zigzag VLQ)
//! - Select optimal Babel box by ERG liquidity
//! - Calculate token swap amounts for ERG fee coverage
//! - REST endpoints for discovery, selection, swap calculation, and pricing

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info, warn};

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Ergo nanoERG per ERG
pub const NANOERG_PER_ERG: u64 = 1_000_000_000;

/// Ergo transaction fee in nanoERG
pub const TX_FEE: u64 = 1_100_000;

/// Minimum box value in nanoERG
pub const MIN_BOX_VALUE: u64 = 1_000_000;

/// Ergo Explorer API base URL
const EXPLORER_BASE: &str = "https://api.ergoplatform.com/api/v1";

/// Babel contract body template. The `{tokenId}` placeholder is replaced with
/// the 32-byte (64 hex char) token ID.
const BABEL_BODY_TEMPLATE: &str =
    "0604000e20{tokenId}0400040005000500d803d601e30004d602e4c6a70408d603e4c6a7050595e67201d804d604b2a5e4720100d605b2db63087204730000d606db6308a7d60799c1a7c17204d1968302019683050193c27204c2a7938c720501730193e4c672040408720293e4c672040505720393e4c67204060ec5a796830201929c998c7205029591b1720673028cb272067303000273047203720792720773057202";

/// Compact header for Babel ErgoTree
const BABEL_HEADER_COMPACT: &str = "10";

/// With-size header for Babel ErgoTree
const BABEL_HEADER_WITH_SIZE: &str = "18c101";

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Token amount held in a box
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenAmount {
    pub token_id: String,
    pub amount: u64,
}

/// A Babel box discovered on-chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BabelBox {
    pub box_id: String,
    /// Box value in nanoERG
    pub value: u64,
    /// Serialized ErgoTree
    pub ergo_tree: String,
    /// Tokens held in the box
    pub tokens: Vec<TokenAmount>,
    /// Additional registers (R4-R9 flattened from Explorer format)
    /// Key format: "R4", "R5", etc. Value is the serialized hex string.
    pub additional_registers: HashMap<String, String>,
}

impl BabelBox {
    /// Extract the token price (nanoERG per raw token unit) from R5 register.
    /// R5 is Sigma-serialized SLong: byte 0 = 0x05, then zigzag VLQ.
    pub fn token_price(&self) -> Option<i64> {
        let r5_hex = self.additional_registers.get("R5")?;
        let serialized = r5_hex.strip_prefix("0x").unwrap_or(r5_hex);
        if serialized.is_empty() || serialized.len() < 2 {
            return None;
        }
        // Skip type byte 0x05
        let value_hex = &serialized[2..];
        if value_hex.is_empty() {
            return None;
        }
        Some(decode_slong_value(value_hex))
    }

    /// Get the creator pubkey hex from R4 register.
    pub fn creator_pubkey(&self) -> Option<String> {
        self.additional_registers.get("R4").cloned()
    }

    /// Get the original box ID from R6 register.
    pub fn original_box_id(&self) -> Option<String> {
        self.additional_registers.get("R6").cloned()
    }
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

/// Query parameters for discover endpoint
#[derive(Debug, Deserialize)]
pub struct DiscoverQuery {
    /// Maximum number of boxes to return (default: 20)
    pub limit: Option<usize>,
}

/// Response for Babel box discovery
#[derive(Debug, Serialize)]
pub struct DiscoverResponse {
    pub token_id: String,
    pub boxes: Vec<BabelBox>,
    pub total_found: usize,
}

/// Response for box selection
#[derive(Debug, Serialize)]
pub struct SelectResponse {
    pub token_id: String,
    pub required_erg: u64,
    pub selected_box: Option<BabelBox>,
    pub token_price: Option<i64>,
    pub tokens_needed: Option<u64>,
}

/// Request body for swap calculation
#[derive(Debug, Deserialize)]
pub struct CalculateSwapRequest {
    /// Token ID to use for the swap
    pub token_id: String,
    /// Token price in nanoERG per raw token unit
    pub token_price: i64,
    /// Amount of ERG needed in nanoERG
    pub erg_needed: u64,
}

/// Response for swap calculation
#[derive(Debug, Serialize)]
pub struct CalculateSwapResponse {
    pub token_id: String,
    pub token_price: i64,
    pub erg_needed: u64,
    pub tokens_required: u64,
    pub erg_value_formatted: String,
}

/// Response for price query
#[derive(Debug, Serialize)]
pub struct PriceResponse {
    pub token_id: String,
    pub token_price: Option<i64>,
    pub erg_value_formatted: Option<String>,
    pub box_id: Option<String>,
    pub box_value: Option<u64>,
}

/// Error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ---------------------------------------------------------------------------
// Explorer API response types (internal deserialization)
// ---------------------------------------------------------------------------

/// Single item in the Explorer API items array
#[derive(Debug, Deserialize)]
struct ExplorerBoxItem {
    #[serde(rename = "boxId")]
    box_id: String,
    #[serde(rename = "value")]
    value: u64,
    #[serde(rename = "ergoTree")]
    ergo_tree: String,
    #[serde(rename = "assets")]
    assets: Vec<ExplorerAsset>,
    #[serde(rename = "additionalRegisters")]
    additional_registers: HashMap<String, ExplorerRegister>,
}

/// Asset in Explorer response
#[derive(Debug, Deserialize)]
struct ExplorerAsset {
    #[serde(rename = "tokenId")]
    token_id: String,
    amount: u64,
}

/// Register value from Explorer (has serializedValue, sigmaType, renderedValue)
#[derive(Debug, Deserialize)]
struct ExplorerRegister {
    #[serde(rename = "serializedValue")]
    serialized_value: String,
    #[serde(rename = "sigmaType")]
    _sigma_type: String,
    #[serde(rename = "renderedValue")]
    _rendered_value: Option<String>,
}

/// Explorer API list response wrapper
#[derive(Debug, Deserialize)]
struct ExplorerBoxesResponse {
    items: Vec<ExplorerBoxItem>,
    #[serde(rename = "total")]
    total: usize,
}

// ---------------------------------------------------------------------------
// ErgoTree construction
// ---------------------------------------------------------------------------

/// Build both valid Babel ErgoTree strings for a given token ID.
/// Returns [compact_header + body, with_size_header + body].
///
/// The token_id must be a 32-byte hex string (64 hex characters).
pub fn get_babel_ergo_trees(token_id: &str) -> [String; 2] {
    let body = BABEL_BODY_TEMPLATE.replace("{tokenId}", token_id);
    let compact = format!("{}{}", BABEL_HEADER_COMPACT, body);
    let with_size = format!("{}{}", BABEL_HEADER_WITH_SIZE, body);
    [compact, with_size]
}

// ---------------------------------------------------------------------------
// Zigzag VLQ encoding/decoding (Sigma protocol)
// ---------------------------------------------------------------------------

/// Encode a value using zigzag encoding: 0 -> 0, -1 -> 1, 1 -> 2, -2 -> 3, ...
#[inline]
fn zigzag_encode(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

/// Decode a zigzag-encoded value back to signed
#[inline]
fn zigzag_decode(encoded: u64) -> i64 {
    ((encoded >> 1) as i64) ^ -((encoded & 1) as i64)
}

/// Encode a value as a variable-length quantity (VLQ) into hex bytes.
/// Each byte uses 7 bits for data and 1 bit (MSB) as continuation flag.
fn encode_vlq(value: u64) -> Vec<u8> {
    let mut result = Vec::new();
    let mut v = value;
    loop {
        let mut byte = (v & 0x7F) as u8;
        v >>= 7;
        if v != 0 {
            byte |= 0x80; // Set continuation bit
        }
        result.push(byte);
        if v == 0 {
            break;
        }
    }
    result
}

/// Decode a VLQ-encoded value from hex string.
/// Returns the decoded value and the number of hex characters consumed.
fn decode_vlq(hex: &str) -> (u64, usize) {
    let mut result: u64 = 0;
    let mut shift: u32 = 0;
    let mut chars_consumed: usize = 0;

    for chunk in hex.as_bytes().chunks(2) {
        if chunk.len() < 2 {
            break;
        }
        let byte = u8::from_str_radix(
            std::str::from_utf8(chunk).unwrap_or("00"),
            16,
        )
        .unwrap_or(0);
        result |= ((byte & 0x7F) as u64) << shift;
        chars_consumed += 2;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }

    (result, chars_consumed)
}

/// Decode a Sigma-serialized SLong value from hex.
/// Expects the value portion after the type byte (0x05).
fn decode_slong_value(hex: &str) -> i64 {
    let (encoded, _) = decode_vlq(hex);
    zigzag_decode(encoded)
}

/// Decode a full Sigma-serialized SLong from hex (including type byte 0x05).
pub fn decode_slong(hex: &str) -> i64 {
    let cleaned = hex.strip_prefix("0x").unwrap_or(hex);
    if cleaned.len() < 4 {
        // Need at least type byte (2 hex chars) + one VLQ byte (2 hex chars)
        return 0;
    }
    let type_byte = u8::from_str_radix(&cleaned[0..2], 16).unwrap_or(0);
    if type_byte != 0x05 {
        debug!("Unexpected SLong type byte: 0x{:02x}, expected 0x05", type_byte);
        return 0;
    }
    decode_slong_value(&cleaned[2..])
}

// ---------------------------------------------------------------------------
// Sigma serialization helpers
// ---------------------------------------------------------------------------

/// Sigma-serialize a signed 32-bit integer.
/// Format: type byte 0x04 + zigzag-encoded VLQ value.
pub fn sigma_serialize_sint(value: i32) -> String {
    let zigzagged = zigzag_encode(value as i64);
    let vlq_bytes = encode_vlq(zigzagged);
    let mut result = String::from("04");
    for b in &vlq_bytes {
        result.push_str(&format!("{:02x}", b));
    }
    result
}

/// Sigma-serialize a Coll[Byte] from a hex string.
/// Format: type byte 0x0e + VLQ-encoded length + raw bytes.
pub fn sigma_coll_byte_from_hex(hex: &str) -> String {
    let cleaned = hex.strip_prefix("0x").unwrap_or(hex);
    let byte_count = cleaned.len() / 2;
    let length_vlq = encode_vlq(byte_count as u64);
    let mut result = String::from("0e");
    for b in &length_vlq {
        result.push_str(&format!("{:02x}", b));
    }
    result.push_str(cleaned);
    result
}

// ---------------------------------------------------------------------------
// Box selection and swap calculation
// ---------------------------------------------------------------------------

/// Select the best Babel box for covering the required ERG amount.
/// Finds the first box (sorted by ERG value descending) with enough value
/// to cover required_nanoerg + MIN_BOX_VALUE (box must remain valid).
pub fn select_babel_box(boxes: &[BabelBox], required_nanoerg: u64) -> Option<&BabelBox> {
    let needed = required_nanoerg.saturating_add(MIN_BOX_VALUE);
    boxes.iter().find(|b| b.value >= needed)
}

/// Calculate the number of tokens needed to cover the ERG requirement.
/// Formula: ergNeeded / tokenPrice + 1 (rounding safety margin).
/// Returns 0 if token_price is <= 0.
pub fn calculate_swap(token_price: i64, erg_needed: u64) -> u64 {
    if token_price <= 0 {
        return 0;
    }
    let tokens = erg_needed / (token_price as u64);
    tokens.saturating_add(1) // Rounding safety
}

// ---------------------------------------------------------------------------
// Explorer API integration
// ---------------------------------------------------------------------------

/// Find available Babel boxes for a given token ID by querying the Ergo Explorer API.
/// Tries both ErgoTree header formats and merges results, deduplicating by box_id.
/// Results are sorted by ERG value descending (most liquidity first).
pub async fn find_babel_boxes(
    http_client: &Client,
    token_id: &str,
) -> Result<Vec<BabelBox>, String> {
    let trees = get_babel_ergo_trees(token_id);
    let mut all_boxes: HashMap<String, BabelBox> = HashMap::new();

    for (i, ergo_tree) in trees.iter().enumerate() {
        let url = format!(
            "{}/boxes/unspent/byErgoTree/{}",
            EXPLORER_BASE, ergo_tree
        );
        debug!(
            "Querying Explorer API (header variant {}): {}",
            i, url
        );

        let response = match http_client
            .get(&url)
            .header("Accept", "application/json")
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                warn!("Explorer API request failed for variant {}: {}", i, e);
                continue;
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            warn!(
                "Explorer API returned status {} for variant {}",
                status, i
            );
            continue;
        }

        let explorer_data: ExplorerBoxesResponse = match response.json().await {
            Ok(d) => d,
            Err(e) => {
                warn!("Failed to parse Explorer response for variant {}: {}", i, e);
                continue;
            }
        };

        info!(
            "Found {} boxes for header variant {} (total reported: {})",
            explorer_data.items.len(),
            i,
            explorer_data.total
        );

        for item in explorer_data.items {
            let mut registers = HashMap::new();
            for (key, reg) in &item.additional_registers {
                registers.insert(key.clone(), reg.serialized_value.clone());
            }

            let babel_box = BabelBox {
                box_id: item.box_id.clone(),
                value: item.value,
                ergo_tree: item.ergo_tree,
                tokens: item
                    .assets
                    .iter()
                    .map(|a| TokenAmount {
                        token_id: a.token_id.clone(),
                        amount: a.amount,
                    })
                    .collect(),
                additional_registers: registers,
            };

            // Deduplicate: keep first seen (they have same data)
            all_boxes.entry(item.box_id).or_insert(babel_box);
        }
    }

    let mut boxes: Vec<BabelBox> = all_boxes.into_values().collect();
    // Sort by ERG value descending — most liquidity first
    boxes.sort_by(|a, b| b.value.cmp(&a.value));

    info!(
        "Total unique Babel boxes found for token {}: {}",
        token_id,
        boxes.len()
    );

    Ok(boxes)
}

// ---------------------------------------------------------------------------
// REST handlers
// ---------------------------------------------------------------------------

/// GET /api/v1/babel/discover/:token_id
///
/// Discover available Babel boxes for a given token ID.
/// Optional query param `limit` to cap results.
async fn handle_discover(
    State(state): State<AppState>,
    Path(token_id): Path<String>,
    Query(params): Query<DiscoverQuery>,
) -> impl IntoResponse {
    match find_babel_boxes(&state.http_client, &token_id).await {
        Ok(mut boxes) => {
            let total = boxes.len();
            if let Some(limit) = params.limit {
                boxes.truncate(limit);
            }
            let response = DiscoverResponse {
                token_id: token_id.clone(),
                boxes,
                total_found: total,
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e }),
        )
            .into_response(),
    }
}

/// GET /api/v1/babel/select/:token_id/:required_erg
///
/// Select the optimal Babel box for covering the required ERG amount.
/// required_erg is in nanoERG.
async fn handle_select(
    State(state): State<AppState>,
    Path((token_id, required_erg)): Path<(String, u64)>,
) -> impl IntoResponse {
    match find_babel_boxes(&state.http_client, &token_id).await {
        Ok(boxes) => {
            let selected = select_babel_box(&boxes, required_erg);
            let token_price = selected.and_then(|b| b.token_price());
            let tokens_needed =
                token_price.map(|p| calculate_swap(p, required_erg));

            let response = SelectResponse {
                token_id: token_id.clone(),
                required_erg,
                selected_box: selected.cloned(),
                token_price,
                tokens_needed,
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e }),
        )
            .into_response(),
    }
}

/// POST /api/v1/babel/calculate-swap
///
/// Calculate the number of tokens needed to cover a given ERG requirement.
async fn handle_calculate_swap(
    _State: State<AppState>,
    Json(req): Json<CalculateSwapRequest>,
) -> impl IntoResponse {
    let tokens_required = calculate_swap(req.token_price, req.erg_needed);
    let erg_value_formatted = format!("{:.9}", req.erg_needed as f64 / NANOERG_PER_ERG as f64);

    let response = CalculateSwapResponse {
        token_id: req.token_id,
        token_price: req.token_price,
        erg_needed: req.erg_needed,
        tokens_required,
        erg_value_formatted,
    };
    (StatusCode::OK, Json(response)).into_response()
}

/// GET /api/v1/babel/price/:token_id
///
/// Get the current token price from the best (highest liquidity) Babel box.
async fn handle_price(
    State(state): State<AppState>,
    Path(token_id): Path<String>,
) -> impl IntoResponse {
    match find_babel_boxes(&state.http_client, &token_id).await {
        Ok(boxes) => {
            let best = boxes.first();
            let token_price = best.and_then(|b| b.token_price());
            let erg_value_formatted =
                token_price.map(|p| format!("{:.9}", p as f64 / NANOERG_PER_ERG as f64));

            let response = PriceResponse {
                token_id,
                token_price,
                erg_value_formatted,
                box_id: best.map(|b| b.box_id.clone()),
                box_value: best.map(|b| b.value),
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e }),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the Babel box discovery router.
pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/babel/discover/:token_id",
            get(handle_discover),
        )
        .route(
            "/api/v1/babel/select/:token_id/:required_erg",
            get(handle_select),
        )
        .route(
            "/api/v1/babel/calculate-swap",
            post(handle_calculate_swap),
        )
        .route(
            "/api/v1/babel/price/:token_id",
            get(handle_price),
        )
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- ErgoTree construction tests --

    #[test]
    fn test_ergotree_compact_header() {
        let token_id = "00" .to_owned()
            + &"01".repeat(31); // 64 hex chars total
        let trees = get_babel_ergo_trees(&token_id);
        assert_eq!(trees.len(), 2);
        // Compact header
        assert!(trees[0].starts_with("10"));
        assert!(trees[0].contains(&token_id));
        assert!(trees[0].starts_with("100604000e20"));
    }

    #[test]
    fn test_ergotree_with_size_header() {
        let token_id = "ab" .to_owned()
            + &"cd".repeat(31); // 64 hex chars total
        let trees = get_babel_ergo_trees(&token_id);
        assert_eq!(trees.len(), 2);
        // With-size header
        assert!(trees[1].starts_with("18c101"));
        assert!(trees[1].contains(&token_id));
    }

    #[test]
    fn test_ergotree_both_contain_same_body() {
        let token_id = "deadbeef".to_owned() + &"00".repeat(28);
        let trees = get_babel_ergo_trees(&token_id);
        // Both trees should have the same body (everything after the header)
        let body_compact = &trees[0][2..]; // skip "10"
        let body_with_size = &trees[1][6..]; // skip "18c101"
        assert_eq!(body_compact, body_with_size);
    }

    // -- Zigzag / VLQ decoding tests --

    #[test]
    fn test_zigzag_roundtrip() {
        assert_eq!(zigzag_decode(zigzag_encode(0)), 0);
        assert_eq!(zigzag_decode(zigzag_encode(1)), 1);
        assert_eq!(zigzag_decode(zigzag_encode(-1)), -1);
        assert_eq!(zigzag_decode(zigzag_encode(2)), 2);
        assert_eq!(zigzag_decode(zigzag_encode(-2)), -2);
        assert_eq!(zigzag_decode(zigzag_encode(100)), 100);
        assert_eq!(zigzag_decode(zigzag_encode(-100)), -100);
    }

    #[test]
    fn test_vlq_decode_single_byte() {
        // Value 1 = VLQ "01"
        let (val, consumed) = decode_vlq("01");
        assert_eq!(val, 1);
        assert_eq!(consumed, 2);
    }

    #[test]
    fn test_vlq_decode_multi_byte() {
        // Value 128 = VLQ "8001" (continuation bit set on first byte)
        let (val, consumed) = decode_vlq("8001");
        assert_eq!(val, 128);
        assert_eq!(consumed, 4);
    }

    #[test]
    fn test_vlq_encode_decode_roundtrip() {
        for v in [0u64, 1, 127, 128, 255, 256, 16383, 16384, 1_000_000] {
            let encoded = encode_vlq(v);
            let hex: String = encoded.iter().map(|b| format!("{:02x}", b)).collect();
            let (decoded, _) = decode_vlq(&hex);
            assert_eq!(decoded, v, "VLQ roundtrip failed for {}", v);
        }
    }

    // -- Token price decoding tests --

    #[test]
    fn test_decode_slong_simple() {
        // SLong with value 1: type byte 05 + zigzag(1) = 02, VLQ(2) = "02"
        // So full: "0502"
        assert_eq!(decode_slong("0502"), 1);
    }

    #[test]
    fn test_decode_slong_negative() {
        // SLong with value -1: zigzag(-1) = 1, VLQ(1) = "01"
        // Full: "0501"
        assert_eq!(decode_slong("0501"), -1);
    }

    #[test]
    fn test_decode_slong_zero() {
        // SLong with value 0: zigzag(0) = 0, VLQ(0) = "00"
        // Full: "0500"
        assert_eq!(decode_slong("0500"), 0);
    }

    #[test]
    fn test_decode_slong_large_value() {
        // Value 1000: zigzag(1000) = 2000
        // VLQ(2000) = 2000 = 0b11111010000 -> split into 7-bit chunks:
        //   low 7: 1010000 = 0x50, rest: 11111 = 0x0F
        //   With continuation: 0xD0 0x0F
        // Full: "05d00f"
        assert_eq!(decode_slong("05d00f"), 1000);
    }

    #[test]
    fn test_decode_slong_with_0x_prefix() {
        assert_eq!(decode_slong("0x0502"), 1);
    }

    #[test]
    fn test_decode_slong_invalid_type_byte() {
        // Wrong type byte should return 0
        assert_eq!(decode_slong("0402"), 0);
    }

    // -- Swap calculation tests --

    #[test]
    fn test_calculate_swap_basic() {
        // Need 1_100_000 nanoERG, price = 1_000 nanoERG per token
        // 1_100_000 / 1_000 = 1100, +1 = 1101
        assert_eq!(calculate_swap(1_000, 1_100_000), 1101);
    }

    #[test]
    fn test_calculate_swap_exact_division() {
        // 1_000_000 / 1_000 = 1000, +1 = 1001
        assert_eq!(calculate_swap(1_000, 1_000_000), 1001);
    }

    #[test]
    fn test_calculate_swap_rounding_up() {
        // 1_000_001 / 1_000 = 1000 (truncated), +1 = 1001
        assert_eq!(calculate_swap(1_000, 1_000_001), 1001);
    }

    #[test]
    fn test_calculate_swap_zero_price() {
        assert_eq!(calculate_swap(0, 1_000_000), 0);
    }

    #[test]
    fn test_calculate_swap_negative_price() {
        assert_eq!(calculate_swap(-1, 1_000_000), 0);
    }

    #[test]
    fn test_calculate_swap_large_values() {
        // 1 ERG needed at 1 nanoERG/token = 1_000_000_000 tokens
        let result = calculate_swap(1, NANOERG_PER_ERG);
        assert_eq!(result, 1_000_000_001);
    }

    // -- Sigma serialization tests --

    #[test]
    fn test_sigma_serialize_sint_zero() {
        // SInt(0): type 04, zigzag(0)=0, VLQ(0)="00" -> "0400"
        assert_eq!(sigma_serialize_sint(0), "0400");
    }

    #[test]
    fn test_sigma_serialize_sint_positive() {
        // SInt(1): type 04, zigzag(1)=2, VLQ(2)="02" -> "0402"
        assert_eq!(sigma_serialize_sint(1), "0402");
    }

    #[test]
    fn test_sigma_serialize_sint_negative() {
        // SInt(-1): type 04, zigzag(-1)=1, VLQ(1)="01" -> "0401"
        assert_eq!(sigma_serialize_sint(-1), "0401");
    }

    #[test]
    fn test_sigma_serialize_sint_large() {
        // SInt(100): type 04, zigzag(100)=200, VLQ(200)="c801" -> "04c801"
        assert_eq!(sigma_serialize_sint(100), "04c801");
    }

    #[test]
    fn test_sigma_coll_byte_empty() {
        // Coll[Byte](): type 0e, VLQ(0)="00" -> "0e00"
        assert_eq!(sigma_coll_byte_from_hex(""), "0e00");
    }

    #[test]
    fn test_sigma_coll_byte_single() {
        // Coll[Byte](0xAB): type 0e, VLQ(1)="01", bytes "ab" -> "0e01ab"
        assert_eq!(sigma_coll_byte_from_hex("ab"), "0e01ab");
    }

    #[test]
    fn test_sigma_coll_byte_multiple() {
        // Coll[Byte](0xDEADBEEF): type 0e, VLQ(4)="04", bytes "deadbeef" -> "0e04deadbeef"
        assert_eq!(sigma_coll_byte_from_hex("deadbeef"), "0e04deadbeef");
    }

    #[test]
    fn test_sigma_coll_byte_strips_0x_prefix() {
        assert_eq!(sigma_coll_byte_from_hex("0xff"), "0e01ff");
    }

    // -- Box selection tests --

    fn make_babel_box(box_id: &str, value: u64) -> BabelBox {
        BabelBox {
            box_id: box_id.to_string(),
            value,
            ergo_tree: String::new(),
            tokens: Vec::new(),
            additional_registers: HashMap::new(),
        }
    }

    #[test]
    fn test_select_babel_box_exact_match() {
        let boxes = vec![
            make_babel_box("box1", 5_000_000),
            make_babel_box("box2", 3_000_000),
        ];
        // Need 2_000_000 nanoERG -> need box with >= 2_000_000 + 1_000_000 = 3_000_000
        let selected = select_babel_box(&boxes, 2_000_000);
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().box_id, "box1"); // Highest value first
    }

    #[test]
    fn test_select_babel_box_insufficient() {
        let boxes = vec![
            make_babel_box("box1", 1_500_000),
            make_babel_box("box2", 500_000),
        ];
        // Need 1_000_000 -> need >= 2_000_000. Neither box qualifies.
        let selected = select_babel_box(&boxes, 1_000_000);
        assert!(selected.is_none());
    }

    #[test]
    fn test_select_babel_box_exactly_min() {
        let boxes = vec![make_babel_box("box1", 2_000_000)];
        // Need 1_000_000 -> need >= 2_000_000. box1 has exactly 2_000_000.
        let selected = select_babel_box(&boxes, 1_000_000);
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().box_id, "box1");
    }

    #[test]
    fn test_select_babel_box_just_under() {
        let boxes = vec![make_babel_box("box1", 1_999_999)];
        // Need 1_000_000 -> need >= 2_000_000. box1 has 1_999_999. Fails.
        let selected = select_babel_box(&boxes, 1_000_000);
        assert!(selected.is_none());
    }

    #[test]
    fn test_select_babel_box_empty_list() {
        let boxes: Vec<BabelBox> = Vec::new();
        let selected = select_babel_box(&boxes, 1_000_000);
        assert!(selected.is_none());
    }

    #[test]
    fn test_select_babel_box_prefers_most_liquidity() {
        let boxes = vec![
            make_babel_box("small", 2_500_000),
            make_babel_box("large", 10_000_000),
            make_babel_box("medium", 5_000_000),
        ];
        // All qualify, but first (highest value) should be picked
        let selected = select_babel_box(&boxes, 1_000_000);
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().box_id, "large");
    }

    // -- Context extension test --

    #[test]
    fn test_context_extension_format() {
        // Context extension for Babel: { "0": "0402" } = SInt(1) output index
        let ctx_json = r#"{"0":"0402"}"#;
        let parsed: HashMap<String, String> = serde_json::from_str(ctx_json).unwrap();
        assert_eq!(parsed.get("0").unwrap(), "0402");
        // Verify "0402" decodes to SInt(1)
        assert_eq!(decode_slong("0402"), 0); // 0x04 is not SLong type
        // The context value "0402" is actually SInt(1): type 0x04, value VLQ 0x02 = zigzag(1) = 1
        let cleaned = "0402";
        let type_byte = u8::from_str_radix(&cleaned[0..2], 16).unwrap();
        assert_eq!(type_byte, 0x04); // SInt type
        let (zigzagged, _) = decode_vlq(&cleaned[2..]);
        assert_eq!(zigzag_decode(zigzagged), 1);
    }

    // -- Integration-style test: full flow --

    #[test]
    fn test_full_discovery_flow_mock() {
        let token_id = "a".repeat(64);

        // Build ergo trees
        let trees = get_babel_ergo_trees(&token_id);
        assert_eq!(trees.len(), 2);

        // Create mock boxes (as if discovered)
        let mut boxes = vec![
            {
                let mut regs = HashMap::new();
                // R5 with price 1000 nanoERG/token: SLong type 05, zigzag(1000)=2000, VLQ(2000)=d00f
                regs.insert("R5".to_string(), "05d00f".to_string());
                BabelBox {
                    box_id: "box_liquidity_high".to_string(),
                    value: 10_000_000,
                    ergo_tree: trees[0].clone(),
                    tokens: vec![TokenAmount {
                        token_id: token_id.clone(),
                        amount: 5_000,
                    }],
                    additional_registers: regs,
                }
            },
            {
                let mut regs = HashMap::new();
                regs.insert("R5".to_string(), "05d00f".to_string());
                BabelBox {
                    box_id: "box_liquidity_low".to_string(),
                    value: 2_500_000,
                    ergo_tree: trees[1].clone(),
                    tokens: vec![TokenAmount {
                        token_id: token_id.clone(),
                        amount: 1_000,
                    }],
                    additional_registers: regs,
                }
            },
        ];

        // Sort by value descending
        boxes.sort_by(|a, b| b.value.cmp(&a.value));

        // Get price from best box
        let price = boxes[0].token_price().unwrap();
        assert_eq!(price, 1000);

        // Calculate swap for TX_FEE
        let tokens_needed = calculate_swap(price, TX_FEE);
        assert_eq!(tokens_needed, 1101); // 1_100_000 / 1000 + 1

        // Select box
        let selected = select_babel_box(&boxes, TX_FEE);
        assert!(selected.is_some());
        assert_eq!(selected.unwrap().box_id, "box_liquidity_high");
    }
}
