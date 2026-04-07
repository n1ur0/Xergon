//! REST API layer for governance and treasury management.
//!
//! Provides axum handlers and router for governance proposals,
//! voting, treasury operations, and threshold configuration.
//!
//! All proposal and vote operations are backed by [`ProposalStore`].
//! All treasury operations are backed by [`TreasuryAppState`].

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::onchain::OnChainGovernance;
use super::state_manager::ProposalStore;
use super::treasury::{DepositRecord, TreasuryAppState, TreasurySpend};
use super::types::*;

// ─── Request Types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateProposalRequest {
    pub title: String,
    pub description: String,
    pub category: ProposalCategory,
    pub proposer: String,
    pub current_height: Option<u32>,
    pub execution_data: Option<Vec<u8>>,
}

#[derive(Debug, Deserialize)]
pub struct CastVoteRequest {
    pub proposal_id: String,
    pub voter: String,
    pub direction: String, // "for" or "against"
    pub stake_amount: Option<u64>,
    pub current_height: Option<u32>,
    pub tx_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RequestSpendRequest {
    pub proposal_id: String,
    pub recipient: String,
    pub amount_nanoerg: u64,
}

#[derive(Debug, Deserialize)]
pub struct AddSignatureRequest {
    pub spend_id: String,
    pub signer: String,
}

#[derive(Debug, Deserialize)]
pub struct ExecuteSpendRequest {
    pub spend_id: String,
    pub tx_id: String,
}

#[derive(Debug, Deserialize)]
pub struct FailSpendRequest {
    pub spend_id: String,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct DepositRequest {
    pub depositor: String,
    pub amount_nanoerg: u64,
    pub tx_id: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateThresholdRequest {
    pub required_signatures: u32,
    pub signatory_addresses: Vec<String>,
}

/// Query parameters for list_proposals.
#[derive(Debug, Deserialize, Default)]
pub struct ListProposalsQuery {
    pub stage: Option<String>,
    pub category: Option<String>,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

/// Query parameters for spends history.
#[derive(Debug, Deserialize, Default)]
pub struct SpendsQuery {
    pub limit: Option<usize>,
}

// ─── Response Types ─────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ApiOk<T: Serialize> {
    pub ok: bool,
    pub data: T,
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub ok: bool,
    pub error: String,
}

#[derive(Debug, Serialize)]
pub struct GovernanceSummary {
    pub total_proposals: u64,
    pub stored_proposals: usize,
    pub by_stage: Vec<(String, usize)>,
    pub treasury_state: super::treasury::TreasuryState,
}

// ─── Parsing Helpers ────────────────────────────────────────────────

/// Parse a stage name string ("created", "voting", "executed", "closed", "expired") into a ProposalStage.
fn parse_stage(s: &str) -> Option<ProposalStage> {
    match s.to_lowercase().as_str() {
        "created" => Some(ProposalStage::Created),
        "voting" => Some(ProposalStage::Voting),
        "executed" => Some(ProposalStage::Executed),
        "closed" => Some(ProposalStage::Closed),
        "expired" => Some(ProposalStage::Expired),
        _ => None,
    }
}

// ─── Error Mapping ─────────────────────────────────────────────────

/// Map a [`GovernanceError`] to an appropriate HTTP status code.
fn gov_error_status(err: &GovernanceError) -> StatusCode {
    match err {
        GovernanceError::ProposalNotFound(_) => StatusCode::NOT_FOUND,
        GovernanceError::ProposalFinalized(_) => StatusCode::CONFLICT,
        GovernanceError::AlreadyVoted(_) => StatusCode::CONFLICT,
        GovernanceError::VotingPeriodEnded { .. } => StatusCode::CONFLICT,
        GovernanceError::VotingNotStarted { .. } => StatusCode::CONFLICT,
        GovernanceError::QuorumNotMet { .. } => StatusCode::CONFLICT,
        GovernanceError::ApprovalNotMet { .. } => StatusCode::CONFLICT,
        GovernanceError::TitleTooLong { .. }
        | GovernanceError::DescriptionTooLong { .. }
        | GovernanceError::InvalidStage(_)
        | GovernanceError::InvalidCategory(_)
        | GovernanceError::ValidationFailed(_) => StatusCode::BAD_REQUEST,
        GovernanceError::InsufficientStakeToPropose { .. }
        | GovernanceError::InsufficientStakeToVote { .. } => StatusCode::FORBIDDEN,
        GovernanceError::TemplateNotFound(_) => StatusCode::NOT_FOUND,
        GovernanceError::TemplateParamError(_) | GovernanceError::TxBuildError(_) => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

/// Map a treasury `String` error to an HTTP status code.
fn treasury_error_status(msg: &str) -> StatusCode {
    if msg.contains("not found") || msg.contains("Not found") {
        StatusCode::NOT_FOUND
    } else if msg.contains("Insufficient") || msg.contains("insufficient") {
        StatusCode::CONFLICT
    } else if msg.contains("not a valid signatory") {
        StatusCode::FORBIDDEN
    } else if msg.contains("not pending") || msg.contains("Not enough signatures") {
        StatusCode::CONFLICT
    } else {
        StatusCode::BAD_REQUEST
    }
}

// ─── App State ──────────────────────────────────────────────────────

pub struct GovernanceApiState {
    pub governance: OnChainGovernance,
    pub store: Arc<ProposalStore>,
    pub treasury: Arc<TreasuryAppState>,
}

// ─── Proposal Handlers ──────────────────────────────────────────────

/// POST /api/gov/proposal — create a new proposal via the ProposalStore.
async fn create_proposal_handler(
    State(state): State<Arc<GovernanceApiState>>,
    Json(req): Json<CreateProposalRequest>,
) -> Result<(StatusCode, Json<ApiOk<OnChainProposal>>), (StatusCode, Json<ApiError>)> {
    if req.title.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError { ok: false, error: "Title cannot be empty".into() }),
        ));
    }
    if req.description.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError { ok: false, error: "Description cannot be empty".into() }),
        ));
    }

    let current_height = req.current_height.unwrap_or(0);
    let proposal = state
        .store
        .create_proposal(
            &req.title,
            &req.description,
            req.category,
            &req.proposer,
            current_height,
            req.execution_data,
        )
        .map_err(|e| {
            (
                gov_error_status(&e),
                Json(ApiError { ok: false, error: e.to_string() }),
            )
        })?;

    Ok((StatusCode::CREATED, Json(ApiOk { ok: true, data: proposal })))
}

/// POST /api/gov/vote — cast a vote via the ProposalStore.
async fn cast_vote_handler(
    State(state): State<Arc<GovernanceApiState>>,
    Json(req): Json<CastVoteRequest>,
) -> Result<(StatusCode, Json<ApiOk<VoteRecord>>), (StatusCode, Json<ApiError>)> {
    if req.proposal_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError { ok: false, error: "Proposal ID required".into() }),
        ));
    }
    if req.voter.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError { ok: false, error: "Voter address required".into() }),
        ));
    }

    let support = match req.direction.to_lowercase().as_str() {
        "for" | "yes" | "true" | "1" => true,
        "against" | "no" | "false" | "0" => false,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiError {
                    ok: false,
                    error: format!("Invalid vote direction: '{}'. Use 'for' or 'against'.", req.direction),
                }),
            ));
        }
    };

    let voting_power = req.stake_amount.unwrap_or(1);
    let current_height = req.current_height.unwrap_or(0);
    let tx_id = req.tx_id.unwrap_or_default();

    let vote = state
        .store
        .cast_vote(
            &req.proposal_id,
            &req.voter,
            support,
            voting_power,
            current_height,
            &tx_id,
        )
        .map_err(|e| {
            (
                gov_error_status(&e),
                Json(ApiError { ok: false, error: e.to_string() }),
            )
        })?;

    Ok((StatusCode::OK, Json(ApiOk { ok: true, data: vote })))
}

/// GET /api/gov/proposals — list proposals with optional filters.
async fn list_proposals_handler(
    State(state): State<Arc<GovernanceApiState>>,
    Query(params): Query<ListProposalsQuery>,
) -> Json<ApiOk<Vec<OnChainProposal>>> {
    let stage_filter = params.stage.as_deref().and_then(parse_stage);
    let category_filter = params
        .category
        .as_deref()
        .and_then(|s| <ProposalCategory as std::str::FromStr>::from_str(s).ok());
    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(50);

    let proposals = state
        .store
        .list_proposals(stage_filter, category_filter, offset, limit);

    Json(ApiOk { ok: true, data: proposals })
}

/// GET /api/gov/proposal/:id — get a single proposal by ID.
async fn get_proposal_handler(
    State(state): State<Arc<GovernanceApiState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiOk<OnChainProposal>>, (StatusCode, Json<ApiError>)> {
    let proposal = state.store.get_proposal(&id).map_err(|e| {
        (
            gov_error_status(&e),
            Json(ApiError { ok: false, error: e.to_string() }),
        )
    })?;
    Ok(Json(ApiOk { ok: true, data: proposal }))
}

/// GET /api/gov/proposal/:id/tally — tally votes on a proposal.
async fn tally_proposal_handler(
    State(state): State<Arc<GovernanceApiState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiOk<TallyResult>>, (StatusCode, Json<ApiError>)> {
    let tally = state.store.tally_proposal(&id).map_err(|e| {
        (
            gov_error_status(&e),
            Json(ApiError { ok: false, error: e.to_string() }),
        )
    })?;
    Ok(Json(ApiOk { ok: true, data: tally }))
}

/// POST /api/gov/proposal/:id/advance — advance proposal to next stage.
async fn advance_stage_handler(
    State(state): State<Arc<GovernanceApiState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiOk<String>>, (StatusCode, Json<ApiError>)> {
    let new_stage = state.store.advance_stage(&id).map_err(|e| {
        (
            gov_error_status(&e),
            Json(ApiError { ok: false, error: e.to_string() }),
        )
    })?;
    let stage_name = format!("{:?}", new_stage);
    Ok(Json(ApiOk { ok: true, data: stage_name }))
}

/// POST /api/gov/proposal/:id/execute — execute a finalized proposal.
async fn execute_proposal_handler(
    State(state): State<Arc<GovernanceApiState>>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<ApiOk<String>>), (StatusCode, Json<ApiError>)> {
    // First try advancing — if it passes, the proposal is executed
    let new_stage = state.store.advance_stage(&id).map_err(|e| {
        (
            gov_error_status(&e),
            Json(ApiError { ok: false, error: e.to_string() }),
        )
    })?;

    if new_stage == ProposalStage::Executed {
        Ok((
            StatusCode::OK,
            Json(ApiOk {
                ok: true,
                data: format!("Proposal {} executed successfully", id),
            }),
        ))
    } else {
        Err((
            StatusCode::CONFLICT,
            Json(ApiError {
                ok: false,
                error: format!(
                    "Proposal not executed — advanced to {:?} instead. Ensure quorum and approval thresholds are met.",
                    new_stage
                ),
            }),
        ))
    }
}

/// POST /api/gov/cancel/:id — cancel a proposal.
async fn cancel_proposal_handler(
    State(state): State<Arc<GovernanceApiState>>,
    Path(id): Path<String>,
) -> Result<Json<ApiOk<String>>, (StatusCode, Json<ApiError>)> {
    let stage = state.store.cancel_proposal(&id).map_err(|e| {
        (
            gov_error_status(&e),
            Json(ApiError { ok: false, error: e.to_string() }),
        )
    })?;
    Ok(Json(ApiOk {
        ok: true,
        data: format!("Proposal {} cancelled (stage: {:?})", id, stage),
    }))
}

// ─── Treasury Handlers ──────────────────────────────────────────────

/// GET /api/gov/treasury — treasury state snapshot.
async fn treasury_state_handler(
    State(state): State<Arc<GovernanceApiState>>,
) -> Json<ApiOk<super::treasury::TreasuryState>> {
    let ts = state.treasury.get_state();
    Json(ApiOk { ok: true, data: ts })
}

/// POST /api/gov/treasury/deposit — record a deposit.
async fn treasury_deposit_handler(
    State(state): State<Arc<GovernanceApiState>>,
    Json(req): Json<DepositRequest>,
) -> Result<(StatusCode, Json<ApiOk<DepositRecord>>), (StatusCode, Json<ApiError>)> {
    if req.depositor.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError { ok: false, error: "Depositor address required".into() }),
        ));
    }
    if req.amount_nanoerg == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError { ok: false, error: "Amount must be > 0".into() }),
        ));
    }
    if req.tx_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError { ok: false, error: "Transaction ID required".into() }),
        ));
    }

    let record = state.treasury.deposit(&req.depositor, req.amount_nanoerg, &req.tx_id);
    Ok((StatusCode::CREATED, Json(ApiOk { ok: true, data: record })))
}

/// POST /api/gov/treasury/spend — request a new spend.
async fn request_spend_handler(
    State(state): State<Arc<GovernanceApiState>>,
    Json(req): Json<RequestSpendRequest>,
) -> Result<(StatusCode, Json<ApiOk<TreasurySpend>>), (StatusCode, Json<ApiError>)> {
    let spend = state
        .treasury
        .request_spend(&req.proposal_id, &req.recipient, req.amount_nanoerg)
        .map_err(|e| {
            (
                treasury_error_status(&e),
                Json(ApiError { ok: false, error: e }),
            )
        })?;
    Ok((StatusCode::CREATED, Json(ApiOk { ok: true, data: spend })))
}

/// POST /api/gov/treasury/sign — add a signature to a pending spend.
async fn add_signature_handler(
    State(state): State<Arc<GovernanceApiState>>,
    Json(req): Json<AddSignatureRequest>,
) -> Result<(StatusCode, Json<ApiOk<TreasurySpend>>), (StatusCode, Json<ApiError>)> {
    let spend = state
        .treasury
        .add_signature(&req.spend_id, &req.signer)
        .map_err(|e| {
            (
                treasury_error_status(&e),
                Json(ApiError { ok: false, error: e }),
            )
        })?;
    Ok((StatusCode::OK, Json(ApiOk { ok: true, data: spend })))
}

/// POST /api/gov/treasury/execute — execute a fully-signed spend.
async fn execute_spend_handler(
    State(state): State<Arc<GovernanceApiState>>,
    Json(req): Json<ExecuteSpendRequest>,
) -> Result<(StatusCode, Json<ApiOk<TreasurySpend>>), (StatusCode, Json<ApiError>)> {
    let spend = state
        .treasury
        .execute_spend(&req.spend_id, &req.tx_id)
        .map_err(|e| {
            (
                treasury_error_status(&e),
                Json(ApiError { ok: false, error: e }),
            )
        })?;
    Ok((StatusCode::OK, Json(ApiOk { ok: true, data: spend })))
}

/// POST /api/gov/treasury/fail — fail a pending spend and unlock funds.
async fn fail_spend_handler(
    State(state): State<Arc<GovernanceApiState>>,
    Json(req): Json<FailSpendRequest>,
) -> Result<(StatusCode, Json<ApiOk<TreasurySpend>>), (StatusCode, Json<ApiError>)> {
    let spend = state
        .treasury
        .fail_spend(&req.spend_id, &req.reason)
        .map_err(|e| {
            (
                treasury_error_status(&e),
                Json(ApiError { ok: false, error: e }),
            )
        })?;
    Ok((StatusCode::OK, Json(ApiOk { ok: true, data: spend })))
}

/// GET /api/gov/treasury/history — spend history.
async fn treasury_history_handler(
    State(state): State<Arc<GovernanceApiState>>,
    Query(params): Query<SpendsQuery>,
) -> Json<ApiOk<Vec<TreasurySpend>>> {
    let limit = params.limit.unwrap_or(100);
    let spends = state.treasury.get_spends(limit);
    Json(ApiOk { ok: true, data: spends })
}

// ─── Threshold Handlers ─────────────────────────────────────────────

/// GET /api/gov/threshold — get current threshold config.
async fn get_threshold_handler(
    State(state): State<Arc<GovernanceApiState>>,
) -> Json<ApiOk<super::treasury::ThresholdConfig>> {
    let threshold = state.treasury.threshold.read().unwrap().clone();
    Json(ApiOk { ok: true, data: threshold })
}

/// PUT /api/gov/threshold — update threshold config.
async fn update_threshold_handler(
    State(state): State<Arc<GovernanceApiState>>,
    Json(req): Json<UpdateThresholdRequest>,
) -> Result<(StatusCode, Json<ApiOk<super::treasury::ThresholdConfig>>), (StatusCode, Json<ApiError>)> {
    let updated = state
        .treasury
        .update_threshold(req.required_signatures, req.signatory_addresses)
        .map_err(|e| {
            (
                treasury_error_status(&e),
                Json(ApiError { ok: false, error: e }),
            )
        })?;
    Ok((StatusCode::OK, Json(ApiOk { ok: true, data: updated })))
}

// ─── Summary Handler ────────────────────────────────────────────────

/// GET /api/gov/summary — governance + treasury summary.
async fn governance_summary_handler(
    State(state): State<Arc<GovernanceApiState>>,
) -> Json<ApiOk<GovernanceSummary>> {
    let treasury_state = state.treasury.get_state();
    let total_proposals = state.store.proposal_count();
    let stored_proposals = state.store.len();

    // Build per-stage counts
    let all = state.store.list_proposals(None, None, 0, 0);
    let mut stage_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for p in &all {
        let key = format!("{:?}", p.stage);
        *stage_counts.entry(key).or_insert(0) += 1;
    }
    let mut by_stage: Vec<(String, usize)> = stage_counts.into_iter().collect();
    by_stage.sort_by(|a, b| a.0.cmp(&b.0));

    let summary = GovernanceSummary {
        total_proposals,
        stored_proposals,
        by_stage,
        treasury_state,
    };
    Json(ApiOk { ok: true, data: summary })
}

// ─── Router ─────────────────────────────────────────────────────────

pub fn build_router(state: Arc<GovernanceApiState>) -> Router {
    Router::new()
        // Proposal CRUD
        .route("/api/gov/proposal", axum::routing::post(create_proposal_handler))
        .route("/api/gov/proposals", axum::routing::get(list_proposals_handler))
        .route("/api/gov/proposal/:id", axum::routing::get(get_proposal_handler))
        .route("/api/gov/proposal/:id/tally", axum::routing::get(tally_proposal_handler))
        .route("/api/gov/proposal/:id/advance", axum::routing::post(advance_stage_handler))
        .route("/api/gov/proposal/:id/execute", axum::routing::post(execute_proposal_handler))
        .route("/api/gov/cancel/:id", axum::routing::post(cancel_proposal_handler))
        // Voting
        .route("/api/gov/vote", axum::routing::post(cast_vote_handler))
        // Treasury
        .route("/api/gov/treasury", axum::routing::get(treasury_state_handler))
        .route("/api/gov/treasury/deposit", axum::routing::post(treasury_deposit_handler))
        .route("/api/gov/treasury/spend", axum::routing::post(request_spend_handler))
        .route("/api/gov/treasury/sign", axum::routing::post(add_signature_handler))
        .route("/api/gov/treasury/execute", axum::routing::post(execute_spend_handler))
        .route("/api/gov/treasury/fail", axum::routing::post(fail_spend_handler))
        .route("/api/gov/treasury/history", axum::routing::get(treasury_history_handler))
        // Threshold
        .route("/api/gov/threshold", axum::routing::get(get_threshold_handler))
        .route("/api/gov/threshold", axum::routing::put(update_threshold_handler))
        // Summary
        .route("/api/gov/summary", axum::routing::get(governance_summary_handler))
        .with_state(state)
}
