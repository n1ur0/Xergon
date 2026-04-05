//! Protocol module -- implements the headless dApp pattern for Xergon.
//!
//! Submodules:
//! - [`bootstrap`] -- Protocol genesis deployment and provider registration
//! - [`specs`] -- Box specification validation rules
//! - [`equations`] -- Pure functions for protocol calculations
//! - [`actions`] -- Transaction builder stubs (requires ergo-lib-wasm for full impl)

pub mod actions;
pub mod bootstrap;
pub mod equations;
pub mod specs;
pub mod tx_safety;
