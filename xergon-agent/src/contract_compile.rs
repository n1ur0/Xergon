//! Contract compilation pipeline -- embedded hex loader with config overrides.
//!
//! Contracts are pre-compiled to ErgoTree hex and embedded at compile time via
//! `include_str!`. The [`get_contract_hex`] function checks for config overrides
//! first (set via `[contracts]` section in config.toml or `XERGON__CONTRACTS__*`
//! env vars), then falls back to the embedded hex.
//!
//! # Validation
//!
//! On startup, call [`validate_all_contracts`] to log the status of every
//! embedded contract. Each hex is checked for valid base16 and minimum length.

use std::collections::HashMap;
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// Embedded compiled hex (loaded at compile time via include_str!)
// ---------------------------------------------------------------------------

/// Embedded contract hex: provider_box.es
static PROVIDER_BOX_HEX: &str =
    include_str!("../contracts/compiled/provider_box.hex");

/// Embedded contract hex: provider_registration.es
static PROVIDER_REGISTRATION_HEX: &str =
    include_str!("../contracts/compiled/provider_registration.hex");

/// Embedded contract hex: treasury_box.es
static TREASURY_BOX_HEX: &str =
    include_str!("../contracts/compiled/treasury_box.hex");

/// Embedded contract hex: usage_proof.es
static USAGE_PROOF_HEX: &str =
    include_str!("../contracts/compiled/usage_proof.hex");

/// Embedded contract hex: user_staking.es
static USER_STAKING_HEX: &str =
    include_str!("../contracts/compiled/user_staking.hex");

/// Embedded contract hex: gpu_rental.es (placeholder - not yet compiled)
static GPU_RENTAL_HEX: &str =
    include_str!("../contracts/compiled/gpu_rental.hex");

/// Embedded contract hex: usage_commitment.es (placeholder - not yet compiled)
static USAGE_COMMITMENT_HEX: &str =
    include_str!("../contracts/compiled/usage_commitment.hex");

/// Embedded contract hex: relay_registry.es (placeholder - not yet compiled)
static RELAY_REGISTRY_HEX: &str =
    include_str!("../contracts/compiled/relay_registry.hex");

/// Embedded contract hex: gpu_rating.es (placeholder - not yet compiled)
static GPU_RATING_HEX: &str =
    include_str!("../contracts/compiled/gpu_rating.hex");

/// Embedded contract hex: gpu_rental_listing.es (placeholder - not yet compiled)
static GPU_RENTAL_LISTING_HEX: &str =
    include_str!("../contracts/compiled/gpu_rental_listing.hex");

/// Embedded contract hex: payment_bridge.es (placeholder - not yet compiled)
static PAYMENT_BRIDGE_HEX: &str =
    include_str!("../contracts/compiled/payment_bridge.hex");

/// Embedded contract hex: provider_slashing.es (placeholder - not yet compiled)
static PROVIDER_SLASHING_HEX: &str =
    include_str!("../contracts/compiled/provider_slashing.hex");

/// Embedded contract hex: governance_proposal.es (placeholder - not yet compiled)
static GOVERNANCE_PROPOSAL_HEX: &str =
    include_str!("../contracts/compiled/governance_proposal.hex");

// ---------------------------------------------------------------------------
// Config overrides (set at runtime via AgentConfig)
// ---------------------------------------------------------------------------

/// Runtime config overrides for contract hex values.
/// Populated once from AgentConfig on startup.
static CONFIG_OVERRIDES: OnceLock<HashMap<String, String>> = OnceLock::new();

/// Initialize config overrides from the agent configuration.
///
/// Call this once during startup with the contract hex overrides from config.
/// Values from `[contracts]` in config.toml take precedence over embedded hex.
pub fn init_config_overrides(overrides: HashMap<String, String>) {
    let _ = CONFIG_OVERRIDES.set(overrides);
}

// ---------------------------------------------------------------------------
// Contract registry
// ---------------------------------------------------------------------------

/// A single entry in the contract registry.
struct ContractEntry {
    /// Logical contract name (e.g., "provider_box")
    name: &'static str,
    /// Embedded ErgoTree hex (trimmed, pre-compiled)
    embedded_hex: &'static str,
    /// Config key for override (e.g., "provider_box_hex")
    config_key: &'static str,
}

/// All known contracts, in canonical order.
const CONTRACT_REGISTRY: &[ContractEntry] = &[
    ContractEntry {
        name: "provider_box",
        embedded_hex: PROVIDER_BOX_HEX,
        config_key: "provider_box_hex",
    },
    ContractEntry {
        name: "provider_registration",
        embedded_hex: PROVIDER_REGISTRATION_HEX,
        config_key: "provider_registration_hex",
    },
    ContractEntry {
        name: "treasury_box",
        embedded_hex: TREASURY_BOX_HEX,
        config_key: "treasury_box_hex",
    },
    ContractEntry {
        name: "usage_proof",
        embedded_hex: USAGE_PROOF_HEX,
        config_key: "usage_proof_hex",
    },
    ContractEntry {
        name: "user_staking",
        embedded_hex: USER_STAKING_HEX,
        config_key: "user_staking_hex",
    },
    ContractEntry {
        name: "gpu_rental",
        embedded_hex: GPU_RENTAL_HEX,
        config_key: "gpu_rental_hex",
    },
    ContractEntry {
        name: "usage_commitment",
        embedded_hex: USAGE_COMMITMENT_HEX,
        config_key: "usage_commitment_hex",
    },
    ContractEntry {
        name: "relay_registry",
        embedded_hex: RELAY_REGISTRY_HEX,
        config_key: "relay_registry_hex",
    },
    ContractEntry {
        name: "gpu_rating",
        embedded_hex: GPU_RATING_HEX,
        config_key: "gpu_rating_hex",
    },
    ContractEntry {
        name: "gpu_rental_listing",
        embedded_hex: GPU_RENTAL_LISTING_HEX,
        config_key: "gpu_rental_listing_hex",
    },
    ContractEntry {
        name: "payment_bridge",
        embedded_hex: PAYMENT_BRIDGE_HEX,
        config_key: "payment_bridge_hex",
    },
    ContractEntry {
        name: "provider_slashing",
        embedded_hex: PROVIDER_SLASHING_HEX,
        config_key: "provider_slashing_hex",
    },
    ContractEntry {
        name: "governance_proposal",
        embedded_hex: GOVERNANCE_PROPOSAL_HEX,
        config_key: "governance_proposal_hex",
    },
];

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Returns the compiled ErgoTree hex for a contract by name.
///
/// Resolution order:
/// 1. Config override (from `[contracts]` section or `XERGON__CONTRACTS__*` env)
/// 2. Embedded hex (compiled into the binary at build time)
///
/// Returns `None` if the contract name is not recognized.
pub fn get_contract_hex(name: &str) -> Option<String> {
    let entry = CONTRACT_REGISTRY
        .iter()
        .find(|e| e.name == name)?;

    // Check config override first
    if let Some(overrides) = CONFIG_OVERRIDES.get() {
        if let Some(override_hex) = overrides.get(entry.config_key) {
            if !override_hex.is_empty() {
                return Some(override_hex.clone());
            }
        }
    }

    // Fall back to embedded hex
    let hex = entry.embedded_hex.trim();
    if hex.is_empty() {
        return None;
    }

    Some(hex.to_string())
}

/// Lists all available contract names.
pub fn list_contracts() -> Vec<&'static str> {
    CONTRACT_REGISTRY.iter().map(|e| e.name).collect()
}

/// Validates a hex string as valid base16.
///
/// Returns `true` if the string contains only hex characters (0-9, a-f, A-F).
pub fn validate_hex(hex: &str) -> bool {
    let trimmed = hex.trim();
    if trimmed.is_empty() {
        return false;
    }
    trimmed.chars().all(|c| c.is_ascii_hexdigit())
}

/// Validates all embedded contracts and returns a summary.
///
/// Logs the status of each contract via `tracing` and returns:
/// - (total, valid, invalid) counts
pub fn validate_all_contracts() -> (usize, usize, usize) {
    use tracing::{info, warn};

    let mut valid = 0usize;
    let mut invalid = 0usize;
    let total = CONTRACT_REGISTRY.len();

    info!(count = total, "Validating embedded contracts");

    for entry in CONTRACT_REGISTRY {
        let hex = entry.embedded_hex.trim();
        let hex_len = hex.len();

        if !validate_hex(hex) {
            warn!(
                contract = entry.name,
                status = "INVALID_HEX",
                "Contract hex is not valid base16"
            );
            invalid += 1;
            continue;
        }

        if hex_len < 64 {
            warn!(
                contract = entry.name,
                hex_len,
                status = "SHORT",
                "Contract hex is shorter than expected (min 64 chars)"
            );
            invalid += 1;
            continue;
        }

        // Check if config override is active
        let source = if let Some(overrides) = CONFIG_OVERRIDES.get() {
            if let Some(override_hex) = overrides.get(entry.config_key) {
                if !override_hex.is_empty() {
                    "config_override"
                } else {
                    "embedded"
                }
            } else {
                "embedded"
            }
        } else {
            "embedded"
        };

        info!(
            contract = entry.name,
            hex_len,
            source,
            "Contract loaded"
        );
        valid += 1;
    }

    (total, valid, invalid)
}

/// Returns all contract hex values as a map of name -> hex string.
///
/// Useful for debugging or API endpoints that expose contract info.
pub fn get_all_contract_hexes() -> HashMap<String, String> {
    let mut map = HashMap::new();
    for entry in CONTRACT_REGISTRY {
        if let Some(hex) = get_contract_hex(entry.name) {
            map.insert(entry.name.to_string(), hex);
        }
    }
    map
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_contracts_returns_all() {
        let contracts = list_contracts();
        assert!(contracts.contains(&"provider_box"));
        assert!(contracts.contains(&"usage_proof"));
        assert!(contracts.contains(&"user_staking"));
        assert!(contracts.contains(&"treasury_box"));
        assert!(contracts.contains(&"provider_registration"));
        assert!(contracts.contains(&"gpu_rental"));
        assert!(contracts.contains(&"usage_commitment"));
        assert!(contracts.contains(&"relay_registry"));
        assert!(contracts.contains(&"gpu_rating"));
        assert!(contracts.contains(&"gpu_rental_listing"));
        assert!(contracts.contains(&"payment_bridge"));
        assert!(contracts.contains(&"provider_slashing"));
        assert!(contracts.contains(&"governance_proposal"));
        assert_eq!(contracts.len(), 13);
    }

    #[test]
    fn test_get_contract_hex_returns_embedded() {
        // Use usage_proof which is not overridden by other tests
        let hex = get_contract_hex("usage_proof").expect("usage_proof should exist");
        assert!(hex.len() >= 64, "hex should be at least 64 chars, got {}", hex.len());
    }

    #[test]
    fn test_get_contract_hex_unknown_returns_none() {
        assert!(get_contract_hex("nonexistent_contract").is_none());
    }

    #[test]
    fn test_validate_hex_valid() {
        assert!(validate_hex("abcdef0123456789"));
        assert!(validate_hex("ABCDEF0123456789"));
        assert!(validate_hex("100804020e361002"));
    }

    #[test]
    fn test_validate_hex_invalid() {
        assert!(!validate_hex(""));
        assert!(!validate_hex("   "));
        assert!(!validate_hex("ghij"));  // non-hex chars
        assert!(!validate_hex("hello world"));
    }

    #[test]
    fn test_validate_hex_with_whitespace() {
        // Hex with leading/trailing whitespace should still validate
        assert!(validate_hex("  abcdef0123456789  "));
    }

    #[test]
    fn test_get_all_contract_hexes() {
        let map = get_all_contract_hexes();
        assert_eq!(map.len(), 13);
        for (name, hex) in map.values().enumerate() {
            assert!(validate_hex(hex), "hex for contract index {} is not valid hex", name);
            // Note: provider_box_hex may be overridden to a short value by test_config_override_precedence
            // Placeholder contracts may have short hex; only check non-empty
            if hex.len() < 64 {
                assert!(hex.len() >= 4, "hex should have some content, got {}", hex.len());
            }
        }
    }

    #[test]
    fn test_config_override_precedence() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "provider_box_hex".to_string(),
            "aabbccdd".to_string(), // intentionally short for test
        );
        init_config_overrides(overrides);

        let hex = get_contract_hex("provider_box").unwrap();
        assert_eq!(hex, "aabbccdd");

        // Non-overridden contract should still return embedded
        let usage_hex = get_contract_hex("usage_proof").unwrap();
        assert!(usage_hex.len() >= 64);

        // Note: OnceLock cannot be reset, so subsequent tests using
        // config overrides will see the same overrides. This is by design --
        // config is initialized once per process lifetime.
    }

    #[test]
    fn test_validate_all_contracts() {
        let (total, valid, _invalid) = validate_all_contracts();
        assert_eq!(total, 13);
        // 8 placeholder contracts have short hex (below 64-char minimum),
        // so they count as SHORT/invalid. Only the 5 original contracts should be valid.
        assert!(valid >= 5);
    }
}
