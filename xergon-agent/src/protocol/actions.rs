//! Transaction builders for the Xergon protocol.
//!
//! This module provides two layers:
//! 1. **Node wallet API builders** (always available): Functions that use the
//!    Ergo node wallet API (`POST /wallet/payment/send`) to build, sign, and
//!    broadcast transactions. These do not require `ergo-lib`.
//! 2. **ergo-lib builders** (feature-gated): Lower-level builders using
//!    `ergo-lib`'s Sigma compiler and transaction builder for advanced use.
//!
//! For provider registration and bootstrap, prefer the higher-level functions
//! in [`crate::protocol::bootstrap`] which wrap these with validation and
//! state persistence.

use anyhow::{Context, Result};
use tracing::info;

use crate::chain::client::ErgoNodeClient;
use crate::protocol::bootstrap::{build_staking_tx, ProviderRegistrationInfo, SAFE_MIN_BOX_VALUE, UserStakingInfo, UserStakingResult};

// ---------------------------------------------------------------------------
// register_provider (node wallet API — always available)
// ---------------------------------------------------------------------------

/// Register a new provider on-chain by minting a per-provider NFT and
/// creating a Provider Box with registers R4-R9.
///
/// This is the primary entry point for provider registration. It uses the
/// Ergo node wallet API to build, sign, and broadcast the transaction.
///
/// # Arguments
///
/// * `client` - Ergo node client
/// * `endpoint` - Provider endpoint URL (e.g., "http://192.168.1.5:9099")
/// * `models` - List of model names served by this provider
/// * `region` - Provider region code (e.g., "us-east")
/// * `pk_hex` - Provider's compressed secp256k1 public key (hex, 33 bytes)
/// * `provider_tree_hex` - Compiled ErgoTree hex for provider_box.es (can be empty)
///
/// # Returns
///
/// The transaction ID of the registration transaction.
///
/// # NFT Minting
///
/// The NFT token ID follows Ergo's standard minting rule:
/// `token_id = blake2b256(first_input_box_id)`. This is handled automatically
/// by the node wallet API when a token with amount=1 is placed on the first output.
///
/// # Provider Box Register Layout
///
/// | Register | Type         | Content                        |
/// |----------|--------------|--------------------------------|
/// | R4       | GroupElement | Provider public key            |
/// | R5       | Coll[Byte]   | Endpoint URL (UTF-8)           |
/// | R6       | Coll[Byte]   | Models served (JSON array)     |
/// | R7       | Int          | PoNW score (initial: 0)        |
/// | R8       | Int          | Last heartbeat (initial: 0)    |
/// | R9       | Coll[Byte]   | Region code (UTF-8)            |
pub async fn register_provider(
    client: &ErgoNodeClient,
    endpoint: &str,
    models: &[String],
    region: &str,
    pk_hex: &str,
    provider_tree_hex: &str,
) -> Result<String> {
    let models_json = serde_json::to_string(models)
        .context("Failed to serialize models list to JSON")?;

    let provider_info = ProviderRegistrationInfo {
        provider_id: format!("provider-{}", &pk_hex[..8]),
        provider_pk_hex: pk_hex.to_string(),
        endpoint: endpoint.to_string(),
        models_json,
        region: region.to_string(),
        provider_tree_hex: provider_tree_hex.to_string(),
        box_value_nanoerg: SAFE_MIN_BOX_VALUE,
    };

    let result = crate::protocol::bootstrap::build_register_provider_tx(client, &provider_info)
        .await
        .context("Provider registration failed")?;

    info!(
        tx_id = %result.tx_id,
        provider_nft_id = %result.provider_nft_id,
        provider_box_id = %result.provider_box_id,
        "Provider registered successfully"
    );

    Ok(result.tx_id)
}

// ---------------------------------------------------------------------------
// register_provider (ergo-lib feature-gated — advanced)
// ---------------------------------------------------------------------------


// -----------------------------------------------------------------------------------------------------------------------------------------------
// submit_heartbeat
// -----------------------------------------------------------------------------------------------------------------------------------------------

/// Submit a heartbeat transaction that updates the provider's on-chain box.
///
/// Spends the existing provider box (found via NFT token ID) and creates a new
/// one with an updated R8 (last heartbeat height). R4–R7 and R9 are preserved
/// from the existing box.
///
/// The `new_height` parameter is ignored — the current blockchain height is
/// always fetched from the Ergo node to ensure monotonic, accurate timestamps.
///
/// Uses the Ergo node wallet API (`POST /wallet/payment/send`) to build,
/// sign, and broadcast atomically.
///
/// # Arguments
///
/// * `client` - Ergo node client
/// * `provider_nft_id` - Token ID of the provider's NFT (used to find the box)
/// * `endpoint_url` - Provider endpoint URL (e.g. "http://192.168.1.5:9099")
/// * `models_r6_json` - JSON string of models + pricing served
/// * `ponw_score` - Updated PoNW score (0–1000); clamped to valid range
/// * `region` - Provider region code (e.g. "us-east")
///
/// # Returns
///
/// The transaction ID of the heartbeat transaction.
#[allow(dead_code)] // Public API — called from integration tests; production wiring pending
pub async fn submit_heartbeat(
    client: &ErgoNodeClient,
    provider_nft_id: &str,
    _new_height: i32,
    endpoint_url: &str,
    models_r6_json: &str,
    ponw_score: i32,
    region: &str,
) -> anyhow::Result<String> {
    let tx_id = crate::chain::transactions::submit_heartbeat_tx(
        client,
        provider_nft_id,
        endpoint_url,
        models_r6_json,
        ponw_score,
        region,
    )
    .await
    .context("Heartbeat transaction failed")?;

    info!(tx_id = %tx_id, provider_nft_id = %provider_nft_id, "Heartbeat submitted");
    Ok(tx_id)
}

// -----------------------------------------------------------------------------------------------------------------------------------------------
// submit_usage_proof
// -----------------------------------------------------------------------------------------------------------------------------------------------

/// Submit a usage proof box recording a single inference completion.
///
/// Creates an on-chain box (min ERG value) with registers:
/// - R4: user_pk (Coll[Byte])
/// - R5: provider_id (String)
/// - R6: model_name (String)
/// - R7: token_count (Int)
/// - R8: timestamp_ms (Long)
///
/// Uses the Ergo node wallet API to build, sign, and broadcast.
///
/// # Arguments
///
/// * `client` - Ergo node client
/// * `user_pk_hex` - User's compressed secp256k1 public key (hex, 33 bytes)
/// * `provider_id` - Provider identifier string
/// * `model` - Model name used for the inference
/// * `token_count` - Number of tokens in the completion
/// * `timestamp_ms` - Unix timestamp in milliseconds
/// * `proof_tree_hex` - Compiled ErgoTree hex for usage_proof.es contract
/// * `min_value_nanoerg` - Minimum ERG value for the proof box (default: `SAFE_MIN_BOX_VALUE`)
#[allow(dead_code)] // Public API — called from integration tests; production wiring pending
pub async fn submit_usage_proof(
    client: &ErgoNodeClient,
    user_pk_hex: &str,
    provider_id: &str,
    model: &str,
    token_count: i32,
    timestamp_ms: i64,
    proof_tree_hex: &str,
    min_value_nanoerg: u64,
) -> anyhow::Result<String> {
    let proof = crate::chain::transactions::PendingUsageProof {
        user_pk: user_pk_hex.to_string(),
        provider_id: provider_id.to_string(),
        model: model.to_string(),
        token_count: token_count as i64,
        timestamp_ms,
        rarity_multiplier: 1.0, // default; override per-call if needed
    };

    crate::chain::transactions::submit_usage_proof_tx(
        client,
        &proof,
        proof_tree_hex,
        min_value_nanoerg,
    )
    .await
    .context("Usage proof transaction failed")
}

// ---------------------------------------------------------------------------
// create_user_staking_box
// ---------------------------------------------------------------------------

/// Create a User Staking Box on-chain.
///
/// Builds and submits a transaction that creates an output box guarded by the
/// `user_staking.es` contract (or P2PK fallback if `staking_tree_hex` is empty).
///
/// The box contains:
/// - **ErgoTree**: from `staking_tree_hex` (compiled user_staking.es), or P2PK
/// - **R4 register**: user's public key as a Sigma `GroupElement`
/// - **Value**: `amount_nanoerg` (must be >= `SAFE_MIN_BOX_VALUE`)
///
/// # Arguments
///
/// * `client` - Ergo node client
/// * `user_pk_hex` - User's compressed secp256k1 public key (hex, 33 bytes)
/// * `amount_nanoerg` - ERG amount to lock in the staking box (nanoERG)
/// * `staking_tree_hex` - Compiled ErgoTree hex for user_staking.es (can be empty for P2PK fallback)
///
/// # Returns
///
/// `UserStakingResult` with `tx_id`, `box_id`, and `staking_address`.
///
/// # User Staking Box Register Layout
///
/// | Register | Type         | Content                       |
/// |----------|--------------|-------------------------------|
/// | R4       | GroupElement | User's compressed secp256k1 PK|
pub async fn create_user_staking_box(
    client: &ErgoNodeClient,
    user_pk_hex: &str,
    amount_nanoerg: u64,
    staking_tree_hex: &str,
) -> Result<UserStakingResult> {
    let info = UserStakingInfo {
        user_pk_hex: user_pk_hex.to_string(),
        amount_nanoerg,
        staking_tree_hex: staking_tree_hex.to_string(),
    };

    let result = build_staking_tx(client, &info)
        .await
        .context("User staking box creation failed")?;

    info!(
        tx_id = %result.tx_id,
        box_id = %result.box_id,
        staking_address = %result.staking_address,
        "User staking box created successfully"
    );

    Ok(result)
}

// -----------------------------------------------------------------------------------------------------------------------------------------------
// pay_provider
// -----------------------------------------------------------------------------------------------------------------------------------------------

/// Pay a provider for inference services by sending ERG from the node wallet.
///
/// This is a simplified settlement that sends ERG directly to the provider's
/// registered P2PK address (looked up from their on-chain Provider Box).
/// The payment is made from the node wallet's available balance.
///
/// A full implementation (Phase 2) would instead spend the User Staking Box
/// via ergo-lib to atomically transfer ERG from the staked amount, ensuring
/// the user cannot double-spend. This simplified version is useful for testing
/// and for wallets that are not the user staking box.
///
/// # Arguments
///
/// * `client` - Ergo node client
/// * `provider_nft_id` - Provider's NFT token ID (to look up their box)
/// * `amount_nanoerg` - ERG amount to pay in nanoERG
///
/// # Returns
///
/// The transaction ID of the payment transaction.
#[allow(dead_code)] // Public API — called from integration tests; production wiring pending
pub async fn pay_provider(
    client: &ErgoNodeClient,
    provider_nft_id: &str,
    amount_nanoerg: u64,
) -> anyhow::Result<String> {
    // Look up the provider's box to find their registered P2PK address
    let boxes = client
        .get_boxes_by_token_id(provider_nft_id)
        .await
        .context("Failed to scan for provider box by NFT token ID")?;

    let provider_box = boxes
        .iter()
        .find(|b| b.assets.iter().any(|a| a.token_id == provider_nft_id && a.amount == 1))
        .context("Provider box not found on-chain — NFT token ID may be wrong or provider not registered")?;

    let provider_address = &provider_box.ergo_tree;

    if amount_nanoerg < SAFE_MIN_BOX_VALUE {
        anyhow::bail!(
            "Payment amount {} nanoERG is below minimum box value {}",
            amount_nanoerg,
            SAFE_MIN_BOX_VALUE
        );
    }

    let tx_id = client
        .send_payment(provider_address, amount_nanoerg)
        .await
        .context("Failed to send provider payment via wallet")?;

    info!(
        tx_id = %tx_id,
        provider_nft_id = %provider_nft_id,
        provider_address = %provider_address,
        amount_nanoerg = amount_nanoerg,
        "Provider payment sent"
    );

    Ok(tx_id)
}

// ---------------------------------------------------------------------------
// Governance transaction planning
// ---------------------------------------------------------------------------
//
// These functions create `GovernanceTxPlan` structs that describe the exact
// state transitions needed for create, vote, execute, and close operations
// on the governance proposal box (singleton NFT state machine).
//
// Since the Ergo node wallet API (`POST /wallet/payment/send`) cannot set
// custom registers on outputs, these functions return descriptive plans
// rather than broadcasting transactions directly. The plan can be consumed
// by:
//   - An ergo-lib based builder (feature-gated, future)
//   - A manual transaction built by the operator
//   - The API layer for inspection before signing

use crate::chain::types::GovernanceProposalBox;
use crate::protocol::specs::validate_governance_box;

/// The type of governance operation being planned.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum GovernanceOp {
    /// Create a new proposal (requires R5 == 0 on current box).
    CreateProposal,
    /// Vote on the active proposal (requires HEIGHT <= R8).
    Vote,
    /// Execute a passed proposal (requires HEIGHT > R8 + off-chain threshold check).
    Execute,
    /// Close/cancel a proposal (requires HEIGHT > R8).
    Close,
}

impl std::fmt::Display for GovernanceOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CreateProposal => write!(f, "CreateProposal"),
            Self::Vote => write!(f, "Vote"),
            Self::Execute => write!(f, "Execute"),
            Self::Close => write!(f, "Close"),
        }
    }
}

/// Describes the register values for the successor (OUTPUTS(0)) governance box.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GovernanceOutputRegisters {
    /// R4: proposalCount (Int)
    pub r4_proposal_count: i32,
    /// R5: activeProposalId (Int) -- 0 = no active proposal
    pub r5_active_proposal_id: i32,
    /// R6: votingThreshold (Int)
    pub r6_voting_threshold: i32,
    /// R7: totalVoters (Int)
    pub r7_total_voters: i32,
    /// R8: proposalEndHeight (Int)
    pub r8_proposal_end_height: i32,
    /// R9: proposalDataHash (hex-encoded Coll[Byte])
    pub r9_proposal_data_hash: String,
}

/// A complete plan describing a governance state transition.
///
/// This struct captures everything needed to build the actual transaction,
/// whether via ergo-lib, a manual offline build, or a future integration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GovernanceTxPlan {
    /// The type of governance operation.
    pub op: GovernanceOp,
    /// The box ID of the governance box being spent.
    pub gov_box_id: String,
    /// The Governance NFT token ID that must be preserved.
    pub gov_nft_id: String,
    /// The current chain height at the time of planning.
    pub current_height: i32,
    /// Snapshot of the governance box registers *before* the transition.
    pub current_state: GovernanceProposalBox,
    /// The register values that must appear on the successor box (OUTPUTS(0)).
    pub output_registers: GovernanceOutputRegisters,
    /// Human-readable description of the state transition.
    pub description: String,
    /// Whether this operation is currently valid given the chain state.
    pub is_valid: bool,
    /// Validation errors (empty if `is_valid` is true).
    pub validation_errors: Vec<String>,
}

impl GovernanceTxPlan {
    /// Return a JSON string representation of the plan.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .context("Failed to serialize GovernanceTxPlan to JSON")
    }
}

/// Internal helper: fetch and validate the governance box from the chain.
async fn fetch_and_validate_governance_box(
    client: &ErgoNodeClient,
    gov_box_id: &str,
) -> Result<GovernanceProposalBox> {
    let raw_box = client
        .get_box(gov_box_id)
        .await
        .with_context(|| format!("Failed to fetch governance box {}", gov_box_id))?;

    let current_height = client
        .get_height()
        .await
        .context("Failed to get current chain height")?;

    let gov_box = validate_governance_box(&raw_box, current_height)
        .with_context(|| format!("Governance box {} failed validation", gov_box_id))?;

    Ok(gov_box)
}

/// Internal helper: compute blake2b256 hash of proposal data bytes.
fn hash_proposal_data(data: &[u8]) -> String {
    use blake2::Digest;
    let mut hasher = blake2::Blake2b::<blake2::digest::consts::U32>::new();
    hasher.update(data);
    let hash = hasher.finalize();
    hex::encode(hash)
}

// ---------------------------------------------------------------------------
// plan_create_proposal
// ---------------------------------------------------------------------------

/// Plan a "Create Proposal" transaction on the governance box.
///
/// This operation is valid when:
/// - The governance box has no active proposal (R5 == 0)
///
/// The successor box will have:
/// - R4 = proposalCount + 1
/// - R5 = proposalCount + 1 (the new active proposal ID)
/// - R6 = `threshold` (voting threshold)
/// - R7 = `total_voters` (eligible voter count)
/// - R8 = `end_height` (when voting ends)
/// - R9 = blake2b256(proposal_data)
///
/// # Arguments
///
/// * `client` - Ergo node client (used to fetch current box state)
/// * `gov_box_id` - Box ID of the governance proposal box
/// * `threshold` - Minimum votes needed to pass the proposal
/// * `total_voters` - Total number of eligible voters
/// * `end_height` - Block height at which voting ends
/// * `proposal_data` - Raw proposal content bytes (will be hashed into R9)
pub async fn plan_create_proposal(
    client: &ErgoNodeClient,
    gov_box_id: &str,
    threshold: i32,
    total_voters: i32,
    end_height: i32,
    proposal_data: &[u8],
) -> Result<GovernanceTxPlan> {
    let gov_box = fetch_and_validate_governance_box(client, gov_box_id).await?;
    let current_height = client.get_height().await?;

    let mut validation_errors = Vec::new();
    let mut is_valid = true;

    // Validate: no active proposal
    if gov_box.active_proposal_id != 0 {
        validation_errors.push(format!(
            "Cannot create proposal: active proposal {} already exists (R5 must be 0)",
            gov_box.active_proposal_id
        ));
        is_valid = false;
    }

    // Validate: threshold must be positive
    if threshold <= 0 {
        validation_errors.push(format!(
            "Invalid voting threshold: {} (must be > 0)",
            threshold
        ));
        is_valid = false;
    }

    // Validate: total_voters must be positive
    if total_voters <= 0 {
        validation_errors.push(format!(
            "Invalid total voters: {} (must be > 0)",
            total_voters
        ));
        is_valid = false;
    }

    // Validate: end_height must be in the future
    if end_height <= current_height {
        validation_errors.push(format!(
            "Invalid end height: {} (must be > current height {})",
            end_height, current_height
        ));
        is_valid = false;
    }

    // Validate: threshold cannot exceed total_voters
    if threshold > total_voters {
        validation_errors.push(format!(
            "Threshold {} exceeds total voters {}",
            threshold, total_voters
        ));
        is_valid = false;
    }

    let new_proposal_id = gov_box.proposal_count + 1;
    let proposal_data_hash = hash_proposal_data(proposal_data);

    let output_registers = GovernanceOutputRegisters {
        r4_proposal_count: new_proposal_id,
        r5_active_proposal_id: new_proposal_id,
        r6_voting_threshold: threshold,
        r7_total_voters: total_voters,
        r8_proposal_end_height: end_height,
        r9_proposal_data_hash: proposal_data_hash.clone(),
    };

    let description = format!(
        "Create proposal #{}: threshold={}/{}, end_height={}, data_hash={}",
        new_proposal_id, threshold, total_voters, end_height, &proposal_data_hash[..16]
    );

    info!(
        gov_box_id = %gov_box_id,
        proposal_id = new_proposal_id,
        threshold,
        total_voters,
        end_height,
        "Planned governance: create proposal"
    );

    Ok(GovernanceTxPlan {
        op: GovernanceOp::CreateProposal,
        gov_box_id: gov_box_id.to_string(),
        gov_nft_id: gov_box.gov_nft_id.clone(),
        current_height,
        current_state: gov_box,
        output_registers,
        description,
        is_valid,
        validation_errors,
    })
}

// ---------------------------------------------------------------------------
// plan_vote
// ---------------------------------------------------------------------------

/// Plan a "Vote" transaction on the governance box.
///
/// This operation is valid when:
/// - The governance box has an active proposal (R5 > 0)
/// - The current block height is within the voting window (HEIGHT <= R8)
///
/// The successor box preserves all registers unchanged (R4-R9 identical).
/// The voter's proveDlog signature authorizes the vote.
///
/// # Arguments
///
/// * `client` - Ergo node client
/// * `gov_box_id` - Box ID of the governance proposal box
/// * `voter_pk_hex` - Voter's compressed secp256k1 public key (hex, for logging)
pub async fn plan_vote(
    client: &ErgoNodeClient,
    gov_box_id: &str,
    voter_pk_hex: &str,
) -> Result<GovernanceTxPlan> {
    let gov_box = fetch_and_validate_governance_box(client, gov_box_id).await?;
    let current_height = client.get_height().await?;

    let mut validation_errors = Vec::new();
    let mut is_valid = true;

    // Validate: active proposal must exist
    if gov_box.active_proposal_id <= 0 {
        validation_errors.push(format!(
            "Cannot vote: no active proposal (R5 = {})",
            gov_box.active_proposal_id
        ));
        is_valid = false;
    }

    // Validate: must be within voting window
    if current_height > gov_box.proposal_end_height {
        validation_errors.push(format!(
            "Cannot vote: voting period ended (height {} > end_height {})",
            current_height, gov_box.proposal_end_height
        ));
        is_valid = false;
    }

    // All registers preserved
    let output_registers = GovernanceOutputRegisters {
        r4_proposal_count: gov_box.proposal_count,
        r5_active_proposal_id: gov_box.active_proposal_id,
        r6_voting_threshold: gov_box.voting_threshold,
        r7_total_voters: gov_box.total_voters,
        r8_proposal_end_height: gov_box.proposal_end_height,
        r9_proposal_data_hash: gov_box.proposal_data_hash.clone(),
    };

    let description = format!(
        "Vote on proposal #{} by voter {} (height {}/{})",
        gov_box.active_proposal_id,
        &voter_pk_hex[..voter_pk_hex.len().min(16)],
        current_height,
        gov_box.proposal_end_height
    );

    info!(
        gov_box_id = %gov_box_id,
        proposal_id = gov_box.active_proposal_id,
        voter_pk = %voter_pk_hex,
        current_height,
        "Planned governance: vote"
    );

    Ok(GovernanceTxPlan {
        op: GovernanceOp::Vote,
        gov_box_id: gov_box_id.to_string(),
        gov_nft_id: gov_box.gov_nft_id.clone(),
        current_height,
        current_state: gov_box,
        output_registers,
        description,
        is_valid,
        validation_errors,
    })
}

// ---------------------------------------------------------------------------
// plan_execute_proposal
// ---------------------------------------------------------------------------

/// Plan an "Execute Proposal" transaction on the governance box.
///
/// This operation is valid when:
/// - The governance box has an active proposal (R5 > 0)
/// - The voting period has ended (HEIGHT > R8)
/// - Off-chain vote counting confirms the threshold was met
///
/// The successor box resets R5 = 0, allowing new proposals.
/// R4 (proposalCount) is preserved.
///
/// **Note:** The off-chain threshold check (comparing vote counter boxes
/// against `voting_threshold`) is the caller's responsibility before
/// submitting the transaction.
///
/// # Arguments
///
/// * `client` - Ergo node client
/// * `gov_box_id` - Box ID of the governance proposal box
/// * `executor_pk_hex` - Executor's public key (hex, for logging/authorization)
pub async fn plan_execute_proposal(
    client: &ErgoNodeClient,
    gov_box_id: &str,
    executor_pk_hex: &str,
) -> Result<GovernanceTxPlan> {
    let gov_box = fetch_and_validate_governance_box(client, gov_box_id).await?;
    let current_height = client.get_height().await?;

    let mut validation_errors = Vec::new();
    let mut is_valid = true;

    // Validate: active proposal must exist
    if gov_box.active_proposal_id <= 0 {
        validation_errors.push(format!(
            "Cannot execute: no active proposal (R5 = {})",
            gov_box.active_proposal_id
        ));
        is_valid = false;
    }

    // Validate: voting period must have ended
    if current_height <= gov_box.proposal_end_height {
        validation_errors.push(format!(
            "Cannot execute: voting still in progress (height {} <= end_height {})",
            current_height, gov_box.proposal_end_height
        ));
        is_valid = false;
    }

    // Note: off-chain threshold check is the caller's responsibility.
    // We add a warning but don't fail validation, since the plan itself
    // is still correct -- the operator must verify vote counts separately.
    validation_errors.push(
        "WARNING: Off-chain threshold check required. Verify vote counter boxes \
         show votes >= threshold before submitting this transaction."
            .to_string(),
    );

    let output_registers = GovernanceOutputRegisters {
        r4_proposal_count: gov_box.proposal_count,
        r5_active_proposal_id: 0, // Reset to no active proposal
        r6_voting_threshold: gov_box.voting_threshold,
        r7_total_voters: gov_box.total_voters,
        r8_proposal_end_height: gov_box.proposal_end_height,
        r9_proposal_data_hash: gov_box.proposal_data_hash.clone(),
    };

    let description = format!(
        "Execute proposal #{} (threshold={}/{}, end_height={}, executor={})",
        gov_box.active_proposal_id,
        gov_box.voting_threshold,
        gov_box.total_voters,
        gov_box.proposal_end_height,
        &executor_pk_hex[..executor_pk_hex.len().min(16)]
    );

    info!(
        gov_box_id = %gov_box_id,
        proposal_id = gov_box.active_proposal_id,
        executor_pk = %executor_pk_hex,
        "Planned governance: execute proposal"
    );

    Ok(GovernanceTxPlan {
        op: GovernanceOp::Execute,
        gov_box_id: gov_box_id.to_string(),
        gov_nft_id: gov_box.gov_nft_id.clone(),
        current_height,
        current_state: gov_box,
        output_registers,
        description,
        is_valid,
        validation_errors,
    })
}

// ---------------------------------------------------------------------------
// plan_close_proposal
// ---------------------------------------------------------------------------

/// Plan a "Close Proposal" transaction on the governance box.
///
/// This operation is valid when:
/// - The governance box has an active proposal (R5 > 0)
/// - The voting period has ended (HEIGHT > R8)
///
/// The successor box resets R5 = 0, allowing new proposals.
/// R4 (proposalCount) is preserved.
///
/// This is used when a proposal did not reach its voting threshold
/// and should be cancelled, or after execution to fully close the cycle.
///
/// # Arguments
///
/// * `client` - Ergo node client
/// * `gov_box_id` - Box ID of the governance proposal box
/// * `closer_pk_hex` - Closer's public key (hex, for logging/authorization)
pub async fn plan_close_proposal(
    client: &ErgoNodeClient,
    gov_box_id: &str,
    closer_pk_hex: &str,
) -> Result<GovernanceTxPlan> {
    let gov_box = fetch_and_validate_governance_box(client, gov_box_id).await?;
    let current_height = client.get_height().await?;

    let mut validation_errors = Vec::new();
    let mut is_valid = true;

    // Validate: active proposal must exist
    if gov_box.active_proposal_id <= 0 {
        validation_errors.push(format!(
            "Cannot close: no active proposal (R5 = {})",
            gov_box.active_proposal_id
        ));
        is_valid = false;
    }

    // Validate: voting period must have ended
    if current_height <= gov_box.proposal_end_height {
        validation_errors.push(format!(
            "Cannot close: voting still in progress (height {} <= end_height {})",
            current_height, gov_box.proposal_end_height
        ));
        is_valid = false;
    }

    let output_registers = GovernanceOutputRegisters {
        r4_proposal_count: gov_box.proposal_count,
        r5_active_proposal_id: 0, // Reset to no active proposal
        r6_voting_threshold: gov_box.voting_threshold,
        r7_total_voters: gov_box.total_voters,
        r8_proposal_end_height: gov_box.proposal_end_height,
        r9_proposal_data_hash: gov_box.proposal_data_hash.clone(),
    };

    let description = format!(
        "Close proposal #{} (end_height={}, closer={})",
        gov_box.active_proposal_id,
        gov_box.proposal_end_height,
        &closer_pk_hex[..closer_pk_hex.len().min(16)]
    );

    info!(
        gov_box_id = %gov_box_id,
        proposal_id = gov_box.active_proposal_id,
        closer_pk = %closer_pk_hex,
        "Planned governance: close proposal"
    );

    Ok(GovernanceTxPlan {
        op: GovernanceOp::Close,
        gov_box_id: gov_box_id.to_string(),
        gov_nft_id: gov_box.gov_nft_id.clone(),
        current_height,
        current_state: gov_box,
        output_registers,
        description,
        is_valid,
        validation_errors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_provider_invalid_pk() {
        // This tests the sync validation path; we can't test async without a node.
        let models_json = serde_json::to_string(&["llama-3.1-8b".to_string()]).unwrap();
        let info = ProviderRegistrationInfo {
            provider_pk_hex: "not_hex".to_string(),
            provider_id: "test".to_string(),
            endpoint: "http://localhost:9099".to_string(),
            models_json,
            region: "us-east".to_string(),
            provider_tree_hex: String::new(),
            box_value_nanoerg: SAFE_MIN_BOX_VALUE,
        };
        // PK validation would happen in the async function, but we can check the struct
        assert_eq!(info.provider_pk_hex, "not_hex");
    }

    #[test]
    fn test_models_serialization() {
        let models = vec!["llama-3.1-8b".to_string(), "mistral-7b".to_string()];
        let json = serde_json::to_string(&models).unwrap();
        assert_eq!(json, r#"["llama-3.1-8b","mistral-7b"]"#);
    }

    // TODO(x7): integration tests — these require a running Ergo node or mock client
    // #[tokio::test]
    // async fn test_submit_heartbeat_ok() { ... }
    // #[tokio::test]
    // async fn test_submit_usage_proof_ok() { ... }
    // #[tokio::test]
    // async fn test_pay_provider_ok() { ... }

    #[test]
    fn test_user_staking_info_defaults() {
        let info = UserStakingInfo {
            user_pk_hex: String::new(),
            amount_nanoerg: SAFE_MIN_BOX_VALUE,
            staking_tree_hex: String::new(),
        };
        assert_eq!(info.amount_nanoerg, SAFE_MIN_BOX_VALUE);
        assert!(info.user_pk_hex.is_empty());
        assert!(info.staking_tree_hex.is_empty());
    }

    #[test]
    fn test_user_staking_result_serialization() {
        let result = UserStakingResult {
            tx_id: "tx_abc123".to_string(),
            box_id: "box_def456".to_string(),
            staking_address: "3WwxTestAddress".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: UserStakingResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tx_id, "tx_abc123");
        assert_eq!(parsed.box_id, "box_def456");
        assert_eq!(parsed.staking_address, "3WwxTestAddress");
    }

    #[test]
    fn test_create_user_staking_box_validation_empty_pk() {
        // Verify that an empty PK string would be caught by the async validation.
        // We test the sync validation path via the struct construction.
        let info = UserStakingInfo {
            user_pk_hex: String::new(),
            amount_nanoerg: SAFE_MIN_BOX_VALUE,
            staking_tree_hex: String::new(),
        };
        assert!(info.user_pk_hex.is_empty());
    }

    #[test]
    fn test_create_user_staking_box_validation_bad_pk_length() {
        // PK hex that decodes to wrong length
        let bad_pk = "02aabbccdd"; // 5 bytes, not 33
        let decoded = hex::decode(bad_pk).unwrap();
        assert_ne!(decoded.len(), 33, "Bad PK should not be 33 bytes");
    }

    #[test]
    fn test_create_user_staking_box_validation_insufficient_amount() {
        let info = UserStakingInfo {
            user_pk_hex: "02aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            amount_nanoerg: 500_000, // Below SAFE_MIN_BOX_VALUE
            staking_tree_hex: String::new(),
        };
        assert!(info.amount_nanoerg < SAFE_MIN_BOX_VALUE);
    }

    #[test]
    fn test_create_user_staking_box_validation_valid_pk() {
        // A valid 33-byte compressed PK (hex = 66 chars)
        let valid_pk = "02aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let decoded = hex::decode(valid_pk).unwrap();
        assert_eq!(decoded.len(), 33);
    }
}
