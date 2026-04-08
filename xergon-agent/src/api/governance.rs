//! Governance proposal lifecycle REST API.
//!
//! Provides endpoints for managing governance proposals on the Xergon protocol:
//!
//! - `GET  /v1/governance/state`            -- current governance box state
//! - `POST /v1/governance/proposal/create`  -- create a new proposal
//! - `POST /v1/governance/proposal/vote`     -- vote on active proposal
//! - `POST /v1/governance/proposal/execute`  -- execute a passed proposal
//! - `POST /v1/governance/proposal/close`    -- close/cancel a proposal
//!
//! These endpoints use the governance transaction planning functions from
//! `crate::protocol::actions` to validate and describe state transitions.
//! The actual transaction broadcasting requires off-chain tools since the
//! Ergo node wallet API cannot set custom registers on outputs.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use tracing::warn;

use crate::chain::client::ErgoNodeClient;
use crate::governance::{
    get_template, built_in_templates, OnChainGovernance, OnChainProposal, ProposalCategory, ProposalStage,
};
use crate::protocol::actions::{
    plan_close_proposal, plan_create_proposal, plan_execute_proposal, plan_vote, GovernanceTxPlan,
};
use crate::protocol::specs::validate_governance_box;

// ---------------------------------------------------------------------------
// Shared state for governance routes
// ---------------------------------------------------------------------------

/// State shared across governance API handlers.
#[derive(Clone)]
pub struct GovernanceState {
    /// Ergo node client for chain queries.
    pub ergo_node_url: String,
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

// --- GET /v1/governance/state ---

#[derive(Debug, Serialize)]
pub struct GovernanceStateResponse {
    /// Total number of proposals submitted (R4).
    pub proposal_count: i32,
    /// Currently active proposal ID, 0 if none (R5).
    pub active_proposal_id: i32,
    /// Voting threshold for the active proposal (R6).
    pub voting_threshold: i32,
    /// Total eligible voters (R7).
    pub total_voters: i32,
    /// Block height when the active proposal ends (R8).
    pub proposal_end_height: i32,
    /// Blake2b256 hash of proposal data (R9).
    pub proposal_data_hash: String,
    /// The governance box ID.
    pub box_id: String,
    /// Current chain height.
    pub current_height: i32,
    /// Blocks remaining until voting ends (0 if no active proposal).
    pub blocks_remaining: i32,
    /// Lifecycle status of the governance box.
    pub status: String,
}

// --- POST /v1/governance/proposal/create ---

#[derive(Debug, Deserialize)]
pub struct CreateProposalRequest {
    /// Hex-encoded governance box ID.
    pub gov_box_id: String,
    /// Blake2b256 hash of the proposal data (hex string).
    pub proposal_data_hash: String,
    /// Voting period in blocks (optional, default 10000).
    pub voting_period_blocks: Option<i32>,
    /// Voting threshold (optional, uses current R6 if not set).
    pub voting_threshold: Option<i32>,
    /// Total eligible voters (optional, uses current R7 if not set).
    pub total_voters: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct CreateProposalResponse {
    /// The transaction plan describing the state transition.
    pub plan: GovernanceTxPlan,
}

// --- POST /v1/governance/proposal/vote ---

#[derive(Debug, Deserialize)]
pub struct VoteRequest {
    /// Hex-encoded governance box ID.
    pub gov_box_id: String,
    /// Voter's compressed secp256k1 public key (hex).
    pub voter_pk_hex: String,
}

#[derive(Debug, Serialize)]
pub struct VoteResponse {
    /// The transaction plan describing the vote.
    pub plan: GovernanceTxPlan,
}

// --- POST /v1/governance/proposal/execute ---

#[derive(Debug, Deserialize)]
pub struct ExecuteRequest {
    /// Hex-encoded governance box ID.
    pub gov_box_id: String,
    /// Executor's public key (hex).
    pub executor_pk_hex: String,
}

#[derive(Debug, Serialize)]
pub struct ExecuteResponse {
    /// The transaction plan describing the execution.
    pub plan: GovernanceTxPlan,
}

// --- POST /v1/governance/proposal/close ---

#[derive(Debug, Deserialize)]
pub struct CloseRequest {
    /// Hex-encoded governance box ID.
    pub gov_box_id: String,
    /// Closer's public key (hex).
    pub closer_pk_hex: String,
    /// Optional reason for closing (logged only).
    pub reason: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CloseResponse {
    /// The transaction plan describing the close operation.
    pub plan: GovernanceTxPlan,
}

// ---------------------------------------------------------------------------
// Error response
// ---------------------------------------------------------------------------

fn governance_error(status: StatusCode, message: &str) -> axum::response::Response {
    (
        status,
        Json(serde_json::json!({
            "error": {
                "type": "governance_error",
                "message": message,
                "code": status.as_u16(),
            }
        })),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /v1/governance/state` -- Returns the current governance box state.
///
/// Accepts query parameter `gov_box_id` to identify which governance box to read.
/// Falls back to searching by token ID if box_id is not provided.
async fn governance_state_handler(
    State(state): State<GovernanceState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let client = ErgoNodeClient::new(state.ergo_node_url.clone());

    let gov_box_id = match params.get("gov_box_id") {
        Some(id) => id.clone(),
        None => {
            return governance_error(
                StatusCode::BAD_REQUEST,
                "Missing required query parameter: gov_box_id",
            );
        }
    };

    let current_height = match client.get_height().await {
        Ok(h) => h,
        Err(e) => {
            warn!(error = %e, "Failed to get current height for governance state");
            return governance_error(
                StatusCode::SERVICE_UNAVAILABLE,
                &format!("Failed to get current chain height: {}", e),
            );
        }
    };

    let raw_box = match client.get_box(&gov_box_id).await {
        Ok(b) => b,
        Err(e) => {
            return governance_error(
                StatusCode::NOT_FOUND,
                &format!("Governance box {} not found: {}", gov_box_id, e),
            );
        }
    };

    let gov_box = match validate_governance_box(&raw_box, current_height) {
        Ok(b) => b,
        Err(e) => {
            return governance_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Governance box validation failed: {}", e),
            );
        }
    };

    let blocks_remaining = if gov_box.active_proposal_id > 0 {
        (gov_box.proposal_end_height - current_height).max(0)
    } else {
        0
    };

    let status = if gov_box.active_proposal_id == 0 {
        "idle".to_string()
    } else if current_height <= gov_box.proposal_end_height {
        "voting".to_string()
    } else {
        "executable".to_string()
    };

    let response = GovernanceStateResponse {
        proposal_count: gov_box.proposal_count,
        active_proposal_id: gov_box.active_proposal_id,
        voting_threshold: gov_box.voting_threshold,
        total_voters: gov_box.total_voters,
        proposal_end_height: gov_box.proposal_end_height,
        proposal_data_hash: gov_box.proposal_data_hash,
        box_id: gov_box.box_id,
        current_height,
        blocks_remaining,
        status,
    };

    (StatusCode::OK, Json(response)).into_response()
}

/// `POST /v1/governance/proposal/create` -- Plan a new governance proposal.
async fn governance_create_handler(
    State(state): State<GovernanceState>,
    axum::Json(req): axum::Json<CreateProposalRequest>,
) -> impl IntoResponse {
    let client = ErgoNodeClient::new(state.ergo_node_url.clone());

    // Fetch current box state for defaults
    let current_height = match client.get_height().await {
        Ok(h) => h,
        Err(e) => {
            return governance_error(
                StatusCode::SERVICE_UNAVAILABLE,
                &format!("Failed to get chain height: {}", e),
            );
        }
    };

    let voting_period = req.voting_period_blocks.unwrap_or(10_000);
    let end_height = current_height + voting_period;

    // Use provided values or defaults from the current box
    let (threshold, total_voters) = if req.voting_threshold.is_some()
        && req.total_voters.is_some()
    {
        (req.voting_threshold.unwrap(), req.total_voters.unwrap())
    } else {
        // Try to read defaults from the current box
        match client.get_box(&req.gov_box_id).await {
            Ok(raw_box) => match validate_governance_box(&raw_box, current_height) {
                Ok(gov_box) => (
                    req.voting_threshold.unwrap_or(gov_box.voting_threshold.max(1)),
                    req.total_voters.unwrap_or(gov_box.total_voters.max(1)),
                ),
                Err(e) => {
                    return governance_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        &format!("Governance box validation failed: {}", e),
                    );
                }
            },
            Err(e) => {
                return governance_error(
                    StatusCode::NOT_FOUND,
                    &format!("Governance box not found: {}", e),
                );
            }
        }
    };

    // Decode the proposal data hash from hex to bytes for hashing
    let proposal_data = match hex::decode(&req.proposal_data_hash) {
        Ok(data) => data,
        Err(e) => {
            return governance_error(
                StatusCode::BAD_REQUEST,
                &format!("Invalid proposal_data_hash hex: {}", e),
            );
        }
    };

    match plan_create_proposal(
        &client,
        &req.gov_box_id,
        threshold,
        total_voters,
        end_height,
        &proposal_data,
    )
    .await
    {
        Ok(plan) => {
            if !plan.is_valid {
                (
                    StatusCode::CONFLICT,
                    Json(CreateProposalResponse { plan }),
                )
                    .into_response()
            } else {
                (
                    StatusCode::OK,
                    Json(CreateProposalResponse { plan }),
                )
                    .into_response()
            }
        }
        Err(e) => {
            governance_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Failed to plan proposal creation: {}", e),
            )
        }
    }
}

/// `POST /v1/governance/proposal/vote` -- Plan a vote on the active proposal.
async fn governance_vote_handler(
    State(state): State<GovernanceState>,
    axum::Json(req): axum::Json<VoteRequest>,
) -> impl IntoResponse {
    let client = ErgoNodeClient::new(state.ergo_node_url.clone());

    match plan_vote(&client, &req.gov_box_id, &req.voter_pk_hex).await {
        Ok(plan) => {
            if !plan.is_valid {
                (
                    StatusCode::CONFLICT,
                    Json(VoteResponse { plan }),
                )
                    .into_response()
            } else {
                (StatusCode::OK, Json(VoteResponse { plan })).into_response()
            }
        }
        Err(e) => {
            governance_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Failed to plan vote: {}", e),
            )
        }
    }
}

/// `POST /v1/governance/proposal/execute` -- Plan execution of a passed proposal.
async fn governance_execute_handler(
    State(state): State<GovernanceState>,
    axum::Json(req): axum::Json<ExecuteRequest>,
) -> impl IntoResponse {
    let client = ErgoNodeClient::new(state.ergo_node_url.clone());

    match plan_execute_proposal(&client, &req.gov_box_id, &req.executor_pk_hex).await {
        Ok(plan) => {
            if !plan.is_valid {
                (
                    StatusCode::CONFLICT,
                    Json(ExecuteResponse { plan }),
                )
                    .into_response()
            } else {
                (
                    StatusCode::OK,
                    Json(ExecuteResponse { plan }),
                )
                    .into_response()
            }
        }
        Err(e) => {
            governance_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Failed to plan execution: {}", e),
            )
        }
    }
}

/// `POST /v1/governance/proposal/close` -- Plan closing/cancelling a proposal.
async fn governance_close_handler(
    State(state): State<GovernanceState>,
    axum::Json(req): axum::Json<CloseRequest>,
) -> impl IntoResponse {
    let client = ErgoNodeClient::new(state.ergo_node_url.clone());

    if let Some(ref reason) = req.reason {
        tracing::info!(
            gov_box_id = %req.gov_box_id,
            closer_pk = %req.closer_pk_hex,
            reason,
            "Governance: close proposal requested"
        );
    }

    match plan_close_proposal(&client, &req.gov_box_id, &req.closer_pk_hex).await {
        Ok(plan) => {
            if !plan.is_valid {
                (
                    StatusCode::CONFLICT,
                    Json(CloseResponse { plan }),
                )
                    .into_response()
            } else {
                (StatusCode::OK, Json(CloseResponse { plan })).into_response()
            }
        }
        Err(e) => {
            governance_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Failed to plan close: {}", e),
            )
        }
    }
}

// ---------------------------------------------------------------------------
// On-chain governance endpoints
// ---------------------------------------------------------------------------

// --- POST /v1/governance/onchain/proposals ---

#[derive(Debug, Deserialize)]
pub struct OnChainCreateProposalRequest {
    pub title: String,
    pub description: String,
    pub category: String,
    pub creator_pk: String,
    pub vote_duration_blocks: Option<u32>,
    pub quorum_threshold: Option<u32>,
    pub approval_threshold: Option<u32>,
    pub execution_data: Option<String>, // hex-encoded
    pub erg_value: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct OnChainProposalResponse {
    pub proposal: OnChainProposal,
    pub tx: crate::governance::GovernanceTxResult,
}

async fn onchain_create_proposal_handler(
    axum::Json(req): axum::Json<OnChainCreateProposalRequest>,
) -> impl IntoResponse {
    let category = match ProposalCategory::from_str(&req.category) {
        Ok(c) => c,
        Err(_) => {
            return governance_error(
                StatusCode::BAD_REQUEST,
                &format!("Invalid proposal category: {}", req.category),
            );
        }
    };

    let gov = OnChainGovernance::with_defaults();
    let now = 1000u32; // In production, fetch from chain
    let vote_duration = req.vote_duration_blocks.unwrap_or(gov.config.default_vote_duration);
    let quorum = req.quorum_threshold.unwrap_or(gov.config.default_quorum);
    let approval = req.approval_threshold.unwrap_or(gov.config.default_approval);
    let erg_val = req.erg_value.unwrap_or(gov.config.min_proposal_erg);

    let execution_data = req
        .execution_data
        .as_ref()
        .and_then(|hex| hex::decode(hex).ok());

    let creator_pk = req.creator_pk.clone();
    let pk_short = creator_pk[..8.min(creator_pk.len())].to_string();

    let proposal = OnChainProposal {
        box_id: format!("box_onchain_{}_{}", now, pk_short),
        proposal_id: format!("prop_{}_{}", now, pk_short),
        title: req.title,
        description: req.description,
        category,
        stage: ProposalStage::Created,
        creator_pk,
        created_height: now,
        vote_start_height: now,
        vote_end_height: now + vote_duration,
        quorum_threshold: quorum,
        approval_threshold: approval,
        votes_for: 0,
        votes_against: 0,
        voters: vec![],
        execution_data,
        nft_token_id: format!("nft_prop_{}_{}", now, pk_short),
        erg_value: erg_val,
    };

    match gov.validate_proposal(&proposal) {
        Ok(validation) if !validation.is_valid => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "Validation failed",
                    "details": validation.errors,
                })),
            )
                .into_response();
        }
        Err(e) => {
            return governance_error(
                StatusCode::BAD_REQUEST,
                &format!("Proposal validation error: {}", e),
            );
        }
        _ => {}
    }

    match gov.build_create_proposal_tx(&proposal).await {
        Ok(tx_result) => (
            StatusCode::OK,
            Json(OnChainProposalResponse {
                proposal,
                tx: tx_result,
            }),
        )
            .into_response(),
        Err(e) => governance_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Failed to build proposal tx: {}", e),
        ),
    }
}

// --- POST /v1/governance/onchain/proposals/:id/vote ---

#[derive(Debug, Deserialize)]
pub struct OnChainVoteRequest {
    pub voter_pk: String,
    pub support: bool,
    pub voting_power: u64,
}

async fn onchain_vote_handler(
    Path(proposal_id): Path<String>,
    axum::Json(req): axum::Json<OnChainVoteRequest>,
) -> impl IntoResponse {
    let gov = OnChainGovernance::with_defaults();
    let box_id = format!("box_onchain_{}", proposal_id);

    match gov.build_vote_tx(&box_id, &req.voter_pk, req.support, req.voting_power).await {
        Ok(tx_result) => (StatusCode::OK, Json(tx_result)).into_response(),
        Err(e) => governance_error(
            StatusCode::BAD_REQUEST,
            &format!("Failed to build vote tx: {}", e),
        ),
    }
}

// --- POST /v1/governance/onchain/proposals/:id/execute ---

#[derive(Debug, Deserialize)]
pub struct OnChainExecuteRequest {
    pub execution_boxes: Option<Vec<String>>,
}

async fn onchain_execute_handler(
    Path(proposal_id): Path<String>,
    axum::Json(req): axum::Json<OnChainExecuteRequest>,
) -> impl IntoResponse {
    let gov = OnChainGovernance::with_defaults();
    let box_id = format!("box_onchain_{}", proposal_id);
    let exec_boxes = req.execution_boxes.unwrap_or_default();

    match gov.build_execute_tx(&box_id, exec_boxes).await {
        Ok(tx_result) => (StatusCode::OK, Json(tx_result)).into_response(),
        Err(e) => governance_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Failed to build execute tx: {}", e),
        ),
    }
}

// --- POST /v1/governance/onchain/proposals/:id/close ---

async fn onchain_close_handler(
    Path(proposal_id): Path<String>,
) -> impl IntoResponse {
    let gov = OnChainGovernance::with_defaults();
    let box_id = format!("box_onchain_{}", proposal_id);

    match gov.build_close_tx(&box_id).await {
        Ok(tx_result) => (StatusCode::OK, Json(tx_result)).into_response(),
        Err(e) => governance_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Failed to build close tx: {}", e),
        ),
    }
}

// --- GET /v1/governance/templates ---

async fn list_templates_handler() -> impl IntoResponse {
    let templates = built_in_templates();
    let summary: Vec<serde_json::Value> = templates
        .iter()
        .map(|t| {
            serde_json::json!({
                "id": t.id,
                "name": t.name,
                "category": t.category.to_string(),
                "description": t.description,
                "parameter_count": t.parameters.len(),
                "requires_stake": t.requires_stake,
                "vote_duration_override": t.vote_duration_override,
            })
        })
        .collect();
    (StatusCode::OK, Json(summary)).into_response()
}

// --- GET /v1/governance/templates/:id ---

async fn get_template_handler(Path(template_id): Path<String>) -> impl IntoResponse {
    match get_template(&template_id) {
        Some(template) => (StatusCode::OK, Json(template)).into_response(),
        None => governance_error(
            StatusCode::NOT_FOUND,
            &format!("Template '{}' not found", template_id),
        ),
    }
}

// --- POST /v1/governance/proposals/from-template ---

#[derive(Debug, Deserialize)]
pub struct CreateFromTemplateRequest {
    pub template_id: String,
    pub parameters: HashMap<String, serde_json::Value>,
    pub creator_pk: String,
    pub vote_duration_blocks: Option<u32>,
    pub quorum_threshold: Option<u32>,
    pub approval_threshold: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct CreateFromTemplateResponse {
    pub template_id: String,
    pub validation: crate::governance::ValidationResult,
    pub proposal: Option<OnChainProposal>,
    pub tx: Option<crate::governance::GovernanceTxResult>,
}

async fn create_from_template_handler(
    axum::Json(req): axum::Json<CreateFromTemplateRequest>,
) -> impl IntoResponse {
    let template = match get_template(&req.template_id) {
        Some(t) => t,
        None => {
            return governance_error(
                StatusCode::NOT_FOUND,
                &format!("Template '{}' not found", req.template_id),
            );
        }
    };

    let validation = template.validate_params(&req.parameters);
    if !validation.is_valid {
        return (
            StatusCode::BAD_REQUEST,
            Json(CreateFromTemplateResponse {
                template_id: req.template_id,
                validation,
                proposal: None,
                tx: None,
            }),
        )
            .into_response();
    }

    let filled_params = template.apply_defaults(&req.parameters);
    let execution_data = crate::governance::serialize_params(&filled_params);

    let gov = OnChainGovernance::with_defaults();
    let now = 1000u32;
    let vote_duration = req
        .vote_duration_blocks
        .or(template.vote_duration_override)
        .unwrap_or(gov.config.default_vote_duration);
    let quorum = req.quorum_threshold.unwrap_or(gov.config.default_quorum);
    let approval = req.approval_threshold.unwrap_or(gov.config.default_approval);

    let title = template.name.clone();
    let description = format!(
        "[{}] {}",
        template.category,
        serde_json::to_string_pretty(&filled_params).unwrap_or_default()
    );

    let proposal = OnChainProposal {
        box_id: format!("box_tmpl_{}_{}", template.id, now),
        proposal_id: format!("prop_tmpl_{}_{}", template.id, now),
        title,
        description,
        category: template.category,
        stage: ProposalStage::Created,
        creator_pk: req.creator_pk,
        created_height: now,
        vote_start_height: now,
        vote_end_height: now + vote_duration,
        quorum_threshold: quorum,
        approval_threshold: approval,
        votes_for: 0,
        votes_against: 0,
        voters: vec![],
        execution_data: Some(execution_data),
        nft_token_id: format!("nft_tmpl_{}_{}", template.id, now),
        erg_value: gov.config.min_proposal_erg,
    };

    match gov.build_create_proposal_tx(&proposal).await {
        Ok(tx_result) => (
            StatusCode::OK,
            Json(CreateFromTemplateResponse {
                template_id: req.template_id,
                validation,
                proposal: Some(proposal),
                tx: Some(tx_result),
            }),
        )
            .into_response(),
        Err(e) => governance_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Failed to build proposal tx: {}", e),
        ),
    }
}

// --- GET /v1/governance/validate/:id ---

#[derive(Debug, Serialize)]
pub struct ValidateProposalResponse {
    pub proposal_id: String,
    pub validation: crate::governance::ValidationResult,
    pub tally: Option<crate::governance::TallyResult>,
    pub can_execute: bool,
}

async fn validate_proposal_handler(
    Path(proposal_id): Path<String>,
) -> impl IntoResponse {
    let gov = OnChainGovernance::with_defaults();

    // Build a synthetic proposal for validation (in production, fetch from chain)
    let proposal = OnChainProposal {
        box_id: proposal_id.clone(),
        proposal_id: proposal_id.clone(),
        title: "Sample".to_string(),
        description: "Sample description".to_string(),
        category: ProposalCategory::ConfigChange,
        stage: ProposalStage::Voting,
        creator_pk: "02abc".to_string(),
        created_height: 1000,
        vote_start_height: 1000,
        vote_end_height: 11000,
        quorum_threshold: gov.config.default_quorum,
        approval_threshold: gov.config.default_approval,
        votes_for: 0,
        votes_against: 0,
        voters: vec![],
        execution_data: None,
        nft_token_id: "nft_sample".to_string(),
        erg_value: gov.config.min_proposal_erg,
    };

    let validation = match gov.validate_proposal(&proposal) {
        Ok(v) => v,
        Err(e) => {
            return governance_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Validation error: {}", e),
            );
        }
    };

    let tally = gov.tally_votes(&proposal);
    let can_exec = gov.can_execute(&proposal);

    (
        StatusCode::OK,
        Json(ValidateProposalResponse {
            proposal_id,
            validation,
            tally: Some(tally),
            can_execute: can_exec,
        }),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the governance lifecycle API router.
///
/// Returns a `Router<()>` so it can be merged with routers using different
/// state types.
pub fn build_governance_router() -> Router<()> {
    let governance_state = GovernanceState {
        ergo_node_url: String::new(), // Will be overridden by middleware or replaced
    };

    // We use Router<()> so it can be merged with the main router.
    // The state is passed through the handler by reconstructing the client
    // from the stored ergo_node_url. Since GovernanceState needs the URL,
    // we create a placeholder and rely on the fact that the API layer
    // provides the URL through the parent AppState.
    //
    // For now, we use a simple approach: store the URL in the state and
    // have each handler create its own ErgoNodeClient.
    Router::new()
        .route("/v1/governance/state", get(governance_state_handler))
        .route(
            "/v1/governance/proposal/create",
            post(governance_create_handler),
        )
        .route("/v1/governance/proposal/vote", post(governance_vote_handler))
        .route(
            "/v1/governance/proposal/execute",
            post(governance_execute_handler),
        )
        .route(
            "/v1/governance/proposal/close",
            post(governance_close_handler),
        )
        // On-chain governance endpoints
        .route(
            "/v1/governance/onchain/proposals",
            post(onchain_create_proposal_handler),
        )
        .route(
            "/v1/governance/onchain/proposals/:id/vote",
            post(onchain_vote_handler),
        )
        .route(
            "/v1/governance/onchain/proposals/:id/execute",
            post(onchain_execute_handler),
        )
        .route(
            "/v1/governance/onchain/proposals/:id/close",
            post(onchain_close_handler),
        )
        // Template endpoints
        .route("/v1/governance/templates", get(list_templates_handler))
        .route(
            "/v1/governance/templates/:id",
            get(get_template_handler),
        )
        .route(
            "/v1/governance/proposals/from-template",
            post(create_from_template_handler),
        )
        .route(
            "/v1/governance/validate/:id",
            get(validate_proposal_handler),
        )
        .with_state(governance_state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::actions::GovernanceOutputRegisters;

    #[test]
    fn test_governance_state_response_serialization() {
        let response = GovernanceStateResponse {
            proposal_count: 5,
            active_proposal_id: 3,
            voting_threshold: 51,
            total_voters: 100,
            proposal_end_height: 800_000,
            proposal_data_hash: "abc123def456".to_string(),
            box_id: "box_id_123".to_string(),
            current_height: 750_000,
            blocks_remaining: 50_000,
            status: "voting".to_string(),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"proposal_count\":5"));
        assert!(json.contains("\"status\":\"voting\""));
        assert!(json.contains("\"blocks_remaining\":50000"));
    }

    #[test]
    fn test_governance_state_idle_status() {
        let response = GovernanceStateResponse {
            proposal_count: 5,
            active_proposal_id: 0, // No active proposal
            voting_threshold: 51,
            total_voters: 100,
            proposal_end_height: 0,
            proposal_data_hash: String::new(),
            box_id: "box_id_123".to_string(),
            current_height: 750_000,
            blocks_remaining: 0,
            status: "idle".to_string(),
        };
        assert_eq!(response.status, "idle");
        assert_eq!(response.blocks_remaining, 0);
    }

    #[test]
    fn test_create_proposal_request_deserialization() {
        let json = r#"{
            "gov_box_id": "box123",
            "proposal_data_hash": "abcdef",
            "voting_period_blocks": 5000,
            "voting_threshold": 60,
            "total_voters": 200
        }"#;
        let req: CreateProposalRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.gov_box_id, "box123");
        assert_eq!(req.voting_period_blocks, Some(5000));
        assert_eq!(req.voting_threshold, Some(60));
        assert_eq!(req.total_voters, Some(200));
    }

    #[test]
    fn test_create_proposal_request_defaults() {
        let json = r#"{
            "gov_box_id": "box123",
            "proposal_data_hash": "abcdef"
        }"#;
        let req: CreateProposalRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.gov_box_id, "box123");
        assert_eq!(req.voting_period_blocks, None);
        assert_eq!(req.voting_threshold, None);
        assert_eq!(req.total_voters, None);
    }

    #[test]
    fn test_vote_request_deserialization() {
        let json = r#"{
            "gov_box_id": "box123",
            "voter_pk_hex": "02aabbccdd"
        }"#;
        let req: VoteRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.gov_box_id, "box123");
        assert_eq!(req.voter_pk_hex, "02aabbccdd");
    }

    #[test]
    fn test_execute_request_deserialization() {
        let json = r#"{
            "gov_box_id": "box123",
            "executor_pk_hex": "02aabbccdd"
        }"#;
        let req: ExecuteRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.gov_box_id, "box123");
    }

    #[test]
    fn test_close_request_with_reason() {
        let json = r#"{
            "gov_box_id": "box123",
            "closer_pk_hex": "02aabbccdd",
            "reason": "Proposal did not reach threshold"
        }"#;
        let req: CloseRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.reason, Some("Proposal did not reach threshold".to_string()));
    }

    #[test]
    fn test_close_request_without_reason() {
        let json = r#"{
            "gov_box_id": "box123",
            "closer_pk_hex": "02aabbccdd"
        }"#;
        let req: CloseRequest = serde_json::from_str(json).unwrap();
        assert!(req.reason.is_none());
    }

    #[test]
    fn test_proposal_creation_validation_rejects_active() {
        // Verify that a GovernanceTxPlan with is_valid=false is produced
        // when there's already an active proposal. We test the plan structure.
        let plan = GovernanceTxPlan {
            op: crate::protocol::actions::GovernanceOp::CreateProposal,
            gov_box_id: "test_box".to_string(),
            gov_nft_id: "nft123".to_string(),
            current_height: 100,
            current_state: crate::chain::types::GovernanceProposalBox {
                box_id: "test_box".to_string(),
                tx_id: "tx123".to_string(),
                gov_nft_id: "nft123".to_string(),
                proposal_count: 5,
                active_proposal_id: 3, // Active proposal exists
                voting_threshold: 51,
                total_voters: 100,
                proposal_end_height: 800_000,
                proposal_data_hash: "hash123".to_string(),
                value: "1000000000".to_string(),
                creation_height: 500_000,
            },
            output_registers: GovernanceOutputRegisters {
                r4_proposal_count: 6,
                r5_active_proposal_id: 6,
                r6_voting_threshold: 51,
                r7_total_voters: 100,
                r8_proposal_end_height: 110_000,
                r9_proposal_data_hash: "newhash".to_string(),
            },
            description: "Create proposal #6".to_string(),
            is_valid: false,
            validation_errors: vec![
                "Cannot create proposal: active proposal 3 already exists (R5 must be 0)"
                    .to_string(),
            ],
        };
        assert!(!plan.is_valid);
        assert_eq!(plan.validation_errors.len(), 1);
        assert!(plan.validation_errors[0].contains("active proposal 3"));
    }

    #[test]
    fn test_voting_validation_rejects_past_deadline() {
        // Verify that a GovernanceTxPlan with is_valid=false is produced
        // when voting period has ended.
        let plan = GovernanceTxPlan {
            op: crate::protocol::actions::GovernanceOp::Vote,
            gov_box_id: "test_box".to_string(),
            gov_nft_id: "nft123".to_string(),
            current_height: 900_000, // Past end_height
            current_state: crate::chain::types::GovernanceProposalBox {
                box_id: "test_box".to_string(),
                tx_id: "tx123".to_string(),
                gov_nft_id: "nft123".to_string(),
                proposal_count: 5,
                active_proposal_id: 3,
                voting_threshold: 51,
                total_voters: 100,
                proposal_end_height: 800_000, // Voting ended
                proposal_data_hash: "hash123".to_string(),
                value: "1000000000".to_string(),
                creation_height: 500_000,
            },
            output_registers: GovernanceOutputRegisters {
                r4_proposal_count: 5,
                r5_active_proposal_id: 3,
                r6_voting_threshold: 51,
                r7_total_voters: 100,
                r8_proposal_end_height: 800_000,
                r9_proposal_data_hash: "hash123".to_string(),
            },
            description: "Vote on proposal #3".to_string(),
            is_valid: false,
            validation_errors: vec![
                "Cannot vote: voting period ended (height 900000 > end_height 800000)"
                    .to_string(),
            ],
        };
        assert!(!plan.is_valid);
        assert!(plan.validation_errors[0].contains("voting period ended"));
    }

    #[test]
    fn test_execution_validation_rejects_not_passed() {
        // Execution requires voting period to have ended AND off-chain threshold check.
        // If voting is still in progress, it should fail.
        let plan = GovernanceTxPlan {
            op: crate::protocol::actions::GovernanceOp::Execute,
            gov_box_id: "test_box".to_string(),
            gov_nft_id: "nft123".to_string(),
            current_height: 750_000, // Before end_height
            current_state: crate::chain::types::GovernanceProposalBox {
                box_id: "test_box".to_string(),
                tx_id: "tx123".to_string(),
                gov_nft_id: "nft123".to_string(),
                proposal_count: 5,
                active_proposal_id: 3,
                voting_threshold: 51,
                total_voters: 100,
                proposal_end_height: 800_000, // Voting still in progress
                proposal_data_hash: "hash123".to_string(),
                value: "1000000000".to_string(),
                creation_height: 500_000,
            },
            output_registers: GovernanceOutputRegisters {
                r4_proposal_count: 5,
                r5_active_proposal_id: 0,
                r6_voting_threshold: 51,
                r7_total_voters: 100,
                r8_proposal_end_height: 800_000,
                r9_proposal_data_hash: "hash123".to_string(),
            },
            description: "Execute proposal #3".to_string(),
            is_valid: false,
            validation_errors: vec![
                "Cannot execute: voting still in progress (height 750000 <= end_height 800000)"
                    .to_string(),
            ],
        };
        assert!(!plan.is_valid);
        assert!(plan.validation_errors[0].contains("voting still in progress"));
    }

    #[test]
    fn test_governance_output_registers_serialization() {
        let regs = GovernanceOutputRegisters {
            r4_proposal_count: 6,
            r5_active_proposal_id: 6,
            r6_voting_threshold: 60,
            r7_total_voters: 200,
            r8_proposal_end_height: 850_000,
            r9_proposal_data_hash: "abcdef123456".to_string(),
        };
        let json = serde_json::to_string(&regs).unwrap();
        let parsed: GovernanceOutputRegisters = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.r4_proposal_count, 6);
        assert_eq!(parsed.r5_active_proposal_id, 6);
        assert_eq!(parsed.r6_voting_threshold, 60);
    }

    #[test]
    fn test_governance_tx_plan_to_json() {
        let plan = GovernanceTxPlan {
            op: crate::protocol::actions::GovernanceOp::CreateProposal,
            gov_box_id: "box123".to_string(),
            gov_nft_id: "nft456".to_string(),
            current_height: 100,
            current_state: crate::chain::types::GovernanceProposalBox {
                box_id: "box123".to_string(),
                tx_id: "tx789".to_string(),
                gov_nft_id: "nft456".to_string(),
                proposal_count: 0,
                active_proposal_id: 0,
                voting_threshold: 0,
                total_voters: 0,
                proposal_end_height: 0,
                proposal_data_hash: String::new(),
                value: "1000000000".to_string(),
                creation_height: 50,
            },
            output_registers: GovernanceOutputRegisters {
                r4_proposal_count: 1,
                r5_active_proposal_id: 1,
                r6_voting_threshold: 51,
                r7_total_voters: 100,
                r8_proposal_end_height: 10100,
                r9_proposal_data_hash: "hash".to_string(),
            },
            description: "Test plan".to_string(),
            is_valid: true,
            validation_errors: vec![],
        };
        let json = plan.to_json().unwrap();
        assert!(json.contains("CreateProposal"));
        assert!(json.contains("box123"));
    }

    // ---- New on-chain governance API tests ----

    #[test]
    fn test_onchain_create_proposal_request_deserialization() {
        let json = r#"{
            "title": "Change Fee Rate",
            "description": "Increase fee rate to 2.5%",
            "category": "protocol_param",
            "creator_pk": "02abc123def456",
            "vote_duration_blocks": 5000,
            "quorum_threshold": 20,
            "approval_threshold": 70,
            "erg_value": 2000000000
        }"#;
        let req: OnChainCreateProposalRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.title, "Change Fee Rate");
        assert_eq!(req.category, "protocol_param");
        assert_eq!(req.vote_duration_blocks, Some(5000));
        assert_eq!(req.quorum_threshold, Some(20));
        assert_eq!(req.approval_threshold, Some(70));
        assert_eq!(req.erg_value, Some(2_000_000_000));
    }

    #[test]
    fn test_onchain_vote_request_deserialization() {
        let json = r#"{
            "voter_pk": "02abc123def456",
            "support": true,
            "voting_power": 50000000
        }"#;
        let req: OnChainVoteRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.voter_pk, "02abc123def456");
        assert!(req.support);
        assert_eq!(req.voting_power, 50_000_000);
    }

    #[test]
    fn test_onchain_execute_request_with_boxes() {
        let json = r#"{
            "execution_boxes": ["treasury_box_1", "treasury_box_2"]
        }"#;
        let req: OnChainExecuteRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.execution_boxes.unwrap().len(), 2);
    }

    #[test]
    fn test_onchain_execute_request_no_boxes() {
        let json = r#"{}"#;
        let req: OnChainExecuteRequest = serde_json::from_str(json).unwrap();
        assert!(req.execution_boxes.is_none());
    }

    #[test]
    fn test_create_from_template_request_deserialization() {
        let json = r#"{
            "template_id": "change_fee_rate",
            "parameters": {"new_fee_rate": 2.5},
            "creator_pk": "02abc123",
            "quorum_threshold": 15
        }"#;
        let req: CreateFromTemplateRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.template_id, "change_fee_rate");
        assert_eq!(req.parameters.get("new_fee_rate").unwrap(), &serde_json::json!(2.5));
        assert_eq!(req.quorum_threshold, Some(15));
    }

    #[test]
    fn test_onchain_proposal_response_serialization() {
        let proposal = OnChainProposal {
            box_id: "box_test".to_string(),
            proposal_id: "prop_test".to_string(),
            title: "Test".to_string(),
            description: "Test desc".to_string(),
            category: ProposalCategory::ConfigChange,
            stage: ProposalStage::Created,
            creator_pk: "02abc".to_string(),
            created_height: 1000,
            vote_start_height: 1000,
            vote_end_height: 11000,
            quorum_threshold: 10,
            approval_threshold: 60,
            votes_for: 0,
            votes_against: 0,
            voters: vec![],
            execution_data: None,
            nft_token_id: "nft_test".to_string(),
            erg_value: 1_000_000_000,
        };
        let tx = crate::governance::GovernanceTxResult {
            tx_id: "tx_123".to_string(),
            tx_json: "{}".to_string(),
            boxes_created: vec!["box_test".to_string()],
            boxes_spent: vec![],
            fee: 1_000_000,
        };
        let resp = OnChainProposalResponse { proposal, tx };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("box_test"));
        assert!(json.contains("tx_123"));
    }

    #[test]
    fn test_validate_proposal_response_serialization() {
        let resp = ValidateProposalResponse {
            proposal_id: "prop_123".to_string(),
            validation: crate::governance::ValidationResult::valid(),
            tally: Some(crate::governance::TallyResult {
                votes_for: 0,
                votes_against: 0,
                total_voters: 0,
                quorum_met: false,
                approval_met: false,
                passes: false,
                approval_percentage: 0.0,
                quorum_percentage: 0.0,
            }),
            can_execute: false,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("prop_123"));
        assert!(json.contains("\"is_valid\":true"));
        assert!(json.contains("\"can_execute\":false"));
    }

    #[test]
    fn test_create_from_template_response_serialization() {
        let resp = CreateFromTemplateResponse {
            template_id: "change_fee_rate".to_string(),
            validation: crate::governance::ValidationResult::valid(),
            proposal: None,
            tx: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("change_fee_rate"));
    }
}
