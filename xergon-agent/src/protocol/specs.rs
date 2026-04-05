//! Box specification validation rules.
//!
//! Each box type (Provider, Staking, UsageProof) has a specification that
//! defines required tokens, registers, and minimum ERG values. The validate
//! functions take a raw box from the chain and return a typed, validated struct.

use anyhow::{bail, Context, Result};

use crate::chain::scanner;
use crate::chain::types::{ProviderBox, RawBox, UsageProofBox, UserStakingBox};

/// Minimum ERG value for a Provider Box (must cover box size * 360 nanoerg/byte).
pub const MIN_PROVIDER_BOX_VALUE: u64 = 1_000_000; // 0.001 ERG

/// Minimum ERG value for a Usage Proof Box.
pub const MIN_USAGE_PROOF_VALUE: u64 = 1_000_000; // 0.001 ERG

/// Minimum ERG deposit for a User Staking Box.
pub const MIN_STAKING_DEPOSIT: u64 = 1_000_000; // 0.001 ERG

/// Maximum heartbeat age (in blocks) before a provider is considered inactive.
pub const MAX_HEARTBEAT_AGE_BLOCKS: i32 = 100;

/// Storage rent period in blocks (4 years).
pub const STORAGE_RENT_PERIOD_BLOCKS: i32 = 1_051_200;

/// Validate that a RawBox matches the Provider Box specification.
///
/// Checks:
/// - At least one token present (the Provider NFT)
/// - Minimum ERG value
/// - Required registers R4-R9 (provider PK, endpoint, models, pown, heartbeat, region)
///
/// Returns a validated `ProviderBox` on success.
pub fn validate_provider_box(raw: &RawBox, current_height: i32) -> Result<ProviderBox> {
    // Check minimum ERG value
    if raw.value < MIN_PROVIDER_BOX_VALUE {
        bail!(
            "Provider box {} has insufficient value: {} < {}",
            raw.box_id,
            raw.value,
            MIN_PROVIDER_BOX_VALUE
        );
    }

    // Check that at least one token exists (Provider NFT)
    if raw.assets.is_empty() {
        bail!(
            "Provider box {} has no tokens (expected Provider NFT)",
            raw.box_id
        );
    }

    // The first token is the Provider NFT
    let provider_nft_id = &raw.assets[0].token_id;
    if raw.assets[0].amount != 1 {
        bail!(
            "Provider box {} NFT has amount {} (expected exactly 1)",
            raw.box_id,
            raw.assets[0].amount
        );
    }

    // Parse registers R4-R9
    let regs = &raw.additional_registers;

    let provider_pk = scanner::get_register(regs, "R4")
        .and_then(|v| scanner::parse_group_element(v))
        .context(format!(
            "Missing or invalid R4 (provider PK) in box {}",
            raw.box_id
        ))?;

    let endpoint = scanner::get_register(regs, "R5")
        .and_then(|v| scanner::parse_coll_byte(v))
        .context(format!(
            "Missing or invalid R5 (endpoint) in box {}",
            raw.box_id
        ))?;

    let models_json = scanner::get_register(regs, "R6")
        .and_then(|v| scanner::parse_coll_byte(v))
        .unwrap_or_else(|| "[]".to_string());

    let (models, model_pricing) = parse_r6_models(&models_json);

    let pown_score = scanner::get_register(regs, "R7")
        .and_then(|v| scanner::parse_int(v))
        .unwrap_or(0);

    let last_heartbeat = scanner::get_register(regs, "R8")
        .and_then(|v| scanner::parse_int(v))
        .unwrap_or(0);

    let region = scanner::get_register(regs, "R9")
        .and_then(|v| scanner::parse_coll_byte(v))
        .unwrap_or_else(|| "unknown".to_string());

    let is_active = is_provider_active_by_height(last_heartbeat, current_height);

    Ok(ProviderBox {
        box_id: raw.box_id.clone(),
        tx_id: raw.tx_id.clone(),
        provider_nft_id: provider_nft_id.clone(),
        provider_pk,
        endpoint,
        models,
        model_pricing,
        pown_score,
        last_heartbeat,
        region,
        value: raw.value.to_string(),
        creation_height: raw.creation_height,
        is_active,
    })
}

/// Validate that a RawBox matches the User Staking Box specification.
///
/// Checks:
/// - Minimum ERG value
/// - R4 register (user public key)
pub fn validate_staking_box(raw: &RawBox) -> Result<UserStakingBox> {
    if raw.value < MIN_STAKING_DEPOSIT {
        bail!(
            "Staking box {} has insufficient value: {} < {}",
            raw.box_id,
            raw.value,
            MIN_STAKING_DEPOSIT
        );
    }

    let regs = &raw.additional_registers;

    let user_pk = scanner::get_register(regs, "R4")
        .and_then(|v| scanner::parse_group_element(v))
        .context(format!(
            "Missing or invalid R4 (user PK) in staking box {}",
            raw.box_id
        ))?;

    Ok(UserStakingBox {
        box_id: raw.box_id.clone(),
        tx_id: raw.tx_id.clone(),
        user_pk,
        balance_nanoerg: raw.value,
        creation_height: raw.creation_height,
    })
}

/// Validate that a RawBox matches the Usage Proof Box specification.
///
/// Checks:
/// - Minimum ERG value
/// - Required registers R4-R8 (user PK hash, provider NFT ID, model, token count, timestamp)
pub fn validate_usage_proof(raw: &RawBox) -> Result<UsageProofBox> {
    if raw.value < MIN_USAGE_PROOF_VALUE {
        bail!(
            "Usage proof box {} has insufficient value: {} < {}",
            raw.box_id,
            raw.value,
            MIN_USAGE_PROOF_VALUE
        );
    }

    let regs = &raw.additional_registers;

    let user_pk_hash = scanner::get_register(regs, "R4")
        .and_then(|v| scanner::parse_coll_byte(v))
        .context(format!(
            "Missing or invalid R4 (user PK hash) in box {}",
            raw.box_id
        ))?;

    let provider_nft_id = scanner::get_register(regs, "R5")
        .and_then(|v| scanner::parse_coll_byte(v))
        .context(format!(
            "Missing or invalid R5 (provider NFT ID) in box {}",
            raw.box_id
        ))?;

    let model = scanner::get_register(regs, "R6")
        .and_then(|v| scanner::parse_coll_byte(v))
        .context(format!(
            "Missing or invalid R6 (model) in box {}",
            raw.box_id
        ))?;

    let token_count = scanner::get_register(regs, "R7")
        .and_then(|v| scanner::parse_int(v))
        .context(format!(
            "Missing or invalid R7 (token count) in box {}",
            raw.box_id
        ))?;

    let timestamp = scanner::get_register(regs, "R8")
        .and_then(|v| scanner::parse_long(v))
        .context(format!(
            "Missing or invalid R8 (timestamp) in box {}",
            raw.box_id
        ))?;

    Ok(UsageProofBox {
        box_id: raw.box_id.clone(),
        tx_id: raw.tx_id.clone(),
        user_pk_hash,
        provider_nft_id,
        model,
        token_count,
        timestamp,
        creation_height: raw.creation_height,
    })
}

/// Check if a provider is active (heartbeat within `MAX_HEARTBEAT_AGE_BLOCKS`).
pub fn is_provider_active(provider: &ProviderBox, current_height: i32) -> bool {
    is_provider_active_by_height(provider.last_heartbeat, current_height)
}

/// Internal: check activity by raw heartbeat height.
fn is_provider_active_by_height(last_heartbeat: i32, current_height: i32) -> bool {
    current_height - last_heartbeat <= MAX_HEARTBEAT_AGE_BLOCKS
}

/// Parse R6 models JSON with backward compatibility.
///
/// Tries structured format first:
///   {"models":[{"id":"llama-3.1-8b","price_per_1m_tokens":50000}]}
///
/// Falls back to plain array (old format):
///   ["llama-3.1-8b","qwen3.5-4b"]
///
/// Returns (model_ids, model_id -> price_per_1m_tokens).
/// Old-format models get price 0 (free tier).
fn parse_r6_models(raw: &str) -> (Vec<String>, std::collections::HashMap<String, u64>) {
    use serde::Deserialize;
    use std::collections::HashMap;

    #[derive(Deserialize)]
    struct ModelsPayload {
        #[serde(default)]
        models: Vec<ModelEntry>,
    }
    #[derive(Deserialize)]
    struct ModelEntry {
        id: String,
        #[serde(default)]
        price_per_1m_tokens: u64,
    }

    // Try structured format first
    if let Ok(payload) = serde_json::from_str::<ModelsPayload>(raw) {
        if !payload.models.is_empty() {
            let models: Vec<String> = payload.models.iter().map(|m| m.id.clone()).collect();
            let model_pricing: HashMap<String, u64> = payload
                .models
                .into_iter()
                .map(|m| (m.id, m.price_per_1m_tokens))
                .collect();
            return (models, model_pricing);
        }
    }

    // Fallback: try plain JSON array (old format)
    if let Ok(ids) = serde_json::from_str::<Vec<String>>(raw) {
        let model_pricing: HashMap<String, u64> = ids.iter().map(|id| (id.clone(), 0)).collect();
        return (ids, model_pricing);
    }

    // Last resort: treat entire string as single model name
    let model = raw.to_string();
    let mut model_pricing = HashMap::new();
    model_pricing.insert(model.clone(), 0);
    (vec![model], model_pricing)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::types::RawAsset;
    use std::collections::HashMap;

    /// Helper to build a minimal RawBox for testing.
    fn make_raw_box(value: u64, assets: Vec<RawAsset>, registers: HashMap<String, serde_json::Value>) -> RawBox {
        RawBox {
            box_id: "test_box_id".to_string(),
            tx_id: "test_tx_id".to_string(),
            value,
            creation_height: 500,
            assets,
            additional_registers: registers,
            ergo_tree: "fake_ergo_tree".to_string(),
        }
    }

    /// Helper to encode a string as a Coll[Byte] hex string.
    fn encode_coll_byte(s: &str) -> String {
        let data = s.as_bytes();
        let mut bytes = vec![0x0e, data.len() as u8];
        bytes.extend_from_slice(data);
        hex::encode(&bytes)
    }

    /// Helper to encode an i32 as a Sigma Int hex string.
    fn encode_int(val: i32) -> String {
        let mut bytes = vec![0x04];
        bytes.extend_from_slice(&val.to_be_bytes());
        hex::encode(&bytes)
    }

    /// Helper to encode an i64 as a Sigma Long hex string.
    fn encode_long(val: i64) -> String {
        let mut bytes = vec![0x05];
        bytes.extend_from_slice(&val.to_be_bytes());
        hex::encode(&bytes)
    }

    /// Build registers for a valid provider box.
    fn provider_registers() -> HashMap<String, serde_json::Value> {
        let mut regs = HashMap::new();
        // R4: Provider PK (GroupElement = Coll[Byte] of 33 bytes)
        let pk_hex = "0e21".to_string() + &"02".repeat(1) + &"00".repeat(32);
        regs.insert("R4".to_string(), serde_json::Value::String(pk_hex));
        // R5: Endpoint
        regs.insert("R5".to_string(), serde_json::Value::String(encode_coll_byte("http://localhost:9099")));
        // R6: Models JSON
        regs.insert("R6".to_string(), serde_json::Value::String(encode_coll_byte(r#"["llama-3","mistral-7b"]"#)));
        // R7: PoNW score
        regs.insert("R7".to_string(), serde_json::Value::String(encode_int(750)));
        // R8: Heartbeat
        regs.insert("R8".to_string(), serde_json::Value::String(encode_int(490)));
        // R9: Region
        regs.insert("R9".to_string(), serde_json::Value::String(encode_coll_byte("us-east")));
        regs
    }

    fn provider_asset() -> RawAsset {
        RawAsset {
            token_id: "nft_token_id_123".to_string(),
            amount: 1,
            name: Some("XergonProviderNFT".to_string()),
            decimals: Some(0),
        }
    }

    #[test]
    fn test_validate_provider_box_success() {
        let raw = make_raw_box(
            2_000_000,
            vec![provider_asset()],
            provider_registers(),
        );
        let result = validate_provider_box(&raw, 500).unwrap();
        assert_eq!(result.endpoint, "http://localhost:9099");
        assert_eq!(result.pown_score, 750);
        assert_eq!(result.last_heartbeat, 490);
        assert_eq!(result.region, "us-east");
        assert!(result.is_active);
        assert_eq!(result.models.len(), 2);
    }

    #[test]
    fn test_validate_provider_box_insufficient_value() {
        let raw = make_raw_box(
            500_000, // below minimum
            vec![provider_asset()],
            provider_registers(),
        );
        let result = validate_provider_box(&raw, 500);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("insufficient value"));
    }

    #[test]
    fn test_validate_provider_box_no_tokens() {
        let raw = make_raw_box(2_000_000, vec![], provider_registers());
        let result = validate_provider_box(&raw, 500);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no tokens"));
    }

    #[test]
    fn test_validate_provider_box_inactive() {
        let raw = make_raw_box(
            2_000_000,
            vec![provider_asset()],
            provider_registers(), // heartbeat at 490
        );
        let result = validate_provider_box(&raw, 600).unwrap();
        assert!(!result.is_active);
    }

    #[test]
    fn test_validate_staking_box_success() {
        let mut regs = HashMap::new();
        let pk_hex = "0e21".to_string() + &"03".repeat(1) + &"aa".repeat(32);
        regs.insert("R4".to_string(), serde_json::Value::String(pk_hex));

        let raw = make_raw_box(5_000_000, vec![], regs);
        let result = validate_staking_box(&raw).unwrap();
        assert_eq!(result.balance_nanoerg, 5_000_000);
        assert_eq!(result.creation_height, 500);
    }

    #[test]
    fn test_validate_staking_box_insufficient_value() {
        let mut regs = HashMap::new();
        regs.insert("R4".to_string(), serde_json::Value::String("0e21".to_string() + &"00".repeat(33)));
        let raw = make_raw_box(100_000, vec![], regs);
        let result = validate_staking_box(&raw);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_usage_proof_success() {
        let mut regs = HashMap::new();
        regs.insert("R4".to_string(), serde_json::Value::String(encode_coll_byte("user_pk_hash_abc")));
        regs.insert("R5".to_string(), serde_json::Value::String(encode_coll_byte("nft_token_id_123")));
        regs.insert("R6".to_string(), serde_json::Value::String(encode_coll_byte("llama-3")));
        regs.insert("R7".to_string(), serde_json::Value::String(encode_int(2048)));
        regs.insert("R8".to_string(), serde_json::Value::String(encode_long(1710000000000)));

        let raw = make_raw_box(1_500_000, vec![], regs);
        let result = validate_usage_proof(&raw).unwrap();
        assert_eq!(result.user_pk_hash, "user_pk_hash_abc");
        assert_eq!(result.provider_nft_id, "nft_token_id_123");
        assert_eq!(result.model, "llama-3");
        assert_eq!(result.token_count, 2048);
        assert_eq!(result.timestamp, 1710000000000);
    }

    #[test]
    fn test_validate_usage_proof_missing_register() {
        let mut regs = HashMap::new();
        regs.insert("R4".to_string(), serde_json::Value::String(encode_coll_byte("user_pk_hash")));
        // Missing R5-R8
        let raw = make_raw_box(1_500_000, vec![], regs);
        let result = validate_usage_proof(&raw);
        assert!(result.is_err());
    }

    #[test]
    fn test_is_provider_active() {
        let mut provider = ProviderBox {
            box_id: "box1".to_string(),
            tx_id: "tx1".to_string(),
            provider_nft_id: "nft1".to_string(),
            provider_pk: "pk1".to_string(),
            endpoint: "http://localhost:9099".to_string(),
            models: vec![],
            model_pricing: std::collections::HashMap::new(),
            pown_score: 100,
            last_heartbeat: 400,
            region: "us".to_string(),
            value: "1000000".to_string(),
            creation_height: 300,
            is_active: false,
        };

        // Exactly at the boundary
        assert!(is_provider_active(&provider, 400));
        assert!(is_provider_active(&provider, 500));

        // Beyond the boundary
        assert!(!is_provider_active(&provider, 501));

        // Fresh heartbeat
        provider.last_heartbeat = 490;
        assert!(is_provider_active(&provider, 500));
    }

    #[test]
    fn test_constants() {
        assert_eq!(MIN_PROVIDER_BOX_VALUE, 1_000_000);
        assert_eq!(MIN_USAGE_PROOF_VALUE, 1_000_000);
        assert_eq!(MIN_STAKING_DEPOSIT, 1_000_000);
        assert_eq!(MAX_HEARTBEAT_AGE_BLOCKS, 100);
        assert_eq!(STORAGE_RENT_PERIOD_BLOCKS, 1_051_200);
    }
}
