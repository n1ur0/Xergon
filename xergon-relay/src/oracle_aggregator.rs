//! Oracle Aggregator module.
//!
//! Multi-oracle price aggregation with configurable strategies and failover.
//! Supports Ergo oracle pools (eUTXO data inputs), Crux Finance, Spectrum DEX,
//! CoinGecko, and custom sources. Prices are fetched asynchronously, cached,
//! and aggregated using median, weighted-average, mean, min, or max strategies.
//!
//! In Ergo's eUTXO model, oracle pool boxes are read via *data inputs* without
//! spending them, allowing efficient on-chain price verification.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Top-level configuration for the oracle aggregator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleAggregatorConfig {
    /// Individual oracle source definitions.
    pub sources: Vec<OracleSourceConfig>,
    /// How to combine prices from multiple sources.
    pub aggregation_strategy: AggregationStrategy,
    /// Fall back to next-best source on failure.
    pub failover_enabled: bool,
    /// Max age in seconds before a price is considered stale.
    pub max_age_seconds: u64,
    /// Max relative deviation between sources before flagging (0.05 = 5%).
    pub deviation_threshold: f64,
    /// Minimum number of sources required for a valid aggregation.
    pub min_sources: usize,
    /// Seconds between automatic background refreshes.
    pub refresh_interval_secs: u64,
    /// TTL in seconds for the aggregated result cache.
    pub cache_ttl_secs: u64,
}

impl Default for OracleAggregatorConfig {
    fn default() -> Self {
        Self {
            sources: Vec::new(),
            aggregation_strategy: AggregationStrategy::Median,
            failover_enabled: true,
            max_age_seconds: 300,
            deviation_threshold: 0.05,
            min_sources: 2,
            refresh_interval_secs: 60,
            cache_ttl_secs: 30,
        }
    }
}

/// Per-source configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleSourceConfig {
    /// Human-readable name (unique identifier).
    pub name: String,
    /// Type of oracle backend.
    pub source_type: OracleSourceType,
    /// Base URL for API requests.
    pub url: String,
    /// Optional API key for authenticated endpoints.
    pub api_key: Option<String>,
    /// Relative weight for weighted-average strategy.
    pub weight: f64,
    /// Lower value = higher priority during failover.
    pub priority: u32,
    /// Whether this source is active.
    pub enabled: bool,
    /// HTTP timeout in milliseconds.
    pub timeout_ms: u64,
}

impl Default for OracleSourceConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            source_type: OracleSourceType::Custom,
            url: String::new(),
            api_key: None,
            weight: 1.0,
            priority: 10,
            enabled: true,
            timeout_ms: 5000,
        }
    }
}

/// Supported oracle source types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OracleSourceType {
    /// Ergo on-chain oracle pool (eUTXO data inputs).
    ErgoOraclePool,
    /// Crux Finance price feed.
    CruxFinance,
    /// Spectrum DEX aggregated price.
    SpectrumDex,
    /// CoinGecko public API.
    CoinGecko,
    /// User-defined HTTP endpoint.
    Custom,
}

/// Strategy for combining multiple price sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregationStrategy {
    /// Median price across all sources.
    Median,
    /// Weighted by source weight.
    WeightedAverage,
    /// Simple arithmetic mean.
    Mean,
    /// Lowest reported price (conservative).
    Min,
    /// Highest reported price.
    Max,
}

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// Result of aggregating prices from multiple sources.
#[derive(Debug, Clone, Serialize)]
pub struct AggregatedPrice {
    /// Trading pair, e.g. "ERG/USD".
    pub pair: String,
    /// Final aggregated price.
    pub price: f64,
    /// Timestamp of the aggregation.
    pub timestamp: DateTime<Utc>,
    /// Names of sources that contributed.
    pub sources_used: Vec<String>,
    /// Strategy used.
    pub strategy: AggregationStrategy,
    /// Standard deviation across contributing sources.
    pub deviation: f64,
    /// 0.0-1.0 confidence based on source agreement.
    pub confidence: f64,
    /// Individual source prices used.
    pub individual_prices: Vec<SourcePrice>,
}

/// Price from a single oracle source.
#[derive(Debug, Clone, Serialize)]
pub struct SourcePrice {
    /// Source name.
    pub source: String,
    /// Reported price.
    pub price: f64,
    /// When this price was fetched.
    pub timestamp: DateTime<Utc>,
    /// Source weight used for aggregation.
    pub weight: f64,
    /// How old this price is (seconds).
    pub age_seconds: u64,
    /// Whether the price exceeds max_age_seconds.
    pub is_stale: bool,
}

/// Status of a single oracle source.
#[derive(Debug, Clone, Serialize)]
pub struct SourceStatus {
    pub name: String,
    pub source_type: OracleSourceType,
    pub enabled: bool,
    pub priority: u32,
    pub weight: f64,
    pub last_price: Option<f64>,
    pub last_fetch: Option<DateTime<Utc>>,
    pub fetch_count: u64,
    pub error_count: u64,
    pub is_healthy: bool,
}

/// Overall health of the aggregator.
#[derive(Debug, Clone, Serialize)]
pub struct AggregatorHealth {
    pub is_healthy: bool,
    pub total_sources: usize,
    pub healthy_sources: usize,
    pub stale_sources: usize,
    pub total_fetches: u64,
    pub total_errors: u64,
    pub avg_deviation: f64,
    pub uptime_seconds: u64,
    pub last_refresh: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Internal source state
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct SourceState {
    config: OracleSourceConfig,
    last_price: Option<f64>,
    last_fetch: Option<DateTime<Utc>>,
    fetch_count: AtomicU64,
    error_count: AtomicU64,
}

// ---------------------------------------------------------------------------
// OracleAggregator
// ---------------------------------------------------------------------------

/// Multi-oracle price aggregator with failover support.
pub struct OracleAggregator {
    config: RwLock<OracleAggregatorConfig>,
    sources: DashMap<String, SourceState>,
    price_cache: DashMap<String, (AggregatedPrice, DateTime<Utc>)>,
    http_client: reqwest::Client,
    total_fetches: AtomicU64,
    total_errors: AtomicU64,
    started_at: DateTime<Utc>,
    last_refresh: RwLock<Option<DateTime<Utc>>>,
}

impl OracleAggregator {
    /// Create a new aggregator with the given configuration.
    pub fn new(config: OracleAggregatorConfig) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(4)
            .build()
            .unwrap_or_default();

        let agg = Self {
            sources: DashMap::new(),
            config: RwLock::new(config),
            price_cache: DashMap::new(),
            http_client,
            total_fetches: AtomicU64::new(0),
            total_errors: AtomicU64::new(0),
            started_at: Utc::now(),
            last_refresh: RwLock::new(None),
        };

        // Register sources from initial config (sync snapshot)
        let cfg_snapshot = agg.config.try_read()
            .map(|c| c.sources.clone())
            .unwrap_or_default();
        for src in cfg_snapshot {
            agg.add_source_inner(src);
        }

        info!("OracleAggregator initialized");
        agg
    }

    /// Register a new oracle source.
    pub async fn add_source(&self, source: OracleSourceConfig) {
        self.add_source_inner(source);
    }

    fn add_source_inner(&self, source: OracleSourceConfig) {
        let name = source.name.clone();
        self.sources.insert(name.clone(), SourceState {
            config: source,
            last_price: None,
            last_fetch: None,
            fetch_count: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
        });
        info!(source = %name, "Oracle source added");
    }

    /// Remove a source by name.
    pub fn remove_source(&self, name: &str) -> bool {
        let removed = self.sources.remove(name).is_some();
        if removed {
            info!(source = %name, "Oracle source removed");
        }
        removed
    }

    /// Get aggregated price for a pair. Uses cache if available and fresh.
    pub async fn get_price(&self, pair: &str) -> Result<AggregatedPrice, String> {
        // Check cache first
        let cfg = self.config.read().await;
        if let Some(cached_ref) = self.price_cache.get(pair) {
            let (ref cached, ref ts) = *cached_ref;
            let age = (Utc::now().timestamp() - ts.timestamp()).unsigned_abs();
            if age < cfg.cache_ttl_secs {
                return Ok(cached.clone());
            }
        }
        drop(cfg);

        self.refresh_price(pair).await
    }

    /// Get all aggregated prices currently in cache.
    pub async fn get_all_prices(&self) -> Vec<AggregatedPrice> {
        self.price_cache
            .iter()
            .map(|entry| entry.value().0.clone())
            .collect()
    }

    /// Force-refresh the price for a specific pair.
    pub async fn refresh_price(&self, pair: &str) -> Result<AggregatedPrice, String> {
        let cfg = self.config.read().await;
        let mut prices: Vec<SourcePrice> = Vec::new();

        for entry in self.sources.iter() {
            let state = entry.value();
            if !state.config.enabled {
                continue;
            }
            match self.fetch_source_price(&state.config, pair).await {
                Ok(sp) => {
                    prices.push(sp);
                }
                Err(e) => {
                    warn!(source = %state.config.name, error = %e, "Source fetch failed");
                    state.error_count.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        if prices.len() < cfg.min_sources {
            return Err(format!(
                "Not enough sources: got {}, need {}",
                prices.len(),
                cfg.min_sources
            ));
        }

        let aggregated = self.compute_aggregation(pair, &prices, &cfg);
        let now = Utc::now();
        self.price_cache.insert(pair.to_string(), (aggregated.clone(), now));
        *self.last_refresh.write().await = Some(now);

        debug!(
            pair = %pair,
            price = aggregated.price,
            sources = aggregated.sources_used.len(),
            "Price refreshed"
        );

        Ok(aggregated)
    }

    /// Refresh all known pairs (currently refreshes ERG/USD and common pairs).
    pub async fn refresh_all(&self) {
        let pairs = vec!["ERG/USD".to_string(), "ERG/BTC".to_string(), "SigmaUSD/USD".to_string()];
        for pair in pairs {
            if let Err(e) = self.refresh_price(&pair).await {
                warn!(pair = %pair, error = %e, "Failed to refresh pair");
            }
        }
    }

    /// Get status of all registered sources.
    pub async fn get_source_status(&self) -> Vec<SourceStatus> {
        let cfg = self.config.read().await;
        self.sources.iter().map(|entry| {
            let state = entry.value();
            let now = Utc::now();
            let is_stale = state.last_fetch.map_or(true, |t| {
                (now - t).num_seconds().unsigned_abs() > cfg.max_age_seconds
            });
            SourceStatus {
                name: state.config.name.clone(),
                source_type: state.config.source_type,
                enabled: state.config.enabled,
                priority: state.config.priority,
                weight: state.config.weight,
                last_price: state.last_price,
                last_fetch: state.last_fetch,
                fetch_count: state.fetch_count.load(Ordering::Relaxed),
                error_count: state.error_count.load(Ordering::Relaxed),
                is_healthy: !is_stale && state.error_count.load(Ordering::Relaxed) < 10,
            }
        }).collect()
    }

    /// Get overall aggregator health.
    pub async fn get_health(&self) -> AggregatorHealth {
        let cfg = self.config.read().await;
        let statuses = self.get_source_status().await;
        let total = statuses.len();
        let healthy = statuses.iter().filter(|s| s.is_healthy).count();
        let stale = statuses.iter().filter(|s| !s.is_healthy).count();

        let avg_dev: f64 = if total > 0 {
            self.price_cache.iter().map(|e| e.value().0.deviation).sum::<f64>() / total as f64
        } else {
            0.0
        };

        let uptime = (Utc::now() - self.started_at).num_seconds().unsigned_abs();

        AggregatorHealth {
            is_healthy: healthy >= cfg.min_sources,
            total_sources: total,
            healthy_sources: healthy,
            stale_sources: stale,
            total_fetches: self.total_fetches.load(Ordering::Relaxed),
            total_errors: self.total_errors.load(Ordering::Relaxed),
            avg_deviation: avg_dev,
            uptime_seconds: uptime,
            last_refresh: *self.last_refresh.read().await,
        }
    }

    /// Update the aggregator configuration.
    pub async fn update_config(&self, new_config: OracleAggregatorConfig) {
        let mut cfg = self.config.write().await;
        // Sync sources
        for src in &new_config.sources {
            self.add_source_inner(src.clone());
        }
        *cfg = new_config;
        info!("OracleAggregator config updated");
    }

    // -- Internal methods ---------------------------------------------------

    /// Fetch price from a single source.
    async fn fetch_source_price(
        &self,
        config: &OracleSourceConfig,
        pair: &str,
    ) -> Result<SourcePrice, String> {
        let url = self.build_fetch_url(config, pair);
        let timeout = Duration::from_millis(config.timeout_ms);

        let mut req = self.http_client.get(&url).timeout(timeout);
        if let Some(key) = &config.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let resp = req.send().await.map_err(|e| {
            self.total_errors.fetch_add(1, Ordering::Relaxed);
            format!("HTTP error: {}", e)
        })?;

        if !resp.status().is_success() {
            self.total_errors.fetch_add(1, Ordering::Relaxed);
            return Err(format!("HTTP status: {}", resp.status()));
        }

        self.total_fetches.fetch_add(1, Ordering::Relaxed);

        let body: serde_json::Value = resp.json().await.map_err(|e| format!("JSON error: {}", e))?;
        let price = self.parse_price_from_body(&body, &config.source_type)?;

        // Update source state
        if let Some(mut state) = self.sources.get_mut(&config.name) {
            state.last_price = Some(price);
            state.last_fetch = Some(Utc::now());
            state.fetch_count.fetch_add(1, Ordering::Relaxed);
        }

        let now = Utc::now();
        Ok(SourcePrice {
            source: config.name.clone(),
            price,
            timestamp: now,
            weight: config.weight,
            age_seconds: 0,
            is_stale: false,
        })
    }

    /// Build the fetch URL for a source.
    fn build_fetch_url(&self, config: &OracleSourceConfig, pair: &str) -> String {
        let pair_encoded = pair.replace('/', "_").to_lowercase();
        match config.source_type {
            OracleSourceType::CoinGecko => {
                // e.g. https://api.coingecko.com/api/v3/simple/price?ids=ergo&vs_currencies=usd
                let coin_id = pair_encoded.split('_').next().unwrap_or("ergo");
                let vs = pair_encoded.split('_').nth(1).unwrap_or("usd");
                format!(
                    "{}/api/v3/simple/price?ids={}&vs_currencies={}",
                    config.url.trim_end_matches('/'), coin_id, vs
                )
            }
            OracleSourceType::CruxFinance => {
                format!(
                    "{}/api/v1/price?pair={}",
                    config.url.trim_end_matches('/'), pair_encoded
                )
            }
            OracleSourceType::SpectrumDex => {
                format!(
                    "{}/api/v1/aggregates?pair={}",
                    config.url.trim_end_matches('/'), pair_encoded
                )
            }
            OracleSourceType::ErgoOraclePool => {
                // Ergo oracle pools use node API to read oracle box data
                format!(
                    "{}/oracle/pool/{}",
                    config.url.trim_end_matches('/'), pair_encoded
                )
            }
            OracleSourceType::Custom => {
                format!(
                    "{}/price?pair={}",
                    config.url.trim_end_matches('/'), pair_encoded
                )
            }
        }
    }

    /// Parse price from a JSON response body.
    fn parse_price_from_body(
        &self,
        body: &serde_json::Value,
        source_type: &OracleSourceType,
    ) -> Result<f64, String> {
        match source_type {
            OracleSourceType::CoinGecko => {
                // {"ergo":{"usd":1.23}}
                let coin_id = body.as_object().and_then(|o| o.keys().next());
                if let Some(id) = coin_id {
                    if let Some(coin_data) = body[id].as_object() {
                        if let Some(vs_id) = coin_data.keys().next() {
                            if let Some(price) = coin_data[vs_id].as_f64() {
                                return Ok(price);
                            }
                        }
                    }
                }
                Err("Could not parse CoinGecko response".to_string())
            }
            OracleSourceType::CruxFinance | OracleSourceType::SpectrumDex => {
                // {"price": 1.23}
                body.get("price")
                    .and_then(|v| v.as_f64())
                    .ok_or_else(|| "Missing 'price' field".to_string())
            }
            OracleSourceType::ErgoOraclePool => {
                // {"value": 1230000, "decimals": 6}  => price = value / 10^decimals
                let value = body
                    .get("value")
                    .and_then(|v| v.as_i64())
                    .ok_or("Missing 'value'")?;
                let decimals = body
                    .get("decimals")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(9);
                Ok(value as f64 / 10_f64.powi(decimals as i32))
            }
            OracleSourceType::Custom => {
                body.get("price")
                    .and_then(|v| v.as_f64())
                    .or_else(|| body.get("value").and_then(|v| v.as_f64()))
                    .ok_or_else(|| "Missing 'price' or 'value' field".to_string())
            }
        }
    }

    /// Compute aggregated price from individual source prices.
    fn compute_aggregation(
        &self,
        pair: &str,
        prices: &[SourcePrice],
        cfg: &OracleAggregatorConfig,
    ) -> AggregatedPrice {
        let now = Utc::now();
        let mut values: Vec<f64> = prices.iter().map(|p| p.price).collect();
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let aggregated_price = match cfg.aggregation_strategy {
            AggregationStrategy::Median => {
                if values.len() % 2 == 0 {
                    (values[values.len() / 2 - 1] + values[values.len() / 2]) / 2.0
                } else {
                    values[values.len() / 2]
                }
            }
            AggregationStrategy::WeightedAverage => {
                let total_weight: f64 = prices.iter().map(|p| p.weight).sum();
                if total_weight > 0.0 {
                    prices.iter().map(|p| p.price * p.weight).sum::<f64>() / total_weight
                } else {
                    values[values.len() / 2]
                }
            }
            AggregationStrategy::Mean => {
                values.iter().sum::<f64>() / values.len() as f64
            }
            AggregationStrategy::Min => values[0],
            AggregationStrategy::Max => *values.last().unwrap_or(&0.0),
        };

        let deviation = self.check_deviation(prices);
        let confidence = self.compute_confidence(&deviation, cfg);

        // Apply failover if enabled and deviation too high
        let final_price = if cfg.failover_enabled && deviation > cfg.deviation_threshold {
            if let Some(fallback) = self.failover_select(prices) {
                debug!(
                    pair = %pair,
                    deviation = deviation,
                    fallback = %fallback.source,
                    "Failover triggered due to high deviation"
                );
                fallback.price
            } else {
                aggregated_price
            }
        } else {
            aggregated_price
        };

        AggregatedPrice {
            pair: pair.to_string(),
            price: final_price,
            timestamp: now,
            sources_used: prices.iter().map(|p| p.source.clone()).collect(),
            strategy: cfg.aggregation_strategy,
            deviation,
            confidence,
            individual_prices: prices.to_vec(),
        }
    }

    /// Compute standard deviation across source prices (relative).
    fn check_deviation(&self, prices: &[SourcePrice]) -> f64 {
        if prices.is_empty() {
            return 0.0;
        }
        let mean: f64 = prices.iter().map(|p| p.price).sum::<f64>() / prices.len() as f64;
        if mean == 0.0 {
            return 0.0;
        }
        let variance: f64 = prices
            .iter()
            .map(|p| (p.price - mean).powi(2))
            .sum::<f64>()
            / prices.len() as f64;
        let std_dev = variance.sqrt();
        std_dev / mean.abs()
    }

    /// Compute confidence (0.0-1.0) based on deviation vs threshold.
    fn compute_confidence(&self, deviation: &f64, cfg: &OracleAggregatorConfig) -> f64 {
        let threshold = cfg.deviation_threshold;
        if *deviation <= threshold {
            1.0 - (*deviation / threshold * 0.3) // 0.7-1.0 range
        } else {
            (threshold / deviation.max(0.001) * 0.7).max(0.0)
        }
    }

    /// Select best source during failover (lowest priority number = highest preference).
    fn failover_select(&self, prices: &[SourcePrice]) -> Option<SourcePrice> {
        prices
            .iter()
            .min_by(|a, b| a.age_seconds.cmp(&b.age_seconds))
            .cloned()
    }
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct PairQuery {
    pair: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RefreshQuery {
    pair: Option<String>,
}

fn err(msg: &str, code: StatusCode) -> Response {
    (code, Json(serde_json::json!({ "error": msg }))).into_response()
}

fn ok(val: serde_json::Value) -> Response {
    (StatusCode::OK, Json(val)).into_response()
}

/// GET /api/oracle/aggregated/price?pair=ERG/USD
async fn get_price_handler(
    State(state): State<AppState>,
    Query(q): Query<PairQuery>,
) -> Response {
    let pair = q.pair.unwrap_or_else(|| "ERG/USD".to_string());
    let result: Result<AggregatedPrice, String> = state.oracle_aggregator.get_price(pair.as_str()).await;
    match result {
        Ok(price) => ok(serde_json::to_value(&price).unwrap_or_default()),
        Err(ref e) => err(e.as_str(), StatusCode::BAD_GATEWAY),
    }
}

/// GET /api/oracle/aggregated/prices — All cached aggregated prices.
async fn get_prices_handler(State(state): State<AppState>) -> Response {
    let prices: Vec<AggregatedPrice> = state.oracle_aggregator.get_all_prices().await;
    ok(serde_json::to_value(&prices).unwrap_or_default())
}

/// POST /api/oracle/aggregated/refresh?pair=ERG/USD — Force refresh.
async fn refresh_handler(
    State(state): State<AppState>,
    Query(q): Query<RefreshQuery>,
) -> Response {
    match q.pair {
        Some(ref pair) => {
            let result: Result<AggregatedPrice, String> = state.oracle_aggregator.refresh_price(pair.as_str()).await;
            match result {
                Ok(price) => ok(serde_json::json!({
                    "status": "refreshed",
                    "pair": pair,
                    "price": price,
                })),
                Err(ref e) => err(e.as_str(), StatusCode::BAD_GATEWAY),
            }
        }
        None => {
            state.oracle_aggregator.refresh_all().await;
            ok(serde_json::json!({ "status": "all_refreshed" }))
        }
    }
}

/// GET /api/oracle/aggregated/sources — Source status list.
async fn sources_handler(State(state): State<AppState>) -> Response {
    let statuses: Vec<SourceStatus> = state.oracle_aggregator.get_source_status().await;
    ok(serde_json::to_value(&statuses).unwrap_or_default())
}

/// GET /api/oracle/aggregated/health — Aggregator health.
async fn health_handler(State(state): State<AppState>) -> Response {
    let health: AggregatorHealth = state.oracle_aggregator.get_health().await;
    ok(serde_json::to_value(&health).unwrap_or_default())
}

/// POST /api/oracle/aggregated/sources — Add a new source.
async fn add_source_handler(
    State(state): State<AppState>,
    Json(body): Json<OracleSourceConfig>,
) -> Response {
    if body.name.is_empty() {
        return err("Source name is required", StatusCode::BAD_REQUEST);
    }
    state.oracle_aggregator.add_source(body).await;
    ok(serde_json::json!({ "status": "source_added" }))
}

/// DELETE /api/oracle/aggregated/sources/:name — Remove a source.
async fn remove_source_handler(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Response {
    let removed = state.oracle_aggregator.remove_source(name.as_str());
    if removed {
        ok(serde_json::json!({ "status": "source_removed", "name": name }))
    } else {
        let msg = format!("Source '{}' not found", name);
        err(msg.as_str(), StatusCode::NOT_FOUND)
    }
}

/// PUT /api/oracle/aggregated/config — Update aggregator config.
async fn update_config_handler(
    State(state): State<AppState>,
    Json(body): Json<OracleAggregatorConfig>,
) -> Response {
    state.oracle_aggregator.update_config(body).await;
    ok(serde_json::json!({ "status": "config_updated" }))
}

/// Build the oracle aggregator router.
pub fn build_oracle_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/oracle/aggregated/price", get(get_price_handler))
        .route("/api/oracle/aggregated/prices", get(get_prices_handler))
        .route("/api/oracle/aggregated/refresh", post(refresh_handler))
        .route("/api/oracle/aggregated/sources", get(sources_handler))
        .route("/api/oracle/aggregated/health", get(health_handler))
        .route("/api/oracle/aggregated/sources", post(add_source_handler))
        .route(
            "/api/oracle/aggregated/sources/{name}",
            delete(remove_source_handler),
        )
        .route("/api/oracle/aggregated/config", put(update_config_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    // -- Helpers --

    fn default_test_config() -> OracleAggregatorConfig {
        OracleAggregatorConfig {
            sources: vec![
                OracleSourceConfig {
                    name: "coingecko".to_string(),
                    source_type: OracleSourceType::CoinGecko,
                    url: "https://api.coingecko.com".to_string(),
                    api_key: None,
                    weight: 1.0,
                    priority: 1,
                    enabled: true,
                    timeout_ms: 5000,
                },
                OracleSourceConfig {
                    name: "crux".to_string(),
                    source_type: OracleSourceType::CruxFinance,
                    url: "https://crux.finance".to_string(),
                    api_key: None,
                    weight: 2.0,
                    priority: 2,
                    enabled: true,
                    timeout_ms: 5000,
                },
            ],
            aggregation_strategy: AggregationStrategy::Median,
            failover_enabled: false,
            max_age_seconds: 300,
            deviation_threshold: 0.05,
            min_sources: 2,
            refresh_interval_secs: 60,
            cache_ttl_secs: 30,
        }
    }

    fn make_source_price(name: &str, price: f64, weight: f64, age_seconds: u64) -> SourcePrice {
        SourcePrice {
            source: name.to_string(),
            price,
            timestamp: Utc::now(),
            weight,
            age_seconds,
            is_stale: age_seconds > 300,
        }
    }

    // -- Config & Source Defaults --

    #[test]
    fn test_default_config_values() {
        let cfg = OracleAggregatorConfig::default();
        assert_eq!(cfg.aggregation_strategy, AggregationStrategy::Median);
        assert!(cfg.failover_enabled);
        assert_eq!(cfg.max_age_seconds, 300);
        assert!((cfg.deviation_threshold - 0.05).abs() < 1e-9);
        assert_eq!(cfg.min_sources, 2);
        assert_eq!(cfg.refresh_interval_secs, 60);
        assert_eq!(cfg.cache_ttl_secs, 30);
        assert!(cfg.sources.is_empty());
    }

    #[test]
    fn test_default_source_config() {
        let src = OracleSourceConfig::default();
        assert!(src.name.is_empty());
        assert_eq!(src.source_type, OracleSourceType::Custom);
        assert!(src.api_key.is_none());
        assert!((src.weight - 1.0).abs() < 1e-9);
        assert_eq!(src.priority, 10);
        assert!(src.enabled);
        assert_eq!(src.timeout_ms, 5000);
    }

    // -- Aggregator Construction & Source Management --

    #[test]
    fn test_new_aggregator_registers_initial_sources() {
        let cfg = default_test_config();
        let agg = OracleAggregator::new(cfg);
        assert_eq!(agg.sources.len(), 2);
        assert!(agg.sources.contains_key("coingecko"));
        assert!(agg.sources.contains_key("crux"));
    }

    #[tokio::test]
    async fn test_add_and_remove_source() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        assert_eq!(agg.sources.len(), 0);

        let src = OracleSourceConfig {
            name: "test_src".to_string(),
            source_type: OracleSourceType::Custom,
            url: "http://localhost:9999".to_string(),
            ..Default::default()
        };
        agg.add_source(src.clone()).await;
        assert_eq!(agg.sources.len(), 1);
        assert!(agg.sources.contains_key("test_src"));

        let removed = agg.remove_source("test_src");
        assert!(removed);
        assert_eq!(agg.sources.len(), 0);
    }

    #[test]
    fn test_remove_nonexistent_source() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let removed = agg.remove_source("does_not_exist");
        assert!(!removed);
    }

    // -- Source Status --

    #[tokio::test]
    async fn test_source_status_initial() {
        let agg = OracleAggregator::new(default_test_config());
        let statuses = agg.get_source_status().await;
        assert_eq!(statuses.len(), 2);

        let cg = statuses.iter().find(|s| s.name == "coingecko").unwrap();
        assert!(cg.enabled);
        assert_eq!(cg.priority, 1);
        assert_eq!(cg.fetch_count, 0);
        assert_eq!(cg.error_count, 0);
        // No fetch yet => stale
        assert!(!cg.is_healthy);
    }

    // -- Health Check --

    #[tokio::test]
    async fn test_health_with_no_healthy_sources() {
        let agg = OracleAggregator::new(default_test_config());
        let health = agg.get_health().await;
        assert!(!health.is_healthy);
        assert_eq!(health.total_sources, 2);
        assert_eq!(health.healthy_sources, 0);
        assert_eq!(health.stale_sources, 2);
        assert_eq!(health.total_fetches, 0);
        assert_eq!(health.total_errors, 0);
    }

    // -- Price Aggregation Strategies --

    #[test]
    fn test_aggregation_median_odd() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let prices = vec![
            make_source_price("a", 1.0, 1.0, 0),
            make_source_price("b", 3.0, 1.0, 0),
            make_source_price("c", 5.0, 1.0, 0),
        ];
        let cfg = OracleAggregatorConfig {
            aggregation_strategy: AggregationStrategy::Median,
            failover_enabled: false,
            ..Default::default()
        };
        let result = agg.compute_aggregation("TEST/USD", &prices, &cfg);
        assert!((result.price - 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_aggregation_median_even() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let prices = vec![
            make_source_price("a", 1.0, 1.0, 0),
            make_source_price("b", 2.0, 1.0, 0),
            make_source_price("c", 3.0, 1.0, 0),
            make_source_price("d", 4.0, 1.0, 0),
        ];
        let cfg = OracleAggregatorConfig {
            aggregation_strategy: AggregationStrategy::Median,
            failover_enabled: false,
            ..Default::default()
        };
        let result = agg.compute_aggregation("TEST/USD", &prices, &cfg);
        assert!((result.price - 2.5).abs() < 1e-9);
    }

    #[test]
    fn test_aggregation_weighted_average() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let prices = vec![
            make_source_price("a", 10.0, 1.0, 0),
            make_source_price("b", 20.0, 3.0, 0),
        ];
        let cfg = OracleAggregatorConfig {
            aggregation_strategy: AggregationStrategy::WeightedAverage,
            failover_enabled: false,
            ..Default::default()
        };
        let result = agg.compute_aggregation("TEST/USD", &prices, &cfg);
        // (10*1 + 20*3) / (1+3) = 70/4 = 17.5
        assert!((result.price - 17.5).abs() < 1e-9);
    }

    #[test]
    fn test_aggregation_mean() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let prices = vec![
            make_source_price("a", 1.0, 1.0, 0),
            make_source_price("b", 3.0, 1.0, 0),
            make_source_price("c", 5.0, 1.0, 0),
        ];
        let cfg = OracleAggregatorConfig {
            aggregation_strategy: AggregationStrategy::Mean,
            failover_enabled: false,
            ..Default::default()
        };
        let result = agg.compute_aggregation("TEST/USD", &prices, &cfg);
        assert!((result.price - 3.0).abs() < 1e-9);
    }

    #[test]
    fn test_aggregation_min() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let prices = vec![
            make_source_price("a", 5.0, 1.0, 0),
            make_source_price("b", 1.0, 1.0, 0),
            make_source_price("c", 10.0, 1.0, 0),
        ];
        let cfg = OracleAggregatorConfig {
            aggregation_strategy: AggregationStrategy::Min,
            failover_enabled: false,
            ..Default::default()
        };
        let result = agg.compute_aggregation("TEST/USD", &prices, &cfg);
        assert!((result.price - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_aggregation_max() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let prices = vec![
            make_source_price("a", 5.0, 1.0, 0),
            make_source_price("b", 1.0, 1.0, 0),
            make_source_price("c", 10.0, 1.0, 0),
        ];
        let cfg = OracleAggregatorConfig {
            aggregation_strategy: AggregationStrategy::Max,
            failover_enabled: false,
            ..Default::default()
        };
        let result = agg.compute_aggregation("TEST/USD", &prices, &cfg);
        assert!((result.price - 10.0).abs() < 1e-9);
    }

    // -- Deviation & Confidence --

    #[test]
    fn test_deviation_zero_when_identical() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let prices = vec![
            make_source_price("a", 5.0, 1.0, 0),
            make_source_price("b", 5.0, 1.0, 0),
            make_source_price("c", 5.0, 1.0, 0),
        ];
        let dev = agg.check_deviation(&prices);
        assert!(dev.abs() < 1e-9);
    }

    #[test]
    fn test_deviation_high_when_divergent() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let prices = vec![
            make_source_price("a", 1.0, 1.0, 0),
            make_source_price("b", 100.0, 1.0, 0),
        ];
        let dev = agg.check_deviation(&prices);
        assert!(dev > 0.9); // High relative deviation (CV ~0.98)
    }

    #[test]
    fn test_deviation_empty_prices() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let dev = agg.check_deviation(&[]);
        assert!(dev.abs() < 1e-9);
    }

    #[test]
    fn test_confidence_high_when_deviation_low() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let cfg = OracleAggregatorConfig::default();
        let confidence = agg.compute_confidence(&0.0, &cfg);
        assert!(confidence >= 0.99); // Nearly 1.0
    }

    #[test]
    fn test_confidence_low_when_deviation_high() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let cfg = OracleAggregatorConfig::default();
        let confidence = agg.compute_confidence(&0.5, &cfg);
        assert!(confidence < 0.5);
    }

    // -- Failover Logic --

    #[test]
    fn test_failover_selects_freshest_source() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let prices = vec![
            make_source_price("old_source", 1.0, 1.0, 300),
            make_source_price("fresh_source", 2.0, 1.0, 5),
            make_source_price("mid_source", 1.5, 1.0, 100),
        ];
        let selected = agg.failover_select(&prices).unwrap();
        assert_eq!(selected.source, "fresh_source");
    }

    #[test]
    fn test_failover_not_triggered_when_deviation_ok() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let prices = vec![
            make_source_price("a", 1.0, 1.0, 0),
            make_source_price("b", 1.01, 1.0, 0),
        ];
        let cfg = OracleAggregatorConfig {
            failover_enabled: true,
            deviation_threshold: 0.05,
            aggregation_strategy: AggregationStrategy::Median,
            ..Default::default()
        };
        let result = agg.compute_aggregation("TEST/USD", &prices, &cfg);
        // Deviation is tiny, no failover; price should be ~1.005
        assert!((result.price - 1.005).abs() < 1e-6);
    }

    #[test]
    fn test_failover_triggered_when_deviation_exceeds_threshold() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let prices = vec![
            make_source_price("old", 1.0, 1.0, 500),
            make_source_price("fresh", 100.0, 1.0, 1),
        ];
        let cfg = OracleAggregatorConfig {
            failover_enabled: true,
            deviation_threshold: 0.05,
            aggregation_strategy: AggregationStrategy::Median,
            ..Default::default()
        };
        let result = agg.compute_aggregation("TEST/USD", &prices, &cfg);
        // Failover should select the freshest source
        assert_eq!(result.price, 100.0);
    }

    // -- URL Building --

    #[test]
    fn test_build_url_coingecko() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let config = OracleSourceConfig {
            source_type: OracleSourceType::CoinGecko,
            url: "https://api.coingecko.com".to_string(),
            ..Default::default()
        };
        let url = agg.build_fetch_url(&config, "ERG/USD");
        assert!(url.contains("api/v3/simple/price"));
        assert!(url.contains("ids=erg"));
        assert!(url.contains("vs_currencies=usd"));
    }

    #[test]
    fn test_build_url_crux_finance() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let config = OracleSourceConfig {
            source_type: OracleSourceType::CruxFinance,
            url: "https://crux.finance".to_string(),
            ..Default::default()
        };
        let url = agg.build_fetch_url(&config, "ERG/USD");
        assert_eq!(url, "https://crux.finance/api/v1/price?pair=erg_usd");
    }

    #[test]
    fn test_build_url_custom() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let config = OracleSourceConfig {
            source_type: OracleSourceType::Custom,
            url: "https://my-oracle.io".to_string(),
            ..Default::default()
        };
        let url = agg.build_fetch_url(&config, "BTC/ETH");
        assert_eq!(url, "https://my-oracle.io/price?pair=btc_eth");
    }

    #[test]
    fn test_build_url_trailing_slash_trimmed() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let config = OracleSourceConfig {
            source_type: OracleSourceType::Custom,
            url: "https://my-oracle.io/".to_string(),
            ..Default::default()
        };
        let url = agg.build_fetch_url(&config, "X/Y");
        assert_eq!(url, "https://my-oracle.io/price?pair=x_y");
    }

    // -- Price Parsing --

    #[test]
    fn test_parse_coingecko_response() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let body = serde_json::json!({"ergo": {"usd": 1.23}});
        let price = agg.parse_price_from_body(&body, &OracleSourceType::CoinGecko).unwrap();
        assert!((price - 1.23).abs() < 1e-9);
    }

    #[test]
    fn test_parse_coingecko_invalid() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let body = serde_json::json!({"not_a_coin": {}});
        let result = agg.parse_price_from_body(&body, &OracleSourceType::CoinGecko);
        // The coin data exists but has no inner object with a price => Err
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_crux_response() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let body = serde_json::json!({"price": 2.50});
        let price = agg.parse_price_from_body(&body, &OracleSourceType::CruxFinance).unwrap();
        assert!((price - 2.50).abs() < 1e-9);
    }

    #[test]
    fn test_parse_ergo_oracle_pool() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let body = serde_json::json!({"value": 1230000, "decimals": 6});
        let price = agg.parse_price_from_body(&body, &OracleSourceType::ErgoOraclePool).unwrap();
        assert!((price - 1.23).abs() < 1e-9);
    }

    #[test]
    fn test_parse_ergo_oracle_pool_default_decimals() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let body = serde_json::json!({"value": 1000000000});
        let price = agg.parse_price_from_body(&body, &OracleSourceType::ErgoOraclePool).unwrap();
        // Default decimals = 9 => 1000000000 / 10^9 = 1.0
        assert!((price - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_parse_custom_price_field() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let body = serde_json::json!({"price": 42.0});
        let price = agg.parse_price_from_body(&body, &OracleSourceType::Custom).unwrap();
        assert!((price - 42.0).abs() < 1e-9);
    }

    #[test]
    fn test_parse_custom_value_field_fallback() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let body = serde_json::json!({"value": 99.5});
        let price = agg.parse_price_from_body(&body, &OracleSourceType::Custom).unwrap();
        assert!((price - 99.5).abs() < 1e-9);
    }

    #[test]
    fn test_parse_custom_missing_both_fields() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let body = serde_json::json!({"other": 1.0});
        let result = agg.parse_price_from_body(&body, &OracleSourceType::Custom);
        assert!(result.is_err());
    }

    // -- Source Price Stale Detection --

    #[test]
    fn test_source_price_stale_detection() {
        let fresh = make_source_price("fresh", 1.0, 1.0, 10);
        assert!(!fresh.is_stale);

        let stale = make_source_price("stale", 1.0, 1.0, 400);
        assert!(stale.is_stale);
    }

    // -- Get All Prices (Empty) --

    #[tokio::test]
    async fn test_get_all_prices_empty() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        let prices = agg.get_all_prices().await;
        assert!(prices.is_empty());
    }

    // -- Update Config --

    #[tokio::test]
    async fn test_update_config_syncs_sources() {
        let agg = OracleAggregator::new(OracleAggregatorConfig::default());
        assert_eq!(agg.sources.len(), 0);

        let new_cfg = OracleAggregatorConfig {
            sources: vec![OracleSourceConfig {
                name: "new_src".to_string(),
                source_type: OracleSourceType::CoinGecko,
                url: "http://localhost".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };
        agg.update_config(new_cfg).await;
        assert!(agg.sources.contains_key("new_src"));
    }

    // -- Serialization Round-Trip --

    #[test]
    fn test_aggregation_strategy_serde() {
        let strategies = vec![
            AggregationStrategy::Median,
            AggregationStrategy::WeightedAverage,
            AggregationStrategy::Mean,
            AggregationStrategy::Min,
            AggregationStrategy::Max,
        ];
        for s in strategies {
            let json = serde_json::to_string(&s).unwrap();
            let back: AggregationStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(s, back);
        }
    }

    #[test]
    fn test_oracle_source_type_serde() {
        let types = vec![
            OracleSourceType::ErgoOraclePool,
            OracleSourceType::CruxFinance,
            OracleSourceType::SpectrumDex,
            OracleSourceType::CoinGecko,
            OracleSourceType::Custom,
        ];
        for t in types {
            let json = serde_json::to_string(&t).unwrap();
            let back: OracleSourceType = serde_json::from_str(&json).unwrap();
            assert_eq!(t, back);
        }
    }

    // -- Refresh All (no-op with no fetchable sources) --

    #[tokio::test]
    async fn test_refresh_all_no_panic() {
        let agg = OracleAggregator::new(default_test_config());
        // Should not panic even though sources are unreachable
        agg.refresh_all().await;
        // Price cache should remain empty (sources will fail)
        assert_eq!(agg.price_cache.len(), 0);
    }
}
