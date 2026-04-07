//! Rent Guard — Middleware that deprioritizes providers whose Ergo boxes
//! are approaching storage rent expiry.
//!
//! On Ergo, every UTXO box incurs a per-byte storage fee every 2 years
//! (1,051,200 blocks).  After **four years** (2,102,400 blocks) a box whose
//! ERG value has been fully consumed by rent fees is destroyed by the
//! protocol.  The Rent Guard tracks per-provider box health and applies
//! scoring penalties so that the adaptive router avoids routing traffic to
//! providers at risk of losing their boxes.
//!
//! Integration point:
//! ```ignore
//! use crate::rent_guard::apply_rent_penalty;
//! let adjusted = apply_rent_penalty(base_score, provider_id, &guard);
//! ```

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Number of blocks in one Ergo rent cycle (2 years at 2 min/block).
pub const BLOCKS_PER_RENT_CYCLE: u64 = 1_051_200;

/// Maximum box lifetime before the protocol destroys it (4 years).
/// After two consecutive rent cycles the box is consumed if its value is
/// insufficient to cover fees.
pub const MAX_BOX_LIFETIME_BLOCKS: u64 = 2_102_400;

/// Blocks remaining before rent expiry that triggers the "Warning" risk
/// level.  720 blocks ≈ 1 day at 2-min block times.
pub const RENT_WARNING_BLOCKS: u64 = 720;

/// Blocks remaining before rent expiry that triggers the "Critical" risk
/// level.  144 blocks ≈ 4.8 hours.
pub const RENT_CRITICAL_BLOCKS: u64 = 144;

/// Penalty applied to the health score when a provider has boxes in the
/// Warning zone (within `RENT_WARNING_BLOCKS` of expiry).
pub const WARNING_SCORE_PENALTY: f64 = 0.3;

/// Penalty applied to the health score when a provider has boxes in the
/// Critical zone (within `RENT_CRITICAL_BLOCKS` of expiry).
pub const CRITICAL_SCORE_PENALTY: f64 = 0.7;

/// Default health-score threshold below which a provider is considered
/// unhealthy and should not receive routed traffic.
pub const DEFAULT_HEALTH_THRESHOLD: f64 = 0.5;

/// Default maximum age (in seconds) before a provider entry is pruned from
/// the guard during background cleanup.
pub const DEFAULT_STALE_PROVIDER_SECS: u64 = 3_600; // 1 hour

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Risk level for a provider based on the rent health of its boxes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    /// All boxes are well within their rent window.
    Healthy,
    /// At least one box is within `RENT_WARNING_BLOCKS` of the 4-year
    /// expiry boundary.
    Warning,
    /// At least one box is within `RENT_CRITICAL_BLOCKS` of expiry.
    Critical,
    /// At least one box has exceeded `MAX_BOX_LIFETIME_BLOCKS`.
    Expired,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::Healthy => write!(f, "healthy"),
            RiskLevel::Warning => write!(f, "warning"),
            RiskLevel::Critical => write!(f, "critical"),
            RiskLevel::Expired => write!(f, "expired"),
        }
    }
}

impl RiskLevel {
    /// Returns `true` if the provider should not receive routed traffic.
    pub fn is_routable(&self) -> bool {
        matches!(self, RiskLevel::Healthy | RiskLevel::Warning)
    }
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Minimal box information fed into the rent guard for scoring.
///
/// This mirrors the `BoxData` struct from `storage_rent_monitor` but is
/// kept intentionally small so callers can construct it cheaply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxInfo {
    /// Base-16 box ID.
    pub box_id: String,
    /// Block height when the box was created.
    pub creation_height: u64,
    /// Current chain height at the time of the health check.
    pub current_height: u64,
}

impl BoxInfo {
    /// Create a new `BoxInfo`.
    pub fn new(box_id: impl Into<String>, creation_height: u64, current_height: u64) -> Self {
        Self {
            box_id: box_id.into(),
            creation_height,
            current_height,
        }
    }

    /// Compute the age of this box in blocks.
    pub fn age_blocks(&self) -> u64 {
        self.current_height.saturating_sub(self.creation_height)
    }

    /// Compute the number of blocks remaining before the 4-year lifetime
    /// limit is reached.  Returns 0 if already expired.
    pub fn blocks_until_expiry(&self) -> u64 {
        MAX_BOX_LIFETIME_BLOCKS.saturating_sub(self.age_blocks())
    }
}

/// Per-provider box health snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderBoxHealth {
    /// Provider identifier (e.g. node public key or peer address).
    pub provider_id: String,
    /// Block height of the oldest box owned by this provider.
    pub oldest_box_creation_height: u64,
    /// Total number of boxes tracked for this provider.
    pub boxes_count: u32,
    /// Composite health score in [0.0, 1.0].  1.0 = fully healthy.
    pub health_score: f64,
    /// Highest-severity risk level across all boxes.
    pub risk_level: RiskLevel,
    /// Timestamp of the most recent health check.
    pub last_checked: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// RentGuard
// ---------------------------------------------------------------------------

/// Concurrent rent-health tracker for all known providers.
///
/// Internally uses a `DashMap` so that updates from the background scanner
/// and reads from the routing hot-path never contend.
#[derive(Debug, Clone)]
pub struct RentGuard {
    /// Per-provider health snapshots.
    providers: Arc<DashMap<String, ProviderBoxHealth>>,
    /// Minimum health score for a provider to be considered routable.
    health_threshold: f64,
}

use std::sync::Arc;

impl RentGuard {
    /// Create a new `RentGuard` with the default health threshold.
    pub fn new() -> Self {
        Self {
            providers: Arc::new(DashMap::new()),
            health_threshold: DEFAULT_HEALTH_THRESHOLD,
        }
    }

    /// Create a new `RentGuard` with a custom health threshold.
    pub fn with_threshold(health_threshold: f64) -> Self {
        Self {
            providers: Arc::new(DashMap::new()),
            health_threshold: health_threshold.clamp(0.0, 1.0),
        }
    }

    /// Get the current health threshold.
    pub fn health_threshold(&self) -> f64 {
        self.health_threshold
    }

    /// Update the health record for a provider given its current set of
    /// tracked boxes.
    ///
    /// The health score is computed from the oldest box age:
    /// - Base score = 1.0
    /// - Subtract `WARNING_SCORE_PENALTY` (0.3) if any box is within
    ///   `RENT_WARNING_BLOCKS` of the 4-year expiry.
    /// - Subtract `CRITICAL_SCORE_PENALTY` (0.7) if any box is within
    ///   `RENT_CRITICAL_BLOCKS` of expiry.
    /// - Score = 0.0 if any box has already expired (age >= 2,102,400).
    ///
    /// The penalties stack: a provider with critical boxes will have both
    /// the warning and critical penalties applied (1.0 - 0.3 - 0.7 = 0.0),
    /// which effectively matches the "expired" score.
    pub fn update_provider_boxes(&self, provider_id: &str, boxes: Vec<BoxInfo>) {
        let now = Utc::now();
        let boxes_count = boxes.len() as u32;

        if boxes.is_empty() {
            // No boxes tracked — mark as healthy with a fresh timestamp so
            // the provider isn't pruned as stale.
            let health = ProviderBoxHealth {
                provider_id: provider_id.to_string(),
                oldest_box_creation_height: 0,
                boxes_count: 0,
                health_score: 1.0,
                risk_level: RiskLevel::Healthy,
                last_checked: now,
            };
            self.providers.insert(provider_id.to_string(), health);
            return;
        }

        let mut oldest_creation = u64::MAX;
        let mut has_warning = false;
        let mut has_critical = false;
        let mut has_expired = false;

        for b in &boxes {
            if b.creation_height < oldest_creation {
                oldest_creation = b.creation_height;
            }

            let remaining = b.blocks_until_expiry();
            if remaining == 0 {
                has_expired = true;
            } else if remaining <= RENT_CRITICAL_BLOCKS {
                has_critical = true;
            } else if remaining <= RENT_WARNING_BLOCKS {
                has_warning = true;
            }
        }

        let (health_score, risk_level) = if has_expired {
            (0.0, RiskLevel::Expired)
        } else if has_critical {
            // Both warning and critical penalties apply.
            let score = (1.0 - WARNING_SCORE_PENALTY - CRITICAL_SCORE_PENALTY).max(0.0);
            (score, RiskLevel::Critical)
        } else if has_warning {
            let score = (1.0 - WARNING_SCORE_PENALTY).max(0.0);
            (score, RiskLevel::Warning)
        } else {
            (1.0, RiskLevel::Healthy)
        };

        let health = ProviderBoxHealth {
            provider_id: provider_id.to_string(),
            oldest_box_creation_height: oldest_creation,
            boxes_count,
            health_score,
            risk_level,
            last_checked: now,
        };
        self.providers.insert(provider_id.to_string(), health);
    }

    /// Retrieve the current health snapshot for a provider.
    ///
    /// Returns `None` if the provider has never been checked.
    pub fn get_provider_health(&self, provider_id: &str) -> Option<ProviderBoxHealth> {
        self.providers.get(provider_id).map(|r| r.value().clone())
    }

    /// Return the health score for a provider, or `None` if unknown.
    pub fn get_health_score(&self, provider_id: &str) -> Option<f64> {
        self.providers
            .get(provider_id)
            .map(|r| r.value().health_score)
    }

    /// Return the risk level for a provider, or `None` if unknown.
    pub fn get_risk_level(&self, provider_id: &str) -> Option<RiskLevel> {
        self.providers
            .get(provider_id)
            .map(|r| r.value().risk_level)
    }

    /// Return all provider IDs whose health score meets the routable
    /// threshold (≥ `health_threshold`).
    pub fn get_healthy_providers(&self) -> Vec<String> {
        self.providers
            .iter()
            .filter(|r| {
                r.value().health_score >= self.health_threshold
                    && r.value().risk_level.is_routable()
            })
            .map(|r| r.key().clone())
            .collect()
    }

    /// Compute a routing score for a provider by applying the rent-health
    /// penalty to a base score.
    ///
    /// If the provider is unknown to the guard, the base score is returned
    /// unchanged (no penalty).
    pub fn score_provider(&self, provider_id: &str, base_score: f64) -> f64 {
        match self.get_health_score(provider_id) {
            Some(health) => (base_score * health).max(0.0),
            None => base_score, // unknown provider — no penalty
        }
    }

    /// Check whether a provider is routable.
    ///
    /// Returns `false` if the provider has no health record, has a
    /// `Critical` or `Expired` risk level, or its health score is below
    /// the threshold.
    pub fn is_provider_routable(&self, provider_id: &str) -> bool {
        match self.providers.get(provider_id) {
            Some(r) => {
                r.value().risk_level.is_routable()
                    && r.value().health_score >= self.health_threshold
            }
            None => false,
        }
    }

    /// Return the total number of tracked providers.
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }

    /// Return a summary of all tracked providers.
    pub fn all_providers(&self) -> Vec<ProviderBoxHealth> {
        self.providers.iter().map(|r| r.value().clone()).collect()
    }

    /// Remove provider entries that have not been checked within
    /// `max_age_secs`.
    ///
    /// This is intended to be called periodically from a background task
    /// to prevent the map from growing unboundedly as providers come and
    /// go.
    pub fn prune_stale_providers(&self, max_age_secs: u64) -> usize {
        let cutoff = Utc::now() - Duration::from_secs(max_age_secs);
        let mut removed = 0;
        self.providers.retain(|_, health| {
            if health.last_checked < cutoff {
                removed += 1;
                false
            } else {
                true
            }
        });
        removed
    }
}

impl Default for RentGuard {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Integration helper
// ---------------------------------------------------------------------------

/// Apply a rent-health penalty to a base routing score.
///
/// This is a free function designed to be called from `adaptive_router.rs`
/// without requiring a direct dependency on the `RentGuard` struct beyond
/// a borrowed reference.
///
/// # Example
/// ```ignore
/// let adjusted = apply_rent_penalty(0.95, "provider-A", &rent_guard);
/// ```
pub fn apply_rent_penalty(base_score: f64, provider_id: &str, guard: &RentGuard) -> f64 {
    guard.score_provider(provider_id, base_score)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    // -- Helper constructors ------------------------------------------------

    /// Helper: build a `BoxInfo` with a given age (blocks) at a fixed
    /// current height.
    fn box_with_age(age_blocks: u64) -> BoxInfo {
        let current_height = 3_000_000u64;
        let creation_height = current_height.saturating_sub(age_blocks);
        BoxInfo::new("test-box", creation_height, current_height)
    }

    // -- Basic construction -------------------------------------------------

    #[test]
    fn rent_guard_default_creation() {
        let guard = RentGuard::new();
        assert_eq!(guard.provider_count(), 0);
        assert_eq!(guard.health_threshold(), DEFAULT_HEALTH_THRESHOLD);
    }

    #[test]
    fn rent_guard_custom_threshold() {
        let guard = RentGuard::with_threshold(0.8);
        assert_eq!(guard.health_threshold(), 0.8);
    }

    #[test]
    fn rent_guard_threshold_clamped() {
        let guard = RentGuard::with_threshold(2.0);
        assert_eq!(guard.health_threshold(), 1.0);
        let guard = RentGuard::with_threshold(-1.0);
        assert_eq!(guard.health_threshold(), 0.0);
    }

    // -- Empty boxes --------------------------------------------------------

    #[test]
    fn empty_boxes_marked_healthy() {
        let guard = RentGuard::new();
        guard.update_provider_boxes("p1", vec![]);
        let h = guard.get_provider_health("p1").unwrap();
        assert_eq!(h.boxes_count, 0);
        assert_eq!(h.health_score, 1.0);
        assert_eq!(h.risk_level, RiskLevel::Healthy);
    }

    // -- Healthy provider ---------------------------------------------------

    #[test]
    fn healthy_provider_full_score() {
        let guard = RentGuard::new();
        // A young box: age = 100 blocks.
        let box1 = box_with_age(100);
        guard.update_provider_boxes("p2", vec![box1]);
        let h = guard.get_provider_health("p2").unwrap();
        assert_eq!(h.health_score, 1.0);
        assert_eq!(h.risk_level, RiskLevel::Healthy);
        assert_eq!(h.boxes_count, 1);
    }

    // -- Warning zone -------------------------------------------------------

    #[test]
    fn warning_provider_penalized() {
        let guard = RentGuard::new();
        // Box age such that remaining = 600 blocks (< RENT_WARNING_BLOCKS=720).
        let age = MAX_BOX_LIFETIME_BLOCKS - 600;
        let box1 = box_with_age(age);
        guard.update_provider_boxes("p3", vec![box1]);
        let h = guard.get_provider_health("p3").unwrap();
        let expected = 1.0 - WARNING_SCORE_PENALTY;
        assert!((h.health_score - expected).abs() < 1e-9);
        assert_eq!(h.risk_level, RiskLevel::Warning);
    }

    #[test]
    fn warning_boundary_exactly() {
        let guard = RentGuard::new();
        // Exactly RENT_WARNING_BLOCKS remaining.
        let age = MAX_BOX_LIFETIME_BLOCKS - RENT_WARNING_BLOCKS;
        let box1 = box_with_age(age);
        guard.update_provider_boxes("p3b", vec![box1]);
        let h = guard.get_provider_health("p3b").unwrap();
        assert_eq!(h.risk_level, RiskLevel::Warning);
    }

    // -- Critical zone ------------------------------------------------------

    #[test]
    fn critical_provider_heavily_penalized() {
        let guard = RentGuard::new();
        // Remaining = 100 blocks (< RENT_CRITICAL_BLOCKS=144).
        let age = MAX_BOX_LIFETIME_BLOCKS - 100;
        let box1 = box_with_age(age);
        guard.update_provider_boxes("p4", vec![box1]);
        let h = guard.get_provider_health("p4").unwrap();
        let expected = (1.0 - WARNING_SCORE_PENALTY - CRITICAL_SCORE_PENALTY).max(0.0);
        assert!((h.health_score - expected).abs() < 1e-9);
        assert_eq!(h.risk_level, RiskLevel::Critical);
    }

    #[test]
    fn critical_boundary_exactly() {
        let guard = RentGuard::new();
        let age = MAX_BOX_LIFETIME_BLOCKS - RENT_CRITICAL_BLOCKS;
        let box1 = box_with_age(age);
        guard.update_provider_boxes("p4b", vec![box1]);
        let h = guard.get_provider_health("p4b").unwrap();
        assert_eq!(h.risk_level, RiskLevel::Critical);
    }

    // -- Expired ------------------------------------------------------------

    #[test]
    fn expired_provider_zero_score() {
        let guard = RentGuard::new();
        let age = MAX_BOX_LIFETIME_BLOCKS + 500;
        let box1 = box_with_age(age);
        guard.update_provider_boxes("p5", vec![box1]);
        let h = guard.get_provider_health("p5").unwrap();
        assert_eq!(h.health_score, 0.0);
        assert_eq!(h.risk_level, RiskLevel::Expired);
    }

    #[test]
    fn expired_boundary_exactly() {
        let guard = RentGuard::new();
        let age = MAX_BOX_LIFETIME_BLOCKS;
        let box1 = box_with_age(age);
        guard.update_provider_boxes("p5b", vec![box1]);
        let h = guard.get_provider_health("p5b").unwrap();
        assert_eq!(h.health_score, 0.0);
        assert_eq!(h.risk_level, RiskLevel::Expired);
    }

    // -- Multi-box: worst case wins -----------------------------------------

    #[test]
    fn multi_box_takes_worst_risk() {
        let guard = RentGuard::new();
        let healthy = box_with_age(100);
        let warning = box_with_age(MAX_BOX_LIFETIME_BLOCKS - 600);
        guard.update_provider_boxes("p6", vec![healthy, warning]);
        let h = guard.get_provider_health("p6").unwrap();
        assert_eq!(h.risk_level, RiskLevel::Warning);
        assert_eq!(h.boxes_count, 2);
    }

    #[test]
    fn multi_box_expired_dominates() {
        let guard = RentGuard::new();
        let healthy = box_with_age(100);
        let expired = box_with_age(MAX_BOX_LIFETIME_BLOCKS + 10);
        guard.update_provider_boxes("p7", vec![healthy, expired]);
        let h = guard.get_provider_health("p7").unwrap();
        assert_eq!(h.risk_level, RiskLevel::Expired);
        assert_eq!(h.health_score, 0.0);
    }

    // -- Oldest box tracking ------------------------------------------------

    #[test]
    fn oldest_box_creation_tracked() {
        let guard = RentGuard::new();
        let current = 3_000_000u64;
        let box_a = BoxInfo::new("a", 1_000_000, current);
        let box_b = BoxInfo::new("b", 500_000, current);
        guard.update_provider_boxes("p8", vec![box_a, box_b]);
        let h = guard.get_provider_health("p8").unwrap();
        assert_eq!(h.oldest_box_creation_height, 500_000);
    }

    // -- score_provider -----------------------------------------------------

    #[test]
    fn score_provider_unknown_returns_base() {
        let guard = RentGuard::new();
        assert_eq!(guard.score_provider("ghost", 0.9), 0.9);
    }

    #[test]
    fn score_provider_healthy_no_penalty() {
        let guard = RentGuard::new();
        guard.update_provider_boxes("p9", vec![box_with_age(100)]);
        assert_eq!(guard.score_provider("p9", 0.85), 0.85);
    }

    #[test]
    fn score_provider_warning_applies_penalty() {
        let guard = RentGuard::new();
        let age = MAX_BOX_LIFETIME_BLOCKS - 600;
        guard.update_provider_boxes("p10", vec![box_with_age(age)]);
        let expected = 0.85 * (1.0 - WARNING_SCORE_PENALTY);
        assert!((guard.score_provider("p10", 0.85) - expected).abs() < 1e-9);
    }

    #[test]
    fn score_provider_expired_zeroes() {
        let guard = RentGuard::new();
        guard.update_provider_boxes(
            "p11",
            vec![box_with_age(MAX_BOX_LIFETIME_BLOCKS + 1)],
        );
        assert_eq!(guard.score_provider("p11", 0.99), 0.0);
    }

    // -- is_provider_routable -----------------------------------------------

    #[test]
    fn routable_unknown_false() {
        let guard = RentGuard::new();
        assert!(!guard.is_provider_routable("nobody"));
    }

    #[test]
    fn routable_healthy_true() {
        let guard = RentGuard::new();
        guard.update_provider_boxes("p12", vec![box_with_age(100)]);
        assert!(guard.is_provider_routable("p12"));
    }

    #[test]
    fn routable_warning_true() {
        let guard = RentGuard::new();
        let age = MAX_BOX_LIFETIME_BLOCKS - 600;
        guard.update_provider_boxes("p13", vec![box_with_age(age)]);
        // Warning score = 0.7 >= threshold 0.5 → routable.
        assert!(guard.is_provider_routable("p13"));
    }

    #[test]
    fn routable_critical_false() {
        let guard = RentGuard::new();
        let age = MAX_BOX_LIFETIME_BLOCKS - 100;
        guard.update_provider_boxes("p14", vec![box_with_age(age)]);
        assert!(!guard.is_provider_routable("p14"));
    }

    #[test]
    fn routable_expired_false() {
        let guard = RentGuard::new();
        guard.update_provider_boxes(
            "p15",
            vec![box_with_age(MAX_BOX_LIFETIME_BLOCKS + 1)],
        );
        assert!(!guard.is_provider_routable("p15"));
    }

    #[test]
    fn routable_below_custom_threshold() {
        let guard = RentGuard::with_threshold(0.8);
        let age = MAX_BOX_LIFETIME_BLOCKS - 600; // score = 0.7
        guard.update_provider_boxes("p16", vec![box_with_age(age)]);
        // 0.7 < 0.8 → not routable even though risk is Warning.
        assert!(!guard.is_provider_routable("p16"));
    }

    // -- get_healthy_providers ----------------------------------------------

    #[test]
    fn get_healthy_filters_correctly() {
        let guard = RentGuard::new();
        guard.update_provider_boxes("h1", vec![box_with_age(100)]);
        guard.update_provider_boxes("h2", vec![box_with_age(500)]);
        let age = MAX_BOX_LIFETIME_BLOCKS - 600;
        guard.update_provider_boxes("w1", vec![box_with_age(age)]); // warning, score 0.7 ≥ 0.5 → included
        guard.update_provider_boxes(
            "e1",
            vec![box_with_age(MAX_BOX_LIFETIME_BLOCKS + 1)],
        );
        let healthy = guard.get_healthy_providers();
        assert!(healthy.contains(&"h1".to_string()));
        assert!(healthy.contains(&"h2".to_string()));
        assert!(healthy.contains(&"w1".to_string())); // warning still routable
        assert!(!healthy.contains(&"e1".to_string()));
    }

    // -- prune_stale_providers ----------------------------------------------

    #[test]
    fn prune_removes_old_entries() {
        let guard = RentGuard::new();
        guard.update_provider_boxes("fresh", vec![box_with_age(100)]);
        // Manually backdate an entry by inserting directly.
        let stale = ProviderBoxHealth {
            provider_id: "stale".to_string(),
            oldest_box_creation_height: 0,
            boxes_count: 0,
            health_score: 1.0,
            risk_level: RiskLevel::Healthy,
            last_checked: Utc::now() - Duration::from_secs(7200),
        };
        guard.providers.insert("stale".to_string(), stale);
        assert_eq!(guard.provider_count(), 2);
        let removed = guard.prune_stale_providers(3600);
        assert_eq!(removed, 1);
        assert_eq!(guard.provider_count(), 1);
        assert!(guard.get_provider_health("fresh").is_some());
        assert!(guard.get_provider_health("stale").is_none());
    }

    #[test]
    fn prune_zero_max_age_removes_all() {
        let guard = RentGuard::new();
        guard.update_provider_boxes("a", vec![box_with_age(100)]);
        guard.update_provider_boxes("b", vec![box_with_age(200)]);
        let removed = guard.prune_stale_providers(0);
        assert_eq!(removed, 2);
        assert_eq!(guard.provider_count(), 0);
    }

    // -- apply_rent_penalty integration helper ------------------------------

    #[test]
    fn apply_rent_penalty_passes_through() {
        let guard = RentGuard::new();
        assert_eq!(apply_rent_penalty(0.8, "unknown", &guard), 0.8);
    }

    #[test]
    fn apply_rent_penalty_applies_health() {
        let guard = RentGuard::new();
        let age = MAX_BOX_LIFETIME_BLOCKS - 600;
        guard.update_provider_boxes("pen", vec![box_with_age(age)]);
        let result = apply_rent_penalty(1.0, "pen", &guard);
        assert!((result - (1.0 - WARNING_SCORE_PENALTY)).abs() < 1e-9);
    }

    // -- BoxInfo helpers ----------------------------------------------------

    #[test]
    fn box_info_age_and_remaining() {
        let b = BoxInfo::new("x", 100, 300);
        assert_eq!(b.age_blocks(), 200);
        assert_eq!(b.blocks_until_expiry(), MAX_BOX_LIFETIME_BLOCKS - 200);
    }

    #[test]
    fn box_info_creation_after_current_zero_age() {
        let b = BoxInfo::new("x", 500, 300);
        assert_eq!(b.age_blocks(), 0);
    }

    // -- RiskLevel Display --------------------------------------------------

    #[test]
    fn risk_level_display() {
        assert_eq!(RiskLevel::Healthy.to_string(), "healthy");
        assert_eq!(RiskLevel::Warning.to_string(), "warning");
        assert_eq!(RiskLevel::Critical.to_string(), "critical");
        assert_eq!(RiskLevel::Expired.to_string(), "expired");
    }

    #[test]
    fn risk_level_is_routable() {
        assert!(RiskLevel::Healthy.is_routable());
        assert!(RiskLevel::Warning.is_routable());
        assert!(!RiskLevel::Critical.is_routable());
        assert!(!RiskLevel::Expired.is_routable());
    }

    // -- all_providers ------------------------------------------------------

    #[test]
    fn all_providers_returns_all() {
        let guard = RentGuard::new();
        guard.update_provider_boxes("a", vec![box_with_age(100)]);
        guard.update_provider_boxes("b", vec![box_with_age(200)]);
        let all = guard.all_providers();
        assert_eq!(all.len(), 2);
    }

    // -- Concurrent access --------------------------------------------------

    #[test]
    fn concurrent_updates_no_deadlock() {
        use std::sync::Arc;

        let guard = Arc::new(RentGuard::new());
        let mut handles = vec![];

        for i in 0..8 {
            let g = Arc::clone(&guard);
            let id = format!("prov-{}", i);
            handles.push(thread::spawn(move || {
                for j in 0..100 {
                    let boxes = vec![box_with_age((j * 1000) as u64)];
                    g.update_provider_boxes(&id, boxes);
                    let _ = g.get_provider_health(&id);
                    let _ = g.is_provider_routable(&id);
                    let _ = g.score_provider(&id, 0.9);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(guard.provider_count(), 8);
    }

    // -- Constants consistency ----------------------------------------------

    #[test]
    fn constants_are_sensible() {
        assert!(RENT_CRITICAL_BLOCKS < RENT_WARNING_BLOCKS);
        assert!(WARNING_SCORE_PENALTY < CRITICAL_SCORE_PENALTY);
        assert!(CRITICAL_SCORE_PENALTY <= 1.0);
        assert!(MAX_BOX_LIFETIME_BLOCKS == 2 * BLOCKS_PER_RENT_CYCLE);
    }
}
