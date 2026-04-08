//! Reputation Dashboard API -- aggregates reputation data into a dashboard-friendly format.
//!
//! Provides endpoints for:
//! - Leaderboard (top N providers by reputation score)
//! - Provider reputation detail (rating, stars, interactions, disputes)
//! - Network-wide aggregate stats (avg rating, total providers, dispute rate)
//! - Reputation history over time (score snapshots)
//!
//! API endpoints:
//! - GET /api/reputation/leaderboard?limit=20
//! - GET /api/reputation/provider/{pk}
//! - GET /api/reputation/stats
//! - GET /api/reputation/history/{pk}?days=30

use chrono::{Duration, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::reputation::ReputationStore;

// ---------------------------------------------------------------------------
// Data types for API responses
// ---------------------------------------------------------------------------

/// A single entry in the leaderboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    /// Provider public key.
    pub peer_pk: String,
    /// Current composite reputation score.
    pub score: f64,
    /// Number of successful interactions.
    pub successful_interactions: u64,
    /// Number of failed interactions.
    pub failed_interactions: u64,
    /// Total interactions.
    pub total_interactions: u64,
    /// Success rate (0.0 - 1.0), or null if no interactions.
    pub success_rate: Option<f64>,
    /// Disputes won by this agent against the peer.
    pub disputes_won: u32,
    /// Disputes lost by this agent against the peer.
    pub disputes_lost: u32,
    /// Star rating derived from score (1.0 - 5.0).
    pub stars: f64,
    /// Trust status.
    pub trusted: bool,
    /// Rank in the leaderboard (1-based).
    pub rank: usize,
}

/// Detailed breakdown for a single provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderReputationDetail {
    /// Provider public key.
    pub peer_pk: String,
    /// Current composite reputation score.
    pub score: f64,
    /// Star rating (1.0 - 5.0).
    pub stars: f64,
    /// Number of successful interactions.
    pub successful_interactions: u64,
    /// Number of failed interactions.
    pub failed_interactions: u64,
    /// Total interactions.
    pub total_interactions: u64,
    /// Success rate (0.0 - 1.0).
    pub success_rate: Option<f64>,
    /// Disputes won.
    pub disputes_won: u32,
    /// Disputes lost.
    pub disputes_lost: u32,
    /// Total disputes.
    pub total_disputes: u32,
    /// Dispute win rate (0.0 - 1.0).
    pub dispute_win_rate: Option<f64>,
    /// Trust status.
    pub trusted: bool,
    /// Last update timestamp (RFC3339).
    pub last_updated: String,
    /// Reputation tier.
    pub tier: String,
}

/// Network-wide aggregate reputation statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkReputationStats {
    /// Total number of tracked providers.
    pub total_providers: usize,
    /// Average reputation score across all providers.
    pub average_score: f64,
    /// Median reputation score.
    pub median_score: f64,
    /// Highest reputation score.
    pub max_score: f64,
    /// Lowest reputation score.
    pub min_score: f64,
    /// Number of trusted providers.
    pub trusted_count: usize,
    /// Number of untrusted providers.
    pub untrusted_count: usize,
    /// Total successful interactions across all providers.
    pub total_successful_interactions: u64,
    /// Total failed interactions across all providers.
    pub total_failed_interactions: u64,
    /// Total disputes across all providers.
    pub total_disputes: u32,
    /// Dispute rate (disputes / total interactions).
    pub dispute_rate: f64,
    /// Provider count by tier.
    pub tier_distribution: TierDistribution,
}

/// Distribution of providers across reputation tiers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierDistribution {
    pub legendary: usize,
    pub excellent: usize,
    pub good: usize,
    pub neutral: usize,
    pub poor: usize,
    pub toxic: usize,
}

/// A single data point in reputation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationHistoryPoint {
    /// Timestamp (RFC3339).
    pub timestamp: String,
    /// Score at this point in time.
    pub score: f64,
    /// Number of interactions recorded up to this point.
    pub cumulative_interactions: u64,
}

/// Reputation history response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationHistory {
    pub peer_pk: String,
    pub current_score: f64,
    pub days_requested: usize,
    pub data_points: Vec<ReputationHistoryPoint>,
}

// ---------------------------------------------------------------------------
// ReputationHistoryStore
// ---------------------------------------------------------------------------

/// Stores periodic snapshots of reputation scores for history tracking.
/// Uses a DashMap for thread-safe concurrent access.
pub struct ReputationHistoryStore {
    /// peer_pk -> Vec<(timestamp_secs, score, cumulative_interactions)>
    snapshots: Arc<DashMap<String, Vec<(i64, f64, u64)>>>,
    /// Maximum number of snapshots to keep per peer.
    max_snapshots_per_peer: usize,
}

impl ReputationHistoryStore {
    /// Create a new history store.
    pub fn new(max_snapshots_per_peer: usize) -> Self {
        Self {
            snapshots: Arc::new(DashMap::new()),
            max_snapshots_per_peer,
        }
    }

    /// Record a snapshot for a peer.
    pub fn record_snapshot(&self, peer_pk: &str, score: f64, cumulative_interactions: u64) {
        let now = Utc::now().timestamp();
        let mut entry = self.snapshots.entry(peer_pk.to_string()).or_default();
        entry.push((now, score, cumulative_interactions));

        // Trim old snapshots
        if entry.len() > self.max_snapshots_per_peer {
            let drain_from = entry.len() - self.max_snapshots_per_peer;
            entry.drain(..drain_from);
        }
    }

    /// Get history for a peer within the last `days` days.
    pub fn get_history(&self, peer_pk: &str, days: usize) -> Vec<ReputationHistoryPoint> {
        let cutoff = Utc::now() - Duration::days(days as i64);
        let cutoff_secs = cutoff.timestamp();

        match self.snapshots.get(peer_pk) {
            Some(entry) => entry
                .iter()
                .filter(|(ts, _, _)| *ts >= cutoff_secs)
                .map(|(ts, score, interactions)| ReputationHistoryPoint {
                    timestamp: chrono::DateTime::from_timestamp(*ts, 0)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_else(|| "unknown".to_string()),
                    score: *score,
                    cumulative_interactions: *interactions,
                })
                .collect(),
            None => vec![],
        }
    }

    /// Number of peers with history.
    pub fn peer_count(&self) -> usize {
        self.snapshots.len()
    }
}

// ---------------------------------------------------------------------------
// ReputationDashboard
// ---------------------------------------------------------------------------

/// Dashboard service that aggregates reputation data from the ReputationStore
/// into API-friendly formats.
pub struct ReputationDashboard {
    /// Reference to the main reputation store.
    reputation: Arc<ReputationStore>,
    /// History store for time-series data.
    history: Arc<ReputationHistoryStore>,
}

impl ReputationDashboard {
    /// Create a new dashboard backed by the given reputation store.
    pub fn new(reputation: Arc<ReputationStore>) -> Self {
        let history = Arc::new(ReputationHistoryStore::new(10000));
        Self { reputation, history }
    }

    /// Get a reference to the history store (for recording snapshots externally).
    pub fn history_store(&self) -> Arc<ReputationHistoryStore> {
        self.history.clone()
    }

    // ---- Public API methods ----

    /// Get the leaderboard: top N providers sorted by reputation score descending.
    pub fn get_leaderboard(&self, limit: usize) -> Vec<LeaderboardEntry> {
        let top_peers = self.reputation.get_top_peers(limit);

        let entries: Vec<LeaderboardEntry> = top_peers
            .iter()
            .enumerate()
            .map(|(idx, (peer_pk, score))| {
                let rep = self.reputation.get_reputation(peer_pk);
                let successful = rep.as_ref().map(|r| r.successful_interactions).unwrap_or(0);
                let failed = rep.as_ref().map(|r| r.failed_interactions).unwrap_or(0);
                let total = successful + failed;
                let success_rate = if total > 0 {
                    Some(successful as f64 / total as f64)
                } else {
                    None
                };
                let disputes_won = rep.as_ref().map(|r| r.disputes_won).unwrap_or(0);
                let disputes_lost = rep.as_ref().map(|r| r.disputes_lost).unwrap_or(0);

                LeaderboardEntry {
                    peer_pk: peer_pk.clone(),
                    score: *score,
                    successful_interactions: successful,
                    failed_interactions: failed,
                    total_interactions: total,
                    success_rate,
                    disputes_won,
                    disputes_lost,
                    stars: score_to_stars(*score),
                    trusted: self.reputation.is_peer_trusted(peer_pk),
                    rank: idx + 1,
                }
            })
            .collect();

        entries
    }

    /// Get detailed reputation breakdown for a single provider.
    pub fn get_provider_reputation_detail(&self, pk: &str) -> Option<ProviderReputationDetail> {
        let rep = self.reputation.get_reputation(pk)?;

        let total_interactions = rep.successful_interactions + rep.failed_interactions;
        let success_rate = if total_interactions > 0 {
            Some(rep.successful_interactions as f64 / total_interactions as f64)
        } else {
            None
        };

        let total_disputes = rep.disputes_won + rep.disputes_lost;
        let dispute_win_rate = if total_disputes > 0 {
            Some(rep.disputes_won as f64 / total_disputes as f64)
        } else {
            None
        };

        Some(ProviderReputationDetail {
            peer_pk: rep.peer_pk.clone(),
            score: rep.score,
            stars: score_to_stars(rep.score),
            successful_interactions: rep.successful_interactions,
            failed_interactions: rep.failed_interactions,
            total_interactions,
            success_rate,
            disputes_won: rep.disputes_won,
            disputes_lost: rep.disputes_lost,
            total_disputes,
            dispute_win_rate,
            trusted: self.reputation.is_peer_trusted(pk),
            last_updated: chrono::DateTime::from_timestamp(rep.last_updated, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| "unknown".to_string()),
            tier: score_to_tier(rep.score),
        })
    }

    /// Get aggregate network-wide reputation statistics.
    pub fn get_network_stats(&self) -> NetworkReputationStats {
        let all_scores = self.reputation.all_scores();
        let total_providers = all_scores.len();

        if total_providers == 0 {
            return NetworkReputationStats {
                total_providers: 0,
                average_score: 0.0,
                median_score: 0.0,
                max_score: 0.0,
                min_score: 0.0,
                trusted_count: 0,
                untrusted_count: 0,
                total_successful_interactions: 0,
                total_failed_interactions: 0,
                total_disputes: 0,
                dispute_rate: 0.0,
                tier_distribution: TierDistribution {
                    legendary: 0,
                    excellent: 0,
                    good: 0,
                    neutral: 0,
                    poor: 0,
                    toxic: 0,
                },
            };
        }

        let mut scores: Vec<f64> = all_scores.iter().map(|(_, s)| *s).collect();
        scores.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let average_score = scores.iter().sum::<f64>() / scores.len() as f64;
        let median_score = {
            let mid = scores.len() / 2;
            if scores.len() % 2 == 0 && mid > 0 {
                (scores[mid - 1] + scores[mid]) / 2.0
            } else {
                scores[mid]
            }
        };
        let max_score = scores.last().copied().unwrap_or(0.0);
        let min_score = scores.first().copied().unwrap_or(0.0);

        let mut trusted_count = 0usize;
        let mut total_successful = 0u64;
        let mut total_failed = 0u64;
        let mut total_disputes = 0u32;
        let mut tier_dist = TierDistribution {
            legendary: 0,
            excellent: 0,
            good: 0,
            neutral: 0,
            poor: 0,
            toxic: 0,
        };

        for (pk, _score) in &all_scores {
            if self.reputation.is_peer_trusted(pk) {
                trusted_count += 1;
            }
            if let Some(rep) = self.reputation.get_reputation(pk) {
                total_successful += rep.successful_interactions;
                total_failed += rep.failed_interactions;
                total_disputes += rep.disputes_won + rep.disputes_lost;

                match rep.score {
                    s if s >= 80.0 => tier_dist.legendary += 1,
                    s if s >= 50.0 => tier_dist.excellent += 1,
                    s if s >= 20.0 => tier_dist.good += 1,
                    s if s >= -20.0 => tier_dist.neutral += 1,
                    s if s >= -50.0 => tier_dist.poor += 1,
                    _ => tier_dist.toxic += 1,
                }
            }
        }

        let total_interactions = total_successful + total_failed;
        let dispute_rate = if total_interactions > 0 {
            total_disputes as f64 / total_interactions as f64
        } else {
            0.0
        };

        NetworkReputationStats {
            total_providers,
            average_score,
            median_score,
            max_score,
            min_score,
            trusted_count,
            untrusted_count: total_providers - trusted_count,
            total_successful_interactions: total_successful,
            total_failed_interactions: total_failed,
            total_disputes,
            dispute_rate,
            tier_distribution: tier_dist,
        }
    }

    /// Get reputation score history for a provider over the last `days` days.
    pub fn get_reputation_history(&self, pk: &str, days: usize) -> ReputationHistory {
        let current_score = self.reputation.get_score(pk);
        let data_points = self.history.get_history(pk, days);

        ReputationHistory {
            peer_pk: pk.to_string(),
            current_score,
            days_requested: days,
            data_points,
        }
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Convert a reputation score to a 1-5 star rating.
///
/// Maps the score range [-100, +100] to [1.0, 5.0] using a linear mapping.
fn score_to_stars(score: f64) -> f64 {
    // Map [-100, 100] -> [1.0, 5.0]
    // midpoint (0.0) -> 3.0
    let normalized = (score + 100.0) / 200.0; // 0.0 to 1.0
    let stars = 1.0 + normalized * 4.0; // 1.0 to 5.0
    (stars * 10.0).round() / 10.0 // Round to 1 decimal
}

/// Convert a reputation score to a tier string.
fn score_to_tier(score: f64) -> String {
    match score {
        s if s >= 80.0 => "Legendary".to_string(),
        s if s >= 50.0 => "Excellent".to_string(),
        s if s >= 20.0 => "Good".to_string(),
        s if s >= -20.0 => "Neutral".to_string(),
        s if s >= -50.0 => "Poor".to_string(),
        _ => "Toxic".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reputation::{ReputationConfig, ReputationStore};

    fn default_store() -> ReputationStore {
        ReputationStore::new(ReputationConfig::default())
    }

    fn default_dashboard() -> ReputationDashboard {
        let store = Arc::new(default_store());
        ReputationDashboard::new(store)
    }

    #[test]
    fn test_score_to_stars_range() {
        assert_eq!(score_to_stars(-100.0), 1.0);
        assert_eq!(score_to_stars(0.0), 3.0);
        assert_eq!(score_to_stars(100.0), 5.0);
    }

    #[test]
    fn test_score_to_tier() {
        assert_eq!(score_to_tier(90.0), "Legendary");
        assert_eq!(score_to_tier(60.0), "Excellent");
        assert_eq!(score_to_tier(30.0), "Good");
        assert_eq!(score_to_tier(0.0), "Neutral");
        assert_eq!(score_to_tier(-30.0), "Poor");
        assert_eq!(score_to_tier(-80.0), "Toxic");
    }

    #[test]
    fn test_leaderboard_empty() {
        let dashboard = default_dashboard();
        let lb = dashboard.get_leaderboard(10);
        assert!(lb.is_empty());
    }

    #[test]
    fn test_leaderboard_ordering() {
        let store = Arc::new(default_store());
        // Create some peers with different scores
        store.record_interaction("peer-A", true); // +1.0
        store.record_interaction("peer-A", true); // +2.0
        store.record_interaction("peer-B", true); // +1.0
        store.record_interaction("peer-C", false); // -5.0

        let dashboard = ReputationDashboard::new(store);
        let lb = dashboard.get_leaderboard(10);
        assert_eq!(lb.len(), 3);
        assert_eq!(lb[0].peer_pk, "peer-A");
        assert_eq!(lb[0].rank, 1);
        assert_eq!(lb[1].peer_pk, "peer-B");
        assert_eq!(lb[1].rank, 2);
        assert_eq!(lb[2].peer_pk, "peer-C");
        assert_eq!(lb[2].rank, 3);
    }

    #[test]
    fn test_provider_detail() {
        let store = Arc::new(default_store());
        store.record_interaction("peer-A", true);
        store.record_interaction("peer-A", true);
        store.record_interaction("peer-A", false);

        let dashboard = ReputationDashboard::new(store);
        let detail = dashboard.get_provider_reputation_detail("peer-A").unwrap();

        assert_eq!(detail.peer_pk, "peer-A");
        assert_eq!(detail.successful_interactions, 2);
        assert_eq!(detail.failed_interactions, 1);
        assert_eq!(detail.total_interactions, 3);
        assert!((detail.success_rate.unwrap() - 0.6667).abs() < 0.001);
        assert!(detail.trusted); // score = -3.0 > threshold -50.0
    }

    #[test]
    fn test_provider_detail_not_found() {
        let dashboard = default_dashboard();
        assert!(dashboard.get_provider_reputation_detail("nonexistent").is_none());
    }

    #[test]
    fn test_network_stats_empty() {
        let dashboard = default_dashboard();
        let stats = dashboard.get_network_stats();
        assert_eq!(stats.total_providers, 0);
        assert_eq!(stats.average_score, 0.0);
    }

    #[test]
    fn test_network_stats_with_data() {
        let store = Arc::new(default_store());
        store.record_interaction("a", true);
        store.record_interaction("b", false);

        let dashboard = ReputationDashboard::new(store);
        let stats = dashboard.get_network_stats();

        assert_eq!(stats.total_providers, 2);
        assert_eq!(stats.total_successful_interactions, 1);
        assert_eq!(stats.total_failed_interactions, 1);
        assert_eq!(stats.trusted_count, 2); // both > -50
        assert_eq!(stats.untrusted_count, 0);
    }

    #[test]
    fn test_history_store() {
        let history = ReputationHistoryStore::new(100);
        history.record_snapshot("peer-A", 5.0, 10);
        history.record_snapshot("peer-A", 10.0, 20);

        let points = history.get_history("peer-A", 30);
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].score, 5.0);
        assert_eq!(points[1].score, 10.0);

        // Unknown peer returns empty
        let empty = history.get_history("unknown", 30);
        assert!(empty.is_empty());
    }

    #[test]
    fn test_history_trimming() {
        let history = ReputationHistoryStore::new(3);
        history.record_snapshot("peer-A", 1.0, 1);
        history.record_snapshot("peer-A", 2.0, 2);
        history.record_snapshot("peer-A", 3.0, 3);
        history.record_snapshot("peer-A", 4.0, 4); // Should trim the oldest

        let points = history.get_history("peer-A", 30);
        assert_eq!(points.len(), 3);
        assert_eq!(points[0].score, 2.0); // First one was trimmed
    }

    #[test]
    fn test_reputation_history_integration() {
        let store = Arc::new(default_store());
        let dashboard = ReputationDashboard::new(store.clone());

        store.record_interaction("peer-X", true);

        // Record some history
        dashboard.history_store().record_snapshot("peer-X", 1.0, 1);
        dashboard.history_store().record_snapshot("peer-X", 2.0, 2);

        let history = dashboard.get_reputation_history("peer-X", 30);
        assert_eq!(history.peer_pk, "peer-X");
        assert_eq!(history.current_score, 1.0);
        assert_eq!(history.data_points.len(), 2);
    }
}
