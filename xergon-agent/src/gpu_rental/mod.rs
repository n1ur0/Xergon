//! GPU Rental module (Phase 4 — GPU Bazar).
//!
//! Provides on-chain GPU rental infrastructure:
//! - Listing boxes that providers create to advertise their GPU
//! - Rental boxes with time-boxed escrow (prepaid ERG, auto-refund if unused)
//! - UTXO scanning for available GPU listings
//! - Transaction building for creating listings, renting, claiming, refunding
//! - Usage metering (session tracking, hourly deduction, expiration)
//! - SSH/Jupyter tunnel management (ssh2-based)
//! - Rating and reputation system (Phase 4 final)
//!
//! Submodules:
//! - [`types`] -- GpuListingBox, GpuRentalBox, and request/response types
//! - [`scanner`] -- Scan UTXO set for GPU listing and rental boxes
//! - [`transactions`] -- Build on-chain txs for listing, renting, claiming, refunding
//! - [`metering`] -- Usage metering: session tracking, hourly deduction, expiration
//! - [`tunnel`] -- SSH/Jupyter tunnel management using ssh2
//! - [`rating`] -- Rating submission, scanning, and reputation aggregation

pub mod metering;
pub mod rating;
pub mod scanner;
pub mod transactions;
pub mod tunnel;
pub mod types;
