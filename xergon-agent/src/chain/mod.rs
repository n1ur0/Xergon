//! Chain state reader module.
//!
//! Provides the interface between the xergon-agent and the Ergo blockchain.
//! This module reads box state from the Ergo node REST API, parses Sigma-serialized
//! registers into typed Rust structs, and caches results in memory.
//!
//! Submodules:
//! - [`client`] -- Ergo node REST API client
//! - [`types`] -- On-chain box type definitions
//! - [`scanner`] -- Box scanner with register parsing
//! - [`cache`] -- In-memory TTL cache for chain state
//! - [`transactions`] -- On-chain tx building (heartbeat, usage proofs)
//! - [`usage_proofs`] -- Usage proof accumulator with batched submission

pub mod cache;
pub mod client;
pub mod merkle;
pub mod scanner;
pub mod transactions;
pub mod types;
pub mod usage_proofs;
