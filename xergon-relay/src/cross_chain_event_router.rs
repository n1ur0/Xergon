#![allow(dead_code)]
//! Cross-chain event router for the Xergon Network relay.
//!
//! Extends the cross-chain bridge (Phase 74) with:
//!   - Event subscriptions with per-chain, per-event-type filtering
//!   - State sync manager with reorg detection and ring-buffer snapshots
//!   - Event router with per-chain queues, ordering, and replay
//!   - Bridge analytics: latency histograms, throughput, success/failure rates
//!   - REST handler functions for subscriptions, sync, analytics, and replay

use crate::cross_chain_bridge::ChainId;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Event types that can be subscribed to and routed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    Lock,
    Commit,
    Reveal,
    Fraud,
    Transfer,
    StateChange,
}

impl EventType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "lock" => Some(EventType::Lock),
            "commit" => Some(EventType::Commit),
            "reveal" => Some(EventType::Reveal),
            "fraud" => Some(EventType::Fraud),
            "transfer" => Some(EventType::Transfer),
            "state_change" => Some(EventType::StateChange),
            _ => None,
        }
    }
}

/// Processing result for a routed event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventOutcome {
    Success,
    Failed { reason: String },
    Rejected { reason: String },
    Pending,
}

// ---------------------------------------------------------------------------
// Data Types
// ---------------------------------------------------------------------------

/// Subscription to cross-chain events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSubscription {
    pub subscription_id: String,
    pub chain_id: ChainId,
    pub event_type: Option<EventType>,
    pub callback_url: String,
    pub active: bool,
    pub created_at: u64,
    pub events_delivered: u64,
    pub last_delivery_at: Option<u64>,
}

/// Block snapshot stored in the ring buffer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockSnapshot {
    pub chain: ChainId,
    pub height: u64,
    pub block_hash: String,
    pub parent_hash: String,
    pub timestamp: u64,
    pub tx_count: u32,
}

/// Per-chain sync state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainSyncState {
    pub chain: ChainId,
    pub synced_height: u64,
    pub tip_height: u64,
    pub syncing: bool,
    pub last_reorg_height: Option<u64>,
    pub reorg_count: u32,
    pub last_sync_at: u64,
    pub snapshots: VecDeque<BlockSnapshot>,
}

/// A cross-chain event waiting to be routed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossChainEvent {
    pub event_id: String,
    pub source_chain: ChainId,
    pub target_chain: Option<ChainId>,
    pub event_type: EventType,
    pub block_height: u64,
    pub tx_id: String,
    pub payload: String,
    pub timestamp: u64,
    pub outcome: Option<EventOutcome>,
    pub retry_count: u32,
    pub max_retries: u32,
}

/// Latency histogram bucket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyBucket {
    pub bucket_ms: u64,
    pub count: u64,
}

/// Per-chain analytics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainAnalytics {
    pub chain: ChainId,
    pub events_processed: u64,
    pub events_success: u64,
    pub events_failed: u64,
    pub events_rejected: u64,
    pub events_pending: u64,
    pub total_latency_ms: u64,
    pub latency_buckets: Vec<LatencyBucket>,
    pub throughput_per_minute: f64,
    pub last_event_at: u64,
}

/// Time-bucketed analytics entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeBucket {
    pub bucket_key: String,
    pub events_processed: u64,
    pub events_success: u64,
    pub events_failed: u64,
    pub avg_latency_ms: u64,
}

/// Reorg detection result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReorgInfo {
    pub chain: ChainId,
    pub previous_height: u64,
    pub new_height: u64,
    pub depth: u64,
    pub detected_at: u64,
}

// ---------------------------------------------------------------------------
// Subscription Manager
// ---------------------------------------------------------------------------

pub struct EventSubscriptionManager {
    pub subscriptions: DashMap<String, EventSubscription>,
    pub chain_subs: DashMap<String, Vec<String>>,
    pub id_counter: AtomicU64,
}

impl EventSubscriptionManager {
    pub fn new() -> Self {
        Self {
            subscriptions: DashMap::new(),
            chain_subs: DashMap::new(),
            id_counter: AtomicU64::new(0),
        }
    }

    /// Create a new event subscription.
    pub fn subscribe(
        &self,
        chain_id: ChainId,
        event_type: Option<EventType>,
        callback_url: &str,
    ) -> EventSubscription {
        let sub_id = format!("sub-{}", self.id_counter.fetch_add(1, Ordering::Relaxed));
        let sub = EventSubscription {
            subscription_id: sub_id.clone(),
            chain_id: chain_id.clone(),
            event_type: event_type.clone(),
            callback_url: callback_url.to_string(),
            active: true,
            created_at: now_secs(),
            events_delivered: 0,
            last_delivery_at: None,
        };

        let chain_key = chain_id.chain_name().to_string();
        self.chain_subs
            .entry(chain_key)
            .or_insert_with(Vec::new)
            .push(sub_id.clone());
        self.subscriptions.insert(sub_id.clone(), sub);
        self.subscriptions.get(&sub_id).unwrap().clone()
    }

    /// List all subscriptions, optionally filtered by chain.
    pub fn list_subscriptions(&self, chain_id: Option<&ChainId>) -> Vec<EventSubscription> {
        let mut result = Vec::new();
        for item in self.subscriptions.iter() {
            if chain_id.is_none() || &item.value().chain_id == chain_id.unwrap() {
                result.push(item.value().clone());
            }
        }
        result
    }

    /// Unsubscribe by id. Returns true if found and removed.
    pub fn unsubscribe(&self, subscription_id: &str) -> bool {
        let removed = self.subscriptions.remove(subscription_id).is_some();
        if removed {
            let mut keys_to_clean: Vec<String> = Vec::new();
            let mut subs_to_update: Vec<(String, Vec<String>)> = Vec::new();
            for entry in self.chain_subs.iter() {
                let key = entry.key().clone();
                let val = entry.value();
                if val.iter().any(|k| k == subscription_id) {
                    let filtered: Vec<String> = val.iter().filter(|k| k != &subscription_id).cloned().collect();
                    if filtered.is_empty() {
                        keys_to_clean.push(key.clone());
                    }
                    subs_to_update.push((key, filtered));
                }
            }
            for (key, filtered) in subs_to_update {
                if filtered.is_empty() {
                    self.chain_subs.remove(&key);
                } else {
                    self.chain_subs.insert(key, filtered);
                }
            }
        }
        removed
    }

    /// Get subscriptions matching a chain and optional event type.
    pub fn matching_subscriptions(
        &self,
        chain_id: &ChainId,
        event_type: &EventType,
    ) -> Vec<EventSubscription> {
        let chain_key = chain_id.chain_name().to_string();
        let sub_ids = self
            .chain_subs
            .get(&chain_key)
            .map(|v| v.clone())
            .unwrap_or_default();

        let mut result = Vec::new();
        for sub_id in sub_ids {
            if let Some(sub) = self.subscriptions.get(&sub_id) {
                if sub.active {
                    match &sub.event_type {
                        None => result.push(sub.clone()),
                        Some(filter) if filter == event_type => result.push(sub.clone()),
                        _ => {}
                    }
                }
            }
        }
        result
    }

    /// Record a delivery to a subscription.
    pub fn record_delivery(&self, subscription_id: &str) {
        if let Some(mut sub) = self.subscriptions.get_mut(subscription_id) {
            sub.events_delivered += 1;
            sub.last_delivery_at = Some(now_secs());
        }
    }
}

impl Default for EventSubscriptionManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// State Sync Manager
// ---------------------------------------------------------------------------

const RING_BUFFER_CAPACITY: usize = 100;

pub struct StateSyncManager {
    pub chain_states: DashMap<String, ChainSyncState>,
    pub reorg_log: DashMap<String, Vec<ReorgInfo>>,
}

impl StateSyncManager {
    pub fn new() -> Self {
        Self {
            chain_states: DashMap::new(),
            reorg_log: DashMap::new(),
        }
    }

    /// Initialize sync state for a chain.
    pub fn init_chain(&self, chain: &ChainId, start_height: u64) {
        let key = chain.chain_name().to_string();
        self.chain_states
            .entry(key)
            .or_insert_with(|| ChainSyncState {
                chain: chain.clone(),
                synced_height: start_height,
                tip_height: start_height,
                syncing: false,
                last_reorg_height: None,
                reorg_count: 0,
                last_sync_at: now_secs(),
                snapshots: VecDeque::with_capacity(RING_BUFFER_CAPACITY),
            });
    }

    /// Update synced height. Detects reorgs if height rolls back.
    pub fn update_height(
        &self,
        chain: &ChainId,
        new_height: u64,
        block_hash: &str,
        parent_hash: &str,
    ) -> Result<Option<ReorgInfo>, String> {
        let key = chain.chain_name().to_string();
        let mut state = self
            .chain_states
            .get_mut(&key)
            .ok_or_else(|| format!("No sync state for chain: {}", key))?;

        let reorg = if new_height < state.synced_height {
            let depth = state.synced_height.saturating_sub(new_height);
            let info = ReorgInfo {
                chain: chain.clone(),
                previous_height: state.synced_height,
                new_height,
                depth,
                detected_at: now_secs(),
            };
            state.last_reorg_height = Some(state.synced_height);
            state.reorg_count += 1;

            // Log the reorg
            let log_key = chain.chain_name().to_string();
            self.reorg_log
                .entry(log_key)
                .or_insert_with(Vec::new)
                .push(info.clone());
            // Keep last 50 reorg entries per chain
            let chain_name = chain.chain_name().to_string();
            if let Some(mut log) = self.reorg_log.get_mut(&chain_name) {
                if log.len() > 50 {
                    let drain_count = log.len() - 50;
                    log.drain(..drain_count);
                }
            }

            Some(info)
        } else {
            None
        };

        state.synced_height = new_height;
        if new_height > state.tip_height {
            state.tip_height = new_height;
        }
        state.last_sync_at = now_secs();

        // Push snapshot into ring buffer
        let snapshot = BlockSnapshot {
            chain: chain.clone(),
            height: new_height,
            block_hash: block_hash.to_string(),
            parent_hash: parent_hash.to_string(),
            timestamp: now_secs(),
            tx_count: 0,
        };
        if state.snapshots.len() >= RING_BUFFER_CAPACITY {
            state.snapshots.pop_front();
        }
        state.snapshots.push_back(snapshot);

        Ok(reorg)
    }

    /// Get sync status for a chain.
    pub fn get_sync_status(&self, chain: &ChainId) -> Option<ChainSyncState> {
        let key = chain.chain_name().to_string();
        self.chain_states.get(&key).map(|v| v.clone())
    }

    /// Get all sync statuses.
    pub fn get_all_sync_statuses(&self) -> Vec<ChainSyncState> {
        self.chain_states.iter().map(|c| c.value().clone()).collect()
    }

    /// Mark a chain as syncing or idle.
    pub fn set_syncing(&self, chain: &ChainId, syncing: bool) {
        let key = chain.chain_name().to_string();
        if let Some(mut state) = self.chain_states.get_mut(&key) {
            state.syncing = syncing;
        }
    }

    /// Force resync: reset synced height to target.
    pub fn force_resync(&self, chain: &ChainId, target_height: u64) -> Result<(), String> {
        let key = chain.chain_name().to_string();
        let mut state = self
            .chain_states
            .get_mut(&key)
            .ok_or_else(|| format!("No sync state for chain: {}", key))?;
        state.synced_height = target_height;
        state.syncing = true;
        state.snapshots.clear();
        state.last_sync_at = now_secs();
        Ok(())
    }

    /// Get reorg history for a chain.
    pub fn get_reorg_history(&self, chain: &ChainId) -> Vec<ReorgInfo> {
        let key = chain.chain_name().to_string();
        self.reorg_log
            .get(&key)
            .map(|v| v.clone())
            .unwrap_or_default()
    }
}

impl Default for StateSyncManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Event Router
// ---------------------------------------------------------------------------

const EVENT_QUEUE_MAX: usize = 10_000;

pub struct EventRouter {
    pub event_queues: DashMap<String, VecDeque<CrossChainEvent>>,
    pub processed_events: DashMap<String, CrossChainEvent>,
    pub pending_events: DashMap<String, CrossChainEvent>,
    pub id_counter: AtomicU64,
    pub total_routed: AtomicU64,
    pub total_replayed: AtomicU64,
}

impl EventRouter {
    pub fn new() -> Self {
        Self {
            event_queues: DashMap::new(),
            processed_events: DashMap::new(),
            pending_events: DashMap::new(),
            id_counter: AtomicU64::new(0),
            total_routed: AtomicU64::new(0),
            total_replayed: AtomicU64::new(0),
        }
    }

    /// Enqueue an incoming cross-chain event.
    pub fn enqueue(&self, mut event: CrossChainEvent) -> Result<String, String> {
        if event.event_id.is_empty() {
            event.event_id = format!(
                "evt-{}",
                self.id_counter.fetch_add(1, Ordering::Relaxed)
            );
        }
        let event_id = event.event_id.clone();
        let chain_key = event.source_chain.chain_name().to_string();

        self.event_queues
            .entry(chain_key)
            .or_insert_with(VecDeque::new);

        let mut queue = self
            .event_queues
            .get_mut(&event.source_chain.chain_name().to_string())
            .unwrap();
        if queue.len() >= EVENT_QUEUE_MAX {
            queue.pop_front();
        }
        queue.push_back(event.clone());

        self.pending_events
            .insert(event_id.clone(), event.clone());
        self.total_routed.fetch_add(1, Ordering::Relaxed);
        Ok(event_id)
    }

    /// Dequeue the next event for a given chain (FIFO).
    pub fn dequeue(&self, chain: &ChainId) -> Option<CrossChainEvent> {
        let chain_key = chain.chain_name().to_string();
        let mut queue = self.event_queues.get_mut(&chain_key)?;
        let event = queue.pop_front()?;
        Some(event)
    }

    /// Mark an event as processed with an outcome.
    pub fn mark_processed(&self, event_id: &str, outcome: EventOutcome) {
        if let Some((_, mut event)) = self.pending_events.remove(event_id) {
            event.outcome = Some(outcome);
            event.timestamp = now_secs();
            let event_clone = event.clone();
            self.processed_events
                .insert(event_id.to_string(), event_clone);
        }
    }

    /// Retry a failed event.
    pub fn retry_event(&self, event_id: &str) -> Result<(), String> {
        let mut event = self
            .pending_events
            .get_mut(event_id)
            .ok_or("Event not found in pending")?;
        if event.retry_count >= event.max_retries {
            return Err("Max retries exceeded".into());
        }
        event.retry_count += 1;
        Ok(())
    }

    /// Replay events for a chain within a block range.
    pub fn replay_events(
        &self,
        chain: &ChainId,
        from_height: u64,
        to_height: u64,
    ) -> Vec<CrossChainEvent> {
        let mut replayed = Vec::new();
        for item in self.processed_events.iter() {
            let evt = item.value();
            if evt.source_chain == *chain
                && evt.block_height >= from_height
                && evt.block_height <= to_height
            {
                replayed.push(evt.clone());
            }
        }
        replayed.sort_by_key(|e| e.block_height);
        self.total_replayed
            .fetch_add(replayed.len() as u64, Ordering::Relaxed);
        replayed
    }

    /// Get queue depth for a chain.
    pub fn queue_depth(&self, chain: &ChainId) -> usize {
        let key = chain.chain_name().to_string();
        self.event_queues.get(&key).map(|q| q.len()).unwrap_or(0)
    }
}

impl Default for EventRouter {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Bridge Analytics
// ---------------------------------------------------------------------------

pub struct BridgeAnalytics {
    pub chain_stats: DashMap<String, ChainAnalytics>,
    pub time_buckets: DashMap<String, Vec<TimeBucket>>,
    pub global_processed: AtomicU64,
    pub global_success: AtomicU64,
    pub global_failed: AtomicU64,
    pub global_rejected: AtomicU64,
    pub global_latency_ms: AtomicU64,
}

impl BridgeAnalytics {
    pub fn new() -> Self {
        Self {
            chain_stats: DashMap::new(),
            time_buckets: DashMap::new(),
            global_processed: AtomicU64::new(0),
            global_success: AtomicU64::new(0),
            global_failed: AtomicU64::new(0),
            global_rejected: AtomicU64::new(0),
            global_latency_ms: AtomicU64::new(0),
        }
    }

    /// Record a processed event.
    pub fn record_event(
        &self,
        chain: &ChainId,
        event_type: &EventType,
        outcome: &EventOutcome,
        latency_ms: u64,
    ) {
        self.global_processed.fetch_add(1, Ordering::Relaxed);
        self.global_latency_ms.fetch_add(latency_ms, Ordering::Relaxed);

        let chain_key = chain.chain_name().to_string();

        self.chain_stats
            .entry(chain_key.clone())
            .or_insert_with(|| ChainAnalytics {
                chain: chain.clone(),
                events_processed: 0,
                events_success: 0,
                events_failed: 0,
                events_rejected: 0,
                events_pending: 0,
                total_latency_ms: 0,
                latency_buckets: Self::default_latency_buckets(),
                throughput_per_minute: 0.0,
                last_event_at: now_secs(),
            });

        if let Some(mut stats) = self.chain_stats.get_mut(&chain_key) {
            stats.events_processed += 1;
            stats.total_latency_ms += latency_ms;
            stats.last_event_at = now_secs();
            match outcome {
                EventOutcome::Success => {
                    stats.events_success += 1;
                    self.global_success.fetch_add(1, Ordering::Relaxed);
                }
                EventOutcome::Failed { .. } => {
                    stats.events_failed += 1;
                    self.global_failed.fetch_add(1, Ordering::Relaxed);
                }
                EventOutcome::Rejected { .. } => {
                    stats.events_rejected += 1;
                    self.global_rejected.fetch_add(1, Ordering::Relaxed);
                }
                EventOutcome::Pending => {
                    stats.events_pending += 1;
                }
            }

            // Update latency histogram bucket
            let bucket_ms = Self::bucket_for_latency(latency_ms);
            for bucket in stats.latency_buckets.iter_mut() {
                if bucket.bucket_ms == bucket_ms {
                    bucket.count += 1;
                    break;
                }
            }
        }

        // Update time bucket (hourly)
        let hour_key = Self::hour_bucket_key();
        let type_key = format!("{}_{}", hour_key, format!("{:?}", event_type).to_lowercase());
        self.time_buckets
            .entry(type_key.clone())
            .or_insert_with(Vec::new);
        if let Some(mut buckets) = self.time_buckets.get_mut(&type_key) {
            if let Some(last) = buckets.last_mut() {
                if last.bucket_key == hour_key {
                    last.events_processed += 1;
                    match outcome {
                        EventOutcome::Success => last.events_success += 1,
                        EventOutcome::Failed { .. } => last.events_failed += 1,
                        _ => {}
                    }
                    last.avg_latency_ms = (last.avg_latency_ms + latency_ms) / 2;
                    return;
                }
            }
            buckets.push(TimeBucket {
                bucket_key: hour_key.clone(),
                events_processed: 1,
                events_success: if outcome == &EventOutcome::Success { 1 } else { 0 },
                events_failed: if matches!(outcome, EventOutcome::Failed { .. }) { 1 } else { 0 },
                avg_latency_ms: latency_ms,
            });
            // Keep last 168 hourly buckets (7 days)
            if buckets.len() > 168 {
                let drain_count = buckets.len() - 168;
                buckets.drain(..drain_count);
            }
        }
    }

    /// Get analytics for a specific chain.
    pub fn get_chain_analytics(&self, chain: &ChainId) -> Option<ChainAnalytics> {
        let key = chain.chain_name().to_string();
        self.chain_stats.get(&key).map(|v| v.clone())
    }

    /// Get all chain analytics.
    pub fn get_all_analytics(&self) -> Vec<ChainAnalytics> {
        let mut result: Vec<ChainAnalytics> = self
            .chain_stats
            .iter()
            .map(|c| c.value().clone())
            .collect();
        result.sort_by_key(|a| a.chain.chain_name().to_string());
        result
    }

    /// Get time-bucketed analytics for an event type.
    pub fn get_time_buckets(&self, event_type: &EventType, limit: usize) -> Vec<TimeBucket> {
        let hour_key = Self::hour_bucket_key();
        let type_key = format!("{}_{}", hour_key, format!("{:?}", event_type).to_lowercase());
        self.time_buckets
            .get(&type_key)
            .map(|v| {
                let mut buckets: Vec<TimeBucket> = v.clone();
                let len = buckets.len().min(limit);
                buckets.drain(buckets.len() - len..).collect()
            })
            .unwrap_or_default()
    }

    /// Get global summary stats.
    pub fn get_global_summary(&self) -> AnalyticsSummary {
        let processed = self.global_processed.load(Ordering::Relaxed);
        let success = self.global_success.load(Ordering::Relaxed);
        let failed = self.global_failed.load(Ordering::Relaxed);
        let rejected = self.global_rejected.load(Ordering::Relaxed);
        let total_latency = self.global_latency_ms.load(Ordering::Relaxed);
        let avg_latency = if processed > 0 {
            total_latency / processed
        } else {
            0
        };
        let success_rate = if processed > 0 {
            (success as f64 / processed as f64) * 100.0
        } else {
            0.0
        };

        AnalyticsSummary {
            total_events_processed: processed,
            total_success: success,
            total_failed: failed,
            total_rejected: rejected,
            avg_latency_ms: avg_latency,
            success_rate_percent: success_rate,
            active_chains: self.chain_stats.len(),
        }
    }

    /// Calculate throughput per minute for a chain.
    pub fn calculate_throughput(&self, chain: &ChainId) -> f64 {
        let key = chain.chain_name().to_string();
        if let Some(mut stats) = self.chain_stats.get_mut(&key) {
            let elapsed = now_secs().saturating_sub(stats.last_event_at);
            if elapsed > 0 && stats.events_processed > 0 {
                stats.throughput_per_minute =
                    (stats.events_processed as f64 / elapsed as f64) * 60.0;
            }
            stats.throughput_per_minute
        } else {
            0.0
        }
    }

    fn default_latency_buckets() -> Vec<LatencyBucket> {
        vec![
            LatencyBucket { bucket_ms: 10, count: 0 },
            LatencyBucket { bucket_ms: 50, count: 0 },
            LatencyBucket { bucket_ms: 100, count: 0 },
            LatencyBucket { bucket_ms: 250, count: 0 },
            LatencyBucket { bucket_ms: 500, count: 0 },
            LatencyBucket { bucket_ms: 1000, count: 0 },
            LatencyBucket { bucket_ms: 2500, count: 0 },
            LatencyBucket { bucket_ms: 5000, count: 0 },
            LatencyBucket { bucket_ms: 10_000, count: 0 },
        ]
    }

    fn bucket_for_latency(latency_ms: u64) -> u64 {
        let buckets = [10, 50, 100, 250, 500, 1000, 2500, 5000, 10_000];
        for &b in &buckets {
            if latency_ms <= b {
                return b;
            }
        }
        10_000
    }

    fn hour_bucket_key() -> String {
        let secs = now_secs();
        format!("h_{}", secs / 3600)
    }
}

impl Default for BridgeAnalytics {
    fn default() -> Self {
        Self::new()
    }
}

/// Global analytics summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsSummary {
    pub total_events_processed: u64,
    pub total_success: u64,
    pub total_failed: u64,
    pub total_rejected: u64,
    pub avg_latency_ms: u64,
    pub success_rate_percent: f64,
    pub active_chains: usize,
}

// ---------------------------------------------------------------------------
// REST Handler Functions
// ---------------------------------------------------------------------------

/// Handler: subscribe to events on a chain.
pub fn handle_subscribe(
    sub_mgr: &EventSubscriptionManager,
    chain_id: ChainId,
    event_type: Option<EventType>,
    callback_url: &str,
) -> EventSubscription {
    sub_mgr.subscribe(chain_id, event_type, callback_url)
}

/// Handler: list active subscriptions, optionally filtered by chain.
pub fn handle_list_subscriptions(
    sub_mgr: &EventSubscriptionManager,
    chain_id: Option<ChainId>,
) -> Vec<EventSubscription> {
    sub_mgr.list_subscriptions(chain_id.as_ref())
}

/// Handler: unsubscribe by id.
pub fn handle_unsubscribe(sub_mgr: &EventSubscriptionManager, subscription_id: &str) -> bool {
    sub_mgr.unsubscribe(subscription_id)
}

/// Handler: get sync status for all chains.
pub fn handle_get_sync_status(sync_mgr: &StateSyncManager) -> Vec<ChainSyncState> {
    sync_mgr.get_all_sync_statuses()
}

/// Handler: get sync status for a specific chain.
pub fn handle_get_chain_sync_status(
    sync_mgr: &StateSyncManager,
    chain: &ChainId,
) -> Option<ChainSyncState> {
    sync_mgr.get_sync_status(chain)
}

/// Handler: force resync a chain to a target height.
pub fn handle_force_resync(
    sync_mgr: &StateSyncManager,
    chain: &ChainId,
    target_height: u64,
) -> Result<(), String> {
    sync_mgr.force_resync(chain, target_height)
}

/// Handler: get bridge analytics summary.
pub fn handle_get_analytics_summary(analytics: &BridgeAnalytics) -> AnalyticsSummary {
    analytics.get_global_summary()
}

/// Handler: get per-chain analytics.
pub fn handle_get_chain_analytics(
    analytics: &BridgeAnalytics,
    chain: &ChainId,
) -> Option<ChainAnalytics> {
    analytics.get_chain_analytics(chain)
}

/// Handler: get time-bucketed analytics for an event type.
pub fn handle_get_time_bucket_analytics(
    analytics: &BridgeAnalytics,
    event_type: &EventType,
    limit: usize,
) -> Vec<TimeBucket> {
    analytics.get_time_buckets(event_type, limit)
}

/// Handler: replay events for a chain within a block range.
pub fn handle_replay_events(
    router: &EventRouter,
    chain: &ChainId,
    from_height: u64,
    to_height: u64,
) -> Vec<CrossChainEvent> {
    router.replay_events(chain, from_height, to_height)
}

/// Handler: route an incoming event (enqueue + notify matching subscriptions).
pub fn handle_route_event(
    router: &EventRouter,
    sub_mgr: &EventSubscriptionManager,
    analytics: &BridgeAnalytics,
    mut event: CrossChainEvent,
) -> Result<RouteResult, String> {
    let start = now_millis();
    let event_id = router.enqueue(event.clone())?;
    let event_type = event.event_type.clone();
    let source_chain = event.source_chain.clone();

    // Find matching subscriptions
    let subs = sub_mgr.matching_subscriptions(&source_chain, &event_type);

    // Deliver to each subscription
    for sub in &subs {
        // In production this would POST to sub.callback_url.
        // Here we just record the delivery.
        sub_mgr.record_delivery(&sub.subscription_id);
    }

    let latency = now_millis().saturating_sub(start);
    let outcome = EventOutcome::Success;

    analytics.record_event(&source_chain, &event_type, &outcome, latency);
    router.mark_processed(&event_id, outcome.clone());
    event.outcome = Some(outcome);

    Ok(RouteResult {
        event_id,
        event,
        subscriptions_notified: subs.len(),
        latency_ms: latency,
    })
}

/// Handler: get reorg history for a chain.
pub fn handle_get_reorg_history(sync_mgr: &StateSyncManager, chain: &ChainId) -> Vec<ReorgInfo> {
    sync_mgr.get_reorg_history(chain)
}

/// Handler: get queue depth for a chain.
pub fn handle_get_queue_depth(router: &EventRouter, chain: &ChainId) -> usize {
    router.queue_depth(chain)
}

/// Result of routing a single event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteResult {
    pub event_id: String,
    pub event: CrossChainEvent,
    pub subscriptions_notified: usize,
    pub latency_ms: u64,
}

// ---------------------------------------------------------------------------
// Unified Router State
// ---------------------------------------------------------------------------

/// Top-level state combining all event router components.
pub struct CrossChainEventRouter {
    pub subscriptions: EventSubscriptionManager,
    pub state_sync: StateSyncManager,
    pub event_router: EventRouter,
    pub analytics: BridgeAnalytics,
}

impl CrossChainEventRouter {
    pub fn new() -> Self {
        let router = Self {
            subscriptions: EventSubscriptionManager::new(),
            state_sync: StateSyncManager::new(),
            event_router: EventRouter::new(),
            analytics: BridgeAnalytics::new(),
        };
        // Initialize sync state for all known chains
        for chain in &[
            ChainId::Ergo,
            ChainId::Ethereum,
            ChainId::Cardano,
            ChainId::Bitcoin,
            ChainId::Bsc,
            ChainId::Polygon,
        ] {
            router.state_sync.init_chain(chain, 0);
        }
        router
    }
}

impl Default for CrossChainEventRouter {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscribe_and_list() {
        let mgr = EventSubscriptionManager::new();
        let sub = mgr.subscribe(
            ChainId::Ergo,
            Some(EventType::Lock),
            "https://example.com/webhook",
        );
        assert!(sub.active);
        assert_eq!(sub.chain_id, ChainId::Ergo);

        let all = mgr.list_subscriptions(None);
        assert_eq!(all.len(), 1);

        let filtered = mgr.list_subscriptions(Some(&ChainId::Ethereum));
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_unsubscribe() {
        let mgr = EventSubscriptionManager::new();
        let sub = mgr.subscribe(ChainId::Ergo, None, "https://example.com");
        assert!(mgr.unsubscribe(&sub.subscription_id));
        assert!(!mgr.unsubscribe(&sub.subscription_id)); // already removed
        assert_eq!(mgr.list_subscriptions(None).len(), 0);
    }

    #[test]
    fn test_matching_subscriptions() {
        let mgr = EventSubscriptionManager::new();
        mgr.subscribe(ChainId::Ergo, Some(EventType::Lock), "https://a.com");
        mgr.subscribe(ChainId::Ergo, None, "https://b.com"); // matches all
        mgr.subscribe(ChainId::Ergo, Some(EventType::Fraud), "https://c.com");

        let matches = mgr.matching_subscriptions(&ChainId::Ergo, &EventType::Lock);
        assert_eq!(matches.len(), 2); // Lock-specific + wildcard

        let fraud_matches = mgr.matching_subscriptions(&ChainId::Ergo, &EventType::Fraud);
        assert_eq!(fraud_matches.len(), 2); // Fraud-specific + wildcard
    }

    #[test]
    fn test_state_sync_update_and_reorg() {
        let sync = StateSyncManager::new();
        sync.init_chain(&ChainId::Ethereum, 100);

        // Normal advance
        let result = sync.update_height(&ChainId::Ethereum, 101, "0xabc", "0xdef");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());

        // Reorg detected
        let result = sync.update_height(&ChainId::Ethereum, 98, "0xnew", "0xprev");
        assert!(result.is_ok());
        let reorg = result.unwrap().unwrap();
        assert_eq!(reorg.depth, 3);
        assert_eq!(reorg.chain, ChainId::Ethereum);

        // Verify reorg logged
        let history = sync.get_reorg_history(&ChainId::Ethereum);
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn test_ring_buffer_capacity() {
        let sync = StateSyncManager::new();
        sync.init_chain(&ChainId::Bitcoin, 0);

        for i in 0..150u64 {
            sync.update_height(&ChainId::Bitcoin, i, &format!("0x{}", i), &format!("0x{}", i - 1))
                .unwrap();
        }

        let state = sync.get_sync_status(&ChainId::Bitcoin).unwrap();
        assert_eq!(state.synced_height, 149);
        assert!(state.snapshots.len() <= RING_BUFFER_CAPACITY);
    }

    #[test]
    fn test_force_resync() {
        let sync = StateSyncManager::new();
        sync.init_chain(&ChainId::Cardano, 500);
        sync.update_height(&ChainId::Cardano, 1000, "0xaa", "0xbb").unwrap();

        sync.force_resync(&ChainId::Cardano, 800).unwrap();
        let state = sync.get_sync_status(&ChainId::Cardano).unwrap();
        assert_eq!(state.synced_height, 800);
        assert!(state.syncing);
        assert!(state.snapshots.is_empty());
    }

    #[test]
    fn test_event_enqueue_and_dequeue() {
        let router = EventRouter::new();
        let event = CrossChainEvent {
            event_id: String::new(),
            source_chain: ChainId::Polygon,
            target_chain: Some(ChainId::Ethereum),
            event_type: EventType::Transfer,
            block_height: 42,
            tx_id: "0xtx1".to_string(),
            payload: "{}".to_string(),
            timestamp: now_secs(),
            outcome: None,
            retry_count: 0,
            max_retries: 3,
        };

        let eid = router.enqueue(event).unwrap();
        assert!(!eid.is_empty());
        assert_eq!(router.queue_depth(&ChainId::Polygon), 1);

        let dequeued = router.dequeue(&ChainId::Polygon).unwrap();
        assert_eq!(dequeued.event_type, EventType::Transfer);
        assert_eq!(router.queue_depth(&ChainId::Polygon), 0);
    }

    #[test]
    fn test_event_replay() {
        let router = EventRouter::new();

        for i in 10..15u64 {
            let evt = CrossChainEvent {
                event_id: format!("evt-{}", i),
                source_chain: ChainId::Bsc,
                target_chain: None,
                event_type: EventType::StateChange,
                block_height: i,
                tx_id: format!("0x{}", i),
                payload: String::new(),
                timestamp: now_secs(),
                outcome: Some(EventOutcome::Success),
                retry_count: 0,
                max_retries: 3,
            };
            router.enqueue(evt).unwrap();
            router.mark_processed(&format!("evt-{}", i), EventOutcome::Success);
        }

        let replayed = router.replay_events(&ChainId::Bsc, 11, 13);
        assert_eq!(replayed.len(), 3);
        assert_eq!(replayed[0].block_height, 11);
        assert_eq!(replayed[2].block_height, 13);
    }

    #[test]
    fn test_analytics_record_and_summary() {
        let analytics = BridgeAnalytics::new();
        analytics.record_event(
            &ChainId::Ergo,
            &EventType::Lock,
            &EventOutcome::Success,
            50,
        );
        analytics.record_event(
            &ChainId::Ergo,
            &EventType::Commit,
            &EventOutcome::Failed {
                reason: "timeout".to_string(),
            },
            200,
        );

        let summary = analytics.get_global_summary();
        assert_eq!(summary.total_events_processed, 2);
        assert_eq!(summary.total_success, 1);
        assert_eq!(summary.total_failed, 1);
        assert_eq!(summary.avg_latency_ms, 125);

        let chain_stats = analytics.get_chain_analytics(&ChainId::Ergo).unwrap();
        assert_eq!(chain_stats.events_processed, 2);
    }

    #[test]
    fn test_latency_buckets() {
        let analytics = BridgeAnalytics::new();
        analytics.record_event(&ChainId::Ethereum, &EventType::Lock, &EventOutcome::Success, 30);
        analytics.record_event(&ChainId::Ethereum, &EventType::Lock, &EventOutcome::Success, 75);

        let stats = analytics.get_chain_analytics(&ChainId::Ethereum).unwrap();
        let bucket_50 = stats.latency_buckets.iter().find(|b| b.bucket_ms == 50).unwrap();
        assert_eq!(bucket_50.count, 1);
        let bucket_100 = stats
            .latency_buckets
            .iter()
            .find(|b| b.bucket_ms == 100)
            .unwrap();
        assert_eq!(bucket_100.count, 1);
    }

    #[test]
    fn test_route_event_end_to_end() {
        let router = CrossChainEventRouter::new();
        router.subscriptions.subscribe(
            ChainId::Ergo,
            Some(EventType::Lock),
            "https://hook.example.com",
        );

        let event = CrossChainEvent {
            event_id: String::new(),
            source_chain: ChainId::Ergo,
            target_chain: Some(ChainId::Ethereum),
            event_type: EventType::Lock,
            block_height: 500,
            tx_id: "0xlock1".to_string(),
            payload: r#"{"amount": 1000}"#.to_string(),
            timestamp: now_secs(),
            outcome: None,
            retry_count: 0,
            max_retries: 3,
        };

        let result = handle_route_event(
            &router.event_router,
            &router.subscriptions,
            &router.analytics,
            event,
        )
        .unwrap();

        assert!(!result.event_id.is_empty());
        assert_eq!(result.subscriptions_notified, 1);

        let summary = router.analytics.get_global_summary();
        assert_eq!(summary.total_events_processed, 1);
        assert_eq!(summary.total_success, 1);
    }

    #[test]
    fn test_throughput_calculation() {
        let analytics = BridgeAnalytics::new();
        analytics.record_event(&ChainId::Polygon, &EventType::Transfer, &EventOutcome::Success, 10);
        let throughput = analytics.calculate_throughput(&ChainId::Polygon);
        // Should be positive since events_processed > 0
        assert!(throughput > 0.0);
    }
}
