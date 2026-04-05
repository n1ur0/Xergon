//! Protocol bootstrap and provider registration.
//!
//! This module handles the on-chain deployment of the Xergon Network protocol:
//!
//! - **Bootstrap**: Mints the Xergon Network NFT (singleton, supply=1) and creates
//!   the Treasury Box that holds the NFT + ERG reserve.
//! - **Provider Registration**: Mints a per-provider NFT and creates a Provider Box
//!   with the required registers (R4-R9).
//!
//! Both operations use the Ergo node wallet API (`POST /wallet/payment/send`),
//! which builds, signs, and broadcasts the transaction in a single call.

use anyhow::{bail, Context, Result};
use blake2::Digest;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::chain::client::ErgoNodeClient;
use crate::chain::transactions::{encode_coll_byte, encode_group_element, encode_int, encode_string};
use crate::protocol::tx_safety::{validate_address_or_tree, validate_payment_request, validate_pk_hex};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Minimum ERG value for any box on Ergo (0.001 ERG = 1,000,000 nanoERG).
pub const SAFE_MIN_BOX_VALUE: u64 = 1_000_000;

/// Recommended minimum transaction fee.
pub const RECOMMENDED_MIN_FEE: u64 = 1_000_000;

/// Default ERG amount to lock in the Treasury Box (1.0 ERG).
pub const DEFAULT_TREASURY_ERG: u64 = 1_000_000_000;

/// Initial PoNW score for a newly registered provider.
pub const INITIAL_POWN_SCORE: i32 = 0;

/// Initial heartbeat height for a newly registered provider (0 = not yet active).
pub const INITIAL_HEARTBEAT: i32 = 0;

// ---------------------------------------------------------------------------
// Bootstrap Config
// ---------------------------------------------------------------------------

/// Configuration for the protocol bootstrap (genesis) deployment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapConfig {
    /// Deployer's Ergo address (P2S or P2PK) that guards the Treasury Box.
    pub deployer_address: String,
    /// Amount of ERG (in nanoERG) to lock in the Treasury Box.
    pub treasury_erg_nanoerg: u64,
    /// Compiled ErgoTree hex for the treasury_box.es contract.
    /// If empty, the deployer address is used directly as the guard.
    pub treasury_tree_hex: String,
    /// NFT token name.
    pub nft_name: String,
    /// NFT token description.
    pub nft_description: String,
    /// NFT token decimals (should be 0 for NFT).
    pub nft_decimals: i32,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            deployer_address: String::new(),
            treasury_erg_nanoerg: DEFAULT_TREASURY_ERG,
            treasury_tree_hex: String::new(),
            nft_name: "XergonNetworkNFT".to_string(),
            nft_description: "Xergon Network Protocol Identity".to_string(),
            nft_decimals: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Treasury Box Spec
// ---------------------------------------------------------------------------

/// Specification for the Treasury Box layout on-chain.
///
/// The Treasury Box holds the Xergon Network NFT and an ERG reserve.
/// It is guarded by the deployer's public key (via treasury_box.es contract).
///
/// Register layout:
/// - R4: GroupElement — Deployer/governance public key
/// - R5: Long — Total ERG airdropped so far (0 at genesis)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreasuryBoxSpec {
    /// Xergon Network NFT token ID (set after minting).
    pub nft_token_id: String,
    /// Treasury Box ID (set after creation).
    pub treasury_box_id: String,
    /// ERG value in the Treasury Box (nanoERG).
    pub value_nanoerg: u64,
    /// Genesis transaction ID.
    pub genesis_tx_id: String,
}

// ---------------------------------------------------------------------------
// Provider Registration Spec
// ---------------------------------------------------------------------------

/// Information needed to register a new provider on-chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRegistrationInfo {
    /// Unique provider identifier (human-readable, e.g., "provider-1").
    pub provider_id: String,
    /// Provider's compressed secp256k1 public key (hex, 33 bytes).
    pub provider_pk_hex: String,
    /// Provider endpoint URL (e.g., "http://192.168.1.5:9099").
    pub endpoint: String,
    /// Models served (JSON array string, e.g., r#"["llama-3.1-8b","mistral-7b"]"#).
    pub models_json: String,
    /// Provider region code (e.g., "us-east").
    pub region: String,
    /// Compiled ErgoTree hex for the provider_box.es contract.
    /// If empty, the provider PK address is used as the guard.
    pub provider_tree_hex: String,
    /// ERG value to put in the Provider Box (must be >= SAFE_MIN_BOX_VALUE).
    pub box_value_nanoerg: u64,
}

impl Default for ProviderRegistrationInfo {
    fn default() -> Self {
        Self {
            provider_id: String::new(),
            provider_pk_hex: String::new(),
            endpoint: String::new(),
            models_json: "[]".to_string(),
            region: "unknown".to_string(),
            provider_tree_hex: String::new(),
            box_value_nanoerg: SAFE_MIN_BOX_VALUE,
        }
    }
}

/// Result of a provider registration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRegistrationResult {
    /// Transaction ID of the registration transaction.
    pub tx_id: String,
    /// Provider NFT token ID (set after minting).
    pub provider_nft_id: String,
    /// Provider Box ID (set after creation).
    pub provider_box_id: String,
}

// ---------------------------------------------------------------------------
// Bootstrap functions
// ---------------------------------------------------------------------------

/// Check if the Treasury Box already exists on-chain.
///
/// Scans the UTXO set for boxes containing the Xergon Network NFT.
/// If `nft_token_id` is provided, scans directly by that token.
/// Otherwise, scans by the treasury ErgoTree (if `treasury_tree_hex` is provided).
pub async fn check_treasury_exists(
    client: &ErgoNodeClient,
    nft_token_id: Option<&str>,
    treasury_tree_hex: Option<&str>,
) -> Result<bool> {
    // Primary: scan by NFT token ID
    if let Some(token_id) = nft_token_id {
        if !token_id.is_empty() {
            let boxes = client
                .get_boxes_by_token_id(token_id)
                .await
                .context("Failed to scan UTXO for Treasury NFT")?;

            if !boxes.is_empty() {
                info!(
                    nft_token_id = %token_id,
                    box_count = boxes.len(),
                    "Treasury box found on-chain (by NFT token ID)"
                );
                return Ok(true);
            }
        }
    }

    // Fallback: scan by treasury ErgoTree
    if let Some(tree_hex) = treasury_tree_hex {
        if !tree_hex.is_empty() {
            let boxes = client
                .get_boxes_by_ergo_tree(tree_hex)
                .await
                .context("Failed to scan UTXO for Treasury ErgoTree")?;

            if !boxes.is_empty() {
                info!(
                    box_count = boxes.len(),
                    "Treasury box found on-chain (by ErgoTree)"
                );
                return Ok(true);
            }
        }
    }

    debug!("Treasury box not found on-chain");
    Ok(false)
}

/// Build and submit the bootstrap (genesis) transaction.
///
/// Mints the Xergon Network NFT (supply=1) and creates the Treasury Box
/// holding the NFT + ERG reserve.
///
/// Uses the Ergo node wallet API to build, sign, and broadcast.
///
/// Returns the `TreasuryBoxSpec` with the NFT token ID, box ID, and tx ID.
pub async fn build_treasury_tx(
    client: &ErgoNodeClient,
    config: &BootstrapConfig,
) -> Result<TreasuryBoxSpec> {
    if config.deployer_address.is_empty() {
        bail!("deployer_address is required for bootstrap");
    }
    validate_address_or_tree(&config.deployer_address)
        .context("Invalid deployer_address for bootstrap")?;

    if config.treasury_erg_nanoerg < SAFE_MIN_BOX_VALUE {
        bail!(
            "treasury_erg_nanoerg {} is below minimum box value {}",
            config.treasury_erg_nanoerg,
            SAFE_MIN_BOX_VALUE
        );
    }

    // Check wallet is ready
    let wallet_ready = client
        .wallet_status()
        .await
        .context("Failed to check wallet status")?;
    if !wallet_ready {
        bail!("Ergo node wallet is locked. Unlock it before bootstrap.");
    }

    // Build the payment request
    let mut request_obj = serde_json::json!({
        "value": config.treasury_erg_nanoerg.to_string(),
        "assets": [{
            "amount": 1,
            "name": config.nft_name,
            "description": config.nft_description,
            "decimals": config.nft_decimals,
            "type": "EIP-004"
        }],
        "registers": {}
    });

    // Use compiled ErgoTree if available, otherwise use address
    if !config.treasury_tree_hex.is_empty() {
        request_obj["ergoTree"] = serde_json::Value::String(config.treasury_tree_hex.clone());
    } else {
        request_obj["address"] =
            serde_json::Value::String(config.deployer_address.clone());
    }

    let payment_request = serde_json::json!({
        "requests": [request_obj],
        "fee": RECOMMENDED_MIN_FEE.to_string()
    });

    debug!(
        deployer = %config.deployer_address,
        treasury_erg = config.treasury_erg_nanoerg,
        "Submitting bootstrap transaction"
    );

    validate_payment_request(&payment_request)
        .context("Bootstrap transaction safety validation failed")?;

    let tx_id = client
        .wallet_payment_send(&payment_request)
        .await
        .context("Failed to submit bootstrap transaction via wallet")?;

    info!(tx_id = %tx_id, "Bootstrap transaction submitted");

    // Fetch the transaction to extract NFT token ID and Treasury box ID
    let tx_detail = client
        .get_transaction(&tx_id)
        .await
        .context("Failed to fetch bootstrap transaction details")?;

    // Extract NFT token ID from outputs (first token with amount=1)
    let nft_token_id = extract_nft_token_id(&tx_detail)?;

    // Extract Treasury box ID (the output containing the NFT)
    let treasury_box_id = extract_box_with_nft(&tx_detail, &nft_token_id)?;

    info!(
        nft_token_id = %nft_token_id,
        treasury_box_id = %treasury_box_id,
        "Bootstrap complete"
    );

    Ok(TreasuryBoxSpec {
        nft_token_id,
        treasury_box_id,
        value_nanoerg: config.treasury_erg_nanoerg,
        genesis_tx_id: tx_id,
    })
}

// ---------------------------------------------------------------------------
// Provider Registration functions
// ---------------------------------------------------------------------------

/// Build and submit a provider registration transaction.
///
/// Mints a per-provider NFT (supply=1) and creates a Provider Box with
/// registers R4-R9 populated.
///
/// The NFT token ID = blake2b256(first_input_box_id), which is handled
/// automatically by the Ergo node wallet API.
///
/// Returns the `ProviderRegistrationResult` with tx ID, NFT ID, and box ID.
pub async fn build_register_provider_tx(
    client: &ErgoNodeClient,
    provider_info: &ProviderRegistrationInfo,
) -> Result<ProviderRegistrationResult> {
    if provider_info.provider_pk_hex.is_empty() {
        bail!("provider_pk_hex is required for registration");
    }
    if provider_info.endpoint.is_empty() {
        bail!("endpoint is required for registration");
    }

    // Validate PK using centralized safety validator
    validate_pk_hex(&provider_info.provider_pk_hex, "provider_pk_hex")
        .context("Invalid provider public key for registration")?;
    let pk_bytes = hex::decode(&provider_info.provider_pk_hex)
        .context("Invalid provider PK hex")?;

    if provider_info.box_value_nanoerg < SAFE_MIN_BOX_VALUE {
        bail!(
            "box_value_nanoerg {} is below minimum {}",
            provider_info.box_value_nanoerg,
            SAFE_MIN_BOX_VALUE
        );
    }

    // Check wallet is ready
    let wallet_ready = client
        .wallet_status()
        .await
        .context("Failed to check wallet status")?;
    if !wallet_ready {
        bail!("Ergo node wallet is locked. Unlock it before provider registration.");
    }

    // Encode registers as Sigma constants
    // R4: GroupElement (provider PK) — stored as Coll[Byte] with 33-byte prefix
    let r4_pk = encode_coll_byte(&pk_bytes);

    // R5: Endpoint URL (Coll[Byte], UTF-8)
    let r5_endpoint = encode_string(&provider_info.endpoint);

    // R6: Models JSON (Coll[Byte], UTF-8)
    let r6_models = encode_string(&provider_info.models_json);

    // R7: PoNW score (Int)
    let r7_pown = encode_int(INITIAL_POWN_SCORE);

    // R8: Last heartbeat block height (Int)
    let r8_heartbeat = encode_int(INITIAL_HEARTBEAT);

    // R9: Region (Coll[Byte], UTF-8)
    let r9_region = encode_string(&provider_info.region);

    // Build the payment request
    let mut request_obj = serde_json::json!({
        "value": provider_info.box_value_nanoerg.to_string(),
        "assets": [{
            "amount": 1,
            "name": format!("XergonProvider-{}", provider_info.provider_id),
            "description": format!("Xergon Provider NFT: {}", provider_info.provider_id),
            "decimals": 0,
            "type": "EIP-004"
        }],
        "registers": {
            "R4": r4_pk,
            "R5": r5_endpoint,
            "R6": r6_models,
            "R7": r7_pown,
            "R8": r8_heartbeat,
            "R9": r9_region
        }
    });

    // Use compiled ErgoTree if available, otherwise use address
    if !provider_info.provider_tree_hex.is_empty() {
        request_obj["ergoTree"] =
            serde_json::Value::String(provider_info.provider_tree_hex.clone());
    } else {
        // We need an address — the provider's P2PK address derived from their PK
        // For now, this requires the caller to provide it via config or we use
        // a fallback. In practice, the compiled provider_box.es ErgoTree should
        // always be used.
        warn!("No provider_tree_hex provided; using PK-derived address. Compile provider_box.es for production.");
        // Store PK hex as address placeholder — the node will handle P2PK conversion
        // In practice, the caller should provide the P2S address
        request_obj["address"] =
            serde_json::Value::String(format!("pk_{}", &provider_info.provider_pk_hex));
    }

    let payment_request = serde_json::json!({
        "requests": [request_obj],
        "fee": (RECOMMENDED_MIN_FEE + 100_000).to_string()  // Slightly higher fee for register data
    });

    debug!(
        provider_id = %provider_info.provider_id,
        endpoint = %provider_info.endpoint,
        region = %provider_info.region,
        "Submitting provider registration transaction"
    );

    validate_payment_request(&payment_request)
        .context("Provider registration transaction safety validation failed")?;

    let tx_id = client
        .wallet_payment_send(&payment_request)
        .await
        .context("Failed to submit provider registration transaction via wallet")?;

    info!(
        tx_id = %tx_id,
        provider_id = %provider_info.provider_id,
        "Provider registration transaction submitted"
    );

    // Fetch the transaction to extract NFT token ID and Provider box ID
    let tx_detail = client
        .get_transaction(&tx_id)
        .await
        .context("Failed to fetch registration transaction details")?;

    let provider_nft_id = extract_nft_token_id(&tx_detail)?;
    let provider_box_id = extract_box_with_nft(&tx_detail, &provider_nft_id)?;

    info!(
        provider_id = %provider_info.provider_id,
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
// User Staking Box functions
// ---------------------------------------------------------------------------

/// Information needed to create a User Staking Box on-chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserStakingInfo {
    /// User's compressed secp256k1 public key (hex, 33 bytes).
    pub user_pk_hex: String,
    /// ERG value to lock in the Staking Box (must be >= SAFE_MIN_BOX_VALUE).
    pub amount_nanoerg: u64,
    /// Compiled ErgoTree hex for the user_staking.es contract.
    /// If empty, a P2PK address derived from the user PK is used as the guard.
    pub staking_tree_hex: String,
}

/// Result of a user staking box creation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserStakingResult {
    /// Transaction ID of the staking box creation transaction.
    pub tx_id: String,
    /// Box ID of the created User Staking Box.
    pub box_id: String,
    /// P2S address derived from the ErgoTree used for the staking box.
    pub staking_address: String,
}

/// Build and submit a User Staking Box creation transaction.
///
/// Creates an output box guarded by the user_staking.es contract (or a P2PK
/// fallback if no compiled ErgoTree is provided). The box has:
/// - R4: GroupElement — the user's compressed secp256k1 public key
/// - Value: `amount_nanoerg` ERG
///
/// Uses the Ergo node wallet API to build, sign, and broadcast.
///
/// Returns the `UserStakingResult` with tx ID, box ID, and staking address.
pub async fn build_staking_tx(
    client: &ErgoNodeClient,
    info: &UserStakingInfo,
) -> Result<UserStakingResult> {
    if info.user_pk_hex.is_empty() {
        bail!("user_pk_hex is required for staking box creation");
    }

    // Validate PK using centralized safety validator
    validate_pk_hex(&info.user_pk_hex, "user_pk_hex")
        .context("Invalid user public key for staking box")?;
    let pk_bytes = hex::decode(&info.user_pk_hex)
        .context("Invalid user PK hex")?;

    if info.amount_nanoerg < SAFE_MIN_BOX_VALUE {
        bail!(
            "amount_nanoerg {} is below minimum box value {}",
            info.amount_nanoerg,
            SAFE_MIN_BOX_VALUE
        );
    }

    // Check wallet is ready
    let wallet_ready = client
        .wallet_status()
        .await
        .context("Failed to check wallet status")?;
    if !wallet_ready {
        bail!("Ergo node wallet is locked. Unlock it before creating a staking box.");
    }

    // Encode R4 register as Sigma GroupElement: 0e 21 <33 bytes>
    let r4_pk = encode_group_element(&pk_bytes)?;

    // Build the payment request
    let mut request_obj = serde_json::json!({
        "value": info.amount_nanoerg.to_string(),
        "assets": [],
        "registers": {
            "R4": r4_pk
        }
    });

    // Use compiled ErgoTree if available, otherwise use P2PK address
    if !info.staking_tree_hex.is_empty() {
        request_obj["ergoTree"] =
            serde_json::Value::String(info.staking_tree_hex.clone());
    } else {
        // P2PK fallback: use the pk_ prefix format that the node understands
        warn!("No staking_tree_hex provided; using PK-derived address. Compile user_staking.es for production.");
        request_obj["address"] =
            serde_json::Value::String(format!("pk_{}", &info.user_pk_hex));
    }

    let payment_request = serde_json::json!({
        "requests": [request_obj],
        "fee": (RECOMMENDED_MIN_FEE + 100_000).to_string()  // Slightly higher fee for register data
    });

    debug!(
        user_pk_prefix = &info.user_pk_hex[..8.min(info.user_pk_hex.len())],
        amount_nanoerg = info.amount_nanoerg,
        "Submitting staking box creation transaction"
    );

    validate_payment_request(&payment_request)
        .context("Staking box creation transaction safety validation failed")?;

    let tx_id = client
        .wallet_payment_send(&payment_request)
        .await
        .context("Failed to submit staking box creation transaction via wallet")?;

    info!(tx_id = %tx_id, "Staking box creation transaction submitted");

    // Fetch the transaction to extract the staking box ID
    let tx_detail = client
        .get_transaction(&tx_id)
        .await
        .context("Failed to fetch staking box creation transaction details")?;

    // The staking box is the first output (no NFT to search for, so use first output)
    let staking_box_id = extract_first_output_box_id(&tx_detail)?;

    // Derive the staking address from the ErgoTree
    let staking_address = if !info.staking_tree_hex.is_empty() {
        derive_p2s_address(&info.staking_tree_hex)?
    } else {
        format!("pk_{}", &info.user_pk_hex)
    };

    info!(
        tx_id = %tx_id,
        staking_box_id = %staking_box_id,
        staking_address = %staking_address,
        "User staking box created"
    );

    Ok(UserStakingResult {
        tx_id,
        box_id: staking_box_id,
        staking_address,
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Extract the NFT token ID (first token with amount=1) from a transaction.
fn extract_nft_token_id(tx_detail: &serde_json::Value) -> Result<String> {
    let outputs = tx_detail
        .get("outputs")
        .and_then(|o| o.as_array())
        .context("No outputs in transaction")?;

    for output in outputs {
        if let Some(assets) = output.get("assets").and_then(|a| a.as_array()) {
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

    // Fallback: compute from first input box ID
    if let Some(inputs) = tx_detail.get("inputs").and_then(|i| i.as_array()) {
        if let Some(first_input) = inputs.first() {
            let box_id = first_input
                .get("boxId")
                .and_then(|b| b.as_str())
                .unwrap_or("");
            if !box_id.is_empty() {
                // blake2b256 of the box ID bytes
                let box_id_bytes = hex::decode(box_id)
                    .context("Failed to decode first input box ID as hex")?;
                type Blake2b256 = blake2::Blake2b<blake2::digest::consts::U32>;
                let mut hasher = Blake2b256::new();
                hasher.update(&box_id_bytes);
                let hash = hasher.finalize();
                return Ok(hex::encode(hash));
            }
        }
    }

    bail!("Could not extract NFT token ID from transaction")
}

/// Extract the first output box ID from a transaction.
fn extract_first_output_box_id(tx_detail: &serde_json::Value) -> Result<String> {
    let outputs = tx_detail
        .get("outputs")
        .and_then(|o| o.as_array())
        .context("No outputs in transaction")?;

    if let Some(first_output) = outputs.first() {
        let box_id = first_output
            .get("boxId")
            .and_then(|b| b.as_str())
            .unwrap_or("");
        if !box_id.is_empty() {
            return Ok(box_id.to_string());
        }
    }

    bail!("Could not extract first output box ID from transaction")
}

/// Derive a P2S address from an ErgoTree hex string.
///
/// Encodes the ErgoTree as a testnet P2S address (prefix 0x02) with a
/// BLAKE2b256 checksum, then base58-encodes the result.
fn derive_p2s_address(ergo_tree_hex: &str) -> Result<String> {
    let ergo_tree_bytes = hex::decode(ergo_tree_hex)
        .context("Failed to decode ErgoTree hex for address derivation")?;

    // Build the address bytes: network_prefix (0x02 for testnet) || ergo_tree || checksum
    let mut addr_bytes = vec![0x02u8]; // testnet prefix
    addr_bytes.extend_from_slice(&ergo_tree_bytes);

    // Compute BLAKE2b256 checksum of the prefix + content
    type Blake2b256 = blake2::Blake2b<blake2::digest::consts::U32>;
    let mut hasher = Blake2b256::new();
    hasher.update(&addr_bytes);
    let hash = hasher.finalize();
    addr_bytes.extend_from_slice(&hash[..4]); // first 4 bytes as checksum

    // Base58 encode
    let encoded = bs58::encode(&addr_bytes).into_string();
    Ok(encoded)
}

/// Extract the box ID of the output containing a specific NFT token.
fn extract_box_with_nft(
    tx_detail: &serde_json::Value,
    nft_token_id: &str,
) -> Result<String> {
    let outputs = tx_detail
        .get("outputs")
        .and_then(|o| o.as_array())
        .context("No outputs in transaction")?;

    for output in outputs {
        if let Some(assets) = output.get("assets").and_then(|a| a.as_array()) {
            for asset in assets {
                let token_id = asset
                    .get("tokenId")
                    .and_then(|t| t.as_str())
                    .unwrap_or("");
                if token_id == nft_token_id {
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

    // Fallback: return first output box ID
    if let Some(first_output) = outputs.first() {
        if let Some(box_id) = first_output.get("boxId").and_then(|b| b.as_str()) {
            warn!(
                "NFT token ID {} not found in outputs, using first output box ID: {}",
                nft_token_id, box_id
            );
            return Ok(box_id.to_string());
        }
    }

    bail!("Could not extract box ID from transaction outputs")
}

// ---------------------------------------------------------------------------
// Deployment state persistence
// ---------------------------------------------------------------------------

/// Deployment state that can be saved to disk for idempotency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapState {
    /// Xergon Network NFT token ID.
    pub nft_token_id: String,
    /// Treasury Box ID.
    pub treasury_box_id: String,
    /// Genesis transaction ID.
    pub genesis_tx_id: String,
    /// Deployer address.
    pub deployer_address: String,
    /// ERG locked in Treasury (nanoERG).
    pub treasury_erg_nanoerg: u64,
    /// Block height at deployment.
    pub block_height: i32,
    /// ISO 8601 timestamp of deployment.
    pub timestamp: String,
}

impl BootstrapState {
    /// Create from a TreasuryBoxSpec.
    pub fn from_spec(spec: &TreasuryBoxSpec, deployer_address: &str, block_height: i32) -> Self {
        Self {
            nft_token_id: spec.nft_token_id.clone(),
            treasury_box_id: spec.treasury_box_id.clone(),
            genesis_tx_id: spec.genesis_tx_id.clone(),
            deployer_address: deployer_address.to_string(),
            treasury_erg_nanoerg: spec.value_nanoerg,
            block_height,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Save deployment state to a JSON file.
    pub fn save_to_file(&self, path: &std::path::Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)
            .with_context(|| format!("Failed to write bootstrap state to {}", path.display()))?;
        Ok(())
    }

    /// Load deployment state from a JSON file.
    pub fn load_from_file(path: &std::path::Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read bootstrap state from {}", path.display()))?;
        let state: BootstrapState = serde_json::from_str(&content)
            .context("Failed to parse bootstrap state JSON")?;
        Ok(Some(state))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bootstrap_config_default() {
        let config = BootstrapConfig::default();
        assert_eq!(config.treasury_erg_nanoerg, DEFAULT_TREASURY_ERG);
        assert_eq!(config.nft_name, "XergonNetworkNFT");
        assert_eq!(config.nft_decimals, 0);
    }

    #[test]
    fn test_provider_registration_info_default() {
        let info = ProviderRegistrationInfo::default();
        assert_eq!(info.models_json, "[]");
        assert_eq!(info.region, "unknown");
        assert_eq!(info.box_value_nanoerg, SAFE_MIN_BOX_VALUE);
    }

    #[test]
    fn test_constants() {
        assert_eq!(SAFE_MIN_BOX_VALUE, 1_000_000);
        assert_eq!(RECOMMENDED_MIN_FEE, 1_000_000);
        assert_eq!(DEFAULT_TREASURY_ERG, 1_000_000_000);
        assert_eq!(INITIAL_POWN_SCORE, 0);
        assert_eq!(INITIAL_HEARTBEAT, 0);
    }

    #[test]
    fn test_extract_nft_token_id_from_tx() {
        let tx_json = serde_json::json!({
            "outputs": [
                {
                    "boxId": "box1",
                    "assets": [
                        {"tokenId": "abc123", "amount": 1, "name": "XergonNetworkNFT"},
                        {"tokenId": "other", "amount": 100}
                    ]
                }
            ],
            "inputs": [
                {"boxId": "input1"}
            ]
        });

        let nft_id = extract_nft_token_id(&tx_json).unwrap();
        assert_eq!(nft_id, "abc123");
    }

    #[test]
    fn test_extract_nft_token_id_fallback_blake2b() {
        let tx_json = serde_json::json!({
            "outputs": [
                {
                    "boxId": "box1",
                    "assets": []
                }
            ],
            "inputs": [
                {"boxId": "aabbccdd"}
            ]
        });

        // Should fall back to blake2b256 of first input box ID
        let nft_id = extract_nft_token_id(&tx_json).unwrap();
        assert_eq!(nft_id.len(), 64); // 32 bytes = 64 hex chars

        // Verify it's a valid blake2b256 hash
        type Blake2b256 = blake2::Blake2b<blake2::digest::consts::U32>;
        let mut hasher = Blake2b256::new();
        hasher.update(&hex::decode("aabbccdd").unwrap());
        let expected = hasher.finalize();
        assert_eq!(nft_id, hex::encode(expected));
    }

    #[test]
    fn test_extract_box_with_nft() {
        let tx_json = serde_json::json!({
            "outputs": [
                {
                    "boxId": "treasury_box_123",
                    "assets": [
                        {"tokenId": "nft_abc", "amount": 1}
                    ]
                },
                {
                    "boxId": "change_box",
                    "assets": []
                }
            ]
        });

        let box_id = extract_box_with_nft(&tx_json, "nft_abc").unwrap();
        assert_eq!(box_id, "treasury_box_123");
    }

    #[test]
    fn test_extract_box_with_nft_not_found() {
        let tx_json = serde_json::json!({
            "outputs": [
                {
                    "boxId": "change_box",
                    "assets": []
                }
            ]
        });

        let result = extract_box_with_nft(&tx_json, "nonexistent");
        // Should fall back to first output
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "change_box");
    }

    #[test]
    fn test_treasury_box_spec_serialization() {
        let spec = TreasuryBoxSpec {
            nft_token_id: "nft_abc".to_string(),
            treasury_box_id: "box_xyz".to_string(),
            value_nanoerg: 1_000_000_000,
            genesis_tx_id: "tx_123".to_string(),
        };

        let json = serde_json::to_string(&spec).unwrap();
        let parsed: TreasuryBoxSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.nft_token_id, "nft_abc");
        assert_eq!(parsed.treasury_box_id, "box_xyz");
    }

    #[test]
    fn test_bootstrap_state_serialization() {
        let state = BootstrapState {
            nft_token_id: "nft_abc".to_string(),
            treasury_box_id: "box_xyz".to_string(),
            genesis_tx_id: "tx_123".to_string(),
            deployer_address: "3WxsW...".to_string(),
            treasury_erg_nanoerg: 1_000_000_000,
            block_height: 500_000,
            timestamp: "2026-01-01T00:00:00+00:00".to_string(),
        };

        let json = serde_json::to_string_pretty(&state).unwrap();
        assert!(json.contains("nft_abc"));
        assert!(json.contains("box_xyz"));

        let parsed: BootstrapState = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.block_height, 500_000);
    }

    #[test]
    fn test_bootstrap_state_from_spec() {
        let spec = TreasuryBoxSpec {
            nft_token_id: "nft_001".to_string(),
            treasury_box_id: "box_001".to_string(),
            value_nanoerg: 5_000_000_000,
            genesis_tx_id: "tx_genesis".to_string(),
        };

        let state = BootstrapState::from_spec(&spec, "deployer_addr", 123456);
        assert_eq!(state.nft_token_id, "nft_001");
        assert_eq!(state.deployer_address, "deployer_addr");
        assert_eq!(state.block_height, 123456);
        assert_eq!(state.treasury_erg_nanoerg, 5_000_000_000);
    }

    #[test]
    fn test_extract_first_output_box_id() {
        let tx_json = serde_json::json!({
            "outputs": [
                {
                    "boxId": "staking_box_001",
                    "value": "1000000000",
                    "assets": []
                },
                {
                    "boxId": "change_box",
                    "assets": []
                }
            ]
        });

        let box_id = extract_first_output_box_id(&tx_json).unwrap();
        assert_eq!(box_id, "staking_box_001");
    }

    #[test]
    fn test_extract_first_output_box_id_no_outputs() {
        let tx_json = serde_json::json!({
            "outputs": []
        });

        let result = extract_first_output_box_id(&tx_json);
        assert!(result.is_err());
    }

    #[test]
    fn test_user_staking_info_serialization() {
        let info = UserStakingInfo {
            user_pk_hex: "02aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            amount_nanoerg: 10_000_000_000,
            staking_tree_hex: "10080402...".to_string(),
        };

        let json = serde_json::to_string(&info).unwrap();
        let parsed: UserStakingInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.user_pk_hex.len(), 66);
        assert_eq!(parsed.amount_nanoerg, 10_000_000_000);
    }

    #[test]
    fn test_user_staking_result_serialization() {
        let result = UserStakingResult {
            tx_id: "tx_staking".to_string(),
            box_id: "box_staking".to_string(),
            staking_address: "3WwxStaking".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: UserStakingResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tx_id, "tx_staking");
        assert_eq!(parsed.box_id, "box_staking");
        assert_eq!(parsed.staking_address, "3WwxStaking");
    }

    #[test]
    fn test_derive_p2s_address() {
        // Use a simple ErgoTree hex (P2PK)
        let ergo_tree_hex = "100804020e36100204a00b08cd03c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6a7b8010400050004000500040405000400040500040004050005000400050004000500040405040005000405040504040004050004000504040405000405040005000405040500040004040504040004040504";
        let addr = derive_p2s_address(ergo_tree_hex).unwrap();
        // Should be a base58 string
        assert!(!addr.is_empty());
        // Should start with '3' (testnet addresses start with 3 in base58)
        // Actually, the prefix byte determines this; 0x02 testnet prefix
        // results in addresses that start with '3' in base58 for Ergo
        println!("Derived address: {}", addr);
    }
}
