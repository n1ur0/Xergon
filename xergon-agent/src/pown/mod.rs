//! Proof-of-Node-Work (PoNW) scoring
//!
//! Combines three categories of work into a verifiable score:
//! 1. Node Work — uptime, sync, peer count, height accuracy
//! 2. Network Work — peer confirmations received
//! 3. AI Work — tokens processed, requests served

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::node_health::NodeHealthState;
use crate::config::XergonConfig;

/// Current PoNW status (exposed via API)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PownStatus {
    pub ai_enabled: bool,
    pub ai_model: String,
    pub ai_points: u64,
    pub ai_total_requests: u64,
    pub ai_total_tokens: u64,
    pub ai_weight: u64,
    pub ergo_address: String,
    pub last_agreement: u64,
    pub last_tick_ts: i64,
    pub node_id: String,
    pub peers_checked: usize,
    pub total_xergon_confirmations: usize,
    pub unique_xergon_peers_seen: usize,
    pub work_points: u64,
}

/// Weight configuration for PoNW scoring
#[derive(Debug, Clone)]
pub struct PownWeights {
    pub node_weight: f64,     // Weight for node health
    pub network_weight: f64,  // Weight for peer confirmations
    pub ai_weight: f64,       // Weight for AI inference
    pub uptime_weight: f64,   // Within node category
    pub sync_weight: f64,
    pub peer_count_weight: f64,
}

impl Default for PownWeights {
    fn default() -> Self {
        Self {
            node_weight: 0.4,
            network_weight: 0.3,
            ai_weight: 0.3,
            uptime_weight: 0.5,
            sync_weight: 0.3,
            peer_count_weight: 0.2,
        }
    }
}

/// The PoNW calculator
pub struct PownCalculator {
    config: XergonConfig,
    weights: PownWeights,
    status: Arc<RwLock<PownStatus>>,
    start_time: i64,
}

impl PownCalculator {
    pub fn new(config: XergonConfig) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            config,
            weights: PownWeights::default(),
            status: Arc::new(RwLock::new(PownStatus {
                ai_enabled: false,
                ai_model: String::new(),
                ai_points: 0,
                ai_total_requests: 0,
                ai_total_tokens: 0,
                ai_weight: 1,
                ergo_address: String::new(),
                last_agreement: 0,
                last_tick_ts: now,
                node_id: String::new(),
                peers_checked: 0,
                total_xergon_confirmations: 0,
                unique_xergon_peers_seen: 0,
                work_points: 0,
            })),
            start_time: now,
        }
    }

    pub fn status(&self) -> Arc<RwLock<PownStatus>> {
        self.status.clone()
    }

    /// Calculate and update the PoNW score
    pub async fn tick(
        &self,
        node_health: &NodeHealthState,
        peers_checked: usize,
        xergon_peers_seen: usize,
        total_confirmations: usize,
    ) {
        let mut status = self.status.write().await;

        let now = chrono::Utc::now().timestamp();
        let uptime_secs = (now - self.start_time).max(0) as u64;
        let uptime_hours = uptime_secs as f64 / 3600.0;

        // Propagate node_id from health check
        if !node_health.node_id.is_empty() && status.node_id != node_health.node_id {
            status.node_id = node_health.node_id.clone();
        }

        // Node Work Score
        let mut node_score = 0.0;
        if node_health.is_synced {
            node_score += self.weights.sync_weight * 100.0;
        }
        // Uptime bonus (capped at 100 hours for full points)
        let uptime_bonus = (uptime_hours.min(100.0) / 100.0) * 100.0;
        node_score += self.weights.uptime_weight * uptime_bonus;
        // Peer count bonus
        let peer_bonus = (node_health.peer_count.min(10) as f64 / 10.0) * 100.0;
        node_score += self.weights.peer_count_weight * peer_bonus;

        // Network Work Score
        let network_score = (xergon_peers_seen.min(10) as f64 / 10.0) * 100.0;

        // AI Work Score (from current status)
        let ai_score = if status.ai_total_tokens > 0 {
            // Scales with tokens, capped at 100
            ((status.ai_total_tokens as f64 / 10000.0).min(1.0)) * 100.0
        } else {
            0.0
        };

        // Weighted total
        let total_score = (
            self.weights.node_weight * node_score +
            self.weights.network_weight * network_score +
            self.weights.ai_weight * ai_score
        ) as u64;

        status.work_points = total_score;
        status.peers_checked = peers_checked;
        status.unique_xergon_peers_seen = xergon_peers_seen;
        status.total_xergon_confirmations = total_confirmations;
        status.last_tick_ts = now;
        status.ergo_address = self.config.ergo_address.clone();

        info!(
            work_points = status.work_points,
            node_score = node_score as u64,
            network_score = network_score as u64,
            ai_score = ai_score as u64,
            uptime_hours = uptime_hours as u64,
            "PoNW tick complete"
        );
    }

    /// Update AI stats
    pub async fn update_ai_stats(&self, model: &str, tokens: u64, requests: u64) {
        let mut status = self.status.write().await;
        status.ai_enabled = true;
        status.ai_model = model.to_string();
        status.ai_total_tokens += tokens;
        status.ai_total_requests += requests;
        status.ai_points = status.ai_total_tokens / 100; // 1 point per 100 tokens
    }

    /// Set the node ID
    pub async fn set_node_id(&self, node_id: String) {
        self.status.write().await.node_id = node_id;
    }
}
