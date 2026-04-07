//! Cross-chain payment bridge for the Xergon Network relay.
//!
//! Implements Rosen-bridge-style guarded cross-chain transfers between Ergo
//! and external chains (Ethereum, Cardano, Bitcoin). Provides commit-reveal
//! watchers, fraud proof submission, and per-chain guard contracts.
//!
//! Architecture (mirrors Rosen):
//!   - Guard contract on each supported chain holds bridge funds
//!   - Watchers monitor lock events on source chain
//!   - Relayers submit commit-reveal transactions
//!   - Fraud proofs enable slashing of malicious relayers

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Supported chain identifiers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ChainId {
    Ergo,
    Ethereum,
    Cardano,
    Bitcoin,
    Bsc,
    Polygon,
}

impl ChainId {
    pub fn chain_name(&self) -> &'static str {
        match self {
            ChainId::Ergo => "ergo",
            ChainId::Ethereum => "ethereum",
            ChainId::Cardano => "cardano",
            ChainId::Bitcoin => "bitcoin",
            ChainId::Bsc => "bsc",
            ChainId::Polygon => "polygon",
        }
    }

    pub fn block_time_secs(&self) -> u64 {
        match self {
            ChainId::Ergo => 120,
            ChainId::Ethereum => 12,
            ChainId::Cardano => 20,
            ChainId::Bitcoin => 600,
            ChainId::Bsc => 3,
            ChainId::Polygon => 2,
        }
    }

    pub fn confirmation_blocks(&self) -> u32 {
        match self {
            ChainId::Ergo => 30,
            ChainId::Ethereum => 12,
            ChainId::Cardano => 15,
            ChainId::Bitcoin => 6,
            ChainId::Bsc => 15,
            ChainId::Polygon => 128,
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "ergo" => Some(ChainId::Ergo),
            "ethereum" | "eth" => Some(ChainId::Ethereum),
            "cardano" | "ada" => Some(ChainId::Cardano),
            "bitcoin" | "btc" => Some(ChainId::Bitcoin),
            "bsc" | "bnb" => Some(ChainId::Bsc),
            "polygon" | "matic" => Some(ChainId::Polygon),
            _ => None,
        }
    }
}

/// Bridge transfer status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransferStatus {
    Initiated,
    Locked,
    Committed,
    Revealed,
    Completed,
    FraudReported,
    Expired,
    Refunded,
}

/// Watcher event type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WatcherEvent {
    LockDetected,
    CommitDetected,
    RevealDetected,
    FraudDetected,
    TimeoutDetected,
}

/// Fraud proof type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FraudType {
    InvalidCommit,
    DoubleSpend,
    InvalidSignature,
    StaleData,
    InsufficientFee,
}

// ---------------------------------------------------------------------------
// Data Types
// ---------------------------------------------------------------------------

/// Bridge configuration per chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainConfig {
    pub chain: ChainId,
    pub node_url: String,
    pub guard_contract_address: String,
    pub watcher_start_block: u64,
    pub min_confirmations: u32,
    pub lock_timeout_blocks: u32,
    pub max_transfer_amount: u64,
    pub min_transfer_amount: u64,
    pub bridge_fee_bps: u32,
    pub enabled: bool,
}

/// Commit-reveal pair for cross-chain transfer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitRevealPair {
    pub commit_hash: String,
    pub reveal_data: String,
    pub source_chain: ChainId,
    pub target_chain: ChainId,
    pub source_tx_id: String,
    pub target_tx_id: Option<String>,
    pub created_height: u64,
    pub revealed_height: Option<u64>,
    pub amount: u64,
    pub token_id: Option<String>,
    pub sender: String,
    pub recipient: String,
    pub relayer: String,
    pub status: TransferStatus,
}

/// Fraud proof submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FraudProof {
    pub proof_id: String,
    pub transfer_id: String,
    pub fraud_type: FraudType,
    pub evidence: String,
    pub reporter: String,
    pub submitted_height: u64,
    pub processed: bool,
    pub slash_amount: u64,
}

/// Lock event detected by watcher.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockEvent {
    pub event_id: String,
    pub source_chain: ChainId,
    pub target_chain: ChainId,
    pub tx_id: String,
    pub block_height: u64,
    pub sender: String,
    pub recipient: String,
    pub amount: u64,
    pub token_id: Option<String>,
    pub lock_box_id: Option<String>,
    pub confirmations: u32,
    pub required_confirmations: u32,
    pub detected_at: u64,
}

/// Guard contract state on a chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardState {
    pub chain: ChainId,
    pub contract_address: String,
    pub total_locked: u64,
    pub total_bridged: u64,
    pub pending_transfers: u32,
    pub active_relayers: u32,
    pub last_event_height: u64,
}

/// Bridge transfer record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeTransfer {
    pub transfer_id: String,
    pub source_chain: ChainId,
    pub target_chain: ChainId,
    pub status: TransferStatus,
    pub amount: u64,
    pub token_id: Option<String>,
    pub sender: String,
    pub recipient: String,
    pub fee: u64,
    pub created_at: u64,
    pub lock_height: Option<u64>,
    pub commit_height: Option<u64>,
    pub reveal_height: Option<u64>,
    pub complete_height: Option<u64>,
    pub source_tx_id: Option<String>,
    pub target_tx_id: Option<String>,
}

/// Bridge health summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeHealth {
    pub active_chains: usize,
    pub pending_transfers: usize,
    pub total_transfers: u64,
    pub total_bridged_nanoerg: u64,
    pub fraud_reports: u32,
    pub active_watchers: usize,
    pub last_event_secs_ago: u64,
    pub lock_events_24h: u32,
    pub completion_rate_percent: f64,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Shared bridge state.
pub struct BridgeState {
    pub chain_configs: DashMap<String, ChainConfig>,
    pub guard_states: DashMap<String, GuardState>,
    pub transfers: DashMap<String, BridgeTransfer>,
    pub commit_reveals: DashMap<String, CommitRevealPair>,
    pub fraud_proofs: DashMap<String, FraudProof>,
    pub lock_events: DashMap<String, LockEvent>,
    pub watcher_events: DashMap<String, VecDeque<WatcherEvent>>,
    pub relayer_stakes: DashMap<String, u64>,
    pub metrics: DashMap<String, u64>,
    pub event_counter: AtomicU64,
}

impl BridgeState {
    pub fn new() -> Self {
        let state = Self {
            chain_configs: DashMap::new(),
            guard_states: DashMap::new(),
            transfers: DashMap::new(),
            commit_reveals: DashMap::new(),
            fraud_proofs: DashMap::new(),
            lock_events: DashMap::new(),
            watcher_events: DashMap::new(),
            relayer_stakes: DashMap::new(),
            metrics: DashMap::new(),
            event_counter: AtomicU64::new(0),
        };

        // Default Ergo chain config
        let ergo_config = ChainConfig {
            chain: ChainId::Ergo,
            node_url: "http://127.0.0.1:9053".to_string(),
            guard_contract_address: "".to_string(),
            watcher_start_block: 0,
            min_confirmations: 30,
            lock_timeout_blocks: 720,
            max_transfer_amount: 100_000_000_000u64, // 100 ERG
            min_transfer_amount: 1_000_000u64,        // 0.001 ERG
            bridge_fee_bps: 50,
            enabled: true,
        };
        state.chain_configs.insert("ergo".to_string(), ergo_config);

        state
    }
}

impl Default for BridgeState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Business Logic
// ---------------------------------------------------------------------------

impl BridgeState {
    /// Initiate a new bridge transfer.
    pub fn initiate_transfer(
        &self,
        source_chain: ChainId,
        target_chain: ChainId,
        sender: &str,
        recipient: &str,
        amount: u64,
        token_id: Option<String>,
    ) -> Result<BridgeTransfer, String> {
        if source_chain == target_chain {
            return Err("Source and target chain must differ".into());
        }

        let src_key = source_chain.chain_name();
        let config = self.chain_configs.get(src_key)
            .ok_or_else(|| format!("No config for chain: {}", src_key))?;

        if !config.enabled {
            return Err(format!("Chain {} bridge disabled", src_key));
        }
        if amount < config.min_transfer_amount {
            return Err(format!("Below minimum: {} < {}", amount, config.min_transfer_amount));
        }
        if amount > config.max_transfer_amount {
            return Err(format!("Above maximum: {} > {}", amount, config.max_transfer_amount));
        }

        let transfer_id = format!("xbr-{}", self.event_counter.fetch_add(1, Ordering::Relaxed));
        let fee = (amount as f64 * config.bridge_fee_bps as f64 / 10_000.0) as u64;

        let transfer = BridgeTransfer {
            transfer_id: transfer_id.clone(),
            source_chain,
            target_chain,
            status: TransferStatus::Initiated,
            amount,
            token_id,
            sender: sender.to_string(),
            recipient: recipient.to_string(),
            fee,
            created_at: now_secs(),
            lock_height: None,
            commit_height: None,
            reveal_height: None,
            complete_height: None,
            source_tx_id: None,
            target_tx_id: None,
        };

        self.transfers.insert(transfer_id.clone(), transfer.clone());
        self.metrics.entry("total_initiated".to_string()).and_modify(|v| *v += 1).or_insert(1);

        Ok(transfer)
    }

    /// Process a lock event from watcher.
    pub fn process_lock_event(&self, event: LockEvent) -> Result<String, String> {
        if event.confirmations < event.required_confirmations {
            return Err(format!(
                "Insufficient confirmations: {} < {}",
                event.confirmations,
                event.required_confirmations
            ));
        }

        let transfer_id = format!("xbr-{}", self.event_counter.fetch_add(1, Ordering::Relaxed));
        let fee = (event.amount as f64 * 0.005) as u64; // default 50bps

        let transfer = BridgeTransfer {
            transfer_id: transfer_id.clone(),
            source_chain: event.source_chain.clone(),
            target_chain: event.target_chain.clone(),
            status: TransferStatus::Locked,
            amount: event.amount,
            token_id: event.token_id.clone(),
            sender: event.sender.clone(),
            recipient: event.recipient.clone(),
            fee,
            created_at: event.detected_at,
            lock_height: Some(event.block_height),
            commit_height: None,
            reveal_height: None,
            complete_height: None,
            source_tx_id: Some(event.tx_id.clone()),
            target_tx_id: None,
        };

        self.transfers.insert(transfer_id.clone(), transfer);
        self.lock_events.insert(event.event_id.clone(), event.clone());
        self.record_watcher_event(&event.source_chain, WatcherEvent::LockDetected);

        Ok(transfer_id)
    }

    /// Submit a commit for a locked transfer.
    pub fn submit_commit(
        &self,
        transfer_id: &str,
        commit_hash: &str,
        relayer: &str,
    ) -> Result<(), String> {
        let mut transfer = self.transfers.get_mut(transfer_id)
            .ok_or("Transfer not found")?;

        if transfer.status != TransferStatus::Locked {
            return Err(format!("Invalid status for commit: {:?}", transfer.status));
        }

        transfer.status = TransferStatus::Committed;
        transfer.commit_height = Some(self.event_counter.load(Ordering::Relaxed));

        let cr = CommitRevealPair {
            commit_hash: commit_hash.to_string(),
            reveal_data: String::new(),
            source_chain: transfer.source_chain.clone(),
            target_chain: transfer.target_chain.clone(),
            source_tx_id: transfer.source_tx_id.clone().unwrap_or_default(),
            target_tx_id: None,
            created_height: now_secs(),
            revealed_height: None,
            amount: transfer.amount,
            token_id: transfer.token_id.clone(),
            sender: transfer.sender.clone(),
            recipient: transfer.recipient.clone(),
            relayer: relayer.to_string(),
            status: TransferStatus::Committed,
        };

        self.commit_reveals.insert(commit_hash.to_string(), cr);
        self.record_watcher_event(&transfer.source_chain, WatcherEvent::CommitDetected);
        Ok(())
    }

    /// Reveal commit data to complete transfer on target chain.
    pub fn reveal_commit(
        &self,
        commit_hash: &str,
        reveal_data: &str,
        target_tx_id: &str,
        block_height: u64,
    ) -> Result<(), String> {
        let mut cr = self.commit_reveals.get_mut(commit_hash)
            .ok_or("Commit not found")?;

        if cr.status != TransferStatus::Committed {
            return Err(format!("Commit status: {:?}", cr.status));
        }

        cr.reveal_data = reveal_data.to_string();
        cr.target_tx_id = Some(target_tx_id.to_string());
        cr.revealed_height = Some(block_height);
        cr.status = TransferStatus::Revealed;

        // Update corresponding transfer
        for mut t in self.transfers.iter_mut() {
            if t.source_tx_id.as_deref() == Some(&cr.source_tx_id)
                && t.status == TransferStatus::Committed
            {
                t.status = TransferStatus::Completed;
                t.reveal_height = Some(block_height);
                t.target_tx_id = Some(target_tx_id.to_string());
                t.complete_height = Some(block_height);
            }
        }

        self.record_watcher_event(&cr.target_chain, WatcherEvent::RevealDetected);
        self.metrics.entry("total_completed".to_string()).and_modify(|v| *v += 1).or_insert(1);
        Ok(())
    }

    /// Submit a fraud proof.
    pub fn submit_fraud_proof(&self, mut proof: FraudProof) -> Result<(), String> {
        if !self.commit_reveals.contains_key(&proof.transfer_id) {
            return Err("Unknown transfer for fraud proof".into());
        }

        proof.processed = false;
        proof.submitted_height = now_secs();
        let proof_id = proof.proof_id.clone();

        self.fraud_proofs.insert(proof_id.clone(), proof);
        self.metrics.entry("fraud_reports".to_string()).and_modify(|v| *v += 1).or_insert(1);

        Ok(())
    }

    /// Process a detected fraud (slash relayer, flag transfer).
    pub fn process_fraud(&self, proof_id: &str) -> Result<(), String> {
        let proof = self.fraud_proofs.get(proof_id)
            .ok_or("Fraud proof not found")?;

        if proof.processed {
            return Err("Already processed".into());
        }

        // Slash relayer stake
        self.relayer_stakes.entry(proof.reporter.clone())
            .and_modify(|v| *v = v.saturating_add(proof.slash_amount));

        // Flag the transfer
        if let Some(mut transfer) = self.transfers.get_mut(&proof.transfer_id) {
            transfer.status = TransferStatus::FraudReported;
        }

        Ok(())
    }

    /// Register a chain configuration.
    pub fn register_chain(&self, config: ChainConfig) {
        self.chain_configs.insert(config.chain.chain_name().to_string(), config);
    }

    /// Update guard state for a chain.
    pub fn update_guard_state(&self, state: GuardState) {
        self.guard_states.insert(state.chain.chain_name().to_string(), state);
    }

    /// Register relayer stake.
    pub fn register_relayer(&self, relayer: &str, stake: u64) {
        self.relayer_stakes.insert(relayer.to_string(), stake);
    }

    /// Check for expired transfers and mark them.
    pub fn check_timeouts(&self, current_height: u64) -> Vec<String> {
        let mut expired = Vec::new();
        let chain_key = |c: &ChainId| c.chain_name().to_string();

        for mut t in self.transfers.iter_mut() {
            if t.status != TransferStatus::Locked && t.status != TransferStatus::Committed {
                continue;
            }
            let config_key = chain_key(&t.source_chain);
            let timeout = self.chain_configs.get(&config_key)
                .map(|c| c.lock_timeout_blocks as u64)
                .unwrap_or(720);

            let start = t.lock_height.unwrap_or(t.created_at);
            if current_height.saturating_sub(start) > timeout {
                t.status = TransferStatus::Expired;
                expired.push(t.transfer_id.clone());
            }
        }

        expired
    }

    /// Get bridge health summary.
    pub fn get_health(&self) -> BridgeHealth {
        let active_chains = self.chain_configs.iter().filter(|c| c.value().enabled).count();
        let pending: usize = self.transfers.iter()
            .filter(|t| matches!(t.status, TransferStatus::Initiated | TransferStatus::Locked | TransferStatus::Committed))
            .count();
        let total = self.metrics.get("total_initiated").map(|m| *m).unwrap_or(0);
        let completed = self.metrics.get("total_completed").map(|m| *m).unwrap_or(0);
        let fraud = self.metrics.get("fraud_reports").map(|m| *m as u32).unwrap_or(0);
        let completion = if total > 0 { (completed as f64 / total as f64) * 100.0 } else { 0.0 };

        let bridged: u64 = self.transfers.iter()
            .filter(|t| t.status == TransferStatus::Completed)
            .map(|t| t.amount)
            .sum();

        let last_event = self.metrics.get("last_event_time").map(|m| *m).unwrap_or(0);
        let staleness = now_secs().saturating_sub(last_event);
        let watchers = self.chain_configs.iter().filter(|c| c.value().enabled).count();

        let lock_24h = self.lock_events.iter()
            .filter(|e| now_secs().saturating_sub(e.value().detected_at) < 86400)
            .count() as u32;

        BridgeHealth {
            active_chains,
            pending_transfers: pending,
            total_transfers: total,
            total_bridged_nanoerg: bridged,
            fraud_reports: fraud,
            active_watchers: watchers,
            last_event_secs_ago: staleness,
            lock_events_24h: lock_24h,
            completion_rate_percent: completion,
        }
    }

    fn record_watcher_event(&self, chain: &ChainId, event: WatcherEvent) {
        let key = chain.chain_name().to_string();
        self.watcher_events.entry(key).or_insert_with(VecDeque::new).push_back(event);
        self.metrics.entry("last_event_time".to_string()).and_modify(|v| *v = now_secs()).or_insert(now_secs());
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()
}
