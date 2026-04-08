//! # Price Feed
//!
//! Real-time price feed engine for the Xergon marketplace.
//! Provides price updates, aggregation, history tracking, alerts, volatility,
//! and trending-pair analytics for oracle-derived token prices.
//!
//! REST endpoints are nested under `/v1/prices`.

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post},
    Json, Router,
};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ================================================================
// PricePair
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PricePair {
    pub base: String,
    pub quote: String,
}

impl PricePair {
    pub fn new(base: &str, quote: &str) -> Self {
        Self {
            base: base.to_uppercase(),
            quote: quote.to_uppercase(),
        }
    }

    pub fn key(&self) -> String {
        format!("{}/{}", self.base, self.quote)
    }
}

// ================================================================
// PriceEntry
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PriceEntry {
    pub pair: String,
    pub rate: i64,
    pub epoch: i32,
    pub source_pool_id: String,
    pub timestamp: i64,
    pub confidence: f64,
}

// ================================================================
// PriceAggregation
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PriceAggregation {
    pub pair: String,
    pub best_rate: i64,
    pub worst_rate: i64,
    pub avg_rate: i64,
    pub median_rate: i64,
    pub source_count: u32,
    pub timestamp: i64,
}

// ================================================================
// AlertCondition
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AlertCondition {
    Above,
    Below,
    CrossesAbove,
    CrossesBelow,
}

// ================================================================
// PriceAlert
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PriceAlert {
    pub id: String,
    pub pair: String,
    pub condition: AlertCondition,
    pub threshold: i64,
    pub triggered: bool,
    pub created_at: i64,
    pub triggered_at: Option<i64>,
}

// ================================================================
// PriceFeedConfig
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PriceFeedConfig {
    pub update_interval_ms: u64,
    pub max_history_per_pair: u32,
    pub alert_check_interval_ms: u64,
}

impl Default for PriceFeedConfig {
    fn default() -> Self {
        Self {
            update_interval_ms: 5000,
            max_history_per_pair: 1000,
            alert_check_interval_ms: 10000,
        }
    }
}

// ================================================================
// PriceFeedStats
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PriceFeedStats {
    pub total_updates: u64,
    pub total_alerts: u64,
    pub triggered_alerts: u64,
    pub active_pairs: u64,
}

// ================================================================
// PriceHistoryEntry
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PriceHistoryEntry {
    pub rate: i64,
    pub epoch: i32,
    pub timestamp: i64,
}

// ================================================================
// AppState
// ================================================================

#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<PriceFeedEngine>,
}

// ================================================================
// Request / Response DTOs
// ================================================================

#[derive(Deserialize)]
pub struct UpdatePriceRequest {
    pub pair: String,
    pub rate: i64,
    pub epoch: i32,
    pub source_pool_id: String,
    pub confidence: Option<f64>,
}

#[derive(Deserialize)]
pub struct HistoryQueryParams {
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub limit: Option<u32>,
}

#[derive(Deserialize)]
pub struct CreateAlertRequest {
    pub pair: String,
    pub condition: AlertCondition,
    pub threshold: i64,
}

#[derive(Deserialize)]
pub struct AlertQueryParams {
    pub pair: Option<String>,
    pub triggered_only: Option<bool>,
}

#[derive(Deserialize)]
pub struct CheckAlertsRequest {
    pub pair: Option<String>,
}

#[derive(Deserialize)]
pub struct TrendingQueryParams {
    pub limit: Option<u32>,
}

#[derive(Deserialize)]
pub struct VolatilityQueryParams {
    pub window_epochs: Option<u32>,
}

#[derive(Serialize)]
pub struct UpdatePriceResponse {
    pub pair: String,
    pub rate: i64,
    pub epoch: i32,
    pub timestamp: i64,
}

#[derive(Serialize)]
pub struct CreateAlertResponse {
    pub id: String,
    pub pair: String,
    pub condition: String,
    pub threshold: i64,
}

#[derive(Serialize)]
pub struct DeleteAlertResponse {
    pub deleted: bool,
}

#[derive(Serialize)]
pub struct CheckAlertsResponse {
    pub checked: u32,
    pub newly_triggered: u32,
}

#[derive(Serialize)]
pub struct ConfigUpdateRequest {
    pub update_interval_ms: Option<u64>,
    pub max_history_per_pair: Option<u32>,
    pub alert_check_interval_ms: Option<u64>,
}

// ================================================================
// PriceFeedEngine
// ================================================================

pub struct PriceFeedEngine {
    /// Latest price per (pair, source_pool_id).
    prices: DashMap<String, PriceEntry>,
    /// Price history per pair (ring buffer via capped vec).
    history: DashMap<String, Vec<PriceHistoryEntry>>,
    /// Config.
    config: DashMap<String, PriceFeedConfig>,
    /// Alerts.
    alerts: DashMap<String, PriceAlert>,
    /// Stats counters.
    total_updates: std::sync::atomic::AtomicU64,
    total_alerts: std::sync::atomic::AtomicU64,
    triggered_alerts: std::sync::atomic::AtomicU64,
}

impl PriceFeedEngine {
    pub fn new() -> Self {
        let engine = Self {
            prices: DashMap::new(),
            history: DashMap::new(),
            config: DashMap::new(),
            alerts: DashMap::new(),
            total_updates: std::sync::atomic::AtomicU64::new(0),
            total_alerts: std::sync::atomic::AtomicU64::new(0),
            triggered_alerts: std::sync::atomic::AtomicU64::new(0),
        };

        // Store default config
        engine
            .config
            .insert("default".to_string(), PriceFeedConfig::default());

        engine
    }

    /// Create engine with pre-seeded data for ERG/USD, XRG/USD, BTC/USD.
    pub fn with_seed_data() -> Self {
        let engine = Self::new();

        // Seed ERG/USD
        engine.update_price("ERG/USD", 185_000, 100, "pool-erg-001", Some(0.95));
        engine.update_price("ERG/USD", 186_200, 101, "pool-erg-002", Some(0.92));
        engine.update_price("ERG/USD", 184_800, 102, "pool-erg-001", Some(0.97));

        // Seed XRG/USD
        engine.update_price("XRG/USD", 1_250_000, 50, "pool-xrg-001", Some(0.90));
        engine.update_price("XRG/USD", 1_270_000, 51, "pool-xrg-002", Some(0.88));
        engine.update_price("XRG/USD", 1_260_000, 52, "pool-xrg-001", Some(0.93));

        // Seed BTC/USD
        engine.update_price("BTC/USD", 67_500_000_000, 200, "pool-btc-001", Some(0.99));
        engine.update_price("BTC/USD", 67_650_000_000, 201, "pool-btc-002", Some(0.98));
        engine.update_price("BTC/USD", 67_480_000_000, 202, "pool-btc-003", Some(0.97));

        engine
    }

    /// Get the effective config.
    fn effective_config(&self) -> PriceFeedConfig {
        self.config
            .get("default")
            .map(|r| r.value().clone())
            .unwrap_or_default()
    }

    // ── update_price ──────────────────────────────────────────────

    pub fn update_price(
        &self,
        pair: &str,
        rate: i64,
        epoch: i32,
        source_pool_id: &str,
        confidence: Option<f64>,
    ) -> PriceEntry {
        let pair_upper = pair.to_uppercase();
        let now = Utc::now().timestamp_millis();
        let conf = confidence.unwrap_or(1.0);

        let entry = PriceEntry {
            pair: pair_upper.clone(),
            rate,
            epoch,
            source_pool_id: source_pool_id.to_string(),
            timestamp: now,
            confidence: conf,
        };

        // Store latest per (pair, source)
        let key = format!("{}:{}", pair_upper, source_pool_id);
        self.prices.insert(key, entry.clone());

        // Append to history
        let history_entry = PriceHistoryEntry {
            rate,
            epoch,
            timestamp: now,
        };

        let max_history = self.effective_config().max_history_per_pair as usize;

        self.history
            .entry(pair_upper.clone())
            .and_modify(|h| {
                h.push(history_entry.clone());
                if h.len() > max_history {
                    let drain_from = h.len() - max_history;
                    h.drain(0..drain_from);
                }
            })
            .or_insert_with(|| vec![history_entry]);

        // Increment stats
        self.total_updates
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        entry
    }

    // ── get_current_price ─────────────────────────────────────────

    /// Get the latest price entry for a pair (best-confidence source).
    pub fn get_current_price(&self, pair: &str) -> Option<PriceEntry> {
        let pair_upper = pair.to_uppercase();
        let prefix = format!("{}:", pair_upper);

        let mut best: Option<PriceEntry> = None;
        for item in self.prices.iter() {
            let k = item.key();
            if k.starts_with(&prefix) {
                if best.is_none() || item.value().confidence > best.as_ref().unwrap().confidence {
                    best = Some(item.value().clone());
                }
            }
        }
        best
    }

    // ── get_aggregated_price ──────────────────────────────────────

    pub fn get_aggregated_price(&self, pair: &str) -> Option<PriceAggregation> {
        let pair_upper = pair.to_uppercase();
        let prefix = format!("{}:", pair_upper);
        let now = Utc::now().timestamp_millis();

        let mut rates: Vec<i64> = Vec::new();
        for item in self.prices.iter() {
            if item.key().starts_with(&prefix) {
                rates.push(item.value().rate);
            }
        }

        if rates.is_empty() {
            return None;
        }

        rates.sort();

        let best = *rates.last().unwrap();
        let worst = *rates.first().unwrap();
        let sum: i64 = rates.iter().sum();
        let avg = sum / rates.len() as i64;
        let median = rates[rates.len() / 2];
        let source_count = rates.len() as u32;

        Some(PriceAggregation {
            pair: pair_upper,
            best_rate: best,
            worst_rate: worst,
            avg_rate: avg,
            median_rate: median,
            source_count,
            timestamp: now,
        })
    }

    // ── get_all_prices ────────────────────────────────────────────

    pub fn get_all_prices(&self) -> Vec<PriceEntry> {
        // Return one entry per pair (highest confidence).
        let mut by_pair: HashMap<String, PriceEntry> = HashMap::new();
        for item in self.prices.iter() {
            let entry = item.value();
            if let Some(existing) = by_pair.get(&entry.pair) {
                if entry.confidence > existing.confidence {
                    by_pair.insert(entry.pair.clone(), entry.clone());
                }
            } else {
                by_pair.insert(entry.pair.clone(), entry.clone());
            }
        }
        by_pair.into_values().collect()
    }

    // ── get_price_history ─────────────────────────────────────────

    pub fn get_price_history(
        &self,
        pair: &str,
        from: Option<i64>,
        to: Option<i64>,
        limit: Option<u32>,
    ) -> Vec<PriceHistoryEntry> {
        let pair_upper = pair.to_uppercase();

        if let Some(h) = self.history.get(&pair_upper) {
            let mut entries: Vec<PriceHistoryEntry> = h
                .value()
                .iter()
                .filter(|e| {
                    if let Some(f) = from {
                        if e.timestamp < f {
                            return false;
                        }
                    }
                    if let Some(t) = to {
                        if e.timestamp > t {
                            return false;
                        }
                    }
                    true
                })
                .cloned()
                .collect();

            // Most recent first
            entries.reverse();

            if let Some(lim) = limit {
                entries.truncate(lim as usize);
            }

            entries
        } else {
            Vec::new()
        }
    }

    // ── get_price_by_source ───────────────────────────────────────

    pub fn get_price_by_source(&self, pair: &str, source_pool_id: &str) -> Option<PriceEntry> {
        let key = format!("{}:{}", pair.to_uppercase(), source_pool_id);
        self.prices.get(&key).map(|r| r.value().clone())
    }

    // ── create_alert ──────────────────────────────────────────────

    pub fn create_alert(&self, pair: &str, condition: AlertCondition, threshold: i64) -> PriceAlert {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().timestamp_millis();

        let alert = PriceAlert {
            id: id.clone(),
            pair: pair.to_uppercase(),
            condition,
            threshold,
            triggered: false,
            created_at: now,
            triggered_at: None,
        };

        self.alerts.insert(id.clone(), alert.clone());

        self.total_alerts
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        alert
    }

    // ── delete_alert ──────────────────────────────────────────────

    pub fn delete_alert(&self, alert_id: &str) -> bool {
        self.alerts.remove(alert_id).is_some()
    }

    // ── check_alerts ──────────────────────────────────────────────

    pub fn check_alerts(&self, pair: Option<&str>) -> (u32, u32) {
        let mut checked: u32 = 0;
        let mut newly_triggered: u32 = 0;

        for mut item in self.alerts.iter_mut() {
            let alert = item.value_mut();

            if let Some(filter_pair) = pair {
                if alert.pair != filter_pair.to_uppercase() {
                    continue;
                }
            }

            checked += 1;

            if alert.triggered {
                continue;
            }

            let current = self.get_current_price(&alert.pair);
            let rate = match current {
                Some(c) => c.rate,
                None => continue,
            };

            let should_trigger = match &alert.condition {
                AlertCondition::Above => rate > alert.threshold,
                AlertCondition::Below => rate < alert.threshold,
                AlertCondition::CrossesAbove => rate > alert.threshold,
                AlertCondition::CrossesBelow => rate < alert.threshold,
            };

            if should_trigger {
                alert.triggered = true;
                alert.triggered_at = Some(Utc::now().timestamp_millis());
                newly_triggered += 1;
                self.triggered_alerts
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }

        (checked, newly_triggered)
    }

    // ── list_alerts ───────────────────────────────────────────────

    pub fn list_alerts(&self, pair: Option<&str>, triggered_only: Option<bool>) -> Vec<PriceAlert> {
        self.alerts
            .iter()
            .filter(|item| {
                let alert = item.value();
                if let Some(p) = pair {
                    if alert.pair != p.to_uppercase() {
                        return false;
                    }
                }
                if let Some(true) = triggered_only {
                    if !alert.triggered {
                        return false;
                    }
                }
                true
            })
            .map(|item| item.value().clone())
            .collect()
    }

    // ── get_trending_pairs ────────────────────────────────────────

    pub fn get_trending_pairs(&self, limit: Option<u32>) -> Vec<(String, u32)> {
        let lim = limit.unwrap_or(10) as usize;
        let mut pair_counts: HashMap<String, u32> = HashMap::new();

        for item in self.prices.iter() {
            let pair = item.value().pair.clone();
            *pair_counts.entry(pair).or_insert(0) += 1;
        }

        let mut sorted: Vec<(String, u32)> = pair_counts.into_iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted.truncate(lim);
        sorted
    }

    // ── get_volatility ────────────────────────────────────────────

    pub fn get_volatility(&self, pair: &str, window_epochs: Option<u32>) -> Option<f64> {
        let pair_upper = pair.to_uppercase();
        let window = window_epochs.unwrap_or(10) as usize;

        let entries = self.history.get(&pair_upper)?;
        let h = entries.value();

        if h.len() < 2 {
            return Some(0.0);
        }

        let window_entries: Vec<&PriceHistoryEntry> = if h.len() > window {
            h.iter().rev().take(window).collect()
        } else {
            h.iter().collect()
        };

        if window_entries.len() < 2 {
            return Some(0.0);
        }

        let rates: Vec<f64> = window_entries.iter().map(|e| e.rate as f64).collect();
        let n = rates.len() as f64;
        let mean: f64 = rates.iter().sum::<f64>() / n;
        let variance: f64 = rates.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0);
        let stddev = variance.sqrt();

        // Volatility as coefficient of variation (percentage)
        if mean.abs() < 1e-12 {
            return Some(0.0);
        }

        Some((stddev / mean.abs()) * 100.0)
    }

    // ── get_config ────────────────────────────────────────────────

    pub fn get_config(&self) -> PriceFeedConfig {
        self.effective_config()
    }

    // ── update_config ─────────────────────────────────────────────

    pub fn update_config(&self, new_config: PriceFeedConfig) {
        self.config.insert("default".to_string(), new_config);
    }

    // ── get_stats ─────────────────────────────────────────────────

    pub fn get_stats(&self) -> PriceFeedStats {
        let active_pairs = self
            .prices
            .iter()
            .map(|item| item.value().pair.clone())
            .collect::<std::collections::HashSet<_>>()
            .len() as u64;

        PriceFeedStats {
            total_updates: self.total_updates.load(std::sync::atomic::Ordering::Relaxed),
            total_alerts: self.total_alerts.load(std::sync::atomic::Ordering::Relaxed),
            triggered_alerts: self.triggered_alerts.load(std::sync::atomic::Ordering::Relaxed),
            active_pairs,
        }
    }
}

// ================================================================
// REST Handlers
// ================================================================

async fn get_all_prices(State(state): State<AppState>) -> Json<Vec<PriceEntry>> {
    Json(state.engine.get_all_prices())
}

async fn get_price(
    State(state): State<AppState>,
    Path(pair): Path<String>,
) -> Json<Option<PriceEntry>> {
    Json(state.engine.get_current_price(&pair))
}

async fn get_aggregated(
    State(state): State<AppState>,
    Path(pair): Path<String>,
) -> Json<Option<PriceAggregation>> {
    Json(state.engine.get_aggregated_price(&pair))
}

async fn get_history(
    State(state): State<AppState>,
    Path(pair): Path<String>,
    Query(params): Query<HistoryQueryParams>,
) -> Json<Vec<PriceHistoryEntry>> {
    Json(state.engine.get_price_history(&pair, params.from, params.to, params.limit))
}

async fn update_price(
    State(state): State<AppState>,
    Json(req): Json<UpdatePriceRequest>,
) -> Json<UpdatePriceResponse> {
    let entry = state
        .engine
        .update_price(&req.pair, req.rate, req.epoch, &req.source_pool_id, req.confidence);
    Json(UpdatePriceResponse {
        pair: entry.pair,
        rate: entry.rate,
        epoch: entry.epoch,
        timestamp: entry.timestamp,
    })
}

async fn create_alert(
    State(state): State<AppState>,
    Json(req): Json<CreateAlertRequest>,
) -> Json<CreateAlertResponse> {
    let alert = state
        .engine
        .create_alert(&req.pair, req.condition, req.threshold);
    let cond_str = format!("{:?}", alert.condition);
    Json(CreateAlertResponse {
        id: alert.id,
        pair: alert.pair,
        condition: cond_str,
        threshold: alert.threshold,
    })
}

async fn delete_alert(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Json<DeleteAlertResponse> {
    let deleted = state.engine.delete_alert(&id);
    Json(DeleteAlertResponse { deleted })
}

async fn list_alerts(
    State(state): State<AppState>,
    Query(params): Query<AlertQueryParams>,
) -> Json<Vec<PriceAlert>> {
    Json(state.engine.list_alerts(params.pair.as_deref(), params.triggered_only))
}

async fn check_alerts(
    State(state): State<AppState>,
    Json(req): Json<CheckAlertsRequest>,
) -> Json<CheckAlertsResponse> {
    let (checked, newly_triggered) = state.engine.check_alerts(req.pair.as_deref());
    Json(CheckAlertsResponse {
        checked,
        newly_triggered,
    })
}

async fn get_trending(
    State(state): State<AppState>,
    Query(params): Query<TrendingQueryParams>,
) -> Json<Vec<(String, u32)>> {
    Json(state.engine.get_trending_pairs(params.limit))
}

async fn get_volatility(
    State(state): State<AppState>,
    Path(pair): Path<String>,
    Query(params): Query<VolatilityQueryParams>,
) -> Json<Option<f64>> {
    Json(state.engine.get_volatility(&pair, params.window_epochs))
}

async fn get_stats(State(state): State<AppState>) -> Json<PriceFeedStats> {
    Json(state.engine.get_stats())
}

// ================================================================
// Router
// ================================================================

pub fn price_feed_router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_all_prices))
        .route("/stats", get(get_stats))
        .route("/trending", get(get_trending))
        .route("/alerts", get(list_alerts))
        .route("/alerts", post(create_alert))
        .route("/alerts/check", post(check_alerts))
        .route("/alerts/{id}", delete(delete_alert))
        .route("/update", post(update_price))
        .route("/{pair}", get(get_price))
        .route("/{pair}/aggregated", get(get_aggregated))
        .route("/{pair}/history", get(get_history))
        .route("/{pair}/volatility", get(get_volatility))
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> Arc<PriceFeedEngine> {
        Arc::new(PriceFeedEngine::new())
    }

    #[test]
    fn test_update_price() {
        let engine = make_engine();
        let entry = engine.update_price("ERG/USD", 185_000, 1, "pool-1", Some(0.95));

        assert_eq!(entry.pair, "ERG/USD");
        assert_eq!(entry.rate, 185_000);
        assert_eq!(entry.epoch, 1);
        assert_eq!(entry.source_pool_id, "pool-1");
        assert!((entry.confidence - 0.95).abs() < 1e-9);
        assert!(entry.timestamp > 0);

        // Verify stored
        let stored = engine.get_price_by_source("ERG/USD", "pool-1");
        assert!(stored.is_some());
        let stored = stored.unwrap();
        assert_eq!(stored.rate, 185_000);
    }

    #[test]
    fn test_get_current_price() {
        let engine = make_engine();
        engine.update_price("ERG/USD", 185_000, 1, "pool-1", Some(0.90));
        engine.update_price("ERG/USD", 186_000, 1, "pool-2", Some(0.95));

        let current = engine.get_current_price("ERG/USD").unwrap();
        assert_eq!(current.rate, 186_000);
        assert_eq!(current.source_pool_id, "pool-2");
    }

    #[test]
    fn test_get_aggregated_price() {
        let engine = make_engine();
        engine.update_price("BTC/USD", 67_000_000_000, 1, "pool-a", Some(0.99));
        engine.update_price("BTC/USD", 67_500_000_000, 1, "pool-b", Some(0.95));
        engine.update_price("BTC/USD", 67_200_000_000, 1, "pool-c", Some(0.90));

        let agg = engine.get_aggregated_price("BTC/USD").unwrap();
        assert_eq!(agg.source_count, 3);
        assert_eq!(agg.best_rate, 67_500_000_000);
        assert_eq!(agg.worst_rate, 67_000_000_000);
        // median of [67_000_000_000, 67_200_000_000, 67_500_000_000]
        assert_eq!(agg.median_rate, 67_200_000_000);
    }

    #[test]
    fn test_get_all_prices() {
        let engine = make_engine();
        engine.update_price("ERG/USD", 185_000, 1, "pool-1", Some(0.95));
        engine.update_price("XRG/USD", 1_200_000, 1, "pool-2", Some(0.90));
        engine.update_price("ERG/USD", 186_000, 2, "pool-3", Some(0.85));

        let all = engine.get_all_prices();
        // Should return one per pair (highest confidence)
        let pairs: Vec<&str> = all.iter().map(|e| e.pair.as_str()).collect();
        assert!(pairs.contains(&"ERG/USD"));
        assert!(pairs.contains(&"XRG/USD"));

        let erg_entry = all.iter().find(|e| e.pair == "ERG/USD").unwrap();
        assert_eq!(erg_entry.rate, 185_000); // pool-1 has higher confidence
    }

    #[test]
    fn test_price_history() {
        let engine = make_engine();
        engine.update_price("ERG/USD", 100, 1, "pool-1", Some(0.9));
        engine.update_price("ERG/USD", 200, 2, "pool-1", Some(0.9));
        engine.update_price("ERG/USD", 300, 3, "pool-1", Some(0.9));

        let history = engine.get_price_history("ERG/USD", None, None, None);
        assert_eq!(history.len(), 3);
        // Most recent first
        assert_eq!(history[0].rate, 300);
        assert_eq!(history[2].rate, 100);

        // Test with limit
        let limited = engine.get_price_history("ERG/USD", None, None, Some(2));
        assert_eq!(limited.len(), 2);
    }

    #[test]
    fn test_create_alert() {
        let engine = make_engine();
        let alert = engine.create_alert("ERG/USD", AlertCondition::Above, 200_000);

        assert_eq!(alert.pair, "ERG/USD");
        assert!(!alert.triggered);
        assert!(alert.created_at > 0);
        assert!(alert.triggered_at.is_none());
    }

    #[test]
    fn test_check_alerts_above() {
        let engine = make_engine();
        engine.create_alert("ERG/USD", AlertCondition::Above, 200_000);

        // Price below threshold: should not trigger
        engine.update_price("ERG/USD", 190_000, 1, "pool-1", Some(0.95));
        let (checked, triggered) = engine.check_alerts(Some("ERG/USD"));
        assert_eq!(checked, 1);
        assert_eq!(triggered, 0);

        // Price above threshold: should trigger
        engine.update_price("ERG/USD", 210_000, 2, "pool-1", Some(0.95));
        let (checked, triggered) = engine.check_alerts(Some("ERG/USD"));
        assert_eq!(checked, 1);
        assert_eq!(triggered, 1);
    }

    #[test]
    fn test_check_alerts_below() {
        let engine = make_engine();
        engine.create_alert("XRG/USD", AlertCondition::Below, 1_000_000);

        engine.update_price("XRG/USD", 950_000, 1, "pool-1", Some(0.95));
        let (checked, triggered) = engine.check_alerts(Some("XRG/USD"));
        assert_eq!(checked, 1);
        assert_eq!(triggered, 1);
    }

    #[test]
    fn test_trending_pairs() {
        let engine = make_engine();
        engine.update_price("ERG/USD", 185_000, 1, "pool-a", Some(0.9));
        engine.update_price("ERG/USD", 186_000, 1, "pool-b", Some(0.9));
        engine.update_price("ERG/USD", 184_000, 1, "pool-c", Some(0.9));
        engine.update_price("XRG/USD", 1_200_000, 1, "pool-d", Some(0.9));

        let trending = engine.get_trending_pairs(None);
        assert_eq!(trending.len(), 2);
        // ERG/USD has 3 sources, XRG/USD has 1
        assert_eq!(trending[0].0, "ERG/USD");
        assert_eq!(trending[0].1, 3);
        assert_eq!(trending[1].0, "XRG/USD");
    }

    #[test]
    fn test_volatility() {
        let engine = make_engine();
        // Feed some prices
        for i in 0..20 {
            let rate = 100_000 + (i as i64 % 7) * 5_000;
            engine.update_price("ERG/USD", rate, i, "pool-1", Some(0.95));
        }

        let vol = engine.get_volatility("ERG/USD", None);
        assert!(vol.is_some());
        let vol = vol.unwrap();
        // Volatility should be > 0 since prices vary
        assert!(vol > 0.0);
    }

    #[test]
    fn test_config_update() {
        let engine = make_engine();
        let default = engine.get_config();
        assert_eq!(default.max_history_per_pair, 1000);

        let new_config = PriceFeedConfig {
            update_interval_ms: 1000,
            max_history_per_pair: 500,
            alert_check_interval_ms: 5000,
        };
        engine.update_config(new_config.clone());

        let updated = engine.get_config();
        assert_eq!(updated.update_interval_ms, 1000);
        assert_eq!(updated.max_history_per_pair, 500);
    }

    #[test]
    fn test_stats_tracking() {
        let engine = make_engine();
        engine.update_price("ERG/USD", 185_000, 1, "pool-1", Some(0.95));
        engine.update_price("ERG/USD", 186_000, 2, "pool-2", Some(0.90));
        engine.create_alert("ERG/USD", AlertCondition::Above, 200_000);

        let stats = engine.get_stats();
        assert_eq!(stats.total_updates, 2);
        assert_eq!(stats.total_alerts, 1);
        assert_eq!(stats.active_pairs, 1);
    }

    #[test]
    fn test_concurrent_updates() {
        let engine = make_engine();
        let handles: Vec<_> = (0..20)
            .map(|i| {
                let eng = engine.clone();
                std::thread::spawn(move || {
                    let pair = if i % 2 == 0 { "ERG/USD" } else { "XRG/USD" };
                    eng.update_price(pair, 100_000 + i as i64 * 100, i, "pool-1", Some(0.95));
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        let stats = engine.get_stats();
        assert_eq!(stats.total_updates, 20);
    }

    #[test]
    fn test_delete_alert() {
        let engine = make_engine();
        let alert = engine.create_alert("ERG/USD", AlertCondition::Above, 200_000);
        let id = alert.id.clone();

        assert!(engine.delete_alert(&id));
        assert!(!engine.delete_alert(&id)); // Already deleted

        let alerts = engine.list_alerts(None, None);
        assert!(alerts.is_empty());
    }

    #[test]
    fn test_multiple_sources() {
        let engine = make_engine();
        engine.update_price("ERG/USD", 185_000, 1, "source-a", Some(0.99));
        engine.update_price("ERG/USD", 186_000, 1, "source-b", Some(0.80));
        engine.update_price("ERG/USD", 184_000, 1, "source-c", Some(0.70));

        // get_current_price picks highest confidence
        let current = engine.get_current_price("ERG/USD").unwrap();
        assert_eq!(current.rate, 185_000);
        assert_eq!(current.source_pool_id, "source-a");

        // get_price_by_source
        let src_b = engine.get_price_by_source("ERG/USD", "source-b").unwrap();
        assert_eq!(src_b.rate, 186_000);

        // get_aggregated_price
        let agg = engine.get_aggregated_price("ERG/USD").unwrap();
        assert_eq!(agg.source_count, 3);
        assert_eq!(agg.best_rate, 186_000);
        assert_eq!(agg.worst_rate, 184_000);
    }
}
