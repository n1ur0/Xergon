//! GPU rating transaction building and submission.
//!
//! Builds on-chain transactions for submitting a rating after a GPU rental
//! completes. The rating box is created with the compiled gpu_rating.es
//! contract ErgoTree and registers R4-R9.

use anyhow::{Context, Result};
use tracing::{debug, info};

use crate::chain::client::ErgoNodeClient;
use crate::chain::transactions::{encode_coll_byte, encode_int, encode_string};

/// Minimum ERG value for a rating box (nanoERG).
const RATING_BOX_MIN_VALUE: u64 = 100_000; // 0.0001 ERG (lightweight)

/// Submit a GPU rental rating -- creates a rating box on-chain.
///
/// The rater's wallet must be unlocked and funded.
/// No NFT is minted for rating boxes (lightweight).
///
/// Returns the transaction ID on success.
pub async fn submit_rating_tx(
    client: &ErgoNodeClient,
    rating_tree_hex: &str,
    rental_box_id: &str,
    rated_pk: &str,
    role: &str,
    rating: i32,
    comment: Option<&str>,
) -> Result<String> {
    if rating_tree_hex.is_empty() {
        anyhow::bail!("GPU rating contract ErgoTree not configured — cannot submit rating");
    }

    // Validate inputs
    if !(1..=5).contains(&rating) {
        anyhow::bail!("Rating must be between 1 and 5, got {}", rating);
    }
    if role != "provider" && role != "renter" {
        anyhow::bail!("Role must be 'provider' or 'renter', got '{}'", role);
    }

    // R4: Rater PK -- implicit via the P2PK address (wallet signing handles this)
    // R5: Rated PK -- encode the rated person's public key
    let rated_pk_bytes = hex::decode(rated_pk).unwrap_or_else(|_| rated_pk.as_bytes().to_vec());
    let r5_hex = encode_coll_byte(&rated_pk_bytes);

    // R6: Role (String)
    let role_hex = encode_string(role);

    // R7: Rental box ID (Coll[Byte])
    let rental_id_bytes = hex::decode(rental_box_id).unwrap_or_else(|_| rental_box_id.as_bytes().to_vec());
    let r7_hex = encode_coll_byte(&rental_id_bytes);

    // R8: Rating (Int)
    let rating_hex = encode_int(rating);

    // R9: Comment hash -- blake2b256 of optional comment, or empty bytes
    let comment_hash_bytes = match comment {
        Some(c) if !c.is_empty() => blake2b256_hash(c.as_bytes()),
        _ => Vec::new(),
    };
    let r9_hex = if comment_hash_bytes.is_empty() {
        encode_string("")
    } else {
        encode_coll_byte(&comment_hash_bytes)
    };

    let payment_request = serde_json::json!({
        "requests": [{
            "address": rating_tree_hex,
            "value": RATING_BOX_MIN_VALUE.to_string(),
            "assets": [],
            "registers": {
                "R5": r5_hex,
                "R6": role_hex,
                "R7": r7_hex,
                "R8": rating_hex,
                "R9": r9_hex
            }
        }],
        "fee": 1_000_000 // 0.001 ERG fee
    });

    debug!(
        rental_box_id = %rental_box_id,
        rated_pk = %rated_pk,
        role = %role,
        rating,
        "Creating GPU rating transaction"
    );

    let tx_id = client
        .wallet_payment_send(&payment_request)
        .await
        .context("Failed to submit GPU rating via wallet payment")?;

    info!(
        tx_id = %tx_id,
        rental_box_id = %rental_box_id,
        rated_pk = %rated_pk,
        role = %role,
        rating,
        "GPU rating transaction submitted"
    );

    Ok(tx_id)
}

/// Compute blake2b256 hash of the given bytes.
fn blake2b256_hash(data: &[u8]) -> Vec<u8> {
    use blake2::{Blake2b, Digest as _};
    use digest::generic_array::typenum::U32;
    type Blake2b256 = Blake2b<U32>;
    let mut hasher = Blake2b256::new();
    hasher.update(data);
    let result = hasher.finalize();
    result.to_vec()
}
