#![allow(dead_code)]
//! Oracle Data Consumer module.
//!
//! Reads Ergo oracle pool boxes as data inputs, extracts price data (R4),
//! epoch counters (R5), verifies Pool NFT authentication, and manages oracle
//! pool subscriptions. In Ergo's eUTXO model, oracle pool boxes are accessed
//! as DATA INPUTS (not consumed), allowing efficient on-chain price verification.
//!
//! Key patterns:
//! - Pool Box accessed as DATA INPUT: `dataInput.tokens(0)._1 == POOL_NFT_ID`
//! - Price extracted from: `dataInput.R4[Long].get` (aggregated rate in nanoERG per USD cent)
//! - Epoch counter from: `dataInput.R5[Int].get`
//! - Staleness check: compare current epoch with pool epoch

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::proxy;

// ================================================================
// Types
// ================================================================

/// Tracked oracle pool with current state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OraclePool {
    pub id: String,
    pub nft_token_id: String,
    pub reward_token_id: String,
    pub current_rate: i64,
    pub epoch_counter: i32,
    pub box_id: String,
    pub creation_height: u32,
    pub last_updated: i64,
}

/// Individual oracle box participating in a pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleBox {
    pub id: String,
    pub oracle_token_id: String,
    pub reward_tokens: u64,
    pub owner_pubkey: String,
    pub epoch_counter: i32,
    pub data_point: i64,
    pub box_id: String,
}

/// Configuration for a specific oracle pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    pub pool_nft_id: String,
    pub reward_token_id: String,
    pub min_oracles: u32,
    pub max_staleness_epochs: u32,
}

/// A single price reading from an oracle pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceReading {
    pub pool_id: String,
    pub rate: i64,
    pub epoch: i32,
    pub timestamp: i64,
    pub oracle_count: u32,
    pub source_box_id: String,
}

/// Subscription to price updates from a pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleSubscription {
    pub id: String,
    pub pool_id: String,
    pub callback_url: Option<String>,
    pub last_rate: i64,
    pub last_epoch: i32,
    pub active: bool,
    pub created_at: i64,
}

/// Raw data input reference (simulated Ergo box accessed as data input).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataInputRef {
    pub box_id: String,
    pub ergo_tree: String,
    pub value: u64,
    pub tokens: Vec<TokenRef>,
    pub registers: HashMap<String, String>,
}

/// Token reference within a box.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRef {
    pub token_id: String,
    pub amount: u64,
}

/// Consumer configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleConsumerConfig {
    pub default_staleness_threshold: u32,
    pub max_subscriptions: u32,
    pub auto_refresh_interval_ms: u64,
}

impl Default for OracleConsumerConfig {
    fn default() -> Self {
        Self {
            default_staleness_threshold: 30,
            max_subscriptions: 100,
            auto_refresh_interval_ms: 60_000,
        }
    }
}

/// Consumer statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleConsumerStats {
    pub total_reads: u64,
    pub total_subscriptions: u64,
    pub active_pools: u64,
    pub total_price_updates: u64,
}

// ================================================================
// Ingest payload types
// ================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestBoxData {
    pub box_id: String,
    pub rate: i64,
    pub epoch: i32,
    pub oracle_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterPoolRequest {
    pub pool_nft_id: String,
    pub reward_token_id: String,
    pub min_oracles: Option<u32>,
    pub max_staleness_epochs: Option<u32>,
    pub box_id: Option<String>,
    pub initial_rate: Option<i64>,
    pub initial_epoch: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscribeRequest {
    pub pool_id: String,
    pub callback_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchPricesRequest {
    pub pool_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConfigRequest {
    pub default_staleness_threshold: Option<u32>,
    pub max_subscriptions: Option<u32>,
    pub auto_refresh_interval_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceHistoryQuery {
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionListQuery {
    pub pool_id: Option<String>,
}

// ================================================================
// State
// ================================================================

pub struct OracleConsumerState {
    pub inner: Arc<OracleConsumerStateInner>,
}

impl Clone for OracleConsumerState {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

struct OracleConsumerStateInner {
    pools: DashMap<String, OraclePool>,
    subscriptions: DashMap<String, OracleSubscription>,
    price_history: DashMap<String, Vec<PriceReading>>,
    configs: DashMap<String, PoolConfig>,
    config: tokio::sync::RwLock<OracleConsumerConfig>,
    stats_total_reads: AtomicU64,
    stats_total_subscriptions: AtomicU64,
    stats_total_price_updates: AtomicU64,
}

impl OracleConsumerState {
    pub fn new() -> Self {
        let state = Self {
            inner: Arc::new(OracleConsumerStateInner {
                pools: DashMap::new(),
                subscriptions: DashMap::new(),
                price_history: DashMap::new(),
                configs: DashMap::new(),
                config: tokio::sync::RwLock::new(OracleConsumerConfig::default()),
                stats_total_reads: AtomicU64::new(0),
                stats_total_subscriptions: AtomicU64::new(0),
                stats_total_price_updates: AtomicU64::new(0),
            }),
        };

        // Pre-seed 3 pools: ERG/USD, XRG/USD, BTC/USD
        state.seed_default_pools();

        state
    }

    fn seed_default_pools(&self) {
        let now = Utc::now().timestamp_millis();

        // ERG/USD pool
        let erg_pool = OraclePool {
            id: "erg-usd".to_string(),
            nft_token_id: "erg_usd_pool_nft_001".to_string(),
            reward_token_id: "erg_usd_reward_token_001".to_string(),
            current_rate: 523_000_000, // ~$0.523 in nanoERG per cent
            epoch_counter: 4200,
            box_id: "erg_pool_box_abc123".to_string(),
            creation_height: 1_000_000,
            last_updated: now,
        };

        // XRG/USD pool
        let xrg_pool = OraclePool {
            id: "xrg-usd".to_string(),
            nft_token_id: "xrg_usd_pool_nft_002".to_string(),
            reward_token_id: "xrg_usd_reward_token_002".to_string(),
            current_rate: 12_500_000, // ~$0.0125 in nanoERG per cent
            epoch_counter: 2100,
            box_id: "xrg_pool_box_def456".to_string(),
            creation_height: 1_200_000,
            last_updated: now,
        };

        // BTC/USD pool
        let btc_pool = OraclePool {
            id: "btc-usd".to_string(),
            nft_token_id: "btc_usd_pool_nft_003".to_string(),
            reward_token_id: "btc_usd_reward_token_003".to_string(),
            current_rate: 97_500_000_000, // ~$97,500 in nanoERG per cent
            epoch_counter: 8400,
            box_id: "btc_pool_box_ghi789".to_string(),
            creation_height: 800_000,
            last_updated: now,
        };

        self.inner.pools.insert("erg-usd".to_string(), erg_pool);
        self.inner.configs.insert(
            "erg-usd".to_string(),
            PoolConfig {
                pool_nft_id: "erg_usd_pool_nft_001".to_string(),
                reward_token_id: "erg_usd_reward_token_001".to_string(),
                min_oracles: 6,
                max_staleness_epochs: 30,
            },
        );

        self.inner.pools.insert("xrg-usd".to_string(), xrg_pool);
        self.inner.configs.insert(
            "xrg-usd".to_string(),
            PoolConfig {
                pool_nft_id: "xrg_usd_pool_nft_002".to_string(),
                reward_token_id: "xrg_usd_reward_token_002".to_string(),
                min_oracles: 4,
                max_staleness_epochs: 30,
            },
        );

        self.inner.pools.insert("btc-usd".to_string(), btc_pool);
        self.inner.configs.insert(
            "btc-usd".to_string(),
            PoolConfig {
                pool_nft_id: "btc_usd_pool_nft_003".to_string(),
                reward_token_id: "btc_usd_reward_token_003".to_string(),
                min_oracles: 6,
                max_staleness_epochs: 30,
            },
        );

        info!("Pre-seeded 3 oracle pools: erg-usd, xrg-usd, btc-usd");
    }
}

impl Default for OracleConsumerState {
    fn default() -> Self {
        Self::new()
    }
}

// ================================================================
// Engine
// ================================================================

pub struct OracleConsumerEngine {
    pub(crate) state: OracleConsumerState,
}

impl OracleConsumerEngine {
    pub fn new(state: OracleConsumerState) -> Self {
        Self { state }
    }

    // ----------------------------------------------------------------
    // Pool management
    // ----------------------------------------------------------------

    /// Register an oracle pool to track.
    pub fn register_pool(&self, req: &RegisterPoolRequest) -> Result<OraclePool, String> {
        let pool_id = format!("pool-{}", Uuid::new_v4().to_string()[..8].to_string());
        let now = Utc::now().timestamp_millis();

        let pool = OraclePool {
            id: pool_id.clone(),
            nft_token_id: req.pool_nft_id.clone(),
            reward_token_id: req.reward_token_id.clone(),
            current_rate: req.initial_rate.unwrap_or(0),
            epoch_counter: req.initial_epoch.unwrap_or(0),
            box_id: req.box_id.clone().unwrap_or_default(),
            creation_height: 0,
            last_updated: now,
        };

        let config = PoolConfig {
            pool_nft_id: req.pool_nft_id.clone(),
            reward_token_id: req.reward_token_id.clone(),
            min_oracles: req.min_oracles.unwrap_or(3),
            max_staleness_epochs: req.max_staleness_epochs.unwrap_or(30),
        };

        self.state.inner.pools.insert(pool_id.clone(), pool.clone());
        self.state.inner.configs.insert(pool_id.clone(), config);
        self.state.inner.price_history.insert(pool_id.clone(), Vec::new());

        info!(pool_id = %pool_id, "Registered oracle pool");
        Ok(pool)
    }

    /// Get pool details by ID.
    pub fn get_pool(&self, pool_id: &str) -> Option<OraclePool> {
        self.state.inner.pools.get(pool_id).map(|r| r.value().clone())
    }

    /// List all tracked pools.
    pub fn list_pools(&self) -> Vec<OraclePool> {
        self.state
            .inner
            .pools
            .iter()
            .map(|r| r.value().clone())
            .collect()
    }

    // ----------------------------------------------------------------
    // Price reading
    // ----------------------------------------------------------------

    /// Read current price from a pool (returns PriceReading).
    pub fn read_price(&self, pool_id: &str) -> Result<PriceReading, String> {
        self.state.inner.stats_total_reads.fetch_add(1, Ordering::Relaxed);

        let pool = self
            .state
            .inner
            .pools
            .get(pool_id)
            .ok_or_else(|| format!("Pool not found: {}", pool_id))?
            .clone();

        let reading = PriceReading {
            pool_id: pool_id.to_string(),
            rate: pool.current_rate,
            epoch: pool.epoch_counter,
            timestamp: pool.last_updated,
            oracle_count: self
                .state
                .inner
                .configs
                .get(pool_id)
                .map(|c| c.value().min_oracles)
                .unwrap_or(0),
            source_box_id: pool.box_id.clone(),
        };

        debug!(
            pool_id = %pool_id,
            rate = reading.rate,
            epoch = reading.epoch,
            "Read price from pool"
        );

        Ok(reading)
    }

    /// Extract price from a raw data input box.
    pub fn read_price_from_data_input(&self, data_input: &DataInputRef) -> Result<PriceReading, String> {
        self.state.inner.stats_total_reads.fetch_add(1, Ordering::Relaxed);

        // Extract R4 (Long) — aggregated rate in nanoERG per USD cent
        let rate_str = data_input
            .registers
            .get("R4")
            .ok_or("R4 register not found in data input")?;
        let rate: i64 = rate_str
            .parse()
            .map_err(|_| "Failed to parse R4 as i64")?;

        // Extract R5 (Int) — epoch counter
        let epoch_str = data_input
            .registers
            .get("R5")
            .ok_or("R5 register not found in data input")?;
        let epoch: i32 = epoch_str
            .parse()
            .map_err(|_| "Failed to parse R5 as i32")?;

        // Try to find the pool ID by matching NFT token
        let pool_id = data_input
            .tokens
            .first()
            .and_then(|t| {
                self.state
                    .inner
                    .pools
                    .iter()
                    .find(|p| p.value().nft_token_id == t.token_id)
                    .map(|p| p.key().clone())
            })
            .unwrap_or_else(|| {
                let id = Uuid::new_v4().to_string();
                format!("unknown-{}", &id[..8])
            });

        let now = Utc::now().timestamp_millis();

        let reading = PriceReading {
            pool_id: pool_id.clone(),
            rate,
            epoch,
            timestamp: now,
            oracle_count: 0,
            source_box_id: data_input.box_id.clone(),
        };

        debug!(
            box_id = %data_input.box_id,
            rate = rate,
            epoch = epoch,
            "Extracted price from data input"
        );

        Ok(reading)
    }

    // ----------------------------------------------------------------
    // Pool NFT verification
    // ----------------------------------------------------------------

    /// Verify Pool NFT authentication on a data input.
    /// In Ergo: `dataInput.tokens(0)._1 == POOL_NFT_ID`
    pub fn verify_pool_nft(&self, data_input: &DataInputRef, expected_nft_id: &str) -> Result<bool, String> {
        if data_input.tokens.is_empty() {
            return Err("Data input has no tokens".to_string());
        }

        let actual_nft = &data_input.tokens[0].token_id;
        let matches = actual_nft == expected_nft_id;

        if matches {
            info!(
                box_id = %data_input.box_id,
                nft = %expected_nft_id,
                "Pool NFT verified"
            );
        } else {
            warn!(
                box_id = %data_input.box_id,
                expected = %expected_nft_id,
                actual = %actual_nft,
                "Pool NFT mismatch"
            );
        }

        Ok(matches)
    }

    // ----------------------------------------------------------------
    // Subscriptions
    // ----------------------------------------------------------------

    /// Subscribe to price updates for a pool.
    pub fn subscribe(
        &self,
        pool_id: &str,
        callback_url: Option<String>,
    ) -> Result<OracleSubscription, String> {
        // Check pool exists
        if !self.state.inner.pools.contains_key(pool_id) {
            return Err(format!("Pool not found: {}", pool_id));
        }

        // Check subscription limit
        let config = self.state.inner.config.try_read().map_err(|e| e.to_string())?;
        if self.state.inner.subscriptions.len() as u32 >= config.max_subscriptions {
            return Err("Maximum subscriptions reached".to_string());
        }

        let pool = self.state.inner.pools.get(pool_id).unwrap();
        let sub_id = {
            let id = Uuid::new_v4().to_string();
            format!("sub-{}", &id[..8])
        };
        let now = Utc::now().timestamp_millis();

        let subscription = OracleSubscription {
            id: sub_id.clone(),
            pool_id: pool_id.to_string(),
            callback_url,
            last_rate: pool.value().current_rate,
            last_epoch: pool.value().epoch_counter,
            active: true,
            created_at: now,
        };

        drop(pool); // release borrow before inserting

        self.state.inner.subscriptions.insert(sub_id.clone(), subscription.clone());
        self.state.inner.stats_total_subscriptions.fetch_add(1, Ordering::Relaxed);

        info!(
            sub_id = %sub_id,
            pool_id = %pool_id,
            "Created oracle subscription"
        );

        Ok(subscription)
    }

    /// Unsubscribe by subscription ID.
    pub fn unsubscribe(&self, subscription_id: &str) -> Result<(), String> {
        let removed = self
            .state
            .inner
            .subscriptions
            .remove(subscription_id)
            .ok_or_else(|| format!("Subscription not found: {}", subscription_id))?;

        if removed.1.active {
            info!(
                sub_id = %subscription_id,
                pool_id = %removed.1.pool_id,
                "Unsubscribed from oracle pool"
            );
        }

        Ok(())
    }

    /// List subscriptions, optionally filtered by pool_id.
    pub fn list_subscriptions(&self, pool_id: Option<&str>) -> Vec<OracleSubscription> {
        let pool_filter = pool_id.map(|s| s.to_string());
        self.state
            .inner
            .subscriptions
            .iter()
            .filter(|r| {
                match &pool_filter {
                    None => true,
                    Some(pid) => r.value().pool_id == *pid,
                }
            })
            .map(|r| r.value().clone())
            .collect()
    }

    // ----------------------------------------------------------------
    // Price history
    // ----------------------------------------------------------------

    /// Get price history for a pool.
    pub fn get_price_history(
        &self,
        pool_id: &str,
        from: Option<i64>,
        to: Option<i64>,
        limit: Option<usize>,
    ) -> Result<Vec<PriceReading>, String> {
        let history = self
            .state
            .inner
            .price_history
            .get(pool_id)
            .ok_or_else(|| format!("No price history for pool: {}", pool_id))?;

        let mut readings: Vec<PriceReading> = history
            .value()
            .iter()
            .filter(|r| {
                if let Some(f) = from {
                    if r.timestamp < f {
                        return false;
                    }
                }
                if let Some(t) = to {
                    if r.timestamp > t {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        // Most recent first
        readings.reverse();

        if let Some(limit) = limit {
            readings.truncate(limit);
        }

        Ok(readings)
    }

    // ----------------------------------------------------------------
    // Staleness check
    // ----------------------------------------------------------------

    /// Check if pool data is stale by comparing epoch counters.
    pub fn check_staleness(&self, pool_id: &str) -> Result<StalenessInfo, String> {
        let pool = self
            .state
            .inner
            .pools
            .get(pool_id)
            .ok_or_else(|| format!("Pool not found: {}", pool_id))?;

        let config = self.state.inner.configs.get(pool_id);
        let threshold = config
            .map(|c| c.value().max_staleness_epochs)
            .unwrap_or(30);

        // Simulated current epoch (in real impl, would query chain height)
        let current_epoch = pool.epoch_counter + 5; // simulate 5 epochs ahead for testing

        let epochs_behind = (current_epoch - pool.epoch_counter).max(0) as u32;
        let is_stale = epochs_behind > threshold;

        let info = StalenessInfo {
            pool_id: pool_id.to_string(),
            pool_epoch: pool.epoch_counter,
            current_epoch,
            epochs_behind,
            threshold,
            is_stale,
        };

        debug!(
            pool_id = %pool_id,
            epochs_behind = epochs_behind,
            is_stale = is_stale,
            "Staleness check"
        );

        Ok(info)
    }

    // ----------------------------------------------------------------
    // Configuration
    // ----------------------------------------------------------------

    /// Get consumer configuration.
    pub fn get_config(&self) -> Result<OracleConsumerConfig, String> {
        let config = self.state.inner.config.try_read().map_err(|e| e.to_string())?;
        Ok(config.clone())
    }

    /// Update consumer configuration.
    pub async fn update_config(&self, update: &UpdateConfigRequest) -> Result<OracleConsumerConfig, String> {
        let mut config = self.state.inner.config.write().await;

        if let Some(threshold) = update.default_staleness_threshold {
            config.default_staleness_threshold = threshold;
        }
        if let Some(max_sub) = update.max_subscriptions {
            config.max_subscriptions = max_sub;
        }
        if let Some(interval) = update.auto_refresh_interval_ms {
            config.auto_refresh_interval_ms = interval;
        }

        info!("Updated oracle consumer config");
        Ok(config.clone())
    }

    // ----------------------------------------------------------------
    // Statistics
    // ----------------------------------------------------------------

    /// Get consumer statistics.
    pub fn get_stats(&self) -> OracleConsumerStats {
        OracleConsumerStats {
            total_reads: self.state.inner.stats_total_reads.load(Ordering::Relaxed),
            total_subscriptions: self.state.inner.stats_total_subscriptions.load(Ordering::Relaxed),
            active_pools: self.state.inner.pools.len() as u64,
            total_price_updates: self.state.inner.stats_total_price_updates.load(Ordering::Relaxed),
        }
    }

    // ----------------------------------------------------------------
    // Ingestion
    // ----------------------------------------------------------------

    /// Ingest a pool box update (simulated blockchain read).
    pub fn ingest_pool_box(&self, pool_id: &str, box_data: &IngestBoxData) -> Result<OraclePool, String> {
        let now = Utc::now().timestamp_millis();

        // Record in price history
        let reading = PriceReading {
            pool_id: pool_id.to_string(),
            rate: box_data.rate,
            epoch: box_data.epoch,
            timestamp: now,
            oracle_count: box_data.oracle_count,
            source_box_id: box_data.box_id.clone(),
        };

        if let Some(mut history) = self.state.inner.price_history.get_mut(pool_id) {
            history.push(reading);
            // Keep last 1000 readings
            let len = history.len();
            if len > 1000 {
                history.drain(..len - 1000);
            }
        }

        // Update pool state
        let mut pool = self
            .state
            .inner
            .pools
            .get_mut(pool_id)
            .ok_or_else(|| format!("Pool not found: {}", pool_id))?;

        pool.current_rate = box_data.rate;
        pool.epoch_counter = box_data.epoch;
        pool.box_id = box_data.box_id.clone();
        pool.last_updated = now;

        let updated = pool.clone();
        drop(pool); // release borrow

        self.state.inner.stats_total_price_updates.fetch_add(1, Ordering::Relaxed);

        // Update active subscriptions
        for mut sub in self.state.inner.subscriptions.iter_mut() {
            if sub.value().pool_id == pool_id && sub.value().active {
                sub.value_mut().last_rate = box_data.rate;
                sub.value_mut().last_epoch = box_data.epoch;
            }
        }

        info!(
            pool_id = %pool_id,
            rate = box_data.rate,
            epoch = box_data.epoch,
            "Ingested pool box update"
        );

        Ok(updated)
    }

    // ----------------------------------------------------------------
    // Batch operations
    // ----------------------------------------------------------------

    /// Read prices from multiple pools.
    pub fn batch_read_prices(&self, pool_ids: &[String]) -> Vec<Result<PriceReading, String>> {
        pool_ids
            .iter()
            .map(|id| self.read_price(id))
            .collect()
    }
}

/// Staleness check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StalenessInfo {
    pub pool_id: String,
    pub pool_epoch: i32,
    pub current_epoch: i32,
    pub epochs_behind: u32,
    pub threshold: u32,
    pub is_stale: bool,
}

// ================================================================
// REST Handlers
// ================================================================

/// POST /v1/oracle/pools - Register a pool
async fn register_pool_handler(
    State(state): State<proxy::AppState>,
    Json(req): Json<RegisterPoolRequest>,
) -> Response {
    let engine = OracleConsumerEngine::new(state.oracle_consumer.clone());
    match engine.register_pool(&req) {
        Ok(pool) => (StatusCode::CREATED, Json(pool)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// GET /v1/oracle/pools - List tracked pools
async fn list_pools_handler(
    State(state): State<proxy::AppState>,
) -> Response {
    let engine = OracleConsumerEngine::new(state.oracle_consumer.clone());
    let pools = engine.list_pools();
    (StatusCode::OK, Json(pools)).into_response()
}

/// GET /v1/oracle/pools/:id - Get pool details
async fn get_pool_handler(
    State(state): State<proxy::AppState>,
    Path(pool_id): Path<String>,
) -> Response {
    let engine = OracleConsumerEngine::new(state.oracle_consumer.clone());
    match engine.get_pool(&pool_id) {
        Some(pool) => (StatusCode::OK, Json(pool)).into_response(),
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Pool not found"}))).into_response(),
    }
}

/// GET /v1/oracle/pools/:id/price - Read current price
async fn read_price_handler(
    State(state): State<proxy::AppState>,
    Path(pool_id): Path<String>,
) -> Response {
    let engine = OracleConsumerEngine::new(state.oracle_consumer.clone());
    match engine.read_price(&pool_id) {
        Ok(reading) => (StatusCode::OK, Json(reading)).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// POST /v1/oracle/pools/:id/ingest - Ingest pool box update
async fn ingest_pool_box_handler(
    State(state): State<proxy::AppState>,
    Path(pool_id): Path<String>,
    Json(box_data): Json<IngestBoxData>,
) -> Response {
    let engine = OracleConsumerEngine::new(state.oracle_consumer.clone());
    match engine.ingest_pool_box(&pool_id, &box_data) {
        Ok(pool) => (StatusCode::OK, Json(pool)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// GET /v1/oracle/pools/:id/history - Get price history
async fn price_history_handler(
    State(state): State<proxy::AppState>,
    Path(pool_id): Path<String>,
    Query(query): Query<PriceHistoryQuery>,
) -> Response {
    let engine = OracleConsumerEngine::new(state.oracle_consumer.clone());
    match engine.get_price_history(&pool_id, query.from, query.to, query.limit) {
        Ok(history) => (StatusCode::OK, Json(history)).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// GET /v1/oracle/pools/:id/staleness - Check staleness
async fn staleness_handler(
    State(state): State<proxy::AppState>,
    Path(pool_id): Path<String>,
) -> Response {
    let engine = OracleConsumerEngine::new(state.oracle_consumer.clone());
    match engine.check_staleness(&pool_id) {
        Ok(info) => (StatusCode::OK, Json(info)).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// POST /v1/oracle/subscriptions - Subscribe to updates
async fn subscribe_handler(
    State(state): State<proxy::AppState>,
    Json(req): Json<SubscribeRequest>,
) -> Response {
    let engine = OracleConsumerEngine::new(state.oracle_consumer.clone());
    match engine.subscribe(&req.pool_id, req.callback_url) {
        Ok(sub) => (StatusCode::CREATED, Json(sub)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// DELETE /v1/oracle/subscriptions/:id - Unsubscribe
async fn unsubscribe_handler(
    State(state): State<proxy::AppState>,
    Path(sub_id): Path<String>,
) -> Response {
    let engine = OracleConsumerEngine::new(state.oracle_consumer.clone());
    match engine.unsubscribe(&sub_id) {
        Ok(()) => (StatusCode::NO_CONTENT, "").into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// GET /v1/oracle/subscriptions - List subscriptions
async fn list_subscriptions_handler(
    State(state): State<proxy::AppState>,
    Query(query): Query<SubscriptionListQuery>,
) -> Response {
    let engine = OracleConsumerEngine::new(state.oracle_consumer.clone());
    let subs = engine.list_subscriptions(query.pool_id.as_deref());
    (StatusCode::OK, Json(subs)).into_response()
}

/// POST /v1/oracle/batch-prices - Batch read prices
async fn batch_prices_handler(
    State(state): State<proxy::AppState>,
    Json(req): Json<BatchPricesRequest>,
) -> Response {
    let engine = OracleConsumerEngine::new(state.oracle_consumer.clone());
    let results: Vec<serde_json::Value> = engine
        .batch_read_prices(&req.pool_ids)
        .into_iter()
        .map(|r| match r {
            Ok(reading) => serde_json::json!({"ok": reading}),
            Err(e) => serde_json::json!({"error": e}),
        })
        .collect();
    (StatusCode::OK, Json(results)).into_response()
}

/// GET /v1/oracle/stats - Get statistics
async fn stats_handler(
    State(state): State<proxy::AppState>,
) -> Response {
    let engine = OracleConsumerEngine::new(state.oracle_consumer.clone());
    let stats = engine.get_stats();
    (StatusCode::OK, Json(stats)).into_response()
}

// ================================================================
// Router
// ================================================================

pub fn build_router(state: proxy::AppState) -> Router<proxy::AppState> {
    Router::new()
        .route("/v1/oracle/pools", post(register_pool_handler).get(list_pools_handler))
        .route("/v1/oracle/pools/{id}", get(get_pool_handler))
        .route("/v1/oracle/pools/{id}/price", get(read_price_handler))
        .route("/v1/oracle/pools/{id}/ingest", post(ingest_pool_box_handler))
        .route("/v1/oracle/pools/{id}/history", get(price_history_handler))
        .route("/v1/oracle/pools/{id}/staleness", get(staleness_handler))
        .route("/v1/oracle/subscriptions", post(subscribe_handler).get(list_subscriptions_handler))
        .route("/v1/oracle/subscriptions/{id}", delete(unsubscribe_handler))
        .route("/v1/oracle/batch-prices", post(batch_prices_handler))
        .route("/v1/oracle/stats", get(stats_handler))
        .with_state(state)
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_engine() -> OracleConsumerEngine {
        let state = OracleConsumerState::new();
        OracleConsumerEngine::new(state)
    }

    fn make_register_req(nft: &str, reward: &str) -> RegisterPoolRequest {
        RegisterPoolRequest {
            pool_nft_id: nft.to_string(),
            reward_token_id: reward.to_string(),
            min_oracles: Some(4),
            max_staleness_epochs: Some(20),
            box_id: None,
            initial_rate: Some(100_000_000),
            initial_epoch: Some(100),
        }
    }

    fn make_data_input(nft_token_id: &str, rate: i64, epoch: i32) -> DataInputRef {
        let mut registers = HashMap::new();
        registers.insert("R4".to_string(), rate.to_string());
        registers.insert("R5".to_string(), epoch.to_string());
        DataInputRef {
            box_id: {
                let id = Uuid::new_v4().to_string();
                format!("box-{}", &id[..8])
            },
            ergo_tree: "sigmaProp(true)".to_string(),
            value: 1_000_000_000,
            tokens: vec![TokenRef {
                token_id: nft_token_id.to_string(),
                amount: 1,
            }],
            registers,
        }
    }

    // ------------------------------------------------------------------
    // test_register_pool
    // ------------------------------------------------------------------
    #[test]
    fn test_register_pool() {
        let engine = setup_engine();
        let req = make_register_req("test_nft_abc", "test_reward_abc");
        let pool = engine.register_pool(&req).unwrap();

        assert!(pool.id.starts_with("pool-"));
        assert_eq!(pool.nft_token_id, "test_nft_abc");
        assert_eq!(pool.reward_token_id, "test_reward_abc");
        assert_eq!(pool.current_rate, 100_000_000);
        assert_eq!(pool.epoch_counter, 100);

        // Verify it can be retrieved
        let retrieved = engine.get_pool(&pool.id).unwrap();
        assert_eq!(retrieved.id, pool.id);
    }

    // ------------------------------------------------------------------
    // test_get_pool
    // ------------------------------------------------------------------
    #[test]
    fn test_get_pool() {
        let engine = setup_engine();

        // Pre-seeded pool should exist
        let pool = engine.get_pool("erg-usd").unwrap();
        assert_eq!(pool.id, "erg-usd");
        assert_eq!(pool.current_rate, 523_000_000);
        assert_eq!(pool.epoch_counter, 4200);

        // Non-existent pool
        assert!(engine.get_pool("nonexistent").is_none());
    }

    // ------------------------------------------------------------------
    // test_list_pools
    // ------------------------------------------------------------------
    #[test]
    fn test_list_pools() {
        let engine = setup_engine();
        let pools = engine.list_pools();

        // Should have 3 pre-seeded pools
        assert_eq!(pools.len(), 3);
        let ids: Vec<&str> = pools.iter().map(|p| p.id.as_str()).collect();
        assert!(ids.contains(&"erg-usd"));
        assert!(ids.contains(&"xrg-usd"));
        assert!(ids.contains(&"btc-usd"));
    }

    // ------------------------------------------------------------------
    // test_read_price
    // ------------------------------------------------------------------
    #[test]
    fn test_read_price() {
        let engine = setup_engine();

        let reading = engine.read_price("erg-usd").unwrap();
        assert_eq!(reading.pool_id, "erg-usd");
        assert_eq!(reading.rate, 523_000_000);
        assert_eq!(reading.epoch, 4200);
        assert_eq!(reading.source_box_id, "erg_pool_box_abc123");

        // Stats should increment
        let stats = engine.get_stats();
        assert_eq!(stats.total_reads, 1);
    }

    // ------------------------------------------------------------------
    // test_verify_pool_nft
    // ------------------------------------------------------------------
    #[test]
    fn test_verify_pool_nft() {
        let engine = setup_engine();

        // Matching NFT
        let di = make_data_input("erg_usd_pool_nft_001", 100, 10);
        let result = engine.verify_pool_nft(&di, "erg_usd_pool_nft_001").unwrap();
        assert!(result);

        // Mismatched NFT
        let result = engine.verify_pool_nft(&di, "wrong_nft").unwrap();
        assert!(!result);

        // No tokens
        let di_no_tokens = DataInputRef {
            box_id: "empty".to_string(),
            ergo_tree: "".to_string(),
            value: 0,
            tokens: vec![],
            registers: HashMap::new(),
        };
        let result = engine.verify_pool_nft(&di_no_tokens, "any");
        assert!(result.is_err());
    }

    // ------------------------------------------------------------------
    // test_subscribe
    // ------------------------------------------------------------------
    #[test]
    fn test_subscribe() {
        let engine = setup_engine();

        let sub = engine.subscribe("erg-usd", Some("http://callback.test".to_string())).unwrap();
        assert!(sub.id.starts_with("sub-"));
        assert_eq!(sub.pool_id, "erg-usd");
        assert_eq!(sub.last_rate, 523_000_000);
        assert!(sub.active);
        assert_eq!(sub.callback_url.as_deref(), Some("http://callback.test"));

        // Stats should increment
        let stats = engine.get_stats();
        assert_eq!(stats.total_subscriptions, 1);
    }

    // ------------------------------------------------------------------
    // test_unsubscribe
    // ------------------------------------------------------------------
    #[test]
    fn test_unsubscribe() {
        let engine = setup_engine();

        let sub = engine.subscribe("erg-usd", None).unwrap();
        assert!(engine.unsubscribe(&sub.id).is_ok());
        assert!(engine.unsubscribe(&sub.id).is_err()); // already removed
        assert!(engine.unsubscribe("nonexistent").is_err());
    }

    // ------------------------------------------------------------------
    // test_price_history
    // ------------------------------------------------------------------
    #[test]
    fn test_price_history() {
        let engine = setup_engine();

        // Initially empty for pre-seeded pools
        let history = engine.get_price_history("erg-usd", None, None, None).unwrap();
        assert!(history.is_empty());

        // Ingest some data
        let box_data = IngestBoxData {
            box_id: "test_box_1".to_string(),
            rate: 600_000_000,
            epoch: 4201,
            oracle_count: 6,
        };
        engine.ingest_pool_box("erg-usd", &box_data).unwrap();

        let box_data2 = IngestBoxData {
            box_id: "test_box_2".to_string(),
            rate: 610_000_000,
            epoch: 4202,
            oracle_count: 7,
        };
        engine.ingest_pool_box("erg-usd", &box_data2).unwrap();

        let history = engine.get_price_history("erg-usd", None, None, None).unwrap();
        assert_eq!(history.len(), 2);
        // Most recent first
        assert_eq!(history[0].rate, 610_000_000);
        assert_eq!(history[1].rate, 600_000_000);

        // With limit
        let limited = engine.get_price_history("erg-usd", None, None, Some(1)).unwrap();
        assert_eq!(limited.len(), 1);
        assert_eq!(limited[0].rate, 610_000_000);
    }

    // ------------------------------------------------------------------
    // test_staleness_check
    // ------------------------------------------------------------------
    #[test]
    fn test_staleness_check() {
        let engine = setup_engine();

        let info = engine.check_staleness("erg-usd").unwrap();
        assert_eq!(info.pool_id, "erg-usd");
        assert_eq!(info.pool_epoch, 4200);
        assert_eq!(info.epochs_behind, 5);
        assert_eq!(info.threshold, 30);
        assert!(!info.is_stale); // 5 < 30
    }

    // ------------------------------------------------------------------
    // test_ingest_pool_box
    // ------------------------------------------------------------------
    #[test]
    fn test_ingest_pool_box() {
        let engine = setup_engine();

        let box_data = IngestBoxData {
            box_id: "new_box_xyz".to_string(),
            rate: 550_000_000,
            epoch: 4205,
            oracle_count: 6,
        };
        let updated = engine.ingest_pool_box("erg-usd", &box_data).unwrap();

        assert_eq!(updated.current_rate, 550_000_000);
        assert_eq!(updated.epoch_counter, 4205);
        assert_eq!(updated.box_id, "new_box_xyz");

        // Verify pool is updated
        let pool = engine.get_pool("erg-usd").unwrap();
        assert_eq!(pool.current_rate, 550_000_000);

        // Stats
        let stats = engine.get_stats();
        assert_eq!(stats.total_price_updates, 1);
    }

    // ------------------------------------------------------------------
    // test_batch_read_prices
    // ------------------------------------------------------------------
    #[test]
    fn test_batch_read_prices() {
        let engine = setup_engine();

        let results = engine.batch_read_prices(&[
            "erg-usd".to_string(),
            "btc-usd".to_string(),
            "nonexistent".to_string(),
        ]);

        assert_eq!(results.len(), 3);
        assert!(results[0].is_ok()); // erg-usd
        assert!(results[1].is_ok()); // btc-usd
        assert!(results[2].is_err()); // nonexistent

        assert_eq!(results[0].as_ref().unwrap().rate, 523_000_000);
        assert_eq!(results[1].as_ref().unwrap().rate, 97_500_000_000);
    }

    // ------------------------------------------------------------------
    // test_config_update
    // ------------------------------------------------------------------
    #[tokio::test]
    async fn test_config_update() {
        let engine = setup_engine();

        let original = engine.get_config().unwrap();
        assert_eq!(original.default_staleness_threshold, 30);
        assert_eq!(original.max_subscriptions, 100);

        let update = UpdateConfigRequest {
            default_staleness_threshold: Some(50),
            max_subscriptions: Some(200),
            auto_refresh_interval_ms: None,
        };
        let updated = engine.update_config(&update).await.unwrap();
        assert_eq!(updated.default_staleness_threshold, 50);
        assert_eq!(updated.max_subscriptions, 200);
        assert_eq!(updated.auto_refresh_interval_ms, 60_000); // unchanged
    }

    // ------------------------------------------------------------------
    // test_stats_tracking
    // ------------------------------------------------------------------
    #[test]
    fn test_stats_tracking() {
        let engine = setup_engine();

        let initial = engine.get_stats();
        assert_eq!(initial.active_pools, 3); // pre-seeded
        assert_eq!(initial.total_reads, 0);

        // Read price
        engine.read_price("erg-usd").unwrap();
        engine.read_price("btc-usd").unwrap();

        // Ingest
        let box_data = IngestBoxData {
            box_id: "b1".to_string(),
            rate: 100,
            epoch: 1,
            oracle_count: 3,
        };
        engine.ingest_pool_box("erg-usd", &box_data).unwrap();

        // Subscribe
        engine.subscribe("xrg-usd", None).unwrap();

        let stats = engine.get_stats();
        assert_eq!(stats.total_reads, 2);
        assert_eq!(stats.total_subscriptions, 1);
        assert_eq!(stats.active_pools, 3);
        assert_eq!(stats.total_price_updates, 1);
    }

    // ------------------------------------------------------------------
    // test_concurrent_reads
    // ------------------------------------------------------------------
    #[tokio::test]
    async fn test_concurrent_reads() {
        let engine = setup_engine();
        let engine = Arc::new(engine);

        let mut handles = Vec::new();
        for _ in 0..100 {
            let e = engine.clone();
            handles.push(tokio::spawn(async move {
                e.read_price("erg-usd").unwrap()
            }));
        }

        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.unwrap());
        }

        assert_eq!(results.len(), 100);
        for r in &results {
            assert_eq!(r.rate, 523_000_000);
        }

        let stats = engine.get_stats();
        assert_eq!(stats.total_reads, 100);
    }

    // ------------------------------------------------------------------
    // test_data_input_extraction
    // ------------------------------------------------------------------
    #[test]
    fn test_data_input_extraction() {
        let engine = setup_engine();

        // Valid data input with R4 and R5
        let di = make_data_input("erg_usd_pool_nft_001", 999_000_000, 123);
        let reading = engine.read_price_from_data_input(&di).unwrap();

        assert_eq!(reading.rate, 999_000_000);
        assert_eq!(reading.epoch, 123);
        assert_eq!(reading.pool_id, "erg-usd"); // matched by NFT
        assert_eq!(reading.source_box_id, di.box_id);

        // Missing R4
        let mut di_no_r4 = di.clone();
        di_no_r4.registers.remove("R4");
        assert!(engine.read_price_from_data_input(&di_no_r4).is_err());

        // Missing R5
        let mut di_no_r5 = di.clone();
        di_no_r5.registers.remove("R5");
        assert!(engine.read_price_from_data_input(&di_no_r5).is_err());

        // Invalid R4
        let mut di_bad_r4 = di.clone();
        di_bad_r4.registers.insert("R4".to_string(), "not_a_number".to_string());
        assert!(engine.read_price_from_data_input(&di_bad_r4).is_err());

        // Unknown NFT token — should get unknown pool ID
        let di_unknown = make_data_input("unknown_nft_token", 100, 1);
        let reading_unknown = engine.read_price_from_data_input(&di_unknown).unwrap();
        assert!(reading_unknown.pool_id.starts_with("unknown-"));
    }
}
