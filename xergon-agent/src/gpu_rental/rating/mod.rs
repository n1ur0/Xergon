//! GPU Bazar rating and reputation system.
//!
//! Provides on-chain rating submission and off-chain reputation aggregation:
//! - [`types`] -- RatingBox, SubmitRatingRequest, SubmitRatingResponse, Reputation
//! - [`scanner`] -- Scan UTXO set for GPU rating boxes
//! - [`transactions`] -- Build on-chain txs for submitting ratings
//!
//! Reputation is computed as the average of all rating boxes referencing a
//! given public key. The relay reads rating boxes from UTXO to compute scores.

pub mod scanner;
pub mod transactions;
pub mod types;
