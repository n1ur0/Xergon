//! On-chain transaction building and submission.
//!
//! Phase 2: Heartbeat transactions and usage proof transactions.
//!
//! All transactions are built using the Ergo node wallet API
//! (`POST /wallet/payment/send`), which handles signing automatically.
//! This avoids a dependency on ergo-lib (which is optional and heavy).

use anyhow::{Context, Result};
use tracing::{debug, info};

use crate::chain::client::ErgoNodeClient;
use crate::protocol::tx_safety::{validate_batch_size, validate_payment_request, validate_token_id};

/// A pending usage proof to be batched and submitted on-chain.
#[derive(Debug, Clone)]
pub struct PendingUsageProof {
    /// User public key (hex, compressed secp256k1)
    pub user_pk: String,
    /// Provider ID
    pub provider_id: String,
    /// Model name
    pub model: String,
    /// Token count (completion tokens)
    pub token_count: i64,
    /// Unix timestamp (ms) when the inference happened
    pub timestamp_ms: i64,
    /// Rarity multiplier for this model (1.0 = no bonus, >1.0 = rare model bonus)
    pub rarity_multiplier: f64,
}

/// Submit a heartbeat transaction that updates the provider box registers.
///
/// Spends the existing provider box (found via NFT token ID) and creates
/// a new provider box with registers matching the provider_box.ergo contract:
/// - R4: Provider public key (GroupElement) — spending authorization
/// - R5: Endpoint URL (Coll[Byte]) — UTF-8 encoded string
/// - R6: Models served + pricing (Coll[Byte]) — JSON string
/// - R7: PoNW score (Int) — 0-1000
/// - R8: Last heartbeat height (Int) — block height of last heartbeat
/// - R9: Region (Coll[Byte]) — UTF-8 encoded string
///
/// The provider PK (R4) is preserved from the existing box to satisfy the
/// contract check `outBox.R4[GroupElement].get == providerPk`.
///
/// Uses the Ergo node wallet API to build + sign + broadcast.
///
/// Returns the transaction ID on success.
pub async fn submit_heartbeat_tx(
    client: &ErgoNodeClient,
    provider_nft_token_id: &str,
    endpoint_url: &str,
    models_r6_json: &str,
    ponw_score: i32,
    region: &str,
) -> Result<String> {
    if provider_nft_token_id.is_empty() {
        anyhow::bail!("Provider NFT token ID not configured — cannot submit heartbeat tx");
    }
    validate_token_id(provider_nft_token_id)
        .context("Invalid provider NFT token ID for heartbeat tx")?;

    // Find the provider box by NFT token ID
    let boxes = client
        .get_boxes_by_token_id(provider_nft_token_id)
        .await
        .context("Failed to scan for provider box by NFT token ID")?;

    let provider_box = boxes
        .iter()
        .find(|b| {
            b.assets
                .iter()
                .any(|a| a.token_id == provider_nft_token_id && a.amount == 1)
        })
        .context("Provider box not found on-chain — NFT token ID may be wrong or box not created")?;

    // Read existing R4 (provider PK GroupElement) from the box to preserve it
    let existing_r4 = crate::chain::scanner::get_register(&provider_box.additional_registers, "R4")
        .and_then(|v| crate::chain::scanner::parse_group_element(v))
        .context("Missing R4 (provider PK GroupElement) in existing provider box")?;
    let pk_bytes = hex::decode(&existing_r4)
        .context("Invalid provider PK hex from existing box R4")?;
    let r4_hex = encode_group_element(&pk_bytes)
        .context("Failed to encode provider PK GroupElement for R4")?;

    // Fetch current block height for R8
    let current_height = client
        .get_height()
        .await
        .context("Failed to fetch current block height for heartbeat R8")?;

    // Clamp PoNW score to 0-1000 (Int range for contract)
    let ponw_clamped = ponw_score.clamp(0, 1000);

    // Encode registers per contract spec:
    // R4: GroupElement (preserved from existing box)
    // R5: String — endpoint URL
    let r5_hex = encode_string(endpoint_url);
    // R6: String — models + pricing JSON
    let r6_hex = encode_string(models_r6_json);
    // R7: Int — PoNW score 0-1000
    let r7_hex = encode_int(ponw_clamped);
    // R8: Int — last heartbeat height (monotonic)
    let r8_hex = encode_int(current_height);
    // R9: String — region
    let r9_hex = encode_string(region);

    // Build the payment request:
    // - Input: the provider box
    // - Output: new provider box with same NFT, updated registers R4-R9
    let payment_request = serde_json::json!({
        "requests": [{
            "address": provider_box.ergo_tree.clone(),
            "value": provider_box.value.to_string(),
            "assets": [{
                "tokenId": provider_nft_token_id,
                "amount": 1
            }],
            "registers": {
                "R4": r4_hex,
                "R5": r5_hex,
                "R6": r6_hex,
                "R7": r7_hex,
                "R8": r8_hex,
                "R9": r9_hex
            }
        }],
        "fee": 1000000,  // 0.001 ERG fee
        "inputsRaw": [provider_box.box_id.clone()],
        "dataInputsRaw": []
    });

    debug!(
        box_id = %provider_box.box_id,
        current_height,
        ponw_score = ponw_clamped,
        endpoint = %endpoint_url,
        region = %region,
        "Submitting heartbeat transaction"
    );

    validate_payment_request(&payment_request)
        .context("Heartbeat transaction safety validation failed")?;

    let tx_id = client
        .wallet_payment_send(&payment_request)
        .await
        .context("Failed to submit heartbeat transaction via wallet")?;

    info!(
        tx_id = %tx_id,
        box_id = %provider_box.box_id,
        height = current_height,
        ponw_score = ponw_clamped,
        "Heartbeat transaction submitted"
    );

    Ok(tx_id)
}

/// Submit a single usage proof box on-chain.
///
/// Creates a small box (min ERG value) with usage proof registers:
/// - R4: user_pk (Coll[Byte] — compressed secp256k1 public key)
/// - R5: provider_id (String)
/// - R6: model_name (String)
/// - R7: token_count (Int)
/// - R8: timestamp (Long)
///
/// Returns the transaction ID on success.
pub async fn submit_usage_proof_tx(
    client: &ErgoNodeClient,
    proof: &PendingUsageProof,
    proof_tree_hex: &str,
    min_value_nanoerg: u64,
) -> Result<String> {
    if proof_tree_hex.is_empty() {
        anyhow::bail!("Usage proof ErgoTree not configured — cannot submit usage proof tx");
    }

    // Encode register values as Sigma constants (hex)
    let user_pk_hex = encode_coll_byte(&hex::decode(&proof.user_pk).unwrap_or_default());
    let provider_id_hex = encode_string(&proof.provider_id);
    let model_hex = encode_string(&proof.model);
    let token_count_hex = encode_int(proof.token_count as i32);
    let timestamp_hex = encode_long(proof.timestamp_ms);

    let payment_request = serde_json::json!({
        "requests": [{
            "address": proof_tree_hex,
            "value": min_value_nanoerg.to_string(),
            "assets": [],
            "registers": {
                "R4": user_pk_hex,
                "R5": provider_id_hex,
                "R6": model_hex,
                "R7": token_count_hex,
                "R8": timestamp_hex
            }
        }],
        "fee": 1100000  // 0.0011 ERG fee
    });

    debug!(
        user_pk_prefix = &proof.user_pk[..proof.user_pk.len().min(8)],
        provider_id = %proof.provider_id,
        model = %proof.model,
        token_count = proof.token_count,
        "Submitting usage proof transaction"
    );

    validate_payment_request(&payment_request)
        .context("Usage proof transaction safety validation failed")?;

    let tx_id = client
        .wallet_payment_send(&payment_request)
        .await
        .context("Failed to submit usage proof transaction via wallet")?;

    info!(
        tx_id = %tx_id,
        provider_id = %proof.provider_id,
        model = %proof.model,
        token_count = proof.token_count,
        "Usage proof transaction submitted"
    );

    Ok(tx_id)
}

/// Submit multiple usage proofs in a single transaction (batched).
///
/// Creates one output box per proof. This is more gas-efficient than
/// individual transactions.
pub async fn submit_usage_proof_batch(
    client: &ErgoNodeClient,
    proofs: &[PendingUsageProof],
    proof_tree_hex: &str,
    min_value_nanoerg: u64,
) -> Result<String> {
    if proofs.is_empty() {
        return Ok(String::new());
    }

    validate_batch_size(proofs.len())
        .context("Usage proof batch size validation failed")?;

    if proof_tree_hex.is_empty() {
        anyhow::bail!("Usage proof ErgoTree not configured — cannot submit usage proof batch");
    }

    let mut requests = Vec::new();

    for proof in proofs {
        let user_pk_hex = encode_coll_byte(&hex::decode(&proof.user_pk).unwrap_or_default());
        let provider_id_hex = encode_string(&proof.provider_id);
        let model_hex = encode_string(&proof.model);
        let token_count_hex = encode_int(proof.token_count as i32);
        let timestamp_hex = encode_long(proof.timestamp_ms);

        requests.push(serde_json::json!({
            "address": proof_tree_hex,
            "value": min_value_nanoerg.to_string(),
            "assets": [],
            "registers": {
                "R4": user_pk_hex,
                "R5": provider_id_hex,
                "R6": model_hex,
                "R7": token_count_hex,
                "R8": timestamp_hex
            }
        }));
    }

    // Fee scales with number of outputs
    let fee = 1_000_000 + (proofs.len() as u64 * 100_000);

    let payment_request = serde_json::json!({
        "requests": requests,
        "fee": fee.to_string()
    });

    debug!(count = proofs.len(), "Submitting batched usage proof transaction");

    validate_payment_request(&payment_request)
        .context("Batched usage proof transaction safety validation failed")?;

    let tx_id = client
        .wallet_payment_send(&payment_request)
        .await
        .context("Failed to submit batched usage proof transaction via wallet")?;

    info!(
        tx_id = %tx_id,
        count = proofs.len(),
        "Batched usage proof transaction submitted"
    );

    Ok(tx_id)
}

// ---------------------------------------------------------------------------
// Sigma constant encoding helpers
// ---------------------------------------------------------------------------

/// Encode a Sigma `Long` (8 bytes big-endian).
/// Format: `05 <8 bytes big-endian>` as hex string.
pub(crate) fn encode_long(value: i64) -> String {
    let mut bytes = vec![0x05];
    bytes.extend_from_slice(&value.to_be_bytes());
    hex::encode(&bytes)
}

/// Encode a Sigma `Int` (4 bytes big-endian).
/// Format: `04 <4 bytes big-endian>` as hex string.
pub(crate) fn encode_int(value: i32) -> String {
    let mut bytes = vec![0x04];
    bytes.extend_from_slice(&value.to_be_bytes());
    hex::encode(&bytes)
}

/// Encode a Sigma `String` as `Coll[Byte]`.
/// Format: `0e <vlb_length> <utf8_bytes>` as hex string.
pub(crate) fn encode_string(s: &str) -> String {
    let data = s.as_bytes();
    let mut bytes = vec![0x0e];
    encode_vlb(&mut bytes, data.len());
    bytes.extend_from_slice(data);
    hex::encode(&bytes)
}

/// Encode raw bytes as a Sigma `Coll[Byte]`.
/// Format: `0e <vlb_length> <data_bytes>` as hex string.
pub(crate) fn encode_coll_byte(data: &[u8]) -> String {
    let mut bytes = vec![0x0e];
    encode_vlb(&mut bytes, data.len());
    bytes.extend_from_slice(data);
    hex::encode(&bytes)
}

/// Encode a 33-byte compressed secp256k1 public key as a Sigma `GroupElement`.
///
/// Format: `0e 21 <33 bytes>` as hex string.
/// The GroupElement is stored as `Coll[Byte]` with length prefix `0x21` (33).
///
/// # Errors
///
/// Returns an error if `pk_bytes` is not exactly 33 bytes.
pub fn encode_group_element(pk_bytes: &[u8]) -> Result<String> {
    if pk_bytes.len() != 33 {
        anyhow::bail!(
            "GroupElement (compressed secp256k1 PK) must be 33 bytes, got {} bytes",
            pk_bytes.len()
        );
    }
    // SGroupElement is serialized as Coll[Byte]: 0e <vlb_len=0x21> <33 bytes>
    let mut bytes = vec![0x0e, 0x21];
    bytes.extend_from_slice(pk_bytes);
    Ok(hex::encode(&bytes))
}

/// Encode a VLB (variable-length byte) encoded length.
/// Single byte if < 128, two bytes otherwise.
pub(crate) fn encode_vlb(out: &mut Vec<u8>, len: usize) {
    if len < 128 {
        out.push(len as u8);
    } else {
        out.push(((len >> 7) as u8) | 0x80);
        out.push((len & 0x7F) as u8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_long() {
        let hex = encode_long(1234567890);
        assert!(hex.starts_with("05"));
        // 1234567890 = 0x499602D2, encoded as 8 bytes big-endian
        assert_eq!(hex, "0500000000499602d2");
    }

    #[test]
    fn test_encode_long_negative() {
        let hex = encode_long(-1);
        assert_eq!(hex, "05ffffffffffffffff");
    }

    #[test]
    fn test_encode_int() {
        let hex = encode_int(1000);
        assert!(hex.starts_with("04"));
        assert_eq!(hex, "04000003e8");
    }

    #[test]
    fn test_encode_string() {
        let hex = encode_string("hello");
        // 0e 05 68656c6c6f
        assert!(hex.starts_with("0e"));
        assert_eq!(hex, "0e0568656c6c6f");
    }

    #[test]
    fn test_encode_string_empty() {
        let hex = encode_string("");
        assert_eq!(hex, "0e00");
    }

    #[test]
    fn test_encode_string_long() {
        // String with 200 chars — VLB should be 2 bytes
        let s = "a".repeat(200);
        let hex = encode_string(&s);
        assert!(hex.starts_with("0e"));
        // 200 = 0x80 | (200 >> 7), (200 & 0x7F) = 0x81, 0x48
        assert_eq!(&hex[..6], "0e8148");
    }

    #[test]
    fn test_encode_coll_byte() {
        let hex = encode_coll_byte(&[0x02, 0xAB, 0xCD]);
        // 0e 03 02abcd
        assert_eq!(hex, "0e0302abcd");
    }

    #[test]
    fn test_encode_coll_byte_empty() {
        let hex = encode_coll_byte(&[]);
        assert_eq!(hex, "0e00");
    }

    #[test]
    fn test_encode_group_element_valid() {
        // 33 bytes: 02 prefix + 32 bytes
        let pk = hex::decode("02aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
        assert_eq!(pk.len(), 33);
        let result = encode_group_element(&pk).unwrap();
        // Should start with 0e21 (Coll[Byte] tag + length 33)
        assert!(result.starts_with("0e21"));
        // Total length: 2 (prefix) + 2 (length byte) + 66 (hex for 33 bytes) = 70 hex chars
        assert_eq!(result.len(), 70);
    }

    #[test]
    fn test_encode_group_element_wrong_length() {
        // 32 bytes instead of 33
        let pk = vec![0x02u8; 32];
        let result = encode_group_element(&pk);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("33 bytes"));
    }

    #[test]
    fn test_encode_group_element_too_short() {
        let pk = vec![0x02u8; 1];
        let result = encode_group_element(&pk);
        assert!(result.is_err());
    }
}
