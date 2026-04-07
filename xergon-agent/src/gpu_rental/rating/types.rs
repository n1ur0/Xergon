//! GPU Bazar rating types.
//!
//! On-chain box type (GpuRatingBox) and API request/response types for the
//! GPU rental reputation system.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// On-chain box types
// ---------------------------------------------------------------------------

/// Represents a parsed GPU Rating Box from the UTXO set.
/// Maps to the ErgoScript contract in contracts/gpu_rating.es
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuRatingBox {
    /// Box ID (hex)
    pub box_id: String,
    /// Transaction ID that created this box
    pub tx_id: String,
    /// Rater public key (hex) -- who submitted the rating
    pub rater_pk: String,
    /// Rated public key (hex) -- who is being rated
    pub rated_pk: String,
    /// Role of the rated person: "provider" or "renter"
    pub role: String,
    /// Rental box ID this rating is for (hex)
    pub rental_box_id: String,
    /// Rating value: 1-5 stars
    pub rating: i32,
    /// Blake2b256 hash of optional comment (hex)
    pub comment_hash: String,
    /// ERG value in the box (nanoERGs)
    pub value_nanoerg: u64,
    /// Creation block height
    pub creation_height: i32,
}

/// Aggregated reputation score for a public key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reputation {
    /// Public key being rated (hex)
    pub public_key: String,
    /// Number of ratings received
    pub total_ratings: usize,
    /// Average rating (1.0 - 5.0)
    pub average_rating: f64,
    /// Number of 1-star ratings
    pub one_star: usize,
    /// Number of 2-star ratings
    pub two_star: usize,
    /// Number of 3-star ratings
    pub three_star: usize,
    /// Number of 4-star ratings
    pub four_star: usize,
    /// Number of 5-star ratings
    pub five_star: usize,
    /// Provider reputation (average of ratings where role == "provider")
    pub provider_reputation: Option<f64>,
    /// Provider rating count
    pub provider_rating_count: usize,
    /// Renter reputation (average of ratings where role == "renter")
    pub renter_reputation: Option<f64>,
    /// Renter rating count
    pub renter_rating_count: usize,
}

// ---------------------------------------------------------------------------
// API request/response types
// ---------------------------------------------------------------------------

/// Request to submit a GPU rental rating.
#[derive(Debug, Clone, Deserialize)]
pub struct SubmitRatingRequest {
    /// Rental box ID that this rating is for (hex)
    pub rental_box_id: String,
    /// Public key of the person being rated (hex)
    pub rated_pk: String,
    /// Public key of the person submitting the rating (hex)
    pub rater_pk: String,
    /// Role of the rated person: "provider" or "renter"
    pub role: String,
    /// Rating value: 1-5 stars
    pub rating: i32,
    /// Optional comment (will be hashed on-chain, stored off-chain)
    pub comment: Option<String>,
}

/// Response after submitting a rating.
#[derive(Debug, Clone, Serialize)]
pub struct SubmitRatingResponse {
    /// Transaction ID of the rating submission tx
    pub tx_id: String,
    /// Box ID of the new rating box
    pub box_id: String,
    /// Status
    pub status: String,
}

/// Response for reputation query.
#[derive(Debug, Clone, Serialize)]
pub struct ReputationResponse {
    /// Aggregated reputation data
    pub reputation: Reputation,
}
