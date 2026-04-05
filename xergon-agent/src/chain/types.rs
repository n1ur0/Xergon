//! On-chain box types (parsed from Ergo node API responses).
//!
//! These structs mirror the on-chain box structures used by the Xergon protocol
//! contracts: Provider Box, User Staking Box, and Usage Proof Box.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents a parsed Provider Box from the UTXO set.
/// Maps to the ErgoScript contract in contracts/provider_box.ergo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderBox {
    /// Box ID (hex)
    pub box_id: String,
    /// Transaction ID that created this box
    pub tx_id: String,
    /// Provider NFT token ID (hex)
    pub provider_nft_id: String,
    /// Provider public key (hex encoded GroupElement bytes)
    pub provider_pk: String,
    /// Provider endpoint URL (e.g., "http://192.168.1.5:9099")
    pub endpoint: String,
    /// Models served (parsed from JSON in register)
    pub models: Vec<String>,
    /// Per-model pricing: model_id -> nanoERG per 1M tokens (R6 register, structured JSON)
    /// Models from old-format R6 (plain array) default to price 0 (free tier).
    pub model_pricing: HashMap<String, u64>,
    /// PoNW score (0-1000)
    pub pown_score: i32,
    /// Last heartbeat block height
    pub last_heartbeat: i32,
    /// Provider region (e.g., "us-east")
    pub region: String,
    /// ERG value in the box (nanoERGs)
    pub value: String,
    /// Creation block height
    pub creation_height: i32,
    /// Whether this provider is considered active
    /// (heartbeat within last 100 blocks)
    pub is_active: bool,
}

/// Represents a parsed User Staking Box.
/// Maps to contracts/user_staking.ergo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserStakingBox {
    pub box_id: String,
    pub tx_id: String,
    /// User public key (hex)
    pub user_pk: String,
    /// ERG balance (nanoERGs) -- the box value IS the balance
    pub balance_nanoerg: u64,
    pub creation_height: i32,
}

/// Represents a parsed Usage Proof Box.
/// Maps to contracts/usage_proof.ergo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageProofBox {
    pub box_id: String,
    pub tx_id: String,
    /// Hash of user's public key
    pub user_pk_hash: String,
    /// Provider NFT ID that served the request
    pub provider_nft_id: String,
    /// Model name used
    pub model: String,
    /// Token count in the response
    pub token_count: i32,
    /// Request timestamp (Unix ms)
    pub timestamp: i64,
    pub creation_height: i32,
}

/// A raw box from the Ergo node API (before parsing registers).
#[derive(Debug, Clone, Deserialize)]
pub struct RawBox {
    #[serde(rename = "boxId")]
    pub box_id: String,
    #[serde(rename = "transactionId")]
    pub tx_id: String,
    #[serde(default)]
    pub value: u64,
    #[serde(default)]
    pub creation_height: i32,
    #[serde(default)]
    pub assets: Vec<RawAsset>,
    #[serde(default)]
    pub additional_registers: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub ergo_tree: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawAsset {
    #[serde(rename = "tokenId")]
    pub token_id: String,
    pub amount: u64,
    pub name: Option<String>,
    pub decimals: Option<i32>,
}

// Re-export GPU Bazar box types from the gpu_rental module for convenience.
pub use crate::gpu_rental::types::GpuListingBox;
pub use crate::gpu_rental::types::GpuRentalBox;
