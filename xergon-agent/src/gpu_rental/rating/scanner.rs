//! GPU rating box scanner.
//!
//! Scans the Ergo UTXO set for GPU rating boxes, parsing their registers
//! into typed Rust structs. Also provides reputation aggregation.

use anyhow::{Context, Result};
use tracing::{debug, warn};

use crate::chain::client::ErgoNodeClient;
use crate::chain::scanner::{get_register, parse_coll_byte, parse_group_element, parse_int};
use crate::chain::types::RawBox;
use crate::gpu_rental::rating::types::{GpuRatingBox, Reputation};

/// Minimum ERG value for a GPU Rating Box.
pub const MIN_RATING_BOX_VALUE: u64 = 100_000; // 0.0001 ERG (lightweight)

/// Scan for all GPU rating boxes by scanning for boxes with the
/// compiled rating contract ErgoTree.
///
/// If `rating_tree_hex` is empty, returns an error.
pub async fn scan_gpu_ratings(
    client: &ErgoNodeClient,
    rating_tree_hex: &str,
) -> Result<Vec<GpuRatingBox>> {
    if rating_tree_hex.is_empty() {
        anyhow::bail!("GPU rating contract ErgoTree not configured — cannot scan ratings");
    }

    let boxes = client
        .get_boxes_by_ergo_tree(rating_tree_hex)
        .await
        .context("Failed to scan for GPU rating boxes by ErgoTree")?;

    let mut ratings = Vec::new();

    for raw_box in &boxes {
        match validate_gpu_rating_box(raw_box) {
            Ok(rating) => {
                debug!(
                    box_id = %rating.box_id,
                    rater_pk = %rating.rater_pk,
                    rated_pk = %rating.rated_pk,
                    role = %rating.role,
                    rating_val = rating.rating,
                    "Validated GPU rating box"
                );
                ratings.push(rating);
            }
            Err(e) => {
                warn!(box_id = %raw_box.box_id, error = %e, "Skipping invalid GPU rating box");
            }
        }
    }

    Ok(ratings)
}

/// Scan for GPU rating boxes for a specific rated public key.
///
/// Scans all rating boxes and filters by rated PK in R5.
pub async fn scan_ratings_for_pk(
    client: &ErgoNodeClient,
    rating_tree_hex: &str,
    rated_pk: &str,
) -> Result<Vec<GpuRatingBox>> {
    let all_ratings = scan_gpu_ratings(client, rating_tree_hex).await?;

    let filtered: Vec<GpuRatingBox> = all_ratings
        .into_iter()
        .filter(|r| {
            r.rated_pk.to_lowercase() == rated_pk.to_lowercase()
        })
        .collect();

    Ok(filtered)
}

/// Compute aggregated reputation for a public key.
///
/// Scans all rating boxes and aggregates those where rated_pk matches.
/// Returns a Reputation struct with average, breakdown by stars, and
/// separate provider/renter reputation scores.
pub async fn compute_reputation(
    client: &ErgoNodeClient,
    rating_tree_hex: &str,
    public_key: &str,
) -> Result<Reputation> {
    let ratings = scan_ratings_for_pk(client, rating_tree_hex, public_key).await?;

    let total_ratings = ratings.len();

    if total_ratings == 0 {
        return Ok(Reputation {
            public_key: public_key.to_string(),
            total_ratings: 0,
            average_rating: 0.0,
            one_star: 0,
            two_star: 0,
            three_star: 0,
            four_star: 0,
            five_star: 0,
            provider_reputation: None,
            provider_rating_count: 0,
            renter_reputation: None,
            renter_rating_count: 0,
        });
    }

    // Deduplicate: keep only the latest rating per (rater, rental) pair
    // (a rater can update their rating, only the latest box counts)
    let mut latest_per_pair: std::collections::HashMap<(String, String), &GpuRatingBox> =
        std::collections::HashMap::new();

    for rating in &ratings {
        let key = (rating.rater_pk.to_lowercase(), rating.rental_box_id.to_lowercase());
        // Keep the one with the higher creation height (later = newer)
        latest_per_pair
            .entry(key)
            .and_modify(|existing| {
                if rating.creation_height > existing.creation_height {
                    *existing = rating;
                }
            })
            .or_insert(rating);
    }

    let deduped_ratings: Vec<&GpuRatingBox> = latest_per_pair.values().copied().collect();
    let deduped_count = deduped_ratings.len();

    let mut sum: f64 = 0.0;
    let mut one_star = 0usize;
    let mut two_star = 0usize;
    let mut three_star = 0usize;
    let mut four_star = 0usize;
    let mut five_star = 0usize;

    // Separate tracking for provider vs renter reputation
    let mut provider_ratings: Vec<i32> = Vec::new();
    let mut renter_ratings: Vec<i32> = Vec::new();

    for r in &deduped_ratings {
        sum += r.rating as f64;
        match r.rating {
            1 => one_star += 1,
            2 => two_star += 1,
            3 => three_star += 1,
            4 => four_star += 1,
            5 => five_star += 1,
            _ => {}
        }

        if r.role == "provider" {
            provider_ratings.push(r.rating);
        } else if r.role == "renter" {
            renter_ratings.push(r.rating);
        }
    }

    let average_rating = if deduped_count > 0 {
        sum / deduped_count as f64
    } else {
        0.0
    };

    let provider_reputation = if !provider_ratings.is_empty() {
        Some(provider_ratings.iter().map(|&r| r as f64).sum::<f64>() / provider_ratings.len() as f64)
    } else {
        None
    };

    let renter_reputation = if !renter_ratings.is_empty() {
        Some(renter_ratings.iter().map(|&r| r as f64).sum::<f64>() / renter_ratings.len() as f64)
    } else {
        None
    };

    Ok(Reputation {
        public_key: public_key.to_string(),
        total_ratings: deduped_count,
        average_rating,
        one_star,
        two_star,
        three_star,
        four_star,
        five_star,
        provider_reputation,
        provider_rating_count: provider_ratings.len(),
        renter_reputation,
        renter_rating_count: renter_ratings.len(),
    })
}

/// Validate that a RawBox matches the GPU Rating Box specification.
///
/// Checks:
/// - Minimum ERG value
/// - Required registers R4-R9
/// - Rating value in valid range (1-5)
pub fn validate_gpu_rating_box(raw: &RawBox) -> Result<GpuRatingBox> {
    if raw.value < MIN_RATING_BOX_VALUE {
        anyhow::bail!(
            "GPU rating box {} has insufficient value: {} < {}",
            raw.box_id,
            raw.value,
            MIN_RATING_BOX_VALUE
        );
    }

    let regs = &raw.additional_registers;

    let rater_pk = get_register(regs, "R4")
        .and_then(|v| parse_group_element(v))
        .context(format!(
            "Missing or invalid R4 (rater PK) in GPU rating box {}",
            raw.box_id
        ))?;

    let rated_pk = get_register(regs, "R5")
        .and_then(|v| parse_group_element(v))
        .context(format!(
            "Missing or invalid R5 (rated PK) in GPU rating box {}",
            raw.box_id
        ))?;

    let role = get_register(regs, "R6")
        .and_then(|v| parse_coll_byte(v))
        .context(format!(
            "Missing or invalid R6 (role) in GPU rating box {}",
            raw.box_id
        ))?;

    // Validate role
    if role != "provider" && role != "renter" {
        anyhow::bail!(
            "GPU rating box {} has invalid role: '{}' (expected 'provider' or 'renter')",
            raw.box_id,
            role
        );
    }

    let rental_box_id = get_register(regs, "R7")
        .and_then(|v| parse_coll_byte(v))
        .context(format!(
            "Missing or invalid R7 (rental_box_id) in GPU rating box {}",
            raw.box_id
        ))?;

    let rating = get_register(regs, "R8")
        .and_then(|v| parse_int(v))
        .context(format!(
            "Missing or invalid R8 (rating) in GPU rating box {}",
            raw.box_id
        ))?;

    // Validate rating range
    if !(1..=5).contains(&rating) {
        anyhow::bail!(
            "GPU rating box {} has invalid rating: {} (expected 1-5)",
            raw.box_id,
            rating
        );
    }

    let comment_hash = get_register(regs, "R9")
        .and_then(|v| parse_coll_byte(v))
        .unwrap_or_default();

    Ok(GpuRatingBox {
        box_id: raw.box_id.clone(),
        tx_id: raw.tx_id.clone(),
        rater_pk,
        rated_pk,
        role,
        rental_box_id,
        rating,
        comment_hash,
        value_nanoerg: raw.value,
        creation_height: raw.creation_height,
    })
}
