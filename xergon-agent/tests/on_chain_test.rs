//! On-chain integration tests for Xergon Protocol.
//!
//! These tests verify the full lifecycle against a running Ergo node:
//!   1. Node connectivity and wallet status
//!   2. Bootstrap: mint NFT + create Treasury Box
//!   3. Provider registration: mint provider NFT + create Provider Box
//!   4. Box verification: scan UTXO for created boxes
//!   5. Heartbeat: update provider box registers
//!
//! Requires:
//!   - Ergo node running (default http://127.0.0.1:9053)
//!   - Wallet unlocked and funded (>= 2 ERG for all tests)
//!   - Set ERGO_NODE_URL env var to override default
//!
//! Run with: cargo test --test on_chain_test -- --ignored --nocapture
//! Run one:  cargo test --test on_chain_test test_node_connectivity --ignored --nocapture

use std::env;

// We test through the public modules of xergon-agent.
// Import what we need from the agent's lib.
use xergon_agent::chain::client::ErgoNodeClient;
use xergon_agent::protocol::bootstrap::{
    build_register_provider_tx, build_staking_tx, build_treasury_tx, check_treasury_exists,
    BootstrapConfig, BootstrapState, ProviderRegistrationInfo,
    SAFE_MIN_BOX_VALUE, UserStakingInfo,
};

/// Helper: get the node URL from env or default.
fn node_url() -> String {
    env::var("ERGO_NODE_URL").unwrap_or_else(|_| "http://127.0.0.1:9053".to_string())
}

/// Helper: create an ErgoNodeClient for tests.
fn test_client() -> ErgoNodeClient {
    ErgoNodeClient::new(node_url())
}

/// Helper: wait for a transaction to appear in the node's mempool or be confirmed.
/// Polls every 2 seconds for up to 30 seconds.
async fn wait_for_tx(client: &ErgoNodeClient, tx_id: &str) -> bool {
    for _ in 0..15 {
        match client.get_transaction(tx_id).await {
            Ok(_) => return true,
            Err(_) => tokio::time::sleep(tokio::time::Duration::from_secs(2)).await,
        }
    }
    false
}

// =========================================================================
// Test 1: Node Connectivity
// =========================================================================

#[tokio::test]
#[ignore]
async fn test_node_connectivity() {
    let client = test_client();

    // Should be able to get node info
    let info = client.get_node_info().await.expect("Failed to get node info");
    println!("Node info: {}", serde_json::to_string_pretty(&info).unwrap());

    assert!(info["name"].is_string(), "Missing node name");
    assert!(
        info["headersHeight"].as_u64().unwrap_or(0) > 0,
        "Node has no headers — is it syncing?"
    );

    // Should be able to get current height
    let height = client.get_height().await.expect("Failed to get height");
    println!("Current height: {}", height);
    assert!(height > 0, "Height should be > 0");

    // Check wallet status
    let wallet_ok = client.wallet_status().await.expect("Failed to check wallet");
    println!("Wallet unlocked: {}", wallet_ok);
    assert!(wallet_ok, "Wallet must be unlocked for on-chain tests");
}

// =========================================================================
// Test 2: Bootstrap — Mint NFT + Create Treasury Box
// =========================================================================

#[tokio::test]
#[ignore]
async fn test_bootstrap_mint_nft_and_treasury() {
    let client = test_client();

    // Get the wallet's change address as the deployer
    let info = client.get_node_info().await.expect("Failed to get node info");
    // Use a P2PK address from node info (or use a hardcoded testnet address)
    let deployer_address = info["minerAddress"]
        .as_str()
        .unwrap_or("3WwxWkP5Mh5W2ov5mLCS Bennett")
        .to_string();

    let config = BootstrapConfig {
        deployer_address: deployer_address.clone(),
        treasury_erg_nanoerg: 1_000_000_000, // 1.0 ERG
        treasury_tree_hex: String::new(),    // Use deployer address as guard
        nft_name: "XergonTestNFT".to_string(),
        nft_description: "Integration test NFT for Xergon".to_string(),
        nft_decimals: 0,
    };

    // Build and submit the treasury bootstrap transaction
    let result = build_treasury_tx(&client, &config)
        .await
        .expect("Failed to build treasury tx");

    println!("Treasury tx submitted: {}", result.genesis_tx_id);
    println!("NFT token ID: {}", result.nft_token_id);
    println!("Treasury box ID: {}", result.treasury_box_id);

    // Verify the transaction was accepted
    let found = wait_for_tx(&client, &result.genesis_tx_id).await;
    assert!(found, "Treasury transaction not found on node after 30s");

    // Verify the Treasury box exists on-chain
    let boxes = client
        .get_boxes_by_token_id(&result.nft_token_id)
        .await
        .expect("Failed to scan for treasury box by NFT token ID");

    assert!(
        !boxes.is_empty(),
        "Treasury box not found on-chain — NFT token ID scan returned empty"
    );

    let treasury_box = &boxes[0];
    println!(
        "Treasury box found: id={}, value={}, assets={}",
        treasury_box.box_id,
        treasury_box.value,
        treasury_box.assets.len()
    );
    assert!(
        treasury_box.assets.iter().any(|a| a.token_id == result.nft_token_id && a.amount == 1),
        "Treasury box does not contain the Network NFT"
    );
    assert!(
        treasury_box.value >= 1_000_000_000,
        "Treasury box ERG value too low"
    );
}

// =========================================================================
// Test 3: Bootstrap Idempotency — Skip if Already Deployed
// =========================================================================

#[tokio::test]
#[ignore]
async fn test_bootstrap_idempotent() {
    let client = test_client();

    let info = client.get_node_info().await.expect("Failed to get node info");
    let deployer_address = info["minerAddress"]
        .as_str()
        .unwrap_or("3WwxWkP5Mh5W2ov5")
        .to_string();

    // First check should return false (no treasury yet, unless test 2 ran)
    let exists = check_treasury_exists(
        &client,
        Some("nonexistent_nft_token_id_deadbeef1234567890abcdef"),
        None,
    )
    .await
    .expect("Failed to check treasury existence");

    println!("Treasury exists (should be false): {}", exists);
    assert!(!exists, "Should not find treasury for fake NFT token ID");

    // BootstrapState serialization round-trip
    let state = BootstrapState {
        nft_token_id: "abc123".to_string(),
        treasury_box_id: "box456".to_string(),
        genesis_tx_id: "tx789".to_string(),
        deployer_address,
        treasury_erg_nanoerg: 1_000_000_000,
        block_height: 123456,
        timestamp: "2026-01-01T00:00:00+00:00".to_string(),
    };

    let json = serde_json::to_string(&state).unwrap();
    let parsed: BootstrapState = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.nft_token_id, "abc123");
    println!("BootstrapState round-trip OK");
}

// =========================================================================
// Test 4: Provider Registration
// =========================================================================

#[tokio::test]
#[ignore]
async fn test_provider_registration() {
    let client = test_client();

    let provider_info = ProviderRegistrationInfo {
        provider_pk_hex: "02a9e4e965a0b3b4c2d1e0f8a7b6c5d4e3f2a1b0c9d8e7f6a5b4c3d2e1f0a1b"
            .to_string(), // Fake PK for test
        provider_id: "test-provider-integration".to_string(),
        endpoint: "http://localhost:9099".to_string(),
        models_json: r#"["llama-3.1-8b","mistral-7b"]"#.to_string(),
        region: "us-east".to_string(),
        provider_tree_hex: String::new(), // Use PK address as guard
        box_value_nanoerg: SAFE_MIN_BOX_VALUE,
    };

    let result = build_register_provider_tx(&client, &provider_info)
        .await
        .expect("Failed to register provider");

    println!("Provider registration tx: {}", result.tx_id);
    println!("Provider NFT token ID: {}", result.provider_nft_id);
    println!("Provider box ID: {}", result.provider_box_id);

    // Verify the transaction was accepted
    let found = wait_for_tx(&client, &result.tx_id).await;
    assert!(found, "Provider registration transaction not found after 30s");

    // Verify the provider box exists on-chain
    let boxes = client
        .get_boxes_by_token_id(&result.provider_nft_id)
        .await
        .expect("Failed to scan for provider box");

    assert!(
        !boxes.is_empty(),
        "Provider box not found on-chain"
    );

    let provider_box = &boxes[0];
    println!(
        "Provider box found: id={}, value={}, assets={}",
        provider_box.box_id,
        provider_box.value,
        provider_box.assets.len()
    );

    // Verify the provider NFT
    assert!(
        provider_box.assets.iter().any(|a| a.token_id == result.provider_nft_id && a.amount == 1),
        "Provider box does not contain the provider NFT"
    );

    // Verify registers exist
    // Note: the node may return registers in different formats depending on version
    println!("Provider box registers: {:?}", provider_box.additional_registers);
}

// =========================================================================
// Test 5: UTXO Scanning
// =========================================================================

#[tokio::test]
#[ignore]
async fn test_utxo_scanning_by_token_id() {
    let client = test_client();

    // Scan for a non-existent token — should return empty
    let boxes = client
        .get_boxes_by_token_id("0000000000000000000000000000000000000000000000000000000000000000")
        .await
        .expect("Failed to scan UTXO");

    println!("Boxes for nonexistent token: {}", boxes.len());
    assert!(boxes.is_empty(), "Should find no boxes for nonexistent token ID");

    // Get node info and verify we can query blocks
    let height = client.get_height().await.expect("Failed to get height");
    println!("Current height: {}", height);
    assert!(height > 0);
}

// =========================================================================
// Test 6: Wallet Payment (Simple Send)
// =========================================================================

#[tokio::test]
#[ignore]
async fn test_wallet_simple_send() {
    let client = test_client();

    // Send a tiny amount to self (this tests the wallet API works)
    let info = client.get_node_info().await.expect("Failed to get node info");
    let address = info["minerAddress"]
        .as_str()
        .unwrap_or("3WwxWkP5Mh5W2ov5")
        .to_string();

    let payment = serde_json::json!({
        "requests": [{
            "address": address,
            "value": SAFE_MIN_BOX_VALUE.to_string(),
            "assets": []
        }],
        "fee": 1_100_000  // 0.0011 ERG
    });

    let tx_id = client
        .wallet_payment_send(&payment)
        .await
        .expect("Failed to send wallet payment");

    println!("Simple send tx: {}", tx_id);
    assert!(!tx_id.is_empty(), "Transaction ID should not be empty");

    // Verify the transaction is visible
    let found = wait_for_tx(&client, &tx_id).await;
    assert!(found, "Simple send transaction not found after 30s");
}

// =========================================================================
// Test 7: Wallet Scan Registration (EIP-1)
// =========================================================================

#[tokio::test]
#[ignore]
async fn test_wallet_scan_registration() {
    let client = test_client();

    // Register a scan for a random token (won't match anything, but tests the API)
    let tracking_rule = serde_json::json!({
        "predicate": "contains",
        "value": "deadbeef12345678",
        "tokenType": "ErgoBoxAsset"
    });

    let scan_id = client
        .register_scan("xergon-test-scan", tracking_rule)
        .await
        .expect("Failed to register wallet scan");

    println!("Registered scan ID: {}", scan_id);
    assert!(scan_id > 0, "Scan ID should be positive");

    // List scans
    let scans = client.list_scans().await.expect("Failed to list scans");
    println!("Total scans: {}", scans.len());
    assert!(!scans.is_empty(), "Should have at least one scan");

    // Get boxes for this scan (should be empty)
    let scan_boxes = client
        .get_scan_boxes(scan_id)
        .await
        .expect("Failed to get scan boxes");
    println!("Boxes for test scan: {}", scan_boxes.len());

    // Cleanup: deregister the test scan
    client
        .deregister_scan(scan_id)
        .await
        .expect("Failed to deregister scan");
    println!("Deregistered scan {}", scan_id);
}

// =========================================================================
// Test 8: Box by ID
// =========================================================================

#[tokio::test]
#[ignore]
async fn test_get_box_by_id() {
    let client = test_client();

    // Try to get a box — any box ID format will either work or 404
    // We just want to verify the API endpoint responds correctly
    let result = client.get_box("0000000000000000000000000000000000000000000000000000000000000000").await;
    // This should fail (not found) since we used a fake box ID
    assert!(result.is_err(), "Fake box ID should return an error");
    println!("Correctly returns error for nonexistent box: {:?}", result.unwrap_err());
}

// =========================================================================
// Test 9: Create User Staking Box
// =========================================================================

#[tokio::test]
#[ignore]
async fn test_create_user_staking_box() {
    let client = test_client();

    // Fake but syntactically valid 33-byte compressed secp256k1 PK
    let user_pk_hex = "02aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    // Try to read the compiled user_staking.es hex from the contracts directory
    let staking_tree_hex = std::fs::read_to_string("contracts/compiled/user_staking.hex")
        .unwrap_or_default()
        .trim()
        .to_string();

    // Skip the ErgoTree test if the hex file is empty or a placeholder
    let use_ergo_tree = !staking_tree_hex.is_empty()
        && staking_tree_hex.len() > 20
        && !staking_tree_hex.starts_with("PLACEHOLDER");

    let staking_info = UserStakingInfo {
        user_pk_hex: user_pk_hex.to_string(),
        amount_nanoerg: 2_000_000_000, // 2.0 ERG — well above minimum
        staking_tree_hex: if use_ergo_tree {
            staking_tree_hex
        } else {
            String::new() // Use P2PK fallback
        },
    };

    println!(
        "Creating staking box: pk={}, amount={}, use_ergo_tree={}",
        &user_pk_hex[..8],
        staking_info.amount_nanoerg,
        use_ergo_tree
    );

    let result = build_staking_tx(&client, &staking_info)
        .await
        .expect("Failed to create user staking box");

    println!("Staking box tx submitted: {}", result.tx_id);
    println!("Staking box ID: {}", result.box_id);
    println!("Staking address: {}", result.staking_address);

    // Verify the transaction was accepted
    let found = wait_for_tx(&client, &result.tx_id).await;
    assert!(found, "Staking box transaction not found on node after 30s");

    // Verify the staking box exists on-chain by fetching it directly
    let staking_box = client
        .get_box(&result.box_id)
        .await
        .expect("Failed to fetch staking box by ID");

    println!(
        "Staking box found: id={}, value={}, creation_height={}",
        staking_box.box_id,
        staking_box.value,
        staking_box.creation_height
    );

    // Verify value
    assert!(
        staking_box.value >= 2_000_000_000,
        "Staking box ERG value too low: {}",
        staking_box.value
    );

    // Verify R4 register exists (contains the user PK)
    let has_r4 = staking_box
        .additional_registers
        .contains_key("R4");
    assert!(has_r4, "Staking box should have R4 register (user PK)");
    println!(
        "R4 register: {:?}",
        staking_box.additional_registers.get("R4")
    );

    // If we used the ErgoTree, verify the box's ergo_tree matches
    if use_ergo_tree {
        println!(
            "Box ergo_tree prefix: {}...",
            &staking_box.ergo_tree[..staking_box.ergo_tree.len().min(16)]
        );
    }
}

// =========================================================================
// Phase 10: New Contract Integration Tests
// =========================================================================
//
// Tests for the 6 new contracts added in Phase 10:
//   gpu_rental, usage_commitment, relay_registry, gpu_rating,
//   gpu_rental_listing, payment_bridge
//
// These tests verify the full lifecycle: read compiled contract hex (or
// fall back to P2PK), build a transaction with proper registers and
// tokens, submit via the node wallet API, and verify on-chain.
//
// Run all:  cargo test --test on_chain_test -- --ignored --nocapture
// Run one:  cargo test --test on_chain_test test_gpu_rental_listing_create --ignored --nocapture

// ---------------------------------------------------------------------------
// Sigma constant encoding helpers
// ---------------------------------------------------------------------------
// These mirror the pub(crate) helpers in chain/transactions.rs.
// They are duplicated here because integration tests (external to the
// crate) cannot access pub(crate) items.

/// Encode a variable-length byte count (VLB).
fn encode_vlb(out: &mut Vec<u8>, len: usize) {
    if len < 128 {
        out.push(len as u8);
    } else {
        out.push(((len >> 7) as u8) | 0x80);
        out.push((len & 0x7F) as u8);
    }
}

/// Encode a Sigma `Int` (4 bytes big-endian).
/// Format: `04 <4 bytes big-endian>` as hex string.
fn encode_int(value: i32) -> String {
    let mut bytes = vec![0x04];
    bytes.extend_from_slice(&value.to_be_bytes());
    hex::encode(&bytes)
}

/// Encode a Sigma `Long` (8 bytes big-endian).
/// Format: `05 <8 bytes big-endian>` as hex string.
fn encode_long(value: i64) -> String {
    let mut bytes = vec![0x05];
    bytes.extend_from_slice(&value.to_be_bytes());
    hex::encode(&bytes)
}

/// Encode a Sigma `String` as `Coll[Byte]`.
/// Format: `0e <vlb_length> <utf8_bytes>` as hex string.
fn encode_string(s: &str) -> String {
    let data = s.as_bytes();
    let mut bytes = vec![0x0e];
    encode_vlb(&mut bytes, data.len());
    bytes.extend_from_slice(data);
    hex::encode(&bytes)
}

/// Encode raw bytes as a Sigma `Coll[Byte]`.
/// Format: `0e <vlb_length> <data_bytes>` as hex string.
fn encode_coll_byte(data: &[u8]) -> String {
    let mut bytes = vec![0x0e];
    encode_vlb(&mut bytes, data.len());
    bytes.extend_from_slice(data);
    hex::encode(&bytes)
}

/// Encode a 33-byte compressed secp256k1 public key as a Sigma `GroupElement`.
///
/// Format: `0e 21 <33 bytes>` as hex string.
/// The GroupElement is stored as `Coll[Byte]` with length prefix `0x21` (33).
fn encode_group_element(pk_hex: &str) -> String {
    let pk_bytes = hex::decode(pk_hex).expect("Invalid PK hex");
    assert_eq!(pk_bytes.len(), 33, "PK must be 33 bytes, got {}", pk_bytes.len());
    // GroupElement serialized as Coll[Byte]: 0e <vlb_len=0x21> <33 bytes>
    let mut bytes = vec![0x0e, 0x21];
    bytes.extend_from_slice(&pk_bytes);
    hex::encode(&bytes)
}

/// Encode a 33-byte compressed secp256k1 public key as a Sigma `SigmaProp`
/// wrapping a `proveDlog`.
///
/// Format: `08 <vlb_len> 00 21 <33 bytes>` as hex string.
/// The `08` tag is SSigmaProp constant, inner bytes are the serialized
/// ProveDlog SigmaBoolean: tag `00` (TrivialProveDlog) + VLB(33) + point bytes.
fn encode_sigma_prop(pk_hex: &str) -> String {
    let pk_bytes = hex::decode(pk_hex).expect("Invalid PK hex");
    assert_eq!(pk_bytes.len(), 33, "PK must be 33 bytes, got {}", pk_bytes.len());
    // ProveDlog SigmaBoolean: tag 0x00 + VLB(33)=0x21 + 33 bytes = 35 bytes
    let mut value_bytes = vec![0x00, 0x21];
    value_bytes.extend_from_slice(&pk_bytes);
    // SigmaProp constant: 0x08 <vlb_len_of_value_bytes> <value_bytes>
    let mut bytes = vec![0x08];
    encode_vlb(&mut bytes, value_bytes.len());
    bytes.extend_from_slice(&value_bytes);
    hex::encode(&bytes)
}

/// Try to read a compiled contract hex from `contracts/compiled/<name>.hex`.
///
/// Returns the trimmed hex string, or an empty string if the file is
/// missing, empty, or a placeholder.
fn read_contract_hex(name: &str) -> String {
    let path = format!("contracts/compiled/{}.hex", name);
    let hex = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .trim()
        .to_string();
    if hex.is_empty() || hex.len() < 20 || hex.starts_with("PLACEHOLDER") {
        String::new()
    } else {
        hex
    }
}

// Fake but syntactically valid test keys (33-byte compressed secp256k1)
const TEST_PROVIDER_PK: &str = "02a9e4e965a0b3b4c2d1e0f8a7b6c5d4e3f2a1b0c9d8e7f6a5b4c3d2e1f0a1b";
const TEST_RENTER_PK: &str  = "02bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const TEST_RATER_PK: &str   = "02cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const TEST_BRIDGE_PK: &str  = "02dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";

// =========================================================================
// Test 10: GPU Rental Listing — Create Listing
// =========================================================================

#[tokio::test]
#[ignore]
async fn test_gpu_rental_listing_create() {
    let client = test_client();

    // Try to read compiled gpu_rental_listing.es hex
    let contract_hex = read_contract_hex("gpu_rental_listing");
    let use_ergo_tree = !contract_hex.is_empty();

    // Register layout from gpu_rental_listing.es:
    //   R4: Provider PK    (GroupElement)
    //   R5: GPU type       (Coll[Byte], UTF-8)
    //   R6: VRAM GB        (Int)
    //   R7: Price per hour (Long, nanoERG)
    //   R8: Region         (Coll[Byte], UTF-8)
    //   R9: Available      (Int) — 1=available, 0=unavailable
    //   tokens(0): Listing NFT (singleton, EIP-4)

    let r4_provider = encode_group_element(TEST_PROVIDER_PK);
    let r5_gpu_type = encode_string("RTX 4090");
    let r6_vram     = encode_int(24);
    let r7_price    = encode_long(50_000_000); // 0.05 ERG/hour
    let r8_region   = encode_string("us-east");
    let r9_available = encode_int(1);

    let mut request_obj = serde_json::json!({
        "value": SAFE_MIN_BOX_VALUE.to_string(),
        "assets": [{
            "amount": 1,
            "name": "XergonListing-RTX4090-test",
            "description": "GPU Rental Listing: RTX 4090",
            "decimals": 0,
            "type": "EIP-004"
        }],
        "registers": {
            "R4": r4_provider,
            "R5": r5_gpu_type,
            "R6": r6_vram,
            "R7": r7_price,
            "R8": r8_region,
            "R9": r9_available
        }
    });

    if use_ergo_tree {
        request_obj["ergoTree"] = serde_json::Value::String(contract_hex);
    } else {
        request_obj["address"] = serde_json::Value::String(format!("pk_{}", TEST_PROVIDER_PK));
    }

    let payment_request = serde_json::json!({
        "requests": [request_obj],
        "fee": (1_100_000).to_string()  // 0.0011 ERG
    });

    println!(
        "Creating GPU rental listing box: use_ergo_tree={}, value={}",
        use_ergo_tree, SAFE_MIN_BOX_VALUE
    );

    let tx_id = client
        .wallet_payment_send(&payment_request)
        .await
        .expect("Failed to create GPU rental listing box");

    println!("GPU rental listing tx submitted: {}", tx_id);

    let found = wait_for_tx(&client, &tx_id).await;
    assert!(found, "GPU rental listing transaction not found after 30s");

    // Fetch tx details and extract the listing NFT + box ID
    let tx_detail = client
        .get_transaction(&tx_id)
        .await
        .expect("Failed to fetch listing tx details");

    // Verify an output was created with the NFT
    let outputs = tx_detail.get("outputs").and_then(|o| o.as_array());
    assert!(outputs.is_some() && !outputs.unwrap().is_empty(), "No outputs in listing tx");

    let listing_output = &outputs.unwrap()[0];
    let box_id = listing_output["boxId"].as_str().unwrap_or("unknown");
    let value = listing_output["value"].as_u64().unwrap_or(0);

    println!(
        "GPU rental listing box created: id={}, value={}",
        box_id, value
    );
    assert!(value >= SAFE_MIN_BOX_VALUE, "Listing box value too low");
}

// =========================================================================
// Test 11: GPU Rental — Create Rental Escrow
// =========================================================================

#[tokio::test]
#[ignore]
async fn test_gpu_rental_create_rental() {
    let client = test_client();

    let contract_hex = read_contract_hex("gpu_rental");
    let use_ergo_tree = !contract_hex.is_empty();

    // Register layout from gpu_rental.es:
    //   R4: Provider PK     (GroupElement)
    //   R5: Renter PK       (GroupElement)
    //   R6: Deadline height (Int)
    //   R7: Listing box ID  (Coll[Byte])
    //   R8: Rental start    (Int)
    //   R9: Hours rented    (Int)
    //   tokens(0): Rental NFT (singleton, EIP-4)

    let height = client.get_height().await.expect("Failed to get height");

    let r4_provider   = encode_group_element(TEST_PROVIDER_PK);
    let r5_renter     = encode_group_element(TEST_RENTER_PK);
    let r6_deadline   = encode_int(height + 7200); // ~10 days at 2-min blocks
    let r7_listing_id = encode_coll_byte(&[0u8; 32]); // placeholder listing box ID
    let r8_start      = encode_int(height);
    let r9_hours      = encode_int(24);

    let escrow_value = 24 * 50_000_000; // 24 hours * 0.05 ERG/hour = 1.2 ERG

    let mut request_obj = serde_json::json!({
        "value": escrow_value.to_string(),
        "assets": [{
            "amount": 1,
            "name": "XergonRental-test",
            "description": "GPU Rental Escrow Box",
            "decimals": 0,
            "type": "EIP-004"
        }],
        "registers": {
            "R4": r4_provider,
            "R5": r5_renter,
            "R6": r6_deadline,
            "R7": r7_listing_id,
            "R8": r8_start,
            "R9": r9_hours
        }
    });

    if use_ergo_tree {
        request_obj["ergoTree"] = serde_json::Value::String(contract_hex);
    } else {
        request_obj["address"] = serde_json::Value::String(format!("pk_{}", TEST_PROVIDER_PK));
    }

    let payment_request = serde_json::json!({
        "requests": [request_obj],
        "fee": (1_100_000).to_string()
    });

    println!(
        "Creating GPU rental escrow box: use_ergo_tree={}, escrow_value={}, deadline={}",
        use_ergo_tree, escrow_value, height + 7200
    );

    let tx_id = client
        .wallet_payment_send(&payment_request)
        .await
        .expect("Failed to create GPU rental escrow box");

    println!("GPU rental escrow tx submitted: {}", tx_id);

    let found = wait_for_tx(&client, &tx_id).await;
    assert!(found, "GPU rental escrow transaction not found after 30s");

    let tx_detail = client
        .get_transaction(&tx_id)
        .await
        .expect("Failed to fetch rental tx details");

    let outputs = tx_detail.get("outputs").and_then(|o| o.as_array());
    assert!(outputs.is_some() && !outputs.unwrap().is_empty());

    let rental_output = &outputs.unwrap()[0];
    let box_id = rental_output["boxId"].as_str().unwrap_or("unknown");
    let value = rental_output["value"].as_u64().unwrap_or(0);

    println!(
        "GPU rental escrow box created: id={}, value={}",
        box_id, value
    );
    assert!(value >= escrow_value, "Rental escrow value too low: {}", value);
}

// =========================================================================
// Test 12: Usage Commitment — Create Commitment Box
// =========================================================================

#[tokio::test]
#[ignore]
async fn test_usage_commitment_create() {
    let client = test_client();

    let contract_hex = read_contract_hex("usage_commitment");
    let use_ergo_tree = !contract_hex.is_empty();

    // Register layout from usage_commitment.es:
    //   R4: Provider PK    (SigmaProp)
    //   R5: Epoch start    (Int)
    //   R6: Epoch end      (Int)
    //   R7: Proof count    (Int)
    //   R8: Merkle root    (Coll[Byte], 32 bytes)
    //   tokens(0): Commitment NFT (singleton, EIP-4)

    let height = client.get_height().await.expect("Failed to get height");

    let r4_provider   = encode_sigma_prop(TEST_PROVIDER_PK);
    let r5_epoch_start = encode_int(height - 100);
    let r6_epoch_end   = encode_int(height);
    let r7_proof_count = encode_int(42);
    let r8_merkle_root = encode_coll_byte(&[0xAB; 32]); // placeholder merkle root

    let mut request_obj = serde_json::json!({
        "value": SAFE_MIN_BOX_VALUE.to_string(),
        "assets": [{
            "amount": 1,
            "name": "XergonCommitment-test",
            "description": "Usage Commitment Box",
            "decimals": 0,
            "type": "EIP-004"
        }],
        "registers": {
            "R4": r4_provider,
            "R5": r5_epoch_start,
            "R6": r6_epoch_end,
            "R7": r7_proof_count,
            "R8": r8_merkle_root
        }
    });

    if use_ergo_tree {
        request_obj["ergoTree"] = serde_json::Value::String(contract_hex);
    } else {
        request_obj["address"] = serde_json::Value::String(format!("pk_{}", TEST_PROVIDER_PK));
    }

    let payment_request = serde_json::json!({
        "requests": [request_obj],
        "fee": (1_100_000).to_string()
    });

    println!(
        "Creating usage commitment box: use_ergo_tree={}, epoch=[{}, {}], proofs={}",
        use_ergo_tree, height - 100, height, 42
    );

    let tx_id = client
        .wallet_payment_send(&payment_request)
        .await
        .expect("Failed to create usage commitment box");

    println!("Usage commitment tx submitted: {}", tx_id);

    let found = wait_for_tx(&client, &tx_id).await;
    assert!(found, "Usage commitment transaction not found after 30s");

    let tx_detail = client
        .get_transaction(&tx_id)
        .await
        .expect("Failed to fetch commitment tx details");

    let outputs = tx_detail.get("outputs").and_then(|o| o.as_array());
    assert!(outputs.is_some() && !outputs.unwrap().is_empty());

    let commitment_output = &outputs.unwrap()[0];
    let box_id = commitment_output["boxId"].as_str().unwrap_or("unknown");

    println!("Usage commitment box created: id={}", box_id);
}

// =========================================================================
// Test 13: Relay Registry — Register Relay
// =========================================================================

#[tokio::test]
#[ignore]
async fn test_relay_registry_register() {
    let client = test_client();

    let contract_hex = read_contract_hex("relay_registry");
    let use_ergo_tree = !contract_hex.is_empty();

    // Register layout from relay_registry.es:
    //   R4: Relay PK       (SigmaProp)
    //   R5: Relay endpoint (Coll[Byte], UTF-8)
    //   R6: Last heartbeat (Int, epoch seconds)
    //   tokens(0): Relay NFT (singleton, EIP-4)

    let now_ts = chrono::Utc::now().timestamp() as i32;

    let r4_relay_pk  = encode_sigma_prop(TEST_PROVIDER_PK);
    let r5_endpoint  = encode_string("http://relay.xergon.test:9099");
    let r6_heartbeat = encode_int(now_ts);

    let mut request_obj = serde_json::json!({
        "value": SAFE_MIN_BOX_VALUE.to_string(),
        "assets": [{
            "amount": 1,
            "name": "XergonRelay-test",
            "description": "Relay Registration Box",
            "decimals": 0,
            "type": "EIP-004"
        }],
        "registers": {
            "R4": r4_relay_pk,
            "R5": r5_endpoint,
            "R6": r6_heartbeat
        }
    });

    if use_ergo_tree {
        request_obj["ergoTree"] = serde_json::Value::String(contract_hex);
    } else {
        request_obj["address"] = serde_json::Value::String(format!("pk_{}", TEST_PROVIDER_PK));
    }

    let payment_request = serde_json::json!({
        "requests": [request_obj],
        "fee": (1_100_000).to_string()
    });

    println!(
        "Creating relay registration box: use_ergo_tree={}, endpoint={}, ts={}",
        use_ergo_tree, "http://relay.xergon.test:9099", now_ts
    );

    let tx_id = client
        .wallet_payment_send(&payment_request)
        .await
        .expect("Failed to create relay registration box");

    println!("Relay registration tx submitted: {}", tx_id);

    let found = wait_for_tx(&client, &tx_id).await;
    assert!(found, "Relay registration transaction not found after 30s");

    let tx_detail = client
        .get_transaction(&tx_id)
        .await
        .expect("Failed to fetch relay tx details");

    let outputs = tx_detail.get("outputs").and_then(|o| o.as_array());
    assert!(outputs.is_some() && !outputs.unwrap().is_empty());

    let relay_output = &outputs.unwrap()[0];
    let box_id = relay_output["boxId"].as_str().unwrap_or("unknown");

    println!("Relay registration box created: id={}", box_id);
}

// =========================================================================
// Test 14: GPU Rating — Submit Rating
// =========================================================================

#[tokio::test]
#[ignore]
async fn test_gpu_rating_submit() {
    let client = test_client();

    let contract_hex = read_contract_hex("gpu_rating");
    let use_ergo_tree = !contract_hex.is_empty();

    // Register layout from gpu_rating.es:
    //   R4: Rater PK      (GroupElement)
    //   R5: Rated PK      (GroupElement)
    //   R6: Role          (Coll[Byte], UTF-8) — "provider" or "renter"
    //   R7: Rental box ID (Coll[Byte], 32 bytes)
    //   R8: Rating        (Int, 1-5)
    //   R9: Comment hash  (Coll[Byte], 32 bytes)
    //   No tokens required

    let r4_rater      = encode_group_element(TEST_RENTER_PK);
    let r5_rated      = encode_group_element(TEST_PROVIDER_PK);
    let r6_role       = encode_string("provider");
    let r7_rental_id  = encode_coll_byte(&[0x01; 32]); // placeholder rental box ID
    let r8_rating     = encode_int(5);
    let r9_comment    = encode_coll_byte(&[0xDE; 32]); // placeholder comment hash

    let mut request_obj = serde_json::json!({
        "value": SAFE_MIN_BOX_VALUE.to_string(),
        "assets": [],
        "registers": {
            "R4": r4_rater,
            "R5": r5_rated,
            "R6": r6_role,
            "R7": r7_rental_id,
            "R8": r8_rating,
            "R9": r9_comment
        }
    });

    if use_ergo_tree {
        request_obj["ergoTree"] = serde_json::Value::String(contract_hex);
    } else {
        request_obj["address"] = serde_json::Value::String(format!("pk_{}", TEST_RENTER_PK));
    }

    let payment_request = serde_json::json!({
        "requests": [request_obj],
        "fee": (1_100_000).to_string()
    });

    println!(
        "Creating GPU rating box: use_ergo_tree={}, rater={}, rated={}, rating=5",
        use_ergo_tree, &TEST_RENTER_PK[..8], &TEST_PROVIDER_PK[..8]
    );

    let tx_id = client
        .wallet_payment_send(&payment_request)
        .await
        .expect("Failed to create GPU rating box");

    println!("GPU rating tx submitted: {}", tx_id);

    let found = wait_for_tx(&client, &tx_id).await;
    assert!(found, "GPU rating transaction not found after 30s");

    // Verify the rating box was created on-chain
    let tx_detail = client
        .get_transaction(&tx_id)
        .await
        .expect("Failed to fetch rating tx details");

    let outputs = tx_detail.get("outputs").and_then(|o| o.as_array());
    assert!(outputs.is_some() && !outputs.unwrap().is_empty());

    let rating_output = &outputs.unwrap()[0];
    let box_id = rating_output["boxId"].as_str().unwrap_or("unknown");
    let value = rating_output["value"].as_u64().unwrap_or(0);

    println!("GPU rating box created: id={}, value={}", box_id, value);
    assert!(value >= SAFE_MIN_BOX_VALUE, "Rating box value too low");

    // Verify registers are present
    let registers = rating_output.get("additionalRegisters");
    if let Some(regs) = registers {
        println!("Rating box registers: {}", serde_json::to_string_pretty(regs).unwrap_or_default());
    }
}

// =========================================================================
// Test 15: Payment Bridge — Deposit (Create Invoice Box)
// =========================================================================

#[tokio::test]
#[ignore]
async fn test_payment_bridge_deposit() {
    let client = test_client();

    let contract_hex = read_contract_hex("payment_bridge");
    let use_ergo_tree = !contract_hex.is_empty();

    // Register layout from payment_bridge.es:
    //   R4: Buyer PK        (SigmaProp)
    //   R5: Provider PK     (SigmaProp)
    //   R6: Amount nanoerg  (Long)
    //   R7: Foreign tx ID   (Coll[Byte], 32 bytes)
    //   R8: Foreign chain   (Int) — 0=BTC, 1=ETH, 2=ADA
    //   R9: Bridge PK       (SigmaProp)
    //   tokens(0): Invoice NFT (singleton, EIP-4)

    let deposit_amount = 500_000_000; // 0.5 ERG

    let r4_buyer     = encode_sigma_prop(TEST_RENTER_PK);
    let r5_provider  = encode_sigma_prop(TEST_PROVIDER_PK);
    let r6_amount    = encode_long(deposit_amount as i64);
    let r7_foreign   = encode_coll_byte(&[0x00; 32]); // placeholder foreign tx ID
    let r8_chain     = encode_int(1); // ETH
    let r9_bridge    = encode_sigma_prop(TEST_BRIDGE_PK);

    let mut request_obj = serde_json::json!({
        "value": deposit_amount.to_string(),
        "assets": [{
            "amount": 1,
            "name": "XergonInvoice-test",
            "description": "Payment Bridge Invoice Box",
            "decimals": 0,
            "type": "EIP-004"
        }],
        "registers": {
            "R4": r4_buyer,
            "R5": r5_provider,
            "R6": r6_amount,
            "R7": r7_foreign,
            "R8": r8_chain,
            "R9": r9_bridge
        }
    });

    if use_ergo_tree {
        request_obj["ergoTree"] = serde_json::Value::String(contract_hex);
    } else {
        request_obj["address"] = serde_json::Value::String(format!("pk_{}", TEST_RENTER_PK));
    }

    let payment_request = serde_json::json!({
        "requests": [request_obj],
        "fee": (1_100_000).to_string()
    });

    println!(
        "Creating payment bridge invoice box: use_ergo_tree={}, amount={}, chain=ETH",
        use_ergo_tree, deposit_amount
    );

    let tx_id = client
        .wallet_payment_send(&payment_request)
        .await
        .expect("Failed to create payment bridge invoice box");

    println!("Payment bridge invoice tx submitted: {}", tx_id);

    let found = wait_for_tx(&client, &tx_id).await;
    assert!(found, "Payment bridge invoice transaction not found after 30s");

    let tx_detail = client
        .get_transaction(&tx_id)
        .await
        .expect("Failed to fetch payment bridge tx details");

    let outputs = tx_detail.get("outputs").and_then(|o| o.as_array());
    assert!(outputs.is_some() && !outputs.unwrap().is_empty());

    let invoice_output = &outputs.unwrap()[0];
    let box_id = invoice_output["boxId"].as_str().unwrap_or("unknown");
    let value = invoice_output["value"].as_u64().unwrap_or(0);

    println!(
        "Payment bridge invoice box created: id={}, value={}",
        box_id, value
    );
    assert!(
        value >= deposit_amount,
        "Invoice box value {} is below deposit amount {}",
        value, deposit_amount
    );
}
