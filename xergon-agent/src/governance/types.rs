//! On-chain governance types for the Xergon protocol.
//!
//! These types model the eUTXO-based governance system where proposals,
//! votes, and configuration live on the Ergo blockchain in boxes with
//! registers (R4-R9) and singleton NFTs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Proposal lifecycle stages (eUTXO multi-stage pattern)
// ---------------------------------------------------------------------------

/// Proposal lifecycle stages following the eUTXO multi-stage pattern.
///
/// Each stage maps to a specific register value (R4) in the proposal box.
/// State transitions occur by spending the current box and creating a
/// successor box with updated registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProposalStage {
    /// Initial box created, voting open. (R4 = 0)
    Created,
    /// Votes being collected. (R4 = 1)
    Voting,
    /// Quorum reached, action taken. (R4 = 2)
    Executed,
    /// Proposal finalized (passed or failed). (R4 = 3)
    Closed,
    /// Voting period ended without quorum. (R4 = 4)
    Expired,
}

impl ProposalStage {
    /// Convert to the integer register value stored in R4.
    pub fn to_register_value(self) -> u32 {
        match self {
            Self::Created => 0,
            Self::Voting => 1,
            Self::Executed => 2,
            Self::Closed => 3,
            Self::Expired => 4,
        }
    }

    /// Parse from a register value.
    pub fn from_register_value(val: u32) -> Result<Self, GovernanceError> {
        match val {
            0 => Ok(Self::Created),
            1 => Ok(Self::Voting),
            2 => Ok(Self::Executed),
            3 => Ok(Self::Closed),
            4 => Ok(Self::Expired),
            _ => Err(GovernanceError::InvalidStage(val)),
        }
    }

    /// Returns true if the proposal is in a terminal (finalized) stage.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Executed | Self::Closed | Self::Expired)
    }

    /// Returns true if the proposal is currently accepting votes.
    pub fn is_voting(&self) -> bool {
        matches!(self, Self::Created | Self::Voting)
    }
}

impl std::fmt::Display for ProposalStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Created => write!(f, "created"),
            Self::Voting => write!(f, "voting"),
            Self::Executed => write!(f, "executed"),
            Self::Closed => write!(f, "closed"),
            Self::Expired => write!(f, "expired"),
        }
    }
}

// ---------------------------------------------------------------------------
// Proposal categories
// ---------------------------------------------------------------------------

/// Proposal categories determine the type of action and validation rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProposalCategory {
    /// Change protocol parameters (fees, limits, timeouts).
    ProtocolParam,
    /// Add/remove/suspend provider.
    ProviderAction,
    /// Spend treasury funds.
    TreasurySpend,
    /// Upgrade smart contract.
    ContractUpgrade,
    /// Emergency pause/circuit breaker.
    Emergency,
    /// General configuration change.
    ConfigChange,
}

impl ProposalCategory {
    /// Returns true if this category bypasses normal stake requirements
    /// (e.g., emergency proposals can be created by council members).
    pub fn is_emergency(&self) -> bool {
        matches!(self, Self::Emergency)
    }

    /// Returns true if this category involves fund movement.
    pub fn involves_funds(&self) -> bool {
        matches!(self, Self::TreasurySpend)
    }
}

impl std::fmt::Display for ProposalCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProtocolParam => write!(f, "protocol_param"),
            Self::ProviderAction => write!(f, "provider_action"),
            Self::TreasurySpend => write!(f, "treasury_spend"),
            Self::ContractUpgrade => write!(f, "contract_upgrade"),
            Self::Emergency => write!(f, "emergency"),
            Self::ConfigChange => write!(f, "config_change"),
        }
    }
}

impl std::str::FromStr for ProposalCategory {
    type Err = GovernanceError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "protocol_param" => Ok(Self::ProtocolParam),
            "provider_action" => Ok(Self::ProviderAction),
            "treasury_spend" => Ok(Self::TreasurySpend),
            "contract_upgrade" => Ok(Self::ContractUpgrade),
            "emergency" => Ok(Self::Emergency),
            "config_change" => Ok(Self::ConfigChange),
            _ => Err(GovernanceError::InvalidCategory(s.to_string())),
        }
    }
}

// ---------------------------------------------------------------------------
// On-chain proposal box representation
// ---------------------------------------------------------------------------

/// On-chain proposal box representation.
///
/// Maps to an Ergo box with:
/// - R4: ProposalStage (Int)
/// - R5: vote_end_height (Int)
/// - R6: votes_for (Long)
/// - R7: votes_against (Long)
/// - R8: quorum_threshold (Int)
/// - R9: approval_threshold (Int)
/// - Tokens[0]: singleton proposal NFT (preserved through all transitions)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnChainProposal {
    /// The Ergo box ID.
    pub box_id: String,
    /// Unique proposal identifier (derived from creation tx).
    pub proposal_id: String,
    /// Human-readable title.
    pub title: String,
    /// Detailed description.
    pub description: String,
    /// Proposal category.
    pub category: ProposalCategory,
    /// Current lifecycle stage.
    pub stage: ProposalStage,
    /// Creator's public key (hex).
    pub creator_pk: String,
    /// Block height when the proposal box was created.
    pub created_height: u32,
    /// Block height when voting starts.
    pub vote_start_height: u32,
    /// Block height when voting ends.
    pub vote_end_height: u32,
    /// Minimum total votes needed for the proposal to be valid.
    pub quorum_threshold: u32,
    /// Percentage of "for" votes needed to pass (0-100).
    pub approval_threshold: u32,
    /// Current vote tally for.
    pub votes_for: u64,
    /// Current vote tally against.
    pub votes_against: u64,
    /// Unique voter public keys (deduplication).
    pub voters: Vec<String>,
    /// Serialized execution parameters (for treasury spend, etc.).
    pub execution_data: Option<Vec<u8>>,
    /// Singleton NFT token ID that identifies this proposal box.
    pub nft_token_id: String,
    /// ERG value in the box (nanoERG).
    pub erg_value: u64,
}

impl OnChainProposal {
    /// Total number of votes cast.
    pub fn total_votes(&self) -> u64 {
        self.votes_for + self.votes_against
    }

    /// Whether the quorum threshold has been met.
    pub fn meets_quorum(&self) -> bool {
        self.total_votes() >= self.quorum_threshold as u64
    }

    /// Whether the approval threshold has been met (assuming quorum met).
    pub fn meets_approval(&self) -> bool {
        let total = self.total_votes();
        if total == 0 {
            return false;
        }
        (self.votes_for * 100) / total >= self.approval_threshold as u64
    }

    /// Whether this voter has already voted.
    pub fn has_voted(&self, voter_pk: &str) -> bool {
        self.voters.iter().any(|v| v == voter_pk)
    }
}

// ---------------------------------------------------------------------------
// Governance config (singleton box, read via data input)
// ---------------------------------------------------------------------------

/// Governance configuration stored in a singleton box on-chain.
///
/// This box is read via data inputs (CONTEXT.dataInputs) so it is never spent
/// during normal governance operations. It can only be updated by executing
/// a governance proposal that targets config changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceConfig {
    /// Singleton NFT that identifies the config box.
    pub config_nft_id: String,
    /// Minimum ERG required to create a proposal box.
    pub min_proposal_erg: u64,
    /// Default voting period duration in blocks.
    pub default_vote_duration: u32,
    /// Default quorum threshold (minimum votes).
    pub default_quorum: u32,
    /// Default approval threshold (percentage, 0-100).
    pub default_approval: u32,
    /// Minimum stake (in nanoERG) required to create a proposal.
    pub min_stake_to_propose: u64,
    /// Minimum stake (in nanoERG) required to vote.
    pub min_stake_to_vote: u64,
    /// Maximum proposal title length (characters).
    pub max_proposal_title_len: u32,
    /// Maximum proposal description length (characters).
    pub max_proposal_desc_len: u32,
    /// Emergency council public keys (can create emergency proposals without stake).
    pub emergency_council_pks: Vec<String>,
    /// Running count of proposals created.
    pub proposal_count: u64,
}

impl Default for GovernanceConfig {
    fn default() -> Self {
        Self {
            config_nft_id: String::new(),
            min_proposal_erg: 1_000_000_000, // 1 ERG
            default_vote_duration: 10_000,    // ~35 days at 2-min blocks
            default_quorum: 10,
            default_approval: 60,
            min_stake_to_propose: 100_000_000, // 0.1 ERG
            min_stake_to_vote: 1_000_000,      // 0.001 ERG
            max_proposal_title_len: 200,
            max_proposal_desc_len: 5000,
            emergency_council_pks: vec![],
            proposal_count: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Vote record
// ---------------------------------------------------------------------------

/// A single vote record on-chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteRecord {
    /// Voter's public key (hex).
    pub voter_pk: String,
    /// Proposal ID being voted on.
    pub proposal_id: String,
    /// true = for, false = against.
    pub support: bool,
    /// Stake-weighted voting power.
    pub voting_power: u64,
    /// Block height when the vote was cast.
    pub height: u32,
    /// Transaction ID of the vote.
    pub tx_id: String,
}

// ---------------------------------------------------------------------------
// Transaction building result
// ---------------------------------------------------------------------------

/// Result of building a governance transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceTxResult {
    /// Transaction ID (hash).
    pub tx_id: String,
    /// Serialized Ergo transaction JSON.
    pub tx_json: String,
    /// Box IDs created by this transaction.
    pub boxes_created: Vec<String>,
    /// Box IDs spent by this transaction.
    pub boxes_spent: Vec<String>,
    /// Transaction fee in nanoERG.
    pub fee: u64,
}

// ---------------------------------------------------------------------------
// Validation result
// ---------------------------------------------------------------------------

/// Result of validating a proposal against governance rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Whether the proposal is valid.
    pub is_valid: bool,
    /// Validation errors (empty if valid).
    pub errors: Vec<String>,
    /// Validation warnings (non-blocking).
    pub warnings: Vec<String>,
}

impl ValidationResult {
    /// Create a passing validation result.
    pub fn valid() -> Self {
        Self {
            is_valid: true,
            errors: vec![],
            warnings: vec![],
        }
    }

    /// Create a failing validation result with errors.
    pub fn invalid(errors: Vec<String>) -> Self {
        Self {
            is_valid: false,
            errors,
            warnings: vec![],
        }
    }

    /// Add a warning to an existing result.
    pub fn with_warning(mut self, warning: &str) -> Self {
        self.warnings.push(warning.to_string());
        self
    }
}

// ---------------------------------------------------------------------------
// Tally result
// ---------------------------------------------------------------------------

/// Result of tallying votes on a proposal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TallyResult {
    /// Total votes for.
    pub votes_for: u64,
    /// Total votes against.
    pub votes_against: u64,
    /// Total unique voters.
    pub total_voters: u32,
    /// Whether quorum is met.
    pub quorum_met: bool,
    /// Whether approval threshold is met (only meaningful if quorum met).
    pub approval_met: bool,
    /// Whether the proposal passes overall.
    pub passes: bool,
    /// Approval percentage (votes_for / total * 100).
    pub approval_percentage: f64,
    /// Quorum percentage (total / quorum_threshold * 100).
    pub quorum_percentage: f64,
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Governance error type.
#[derive(Debug, Clone, thiserror::Error)]
pub enum GovernanceError {
    #[error("Invalid proposal stage: {0}")]
    InvalidStage(u32),

    #[error("Invalid proposal category: {0}")]
    InvalidCategory(String),

    #[error("Proposal validation failed: {0}")]
    ValidationFailed(String),

    #[error("Proposal not found: {0}")]
    ProposalNotFound(String),

    #[error("Voting period ended (height {current} > end_height {end})")]
    VotingPeriodEnded { current: u32, end: u32 },

    #[error("Voting not yet started (height {current} < start_height {start})")]
    VotingNotStarted { current: u32, start: u32 },

    #[error("Proposal is in terminal stage: {0}")]
    ProposalFinalized(String),

    #[error("Voter has already voted: {0}")]
    AlreadyVoted(String),

    #[error("Quorum not met: {votes} votes < threshold {threshold}")]
    QuorumNotMet { votes: u64, threshold: u32 },

    #[error("Approval threshold not met: {percentage}% < required {required}%")]
    ApprovalNotMet { percentage: u64, required: u32 },

    #[error("Insufficient stake to propose: {stake} < required {required}")]
    InsufficientStakeToPropose { stake: u64, required: u64 },

    #[error("Insufficient stake to vote: {stake} < required {required}")]
    InsufficientStakeToVote { stake: u64, required: u64 },

    #[error("Title exceeds max length: {len} > {max}")]
    TitleTooLong { len: u32, max: u32 },

    #[error("Description exceeds max length: {len} > {max}")]
    DescriptionTooLong { len: u32, max: u32 },

    #[error("Template not found: {0}")]
    TemplateNotFound(String),

    #[error("Template parameter error: {0}")]
    TemplateParamError(String),

    #[error("Transaction build error: {0}")]
    TxBuildError(String),
}

/// Helper: serialize a map of parameter values to a JSON string.
pub fn serialize_params(params: &HashMap<String, serde_json::Value>) -> Vec<u8> {
    serde_json::to_vec(params).unwrap_or_default()
}
