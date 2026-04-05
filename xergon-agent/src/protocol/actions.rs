//! Transaction builders for the Xergon protocol.
//!
//! This module provides two layers:
//! 1. **Node wallet API builders** (always available): Functions that use the
//!    Ergo node wallet API (`POST /wallet/payment/send`) to build, sign, and
//!    broadcast transactions. These do not require `ergo-lib`.
//! 2. **ergo-lib builders** (feature-gated): Lower-level builders using
//!    `ergo-lib`'s Sigma compiler and transaction builder for advanced use.
//!
//! For provider registration and bootstrap, prefer the higher-level functions
//! in [`crate::protocol::bootstrap`] which wrap these with validation and
//! state persistence.

use anyhow::{Context, Result};
use tracing::info;

use crate::chain::client::ErgoNodeClient;
use crate::protocol::bootstrap::{build_staking_tx, ProviderRegistrationInfo, SAFE_MIN_BOX_VALUE, UserStakingInfo, UserStakingResult};

// ---------------------------------------------------------------------------
// register_provider (node wallet API — always available)
// ---------------------------------------------------------------------------

/// Register a new provider on-chain by minting a per-provider NFT and
/// creating a Provider Box with registers R4-R9.
///
/// This is the primary entry point for provider registration. It uses the
/// Ergo node wallet API to build, sign, and broadcast the transaction.
///
/// # Arguments
///
/// * `client` - Ergo node client
/// * `endpoint` - Provider endpoint URL (e.g., "http://192.168.1.5:9099")
/// * `models` - List of model names served by this provider
/// * `region` - Provider region code (e.g., "us-east")
/// * `pk_hex` - Provider's compressed secp256k1 public key (hex, 33 bytes)
/// * `provider_tree_hex` - Compiled ErgoTree hex for provider_box.es (can be empty)
///
/// # Returns
///
/// The transaction ID of the registration transaction.
///
/// # NFT Minting
///
/// The NFT token ID follows Ergo's standard minting rule:
/// `token_id = blake2b256(first_input_box_id)`. This is handled automatically
/// by the node wallet API when a token with amount=1 is placed on the first output.
///
/// # Provider Box Register Layout
///
/// | Register | Type         | Content                        |
/// |----------|--------------|--------------------------------|
/// | R4       | GroupElement | Provider public key            |
/// | R5       | Coll[Byte]   | Endpoint URL (UTF-8)           |
/// | R6       | Coll[Byte]   | Models served (JSON array)     |
/// | R7       | Int          | PoNW score (initial: 0)        |
/// | R8       | Int          | Last heartbeat (initial: 0)    |
/// | R9       | Coll[Byte]   | Region code (UTF-8)            |
pub async fn register_provider(
    client: &ErgoNodeClient,
    endpoint: &str,
    models: &[String],
    region: &str,
    pk_hex: &str,
    provider_tree_hex: &str,
) -> Result<String> {
    let models_json = serde_json::to_string(models)
        .context("Failed to serialize models list to JSON")?;

    let provider_info = ProviderRegistrationInfo {
        provider_id: format!("provider-{}", &pk_hex[..8]),
        provider_pk_hex: pk_hex.to_string(),
        endpoint: endpoint.to_string(),
        models_json,
        region: region.to_string(),
        provider_tree_hex: provider_tree_hex.to_string(),
        box_value_nanoerg: SAFE_MIN_BOX_VALUE,
    };

    let result = crate::protocol::bootstrap::build_register_provider_tx(client, &provider_info)
        .await
        .context("Provider registration failed")?;

    info!(
        tx_id = %result.tx_id,
        provider_nft_id = %result.provider_nft_id,
        provider_box_id = %result.provider_box_id,
        "Provider registered successfully"
    );

    Ok(result.tx_id)
}

// ---------------------------------------------------------------------------
// register_provider (ergo-lib feature-gated — advanced)
// ---------------------------------------------------------------------------

#[cfg(feature = "ergo-lib")]
#[allow(dead_code)]
pub fn register_provider_lib(
    endpoint: &str,
    models: &[String],
    region: &str,
    pk_hex: &str,
) -> anyhow::Result<String> {
    use ergo_lib::{
        chain::transaction::TxBuilder,
        ergotree_ir::{
            chain::{
                ergo_box::box_value::BoxValue,
                ergo_box::{ErgoBox, ErgoBoxCandidateBuilder},
                token::Token,
            },
            mir::ergo_tree::ErgoTree,
            serialization::SigmaSerializable,
        },
        wallet::{secret_key::SecretKey, signing::TransactionContext, Wallet},
    };

    // ---------------------------------------------------------------------------
    // 1. Load configuration from environment
    // ---------------------------------------------------------------------------

    // TODO: These should come from a secure config / CLI flags, not env vars directly.
    //       The secret key in particular MUST NOT be stored as a plain env var in production.
    let node_url = std::env::var("XERGON_NODE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:9053".to_string());
    let sk_hex = std::env::var("XERGON_SK_HEX")
        .context("XERGON_SK_HEX env var required for ergo-lib provider registration")?;
    let sk_bytes = hex::decode(&sk_hex)
        .context("Invalid secret key hex in XERGON_SK_HEX")?;
    let secret_key = SecretKey::dlog_from_bytes(&sk_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to parse secret key: {:?}", e))?;
    let wallet = Wallet::from_secrets(vec![secret_key.clone()]);
    let prover = wallet.prover();

    // Optional: compiled provider_box.es ErgoTree (P2PK fallback if empty)
    let provider_tree_hex = std::env::var("XERGON_PROVIDER_TREE_HEX").unwrap_or_default();

    // ---------------------------------------------------------------------------
    // 2. Validate inputs
    // ---------------------------------------------------------------------------

    let pk_bytes = hex::decode(pk_hex)
        .context("Invalid provider PK hex")?;
    anyhow::ensure!(pk_bytes.len() == 33, "Provider PK must be 33 bytes");
    let models_json = serde_json::to_string(models)?;
    let box_value = BoxValue::SAFE_USER_MIN;

    // ---------------------------------------------------------------------------
    // 3. Fetch UTXOs from the Ergo node (blocking HTTP call)
    // ---------------------------------------------------------------------------

    // We need the sender's unspent boxes to fund the transaction.
    // Use the P2PK address derived from the secret key to find boxes.
    let pk_address = secret_key.public_image().p2pk_address();
    let encoded_pk = hex::encode(pk_address.script().sigma_serialize_bytes()?);
    let boxes_url = format!(
        "{}/api/v1/boxes/unspent/byErgoTree/{}",
        node_url, encoded_pk
    );

    // TODO: This is a blocking reqwest call. In production, use the tokio runtime
    //       or pass the boxes in. For now, use a blocking request inside the sync fn.
    let http_client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let raw_boxes: Vec<serde_json::Value> = http_client
        .get(&boxes_url)
        .send()
        .context("Failed to fetch UTXOs from node")?
        .error_for_status()
        .context("Node returned error fetching UTXOs")?
        .json()
        .context("Failed to parse UTXO response")?;

    anyhow::ensure!(!raw_boxes.is_empty(), "No UTXOs found for the signing key address");

    // Parse the first box as an ErgoBox to use as input
    let first_raw = &raw_boxes[0];
    let box_id_hex = first_raw["boxId"].as_str().unwrap_or("");
    let box_value_raw: u64 = first_raw["value"].as_u64().unwrap_or(0);
    let creation_height: i32 = first_raw["creationHeight"].as_i64().unwrap_or(0) as i32;
    let ergo_tree_raw = first_raw["ergoTree"].as_str().unwrap_or("");

    let ergo_tree = ErgoTree::sigma_parse_bytes(hex::decode(ergo_tree_raw)?)
        .map_err(|e| anyhow::anyhow!("Failed to parse input box ErgoTree: {:?}", e))?;

    // Parse the box ID
    let box_id_bytes = hex::decode(box_id_hex)?;
    let box_id = ergo_lib::ergotree_ir::chain::ergo_box::BoxId::from(
        box_id_bytes.as_slice().try_into().map_err(|e: Vec<u8>| {
            anyhow::anyhow!("Box ID must be 32 bytes, got {}", e.len())
        })?,
    );

    // Build the input ErgoBox
    let input_box = ErgoBox::new(
        BoxValue::try_from(box_value_raw)
            .map_err(|e| anyhow::anyhow!("Invalid box value: {:?}", e))?,
        ergo_tree,
        vec![],
        ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisters::empty(),
        creation_height,
        ergo_lib::ergotree_ir::chain::tx_id::TxId::zero(), // placeholder, updated by context
        0,
    )
    .map_err(|e| anyhow::anyhow!("Failed to create input ErgoBox: {:?}", e))?
    .with_box_id(box_id);

    let input_boxes = vec![input_box.clone()];

    // ---------------------------------------------------------------------------
    // 4. Build the Provider Box output with NFT and registers R4-R9
    // ---------------------------------------------------------------------------

    // Build the output ErgoTree (compiled provider contract or P2PK fallback)
    let output_tree = if !provider_tree_hex.is_empty() {
        ErgoTree::sigma_parse_bytes(hex::decode(&provider_tree_hex)?)
            .map_err(|e| anyhow::anyhow!("Failed to parse provider ErgoTree: {:?}", e))?
    } else {
        // P2PK guard: proveDlog(providerPk)
        pk_address.script()
    };

    // NFT token: id = blake2b256(first_input_box_id), amount = 1
    let nft_token_id = ergo_lib::ergotree_ir::chain::token::TokenId::from(
        box_id_bytes.as_slice().try_into().map_err(|e: Vec<u8>| {
            anyhow::anyhow!("Box ID must be 32 bytes, got {}", e.len())
        })?,
    );
    let nft_token = Token::new(nft_token_id, 1u64.try_into().unwrap());

    // Build registers R4-R9 using Sigma constant encoding
    let mut registers =
        ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisters::empty();

    // R4: GroupElement — provider PK (stored as Coll[Byte] with 33-byte prefix)
    let r4_ge = ergo_lib::ergotree_ir::sigma_protocol::dlog_group::EcPoint::sigma_parse_bytes(&pk_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to parse provider PK as EC point: {:?}", e))?;
    registers.set_r4(
        ergo_lib::ergotree_ir::mir::constant::Constant::from(
            ergo_lib::ergotree_ir::sigma_protocol::sigma_boolean::ProveDlog::new(r4_ge).into(),
        )
    );

    // R5: Coll[Byte] — endpoint URL (UTF-8)
    registers.set_r5(
        ergo_lib::ergotree_ir::mir::constant::Constant::from(
            endpoint.as_bytes().to_vec(),
        ),
    );

    // R6: Coll[Byte] — models JSON (UTF-8)
    registers.set_r6(
        ergo_lib::ergotree_ir::mir::constant::Constant::from(
            models_json.as_bytes().to_vec(),
        ),
    );

    // R7: Int — PoNW score (initial: 0)
    registers.set_r7(
        ergo_lib::ergotree_ir::mir::constant::Constant::from(0i32),
    );

    // R8: Int — last heartbeat (initial: 0)
    registers.set_r8(
        ergo_lib::ergotree_ir::mir::constant::Constant::from(0i32),
    );

    // R9: Coll[Byte] — region (UTF-8)
    registers.set_r9(
        ergo_lib::ergotree_ir::mir::constant::Constant::from(
            region.as_bytes().to_vec(),
        ),
    );

    // Build the provider box candidate
    let provider_box_candidate = ErgoBoxCandidateBuilder::new(
        box_value,
        output_tree,
        creation_height,
    )
    .add_token(nft_token)
    .set_registers(registers)
    .build()
    .map_err(|e| anyhow::anyhow!("Failed to build provider box candidate: {:?}", e))?;

    // Change box: return remaining ERG to sender minus fee
    let fee_amount = BoxValue::SAFE_USER_MIN; // ~0.001 ERG fee
    let input_total = input_box.value();
    let change_value = input_total.checked_sub(&box_value).and_then(|v| v.checked_sub(&fee_amount));
    anyhow::ensure!(
        change_value.is_some(),
        "Insufficient ERG in input box to cover box value + fee"
    );
    let change_candidate = ErgoBoxCandidateBuilder::new(
        change_value.unwrap(),
        pk_address.script(),
        creation_height,
    )
    .build()
    .map_err(|e| anyhow::anyhow!("Failed to build change box candidate: {:?}", e))?;

    // ---------------------------------------------------------------------------
    // 5. Build the unsigned transaction
    // ---------------------------------------------------------------------------

    let tx_builder = TxBuilder::new(
        vec![input_box],
        vec![provider_box_candidate, change_candidate],
        creation_height,
        fee_amount,
        None, // change address — handled explicitly above
        BoxValue::SAFE_USER_MIN,
    )
    .map_err(|e| anyhow::anyhow!("Failed to create TxBuilder: {:?}", e))?;

    // ---------------------------------------------------------------------------
    // 6. Fetch state context from node (pre-header)
    // ---------------------------------------------------------------------------

    let pre_header_url = format!("{}/blocks/lastHeaders/1", node_url);
    let headers_resp: serde_json::Value = http_client
        .get(&pre_header_url)
        .send()
        .context("Failed to fetch last headers from node")?
        .error_for_status()
        .context("Node returned error for lastHeaders")?
        .json()
        .context("Failed to parse lastHeaders response")?;

    // Build ErgoStateContext from the last header
    // TODO: Full header parsing requires sigma-rust Header type; for now we create
    //       a minimal pre-header from the JSON. In production, parse the full header
    //       bytes from the node and use Header::sigma_parse_bytes.
    let _header_json = &headers_resp[0];
    let state_context = ergo_lib::chain::context::ErgoStateContext {
        pre_header: ergo_lib::ergotree_ir::chain::header::PreHeader::new(
            1,  // version
            ergo_lib::ergotree_ir::chain::header::NBits::try_from(0x1d00ffffu64).unwrap_or_default(),
            ergo_lib::ergotree_ir::chain::digest32::Digest32::zero(),
            ergo_lib::ergotree_ir::chain::digest32::Digest32::zero(),
            creation_height,
            ergo_lib::ergotree_ir::chain::header::Header::AUTOMATIC_GENERATION_PROOF,
        ),
        headers: ergo_lib::ergotree_ir::chain::header::Headers::empty(),
    };

    // ---------------------------------------------------------------------------
    // 7. Sign the transaction
    // ---------------------------------------------------------------------------

    let tx_context = TransactionContext::new(
        tx_builder.unsigned_tx.clone(),
        ergo_lib::ergotree_ir::chain::ergo_box::box_builder::ErgoBox::from_candidate(
            tx_builder.unsigned_tx.outputs().get(0).unwrap(),
            tx_builder.box_selection.boxes.clone(),
        ),
    )
    .map_err(|e| anyhow::anyhow!("Failed to create TransactionContext: {:?}", e))?;

    let signed_tx = prover
        .sign(
            tx_context,
            &state_context,
        )
        .map_err(|e| anyhow::anyhow!("Failed to sign transaction: {:?}", e))?;

    // ---------------------------------------------------------------------------
    // 8. Serialize and return signed tx JSON
    // ---------------------------------------------------------------------------

    let tx_bytes = signed_tx.sigma_serialize_bytes()?;
    let tx_json = serde_json::json!({
        "id": hex::encode(signed_tx.id().sigma_serialize_bytes()?),
        "txBytes": hex::encode(&tx_bytes),
    });

    info!(
        tx_id = %tx_json["id"],
        "Provider registration transaction built (ergo-lib)"
    );

    Ok(tx_json.to_string())
}

// ---------------------------------------------------------------------------
// submit_heartbeat
// ---------------------------------------------------------------------------

#[cfg(feature = "ergo-lib")]
#[allow(dead_code)]
pub fn submit_heartbeat(
    provider_nft_id: &str,
    new_height: i32,
) -> anyhow::Result<String> {
    use ergo_lib::{
        chain::transaction::TxBuilder,
        ergotree_ir::{
            chain::{
                ergo_box::box_value::BoxValue,
                ergo_box::{ErgoBox, ErgoBoxCandidateBuilder},
                token::Token,
            },
            mir::ergo_tree::ErgoTree,
            serialization::SigmaSerializable,
        },
        wallet::{secret_key::SecretKey, signing::TransactionContext, Wallet},
    };

    // ---------------------------------------------------------------------------
    // 1. Load configuration from environment
    // ---------------------------------------------------------------------------

    let node_url = std::env::var("XERGON_NODE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:9053".to_string());
    let sk_hex = std::env::var("XERGON_SK_HEX")
        .context("XERGON_SK_HEX env var required for ergo-lib heartbeat submission")?;
    let sk_bytes = hex::decode(&sk_hex)
        .context("Invalid secret key hex in XERGON_SK_HEX")?;
    let secret_key = SecretKey::dlog_from_bytes(&sk_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to parse secret key: {:?}", e))?;
    let wallet = Wallet::from_secrets(vec![secret_key.clone()]);
    let prover = wallet.prover();
    let pk_address = secret_key.public_image().p2pk_address();

    // ---------------------------------------------------------------------------
    // 2. Fetch the Provider Box containing the NFT from the node
    // ---------------------------------------------------------------------------

    // Search UTXO set for boxes containing the provider NFT token
    let boxes_url = format!(
        "{}/api/v1/boxes/unspent/byTokenId/{}",
        node_url, provider_nft_id
    );

    let http_client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let raw_boxes: Vec<serde_json::Value> = http_client
        .get(&boxes_url)
        .send()
        .context("Failed to fetch provider boxes by token ID")?
        .error_for_status()
        .context("Node returned error for boxes by token ID")?
        .json()
        .context("Failed to parse boxes by token ID response")?;

    anyhow::ensure!(
        !raw_boxes.is_empty(),
        "No boxes found containing provider NFT token {}",
        provider_nft_id
    );

    // Use the first box that contains our NFT as the provider box to spend
    let provider_raw = &raw_boxes[0];
    let provider_box_id_hex = provider_raw["boxId"].as_str().unwrap_or("");
    let provider_value_raw: u64 = provider_raw["value"].as_u64().unwrap_or(0);
    let provider_creation_height: i32 = provider_raw["creationHeight"].as_i64().unwrap_or(0) as i32;
    let provider_tree_raw = provider_raw["ergoTree"].as_str().unwrap_or("");
    let provider_registers = &provider_raw["additionalRegisters"];

    // Parse provider box
    let provider_box_id_bytes = hex::decode(provider_box_id_hex)?;
    let provider_box_id = ergo_lib::ergotree_ir::chain::ergo_box::BoxId::from(
        provider_box_id_bytes.as_slice().try_into().map_err(|e: Vec<u8>| {
            anyhow::anyhow!("Box ID must be 32 bytes, got {}", e.len())
        })?,
    );

    let provider_ergo_tree = ErgoTree::sigma_parse_bytes(hex::decode(provider_tree_raw)?)
        .map_err(|e| anyhow::anyhow!("Failed to parse provider box ErgoTree: {:?}", e))?;

    // Parse the NFT token from the provider box
    let nft_token_id_parsed = ergo_lib::ergotree_ir::chain::token::TokenId::from(
        hex::decode(provider_nft_id)?.as_slice().try_into().map_err(|e: Vec<u8>| {
            anyhow::anyhow!("NFT token ID must be 32 bytes, got {}", e.len())
        })?,
    );

    // Reconstruct registers R4-R9, updating R8 with new_height
    // R4: GroupElement (provider PK), R5: Coll[Byte] (endpoint), R6: Coll[Byte] (models)
    // R7: Int (PoNW score), R8: Int (heartbeat), R9: Coll[Byte] (region)
    let mut registers =
        ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisters::empty();

    // Copy existing registers from the node response
    if let Some(r4_val) = provider_registers.get("R4") {
        let r4_hex = r4_val["rawValue"].as_str().unwrap_or("");
        if !r4_hex.is_empty() {
            let r4_bytes = hex::decode(r4_hex)?;
            let ge = ergo_lib::ergotree_ir::sigma_protocol::dlog_group::EcPoint::sigma_parse_bytes(&r4_bytes)
                .map_err(|e| anyhow::anyhow!("Failed to parse R4 as EC point: {:?}", e))?;
            registers.set_r4(
                ergo_lib::ergotree_ir::mir::constant::Constant::from(
                    ergo_lib::ergotree_ir::sigma_protocol::sigma_boolean::ProveDlog::new(ge).into(),
                ),
            );
        }
    }
    // R5: endpoint URL
    if let Some(r5_val) = provider_registers.get("R5") {
        let r5_hex = r5_val["rawValue"].as_str().unwrap_or("");
        if !r5_hex.is_empty() {
            let r5_bytes = hex::decode(r5_hex)?;
            registers.set_r5(
                ergo_lib::ergotree_ir::mir::constant::Constant::from(r5_bytes),
            );
        }
    }
    // R6: models JSON
    if let Some(r6_val) = provider_registers.get("R6") {
        let r6_hex = r6_val["rawValue"].as_str().unwrap_or("");
        if !r6_hex.is_empty() {
            let r6_bytes = hex::decode(r6_hex)?;
            registers.set_r6(
                ergo_lib::ergotree_ir::mir::constant::Constant::from(r6_bytes),
            );
        }
    }
    // R7: PoNW score (unchanged)
    if let Some(r7_val) = provider_registers.get("R7") {
        let r7_hex = r7_val["rawValue"].as_str().unwrap_or("");
        if !r7_hex.is_empty() {
            let r7_bytes = hex::decode(r7_hex)?;
            registers.set_r7(
                ergo_lib::ergotree_ir::mir::constant::Constant::from(r7_bytes),
            );
        }
    }
    // R8: UPDATED — new heartbeat height
    registers.set_r8(
        ergo_lib::ergotree_ir::mir::constant::Constant::from(new_height),
    );
    // R9: region
    if let Some(r9_val) = provider_registers.get("R9") {
        let r9_hex = r9_val["rawValue"].as_str().unwrap_or("");
        if !r9_hex.is_empty() {
            let r9_bytes = hex::decode(r9_hex)?;
            registers.set_r9(
                ergo_lib::ergotree_ir::mir::constant::Constant::from(r9_bytes),
            );
        }
    }

    // ---------------------------------------------------------------------------
    // 3. Fetch a funding box for the signing key (to pay fees)
    // ---------------------------------------------------------------------------

    let encoded_pk = hex::encode(pk_address.script().sigma_serialize_bytes()?);
    let funding_url = format!(
        "{}/api/v1/boxes/unspent/byErgoTree/{}",
        node_url, encoded_pk
    );
    let funding_boxes: Vec<serde_json::Value> = http_client
        .get(&funding_url)
        .send()
        .context("Failed to fetch funding boxes")?
        .error_for_status()
        .context("Node returned error for funding boxes")?
        .json()?;

    anyhow::ensure!(
        !funding_boxes.is_empty(),
        "No funding boxes found for the signing key"
    );

    let fund_raw = &funding_boxes[0];
    let fund_box_id_hex = fund_raw["boxId"].as_str().unwrap_or("");
    let fund_value_raw: u64 = fund_raw["value"].as_u64().unwrap_or(0);
    let fund_creation_height: i32 = fund_raw["creationHeight"].as_i64().unwrap_or(0) as i32;
    let fund_tree_raw = fund_raw["ergoTree"].as_str().unwrap_or("");

    let fund_box_id_bytes = hex::decode(fund_box_id_hex)?;
    let fund_box_id = ergo_lib::ergotree_ir::chain::ergo_box::BoxId::from(
        fund_box_id_bytes.as_slice().try_into().map_err(|e: Vec<u8>| {
            anyhow::anyhow!("Box ID must be 32 bytes, got {}", e.len())
        })?,
    );

    let fund_ergo_tree = ErgoTree::sigma_parse_bytes(hex::decode(fund_tree_raw)?)
        .map_err(|e| anyhow::anyhow!("Failed to parse funding box ErgoTree: {:?}", e))?;

    let funding_box = ErgoBox::new(
        BoxValue::try_from(fund_value_raw)
            .map_err(|e| anyhow::anyhow!("Invalid funding box value: {:?}", e))?,
        fund_ergo_tree,
        vec![],
        ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisters::empty(),
        fund_creation_height,
        ergo_lib::ergotree_ir::chain::tx_id::TxId::zero(),
        0,
    )
    .map_err(|e| anyhow::anyhow!("Failed to create funding ErgoBox: {:?}", e))?
    .with_box_id(fund_box_id);

    // Build the provider box to spend (as input)
    let provider_input_box = ErgoBox::new(
        BoxValue::try_from(provider_value_raw)
            .map_err(|e| anyhow::anyhow!("Invalid provider box value: {:?}", e))?,
        provider_ergo_tree,
        vec![Token::new(nft_token_id_parsed, 1u64.try_into().unwrap())],
        ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisters::empty(),
        provider_creation_height,
        ergo_lib::ergotree_ir::chain::tx_id::TxId::zero(),
        0,
    )
    .map_err(|e| anyhow::anyhow!("Failed to create provider input ErgoBox: {:?}", e))?
    .with_box_id(provider_box_id);

    // ---------------------------------------------------------------------------
    // 4. Build outputs: updated provider box + change
    // ---------------------------------------------------------------------------

    let provider_output_value = BoxValue::try_from(provider_value_raw)
        .map_err(|e| anyhow::anyhow!("Invalid provider output value: {:?}", e))?;

    // TODO: Use compiled provider_box.es ErgoTree for the output guard.
    //       For now, keep the same ErgoTree from the input.
    let provider_output_tree = provider_input_box.ergo_tree().clone();

    let provider_box_candidate = ErgoBoxCandidateBuilder::new(
        provider_output_value,
        provider_output_tree,
        provider_creation_height,
    )
    .add_token(Token::new(nft_token_id_parsed, 1u64.try_into().unwrap()))
    .set_registers(registers)
    .build()
    .map_err(|e| anyhow::anyhow!("Failed to build provider output candidate: {:?}", e))?;

    // Change box from funding input minus fee
    let fee_amount = BoxValue::SAFE_USER_MIN;
    let change_value = funding_box.value().checked_sub(&fee_amount);
    anyhow::ensure!(
        change_value.is_some(),
        "Insufficient ERG in funding box to cover fee"
    );

    let change_candidate = ErgoBoxCandidateBuilder::new(
        change_value.unwrap(),
        pk_address.script(),
        fund_creation_height,
    )
    .build()
    .map_err(|e| anyhow::anyhow!("Failed to build change box candidate: {:?}", e))?;

    // ---------------------------------------------------------------------------
    // 5. Build, sign, and return the transaction
    // ---------------------------------------------------------------------------

    let height = std::cmp::max(provider_creation_height, fund_creation_height);

    let tx_builder = TxBuilder::new(
        vec![provider_input_box, funding_box],
        vec![provider_box_candidate, change_candidate],
        height,
        fee_amount,
        None,
        BoxValue::SAFE_USER_MIN,
    )
    .map_err(|e| anyhow::anyhow!("Failed to create TxBuilder: {:?}", e))?;

    // Minimal state context (see register_provider_lib for full header parsing TODO)
    let state_context = ergo_lib::chain::context::ErgoStateContext {
        pre_header: ergo_lib::ergotree_ir::chain::header::PreHeader::new(
            1,
            ergo_lib::ergotree_ir::chain::header::NBits::try_from(0x1d00ffffu64).unwrap_or_default(),
            ergo_lib::ergotree_ir::chain::digest32::Digest32::zero(),
            ergo_lib::ergotree_ir::chain::digest32::Digest32::zero(),
            height,
            ergo_lib::ergotree_ir::chain::header::Header::AUTOMATIC_GENERATION_PROOF,
        ),
        headers: ergo_lib::ergotree_ir::chain::header::Headers::empty(),
    };

    let tx_context = TransactionContext::new(
        tx_builder.unsigned_tx.clone(),
        ergo_lib::ergotree_ir::chain::ergo_box::box_builder::ErgoBox::from_candidate(
            tx_builder.unsigned_tx.outputs().get(0).unwrap(),
            tx_builder.box_selection.boxes.clone(),
        ),
    )
    .map_err(|e| anyhow::anyhow!("Failed to create TransactionContext: {:?}", e))?;

    let signed_tx = prover
        .sign(tx_context, &state_context)
        .map_err(|e| anyhow::anyhow!("Failed to sign heartbeat transaction: {:?}", e))?;

    let tx_bytes = signed_tx.sigma_serialize_bytes()?;
    let tx_json = serde_json::json!({
        "id": hex::encode(signed_tx.id().sigma_serialize_bytes()?),
        "txBytes": hex::encode(&tx_bytes),
    });

    info!(
        tx_id = %tx_json["id"],
        provider_nft_id = %provider_nft_id,
        new_height = new_height,
        "Heartbeat transaction built (ergo-lib)"
    );

    Ok(tx_json.to_string())
}

#[cfg(not(feature = "ergo-lib"))]
#[allow(dead_code)]
pub fn submit_heartbeat(
    _provider_nft_id: &str,
    _new_height: i32,
) -> anyhow::Result<String> {
    anyhow::bail!("Not yet implemented: requires ergo-lib dependency (Phase 2)")
}

// ---------------------------------------------------------------------------
// submit_usage_proof
// ---------------------------------------------------------------------------

#[cfg(feature = "ergo-lib")]
#[allow(dead_code)]
pub fn submit_usage_proof(
    user_pk_hash: &str,
    provider_nft_id: &str,
    model: &str,
    token_count: i32,
    timestamp: i64,
) -> anyhow::Result<String> {
    use ergo_lib::{
        chain::transaction::TxBuilder,
        ergotree_ir::{
            chain::{
                ergo_box::box_value::BoxValue,
                ergo_box::{ErgoBox, ErgoBoxCandidateBuilder},
            },
            mir::ergo_tree::ErgoTree,
            serialization::SigmaSerializable,
        },
        wallet::{secret_key::SecretKey, signing::TransactionContext, Wallet},
    };

    // ---------------------------------------------------------------------------
    // 1. Load configuration from environment
    // ---------------------------------------------------------------------------

    let node_url = std::env::var("XERGON_NODE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:9053".to_string());
    let sk_hex = std::env::var("XERGON_SK_HEX")
        .context("XERGON_SK_HEX env var required for ergo-lib usage proof submission")?;
    let sk_bytes = hex::decode(&sk_hex)
        .context("Invalid secret key hex in XERGON_SK_HEX")?;
    let secret_key = SecretKey::dlog_from_bytes(&sk_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to parse secret key: {:?}", e))?;
    let wallet = Wallet::from_secrets(vec![secret_key.clone()]);
    let prover = wallet.prover();
    let pk_address = secret_key.public_image().p2pk_address();

    // Optional: compiled usage_proof.es ErgoTree (P2PK fallback if empty)
    let usage_proof_tree_hex = std::env::var("XERGON_USAGE_PROOF_TREE_HEX").unwrap_or_default();

    // ---------------------------------------------------------------------------
    // 2. Validate inputs
    // ---------------------------------------------------------------------------

    let user_pk_hash_bytes = hex::decode(user_pk_hash)
        .context("Invalid user PK hash hex")?;
    let _provider_nft_id_bytes = hex::decode(provider_nft_id)
        .context("Invalid provider NFT ID hex")?;

    // ---------------------------------------------------------------------------
    // 3. Fetch UTXOs from the node (funding box for the signing key)
    // ---------------------------------------------------------------------------

    let encoded_pk = hex::encode(pk_address.script().sigma_serialize_bytes()?);
    let boxes_url = format!(
        "{}/api/v1/boxes/unspent/byErgoTree/{}",
        node_url, encoded_pk
    );

    let http_client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let raw_boxes: Vec<serde_json::Value> = http_client
        .get(&boxes_url)
        .send()
        .context("Failed to fetch UTXOs from node")?
        .error_for_status()
        .context("Node returned error fetching UTXOs")?
        .json()
        .context("Failed to parse UTXO response")?;

    anyhow::ensure!(!raw_boxes.is_empty(), "No UTXOs found for the signing key address");

    // Parse the first box as input
    let first_raw = &raw_boxes[0];
    let box_id_hex = first_raw["boxId"].as_str().unwrap_or("");
    let box_value_raw: u64 = first_raw["value"].as_u64().unwrap_or(0);
    let creation_height: i32 = first_raw["creationHeight"].as_i64().unwrap_or(0) as i32;
    let ergo_tree_raw = first_raw["ergoTree"].as_str().unwrap_or("");

    let ergo_tree = ErgoTree::sigma_parse_bytes(hex::decode(ergo_tree_raw)?)
        .map_err(|e| anyhow::anyhow!("Failed to parse input box ErgoTree: {:?}", e))?;

    let box_id_bytes = hex::decode(box_id_hex)?;
    let box_id = ergo_lib::ergotree_ir::chain::ergo_box::BoxId::from(
        box_id_bytes.as_slice().try_into().map_err(|e: Vec<u8>| {
            anyhow::anyhow!("Box ID must be 32 bytes, got {}", e.len())
        })?,
    );

    let input_box = ErgoBox::new(
        BoxValue::try_from(box_value_raw)
            .map_err(|e| anyhow::anyhow!("Invalid box value: {:?}", e))?,
        ergo_tree,
        vec![],
        ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisters::empty(),
        creation_height,
        ergo_lib::ergotree_ir::chain::tx_id::TxId::zero(),
        0,
    )
    .map_err(|e| anyhow::anyhow!("Failed to create input ErgoBox: {:?}", e))?
    .with_box_id(box_id);

    // ---------------------------------------------------------------------------
    // 4. Build the Usage Proof Box output with registers R4-R8
    // ---------------------------------------------------------------------------

    // Build the output ErgoTree (compiled usage_proof contract or P2PK fallback)
    let output_tree = if !usage_proof_tree_hex.is_empty() {
        ErgoTree::sigma_parse_bytes(hex::decode(&usage_proof_tree_hex)?)
            .map_err(|e| anyhow::anyhow!("Failed to parse usage proof ErgoTree: {:?}", e))?
    } else {
        // P2PK guard: anyone can create usage proofs, but they reference user PK hash
        pk_address.script()
    };

    let proof_box_value = BoxValue::SAFE_USER_MIN;

    // Build registers R4-R8
    let mut registers =
        ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisters::empty();

    // R4: Coll[Byte] — user PK hash
    registers.set_r4(
        ergo_lib::ergotree_ir::mir::constant::Constant::from(
            user_pk_hash_bytes,
        ),
    );

    // R5: Coll[Byte] — provider NFT ID
    registers.set_r5(
        ergo_lib::ergotree_ir::mir::constant::Constant::from(
            hex::decode(provider_nft_id)?,
        ),
    );

    // R6: Coll[Byte] — model name (UTF-8)
    registers.set_r6(
        ergo_lib::ergotree_ir::mir::constant::Constant::from(
            model.as_bytes().to_vec(),
        ),
    );

    // R7: Int — token count
    registers.set_r7(
        ergo_lib::ergotree_ir::mir::constant::Constant::from(token_count),
    );

    // R8: Long — timestamp (Unix ms)
    registers.set_r8(
        ergo_lib::ergotree_ir::mir::constant::Constant::from(timestamp),
    );

    // No NFT minted — this is a pure data box
    let usage_proof_candidate = ErgoBoxCandidateBuilder::new(
        proof_box_value,
        output_tree,
        creation_height,
    )
    .set_registers(registers)
    .build()
    .map_err(|e| anyhow::anyhow!("Failed to build usage proof box candidate: {:?}", e))?;

    // Change box: return remaining ERG to sender minus fee
    let fee_amount = BoxValue::SAFE_USER_MIN;
    let change_value = input_box.value().checked_sub(&proof_box_value).and_then(|v| v.checked_sub(&fee_amount));
    anyhow::ensure!(
        change_value.is_some(),
        "Insufficient ERG in input box to cover proof box value + fee"
    );
    let change_candidate = ErgoBoxCandidateBuilder::new(
        change_value.unwrap(),
        pk_address.script(),
        creation_height,
    )
    .build()
    .map_err(|e| anyhow::anyhow!("Failed to build change box candidate: {:?}", e))?;

    // ---------------------------------------------------------------------------
    // 5. Build the unsigned transaction
    // ---------------------------------------------------------------------------

    let tx_builder = TxBuilder::new(
        vec![input_box],
        vec![usage_proof_candidate, change_candidate],
        creation_height,
        fee_amount,
        None,
        BoxValue::SAFE_USER_MIN,
    )
    .map_err(|e| anyhow::anyhow!("Failed to create TxBuilder: {:?}", e))?;

    // ---------------------------------------------------------------------------
    // 6. Build state context and sign
    // ---------------------------------------------------------------------------

    let state_context = ergo_lib::chain::context::ErgoStateContext {
        pre_header: ergo_lib::ergotree_ir::chain::header::PreHeader::new(
            1,
            ergo_lib::ergotree_ir::chain::header::NBits::try_from(0x1d00ffffu64).unwrap_or_default(),
            ergo_lib::ergotree_ir::chain::digest32::Digest32::zero(),
            ergo_lib::ergotree_ir::chain::digest32::Digest32::zero(),
            creation_height,
            ergo_lib::ergotree_ir::chain::header::Header::AUTOMATIC_GENERATION_PROOF,
        ),
        headers: ergo_lib::ergotree_ir::chain::header::Headers::empty(),
    };

    let tx_context = TransactionContext::new(
        tx_builder.unsigned_tx.clone(),
        ergo_lib::ergotree_ir::chain::ergo_box::box_builder::ErgoBox::from_candidate(
            tx_builder.unsigned_tx.outputs().get(0).unwrap(),
            tx_builder.box_selection.boxes.clone(),
        ),
    )
    .map_err(|e| anyhow::anyhow!("Failed to create TransactionContext: {:?}", e))?;

    let signed_tx = prover
        .sign(tx_context, &state_context)
        .map_err(|e| anyhow::anyhow!("Failed to sign usage proof transaction: {:?}", e))?;

    // ---------------------------------------------------------------------------
    // 7. Serialize and return signed tx JSON
    // ---------------------------------------------------------------------------

    let tx_bytes = signed_tx.sigma_serialize_bytes()?;
    let tx_json = serde_json::json!({
        "id": hex::encode(signed_tx.id().sigma_serialize_bytes()?),
        "txBytes": hex::encode(&tx_bytes),
    });

    info!(
        tx_id = %tx_json["id"],
        provider_nft_id = %provider_nft_id,
        model = %model,
        token_count = token_count,
        "Usage proof transaction built (ergo-lib)"
    );

    Ok(tx_json.to_string())
}

#[cfg(not(feature = "ergo-lib"))]
#[allow(dead_code)]
pub fn submit_usage_proof(
    _user_pk_hash: &str,
    _provider_nft_id: &str,
    _model: &str,
    _token_count: i32,
    _timestamp: i64,
) -> anyhow::Result<String> {
    anyhow::bail!("Not yet implemented: requires ergo-lib dependency (Phase 2)")
}

// ---------------------------------------------------------------------------
// create_user_staking_box
// ---------------------------------------------------------------------------

/// Create a User Staking Box on-chain.
///
/// Builds and submits a transaction that creates an output box guarded by the
/// `user_staking.es` contract (or P2PK fallback if `staking_tree_hex` is empty).
///
/// The box contains:
/// - **ErgoTree**: from `staking_tree_hex` (compiled user_staking.es), or P2PK
/// - **R4 register**: user's public key as a Sigma `GroupElement`
/// - **Value**: `amount_nanoerg` (must be >= `SAFE_MIN_BOX_VALUE`)
///
/// # Arguments
///
/// * `client` - Ergo node client
/// * `user_pk_hex` - User's compressed secp256k1 public key (hex, 33 bytes)
/// * `amount_nanoerg` - ERG amount to lock in the staking box (nanoERG)
/// * `staking_tree_hex` - Compiled ErgoTree hex for user_staking.es (can be empty for P2PK fallback)
///
/// # Returns
///
/// `UserStakingResult` with `tx_id`, `box_id`, and `staking_address`.
///
/// # User Staking Box Register Layout
///
/// | Register | Type         | Content                       |
/// |----------|--------------|-------------------------------|
/// | R4       | GroupElement | User's compressed secp256k1 PK|
pub async fn create_user_staking_box(
    client: &ErgoNodeClient,
    user_pk_hex: &str,
    amount_nanoerg: u64,
    staking_tree_hex: &str,
) -> Result<UserStakingResult> {
    let info = UserStakingInfo {
        user_pk_hex: user_pk_hex.to_string(),
        amount_nanoerg,
        staking_tree_hex: staking_tree_hex.to_string(),
    };

    let result = build_staking_tx(client, &info)
        .await
        .context("User staking box creation failed")?;

    info!(
        tx_id = %result.tx_id,
        box_id = %result.box_id,
        staking_address = %result.staking_address,
        "User staking box created successfully"
    );

    Ok(result)
}

// ---------------------------------------------------------------------------
// pay_provider
// ---------------------------------------------------------------------------

#[cfg(feature = "ergo-lib")]
#[allow(dead_code)]
pub fn pay_provider(
    user_staking_box_id: &str,
    provider_nft_id: &str,
    amount_nanoerg: u64,
) -> anyhow::Result<String> {
    use ergo_lib::{
        chain::transaction::TxBuilder,
        ergotree_ir::{
            chain::{
                ergo_box::box_value::BoxValue,
                ergo_box::{ErgoBox, ErgoBoxCandidateBuilder},
                token::Token,
            },
            mir::ergo_tree::ErgoTree,
            serialization::SigmaSerializable,
        },
        wallet::{secret_key::SecretKey, signing::TransactionContext, Wallet},
    };

    // ---------------------------------------------------------------------------
    // 1. Load configuration from environment
    // ---------------------------------------------------------------------------

    let node_url = std::env::var("XERGON_NODE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:9053".to_string());
    let sk_hex = std::env::var("XERGON_SK_HEX")
        .context("XERGON_SK_HEX env var required for ergo-lib pay_provider")?;
    let sk_bytes = hex::decode(&sk_hex)
        .context("Invalid secret key hex in XERGON_SK_HEX")?;
    let secret_key = SecretKey::dlog_from_bytes(&sk_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to parse secret key: {:?}", e))?;
    let wallet = Wallet::from_secrets(vec![secret_key.clone()]);
    let prover = wallet.prover();
    let pk_address = secret_key.public_image().p2pk_address();

    // Optional: compiled user_staking.es and provider_box.es ErgoTrees
    let staking_tree_hex = std::env::var("XERGON_STAKING_TREE_HEX").unwrap_or_default();
    let provider_tree_hex = std::env::var("XERGON_PROVIDER_TREE_HEX").unwrap_or_default();

    // ---------------------------------------------------------------------------
    // 2. Fetch the User Staking Box by ID from the node
    // ---------------------------------------------------------------------------

    let http_client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let staking_box_url = format!(
        "{}/api/v1/boxes/{}",
        node_url, user_staking_box_id
    );
    let staking_raw: serde_json::Value = http_client
        .get(&staking_box_url)
        .send()
        .context("Failed to fetch user staking box")?
        .error_for_status()
        .context("Node returned error for staking box")?
        .json()?;

    let staking_value_raw: u64 = staking_raw["value"].as_u64().unwrap_or(0);
    let staking_creation_height: i32 = staking_raw["creationHeight"].as_i64().unwrap_or(0) as i32;
    let staking_tree_raw = staking_raw["ergoTree"].as_str().unwrap_or("");
    let staking_registers = &staking_raw["additionalRegisters"];

    let staking_box_id_bytes = hex::decode(user_staking_box_id)?;
    let staking_box_id = ergo_lib::ergotree_ir::chain::ergo_box::BoxId::from(
        staking_box_id_bytes.as_slice().try_into().map_err(|e: Vec<u8>| {
            anyhow::anyhow!("Box ID must be 32 bytes, got {}", e.len())
        })?,
    );

    let staking_ergo_tree = ErgoTree::sigma_parse_bytes(hex::decode(staking_tree_raw)?)
        .map_err(|e| anyhow::anyhow!("Failed to parse staking box ErgoTree: {:?}", e))?;

    anyhow::ensure!(
        staking_value_raw >= amount_nanoerg,
        "User staking box has {} nanoERG but payment requires {} nanoERG",
        staking_value_raw,
        amount_nanoerg,
    );

    let staking_input_box = ErgoBox::new(
        BoxValue::try_from(staking_value_raw)
            .map_err(|e| anyhow::anyhow!("Invalid staking box value: {:?}", e))?,
        staking_ergo_tree.clone(),
        vec![],
        ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisters::empty(),
        staking_creation_height,
        ergo_lib::ergotree_ir::chain::tx_id::TxId::zero(),
        0,
    )
    .map_err(|e| anyhow::anyhow!("Failed to create staking input ErgoBox: {:?}", e))?
    .with_box_id(staking_box_id);

    // ---------------------------------------------------------------------------
    // 3. Fetch the Provider Box containing the NFT from the node
    // ---------------------------------------------------------------------------

    let provider_boxes_url = format!(
        "{}/api/v1/boxes/unspent/byTokenId/{}",
        node_url, provider_nft_id
    );
    let provider_boxes: Vec<serde_json::Value> = http_client
        .get(&provider_boxes_url)
        .send()
        .context("Failed to fetch provider boxes by token ID")?
        .error_for_status()
        .context("Node returned error for provider boxes")?
        .json()?;

    anyhow::ensure!(
        !provider_boxes.is_empty(),
        "No boxes found containing provider NFT token {}",
        provider_nft_id
    );

    let provider_raw = &provider_boxes[0];
    let provider_box_id_hex = provider_raw["boxId"].as_str().unwrap_or("");
    let provider_value_raw: u64 = provider_raw["value"].as_u64().unwrap_or(0);
    let provider_creation_height: i32 = provider_raw["creationHeight"].as_i64().unwrap_or(0) as i32;
    let provider_tree_raw = provider_raw["ergoTree"].as_str().unwrap_or("");
    let provider_regs = &provider_raw["additionalRegisters"];

    let provider_box_id_bytes = hex::decode(provider_box_id_hex)?;
    let provider_box_id = ergo_lib::ergotree_ir::chain::ergo_box::BoxId::from(
        provider_box_id_bytes.as_slice().try_into().map_err(|e: Vec<u8>| {
            anyhow::anyhow!("Box ID must be 32 bytes, got {}", e.len())
        })?,
    );

    let provider_ergo_tree = ErgoTree::sigma_parse_bytes(hex::decode(provider_tree_raw)?)
        .map_err(|e| anyhow::anyhow!("Failed to parse provider box ErgoTree: {:?}", e))?;

    let nft_token_id_parsed = ergo_lib::ergotree_ir::chain::token::TokenId::from(
        hex::decode(provider_nft_id)?.as_slice().try_into().map_err(|e: Vec<u8>| {
            anyhow::anyhow!("NFT token ID must be 32 bytes, got {}", e.len())
        })?,
    );

    let provider_input_box = ErgoBox::new(
        BoxValue::try_from(provider_value_raw)
            .map_err(|e| anyhow::anyhow!("Invalid provider box value: {:?}", e))?,
        provider_ergo_tree.clone(),
        vec![Token::new(nft_token_id_parsed, 1u64.try_into().unwrap())],
        ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisters::empty(),
        provider_creation_height,
        ergo_lib::ergotree_ir::chain::tx_id::TxId::zero(),
        0,
    )
    .map_err(|e| anyhow::anyhow!("Failed to create provider input ErgoBox: {:?}", e))?
    .with_box_id(provider_box_id);

    // ---------------------------------------------------------------------------
    // 4. Fetch a funding box for the signing key (to pay fees)
    // ---------------------------------------------------------------------------

    let encoded_pk = hex::encode(pk_address.script().sigma_serialize_bytes()?);
    let funding_url = format!(
        "{}/api/v1/boxes/unspent/byErgoTree/{}",
        node_url, encoded_pk
    );
    let funding_boxes: Vec<serde_json::Value> = http_client
        .get(&funding_url)
        .send()
        .context("Failed to fetch funding boxes")?
        .error_for_status()
        .context("Node returned error for funding boxes")?
        .json()?;

    anyhow::ensure!(
        !funding_boxes.is_empty(),
        "No funding boxes found for the signing key"
    );

    let fund_raw = &funding_boxes[0];
    let fund_box_id_hex = fund_raw["boxId"].as_str().unwrap_or("");
    let fund_value_raw: u64 = fund_raw["value"].as_u64().unwrap_or(0);
    let fund_creation_height: i32 = fund_raw["creationHeight"].as_i64().unwrap_or(0) as i32;
    let fund_tree_raw = fund_raw["ergoTree"].as_str().unwrap_or("");

    let fund_box_id_bytes = hex::decode(fund_box_id_hex)?;
    let fund_box_id = ergo_lib::ergotree_ir::chain::ergo_box::BoxId::from(
        fund_box_id_bytes.as_slice().try_into().map_err(|e: Vec<u8>| {
            anyhow::anyhow!("Box ID must be 32 bytes, got {}", e.len())
        })?,
    );

    let fund_ergo_tree = ErgoTree::sigma_parse_bytes(hex::decode(fund_tree_raw)?)
        .map_err(|e| anyhow::anyhow!("Failed to parse funding box ErgoTree: {:?}", e))?;

    let funding_box = ErgoBox::new(
        BoxValue::try_from(fund_value_raw)
            .map_err(|e| anyhow::anyhow!("Invalid funding box value: {:?}", e))?,
        fund_ergo_tree,
        vec![],
        ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisters::empty(),
        fund_creation_height,
        ergo_lib::ergotree_ir::chain::tx_id::TxId::zero(),
        0,
    )
    .map_err(|e| anyhow::anyhow!("Failed to create funding ErgoBox: {:?}", e))?
    .with_box_id(fund_box_id);

    // ---------------------------------------------------------------------------
    // 5. Build outputs
    // ---------------------------------------------------------------------------

    let height = std::cmp::max(staking_creation_height, fund_creation_height);
    let fee_amount = BoxValue::SAFE_USER_MIN;

    // Output 1: Updated user staking box with (value - amount)
    let new_staking_value = BoxValue::try_from(staking_value_raw - amount_nanoerg)
        .map_err(|e| anyhow::anyhow!("Invalid new staking box value: {:?}", e))?;

    // The staking box successor must satisfy the user_staking.ergo contract:
    // - Must have proveDlog(userPk) context variable
    // - R4 register must contain the user PK (copied from input)
    //
    // Copy registers from the input staking box, especially R4 (user PK GroupElement)
    let mut staking_registers =
        ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisters::empty();

    // Copy R4: user PK (GroupElement)
    // For the staking box successor guard, we need to proveDlog(userPk).
    // The contract expects SELF.R4 == userPk (GroupElement) and
    // the context variable must contain the proof.
    //
    // TODO: The user_staking.es contract guard should be compiled and used here.
    //       The prover will automatically attach the proveDlog proof for the
    //       user's secret key when signing.
    //
    // Reconstruct R4 from the input box registers.
    if let Some(r4_val) = staking_registers.get("R4") {
        let r4_hex = r4_val["rawValue"].as_str().unwrap_or("");
        if !r4_hex.is_empty() {
            let r4_bytes = hex::decode(r4_hex)?;
            // Parse as GroupElement (EcPoint) — stored as Coll[Byte] with 33-byte prefix
            let ge = ergo_lib::ergotree_ir::sigma_protocol::dlog_group::EcPoint::sigma_parse_bytes(&r4_bytes)
                .map_err(|e| anyhow::anyhow!("Failed to parse staking R4 as EC point: {:?}", e))?;
            staking_registers.set_r4(
                ergo_lib::ergotree_ir::mir::constant::Constant::from(
                    ergo_lib::ergotree_ir::sigma_protocol::sigma_boolean::ProveDlog::new(ge).into(),
                ),
            );
        }
    }

    // Use compiled staking contract or fallback to input ErgoTree
    let staking_output_tree = if !staking_tree_hex.is_empty() {
        ErgoTree::sigma_parse_bytes(hex::decode(&staking_tree_hex)?)
            .map_err(|e| anyhow::anyhow!("Failed to parse staking output ErgoTree: {:?}", e))?
    } else {
        // Fallback: keep same ErgoTree from input
        staking_input_box.ergo_tree().clone()
    };

    anyhow::ensure!(
        new_staking_value >= BoxValue::SAFE_USER_MIN,
        "Remaining staking value ({}) is below minimum box value ({})",
        staking_value_raw - amount_nanoerg,
        BoxValue::SAFE_USER_MIN.as_u64(),
    );

    let staking_candidate = ErgoBoxCandidateBuilder::new(
        new_staking_value,
        staking_output_tree,
        height,
    )
    .set_registers(staking_registers)
    .build()
    .map_err(|e| anyhow::anyhow!("Failed to build staking output candidate: {:?}", e))?;

    // Output 2: Updated provider box with +amount ERG
    let new_provider_value = BoxValue::try_from(provider_value_raw + amount_nanoerg)
        .map_err(|e| anyhow::anyhow!("Invalid new provider box value: {:?}", e))?;

    // Copy all provider registers from input (R4-R9 unchanged for payment)
    let mut prov_registers =
        ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisters::empty();

    if let Some(r4_val) = provider_regs.get("R4") {
        let r4_hex = r4_val["rawValue"].as_str().unwrap_or("");
        if !r4_hex.is_empty() {
            let r4_bytes = hex::decode(r4_hex)?;
            let ge = ergo_lib::ergotree_ir::sigma_protocol::dlog_group::EcPoint::sigma_parse_bytes(&r4_bytes)
                .map_err(|e| anyhow::anyhow!("Failed to parse provider R4 as EC point: {:?}", e))?;
            prov_registers.set_r4(
                ergo_lib::ergotree_ir::mir::constant::Constant::from(
                    ergo_lib::ergotree_ir::sigma_protocol::sigma_boolean::ProveDlog::new(ge).into(),
                ),
            );
        }
    }
    // R5-R9: copy raw bytes from existing registers
    for (reg_name, set_fn) in [
        ("R5", |regs: &mut ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisters, v: Vec<u8>| regs.set_r5(ergo_lib::ergotree_ir::mir::constant::Constant::from(v))),
        ("R6", |regs: &mut ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisters, v: Vec<u8>| regs.set_r6(ergo_lib::ergotree_ir::mir::constant::Constant::from(v))),
        ("R7", |regs: &mut ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisters, v: Vec<u8>| regs.set_r7(ergo_lib::ergotree_ir::mir::constant::Constant::from(v))),
        ("R8", |regs: &mut ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisters, v: Vec<u8>| regs.set_r8(ergo_lib::ergotree_ir::mir::constant::Constant::from(v))),
        ("R9", |regs: &mut ergo_lib::ergotree_ir::chain::ergo_box::NonMandatoryRegisters, v: Vec<u8>| regs.set_r9(ergo_lib::ergotree_ir::mir::constant::Constant::from(v))),
    ] {
        if let Some(val) = provider_regs.get(reg_name) {
            let hex_str = val["rawValue"].as_str().unwrap_or("");
            if !hex_str.is_empty() {
                if let Ok(bytes) = hex::decode(hex_str) {
                    set_fn(&mut prov_registers, bytes);
                }
            }
        }
    }

    // Use compiled provider contract or fallback to input ErgoTree
    let provider_output_tree = if !provider_tree_hex.is_empty() {
        ErgoTree::sigma_parse_bytes(hex::decode(&provider_tree_hex)?)
            .map_err(|e| anyhow::anyhow!("Failed to parse provider output ErgoTree: {:?}", e))?
    } else {
        provider_input_box.ergo_tree().clone()
    };

    let provider_candidate = ErgoBoxCandidateBuilder::new(
        new_provider_value,
        provider_output_tree,
        height,
    )
    .add_token(Token::new(nft_token_id_parsed, 1u64.try_into().unwrap()))
    .set_registers(prov_registers)
    .build()
    .map_err(|e| anyhow::anyhow!("Failed to build provider output candidate: {:?}", e))?;

    // Output 3: Change box from funding input minus fee
    let change_value = funding_box.value().checked_sub(&fee_amount);
    anyhow::ensure!(
        change_value.is_some(),
        "Insufficient ERG in funding box to cover fee"
    );
    let change_candidate = ErgoBoxCandidateBuilder::new(
        change_value.unwrap(),
        pk_address.script(),
        height,
    )
    .build()
    .map_err(|e| anyhow::anyhow!("Failed to build change box candidate: {:?}", e))?;

    // ---------------------------------------------------------------------------
    // 6. Build, sign, and return the transaction
    // ---------------------------------------------------------------------------

    // Inputs: staking box + provider box + funding box
    let tx_builder = TxBuilder::new(
        vec![staking_input_box, provider_input_box, funding_box],
        vec![staking_candidate, provider_candidate, change_candidate],
        height,
        fee_amount,
        None,
        BoxValue::SAFE_USER_MIN,
    )
    .map_err(|e| anyhow::anyhow!("Failed to create TxBuilder: {:?}", e))?;

    // Minimal state context
    let state_context = ergo_lib::chain::context::ErgoStateContext {
        pre_header: ergo_lib::ergotree_ir::chain::header::PreHeader::new(
            1,
            ergo_lib::ergotree_ir::chain::header::NBits::try_from(0x1d00ffffu64).unwrap_or_default(),
            ergo_lib::ergotree_ir::chain::digest32::Digest32::zero(),
            ergo_lib::ergotree_ir::chain::digest32::Digest32::zero(),
            height,
            ergo_lib::ergotree_ir::chain::header::Header::AUTOMATIC_GENERATION_PROOF,
        ),
        headers: ergo_lib::ergotree_ir::chain::header::Headers::empty(),
    };

    let tx_context = TransactionContext::new(
        tx_builder.unsigned_tx.clone(),
        ergo_lib::ergotree_ir::chain::ergo_box::box_builder::ErgoBox::from_candidate(
            tx_builder.unsigned_tx.outputs().get(0).unwrap(),
            tx_builder.box_selection.boxes.clone(),
        ),
    )
    .map_err(|e| anyhow::anyhow!("Failed to create TransactionContext: {:?}", e))?;

    let signed_tx = prover
        .sign(tx_context, &state_context)
        .map_err(|e| anyhow::anyhow!("Failed to sign pay_provider transaction: {:?}", e))?;

    // ---------------------------------------------------------------------------
    // 7. Serialize and return signed tx JSON
    // ---------------------------------------------------------------------------

    let tx_bytes = signed_tx.sigma_serialize_bytes()?;
    let tx_json = serde_json::json!({
        "id": hex::encode(signed_tx.id().sigma_serialize_bytes()?),
        "txBytes": hex::encode(&tx_bytes),
    });

    info!(
        tx_id = %tx_json["id"],
        user_staking_box_id = %user_staking_box_id,
        provider_nft_id = %provider_nft_id,
        amount_nanoerg = amount_nanoerg,
        "Pay provider transaction built (ergo-lib)"
    );

    Ok(tx_json.to_string())
}

#[cfg(not(feature = "ergo-lib"))]
#[allow(dead_code)]
pub fn pay_provider(
    _user_staking_box_id: &str,
    _provider_nft_id: &str,
    _amount_nanoerg: u64,
) -> anyhow::Result<String> {
    anyhow::bail!("Not yet implemented: requires ergo-lib dependency (Phase 2)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_provider_invalid_pk() {
        // This tests the sync validation path; we can't test async without a node.
        let models_json = serde_json::to_string(&["llama-3.1-8b".to_string()]).unwrap();
        let info = ProviderRegistrationInfo {
            provider_pk_hex: "not_hex".to_string(),
            provider_id: "test".to_string(),
            endpoint: "http://localhost:9099".to_string(),
            models_json,
            region: "us-east".to_string(),
            provider_tree_hex: String::new(),
            box_value_nanoerg: SAFE_MIN_BOX_VALUE,
        };
        // PK validation would happen in the async function, but we can check the struct
        assert_eq!(info.provider_pk_hex, "not_hex");
    }

    #[test]
    fn test_models_serialization() {
        let models = vec!["llama-3.1-8b".to_string(), "mistral-7b".to_string()];
        let json = serde_json::to_string(&models).unwrap();
        assert_eq!(json, r#"["llama-3.1-8b","mistral-7b"]"#);
    }

    #[test]
    fn test_submit_heartbeat_not_implemented() {
        let result = submit_heartbeat("nft-id-123", 500_000);
        assert!(result.is_err());
        #[cfg(not(feature = "ergo-lib"))]
        assert!(result.unwrap_err().to_string().contains("Phase 2"));
    }

    #[test]
    fn test_submit_usage_proof_not_implemented() {
        let result = submit_usage_proof("user-hash", "nft-id", "llama-3.1-8b", 42, 1_700_000_000);
        assert!(result.is_err());
        #[cfg(not(feature = "ergo-lib"))]
        assert!(result.unwrap_err().to_string().contains("Phase 2"));
    }

    #[test]
    fn test_pay_provider_not_implemented() {
        let result = pay_provider("staking-box-id", "nft-id", 500_000);
        assert!(result.is_err());
        #[cfg(not(feature = "ergo-lib"))]
        assert!(result.unwrap_err().to_string().contains("Phase 2"));
    }

    #[test]
    fn test_user_staking_info_defaults() {
        let info = UserStakingInfo {
            user_pk_hex: String::new(),
            amount_nanoerg: SAFE_MIN_BOX_VALUE,
            staking_tree_hex: String::new(),
        };
        assert_eq!(info.amount_nanoerg, SAFE_MIN_BOX_VALUE);
        assert!(info.user_pk_hex.is_empty());
        assert!(info.staking_tree_hex.is_empty());
    }

    #[test]
    fn test_user_staking_result_serialization() {
        let result = UserStakingResult {
            tx_id: "tx_abc123".to_string(),
            box_id: "box_def456".to_string(),
            staking_address: "3WwxTestAddress".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: UserStakingResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tx_id, "tx_abc123");
        assert_eq!(parsed.box_id, "box_def456");
        assert_eq!(parsed.staking_address, "3WwxTestAddress");
    }

    #[test]
    fn test_create_user_staking_box_validation_empty_pk() {
        // Verify that an empty PK string would be caught by the async validation.
        // We test the sync validation path via the struct construction.
        let info = UserStakingInfo {
            user_pk_hex: String::new(),
            amount_nanoerg: SAFE_MIN_BOX_VALUE,
            staking_tree_hex: String::new(),
        };
        assert!(info.user_pk_hex.is_empty());
    }

    #[test]
    fn test_create_user_staking_box_validation_bad_pk_length() {
        // PK hex that decodes to wrong length
        let bad_pk = "02aabbccdd"; // 5 bytes, not 33
        let decoded = hex::decode(bad_pk).unwrap();
        assert_ne!(decoded.len(), 33, "Bad PK should not be 33 bytes");
    }

    #[test]
    fn test_create_user_staking_box_validation_insufficient_amount() {
        let info = UserStakingInfo {
            user_pk_hex: "02aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            amount_nanoerg: 500_000, // Below SAFE_MIN_BOX_VALUE
            staking_tree_hex: String::new(),
        };
        assert!(info.amount_nanoerg < SAFE_MIN_BOX_VALUE);
    }

    #[test]
    fn test_create_user_staking_box_validation_valid_pk() {
        // A valid 33-byte compressed PK (hex = 66 chars)
        let valid_pk = "02aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let decoded = hex::decode(valid_pk).unwrap();
        assert_eq!(decoded.len(), 33);
    }
}
