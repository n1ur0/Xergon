//! Ergo Oracle Feeds — EIP-23 Oracle Pool Price Feeds
//!
//! Multi-source ERG price feed aggregation from Ergo oracle pools (EIP-23),
//! SpectrumDEX, ErgoMarkets, and custom endpoints. Provides median/weighted-average
//! aggregation, staleness detection, deviation alerts, and historical tracking.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Oracle data source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OracleSource {
    /// EIP-23 Ergo oracle pool (on-chain oracle boxes)
    ErgoOraclePool,
    /// SpectrumDEX decentralized exchange
    SpectrumDEX,
    /// ErgoMarkets aggregator
    ErgoMarkets,
    /// Custom user-defined endpoint
    Custom,
}

impl std::fmt::Display for OracleSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ErgoOraclePool => write!(f, "ErgoOraclePool"),
            Self::SpectrumDEX => write!(f, "SpectrumDEX"),
            Self::ErgoMarkets => write!(f, "ErgoMarkets"),
            Self::Custom => write!(f, "Custom"),
        }
    }
}

/// Trading pair for oracle feeds.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OraclePair {
    #[serde(rename = "ERG/USD")]
    ErgUsd,
    #[serde(rename = "ERG/BTC")]
    ErgBtc,
    #[serde(rename = "ERG/ETH")]
    ErgEth,
}

impl std::fmt::Display for OraclePair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ErgUsd => write!(f, "ERG/USD"),
            Self::ErgBtc => write!(f, "ERG/BTC"),
            Self::ErgEth => write!(f, "ERG/ETH"),
        }
    }
}

/// Price aggregation method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AggregationMethod {
    #[serde(rename = "median")]
    Median,
    #[serde(rename = "weighted_average")]
    WeightedAverage,
    #[serde(rename = "mean")]
    Mean,
}

impl std::fmt::Display for AggregationMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Median => write!(f, "median"),
            Self::WeightedAverage => write!(f, "weighted_average"),
            Self::Mean => write!(f, "mean"),
        }
    }
}

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// A single oracle feed data point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleFeed {
    /// Source of this price feed.
    pub source: OracleSource,
    /// Trading pair.
    pub pair: OraclePair,
    /// Price in quote currency per 1 ERG (e.g., 0.45 USD).
    pub price: f64,
    /// Timestamp when this feed was recorded.
    pub timestamp: i64,
    /// Oracle epoch (EIP-23 epoch counter).
    pub epoch: i64,
    /// Confidence score 0.0-1.0.
    pub confidence: f64,
    /// On-chain box ID (if from oracle pool).
    pub box_id: String,
}

impl OracleFeed {
    /// Create a new oracle feed.
    pub fn new(
        source: OracleSource,
        pair: OraclePair,
        price: f64,
        epoch: i64,
        confidence: f64,
        box_id: impl Into<String>,
    ) -> Self {
        Self {
            source,
            pair,
            price,
            timestamp: Utc::now().timestamp(),
            epoch,
            confidence,
            box_id: box_id.into(),
        }
    }

    /// Check if this feed is fresh within the given max age.
    pub fn is_fresh(&self, max_age_secs: i64) -> bool {
        let now = Utc::now().timestamp();
        (now - self.timestamp) < max_age_secs
    }
}

/// Configuration for the oracle service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleFeedConfig {
    /// Poll interval in seconds.
    pub poll_interval_secs: u64,
    /// Maximum feed age in seconds before considered stale.
    pub max_age_secs: i64,
    /// Minimum number of sources required for aggregation.
    pub min_sources: usize,
    /// Staleness threshold in seconds for alerts.
    pub staleness_threshold_secs: i64,
}

impl Default for OracleFeedConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: 60,
            max_age_secs: 300,
            min_sources: 2,
            staleness_threshold_secs: 600,
        }
    }
}

/// Aggregated price from multiple sources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceAggregation {
    /// Aggregation method used.
    pub method: AggregationMethod,
    /// Trading pair.
    pub pair: OraclePair,
    /// Sources used in aggregation.
    pub sources: Vec<OracleFeed>,
    /// Aggregated price.
    pub aggregated_price: f64,
    /// Standard deviation among sources.
    pub deviation: f64,
    /// Number of sources.
    pub source_count: usize,
    /// Timestamp of aggregation.
    pub timestamp: i64,
}

impl PriceAggregation {
    /// Aggregate a list of feeds using the specified method.
    pub fn aggregate(method: AggregationMethod, pair: OraclePair, feeds: Vec<OracleFeed>) -> Option<Self> {
        if feeds.is_empty() {
            return None;
        }

        let prices: Vec<f64> = feeds.iter().map(|f| f.price).collect();
        let aggregated_price = match method {
            AggregationMethod::Median => {
                let mut sorted = prices.clone();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                if sorted.len() % 2 == 0 {
                    (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2.0
                } else {
                    sorted[sorted.len() / 2]
                }
            }
            AggregationMethod::WeightedAverage => {
                let total_confidence: f64 = feeds.iter().map(|f| f.confidence).sum();
                if total_confidence == 0.0 {
                    prices.iter().sum::<f64>() / prices.len() as f64
                } else {
                    feeds.iter().map(|f| f.price * f.confidence).sum::<f64>() / total_confidence
                }
            }
            AggregationMethod::Mean => {
                prices.iter().sum::<f64>() / prices.len() as f64
            }
        };

        // Calculate standard deviation
        let mean = prices.iter().sum::<f64>() / prices.len() as f64;
        let variance = prices.iter().map(|p| (p - mean).powi(2)).sum::<f64>() / prices.len() as f64;
        let deviation = variance.sqrt();

        Some(Self {
            method,
            pair,
            sources: feeds,
            aggregated_price,
            deviation,
            source_count: prices.len(),
            timestamp: Utc::now().timestamp(),
        })
    }
}

/// Statistics for the oracle service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleStats {
    /// Total number of feeds fetched.
    pub total_feeds: u64,
    /// Number of active sources.
    pub active_sources: usize,
    /// Number of stale sources.
    pub stale_sources: usize,
    /// Number of deviation alerts triggered.
    pub deviation_alerts: u64,
    /// Last poll timestamp.
    pub last_poll: i64,
    /// Number of feeds by pair.
    pub feeds_by_pair: HashMap<String, u64>,
}

/// Historical feed entry for tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedHistoryEntry {
    pub feed: OracleFeed,
    pub recorded_at: i64,
}

// ---------------------------------------------------------------------------
// Registered source info
// ---------------------------------------------------------------------------

/// A registered oracle source with its configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredSource {
    /// Source type.
    pub source: OracleSource,
    /// Human-readable name.
    pub name: String,
    /// Endpoint URL (for off-chain sources).
    pub endpoint_url: String,
    /// Whether this source is enabled.
    pub enabled: bool,
    /// Weight for weighted-average aggregation (higher = more trusted).
    pub weight: f64,
    /// Number of successful fetches.
    pub success_count: u64,
    /// Number of failed fetches.
    pub failure_count: u64,
}

impl RegisteredSource {
    /// Create a new registered source.
    pub fn new(
        source: OracleSource,
        name: impl Into<String>,
        endpoint_url: impl Into<String>,
        weight: f64,
    ) -> Self {
        Self {
            source,
            name: name.into(),
            endpoint_url: endpoint_url.into(),
            enabled: true,
            weight,
            success_count: 0,
            failure_count: 0,
        }
    }

    /// Get the effective confidence based on weight and success rate.
    pub fn effective_confidence(&self) -> f64 {
        let total = self.success_count + self.failure_count;
        if total == 0 {
            self.weight
        } else {
            self.weight * (self.success_count as f64 / total as f64)
        }
    }
}

// ---------------------------------------------------------------------------
// ErgoOracleService
// ---------------------------------------------------------------------------

/// Multi-source ERG oracle service backed by DashMap.
///
/// Provides EIP-23 oracle pool price feeds with aggregation, staleness
/// detection, and deviation alerts.
pub struct ErgoOracleService {
    /// Current feeds keyed by (source, pair).
    feeds: DashMap<(OracleSource, OraclePair), OracleFeed>,
    /// Historical feeds for tracking.
    history: DashMap<OraclePair, Vec<FeedHistoryEntry>>,
    /// Registered sources.
    sources: DashMap<OracleSource, RegisteredSource>,
    /// Service configuration.
    config: Arc<std::sync::RwLock<OracleFeedConfig>>,
    /// Last poll time.
    last_poll: Arc<AtomicI64>,
    /// Total feeds fetched counter.
    total_feeds: Arc<std::sync::atomic::AtomicU64>,
    /// Deviation alerts counter.
    deviation_alerts: Arc<std::sync::atomic::AtomicU64>,
    /// Maximum deviation threshold before alert (percentage).
    deviation_threshold_pct: Arc<std::sync::atomic::AtomicU64>,
    /// Poll start instant for interval tracking.
    last_poll_instant: Arc<std::sync::Mutex<Instant>>,
}

impl ErgoOracleService {
    /// Create a new oracle service with default configuration.
    pub fn new() -> Self {
        Self::with_config(OracleFeedConfig::default())
    }

    /// Create a new oracle service with custom configuration.
    pub fn with_config(config: OracleFeedConfig) -> Self {
        let mut svc = Self {
            feeds: DashMap::new(),
            history: DashMap::new(),
            sources: DashMap::new(),
            config: Arc::new(std::sync::RwLock::new(config)),
            last_poll: Arc::new(AtomicI64::new(0)),
            total_feeds: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            deviation_alerts: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            deviation_threshold_pct: Arc::new(std::sync::atomic::AtomicU64::new(10)), // 10%
            last_poll_instant: Arc::new(std::sync::Mutex::new(Instant::now())),
        };

        // Register default sources
        svc.register_source(RegisteredSource::new(
            OracleSource::ErgoOraclePool,
            "Ergo Oracle Pool (EIP-23)",
            "",
            1.0,
        ));
        svc.register_source(RegisteredSource::new(
            OracleSource::SpectrumDEX,
            "SpectrumDEX",
            "https://api.spectrum.fi/v1",
            0.8,
        ));
        svc.register_source(RegisteredSource::new(
            OracleSource::ErgoMarkets,
            "ErgoMarkets",
            "https://ergomarkets.com/api",
            0.7,
        ));

        svc
    }

    // ----- Feed management -----

    /// Fetch and store a price from a source (simulated for offline operation).
    /// In production, this would make HTTP requests to oracle endpoints.
    pub async fn fetch_price(
        &self,
        source: OracleSource,
        pair: OraclePair,
        price: f64,
        epoch: i64,
        confidence: f64,
        box_id: impl Into<String>,
    ) -> OracleFeed {
        let pair_key = pair.clone();
        let source_key = source.clone();
        let feed = OracleFeed::new(source, pair, price, epoch, confidence, box_id);
        self.feeds.insert((source_key, pair_key.clone()), feed.clone());

        // Track history
        {
            let mut hist = self.history.entry(pair_key.clone()).or_default();
            hist.push(FeedHistoryEntry {
                feed: feed.clone(),
                recorded_at: Utc::now().timestamp(),
            });
            // Keep last 1000 entries per pair
            if hist.len() > 1000 {
                let drain_count = hist.len() - 1000;
                hist.drain(..drain_count);
            }
        }

        // Update counters
        self.total_feeds.fetch_add(1, Ordering::Relaxed);
        self.last_poll.store(Utc::now().timestamp(), Ordering::Relaxed);

        // Check deviation
        self.check_deviation(&pair_key, price);

        debug!(
            source = %feed.source,
            pair = %feed.pair,
            price = price,
            "Oracle feed stored"
        );

        feed
    }

    /// Get the latest price for a specific source and pair.
    pub fn get_price(&self, source: OracleSource, pair: &OraclePair) -> Option<OracleFeed> {
        self.feeds.get(&(source, pair.clone())).map(|r| r.value().clone())
    }

    /// Get all current prices for a pair across all sources.
    pub fn get_all_prices(&self, pair: &OraclePair) -> Vec<OracleFeed> {
        self.feeds
            .iter()
            .filter(|entry| &entry.key().1 == pair)
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Aggregate prices from all sources for a pair.
    pub fn aggregate(
        &self,
        pair: &OraclePair,
        method: AggregationMethod,
    ) -> Option<PriceAggregation> {
        let config = self.config.read().unwrap();
        let feeds = self.get_all_prices(pair);

        if feeds.len() < config.min_sources {
            debug!(
                pair = %pair,
                available = feeds.len(),
                required = config.min_sources,
                "Not enough sources for aggregation"
            );
            return None;
        }

        PriceAggregation::aggregate(method, pair.clone(), feeds)
    }

    /// Register a new oracle source.
    pub fn register_source(&self, source: RegisteredSource) {
        let src_key = source.source.clone();
        let src_name = source.name.clone();
        self.sources.insert(source.source, source);
        info!(source = %src_key, name = %src_name, "Oracle source registered");
    }

    /// Unregister an oracle source.
    pub fn unregister_source(&self, source: OracleSource) {
        let src_name = format!("{:?}", source);
        if self.sources.remove(&source).is_some() {
            // Clean up feeds from this source
            let src = source;
            self.feeds.retain(|k, _| k.0 != src);
            info!(source = %src_name, "Oracle source unregistered");
        }
    }

    /// Get feed history for a pair.
    pub fn get_feed_history(&self, pair: &OraclePair, limit: usize) -> Vec<FeedHistoryEntry> {
        self.history
            .get(pair)
            .map(|h| {
                let entries: Vec<_> = h.value().iter().rev().take(limit).cloned().collect();
                entries
            })
            .unwrap_or_default()
    }

    /// Get oracle service statistics.
    pub fn get_stats(&self) -> OracleStats {
        let config = self.config.read().unwrap();
        let now = Utc::now().timestamp();
        let max_age = config.max_age_secs;

        let mut active_sources = 0usize;
        let mut stale_sources = 0usize;
        let mut feeds_by_pair: HashMap<String, u64> = HashMap::new();

        for entry in self.feeds.iter() {
            let feed = entry.value();
            if feed.is_fresh(max_age) {
                active_sources += 1;
            } else {
                stale_sources += 1;
            }
            *feeds_by_pair.entry(feed.pair.to_string()).or_insert(0) += 1;
        }

        OracleStats {
            total_feeds: self.total_feeds.load(Ordering::Relaxed),
            active_sources,
            stale_sources,
            deviation_alerts: self.deviation_alerts.load(Ordering::Relaxed),
            last_poll: self.last_poll.load(Ordering::Relaxed),
            feeds_by_pair,
        }
    }

    /// Update oracle configuration.
    pub fn update_config(&self, new_config: OracleFeedConfig) {
        let mut config = self.config.write().unwrap();
        *config = new_config;
        info!("Oracle configuration updated");
    }

    /// Get current configuration.
    pub fn get_config(&self) -> OracleFeedConfig {
        self.config.read().unwrap().clone()
    }

    /// Check if all feeds are fresh.
    pub fn check_freshness(&self) -> bool {
        let config = self.config.read().unwrap();
        let now = Utc::now().timestamp();

        for entry in self.feeds.iter() {
            let feed = entry.value();
            if (now - feed.timestamp) > config.staleness_threshold_secs {
                return false;
            }
        }
        true
    }

    /// Get all registered sources.
    pub fn get_sources(&self) -> Vec<RegisteredSource> {
        self.sources.iter().map(|r| r.value().clone()).collect()
    }

    /// Get all registered pairs that have feeds.
    pub fn get_active_pairs(&self) -> Vec<OraclePair> {
        self.feeds
            .iter()
            .map(|e| e.key().1.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect()
    }

    /// Set the deviation threshold percentage.
    pub fn set_deviation_threshold_pct(&self, pct: u64) {
        self.deviation_threshold_pct.store(pct, Ordering::Relaxed);
    }

    /// Check price deviation and trigger alert if needed.
    fn check_deviation(&self, pair: &OraclePair, new_price: f64) {
        let existing: Vec<f64> = self
            .feeds
            .iter()
            .filter(|e| &e.key().1 == pair)
            .map(|e| e.value().price)
            .collect();

        if existing.len() < 2 {
            return;
        }

        let mean: f64 = existing.iter().sum::<f64>() / existing.len() as f64;
        if mean == 0.0 {
            return;
        }

        let deviation_pct = ((new_price - mean).abs() / mean) * 100.0;
        let threshold = self.deviation_threshold_pct.load(Ordering::Relaxed) as f64;

        if deviation_pct > threshold {
            self.deviation_alerts.fetch_add(1, Ordering::Relaxed);
            warn!(
                pair = %pair,
                new_price = new_price,
                mean_price = mean,
                deviation_pct = deviation_pct,
                threshold = threshold,
                "Price deviation alert triggered"
            );
        }
    }

    /// Remove stale feeds older than max_age_secs.
    pub fn cleanup_stale(&self) -> usize {
        let config = self.config.read().unwrap();
        let now = Utc::now().timestamp();
        let max_age = config.max_age_secs;

        let mut removed = 0;
        self.feeds.retain(|_, feed| {
            let fresh = (now - feed.timestamp) < max_age;
            if !fresh {
                removed += 1;
            }
            fresh
        });

        if removed > 0 {
            info!(removed = removed, "Cleaned up stale oracle feeds");
        }
        removed
    }

    /// Get the best (most recent) price for a pair from any source.
    pub fn get_best_price(&self, pair: &OraclePair) -> Option<OracleFeed> {
        let mut best: Option<OracleFeed> = None;
        for entry in self.feeds.iter() {
            if &entry.key().1 == pair {
                match &best {
                    None => best = Some(entry.value().clone()),
                    Some(current) if entry.value().timestamp > current.timestamp => {
                        best = Some(entry.value().clone())
                    }
                    _ => {}
                }
            }
        }
        best
    }

    /// Simulate fetching from an oracle pool (for testing / offline mode).
    pub async fn simulate_oracle_pool_fetch(
        &self,
        pair: OraclePair,
        base_price: f64,
        epoch: i64,
    ) -> OracleFeed {
        // Simulate slight price variation
        let variation = (rand::random::<f64>() - 0.5) * 0.02; // +/- 1%
        let price = base_price * (1.0 + variation);
        let box_id = format!(
            "{:x}",
            rand::random::<u64>()
        );

        self.fetch_price(
            OracleSource::ErgoOraclePool,
            pair,
            price,
            epoch,
            0.95,
            box_id,
        )
        .await
    }
}

// ---------------------------------------------------------------------------
// REST API router builder
// ---------------------------------------------------------------------------

/// Build the oracle feeds router.
pub fn build_oracle_feeds_router(state: crate::api::AppState) -> axum::Router<()> {
    use axum::routing::get;

    axum::Router::new()
        .route("/v1/oracle/price/{pair}", get(oracle_price_handler))
        .route("/v1/oracle/prices", get(oracle_prices_handler))
        .route("/v1/oracle/aggregated/{pair}", get(oracle_aggregated_handler))
        .route("/v1/oracle/history/{pair}", get(oracle_history_handler))
        .route("/v1/oracle/sources", get(oracle_sources_handler))
        .route("/v1/oracle/config", get(oracle_config_handler))
        .with_state(state)
}

// ----- Handlers -----

async fn oracle_price_handler(
    axum::extract::State(state): axum::extract::State<crate::api::AppState>,
    axum::extract::Path(pair_str): axum::extract::Path<String>,
) -> axum::response::Response {
    let pair = match pair_str.as_str() {
        "ERG/USD" => OraclePair::ErgUsd,
        "ERG/BTC" => OraclePair::ErgBtc,
        "ERG/ETH" => OraclePair::ErgEth,
        _ => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({
                    "error": format!("Unknown pair: {}", pair_str),
                    "valid_pairs": ["ERG/USD", "ERG/BTC", "ERG/ETH"]
                })),
            )
                .into_response();
        }
    };

    let feeds = state.oracle_feeds.get_all_prices(&pair);
    if feeds.is_empty() {
        return (
            axum::http::StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({
                "error": "No feeds available for pair",
                "pair": pair.to_string()
            })),
        )
            .into_response();
    }

    use axum::response::IntoResponse;
    axum::Json(serde_json::json!({
        "pair": pair.to_string(),
        "feeds": feeds,
        "count": feeds.len()
    }))
    .into_response()
}

async fn oracle_prices_handler(
    axum::extract::State(state): axum::extract::State<crate::api::AppState>,
) -> axum::Json<serde_json::Value> {
    let pairs = state.oracle_feeds.get_active_pairs();
    let mut all_prices = serde_json::Map::new();

    for pair in &pairs {
        let feeds = state.oracle_feeds.get_all_prices(pair);
        if !feeds.is_empty() {
            all_prices.insert(
                pair.to_string(),
                serde_json::json!({
                    "feeds": feeds,
                    "count": feeds.len()
                }),
            );
        }
    }

    axum::Json(serde_json::json!({
        "pairs": all_prices,
        "total_pairs": all_prices.len()
    }))
}

async fn oracle_aggregated_handler(
    axum::extract::State(state): axum::extract::State<crate::api::AppState>,
    axum::extract::Path(pair_str): axum::extract::Path<String>,
) -> axum::response::Response {
    let pair = match pair_str.as_str() {
        "ERG/USD" => OraclePair::ErgUsd,
        "ERG/BTC" => OraclePair::ErgBtc,
        "ERG/ETH" => OraclePair::ErgEth,
        _ => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({
                    "error": format!("Unknown pair: {}", pair_str)
                })),
            )
                .into_response();
        }
    };

    let config = state.oracle_feeds.get_config();
    let agg = state.oracle_feeds.aggregate(&pair, AggregationMethod::Median);

    use axum::response::IntoResponse;
    match agg {
        Some(agg) => axum::Json(serde_json::json!({
            "pair": agg.pair.to_string(),
            "method": agg.method.to_string(),
            "aggregated_price": agg.aggregated_price,
            "deviation": agg.deviation,
            "source_count": agg.source_count,
            "timestamp": agg.timestamp,
            "min_sources": config.min_sources
        }))
        .into_response(),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({
                "error": "Not enough sources for aggregation",
                "pair": pair.to_string(),
                "min_sources": config.min_sources
            })),
        )
            .into_response(),
    }
}

async fn oracle_history_handler(
    axum::extract::State(state): axum::extract::State<crate::api::AppState>,
    axum::extract::Path(pair_str): axum::extract::Path<String>,
) -> axum::response::Response {
    let pair = match pair_str.as_str() {
        "ERG/USD" => OraclePair::ErgUsd,
        "ERG/BTC" => OraclePair::ErgBtc,
        "ERG/ETH" => OraclePair::ErgEth,
        _ => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({
                    "error": format!("Unknown pair: {}", pair_str)
                })),
            )
                .into_response();
        }
    };

    let history = state.oracle_feeds.get_feed_history(&pair, 100);
    use axum::response::IntoResponse;
    axum::Json(serde_json::json!({
        "pair": pair.to_string(),
        "entries": history,
        "count": history.len()
    }))
    .into_response()
}

async fn oracle_sources_handler(
    axum::extract::State(state): axum::extract::State<crate::api::AppState>,
) -> axum::Json<serde_json::Value> {
    let sources = state.oracle_feeds.get_sources();
    axum::Json(serde_json::json!({
        "sources": sources,
        "count": sources.len()
    }))
}

async fn oracle_config_handler(
    axum::extract::State(state): axum::extract::State<crate::api::AppState>,
) -> axum::Json<serde_json::Value> {
    let config = state.oracle_feeds.get_config();
    axum::Json(serde_json::json!(config))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_service() -> ErgoOracleService {
        ErgoOracleService::with_config(OracleFeedConfig {
            poll_interval_secs: 60,
            max_age_secs: 300,
            min_sources: 2,
            staleness_threshold_secs: 600,
        })
    }

    #[tokio::test]
    async fn test_new_service() {
        let svc = make_service();
        let sources = svc.get_sources();
        assert_eq!(sources.len(), 3); // Default 3 sources
    }

    #[tokio::test]
    async fn test_fetch_and_get_price() {
        let svc = make_service();
        svc.fetch_price(
            OracleSource::ErgoOraclePool,
            OraclePair::ErgUsd,
            0.45,
            100,
            0.95,
            "box123",
        )
        .await;

        let feed = svc.get_price(OracleSource::ErgoOraclePool, &OraclePair::ErgUsd);
        assert!(feed.is_some());
        let feed = feed.unwrap();
        assert_eq!(feed.price, 0.45);
        assert_eq!(feed.epoch, 100);
    }

    #[tokio::test]
    async fn test_get_all_prices() {
        let svc = make_service();
        svc.fetch_price(OracleSource::ErgoOraclePool, OraclePair::ErgUsd, 0.45, 100, 0.95, "b1")
            .await;
        svc.fetch_price(OracleSource::SpectrumDEX, OraclePair::ErgUsd, 0.46, 100, 0.85, "b2")
            .await;

        let feeds = svc.get_all_prices(&OraclePair::ErgUsd);
        assert_eq!(feeds.len(), 2);
    }

    #[tokio::test]
    async fn test_aggregate_median() {
        let svc = make_service();
        svc.fetch_price(OracleSource::ErgoOraclePool, OraclePair::ErgUsd, 0.40, 100, 0.9, "b1")
            .await;
        svc.fetch_price(OracleSource::SpectrumDEX, OraclePair::ErgUsd, 0.50, 100, 0.9, "b2")
            .await;
        svc.fetch_price(OracleSource::ErgoMarkets, OraclePair::ErgUsd, 0.45, 100, 0.9, "b3")
            .await;

        let agg = svc.aggregate(&OraclePair::ErgUsd, AggregationMethod::Median);
        assert!(agg.is_some());
        let agg = agg.unwrap();
        assert!((agg.aggregated_price - 0.45).abs() < 0.001);
        assert_eq!(agg.source_count, 3);
    }

    #[tokio::test]
    async fn test_aggregate_mean() {
        let svc = make_service();
        svc.fetch_price(OracleSource::ErgoOraclePool, OraclePair::ErgUsd, 0.40, 100, 0.9, "b1")
            .await;
        svc.fetch_price(OracleSource::SpectrumDEX, OraclePair::ErgUsd, 0.50, 100, 0.9, "b2")
            .await;

        let agg = svc.aggregate(&OraclePair::ErgUsd, AggregationMethod::Mean);
        assert!(agg.is_some());
        let agg = agg.unwrap();
        assert!((agg.aggregated_price - 0.45).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_aggregate_weighted_average() {
        let svc = make_service();
        svc.fetch_price(OracleSource::ErgoOraclePool, OraclePair::ErgUsd, 0.40, 100, 1.0, "b1")
            .await;
        svc.fetch_price(OracleSource::SpectrumDEX, OraclePair::ErgUsd, 0.60, 100, 0.0, "b2")
            .await;

        let agg = svc.aggregate(&OraclePair::ErgUsd, AggregationMethod::WeightedAverage);
        assert!(agg.is_some());
        let agg = agg.unwrap();
        // Weight 1.0 * 0.40 / (1.0 + 0.0) = 0.40
        assert!((agg.aggregated_price - 0.40).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_aggregate_insufficient_sources() {
        let svc = make_service();
        svc.fetch_price(OracleSource::ErgoOraclePool, OraclePair::ErgUsd, 0.45, 100, 0.9, "b1")
            .await;

        let agg = svc.aggregate(&OraclePair::ErgUsd, AggregationMethod::Median);
        assert!(agg.is_none());
    }

    #[tokio::test]
    async fn test_register_and_unregister_source() {
        let svc = make_service();
        svc.register_source(RegisteredSource::new(
            OracleSource::Custom,
            "My Oracle",
            "https://example.com/oracle",
            0.5,
        ));

        assert_eq!(svc.get_sources().len(), 4);
        svc.unregister_source(OracleSource::Custom);
        assert_eq!(svc.get_sources().len(), 3);
    }

    #[tokio::test]
    async fn test_feed_history() {
        let svc = make_service();
        for i in 0..5 {
            svc.fetch_price(
                OracleSource::ErgoOraclePool,
                OraclePair::ErgUsd,
                0.40 + i as f64 * 0.01,
                100 + i,
                0.95,
                format!("box{}", i),
            )
            .await;
        }

        let history = svc.get_feed_history(&OraclePair::ErgUsd, 3);
        assert_eq!(history.len(), 3);
    }

    #[tokio::test]
    async fn test_stats() {
        let svc = make_service();
        svc.fetch_price(OracleSource::ErgoOraclePool, OraclePair::ErgUsd, 0.45, 100, 0.9, "b1")
            .await;
        svc.fetch_price(OracleSource::SpectrumDEX, OraclePair::ErgBtc, 0.0005, 100, 0.9, "b2")
            .await;

        let stats = svc.get_stats();
        assert_eq!(stats.total_feeds, 2);
        assert_eq!(stats.active_sources, 2);
        assert!(stats.feeds_by_pair.contains_key("ERG/USD"));
        assert!(stats.feeds_by_pair.contains_key("ERG/BTC"));
    }

    #[tokio::test]
    async fn test_update_config() {
        let svc = make_service();
        svc.update_config(OracleFeedConfig {
            poll_interval_secs: 30,
            max_age_secs: 120,
            min_sources: 3,
            staleness_threshold_secs: 300,
        });

        let config = svc.get_config();
        assert_eq!(config.poll_interval_secs, 30);
        assert_eq!(config.min_sources, 3);
    }

    #[tokio::test]
    async fn test_cleanup_stale() {
        let svc = make_service();
        svc.update_config(OracleFeedConfig {
            poll_interval_secs: 60,
            max_age_secs: 0, // Immediately stale
            min_sources: 2,
            staleness_threshold_secs: 600,
        });

        svc.fetch_price(OracleSource::ErgoOraclePool, OraclePair::ErgUsd, 0.45, 100, 0.9, "b1")
            .await;

        // Small sleep to ensure timestamp differs
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let removed = svc.cleanup_stale();
        assert!(removed > 0);
    }

    #[test]
    fn test_price_aggregation_empty() {
        let result = PriceAggregation::aggregate(
            AggregationMethod::Median,
            OraclePair::ErgUsd,
            vec![],
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_price_aggregation_deviation() {
        let feeds = vec![
            OracleFeed::new(OracleSource::ErgoOraclePool, OraclePair::ErgUsd, 0.40, 100, 0.9, "b1"),
            OracleFeed::new(OracleSource::SpectrumDEX, OraclePair::ErgUsd, 0.60, 100, 0.9, "b2"),
        ];
        let agg = PriceAggregation::aggregate(AggregationMethod::Median, OraclePair::ErgUsd, feeds).unwrap();
        assert!((agg.deviation - 0.1).abs() < 0.01);
    }
}
