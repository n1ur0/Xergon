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

/// Represents a parsed Governance Proposal Box.
/// Maps to contracts/governance_proposal.ergo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceProposalBox {
    pub box_id: String,
    pub tx_id: String,
    /// Governance NFT token ID (hex)
    pub gov_nft_id: String,
    /// Total number of proposals submitted
    pub proposal_count: i32,
    /// Currently active proposal ID (0 = none)
    pub active_proposal_id: i32,
    /// Voting threshold (e.g., 51)
    pub voting_threshold: i32,
    /// Total eligible voters
    pub total_voters: i32,
    /// Block height when the active proposal ends
    pub proposal_end_height: i32,
    /// Blake2b256 hash of proposal data
    pub proposal_data_hash: String,
    /// ERG value in the box (nanoERGs)
    pub value: String,
    /// Creation block height
    pub creation_height: i32,
}

/// Represents a parsed Provider Slashing Box.
/// Maps to contracts/provider_slashing.ergo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSlashingBox {
    pub box_id: String,
    pub tx_id: String,
    /// Slash Token ID (hex)
    pub slash_token_id: String,
    /// Provider public key (hex encoded GroupElement bytes)
    pub provider_pk: String,
    /// Minimum uptime percentage required (e.g., 95)
    pub min_uptime_percent: i32,
    /// Staked amount in nanoERG
    pub stake_amount: i64,
    /// Block height when the challenge window ends
    pub challenge_window_end: i32,
    /// Slashed flag: 0 = active, 1 = slashed
    pub slashed_flag: i32,
    /// ERG value in the box (nanoERGs)
    pub value: String,
    /// Creation block height
    pub creation_height: i32,
}

/// Represents a parsed Treasury Box.
/// Maps to contracts/treasury.ergo
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreasuryBox {
    pub box_id: String,
    pub tx_id: String,
    /// Xergon Network NFT token ID (hex)
    pub network_nft_id: String,
    /// Cumulative nanoERG distributed via airdrops
    pub total_airdropped: i64,
    /// ERG value in the box (nanoERGs)
    pub value: String,
    /// Creation block height
    pub creation_height: i32,
}

/// Represents a parsed Payment Bridge Box.
/// Maps to contracts/payment_bridge.es
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentBridgeBox {
    pub box_id: String,
    pub tx_id: String,
    /// Invoice NFT token ID (hex)
    pub invoice_nft_id: String,
    /// Buyer public key (hex of SigmaProp propositionBytes)
    pub buyer_pk_hex: String,
    /// Provider public key (hex of SigmaProp propositionBytes)
    pub provider_pk_hex: String,
    /// Payment amount in nanoERG
    pub amount_nanoerg: i64,
    /// Foreign chain transaction ID (empty if not confirmed)
    pub foreign_tx_id: String,
    /// Foreign chain: 0=BTC, 1=ETH, 2=ADA
    pub foreign_chain: i32,
    /// Bridge public key (hex of SigmaProp propositionBytes)
    pub bridge_pk_hex: String,
    /// ERG value in the box (nanoERGs)
    pub value: String,
    /// Creation block height
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
