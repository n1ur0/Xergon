//! Gossip Protocol Improvements
//!
//! Provides message deduplication, fanout control, and reputation
//! broadcasting/merging for P2P gossip between Xergon agents.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;

/// Gossip configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GossipConfig {
    /// Enable gossip subsystem (default: true)
    #[serde(default = "default_gossip_enabled")]
    pub enabled: bool,
    /// Seconds between gossip heartbeat cycles (default: 30)
    #[serde(default = "default_heartbeat_interval_secs")]
    pub heartbeat_interval_secs: u64,
    /// Maximum peers to gossip with (default: 50)
    #[serde(default = "default_max_peers")]
    pub max_peers: usize,
    /// Fanout: forward messages to N random peers (default: 3)
    #[serde(default = "default_fanout")]
    pub fanout: usize,
    /// Deduplication window in seconds (default: 60)
    #[serde(default = "default_dedup_window_secs")]
    pub dedup_window_secs: u64,
    /// Message time-to-live in seconds (default: 300)
    #[serde(default = "default_message_ttl_secs")]
    pub message_ttl_secs: u64,
}

fn default_gossip_enabled() -> bool { true }
fn default_heartbeat_interval_secs() -> u64 { 30 }
fn default_max_peers() -> usize { 50 }
fn default_fanout() -> usize { 3 }
fn default_dedup_window_secs() -> u64 { 60 }
fn default_message_ttl_secs() -> u64 { 300 }

impl Default for GossipConfig {
    fn default() -> Self {
        Self {
            enabled: default_gossip_enabled(),
            heartbeat_interval_secs: default_heartbeat_interval_secs(),
            max_peers: default_max_peers(),
            fanout: default_fanout(),
            dedup_window_secs: default_dedup_window_secs(),
            message_ttl_secs: default_message_ttl_secs(),
        }
    }
}

/// A single reputation score entry for gossip broadcast.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipReputationEntry {
    pub peer_pk: String,
    pub score: f64,
}

/// A gossip message carrying reputation data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationGossipMessage {
    /// Sender's peer ID
    pub sender_pk: String,
    /// Reputation scores from the sender
    pub scores: Vec<GossipReputationEntry>,
    /// Unix timestamp when sent
    pub timestamp: i64,
    /// Unique message ID for dedup
    pub message_id: String,
}

/// Gossip engine providing dedup, fanout, and reputation integration.
pub struct GossipEngine {
    config: GossipConfig,
    /// Seen message IDs -> expiry timestamp (unix epoch seconds)
    seen_messages: Arc<DashMap<String, i64>>,
}

impl GossipEngine {
    /// Create a new gossip engine.
    pub fn new(config: GossipConfig) -> Self {
        Self {
            config,
            seen_messages: Arc::new(DashMap::new()),
        }
    }

    /// Check if a message has already been seen.
    /// Returns true if the message is a duplicate (or expired from window).
    /// If the message is new, it is recorded and false is returned.
    pub fn check_and_record_message(&self, message_id: &str) -> bool {
        let now = chrono::Utc::now().timestamp();
        let expiry = now + self.config.dedup_window_secs as i64;

        // Check if already seen
        if let Some(entry) = self.seen_messages.get(message_id) {
            if *entry.value() > now {
                // Still within dedup window -> duplicate
                return true;
            }
            // Expired, will be overwritten below
        }

        self.seen_messages.insert(message_id.to_string(), expiry);
        false
    }

    /// Remove expired entries from the dedup window.
    /// Returns the number of entries removed.
    pub fn cleanup_expired(&self) -> usize {
        let now = chrono::Utc::now().timestamp();
        let expired: Vec<String> = self.seen_messages
            .iter()
            .filter(|entry| *entry.value() <= now)
            .map(|entry| entry.key().clone())
            .collect();

        let count = expired.len();
        for key in expired {
            self.seen_messages.remove(&key);
        }
        count
    }

    /// Select up to `fanout` random peers from a candidate list.
    /// This controls gossip dissemination to avoid flooding.
    pub fn select_fanout_peers(&self, candidates: &[String]) -> Vec<String> {
        if candidates.len() <= self.config.fanout {
            return candidates.to_vec();
        }
        // Simple random selection without replacement using index shuffling
        let mut indices: Vec<usize> = (0..candidates.len()).collect();
        // Fisher-Yates partial shuffle for fanout elements
        use rand::Rng;
        let mut rng = rand::thread_rng();
        for i in 0..self.config.fanout {
            let j = rng.gen_range(i..candidates.len());
            indices.swap(i, j);
        }
        indices[..self.config.fanout]
            .iter()
            .map(|&i| candidates[i].clone())
            .collect()
    }

    /// Build a reputation gossip message from a list of (peer_pk, score) pairs.
    pub fn build_reputation_gossip(
        &self,
        sender_pk: &str,
        scores: Vec<(String, f64)>,
    ) -> ReputationGossipMessage {
        let entries: Vec<GossipReputationEntry> = scores
            .into_iter()
            .map(|(peer_pk, score)| GossipReputationEntry { peer_pk, score })
            .collect();

        ReputationGossipMessage {
            sender_pk: sender_pk.to_string(),
            scores: entries,
            timestamp: chrono::Utc::now().timestamp(),
            message_id: format!("rep-{}", uuid::Uuid::new_v4()),
        }
    }

    /// Get the number of messages currently in the dedup window.
    pub fn dedup_size(&self) -> usize {
        self.seen_messages.len()
    }

    /// Reference to config.
    pub fn config(&self) -> &GossipConfig {
        &self.config
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn default_engine() -> GossipEngine {
        GossipEngine::new(GossipConfig::default())
    }

    #[test]
    fn test_dedup_rejects_duplicate() {
        let engine = default_engine();
        assert!(!engine.check_and_record_message("msg-1"));
        assert!(engine.check_and_record_message("msg-1")); // duplicate
    }

    #[test]
    fn test_dedup_accepts_different_messages() {
        let engine = default_engine();
        assert!(!engine.check_and_record_message("msg-1"));
        assert!(!engine.check_and_record_message("msg-2"));
        assert!(!engine.check_and_record_message("msg-3"));
    }

    #[test]
    fn test_dedup_cleanup_expired_entries() {
        let engine = default_engine();
        engine.check_and_record_message("msg-1");
        engine.check_and_record_message("msg-2");
        assert_eq!(engine.dedup_size(), 2);

        // Manually insert an expired entry
        let past = chrono::Utc::now().timestamp() - 100;
        engine.seen_messages.insert("msg-1".to_string(), past);

        let removed = engine.cleanup_expired();
        assert_eq!(removed, 1);
        assert_eq!(engine.dedup_size(), 1);
    }

    #[test]
    fn test_fanout_selects_correct_count() {
        let engine = GossipEngine::new(GossipConfig {
            fanout: 3,
            ..GossipConfig::default()
        });

        let candidates: Vec<String> = (0..10).map(|i| format!("peer-{}", i)).collect();
        let selected = engine.select_fanout_peers(&candidates);
        assert_eq!(selected.len(), 3);
        // All selected must be from candidates
        for p in &selected {
            assert!(candidates.contains(p));
        }
    }

    #[test]
    fn test_fanout_handles_fewer_candidates_than_fanout() {
        let engine = GossipEngine::new(GossipConfig {
            fanout: 10,
            ..GossipConfig::default()
        });

        let candidates = vec!["a".to_string(), "b".to_string()];
        let selected = engine.select_fanout_peers(&candidates);
        assert_eq!(selected.len(), 2);
    }

    #[test]
    fn test_fanout_no_duplicates() {
        let engine = GossipEngine::new(GossipConfig {
            fanout: 5,
            ..GossipConfig::default()
        });

        let candidates: Vec<String> = (0..20).map(|i| format!("peer-{}", i)).collect();
        let selected = engine.select_fanout_peers(&candidates);
        let set: HashSet<&String> = selected.iter().collect();
        assert_eq!(set.len(), selected.len(), "fanout should not contain duplicates");
    }

    #[test]
    fn test_reputation_gossip_message_format() {
        let engine = default_engine();
        let scores = vec![
            ("peer-A".to_string(), 50.0),
            ("peer-B".to_string(), -10.0),
        ];
        let msg = engine.build_reputation_gossip("self-pk", scores);

        assert_eq!(msg.sender_pk, "self-pk");
        assert_eq!(msg.scores.len(), 2);
        assert_eq!(msg.scores[0].peer_pk, "peer-A");
        assert!((msg.scores[0].score - 50.0).abs() < f64::EPSILON);
        assert_eq!(msg.scores[1].peer_pk, "peer-B");
        assert!((msg.scores[1].score - (-10.0)).abs() < f64::EPSILON);
        assert!(msg.message_id.starts_with("rep-"));
        assert!(msg.timestamp > 0);
    }

    #[test]
    fn test_reputation_gossip_serialization_roundtrip() {
        let engine = default_engine();
        let scores = vec![("pk1".to_string(), 42.0)];
        let msg = engine.build_reputation_gossip("me", scores);

        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: ReputationGossipMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.sender_pk, "me");
        assert_eq!(deserialized.scores.len(), 1);
        assert_eq!(deserialized.message_id, msg.message_id);
    }

    #[test]
    fn test_gossip_config_defaults() {
        let cfg = GossipConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.heartbeat_interval_secs, 30);
        assert_eq!(cfg.max_peers, 50);
        assert_eq!(cfg.fanout, 3);
        assert_eq!(cfg.dedup_window_secs, 60);
        assert_eq!(cfg.message_ttl_secs, 300);
    }
}
