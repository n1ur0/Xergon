//! Box scanner -- I/O layer that reads raw boxes from the Ergo node.
//!
//! Box validation and register parsing into typed Xergon structs
//! (`ProviderBox`, `UserStakingBox`, `UsageProofBox`) is delegated to the
//! `crate::protocol::specs` module.  This module only provides:
//! - `ChainScanner` -- async methods that query the node API
//! - `ScanManager` -- manages EIP-1 registered scans on the node wallet
//! - Low-level Sigma-type hex parsing helpers (used by specs)

use std::collections::HashMap;

use anyhow::{Context, Result};
use dashmap::DashMap;
use serde_json::Value;
use tracing::{debug, info, warn};

use crate::chain::client::ErgoNodeClient;
use crate::chain::types::*;
use crate::protocol::specs;

/// Scans the Ergo blockchain for Xergon-related boxes and parses them.
pub struct ChainScanner {
    client: ErgoNodeClient,
    scan_manager: ScanManager,
}

impl ChainScanner {
    pub fn new(client: ErgoNodeClient) -> Self {
        Self {
            client,
            scan_manager: ScanManager::new(),
        }
    }

    /// Scan for all Provider Boxes by looking for boxes containing known Provider NFTs.
    ///
    /// For each token ID, fetches unspent boxes containing that token, then validates
    /// them against the Provider Box specification (delegated to `specs` module).
    pub async fn scan_provider_boxes(
        &self,
        provider_nft_ids: &[String],
    ) -> Result<Vec<ProviderBox>> {
        let current_height = self
            .client
            .get_height()
            .await
            .context("Failed to fetch current block height for provider validation")?;

        let mut providers = Vec::new();

        for nft_id in provider_nft_ids {
            let boxes = self
                .client
                .get_boxes_by_token_id(nft_id)
                .await
                .with_context(|| format!("Failed to scan boxes for provider NFT {}", nft_id))?;

            for raw_box in &boxes {
                match specs::validate_provider_box(raw_box, current_height) {
                    Ok(pb) => {
                        debug!(box_id = %pb.box_id, endpoint = %pb.endpoint, "Validated provider box");
                        providers.push(pb);
                    }
                    Err(e) => {
                        warn!(box_id = %raw_box.box_id, error = %e, "Skipping invalid provider box");
                    }
                }
            }
        }

        Ok(providers)
    }

    /// Scan for a specific user's Staking Box by their public key.
    ///
    /// Uses the EIP-1 wallet scan API to register a CONTAINS predicate on R4
    /// (the user's public key encoded as a Sigma GroupElement). The scan ID is
    /// cached so we don't re-register every time.
    ///
    /// LIMITATION: CONTAINS on the raw PK bytes is a heuristic -- it may match
    /// boxes from other contracts that happen to contain the same bytes in any
    /// register. A more robust approach would use EQUALS on the full ergoTree,
    /// but that requires compiling the user_staking contract with the specific PK
    /// first (requires ergo-lib). For now, we rely on `validate_staking_box()` to
    /// filter false positives.
    pub async fn scan_user_staking(&self, user_pk_hex: &str) -> Result<Option<UserStakingBox>> {
        let scan_id = self
            .scan_manager
            .register_staking_scan(&self.client, user_pk_hex)
            .await
            .context("Failed to register staking scan")?;

        let boxes = self
            .client
            .get_scan_boxes(scan_id)
            .await
            .context("Failed to fetch staking scan boxes")?;

        debug!(
            scan_id,
            box_count = boxes.len(),
            pk_prefix = %&user_pk_hex[..user_pk_hex.len().min(8)],
            "Got boxes from staking scan"
        );

        // Validate each box against the staking spec and return the first match.
        // Multiple staking boxes per user are not expected, but we return the first
        // valid one. A future enhancement could aggregate all for total balance.
        for raw_box in &boxes {
            match specs::validate_staking_box(raw_box) {
                Ok(staking_box) => {
                    // Additional check: the parsed PK should match the queried PK
                    // to avoid false positives from CONTAINS matching other registers.
                    if staking_box.user_pk == user_pk_hex {
                        info!(
                            box_id = %staking_box.box_id,
                            balance = staking_box.balance_nanoerg,
                            "Found matching user staking box"
                        );
                        return Ok(Some(staking_box));
                    }
                    debug!(
                        box_id = %raw_box.box_id,
                        "Box matched scan but PK does not match query (CONTAINS false positive)"
                    );
                }
                Err(e) => {
                    debug!(box_id = %raw_box.box_id, error = %e, "Scan box failed staking validation");
                }
            }
        }

        Ok(None)
    }

    /// Get all validated user staking boxes (aggregated balance).
    ///
    /// Returns all boxes that pass `validate_staking_box()` and match the given PK.
    pub async fn scan_all_user_staking(&self, user_pk_hex: &str) -> Result<Vec<UserStakingBox>> {
        let scan_id = self
            .scan_manager
            .register_staking_scan(&self.client, user_pk_hex)
            .await
            .context("Failed to register staking scan")?;

        let boxes = self
            .client
            .get_scan_boxes(scan_id)
            .await
            .context("Failed to fetch staking scan boxes")?;

        let mut results = Vec::new();
        for raw_box in &boxes {
            match specs::validate_staking_box(raw_box) {
                Ok(staking_box) if staking_box.user_pk == user_pk_hex => {
                    debug!(box_id = %staking_box.box_id, "Found user staking box");
                    results.push(staking_box);
                }
                Ok(_) => {
                    debug!(box_id = %raw_box.box_id, "CONTAINS false positive, skipping");
                }
                Err(e) => {
                    debug!(box_id = %raw_box.box_id, error = %e, "Scan box failed staking validation");
                }
            }
        }

        Ok(results)
    }

    /// Scan for recent usage proofs for a specific provider.
    ///
    /// Looks for boxes containing the provider's NFT as a token and validates
    /// them against the Usage Proof Box specification (delegated to `specs` module).
    pub async fn scan_usage_proofs(
        &self,
        provider_nft_id: &str,
        limit: u32,
    ) -> Result<Vec<UsageProofBox>> {
        let boxes = self
            .client
            .get_boxes_by_token_id(provider_nft_id)
            .await
            .with_context(|| {
                format!(
                    "Failed to scan usage proofs for provider NFT {}",
                    provider_nft_id
                )
            })?;

        let mut proofs = Vec::new();
        for raw_box in boxes.iter().take(limit as usize) {
            match specs::validate_usage_proof(raw_box) {
                Ok(up) => {
                    debug!(box_id = %up.box_id, "Validated usage proof box");
                    proofs.push(up);
                }
                Err(e) => {
                    warn!(box_id = %raw_box.box_id, error = %e, "Skipping invalid usage proof");
                }
            }
        }

        Ok(proofs)
    }

    /// Generic helper: get all unspent boxes for a given token ID.
    pub async fn get_boxes_by_token(&self, token_id: &str) -> Result<Vec<RawBox>> {
        self.client
            .get_boxes_by_token_id(token_id)
            .await
            .with_context(|| format!("Failed to get boxes for token {}", token_id))
    }

    /// Accessor for the scan manager (e.g., for deregistration on shutdown).
    pub fn scan_manager(&self) -> &ScanManager {
        &self.scan_manager
    }
}

// ---------------------------------------------------------------------------
// EIP-1 Scan Manager
// ---------------------------------------------------------------------------

/// Manages EIP-1 registered scans on the Ergo node wallet.
///
/// Tracks scan IDs by name so we don't re-register scans on every call.
/// Uses a concurrent `DashMap` for thread-safe access.
pub struct ScanManager {
    /// Maps scan_name -> scan_id for cached lookups.
    scan_ids: DashMap<String, i32>,
}

impl ScanManager {
    /// Create a new empty scan manager.
    pub fn new() -> Self {
        Self {
            scan_ids: DashMap::new(),
        }
    }

    /// Register a staking scan for the given user PK hex.
    ///
    /// Uses a CONTAINS predicate on the user's PK bytes (encoded as a Sigma
    /// GroupElement: `0e21` prefix + 33-byte compressed pubkey).
    ///
    /// If a scan for this PK already exists (cached), returns the cached ID
    /// without re-registering.
    pub async fn register_staking_scan(
        &self,
        client: &ErgoNodeClient,
        user_pk_hex: &str,
    ) -> Result<i32> {
        let scan_name = format!("xergon_staking_{}", user_pk_hex);

        // Check cache first
        if let Some(cached_id) = self.scan_ids.get(&scan_name) {
            debug!(
                scan_id = *cached_id,
                "Using cached staking scan registration"
            );
            return Ok(*cached_id);
        }

        // Build the tracking rule: scan for boxes whose serialized registers
        // CONTAIN the user's PK encoded as a Sigma GroupElement.
        // GroupElement = Coll[Byte] = 0e <len=0x21> <33 bytes>
        let pk_bytes = hex::decode(user_pk_hex)
            .context("Invalid user PK hex for scan registration")?;
        if pk_bytes.len() != 33 {
            anyhow::bail!(
                "User PK must be 33 bytes (compressed secp256k1), got {} bytes",
                pk_bytes.len()
            );
        }
        let mut ge_bytes = vec![0x0e, 0x21]; // Coll[Byte] prefix, length 33
        ge_bytes.extend_from_slice(&pk_bytes);
        let ge_hex = hex::encode(&ge_bytes);

        let tracking_rule = serde_json::json!({
            "predicate": "contains",
            "value": ge_hex,
        });

        info!(
            scan_name = %scan_name,
            pk_prefix = %&user_pk_hex[..user_pk_hex.len().min(8)],
            "Registering new EIP-1 staking scan"
        );

        let scan_id = client
            .register_scan(&scan_name, tracking_rule)
            .await
            .context("Failed to register staking scan with node")?;

        self.scan_ids.insert(scan_name, scan_id);
        info!(scan_id, "Staking scan registered successfully");

        Ok(scan_id)
    }

    /// Get boxes tracked by a staking scan for the given user PK.
    ///
    /// Registers the scan if not already registered.
    pub async fn get_staking_boxes(
        &self,
        client: &ErgoNodeClient,
        user_pk_hex: &str,
    ) -> Result<Vec<RawBox>> {
        let scan_id = self.register_staking_scan(client, user_pk_hex).await?;
        client.get_scan_boxes(scan_id).await
    }

    /// Check if a scan is registered for the given user PK.
    pub fn has_staking_scan(&self, user_pk_hex: &str) -> bool {
        let scan_name = format!("xergon_staking_{}", user_pk_hex);
        self.scan_ids.contains_key(&scan_name)
    }

    /// Get the cached scan ID for a user PK, if any.
    pub fn get_staking_scan_id(&self, user_pk_hex: &str) -> Option<i32> {
        let scan_name = format!("xergon_staking_{}", user_pk_hex);
        self.scan_ids.get(&scan_name).map(|v| *v)
    }

    /// Remove a cached scan ID (does not deregister from node).
    pub fn forget_staking_scan(&self, user_pk_hex: &str) {
        let scan_name = format!("xergon_staking_{}", user_pk_hex);
        self.scan_ids.remove(&scan_name);
    }

    /// Deregister all cached scans from the node and clear the cache.
    ///
    /// Call this on graceful shutdown to clean up node state.
    pub async fn deregister_all(&self, client: &ErgoNodeClient) {
        let entries: Vec<(String, i32)> = self
            .scan_ids
            .iter()
            .map(|kv| (kv.key().clone(), *kv.value()))
            .collect();

        for (name, scan_id) in &entries {
            match client.deregister_scan(*scan_id).await {
                Ok(()) => {
                    info!(scan_id, %name, "Deregistered scan");
                }
                Err(e) => {
                    warn!(scan_id, %name, error = %e, "Failed to deregister scan");
                }
            }
        }

        self.scan_ids.clear();
    }

    /// Return the number of cached scan registrations.
    pub fn len(&self) -> usize {
        self.scan_ids.len()
    }

    /// Return true if no scans are cached.
    pub fn is_empty(&self) -> bool {
        self.scan_ids.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Register value extraction
// ---------------------------------------------------------------------------

/// Extract a register value from the additionalRegisters HashMap.
///
/// Returns a reference to the raw `serde_json::Value` for that register key.
pub fn get_register<'a>(regs: &'a HashMap<String, Value>, key: &str) -> Option<&'a Value> {
    regs.get(key)
}

/// Extract the serialized hex string from a register value.
///
/// Handles both formats:
/// 1. Compact: `"0e2102..."`  (raw hex string)
/// 2. Expanded: `{"serializedValue": "0e2102...", "sigmaType": "...", "renderedValue": "..."}`
pub fn extract_serialized_hex<'a>(val: &'a Value) -> Option<&'a str> {
    match val {
        Value::String(s) => Some(s.as_str()),
        Value::Object(map) => map.get("serializedValue")?.as_str(),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Box parsing is delegated to the specs module (crate::protocol::specs).
// The functions below are low-level Sigma-type hex parsing helpers used by
// the specs validators.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Sigma-type hex parsing helpers
// ---------------------------------------------------------------------------

/// Parse a `Coll[Byte]` from serialized Sigma hex.
///
/// Format: `0e <vlb_length> <data_bytes...>`
/// The data bytes are interpreted as UTF-8.
pub fn parse_coll_byte(raw: &Value) -> Option<String> {
    let hex = extract_serialized_hex(raw)?;

    let bytes = hex::decode(hex).ok()?;

    // Coll[Byte] starts with 0e, followed by a VLB-encoded length, then the data.
    if bytes.len() < 2 || bytes[0] != 0x0e {
        return None;
    }

    // Decode VLB (variable-length byte) length encoding used by Sigma.
    let (data_offset, data_len) = decode_vlb(&bytes[1..])?;
    if bytes.len() < 1 + data_offset + data_len {
        return None;
    }

    String::from_utf8(bytes[1 + data_offset..1 + data_offset + data_len].to_vec()).ok()
}

/// Parse a Sigma `Int` (4 bytes big-endian).
///
/// Format: `04 <4 bytes big-endian>`
pub fn parse_int(raw: &Value) -> Option<i32> {
    let hex = extract_serialized_hex(raw)?;

    let bytes = hex::decode(hex).ok()?;

    // Int tag is 0x04, followed by 4 bytes big-endian
    if bytes.len() != 5 || bytes[0] != 0x04 {
        return None;
    }

    Some(i32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]))
}

/// Parse a Sigma `Long` (8 bytes big-endian).
///
/// Format: `05 <8 bytes big-endian>`
pub fn parse_long(raw: &Value) -> Option<i64> {
    let hex = extract_serialized_hex(raw)?;

    let bytes = hex::decode(hex).ok()?;

    // Long tag is 0x05, followed by 8 bytes big-endian
    if bytes.len() != 9 || bytes[0] != 0x05 {
        return None;
    }

    let mut arr = [0u8; 8];
    arr.copy_from_slice(&bytes[1..9]);
    Some(i64::from_be_bytes(arr))
}

/// Parse a Sigma `GroupElement` (33-byte compressed secp256k1 point).
///
/// Format: `0e 21 <33 bytes>`  (Coll[Byte] of length 33)
/// Returns the 33-byte hex representation.
pub fn parse_group_element(raw: &Value) -> Option<String> {
    let hex = extract_serialized_hex(raw)?;

    let bytes = hex::decode(hex).ok()?;

    // GroupElement is stored as Coll[Byte] with tag 0e, length 0x21 (33), then 33 bytes.
    if bytes.len() < 35 || bytes[0] != 0x0e || bytes[1] != 0x21 {
        return None;
    }

    Some(hex::encode(&bytes[2..35]))
}

/// Decode a Sigma VLB (variable-length byte) encoded integer.
/// Returns (number_of_bytes_consumed, decoded_value).
fn decode_vlb(data: &[u8]) -> Option<(usize, usize)> {
    if data.is_empty() {
        return None;
    }

    let first = data[0];
    let continuation = (first & 0x80) != 0;
    let value = (first & 0x7F) as usize;

    if !continuation {
        // Single byte: value is the length
        Some((1, value))
    } else if data.len() >= 2 {
        // Two bytes: first byte lower 7 bits << 7 | second byte
        let second = data[1] as usize;
        let combined = (value << 7) | second;
        Some((2, combined))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_int() {
        // 04 followed by 4 bytes big-endian = 42
        let val = serde_json::Value::String("040000002a".to_string());
        assert_eq!(parse_int(&val), Some(42));
    }

    #[test]
    fn test_parse_long() {
        // 05 followed by 8 bytes big-endian = 1000
        // 1000 = 0x00000000000003e8
        let val = serde_json::Value::String("0500000000000003e8".to_string());
        assert_eq!(parse_long(&val), Some(1000));
    }

    #[test]
    fn test_parse_coll_byte() {
        // 0e 05 68 65 6c 6c 6f => "hello"
        let val = serde_json::Value::String("0e0568656c6c6f".to_string());
        assert_eq!(parse_coll_byte(&val), Some("hello".to_string()));
    }

    #[test]
    fn test_parse_coll_byte_longer() {
        // Build a proper Coll[Byte] from scratch
        let s = "http://192.168.1.5:9099";
        let data = s.as_bytes();
        // Sigma Coll[Byte]: 0e <vlb_len> <data>
        let mut bytes = vec![0x0e, data.len() as u8];
        bytes.extend_from_slice(data);
        let hex_str = hex::encode(&bytes);
        let val = serde_json::Value::String(hex_str);
        assert_eq!(parse_coll_byte(&val), Some(s.to_string()));
    }

    #[test]
    fn test_parse_group_element() {
        // 0e 21 <33 bytes of zeros>
        let mut hex = "0e21".to_string();
        hex.push_str(&"00".repeat(33));
        let val = serde_json::Value::String(hex.clone());
        let result = parse_group_element(&val).unwrap();
        assert_eq!(result, "00".repeat(33));
    }

    #[test]
    fn test_extract_register_compact() {
        let mut regs = HashMap::new();
        regs.insert("R4".to_string(), serde_json::Value::String("040000002a".to_string()));

        let extracted = get_register(&regs, "R4").unwrap();
        assert_eq!(extract_serialized_hex(extracted), Some("040000002a"));
    }

    #[test]
    fn test_extract_register_expanded() {
        let mut inner = serde_json::Map::new();
        inner.insert(
            "serializedValue".to_string(),
            serde_json::Value::String("040000002a".to_string()),
        );
        inner.insert(
            "sigmaType".to_string(),
            serde_json::Value::String("SInt".to_string()),
        );
        let mut regs = HashMap::new();
        regs.insert("R4".to_string(), serde_json::Value::Object(inner));

        let extracted = get_register(&regs, "R4").unwrap();
        assert_eq!(extract_serialized_hex(extracted), Some("040000002a"));
    }

    #[test]
    fn test_parse_int_expanded_format() {
        let mut inner = serde_json::Map::new();
        inner.insert(
            "serializedValue".to_string(),
            serde_json::Value::String("040000002a".to_string()),
        );
        let val = serde_json::Value::Object(inner);
        assert_eq!(parse_int(&val), Some(42));
    }

    #[test]
    fn test_decode_vlb_single() {
        assert_eq!(decode_vlb(&[0x05]), Some((1, 5)));
        assert_eq!(decode_vlb(&[0x7F]), Some((1, 127)));
    }

    #[test]
    fn test_decode_vlb_two_byte() {
        // 0x80 | 0x01 = 0x81, 0x00 => (1 << 7) | 0 = 128
        assert_eq!(decode_vlb(&[0x81, 0x00]), Some((2, 128)));
    }

    // -- ScanManager unit tests --

    #[test]
    fn test_scan_manager_new() {
        let mgr = ScanManager::new();
        assert!(mgr.is_empty());
        assert_eq!(mgr.len(), 0);
    }

    #[test]
    fn test_scan_manager_cache() {
        let mgr = ScanManager::new();
        let pk = "03".to_string() + &"aa".repeat(32); // 33-byte PK

        // Not cached initially
        assert!(!mgr.has_staking_scan(&pk));
        assert!(mgr.get_staking_scan_id(&pk).is_none());

        // Simulate a cached scan (without hitting the node)
        let scan_name = format!("xergon_staking_{}", pk);
        mgr.scan_ids.insert(scan_name, 42);

        assert!(mgr.has_staking_scan(&pk));
        assert_eq!(mgr.get_staking_scan_id(&pk), Some(42));
        assert_eq!(mgr.len(), 1);

        // Forget it
        mgr.forget_staking_scan(&pk);
        assert!(!mgr.has_staking_scan(&pk));
        assert!(mgr.is_empty());
    }

    #[test]
    fn test_scan_manager_scan_name_format() {
        let _mgr = ScanManager::new();
        let pk = format!("02deadbeef{}", "00".repeat(25)); // 33 bytes
        let scan_name = format!("xergon_staking_{}", pk);
        assert!(scan_name.starts_with("xergon_staking_02"));
        assert!(scan_name.contains("deadbeef"));
    }
}
