//! Provider on-chain registration service.
//!
//! Handles the full lifecycle of provider boxes on the Ergo blockchain:
//! - Registration: mint an NFT + create a provider box with R4-R7 registers
//! - Query: scan UTXO set for all provider boxes matching the registration contract
//! - Update: spend an existing provider box and recreate with updated R5/R6
//!
//! All transactions use the Ergo node wallet API (`POST /wallet/payment/send`).

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info, warn};

use crate::chain::client::ErgoNodeClient;
use crate::chain::transactions::{encode_group_element, encode_int, encode_long, encode_string};
use crate::config::ProviderRegistryConfig;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Minimum box value on Ergo (0.001 ERG = 1,000,000 nanoERG).
const SAFE_MIN_BOX_VALUE: u64 = 1_000_000;

/// Recommended fee for a transaction with register data.
const DEFAULT_FEE: u64 = 1_100_000;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Parameters for registering a provider on-chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterProviderParams {
    /// Human-readable provider name (e.g., "Xergon_US-East-1")
    pub provider_name: String,
    /// Provider API endpoint URL (e.g., "http://192.168.1.5:9099")
    pub endpoint_url: String,
    /// Price per token in nanoERG
    pub price_per_token: u64,
    /// Staking/reward Ergo address for the provider
    #[serde(default)]
    pub staking_address: String,
    /// Provider's compressed secp256k1 public key (hex, 33 bytes).
    /// If empty, registration will fail — PK must be provided.
    #[serde(default)]
    pub provider_pk_hex: String,
}

/// Result of a provider registration transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRegistrationResult {
    /// Transaction ID of the registration
    pub tx_id: String,
    /// Minted provider NFT token ID
    pub provider_nft_id: String,
    /// Provider box ID
    pub provider_box_id: String,
}

/// A provider box parsed from the UTXO set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnChainProvider {
    /// Box ID (hex)
    pub box_id: String,
    /// Transaction ID that created this box
    pub tx_id: String,
    /// Provider public key (hex, from R4)
    pub provider_pk: String,
    /// Endpoint URL (from R5)
    pub endpoint: String,
    /// Price per token in nanoERG (from R6)
    pub price_per_token: u64,
    /// Creation block height (from R7)
    pub creation_height: i32,
    /// Provider NFT token ID
    pub nft_token_id: String,
    /// ERG value in the box (nanoERG)
    pub value_nanoerg: u64,
    /// ErgoTree of the box
    pub ergo_tree: String,
}

/// Parameters for updating a provider box.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProviderParams {
    /// Box ID of the provider box to update
    pub box_id: String,
    /// New price per token (nanoERG). None = keep current.
    pub new_price: Option<u64>,
    /// New endpoint URL. None = keep current.
    pub new_endpoint: Option<String>,
}

/// Result of a provider update transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderUpdateResult {
    /// Transaction ID of the update
    pub tx_id: String,
    /// New provider box ID (may differ from input box_id)
    pub new_box_id: String,
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Register a provider on-chain by creating a provider box with the
/// `provider_registration` ErgoTree and registers R4-R7.
///
/// The transaction mints a singleton NFT (EIP-004) in the same box.
///
/// Register layout:
/// - R4: GroupElement — provider public key (Coll[Byte] with 33-byte prefix)
/// - R5: Coll[Byte] — endpoint URL (UTF-8)
/// - R6: Long — price per token (nanoERG)
/// - R7: Int — creation block height
///
/// Uses `POST /wallet/payment/send` on the Ergo node.
pub async fn register_provider_on_chain(
    client: &ErgoNodeClient,
    config: &ProviderRegistryConfig,
    params: &RegisterProviderParams,
) -> Result<ProviderRegistrationResult> {
    // Validate inputs
    if params.endpoint_url.is_empty() {
        bail!("endpoint_url is required for provider registration");
    }
    if params.provider_name.is_empty() {
        bail!("provider_name is required for provider registration");
    }

    // Get the compiled provider_registration ErgoTree
    let tree_hex = crate::contract_compile::get_contract_hex("provider_registration")
        .context("provider_registration contract not found -- ensure compiled hex is embedded")?;

    info!(
        provider_name = %params.provider_name,
        endpoint = %params.endpoint_url,
        price_per_token = params.price_per_token,
        stake_nanoerg = config.registration_stake_nanoerg,
        "Registering provider on-chain"
    );

    // Check wallet is ready
    let wallet_ready = client
        .wallet_status()
        .await
        .context("Failed to check wallet status")?;
    if !wallet_ready {
        bail!("Ergo node wallet is locked. Unlock it before provider registration.");
    }

    // Get current block height for R7
    let height = client
        .get_height()
        .await
        .context("Failed to get current block height")?;

    // Validate provider PK
    if params.provider_pk_hex.is_empty() {
        bail!("provider_pk_hex is required for provider registration");
    }
    let pk_bytes = hex::decode(&params.provider_pk_hex)
        .context("Invalid provider PK hex")?;
    if pk_bytes.len() != 33 {
        bail!(
            "Provider PK must be 33 bytes (compressed secp256k1), got {} bytes",
            pk_bytes.len()
        );
    }

    // Encode registers as Sigma constants
    // R4: GroupElement (provider PK)
    let r4_pk = encode_group_element(&pk_bytes)
        .context("Failed to encode provider PK as GroupElement")?;

    // R5: Endpoint URL (Coll[Byte], UTF-8)
    let r5_endpoint = encode_string(&params.endpoint_url);

    // R6: Price per token (Long, nanoERG)
    let r6_price = encode_long(params.price_per_token as i64);

    // R7: Creation height (Int)
    let r7_height = encode_int(height);

    let box_value = config.registration_stake_nanoerg;
    if box_value < SAFE_MIN_BOX_VALUE {
        bail!(
            "registration_stake_nanoerg {} is below minimum box value {}",
            box_value,
            SAFE_MIN_BOX_VALUE
        );
    }

    // Build the payment request
    let request_obj = serde_json::json!({
        "ergoTree": tree_hex,
        "value": box_value.to_string(),
        "assets": [{
            "amount": 1,
            "name": format!("XergonProvider-{}", params.provider_name),
            "description": format!("Xergon Provider NFT: {}", params.provider_name),
            "decimals": 0,
            "type": "EIP-004"
        }],
        "registers": {
            "R4": r4_pk,
            "R5": r5_endpoint,
            "R6": r6_price,
            "R7": r7_height
        }
    });

    let payment_request = serde_json::json!({
        "requests": [request_obj],
        "fee": DEFAULT_FEE.to_string()
    });

    debug!(
        provider_name = %params.provider_name,
        tree_hex_len = tree_hex.len(),
        box_value,
        "Submitting provider registration transaction"
    );

    let tx_id = client
        .wallet_payment_send(&payment_request)
        .await
        .context("Failed to submit provider registration transaction via wallet")?;

    info!(
        tx_id = %tx_id,
        provider_name = %params.provider_name,
        "Provider registration transaction submitted"
    );

    // Fetch the transaction to extract NFT token ID and box ID
    let tx_detail = client
        .get_transaction(&tx_id)
        .await
        .context("Failed to fetch registration transaction details")?;

    let provider_nft_id = extract_nft_token_id_from_tx(&tx_detail)?;
    let provider_box_id = extract_box_with_nft_from_tx(&tx_detail, &provider_nft_id)?;

    info!(
        tx_id = %tx_id,
        provider_name = %params.provider_name,
        provider_nft_id = %provider_nft_id,
        provider_box_id = %provider_box_id,
        "Provider registration complete"
    );

    Ok(ProviderRegistrationResult {
        tx_id,
        provider_nft_id,
        provider_box_id,
    })
}

// ---------------------------------------------------------------------------
// Query
// ---------------------------------------------------------------------------

/// Query the UTXO set for all provider boxes matching the `provider_registration`
/// ErgoTree contract.
///
/// Uses the node API `GET /api/v1/boxes/unspent/byErgoTree/{ergoTree}`.
/// Parses R4-R7 from `additionalRegisters` on each box.
///
/// Returns a list of `OnChainProvider` with decoded register values.
pub async fn query_provider_boxes(node_url: &str) -> Result<Vec<OnChainProvider>> {
    let tree_hex = crate::contract_compile::get_contract_hex("provider_registration")
        .context("provider_registration contract not found")?;

    let client = ErgoNodeClient::new(node_url.to_string());

    let boxes = client
        .get_boxes_by_ergo_tree(&tree_hex)
        .await
        .context("Failed to scan UTXO for provider boxes")?;

    debug!(box_count = boxes.len(), "Found raw provider boxes");

    let mut providers = Vec::new();

    for box_data in &boxes {
        // Extract NFT token ID (first token with amount=1)
        let nft_token_id = box_data
            .assets
            .iter()
            .find(|a| a.amount == 1)
            .map(|a| a.token_id.clone())
            .unwrap_or_default();

        // Parse R4: provider PK (Coll[Byte] encoded)
        let provider_pk = decode_coll_byte_register(&box_data.additional_registers, "R4");

        // Parse R5: endpoint URL (Coll[Byte] / String encoded)
        let endpoint = decode_string_register(&box_data.additional_registers, "R5");

        // Parse R6: price per token (Long encoded)
        let price_per_token = decode_long_register(&box_data.additional_registers, "R6")
            .unwrap_or(0) as u64;

        // Parse R7: creation height (Int encoded)
        let creation_height = decode_int_register(&box_data.additional_registers, "R7").unwrap_or(0);

        providers.push(OnChainProvider {
            box_id: box_data.box_id.clone(),
            tx_id: box_data.tx_id.clone(),
            provider_pk,
            endpoint,
            price_per_token,
            creation_height,
            nft_token_id,
            value_nanoerg: box_data.value,
            ergo_tree: box_data.ergo_tree.clone(),
        });
    }

    info!(provider_count = providers.len(), "Queried on-chain providers");
    Ok(providers)
}

// ---------------------------------------------------------------------------
// Update
// ---------------------------------------------------------------------------

/// Update an existing provider box with new R5 (endpoint) and/or R6 (price).
///
/// Spends the existing provider box (found by box_id) and creates a new one
/// with the same NFT, same ErgoTree, and updated registers. Preserves R4 and R7.
///
/// Returns the transaction ID and the new box ID.
pub async fn update_provider_status(
    client: &ErgoNodeClient,
    params: &UpdateProviderParams,
) -> Result<ProviderUpdateResult> {
    if params.new_price.is_none() && params.new_endpoint.is_none() {
        bail!("At least one of new_price or new_endpoint must be provided");
    }

    // Fetch the existing box
    let box_data = client
        .get_box(&params.box_id)
        .await
        .with_context(|| format!("Failed to fetch provider box {}", params.box_id))?;

    // Verify it has the provider_registration ErgoTree
    let expected_tree = crate::contract_compile::get_contract_hex("provider_registration")
        .context("provider_registration contract not found")?;
    if !box_data.ergo_tree.eq_ignore_ascii_case(&expected_tree) {
        bail!(
            "Box {} does not match the provider_registration ErgoTree",
            params.box_id
        );
    }

    // Find the NFT token
    let nft_token = box_data
        .assets
        .iter()
        .find(|a| a.amount == 1)
        .context("Provider box does not contain an NFT token (amount=1)")?;

    // Decode existing registers
    let existing_r4 = box_data
        .additional_registers
        .get("R4")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let existing_r7 = box_data
        .additional_registers
        .get("R7")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Build updated registers
    let r5_new = if let Some(ref endpoint) = params.new_endpoint {
        encode_string(endpoint)
    } else {
        // Keep existing R5
        box_data
            .additional_registers
            .get("R5")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };

    let r6_new = if let Some(price) = params.new_price {
        encode_long(price as i64)
    } else {
        // Keep existing R6
        box_data
            .additional_registers
            .get("R6")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };

    // Check wallet
    let wallet_ready = client
        .wallet_status()
        .await
        .context("Failed to check wallet status")?;
    if !wallet_ready {
        bail!("Ergo node wallet is locked. Unlock it before updating provider.");
    }

    // Build payment request: spend existing box, create new one with same NFT
    let request_obj = serde_json::json!({
        "ergoTree": box_data.ergo_tree,
        "value": box_data.value.to_string(),
        "assets": [{
            "tokenId": nft_token.token_id,
            "amount": 1
        }],
        "registers": {
            "R4": existing_r4,
            "R5": r5_new,
            "R6": r6_new,
            "R7": existing_r7
        }
    });

    let payment_request = serde_json::json!({
        "requests": [request_obj],
        "fee": DEFAULT_FEE.to_string(),
        "inputsRaw": [params.box_id.clone()],
        "dataInputsRaw": []
    });

    debug!(
        box_id = %params.box_id,
        nft_token_id = %nft_token.token_id,
        "Submitting provider update transaction"
    );

    let tx_id = client
        .wallet_payment_send(&payment_request)
        .await
        .context("Failed to submit provider update transaction via wallet")?;

    info!(
        tx_id = %tx_id,
        box_id = %params.box_id,
        "Provider update transaction submitted"
    );

    // Fetch the transaction to extract the new box ID
    let tx_detail = client
        .get_transaction(&tx_id)
        .await
        .context("Failed to fetch update transaction details")?;

    let new_box_id = extract_box_with_nft_from_tx(&tx_detail, &nft_token.token_id)?;

    Ok(ProviderUpdateResult {
        tx_id,
        new_box_id,
    })
}

// ---------------------------------------------------------------------------
// Auto-register helper
// ---------------------------------------------------------------------------

/// Attempt to auto-register the provider on startup.
///
/// Checks if a provider box already exists for this PK, and if not,
/// registers using the config and current agent settings.
/// This is a best-effort operation -- failures are logged but not fatal.
pub async fn auto_register_if_needed(
    client: &ErgoNodeClient,
    config: &ProviderRegistryConfig,
    provider_name: &str,
    endpoint_url: &str,
    provider_pk_hex: &str,
    price_per_token: u64,
) -> Result<Option<ProviderRegistrationResult>> {
    // Check if provider already registered by scanning UTXO for matching PK
    let tree_hex = crate::contract_compile::get_contract_hex("provider_registration")
        .context("provider_registration contract not found")?;
    let existing_boxes = client
        .get_boxes_by_ergo_tree(&tree_hex)
        .await
        .context("Failed to scan UTXO for existing provider boxes")?;

    let already_registered = existing_boxes.iter().any(|b| {
        b.additional_registers
            .get("R4")
            .and_then(|v| v.as_str())
            .map(|hex_val| {
                let bytes = hex::decode(hex_val).unwrap_or_default();
                // Coll[Byte] format: 0x0e 0x21 <33 bytes>
                if bytes.len() >= 35 && bytes[0] == 0x0e && bytes[1] == 0x21 {
                    hex::encode(&bytes[2..35]) == provider_pk_hex
                } else {
                    false
                }
            })
            .unwrap_or(false)
    });

    if already_registered {
        info!(
            provider_name = %provider_name,
            "Provider already registered on-chain, skipping auto-registration"
        );
        return Ok(None);
    }

    info!(
        provider_name = %provider_name,
        "Auto-registering provider on-chain"
    );

    let params = RegisterProviderParams {
        provider_name: provider_name.to_string(),
        endpoint_url: endpoint_url.to_string(),
        price_per_token,
        staking_address: String::new(),
        provider_pk_hex: provider_pk_hex.to_string(),
    };

    match register_provider_on_chain(client, config, &params).await {
        Ok(result) => {
            info!(
                tx_id = %result.tx_id,
                provider_nft_id = %result.provider_nft_id,
                "Auto-registration successful"
            );
            Ok(Some(result))
        }
        Err(e) => {
            warn!(
                error = %e,
                provider_name = %provider_name,
                "Auto-registration failed (non-fatal)"
            );
            Err(e)
        }
    }
}

// ---------------------------------------------------------------------------
// Sigma constant decoding helpers
// ---------------------------------------------------------------------------

/// Decode a Coll[Byte] register value (tag 0x0e) from hex to raw bytes,
/// then interpret as UTF-8 string.
fn decode_string_register(registers: &HashMap<String, serde_json::Value>, key: &str) -> String {
    let hex_val = registers
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if hex_val.is_empty() {
        return String::new();
    }

    let bytes = match hex::decode(hex_val) {
        Ok(b) => b,
        Err(_) => return String::new(),
    };

    // Coll[Byte] tag is 0x0e
    if bytes.is_empty() || bytes[0] != 0x0e {
        return String::new();
    }

    // Skip tag byte, parse VLB length, then read UTF-8 data
    let (data_len, consumed) = decode_vlb(&bytes[1..]);
    let data_start = 1 + consumed;
    if data_start + data_len > bytes.len() {
        return String::new();
    }

    String::from_utf8_lossy(&bytes[data_start..data_start + data_len]).to_string()
}

/// Decode a Coll[Byte] register value (tag 0x0e) and return raw hex string.
fn decode_coll_byte_register(registers: &HashMap<String, serde_json::Value>, key: &str) -> String {
    let hex_val = registers
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if hex_val.is_empty() {
        return String::new();
    }

    let bytes = match hex::decode(hex_val) {
        Ok(b) => b,
        Err(_) => return String::new(),
    };

    if bytes.is_empty() || bytes[0] != 0x0e {
        return String::new();
    }

    let (data_len, consumed) = decode_vlb(&bytes[1..]);
    let data_start = 1 + consumed;
    if data_start + data_len > bytes.len() {
        return String::new();
    }

    hex::encode(&bytes[data_start..data_start + data_len])
}

/// Decode a Long register value (tag 0x05, 8 bytes big-endian).
fn decode_long_register(
    registers: &HashMap<String, serde_json::Value>,
    key: &str,
) -> Option<i64> {
    let hex_val = registers
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if hex_val.is_empty() {
        return None;
    }

    let bytes = match hex::decode(hex_val) {
        Ok(b) => b,
        Err(_) => return None,
    };

    if bytes.len() < 9 || bytes[0] != 0x05 {
        return None;
    }

    let mut arr = [0u8; 8];
    arr.copy_from_slice(&bytes[1..9]);
    Some(i64::from_be_bytes(arr))
}

/// Decode an Int register value (tag 0x04, 4 bytes big-endian).
fn decode_int_register(
    registers: &HashMap<String, serde_json::Value>,
    key: &str,
) -> Option<i32> {
    let hex_val = registers
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if hex_val.is_empty() {
        return None;
    }

    let bytes = match hex::decode(hex_val) {
        Ok(b) => b,
        Err(_) => return None,
    };

    if bytes.len() < 5 || bytes[0] != 0x04 {
        return None;
    }

    let mut arr = [0u8; 4];
    arr.copy_from_slice(&bytes[1..5]);
    Some(i32::from_be_bytes(arr))
}

/// Decode a VLB (variable-length byte) encoded length.
/// Returns (length, bytes_consumed).
fn decode_vlb(data: &[u8]) -> (usize, usize) {
    if data.is_empty() {
        return (0, 0);
    }
    if data[0] < 128 {
        (data[0] as usize, 1)
    } else if data.len() >= 2 {
        let len = (((data[0] & 0x7F) as usize) << 7) | (data[1] as usize);
        (len, 2)
    } else {
        (0, 1)
    }
}

// ---------------------------------------------------------------------------
// Transaction extraction helpers
// ---------------------------------------------------------------------------

/// Extract the NFT token ID from a transaction's outputs.
/// Finds the first token with amount=1 in any output.
fn extract_nft_token_id_from_tx(tx: &serde_json::Value) -> Result<String> {
    let outputs = tx
        .get("outputs")
        .and_then(|o| o.as_array())
        .context("Transaction has no outputs")?;

    for output in outputs {
        let assets = output.get("assets").and_then(|a| a.as_array());
        if let Some(assets) = assets {
            for asset in assets {
                let amount = asset.get("amount").and_then(|a| a.as_u64()).unwrap_or(0);
                if amount == 1 {
                    let token_id = asset
                        .get("tokenId")
                        .and_then(|t| t.as_str())
                        .unwrap_or("");
                    if !token_id.is_empty() {
                        return Ok(token_id.to_string());
                    }
                }
            }
        }
    }

    bail!("No NFT token (amount=1) found in transaction outputs")
}

/// Extract the box ID of the output containing a specific NFT token.
fn extract_box_with_nft_from_tx(tx: &serde_json::Value, nft_token_id: &str) -> Result<String> {
    let outputs = tx
        .get("outputs")
        .and_then(|o| o.as_array())
        .context("Transaction has no outputs")?;

    for output in outputs {
        let assets = output.get("assets").and_then(|a| a.as_array());
        if let Some(assets) = assets {
            for asset in assets {
                let token_id = asset.get("tokenId").and_then(|t| t.as_str()).unwrap_or("");
                let amount = asset.get("amount").and_then(|a| a.as_u64()).unwrap_or(0);
                if token_id == nft_token_id && amount == 1 {
                    let box_id = output
                        .get("boxId")
                        .and_then(|b| b.as_str())
                        .unwrap_or("");
                    if !box_id.is_empty() {
                        return Ok(box_id.to_string());
                    }
                }
            }
        }
    }

    bail!(
        "No output containing NFT token {} found in transaction",
        nft_token_id
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Data type serialization tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_register_provider_params_serialization() {
        let params = RegisterProviderParams {
            provider_name: "Xergon_US-East-1".to_string(),
            endpoint_url: "http://192.168.1.5:9099".to_string(),
            price_per_token: 50_000,
            staking_address: "9fBsJiCzNBiDkiMC2Y6shy9J8JnxGpVD2wPBfyhSysmfKTiXNGh".to_string(),
            provider_pk_hex: "02aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
        };

        let json = serde_json::to_string(&params).unwrap();
        let parsed: RegisterProviderParams = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.provider_name, "Xergon_US-East-1");
        assert_eq!(parsed.endpoint_url, "http://192.168.1.5:9099");
        assert_eq!(parsed.price_per_token, 50_000);
        assert_eq!(parsed.staking_address, params.staking_address);
        assert_eq!(parsed.provider_pk_hex, params.provider_pk_hex);
    }

    #[test]
    fn test_register_provider_params_defaults() {
        // Test that default (empty) fields deserialize correctly
        let json = r#"{"provider_name":"test","endpoint_url":"http://localhost","price_per_token":100}"#;
        let parsed: RegisterProviderParams = serde_json::from_str(json).unwrap();

        assert_eq!(parsed.provider_name, "test");
        assert_eq!(parsed.endpoint_url, "http://localhost");
        assert_eq!(parsed.price_per_token, 100);
        // Default fields should be empty strings
        assert!(parsed.staking_address.is_empty());
        assert!(parsed.provider_pk_hex.is_empty());
    }

    #[test]
    fn test_provider_registration_result_serialization() {
        let result = ProviderRegistrationResult {
            tx_id: "abc123".to_string(),
            provider_nft_id: "nft456".to_string(),
            provider_box_id: "box789".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: ProviderRegistrationResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.tx_id, "abc123");
        assert_eq!(parsed.provider_nft_id, "nft456");
        assert_eq!(parsed.provider_box_id, "box789");
    }

    #[test]
    fn test_on_chain_provider_serialization() {
        let provider = OnChainProvider {
            box_id: "deadbeef".to_string(),
            tx_id: "cafebabe".to_string(),
            provider_pk: "02abcdef".to_string(),
            endpoint: "http://192.168.1.5:9099".to_string(),
            price_per_token: 75_000,
            creation_height: 500,
            nft_token_id: "token123".to_string(),
            value_nanoerg: 2_000_000_000,
            ergo_tree: "1008040004000500040010042004".to_string(),
        };

        let json = serde_json::to_string(&provider).unwrap();
        let parsed: OnChainProvider = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.box_id, "deadbeef");
        assert_eq!(parsed.tx_id, "cafebabe");
        assert_eq!(parsed.provider_pk, "02abcdef");
        assert_eq!(parsed.endpoint, "http://192.168.1.5:9099");
        assert_eq!(parsed.price_per_token, 75_000);
        assert_eq!(parsed.creation_height, 500);
        assert_eq!(parsed.nft_token_id, "token123");
        assert_eq!(parsed.value_nanoerg, 2_000_000_000);
        assert_eq!(parsed.ergo_tree, "1008040004000500040010042004");
    }

    #[test]
    fn test_update_provider_params_serialization() {
        let params = UpdateProviderParams {
            box_id: "box999".to_string(),
            new_price: Some(100_000),
            new_endpoint: Some("http://new-endpoint:9099".to_string()),
        };

        let json = serde_json::to_string(&params).unwrap();
        let parsed: UpdateProviderParams = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.box_id, "box999");
        assert_eq!(parsed.new_price, Some(100_000));
        assert_eq!(parsed.new_endpoint, Some("http://new-endpoint:9099".to_string()));
    }

    #[test]
    fn test_update_provider_params_none_values() {
        let params = UpdateProviderParams {
            box_id: "box999".to_string(),
            new_price: None,
            new_endpoint: None,
        };

        let json = serde_json::to_string(&params).unwrap();
        let parsed: UpdateProviderParams = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.box_id, "box999");
        assert!(parsed.new_price.is_none());
        assert!(parsed.new_endpoint.is_none());
    }

    #[test]
    fn test_provider_update_result_serialization() {
        let result = ProviderUpdateResult {
            tx_id: "update_tx_123".to_string(),
            new_box_id: "new_box_456".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: ProviderUpdateResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.tx_id, "update_tx_123");
        assert_eq!(parsed.new_box_id, "new_box_456");
    }

    // -------------------------------------------------------------------------
    // VLB decoding tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_decode_vlb_single_byte() {
        // Values < 128 use a single byte
        let data = [0x05u8];
        let (len, consumed) = decode_vlb(&data);
        assert_eq!(len, 5);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn test_decode_vlb_single_byte_zero() {
        let data = [0x00u8];
        let (len, consumed) = decode_vlb(&data);
        assert_eq!(len, 0);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn test_decode_vlb_single_byte_max() {
        // 127 is the max single-byte value
        let data = [0x7Fu8];
        let (len, consumed) = decode_vlb(&data);
        assert_eq!(len, 127);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn test_decode_vlb_two_byte() {
        // 200 = 0x80 | (200 >> 7), (200 & 0x7F) = 0x81, 0x48
        let data = [0x81u8, 0x48u8];
        let (len, consumed) = decode_vlb(&data);
        assert_eq!(len, 200);
        assert_eq!(consumed, 2);
    }

    #[test]
    fn test_decode_vlb_empty() {
        let data: [u8; 0] = [];
        let (len, consumed) = decode_vlb(&data);
        assert_eq!(len, 0);
        assert_eq!(consumed, 0);
    }

    #[test]
    fn test_decode_vlb_truncated() {
        // High bit set but no second byte
        let data = [0x80u8];
        let (len, consumed) = decode_vlb(&data);
        assert_eq!(len, 0);
        assert_eq!(consumed, 1);
    }

    // -------------------------------------------------------------------------
    // Sigma register decoding tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_decode_string_register_valid() {
        // Build a valid Coll[Byte] encoded string: 0x0e <vlb_len> <utf8_bytes>
        // "hello" = 5 bytes: 0e 05 68656c6c6f
        let hex_val = "0e0568656c6c6f";
        let mut registers = HashMap::new();
        registers.insert("R5".to_string(), serde_json::Value::String(hex_val.to_string()));

        let result = decode_string_register(&registers, "R5");
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_decode_string_register_empty() {
        let registers: HashMap<String, serde_json::Value> = HashMap::new();
        let result = decode_string_register(&registers, "R5");
        assert!(result.is_empty());
    }

    #[test]
    fn test_decode_string_register_invalid_hex() {
        let mut registers = HashMap::new();
        registers.insert("R5".to_string(), serde_json::Value::String("zzzz".to_string()));

        let result = decode_string_register(&registers, "R5");
        assert!(result.is_empty());
    }

    #[test]
    fn test_decode_string_register_wrong_tag() {
        // Tag 0x05 instead of 0x0e
        let hex_val = "0500000001";
        let mut registers = HashMap::new();
        registers.insert("R5".to_string(), serde_json::Value::String(hex_val.to_string()));

        let result = decode_string_register(&registers, "R5");
        assert!(result.is_empty());
    }

    #[test]
    fn test_decode_coll_byte_register_valid() {
        // 33-byte PK: 0e 21 <33 bytes>
        let pk_hex = "02aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let vlb = vec![0x0e, 0x21];
        let pk_bytes = hex::decode(pk_hex).unwrap();
        let mut all_bytes = vlb;
        all_bytes.extend_from_slice(&pk_bytes);
        let hex_val = hex::encode(&all_bytes);

        let mut registers = HashMap::new();
        registers.insert("R4".to_string(), serde_json::Value::String(hex_val));

        let result = decode_coll_byte_register(&registers, "R4");
        assert_eq!(result, pk_hex);
    }

    #[test]
    fn test_decode_long_register_valid() {
        // Long: 05 + 8 bytes big-endian = 9 bytes
        let hex_val = "050000000000000032"; // 50 in decimal (0x32)
        let mut registers = HashMap::new();
        registers.insert("R6".to_string(), serde_json::Value::String(hex_val.to_string()));

        let result = decode_long_register(&registers, "R6");
        assert_eq!(result, Some(50));
    }

    #[test]
    fn test_decode_long_register_negative() {
        // Long: 05 + 8 bytes big-endian = -1
        let hex_val = "05ffffffffffffffff";
        let mut registers = HashMap::new();
        registers.insert("R6".to_string(), serde_json::Value::String(hex_val.to_string()));

        let result = decode_long_register(&registers, "R6");
        assert_eq!(result, Some(-1));
    }

    #[test]
    fn test_decode_long_register_invalid_tag() {
        // Int tag (0x04) instead of Long tag (0x05)
        let hex_val = "0400000001";
        let mut registers = HashMap::new();
        registers.insert("R6".to_string(), serde_json::Value::String(hex_val.to_string()));

        let result = decode_long_register(&registers, "R6");
        assert!(result.is_none());
    }

    #[test]
    fn test_decode_long_register_short_bytes() {
        // Only 5 bytes, need 9 for Long
        let hex_val = "05deadbeef";
        let mut registers = HashMap::new();
        registers.insert("R6".to_string(), serde_json::Value::String(hex_val.to_string()));

        let result = decode_long_register(&registers, "R6");
        assert!(result.is_none());
    }

    #[test]
    fn test_decode_long_register_empty() {
        let registers: HashMap<String, serde_json::Value> = HashMap::new();
        let result = decode_long_register(&registers, "R6");
        assert!(result.is_none());
    }

    #[test]
    fn test_decode_int_register_valid() {
        // Int: 04 + 4 bytes big-endian = 5 bytes
        let hex_val = "0400000064"; // 100 in decimal
        let mut registers = HashMap::new();
        registers.insert("R7".to_string(), serde_json::Value::String(hex_val.to_string()));

        let result = decode_int_register(&registers, "R7");
        assert_eq!(result, Some(100));
    }

    #[test]
    fn test_decode_int_register_invalid_tag() {
        // Long tag (0x05) instead of Int tag (0x04)
        let hex_val = "050000000000006400";
        let mut registers = HashMap::new();
        registers.insert("R7".to_string(), serde_json::Value::String(hex_val.to_string()));

        let result = decode_int_register(&registers, "R7");
        assert!(result.is_none());
    }

    #[test]
    fn test_decode_int_register_short_bytes() {
        // Only 3 bytes, need 5 for Int
        let hex_val = "04dead";
        let mut registers = HashMap::new();
        registers.insert("R7".to_string(), serde_json::Value::String(hex_val.to_string()));

        let result = decode_int_register(&registers, "R7");
        assert!(result.is_none());
    }

    // -------------------------------------------------------------------------
    // Transaction extraction tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_extract_nft_token_id_from_tx() {
        let tx = serde_json::json!({
            "outputs": [
                {
                    "boxId": "box1",
                    "assets": [
                        {"tokenId": "token_regular", "amount": 1000},
                        {"tokenId": "nft_token_abc", "amount": 1}
                    ]
                },
                {
                    "boxId": "box2",
                    "assets": []
                }
            ]
        });

        let nft_id = extract_nft_token_id_from_tx(&tx).unwrap();
        assert_eq!(nft_id, "nft_token_abc");
    }

    #[test]
    fn test_extract_nft_token_id_no_outputs() {
        // No "outputs" key at all -> context error
        let tx = serde_json::json!({"inputs": []});
        let result = extract_nft_token_id_from_tx(&tx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no outputs"));
    }

    #[test]
    fn test_extract_nft_token_id_none_found() {
        let tx = serde_json::json!({
            "outputs": [
                {
                    "boxId": "box1",
                    "assets": [
                        {"tokenId": "token1", "amount": 100},
                        {"tokenId": "token2", "amount": 50}
                    ]
                }
            ]
        });

        let result = extract_nft_token_id_from_tx(&tx);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NFT token"));
    }

    #[test]
    fn test_extract_box_with_nft_from_tx() {
        let tx = serde_json::json!({
            "outputs": [
                {
                    "boxId": "target_box",
                    "assets": [
                        {"tokenId": "my_nft_id", "amount": 1}
                    ]
                },
                {
                    "boxId": "other_box",
                    "assets": [
                        {"tokenId": "other_token", "amount": 1}
                    ]
                }
            ]
        });

        let box_id = extract_box_with_nft_from_tx(&tx, "my_nft_id").unwrap();
        assert_eq!(box_id, "target_box");
    }

    #[test]
    fn test_extract_box_with_nft_not_found() {
        let tx = serde_json::json!({
            "outputs": [
                {
                    "boxId": "box1",
                    "assets": [{"tokenId": "other_nft", "amount": 1}]
                }
            ]
        });

        let result = extract_box_with_nft_from_tx(&tx, "missing_nft");
        assert!(result.is_err());
    }

    // -------------------------------------------------------------------------
    // Config defaults tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_provider_registry_config_defaults() {
        let config = crate::config::ProviderRegistryConfig::default();
        assert!(!config.enabled);
        assert!(!config.auto_register);
        assert_eq!(config.registration_stake_nanoerg, 1_000_000_000);
        assert!(config.provider_pk_hex.is_empty());
        assert!(config.endpoint_url.is_empty());
        assert_eq!(config.price_per_token, 50_000);
    }

    #[test]
    fn test_oracle_config_defaults() {
        let config = crate::config::OracleConfig::default();
        assert!(!config.enabled);
        assert!(config.pool_nft_id.is_empty());
        assert_eq!(config.refresh_interval_secs, 600);
    }

    #[test]
    fn test_settlement_config_defaults() {
        let config = crate::config::SettlementConfig::default();
        assert!(!config.enabled);
        assert!(config.dry_run); // default_dry_run() returns true
        assert!(!config.chain_enabled);
        assert_eq!(config.interval_secs, 86400);
        assert_eq!(config.cost_per_1k_tokens_nanoerg, 1_000_000);
        assert_eq!(config.min_settlement_nanoerg, 1_000_000_000);
        assert_eq!(config.min_confirmations, 30);
        assert!(config.ledger_file.is_none());
    }
}
