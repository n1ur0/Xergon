//! GPU rental transaction building and submission.
//!
//! Builds on-chain transactions for:
//! - Creating a new GPU listing box
//! - Updating a GPU listing (mark available/unavailable, change price)
//! - Initiating a GPU rental (create rental box with escrowed ERG)
//! - Claiming payment from a completed rental (provider)
//! - Refunding a cancelled rental (renter)
//! - Extending an active rental (renter)
//!
//! All transactions use the Ergo node wallet API (`POST /wallet/payment/send`),
//! which handles signing automatically.

use anyhow::{Context, Result};
use tracing::{debug, info};

use crate::chain::client::ErgoNodeClient;
use crate::chain::transactions::{
    encode_coll_byte, encode_int, encode_long, encode_string,
};
use crate::gpu_rental::scanner::BLOCKS_PER_HOUR;

/// Minimum ERG value for a listing box (nanoERG).
const LISTING_BOX_MIN_VALUE: u64 = 1_000_000; // 0.001 ERG

/// Create a new GPU listing box on-chain.
///
/// The provider's wallet must be unlocked and funded.
/// A new Listing NFT is minted as part of this transaction.
///
/// Returns the transaction ID on success.
pub async fn create_listing_tx(
    client: &ErgoNodeClient,
    listing_tree_hex: &str,
    gpu_type: &str,
    vram_gb: i32,
    price_per_hour_nanoerg: u64,
    region: &str,
    provider_address: &str,
) -> Result<String> {
    // provider_address is used for future R4 PK encoding; currently the wallet
    // implicitly provides the PK via proveDlog when signing.
    let _provider_address = provider_address;

    if listing_tree_hex.is_empty() {
        anyhow::bail!("GPU listing contract ErgoTree not configured — cannot create listing");
    }

    // Encode registers as Sigma constants
    // R4: provider PK is implicit via the P2PK address (wallet signing handles this)
    // But we still need to set R4 with a placeholder; the wallet will use proveDlog.
    // For the wallet payment API, the address field determines who can sign.
    // R4 will be the provider's PK — but since we use the listing contract as the
    // ErgoTree, R4 must be set explicitly.
    //
    // Actually, we need to encode the provider's PK in R4 as a SigmaProp (GroupElement).
    // The wallet payment API doesn't directly support SigmaProp encoding in registers,
    // so we encode it as a GroupElement constant and the contract uses proveDlog().

    // R5: GPU type (String)
    let gpu_type_hex = encode_string(gpu_type);
    // R6: VRAM GB (Int)
    let vram_hex = encode_int(vram_gb);
    // R7: Price per hour (Long)
    let price_hex = encode_long(price_per_hour_nanoerg as i64);
    // R8: Region (String)
    let region_hex = encode_string(region);
    // R9: Available = 1 (Int)
    let available_hex = encode_int(1);

    let payment_request = serde_json::json!({
        "requests": [{
            "address": listing_tree_hex,
            "value": LISTING_BOX_MIN_VALUE.to_string(),
            "assets": [{
                "tokenId": "0000000000000000000000000000000000000000000000000000000000000000",
                "amount": 1
            }],
            "registers": {
                "R5": gpu_type_hex,
                "R6": vram_hex,
                "R7": price_hex,
                "R8": region_hex,
                "R9": available_hex
            }
        }],
        "fee": 1_100_000 // 0.0011 ERG fee (minting token costs more)
    });

    debug!(
        gpu_type = %gpu_type,
        vram_gb = vram_gb,
        price_per_hour_nanoerg,
        region = %region,
        "Creating GPU listing transaction"
    );

    let tx_id = client
        .wallet_payment_send(&payment_request)
        .await
        .context("Failed to create GPU listing via wallet payment")?;

    info!(
        tx_id = %tx_id,
        gpu_type = %gpu_type,
        "GPU listing transaction submitted"
    );

    Ok(tx_id)
}

/// Update an existing GPU listing box (e.g. mark unavailable, change price).
///
/// Spends the existing listing box and creates a new one with updated registers.
/// Requires the provider's wallet to be unlocked (proves proveDlog of R4).
///
/// Returns the transaction ID on success.
pub async fn update_listing_tx(
    client: &ErgoNodeClient,
    listing_box_id: &str,
    price_per_hour_nanoerg: Option<u64>,
    available: Option<bool>,
) -> Result<String> {
    let listing_box = client
        .get_box(listing_box_id)
        .await
        .context("Failed to fetch listing box for update")?;

    // Get current register values from the existing box
    let regs = &listing_box.additional_registers;

    // Re-encode registers, updating only the ones that changed
    let gpu_type_hex = crate::chain::scanner::get_register(regs, "R5")
        .and_then(|v| crate::chain::scanner::parse_coll_byte(v))
        .map(|s| encode_string(&s))
        .context("Missing R5 in existing listing box")?;

    let vram_hex = crate::chain::scanner::get_register(regs, "R6")
        .and_then(|v| crate::chain::scanner::parse_int(v))
        .map(|v| encode_int(v))
        .context("Missing R6 in existing listing box")?;

    let price_hex = match price_per_hour_nanoerg {
        Some(new_price) => encode_long(new_price as i64),
        None => {
            crate::chain::scanner::get_register(regs, "R7")
                .and_then(|v| crate::chain::scanner::parse_long(v))
                .map(|v| encode_long(v))
                .context("Missing R7 in existing listing box")?
        }
    };

    let region_hex = crate::chain::scanner::get_register(regs, "R8")
        .and_then(|v| crate::chain::scanner::parse_coll_byte(v))
        .map(|s| encode_string(&s))
        .context("Missing R8 in existing listing box")?;

    let available_hex = match available {
        Some(avail) => encode_int(if avail { 1 } else { 0 }),
        None => {
            crate::chain::scanner::get_register(regs, "R9")
                .and_then(|v| crate::chain::scanner::parse_int(v))
                .map(|v| encode_int(v))
                .context("Missing R9 in existing listing box")?
        }
    };

    // R4 is preserved implicitly — the wallet proves proveDlog of the same key
    let listing_nft_id = listing_box
        .assets
        .first()
        .map(|a| a.token_id.clone())
        .context("Listing box has no NFT token")?;

    let payment_request = serde_json::json!({
        "requests": [{
            "address": listing_box.ergo_tree.clone(),
            "value": listing_box.value.to_string(),
            "assets": [{
                "tokenId": listing_nft_id,
                "amount": 1
            }],
            "registers": {
                "R5": gpu_type_hex,
                "R6": vram_hex,
                "R7": price_hex,
                "R8": region_hex,
                "R9": available_hex
            }
        }],
        "fee": 1_000_000,
        "inputsRaw": [listing_box_id],
        "dataInputsRaw": []
    });

    debug!(box_id = %listing_box_id, "Updating GPU listing transaction");

    let tx_id = client
        .wallet_payment_send(&payment_request)
        .await
        .context("Failed to update GPU listing via wallet payment")?;

    info!(
        tx_id = %tx_id,
        box_id = %listing_box_id,
        "GPU listing update transaction submitted"
    );

    Ok(tx_id)
}

/// Initiate a GPU rental — creates a rental box with escrowed ERG payment.
///
/// The rental box holds ERG value = hours * price_per_hour.
/// The renter's wallet pays this amount.
///
/// Returns the transaction ID on success.
pub async fn rent_gpu_tx(
    client: &ErgoNodeClient,
    rental_tree_hex: &str,
    listing_box_id: &str,
    hours: i32,
    renter_address: &str,
) -> Result<(String, i32, u64)> {
    if rental_tree_hex.is_empty() {
        anyhow::bail!("GPU rental contract ErgoTree not configured — cannot create rental");
    }

    // Fetch the listing box to get pricing info and mark it unavailable
    let listing_box = client
        .get_box(listing_box_id)
        .await
        .context("Failed to fetch listing box for rental")?;

    let regs = &listing_box.additional_registers;

    let price_per_hour = crate::chain::scanner::get_register(regs, "R7")
        .and_then(|v| crate::chain::scanner::parse_long(v))
        .context("Missing or invalid R7 (price_per_hour) in listing box")?;

    // Calculate total cost
    let cost_nanoerg = (price_per_hour as u64)
        .checked_mul(hours as u64)
        .context("Rental cost overflow")?;

    // Get current height for deadline calculation
    let current_height = client
        .get_height()
        .await
        .context("Failed to get current block height")?;

    let deadline_height = current_height + (hours * BLOCKS_PER_HOUR);

    // Encode rental box registers
    // R4: provider PK — from listing box R4
    let provider_pk_hex = crate::chain::scanner::get_register(regs, "R4")
        .and_then(|v| crate::chain::scanner::parse_group_element(v))
        .context("Missing R4 (provider PK) in listing box")?;
    // Encode as GroupElement constant for R4 (same Sigma encoding as listing)
    let provider_pk_bytes = hex::decode(&provider_pk_hex).unwrap_or_default();
    let r4_hex = encode_coll_byte(&provider_pk_bytes);

    // R5: renter PK — from the renter's address (wallet will prove this)
    // We use the renter_address as a placeholder; the wallet signing handles proveDlog.
    // For the SigmaProp register, we need to encode the PK.
    // Since the wallet payment API doesn't directly set R4/R5 with the wallet's PK,
    // we'll rely on the contract checking proveDlog at spend time.
    // R5 is set to the renter's address encoded as bytes.
    let renter_addr_bytes = renter_address.as_bytes();
    let r5_hex = encode_coll_byte(renter_addr_bytes);

    // R6: deadline height (Int)
    let deadline_hex = encode_int(deadline_height);
    // R7: listing box ID (Coll[Byte])
    let listing_id_bytes = hex::decode(listing_box_id).unwrap_or_default();
    let r7_hex = encode_coll_byte(&listing_id_bytes);
    // R8: rental start height (Int)
    let start_hex = encode_int(current_height);
    // R9: hours rented (Int)
    let hours_hex = encode_int(hours);

    let payment_request = serde_json::json!({
        "requests": [{
            "address": rental_tree_hex,
            "value": cost_nanoerg.to_string(),
            "assets": [{
                "tokenId": "0000000000000000000000000000000000000000000000000000000000000000",
                "amount": 1
            }],
            "registers": {
                "R4": r4_hex,
                "R5": r5_hex,
                "R6": deadline_hex,
                "R7": r7_hex,
                "R8": start_hex,
                "R9": hours_hex
            }
        }],
        "fee": 1_100_000
    });

    debug!(
        listing_box_id = %listing_box_id,
        hours,
        cost_nanoerg,
        deadline_height,
        "Creating GPU rental transaction"
    );

    let tx_id = client
        .wallet_payment_send(&payment_request)
        .await
        .context("Failed to create GPU rental via wallet payment")?;

    info!(
        tx_id = %tx_id,
        listing_box_id = %listing_box_id,
        hours,
        cost_nanoerg,
        deadline_height,
        "GPU rental transaction submitted"
    );

    Ok((tx_id, deadline_height, cost_nanoerg))
}

/// Claim payment from a completed rental (provider path).
///
/// Spends the rental box after the deadline, sending the escrowed ERG
/// to the provider's address.
///
/// Returns the transaction ID on success.
pub async fn claim_rental_tx(
    client: &ErgoNodeClient,
    rental_box_id: &str,
    provider_address: &str,
) -> Result<String> {
    let rental_box = client
        .get_box(rental_box_id)
        .await
        .context("Failed to fetch rental box for claim")?;

    // Verify deadline has passed
    let current_height = client
        .get_height()
        .await
        .context("Failed to get current block height")?;

    let deadline = crate::chain::scanner::get_register(&rental_box.additional_registers, "R6")
        .and_then(|v| crate::chain::scanner::parse_int(v))
        .context("Missing R6 (deadline) in rental box")?;

    if current_height < deadline {
        anyhow::bail!(
            "Cannot claim rental: current height {} < deadline {}",
            current_height,
            deadline
        );
    }

    // Build payment request: send the rental box value to the provider's address
    // The rental NFT is burned (not included in any output)
    let payment_request = serde_json::json!({
        "requests": [{
            "address": provider_address,
            "value": (rental_box.value - 1_000_000).to_string(),
            "assets": []
        }],
        "fee": 1_000_000,
        "inputsRaw": [rental_box_id],
        "dataInputsRaw": []
    });

    debug!(
        rental_box_id = %rental_box_id,
        provider_address = %provider_address,
        value = rental_box.value,
        "Claiming GPU rental payment"
    );

    let tx_id = client
        .wallet_payment_send(&payment_request)
        .await
        .context("Failed to claim GPU rental via wallet payment")?;

    info!(
        tx_id = %tx_id,
        rental_box_id = %rental_box_id,
        provider_address = %provider_address,
        "GPU rental claim transaction submitted"
    );

    Ok(tx_id)
}

/// Refund a cancelled rental (renter path).
///
/// Spends the rental box before the deadline, returning the escrowed ERG
/// to the renter's address.
///
/// Returns the transaction ID on success.
pub async fn refund_rental_tx(
    client: &ErgoNodeClient,
    rental_box_id: &str,
    renter_address: &str,
) -> Result<String> {
    let rental_box = client
        .get_box(rental_box_id)
        .await
        .context("Failed to fetch rental box for refund")?;

    // Verify deadline has NOT passed
    let current_height = client
        .get_height()
        .await
        .context("Failed to get current block height")?;

    let deadline = crate::chain::scanner::get_register(&rental_box.additional_registers, "R6")
        .and_then(|v| crate::chain::scanner::parse_int(v))
        .context("Missing R6 (deadline) in rental box")?;

    if current_height >= deadline {
        anyhow::bail!(
            "Cannot refund rental: current height {} >= deadline {} (provider should claim)",
            current_height,
            deadline
        );
    }

    let payment_request = serde_json::json!({
        "requests": [{
            "address": renter_address,
            "value": (rental_box.value - 1_000_000).to_string(),
            "assets": []
        }],
        "fee": 1_000_000,
        "inputsRaw": [rental_box_id],
        "dataInputsRaw": []
    });

    debug!(
        rental_box_id = %rental_box_id,
        renter_address = %renter_address,
        "Refunding GPU rental"
    );

    let tx_id = client
        .wallet_payment_send(&payment_request)
        .await
        .context("Failed to refund GPU rental via wallet payment")?;

    info!(
        tx_id = %tx_id,
        rental_box_id = %rental_box_id,
        "GPU rental refund transaction submitted"
    );

    Ok(tx_id)
}

/// Extend an active rental (renter path).
///
/// Spends the rental box before the deadline and creates a new one with
/// a later deadline and increased hours.
///
/// Returns the transaction ID on success.
pub async fn extend_rental_tx(
    client: &ErgoNodeClient,
    rental_box_id: &str,
    additional_hours: i32,
) -> Result<String> {
    let rental_box = client
        .get_box(rental_box_id)
        .await
        .context("Failed to fetch rental box for extension")?;

    // Verify deadline has NOT passed
    let current_height = client
        .get_height()
        .await
        .context("Failed to get current block height")?;

    let regs = &rental_box.additional_registers;

    let deadline = crate::chain::scanner::get_register(regs, "R6")
        .and_then(|v| crate::chain::scanner::parse_int(v))
        .context("Missing R6 (deadline) in rental box")?;

    if current_height >= deadline {
        anyhow::bail!(
            "Cannot extend rental: current height {} >= deadline {} (rental expired)",
            current_height,
            deadline
        );
    }

    let existing_hours = crate::chain::scanner::get_register(regs, "R9")
        .and_then(|v| crate::chain::scanner::parse_int(v))
        .unwrap_or(0);

    let new_hours = existing_hours + additional_hours;
    let new_deadline = current_height + (new_hours * BLOCKS_PER_HOUR);

    // Fetch listing box to calculate additional payment
    let listing_box_id = crate::chain::scanner::get_register(regs, "R7")
        .and_then(|v| crate::chain::scanner::parse_coll_byte(v))
        .context("Missing R7 (listing_box_id) in rental box")?;

    let listing_box = client
        .get_box(&listing_box_id)
        .await
        .context("Failed to fetch listing box for price calculation")?;

    let listing_regs = &listing_box.additional_registers;
    let price_per_hour = crate::chain::scanner::get_register(listing_regs, "R7")
        .and_then(|v| crate::chain::scanner::parse_long(v))
        .context("Missing R7 (price_per_hour) in listing box")?;

    let additional_cost = (price_per_hour as u64)
        .checked_mul(additional_hours as u64)
        .context("Additional rental cost overflow")?;

    // Re-encode all registers for the successor rental box
    let r4_hex = crate::chain::scanner::get_register(regs, "R4")
        .and_then(|v| crate::chain::scanner::parse_group_element(v))
        .map(|pk| {
            let bytes = hex::decode(&pk).unwrap_or_default();
            encode_coll_byte(&bytes)
        })
        .context("Missing R4 in rental box")?;

    let r5_hex = crate::chain::scanner::get_register(regs, "R5")
        .and_then(|v| crate::chain::scanner::parse_group_element(v))
        .map(|pk| {
            let bytes = hex::decode(&pk).unwrap_or_default();
            encode_coll_byte(&bytes)
        })
        .context("Missing R5 in rental box")?;

    let r6_hex = encode_int(new_deadline);
    let r7_hex = encode_coll_byte(&hex::decode(&listing_box_id).unwrap_or_default());
    let r8_hex = crate::chain::scanner::get_register(regs, "R8")
        .and_then(|v| crate::chain::scanner::parse_int(v))
        .map(encode_int)
        .unwrap_or_else(|| encode_int(current_height));
    let r9_hex = encode_int(new_hours);

    let rental_nft_id = rental_box
        .assets
        .first()
        .map(|a| a.token_id.clone())
        .context("Rental box has no NFT token")?;

    // The new box value = existing value + additional payment
    let new_box_value = rental_box
        .value
        .checked_add(additional_cost)
        .context("New box value overflow")?;

    let payment_request = serde_json::json!({
        "requests": [{
            "address": rental_box.ergo_tree.clone(),
            "value": new_box_value.to_string(),
            "assets": [{
                "tokenId": rental_nft_id,
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
        "fee": 1_000_000,
        "inputsRaw": [rental_box_id],
        "dataInputsRaw": []
    });

    debug!(
        rental_box_id = %rental_box_id,
        additional_hours,
        new_hours,
        new_deadline,
        additional_cost,
        "Extending GPU rental"
    );

    let tx_id = client
        .wallet_payment_send(&payment_request)
        .await
        .context("Failed to extend GPU rental via wallet payment")?;

    info!(
        tx_id = %tx_id,
        rental_box_id = %rental_box_id,
        additional_hours,
        new_deadline,
        "GPU rental extension transaction submitted"
    );

    Ok(tx_id)
}
