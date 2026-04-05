//! GPU Bazar types.
//!
//! On-chain box types (GpuListingBox, GpuRentalBox) and API request/response
//! types for the GPU rental marketplace.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// On-chain box types
// ---------------------------------------------------------------------------

/// Represents a parsed GPU Listing Box from the UTXO set.
/// Maps to the ErgoScript contract in contracts/gpu_rental_listing.es
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuListingBox {
    /// Box ID (hex)
    pub box_id: String,
    /// Transaction ID that created this box
    pub tx_id: String,
    /// Listing NFT token ID (hex)
    pub listing_nft_id: String,
    /// Provider public key (hex encoded GroupElement bytes)
    pub provider_pk: String,
    /// GPU model name (e.g. "RTX 4090", "A100 80GB")
    pub gpu_type: String,
    /// VRAM in GB
    pub vram_gb: i32,
    /// Price per hour in nanoERG
    pub price_per_hour_nanoerg: u64,
    /// Provider region (e.g. "us-east", "eu-west")
    pub region: String,
    /// Whether this GPU is currently available for rent
    pub available: bool,
    /// ERG value in the box (nanoERGs)
    pub value_nanoerg: u64,
    /// Creation block height
    pub creation_height: i32,
}

/// Represents a parsed GPU Rental Box from the UTXO set.
/// Maps to the ErgoScript contract in contracts/gpu_rental.es
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuRentalBox {
    /// Box ID (hex)
    pub box_id: String,
    /// Transaction ID that created this box
    pub tx_id: String,
    /// Rental NFT token ID (hex)
    pub rental_nft_id: String,
    /// Provider public key (hex)
    pub provider_pk: String,
    /// Renter public key (hex)
    pub renter_pk: String,
    /// Block height when the rental period ends
    pub deadline_height: i32,
    /// Reference to the listing box ID (hex)
    pub listing_box_id: String,
    /// Block height when rental started
    pub rental_start_height: i32,
    /// Total hours rented
    pub hours_rented: i32,
    /// ERG value in the box (nanoERGs) — this IS the escrowed payment
    pub value_nanoerg: u64,
    /// Creation block height
    pub creation_height: i32,
}

// ---------------------------------------------------------------------------
// API request/response types
// ---------------------------------------------------------------------------

/// Request to create a new GPU listing.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateListingRequest {
    /// GPU model name (e.g. "RTX 4090")
    pub gpu_type: String,
    /// VRAM in GB
    pub vram_gb: i32,
    /// Price per hour in nanoERG
    pub price_per_hour_nanoerg: u64,
    /// Provider region (e.g. "us-east")
    pub region: String,
    /// Provider's Ergo address (where payment should go on claim)
    pub provider_address: String,
}

/// Response after creating a listing.
#[derive(Debug, Clone, Serialize)]
pub struct CreateListingResponse {
    /// Transaction ID of the listing creation tx
    pub tx_id: String,
    /// Box ID of the new listing box
    pub box_id: String,
    /// Listing NFT token ID
    pub listing_nft_id: String,
    /// Status
    pub status: String,
}

/// Request to browse/filter available GPU listings.
#[derive(Debug, Clone, Deserialize)]
pub struct BrowseListingsRequest {
    /// Filter by minimum VRAM (GB)
    pub min_vram_gb: Option<i32>,
    /// Filter by maximum price per hour (nanoERG)
    pub max_price_per_hour: Option<u64>,
    /// Filter by region
    pub region: Option<String>,
    /// Filter by GPU type substring
    pub gpu_type_contains: Option<String>,
}

/// Response with available GPU listings.
#[derive(Debug, Clone, Serialize)]
pub struct BrowseListingsResponse {
    /// Available GPU listings
    pub listings: Vec<GpuListingBox>,
    /// Current block height
    pub current_height: i32,
}

/// Request to initiate a GPU rental.
#[derive(Debug, Clone, Deserialize)]
pub struct RentGpuRequest {
    /// Box ID of the GPU listing to rent
    pub listing_box_id: String,
    /// Number of hours to rent
    pub hours: i32,
    /// Renter's Ergo address (for refund path)
    pub renter_address: String,
}

/// Response after initiating a rental.
#[derive(Debug, Clone, Serialize)]
pub struct RentGpuResponse {
    /// Transaction ID of the rental creation tx
    pub tx_id: String,
    /// Box ID of the new rental box
    pub box_id: String,
    /// Rental NFT token ID
    pub rental_nft_id: String,
    /// Deadline block height
    pub deadline_height: i32,
    /// Total cost in nanoERG
    pub cost_nanoerg: u64,
    /// Status
    pub status: String,
}

/// Request to claim payment from a completed rental.
#[derive(Debug, Clone, Deserialize)]
pub struct ClaimRentalRequest {
    /// Box ID of the rental box to claim
    pub rental_box_id: String,
    /// Provider's Ergo address to receive payment
    pub provider_address: String,
}

/// Request to refund a cancelled rental.
#[derive(Debug, Clone, Deserialize)]
pub struct RefundRentalRequest {
    /// Box ID of the rental box to refund
    pub rental_box_id: String,
    /// Renter's Ergo address to receive refund
    pub renter_address: String,
}

/// Request to extend an active rental.
#[derive(Debug, Clone, Deserialize)]
pub struct ExtendRentalRequest {
    /// Box ID of the rental box to extend
    pub rental_box_id: String,
    /// Additional hours to add
    pub additional_hours: i32,
}

/// Response for claim/refund/extend operations.
#[derive(Debug, Clone, Serialize)]
pub struct RentalActionResponse {
    /// Transaction ID
    pub tx_id: String,
    /// Status message
    pub status: String,
}

/// Response for the user's active rentals.
#[derive(Debug, Clone, Serialize)]
pub struct MyRentalsResponse {
    /// Active rental boxes
    pub rentals: Vec<GpuRentalBox>,
    /// Current block height
    pub current_height: i32,
}

/// Response for the user's listings.
#[derive(Debug, Clone, Serialize)]
pub struct MyListingsResponse {
    /// Listing boxes owned by this provider
    pub listings: Vec<GpuListingBox>,
    /// Current block height
    pub current_height: i32,
}

/// Error response for GPU rental operations.
#[derive(Debug, Clone, Serialize)]
pub struct GpuRentalError {
    pub error: String,
    pub code: String,
}
