//! GPU listing and rental box scanner.
//!
//! Scans the Ergo UTXO set for GPU listing boxes and rental boxes,
//! parsing their registers into typed Rust structs.

use anyhow::{Context, Result};
use tracing::{debug, warn};

use crate::chain::client::ErgoNodeClient;
use crate::chain::scanner::{get_register, parse_coll_byte, parse_group_element, parse_int, parse_long};
use crate::chain::types::RawBox;
use crate::gpu_rental::types::{GpuListingBox, GpuRentalBox};

/// Minimum ERG value for a GPU Listing Box.
pub const MIN_LISTING_BOX_VALUE: u64 = 1_000_000; // 0.001 ERG

/// Minimum ERG value for a GPU Rental Box (the escrowed payment).
pub const MIN_RENTAL_BOX_VALUE: u64 = 1_000_000; // 0.001 ERG

/// Default blocks per hour (Ergo: ~2 min/block = 30 blocks/hour).
pub const BLOCKS_PER_HOUR: i32 = 30;

/// Scan for all GPU listing boxes by scanning for boxes with the
/// compiled listing contract ErgoTree.
///
/// If `listing_tree_hex` is empty, returns an error.
pub async fn scan_gpu_listings(
    client: &ErgoNodeClient,
    listing_tree_hex: &str,
) -> Result<Vec<GpuListingBox>> {
    if listing_tree_hex.is_empty() {
        anyhow::bail!("GPU listing contract ErgoTree not configured — cannot scan listings");
    }

    let boxes = client
        .get_boxes_by_ergo_tree(listing_tree_hex)
        .await
        .context("Failed to scan for GPU listing boxes by ErgoTree")?;

    let mut listings = Vec::new();

    for raw_box in &boxes {
        match validate_gpu_listing_box(raw_box) {
            Ok(listing) => {
                debug!(
                    box_id = %listing.box_id,
                    gpu_type = %listing.gpu_type,
                    vram_gb = listing.vram_gb,
                    price = listing.price_per_hour_nanoerg,
                    available = listing.available,
                    "Validated GPU listing box"
                );
                listings.push(listing);
            }
            Err(e) => {
                warn!(box_id = %raw_box.box_id, error = %e, "Skipping invalid GPU listing box");
            }
        }
    }

    Ok(listings)
}

/// Scan for all GPU rental boxes by scanning for boxes with the
/// compiled rental contract ErgoTree.
///
/// If `rental_tree_hex` is empty, returns an error.
pub async fn scan_gpu_rentals(
    client: &ErgoNodeClient,
    rental_tree_hex: &str,
) -> Result<Vec<GpuRentalBox>> {
    if rental_tree_hex.is_empty() {
        anyhow::bail!("GPU rental contract ErgoTree not configured — cannot scan rentals");
    }

    let boxes = client
        .get_boxes_by_ergo_tree(rental_tree_hex)
        .await
        .context("Failed to scan for GPU rental boxes by ErgoTree")?;

    let mut rentals = Vec::new();

    for raw_box in &boxes {
        match validate_gpu_rental_box(raw_box) {
            Ok(rental) => {
                debug!(
                    box_id = %rental.box_id,
                    deadline = rental.deadline_height,
                    hours = rental.hours_rented,
                    value = rental.value_nanoerg,
                    "Validated GPU rental box"
                );
                rentals.push(rental);
            }
            Err(e) => {
                warn!(box_id = %raw_box.box_id, error = %e, "Skipping invalid GPU rental box");
            }
        }
    }

    Ok(rentals)
}

/// Scan for GPU rental boxes belonging to a specific renter (by public key).
///
/// Scans all rental boxes and filters by renter PK in R5.
pub async fn scan_rentals_by_renter(
    client: &ErgoNodeClient,
    rental_tree_hex: &str,
    renter_pk: &str,
) -> Result<Vec<GpuRentalBox>> {
    let all_rentals = scan_gpu_rentals(client, rental_tree_hex).await?;

    let filtered: Vec<GpuRentalBox> = all_rentals
        .into_iter()
        .filter(|r| {
            // Compare lowercase hex strings for matching
            r.renter_pk.to_lowercase() == renter_pk.to_lowercase()
        })
        .collect();

    Ok(filtered)
}

/// Scan for GPU listing boxes belonging to a specific provider (by public key).
pub async fn scan_listings_by_provider(
    client: &ErgoNodeClient,
    listing_tree_hex: &str,
    provider_pk: &str,
) -> Result<Vec<GpuListingBox>> {
    let all_listings = scan_gpu_listings(client, listing_tree_hex).await?;

    let filtered: Vec<GpuListingBox> = all_listings
        .into_iter()
        .filter(|l| {
            l.provider_pk.to_lowercase() == provider_pk.to_lowercase()
        })
        .collect();

    Ok(filtered)
}

/// Validate that a RawBox matches the GPU Listing Box specification.
///
/// Checks:
/// - At least one token present (Listing NFT, supply=1)
/// - Minimum ERG value
/// - Required registers R4-R9
pub fn validate_gpu_listing_box(raw: &RawBox) -> Result<GpuListingBox> {
    if raw.value < MIN_LISTING_BOX_VALUE {
        anyhow::bail!(
            "GPU listing box {} has insufficient value: {} < {}",
            raw.box_id,
            raw.value,
            MIN_LISTING_BOX_VALUE
        );
    }

    if raw.assets.is_empty() {
        anyhow::bail!(
            "GPU listing box {} has no tokens (expected Listing NFT)",
            raw.box_id
        );
    }

    let listing_nft_id = &raw.assets[0].token_id;
    if raw.assets[0].amount != 1 {
        anyhow::bail!(
            "GPU listing box {} NFT has amount {} (expected exactly 1)",
            raw.box_id,
            raw.assets[0].amount
        );
    }

    let regs = &raw.additional_registers;

    let provider_pk = get_register(regs, "R4")
        .and_then(|v| parse_group_element(v))
        .context(format!(
            "Missing or invalid R4 (provider PK) in GPU listing box {}",
            raw.box_id
        ))?;

    let gpu_type = get_register(regs, "R5")
        .and_then(|v| parse_coll_byte(v))
        .context(format!(
            "Missing or invalid R5 (gpu_type) in GPU listing box {}",
            raw.box_id
        ))?;

    let vram_gb = get_register(regs, "R6")
        .and_then(|v| parse_int(v))
        .context(format!(
            "Missing or invalid R6 (vram_gb) in GPU listing box {}",
            raw.box_id
        ))?;

    let price_per_hour_nanoerg = get_register(regs, "R7")
        .and_then(|v| parse_long(v))
        .context(format!(
            "Missing or invalid R7 (price_per_hour) in GPU listing box {}",
            raw.box_id
        ))?;

    let region = get_register(regs, "R8")
        .and_then(|v| parse_coll_byte(v))
        .unwrap_or_else(|| "unknown".to_string());

    let available_int = get_register(regs, "R9")
        .and_then(|v| parse_int(v))
        .unwrap_or(0);
    let available = available_int == 1;

    Ok(GpuListingBox {
        box_id: raw.box_id.clone(),
        tx_id: raw.tx_id.clone(),
        listing_nft_id: listing_nft_id.clone(),
        provider_pk,
        gpu_type,
        vram_gb,
        price_per_hour_nanoerg: price_per_hour_nanoerg as u64,
        region,
        available,
        value_nanoerg: raw.value,
        creation_height: raw.creation_height,
    })
}

/// Validate that a RawBox matches the GPU Rental Box specification.
///
/// Checks:
/// - At least one token present (Rental NFT, supply=1)
/// - Minimum ERG value
/// - Required registers R4-R9
pub fn validate_gpu_rental_box(raw: &RawBox) -> Result<GpuRentalBox> {
    if raw.value < MIN_RENTAL_BOX_VALUE {
        anyhow::bail!(
            "GPU rental box {} has insufficient value: {} < {}",
            raw.box_id,
            raw.value,
            MIN_RENTAL_BOX_VALUE
        );
    }

    if raw.assets.is_empty() {
        anyhow::bail!(
            "GPU rental box {} has no tokens (expected Rental NFT)",
            raw.box_id
        );
    }

    let rental_nft_id = &raw.assets[0].token_id;
    if raw.assets[0].amount != 1 {
        anyhow::bail!(
            "GPU rental box {} NFT has amount {} (expected exactly 1)",
            raw.box_id,
            raw.assets[0].amount
        );
    }

    let regs = &raw.additional_registers;

    let provider_pk = get_register(regs, "R4")
        .and_then(|v| parse_group_element(v))
        .context(format!(
            "Missing or invalid R4 (provider PK) in GPU rental box {}",
            raw.box_id
        ))?;

    let renter_pk = get_register(regs, "R5")
        .and_then(|v| parse_group_element(v))
        .context(format!(
            "Missing or invalid R5 (renter PK) in GPU rental box {}",
            raw.box_id
        ))?;

    let deadline_height = get_register(regs, "R6")
        .and_then(|v| parse_int(v))
        .context(format!(
            "Missing or invalid R6 (deadline) in GPU rental box {}",
            raw.box_id
        ))?;

    let listing_box_id = get_register(regs, "R7")
        .and_then(|v| parse_coll_byte(v))
        .context(format!(
            "Missing or invalid R7 (listing_box_id) in GPU rental box {}",
            raw.box_id
        ))?;

    let rental_start_height = get_register(regs, "R8")
        .and_then(|v| parse_int(v))
        .unwrap_or(0);

    let hours_rented = get_register(regs, "R9")
        .and_then(|v| parse_int(v))
        .unwrap_or(0);

    Ok(GpuRentalBox {
        box_id: raw.box_id.clone(),
        tx_id: raw.tx_id.clone(),
        rental_nft_id: rental_nft_id.clone(),
        provider_pk,
        renter_pk,
        deadline_height,
        listing_box_id,
        rental_start_height,
        hours_rented,
        value_nanoerg: raw.value,
        creation_height: raw.creation_height,
    })
}

/// Filter listings by criteria from a BrowseListingsRequest.
pub fn filter_listings(
    listings: Vec<GpuListingBox>,
    min_vram_gb: Option<i32>,
    max_price_per_hour: Option<u64>,
    region: Option<&str>,
    gpu_type_contains: Option<&str>,
) -> Vec<GpuListingBox> {
    listings
        .into_iter()
        .filter(|l| {
            // Only show available listings
            if !l.available {
                return false;
            }
            if let Some(min_vram) = min_vram_gb {
                if l.vram_gb < min_vram {
                    return false;
                }
            }
            if let Some(max_price) = max_price_per_hour {
                if l.price_per_hour_nanoerg > max_price {
                    return false;
                }
            }
            if let Some(r) = region {
                if !l.region.eq_ignore_ascii_case(r) {
                    return false;
                }
            }
            if let Some(substr) = gpu_type_contains {
                if !l.gpu_type.to_lowercase().contains(&substr.to_lowercase()) {
                    return false;
                }
            }
            true
        })
        .collect()
}
