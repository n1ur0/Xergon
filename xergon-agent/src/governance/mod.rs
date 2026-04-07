//! On-chain governance voting and proposal templates for the Xergon protocol.
//!
//! This module provides:
//! - **types**: On-chain proposal, vote record, governance config, and lifecycle stages
//! - **contract**: ErgoScript contract templates for proposal and config boxes
//! - **proposal_templates**: Pre-defined proposal templates with parameter schemas
//! - **onchain**: Transaction builder for creating, voting, executing, and closing proposals
//!
//! # Ergo eUTXO Model
//!
//! Governance follows the eUTXO multi-stage protocol pattern:
//! - A proposal is a box with a singleton NFT, guarded by a proposal contract
//! - Registers R4-R9 encode proposal state (stage, votes, thresholds)
//! - State transitions occur by spending the box and creating a successor
//! - Governance config is read via data inputs (not spent)

pub mod api;
pub mod contract;
pub mod onchain;
pub mod proposal_templates;
pub mod state_manager;
pub mod treasury;
pub mod types;

pub use types::{
    GovernanceConfig, GovernanceError, GovernanceTxResult, OnChainProposal, ProposalCategory,
    ProposalStage, TallyResult, ValidationResult, VoteRecord,
};
pub use types::serialize_params;
pub use proposal_templates::{
    built_in_templates, get_template, ProposalTemplate, TemplateParameter, TemplateParamType,
};
pub use onchain::OnChainGovernance;
pub use state_manager::ProposalStore;
