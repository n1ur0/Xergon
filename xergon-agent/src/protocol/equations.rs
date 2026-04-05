//! Pure economic and scoring equations for the Xergon protocol.
//!
//! These functions perform all fee calculations, unit conversions, and
//! pown-score adjustments without any I/O or external dependencies.

/// Base inference rate in nanoERG per token.
const BASE_RATE_PER_TOKEN: u64 = 10;

/// Minimum box value mandated by the Ergo protocol (0.001 ERG).
const SAFE_MIN_BOX_VALUE: u64 = 1_000_000;

/// Multiplier applied to "large" model inference fees.
const LARGE_MODEL_MULTIPLIER: u64 = 3;

/// Multiplier applied to "medium" model inference fees.
const MEDIUM_MODEL_MULTIPLIER: u64 = 2;

/// Maximum pown score value.
const MAX_POWN_SCORE: i32 = 1000;

// ---------------------------------------------------------------------------
// Fee calculations
// ---------------------------------------------------------------------------

/// Determine the model size multiplier from the model name.
///
/// - "large" models (contain `70b`, `72b`, `8x7b`, `405b`) → 3×
/// - "medium" models (contain `32b`, `34b`, `8b`, `7b`) → 2×
/// - everything else → 1×
fn model_multiplier(model: &str) -> u64 {
    let lower = model.to_lowercase();
    // Check large patterns first (order matters — "7b" is a prefix of "70b").
    let large_patterns = ["70b", "72b", "8x7b", "405b"];
    let medium_patterns = ["32b", "34b", "8b", "7b"];

    for pat in &large_patterns {
        if lower.contains(pat) {
            return LARGE_MODEL_MULTIPLIER;
        }
    }
    for pat in &medium_patterns {
        if lower.contains(pat) {
            return MEDIUM_MODEL_MULTIPLIER;
        }
    }
    1
}

/// Calculate the inference fee in nanoERGs for a given token count and model.
///
/// `fee = token_count × BASE_RATE × model_multiplier`
pub fn fee_for_tokens(token_count: i32, model: &str) -> u64 {
    if token_count < 0 {
        return 0;
    }
    let tokens = token_count as u64;
    tokens * BASE_RATE_PER_TOKEN * model_multiplier(model)
}

// ---------------------------------------------------------------------------
// Box value
// ---------------------------------------------------------------------------

/// Returns the Ergo-mandated minimum box value (0.001 ERG = 1_000_000 nanoERG).
pub fn min_box_value() -> u64 {
    SAFE_MIN_BOX_VALUE
}

// ---------------------------------------------------------------------------
// Pown score
// ---------------------------------------------------------------------------

/// Compute the effective pown score from the on-chain value with off-chain
/// adjustments for node health.
///
/// - If the node is **not synced**, cap at 50% of `chain_score`.
/// - If the node has **fewer than 3 peers**, cap at 75% of `chain_score`.
/// - Both penalties can stack (not-synced + low peers = min of both caps).
/// - Final value is clamped to [0, 1000].
pub fn pown_score_from_chain(chain_score: i32, node_synced: bool, peer_count: u32) -> i32 {
    let mut score = chain_score;

    if !node_synced {
        score = score / 2; // 50% cap
    }
    if peer_count < 3 {
        score = score * 3 / 4; // 75% cap
    }

    score.clamp(0, MAX_POWN_SCORE)
}

// ---------------------------------------------------------------------------
// Affordability
// ---------------------------------------------------------------------------

/// Returns `true` when `balance_nanoerg` is sufficient to cover `fee`.
pub fn can_afford(balance_nanoerg: u64, fee: u64) -> bool {
    balance_nanoerg >= fee
}

// ---------------------------------------------------------------------------
// Unit conversions
// ---------------------------------------------------------------------------

/// Convert ERG to nanoERG.
///
/// # Panics
/// Panics if `erg` is negative or the result overflows `u64`.
pub fn erg_to_nanoerg(erg: f64) -> u64 {
    assert!(
        erg >= 0.0,
        "erg_to_nanoerg: ERG value must be non-negative, got {erg}"
    );
    (erg * 1_000_000_000.0) as u64
}

/// Convert nanoERG to ERG.
pub fn nanoerg_to_erg(nanoerg: u64) -> f64 {
    nanoerg as f64 / 1_000_000_000.0
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ----- model_multiplier ------------------------------------------------

    #[test]
    fn test_model_multiplier_small() {
        assert_eq!(model_multiplier("llama-3.2-1b"), 1);
        assert_eq!(model_multiplier("phi-3-mini"), 1);
        assert_eq!(model_multiplier("gpt2"), 1);
    }

    #[test]
    fn test_model_multiplier_medium() {
        assert_eq!(model_multiplier("llama-3.1-8b"), 2);
        assert_eq!(model_multiplier("llama-3.1-7b"), 2);
        assert_eq!(model_multiplier("llama-3.1-32b"), 2);
        assert_eq!(model_multiplier("qwen-34b"), 2);
        // Case-insensitive
        assert_eq!(model_multiplier("QWEN-7B-Chat"), 2);
    }

    #[test]
    fn test_model_multiplier_large() {
        assert_eq!(model_multiplier("llama-3.1-70b"), 3);
        assert_eq!(model_multiplier("llama-3.1-72b"), 3);
        assert_eq!(model_multiplier("mixtral-8x7b"), 3);
        assert_eq!(model_multiplier("falcon-405b"), 3);
    }

    // ----- fee_for_tokens --------------------------------------------------

    #[test]
    fn test_fee_for_tokens_small_model() {
        // 100 tokens × 10 nanoERG × 1 = 1000 nanoERG
        assert_eq!(fee_for_tokens(100, "llama-3.2-1b"), 1000);
    }

    #[test]
    fn test_fee_for_tokens_medium_model() {
        // 100 tokens × 10 nanoERG × 2 = 2000 nanoERG
        assert_eq!(fee_for_tokens(100, "llama-3.1-8b"), 2000);
    }

    #[test]
    fn test_fee_for_tokens_large_model() {
        // 100 tokens × 10 nanoERG × 3 = 3000 nanoERG
        assert_eq!(fee_for_tokens(100, "llama-3.1-70b"), 3000);
    }

    #[test]
    fn test_fee_for_tokens_zero_tokens() {
        assert_eq!(fee_for_tokens(0, "llama-3.1-8b"), 0);
    }

    #[test]
    fn test_fee_for_tokens_negative_tokens() {
        assert_eq!(fee_for_tokens(-5, "llama-3.1-8b"), 0);
    }

    // ----- min_box_value ---------------------------------------------------

    #[test]
    fn test_min_box_value() {
        assert_eq!(min_box_value(), 1_000_000);
    }

    // ----- pown_score_from_chain --------------------------------------------

    #[test]
    fn test_pown_score_no_penalties() {
        assert_eq!(pown_score_from_chain(800, true, 10), 800);
    }

    #[test]
    fn test_pown_score_not_synced() {
        // 800 / 2 = 400
        assert_eq!(pown_score_from_chain(800, false, 10), 400);
    }

    #[test]
    fn test_pown_score_low_peers() {
        // 800 * 3/4 = 600
        assert_eq!(pown_score_from_chain(800, true, 2), 600);
    }

    #[test]
    fn test_pown_score_both_penalties() {
        // 800 / 2 = 400, then 400 * 3/4 = 300
        assert_eq!(pown_score_from_chain(800, false, 2), 300);
    }

    #[test]
    fn test_pown_score_clamp_upper() {
        assert_eq!(pown_score_from_chain(2000, true, 10), 1000);
    }

    #[test]
    fn test_pown_score_clamp_zero() {
        assert_eq!(pown_score_from_chain(-500, true, 10), 0);
    }

    #[test]
    fn test_pown_score_zero_input() {
        assert_eq!(pown_score_from_chain(0, false, 0), 0);
    }

    #[test]
    fn test_pown_score_boundary_peers() {
        // peer_count == 3 should NOT trigger the penalty
        assert_eq!(pown_score_from_chain(800, true, 3), 800);
        // peer_count == 2 should trigger
        assert_eq!(pown_score_from_chain(800, true, 2), 600);
    }

    // ----- can_afford ------------------------------------------------------

    #[test]
    fn test_can_afford_sufficient() {
        assert!(can_afford(5_000_000, 1_000_000));
    }

    #[test]
    fn test_can_afford_exact() {
        assert!(can_afford(1_000_000, 1_000_000));
    }

    #[test]
    fn test_can_afford_insufficient() {
        assert!(!can_afford(500_000, 1_000_000));
    }

    #[test]
    fn test_can_afford_zero_fee() {
        assert!(can_afford(0, 0));
    }

    // ----- conversions -----------------------------------------------------

    #[test]
    fn test_erg_to_nanoerg() {
        assert_eq!(erg_to_nanoerg(1.0), 1_000_000_000);
        assert_eq!(erg_to_nanoerg(0.001), 1_000_000);
        assert_eq!(erg_to_nanoerg(0.0), 0);
    }

    #[test]
    fn test_nanoerg_to_erg() {
        let epsilon = 1e-12;
        assert!((nanoerg_to_erg(1_000_000_000) - 1.0).abs() < epsilon);
        assert!((nanoerg_to_erg(1_000_000) - 0.001).abs() < epsilon);
        assert!((nanoerg_to_erg(0) - 0.0).abs() < epsilon);
    }

    #[test]
    fn test_roundtrip_conversion() {
        let original = 3.14159265;
        let nano = erg_to_nanoerg(original);
        let back = nanoerg_to_erg(nano);
        assert!((back - original).abs() < 1e-6);
    }

    #[test]
    #[should_panic(expected = "non-negative")]
    fn test_erg_to_nanoerg_negative_panics() {
        erg_to_nanoerg(-1.0);
    }
}
