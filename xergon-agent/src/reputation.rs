//! Peer Reputation Scoring
//!
//! Tracks per-peer reputation scores based on interactions, disputes,
//! and gossiped reputation data from other peers. Uses a concurrent
//! DashMap for thread-safe access.

use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Reputation score for a single peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationScore {
    /// Peer public key / identifier
    pub peer_pk: String,
    /// Composite score in range [min_reputation, max_reputation], starts at 0
    pub score: f64,
    /// Number of successful interactions
    pub successful_interactions: u64,
    /// Number of failed interactions
    pub failed_interactions: u64,
    /// Disputes this agent has won against the peer
    pub disputes_won: u32,
    /// Disputes this agent has lost against the peer
    pub disputes_lost: u32,
    /// Last update timestamp (unix epoch seconds)
    pub last_updated: i64,
}

/// Configuration for the reputation system.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReputationConfig {
    /// Enable reputation tracking (default: true)
    #[serde(default = "default_reputation_enabled")]
    pub enabled: bool,
    /// Decay factor applied periodically to all scores (default: 0.99)
    #[serde(default = "default_decay_factor")]
    pub decay_factor: f64,
    /// Score increment for a successful interaction (default: +1.0)
    #[serde(default = "default_positive_score")]
    pub positive_score: f64,
    /// Score decrement for a failed interaction (default: -5.0)
    #[serde(default = "default_negative_score")]
    pub negative_score: f64,
    /// Score bonus when winning a dispute (default: +10.0)
    #[serde(default = "default_dispute_won_bonus")]
    pub dispute_won_bonus: f64,
    /// Score penalty when losing a dispute (default: -20.0)
    #[serde(default = "default_dispute_lost_penalty")]
    pub dispute_lost_penalty: f64,
    /// Minimum possible reputation (default: -100.0)
    #[serde(default = "default_min_reputation")]
    pub min_reputation: f64,
    /// Maximum possible reputation (default: +100.0)
    #[serde(default = "default_max_reputation")]
    pub max_reputation: f64,
    /// Seconds between decay applications (default: 3600)
    #[serde(default = "default_decay_interval_secs")]
    pub decay_interval_secs: u64,
    /// Score threshold below which a peer is untrusted (default: -50.0)
    #[serde(default = "default_reputation_threshold")]
    pub reputation_threshold: f64,
}

fn default_reputation_enabled() -> bool { true }
fn default_decay_factor() -> f64 { 0.99 }
fn default_positive_score() -> f64 { 1.0 }
fn default_negative_score() -> f64 { -5.0 }
fn default_dispute_won_bonus() -> f64 { 10.0 }
fn default_dispute_lost_penalty() -> f64 { -20.0 }
fn default_min_reputation() -> f64 { -100.0 }
fn default_max_reputation() -> f64 { 100.0 }
fn default_decay_interval_secs() -> u64 { 3600 }
fn default_reputation_threshold() -> f64 { -50.0 }

impl Default for ReputationConfig {
    fn default() -> Self {
        Self {
            enabled: default_reputation_enabled(),
            decay_factor: default_decay_factor(),
            positive_score: default_positive_score(),
            negative_score: default_negative_score(),
            dispute_won_bonus: default_dispute_won_bonus(),
            dispute_lost_penalty: default_dispute_lost_penalty(),
            min_reputation: default_min_reputation(),
            max_reputation: default_max_reputation(),
            decay_interval_secs: default_decay_interval_secs(),
            reputation_threshold: default_reputation_threshold(),
        }
    }
}

/// Thread-safe reputation store for all peers.
#[derive(Debug, Clone)]
pub struct ReputationStore {
    scores: Arc<DashMap<String, ReputationScore>>,
    config: ReputationConfig,
}

impl ReputationStore {
    /// Create a new store with the given config.
    pub fn new(config: ReputationConfig) -> Self {
        Self {
            scores: Arc::new(DashMap::new()),
            config,
        }
    }

    /// Clamp a score to [min_reputation, max_reputation].
    fn clamp(&self, score: f64) -> f64 {
        score.clamp(self.config.min_reputation, self.config.max_reputation)
    }

    /// Record a peer interaction (success or failure).
    pub fn record_interaction(&self, peer_pk: &str, success: bool) {
        let delta = if success {
            self.config.positive_score
        } else {
            self.config.negative_score
        };

        let now = Utc::now().timestamp();

        let mut entry = self.scores.entry(peer_pk.to_string()).or_insert_with(|| ReputationScore {
            peer_pk: peer_pk.to_string(),
            score: 0.0,
            successful_interactions: 0,
            failed_interactions: 0,
            disputes_won: 0,
            disputes_lost: 0,
            last_updated: now,
        });

        entry.score = self.clamp(entry.score + delta);
        if success {
            entry.successful_interactions += 1;
        } else {
            entry.failed_interactions += 1;
        }
        entry.last_updated = now;
    }

    /// Record a dispute outcome (won or lost).
    pub fn record_dispute_outcome(&self, peer_pk: &str, won: bool) {
        let delta = if won {
            self.config.dispute_won_bonus
        } else {
            self.config.dispute_lost_penalty
        };

        let now = Utc::now().timestamp();

        let mut entry = self.scores.entry(peer_pk.to_string()).or_insert_with(|| ReputationScore {
            peer_pk: peer_pk.to_string(),
            score: 0.0,
            successful_interactions: 0,
            failed_interactions: 0,
            disputes_won: 0,
            disputes_lost: 0,
            last_updated: now,
        });

        entry.score = self.clamp(entry.score + delta);
        if won {
            entry.disputes_won += 1;
        } else {
            entry.disputes_lost += 1;
        }
        entry.last_updated = now;
    }

    /// Get the numeric score for a peer (0.0 if unknown).
    pub fn get_score(&self, peer_pk: &str) -> f64 {
        self.scores.get(peer_pk).map(|e| e.score).unwrap_or(0.0)
    }

    /// Get the full ReputationScore for a peer.
    pub fn get_reputation(&self, peer_pk: &str) -> Option<ReputationScore> {
        self.scores.get(peer_pk).map(|e| e.value().clone())
    }

    /// Get the top N peers sorted by score descending.
    pub fn get_top_peers(&self, n: usize) -> Vec<(String, f64)> {
        let mut all: Vec<(String, f64)> = self.scores
            .iter()
            .map(|e| (e.key().clone(), e.value().score))
            .collect();
        all.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        all.truncate(n);
        all
    }

    /// Apply decay factor to all scores. Called periodically.
    pub fn decay_scores(&self) {
        for mut entry in self.scores.iter_mut() {
            entry.score = self.clamp(entry.score * self.config.decay_factor);
            entry.last_updated = Utc::now().timestamp();
        }
    }

    /// Check if a peer is trusted (score > threshold).
    pub fn is_peer_trusted(&self, peer_pk: &str) -> bool {
        self.get_score(peer_pk) > self.config.reputation_threshold
    }

    /// Merge an external reputation score using weighted average.
    ///
    /// The local score gets weight 2.0, the external score gets weight 1.0,
    /// favouring direct observations over gossiped data.
    pub fn merge_reputation(&self, peer_pk: &str, external_score: f64) {
        let local_score = self.get_score(peer_pk);
        // Weight local observations more heavily than gossiped data
        let merged = (local_score * 2.0 + external_score * 1.0) / 3.0;
        let clamped = self.clamp(merged);
        let now = Utc::now().timestamp();

        let mut entry = self.scores.entry(peer_pk.to_string()).or_insert_with(|| ReputationScore {
            peer_pk: peer_pk.to_string(),
            score: 0.0,
            successful_interactions: 0,
            failed_interactions: 0,
            disputes_won: 0,
            disputes_lost: 0,
            last_updated: now,
        });

        entry.score = clamped;
        entry.last_updated = now;
    }

    /// Get all scores as (peer_pk, score) pairs for gossip broadcast.
    pub fn all_scores(&self) -> Vec<(String, f64)> {
        self.scores
            .iter()
            .map(|e| (e.key().clone(), e.value().score))
            .collect()
    }

    /// Number of peers with reputation entries.
    pub fn peer_count(&self) -> usize {
        self.scores.len()
    }

    /// Reference to the config (e.g. for reading threshold).
    pub fn config(&self) -> &ReputationConfig {
        &self.config
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn default_store() -> ReputationStore {
        ReputationStore::new(ReputationConfig::default())
    }

    #[test]
    fn test_reputation_positive_interaction() {
        let store = default_store();
        store.record_interaction("peer-A", true);
        assert_eq!(store.get_score("peer-A"), 1.0);
    }

    #[test]
    fn test_reputation_negative_interaction() {
        let store = default_store();
        store.record_interaction("peer-A", false);
        assert_eq!(store.get_score("peer-A"), -5.0);
    }

    #[test]
    fn test_reputation_multiple_interactions() {
        let store = default_store();
        store.record_interaction("peer-A", true);
        store.record_interaction("peer-A", true);
        store.record_interaction("peer-A", false);
        // 1.0 + 1.0 + (-5.0) = -3.0
        assert!((store.get_score("peer-A") - (-3.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn test_reputation_bounds_clamped_to_max() {
        let store = default_store();
        // Default max is 100.0, positive_score is 1.0
        for _ in 0..200 {
            store.record_interaction("peer-A", true);
        }
        assert_eq!(store.get_score("peer-A"), 100.0);
    }

    #[test]
    fn test_reputation_bounds_clamped_to_min() {
        let store = default_store();
        // Default min is -100.0, negative_score is -5.0
        for _ in 0..200 {
            store.record_interaction("peer-A", false);
        }
        assert_eq!(store.get_score("peer-A"), -100.0);
    }

    #[test]
    fn test_reputation_decay() {
        let store = default_store();
        store.record_interaction("peer-A", true);
        assert_eq!(store.get_score("peer-A"), 1.0);
        store.decay_scores();
        // 1.0 * 0.99 = 0.99
        assert!((store.get_score("peer-A") - 0.99).abs() < f64::EPSILON);
    }

    #[test]
    fn test_get_top_peers_ordering() {
        let store = default_store();
        store.record_interaction("low-peer", false);  // -5.0
        store.record_interaction("high-peer", true);  // +1.0
        store.record_interaction("mid-peer", true);   // +1.0
        store.record_interaction("high-peer", true);  // +2.0 total

        let top = store.get_top_peers(3);
        assert_eq!(top.len(), 3);
        assert_eq!(top[0].0, "high-peer");
        assert!((top[0].1 - 2.0).abs() < f64::EPSILON);
        assert_eq!(top[1].0, "mid-peer");
        assert!((top[1].1 - 1.0).abs() < f64::EPSILON);
        assert_eq!(top[2].0, "low-peer");
        assert!((top[2].1 - (-5.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn test_is_peer_trusted_threshold() {
        let store = default_store(); // threshold = -50.0
        // New peer defaults to 0.0 which is > -50.0
        assert!(store.is_peer_trusted("unknown-peer"));
        // Get a known peer to exactly -50.0
        store.record_interaction("edge-peer", false); // -5.0
        // Need 10 failures to hit -50.0
        for _ in 1..10 {
            store.record_interaction("edge-peer", false);
        }
        assert_eq!(store.get_score("edge-peer"), -50.0);
        // -50.0 is NOT > -50.0, so not trusted
        assert!(!store.is_peer_trusted("edge-peer"));
    }

    #[test]
    fn test_merge_reputation_weighted_average() {
        let store = default_store();
        store.record_interaction("peer-A", true); // local = 1.0
        store.merge_reputation("peer-A", 10.0);
        // merged = (1.0 * 2.0 + 10.0 * 1.0) / 3.0 = 12.0 / 3.0 = 4.0
        let score = store.get_score("peer-A");
        assert!((score - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_merge_reputation_unknown_peer() {
        let store = default_store();
        // Unknown peer: local = 0.0
        store.merge_reputation("peer-X", 30.0);
        // merged = (0.0 * 2.0 + 30.0 * 1.0) / 3.0 = 10.0
        let score = store.get_score("peer-X");
        assert!((score - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_dispute_outcome() {
        let store = default_store();
        store.record_dispute_outcome("peer-A", true);  // +10.0
        store.record_dispute_outcome("peer-A", false); // -20.0
        // 10.0 + (-20.0) = -10.0
        let score = store.get_score("peer-A");
        assert!((score - (-10.0)).abs() < f64::EPSILON);

        let rep = store.get_reputation("peer-A").unwrap();
        assert_eq!(rep.disputes_won, 1);
        assert_eq!(rep.disputes_lost, 1);
    }

    #[test]
    fn test_all_scores_for_gossip() {
        let store = default_store();
        store.record_interaction("a", true);
        store.record_interaction("b", false);
        let all = store.all_scores();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_get_reputation_unknown_returns_none() {
        let store = default_store();
        assert!(store.get_reputation("nobody").is_none());
    }
}
