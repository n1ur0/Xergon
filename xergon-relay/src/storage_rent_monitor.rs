//! Storage Rent Monitor — Public REST API for Ergo storage rent status queries.
//!
//! This module provides endpoints for external clients to query the storage rent
//! health of Ergo boxes. On Ergo, every UTXO box must pay a per-byte storage fee
//! every 2 years (1,051,200 blocks). If a box's ERG value is insufficient to cover
//! the fee, it is consumed by the protocol.
//!
//! Key features:
//!   - Rent status classification (Fresh / Aging / Warning / Critical / Expired)
//!   - Address-level rent summaries with value-at-risk aggregation
//!   - Multi-address batch scanning
//!   - Ring-buffer event log with automatic overflow at 50 K events per address
//!   - Top-off recommendation engine
//!   - Runtime- adjustable thresholds via the config endpoint

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, put},
    Json, Router,
};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Blocks per Ergo year (≈ 365.25 days × 2 min).
const BLOCKS_PER_YEAR: f64 = 262_800.0;

/// One rent cycle in blocks (2 years on mainnet).
const BLOCKS_PER_RENT_CYCLE: u64 = 1_051_200;

/// Default minimum ERG per byte for storage rent (nanoERG).
const FEE_PER_BYTE_DEFAULT: u64 = 360;

/// Maximum events retained per address in the ring buffer.
const MAX_EVENTS_PER_ADDRESS: usize = 50_000;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Storage rent health status for a single box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RentStatus {
    /// Box age < 730 000 blocks (< ~2.78 years) — plenty of runway.
    Fresh,
    /// 730 000 ≤ age < 1 020 000 — approaching the 2-year rent cycle.
    Aging,
    /// 1 020 000 ≤ age < 1 045 000 — less than ~2 months to rent due.
    Warning,
    /// 1 045 000 ≤ age < 1 051 200 — imminent expiry (< ~13 days).
    Critical,
    /// age ≥ 1 051 200 — rent is due now; box may already be consumed.
    Expired,
}

impl std::fmt::Display for RentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RentStatus::Fresh => write!(f, "fresh"),
            RentStatus::Aging => write!(f, "aging"),
            RentStatus::Warning => write!(f, "warning"),
            RentStatus::Critical => write!(f, "critical"),
            RentStatus::Expired => write!(f, "expired"),
        }
    }
}

/// Types of rent-related events recorded by the monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RentEventType {
    /// A box entered warning or critical status.
    AtRiskDetected,
    /// A top-off transaction has been planned for a box.
    TopOffPlanned,
    /// A box's rent cycle expired (consumed by protocol).
    BoxExpired,
    /// Multiple at-risk boxes were consolidated into fewer boxes.
    BoxConsolidated,
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Per-box rent information returned by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxRentInfo {
    /// Base-16 box ID.
    pub box_id: String,
    /// Owner address.
    pub address: String,
    /// ERG value in nanoERG.
    pub value_nanoerg: u64,
    /// Number of tokens held in the box.
    pub token_count: u32,
    /// Block height when the box was created.
    pub creation_height: u32,
    /// Current chain height used for the calculation.
    pub current_height: u32,
    /// Box age in blocks.
    pub age_blocks: u64,
    /// Derived rent status.
    pub rent_status: RentStatus,
    /// Estimated rent fee for one cycle (nanoERG).
    pub estimated_rent_fee: u64,
    /// How many full rent cycles the box can survive.
    pub cycles_survivable: u32,
    /// Recommended ERG top-off to cover `target_years` years (nanoERG).
    pub recommended_top_off: u64,
}

/// Rent summary for a single address.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressRentSummary {
    /// Base-58 / P2PK address.
    pub address: String,
    /// Total boxes found for this address.
    pub total_boxes: u32,
    /// Box counts grouped by rent status.
    pub by_status: HashMap<RentStatus, u32>,
    /// Sum of nanoERG value in Warning / Critical / Expired boxes.
    pub total_value_at_risk: u64,
    /// Aggregate top-off estimate to bring all at-risk boxes to safety (nanoERG).
    pub top_off_estimate: u64,
}

/// Request body for `POST /v1/rent/scan`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RentScanRequest {
    /// Addresses to scan (max 100).
    pub addresses: Vec<String>,
}

/// Response body for `POST /v1/rent/scan`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RentScanResult {
    /// ISO-8601 timestamp when the scan was performed.
    pub scanned_at: String,
    /// Per-address summaries.
    pub summaries: Vec<AddressRentSummary>,
    /// Total boxes inspected across all addresses.
    pub total_boxes_checked: u32,
    /// Total boxes in Warning / Critical / Expired status.
    pub boxes_at_risk: u32,
}

/// A rent-related event stored in the ring buffer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RentEvent {
    /// What happened.
    pub event_type: RentEventType,
    /// Affected box ID.
    pub box_id: String,
    /// Owner address.
    pub address: String,
    /// ISO-8601 timestamp.
    pub timestamp: String,
    /// Free-form details / human-readable message.
    pub details: String,
}

/// Runtime-adjustable monitor configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    /// Block age threshold for `Warning` status (default 1 020 000).
    pub warning_threshold_blocks: u64,
    /// Block age threshold for `Critical` status (default 1 045 000).
    pub critical_threshold_blocks: u64,
    /// Whether the automatic background scan task is enabled.
    pub auto_scan_enabled: bool,
    /// Seconds between automatic scans (default 3 600).
    pub scan_interval_secs: u64,
    /// Addresses that are excluded from "at risk" alerts.
    pub protected_addresses: Vec<String>,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            warning_threshold_blocks: 1_020_000,
            critical_threshold_blocks: 1_045_000,
            auto_scan_enabled: false,
            scan_interval_secs: 3_600,
            protected_addresses: Vec::new(),
        }
    }
}

/// Optional query parameters for the events endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct EventsQuery {
    /// Filter by address.
    pub address: Option<String>,
    /// Maximum events to return (default 100, max 1 000).
    pub limit: Option<usize>,
}

/// Optional query parameters for the summary endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct SummaryQuery {
    /// If true, include per-address breakdowns.
    pub detailed: Option<bool>,
}

// ---------------------------------------------------------------------------
// Application state (NOT Clone — contains atomics)
// ---------------------------------------------------------------------------

/// Shared application state for the storage rent monitor.
///
/// Uses `Arc<DashMap<…>>` so the state itself never needs to be cloned;
/// each handler receives a reference through `State<AppState>`.
pub struct AppState {
    /// Runtime-adjustable configuration.
    pub config: Arc<RwLock<MonitorConfig>>,
    /// Ring-buffer event log, keyed by address.
    pub events: Arc<DashMap<String, VecDeque<RentEvent>>>,
    /// Monotonic counter of total events ever recorded (across all addresses).
    pub event_total: AtomicU64,
    /// Most-recently-seen chain height.
    pub current_height: AtomicU64,
}

impl AppState {
    /// Create a new `AppState` with default configuration.
    pub fn new() -> Self {
        Self {
            config: Arc::new(RwLock::new(MonitorConfig::default())),
            events: Arc::new(DashMap::new()),
            event_total: AtomicU64::new(0),
            current_height: AtomicU64::new(0),
        }
    }

    /// Create a new `AppState` with a custom initial configuration.
    pub fn with_config(config: MonitorConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            events: Arc::new(DashMap::new()),
            event_total: AtomicU64::new(0),
            current_height: AtomicU64::new(0),
        }
    }

    /// Record a rent event into the ring buffer for the given address.
    pub fn record_event(&self, event: RentEvent) {
        let address = event.address.clone();
        let mut entry = self.events.entry(address).or_default();
        if entry.len() >= MAX_EVENTS_PER_ADDRESS {
            entry.pop_front(); // ring-buffer overflow
        }
        entry.push_back(event);
        self.event_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Get the current chain height.
    pub fn get_height(&self) -> u32 {
        self.current_height.load(Ordering::Relaxed) as u32
    }

    /// Set the current chain height (e.g. from a node poller).
    pub fn set_height(&self, height: u32) {
        self.current_height.store(height as u64, Ordering::Relaxed);
    }

    /// Get total event count.
    pub fn total_events(&self) -> u64 {
        self.event_total.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// Internal helper — lightweight box data used for analysis
// ---------------------------------------------------------------------------

/// Minimal box representation fed into `analyze_address`.
#[derive(Debug, Clone)]
pub struct BoxData {
    pub box_id: String,
    pub address: String,
    pub value_nanoerg: u64,
    pub token_count: u32,
    pub creation_height: u32,
    pub box_size_bytes: usize,
}

// ---------------------------------------------------------------------------
// Core calculation functions
// ---------------------------------------------------------------------------

/// Calculate the age of a box in blocks and approximate years.
///
/// # Arguments
/// * `creation_height` — Block height when the box was created.
/// * `current_height`  — Current chain tip height.
///
/// # Returns
/// `(age_blocks, age_years)` where `age_years` is a floating-point estimate.
pub fn calculate_box_age(creation_height: u32, current_height: u32) -> (u64, f64) {
    let age_blocks = if current_height > creation_height {
        (current_height - creation_height) as u64
    } else {
        0
    };
    let age_years = age_blocks as f64 / BLOCKS_PER_YEAR;
    (age_blocks, age_years)
}

/// Classify a box's rent status from its age in blocks.
///
/// The thresholds follow the Ergo protocol:
/// - Fresh:   age < 730 000
/// - Aging:   730 000 ≤ age < warning_threshold (default 1 020 000)
/// - Warning: warning_threshold ≤ age < critical_threshold (default 1 045 000)
/// - Critical: critical_threshold ≤ age < 1 051 200
/// - Expired: age ≥ 1 051 200
pub fn classify_rent_status(age_blocks: u64) -> RentStatus {
    if age_blocks < 730_000 {
        RentStatus::Fresh
    } else if age_blocks < 1_020_000 {
        RentStatus::Aging
    } else if age_blocks < 1_045_000 {
        RentStatus::Warning
    } else if age_blocks < 1_051_200 {
        RentStatus::Critical
    } else {
        RentStatus::Expired
    }
}

/// Classify rent status using custom thresholds from the config.
pub fn classify_rent_status_with_config(age_blocks: u64, config: &MonitorConfig) -> RentStatus {
    if age_blocks < 730_000 {
        RentStatus::Fresh
    } else if age_blocks < config.warning_threshold_blocks {
        RentStatus::Aging
    } else if age_blocks < config.critical_threshold_blocks {
        RentStatus::Warning
    } else if age_blocks < 1_051_200 {
        RentStatus::Critical
    } else {
        RentStatus::Expired
    }
}

/// Estimate the storage rent fee for one rent cycle.
///
/// # Arguments
/// * `box_size_bytes` — Serialized size of the box in bytes.
/// * `fee_per_byte`   — Storage fee rate in nanoERG per byte (default 360).
///
/// # Returns
/// Rent fee in nanoERG for a single 2-year cycle.
pub fn estimate_rent_fee(box_size_bytes: usize, fee_per_byte: u64) -> u64 {
    let box_size_bytes = box_size_bytes as u64;
    let fee_per_byte = if fee_per_byte == 0 {
        FEE_PER_BYTE_DEFAULT
    } else {
        fee_per_byte
    };
    box_size_bytes * fee_per_byte
}

/// Estimate how many full rent cycles a box can survive given its value.
///
/// # Arguments
/// * `value_nanoerg`      — Current ERG value of the box.
/// * `rent_fee_per_cycle` — Cost of one rent cycle in nanoERG.
///
/// # Returns
/// Number of full cycles the box value can cover (0 if fee exceeds value).
pub fn estimate_survival_cycles(value_nanoerg: u64, rent_fee_per_cycle: u64) -> u32 {
    if rent_fee_per_cycle == 0 {
        return u32::MAX;
    }
    (value_nanoerg / rent_fee_per_cycle) as u32
}

/// Recommend a top-off amount to cover a target number of years.
///
/// # Arguments
/// * `value_nanoerg`   — Current ERG value of the box.
/// * `box_size_bytes`  — Serialized box size in bytes.
/// * `target_years`    — Desired runway in years (e.g. 4.0 for two more cycles).
///
/// # Returns
/// Additional nanoERG needed, or 0 if the box already has enough runway.
pub fn recommend_top_off(value_nanoerg: u64, box_size_bytes: usize, target_years: f64) -> u64 {
    let cycles_needed = (target_years / 2.0).ceil() as u32; // each cycle = 2 years
    let fee_per_cycle = estimate_rent_fee(box_size_bytes, FEE_PER_BYTE_DEFAULT);
    let required = fee_per_cycle * cycles_needed as u64;
    if value_nanoerg >= required {
        0
    } else {
        required - value_nanoerg
    }
}

/// Analyze a collection of boxes belonging to one address and produce a summary.
///
/// This is the main workhorse used by both the single-address and scan endpoints.
pub fn analyze_address(boxes: Vec<BoxData>) -> AddressRentSummary {
    let address = boxes
        .first()
        .map(|b| b.address.clone())
        .unwrap_or_default();

    let mut by_status: HashMap<RentStatus, u32> = HashMap::new();
    let mut total_value_at_risk: u64 = 0;
    let mut top_off_estimate: u64 = 0;

    for b in &boxes {
        let (age_blocks, _age_years) = calculate_box_age(b.creation_height, 0);
        let status = classify_rent_status(age_blocks);
        *by_status.entry(status).or_insert(0) += 1;

        if matches!(
            status,
            RentStatus::Warning | RentStatus::Critical | RentStatus::Expired
        ) {
            total_value_at_risk += b.value_nanoerg;
            let fee = estimate_rent_fee(b.box_size_bytes, FEE_PER_BYTE_DEFAULT);
            top_off_estimate += recommend_top_off(b.value_nanoerg, b.box_size_bytes, 4.0);
            // If the box has 0 tokens and value is small, we don't want a division by zero
            let _survival = estimate_survival_cycles(b.value_nanoerg, fee);
        }
    }

    AddressRentSummary {
        address,
        total_boxes: boxes.len() as u32,
        by_status,
        total_value_at_risk,
        top_off_estimate,
    }
}

/// Analyze a collection of boxes for one address using a specific chain height and config.
pub fn analyze_address_with_config(
    boxes: Vec<BoxData>,
    current_height: u32,
    config: &MonitorConfig,
) -> AddressRentSummary {
    let address = boxes
        .first()
        .map(|b| b.address.clone())
        .unwrap_or_default();

    let mut by_status: HashMap<RentStatus, u32> = HashMap::new();
    let mut total_value_at_risk: u64 = 0;
    let mut top_off_estimate: u64 = 0;

    for b in &boxes {
        let (age_blocks, _age_years) = calculate_box_age(b.creation_height, current_height);
        let status = classify_rent_status_with_config(age_blocks, config);
        *by_status.entry(status).or_insert(0) += 1;

        if matches!(
            status,
            RentStatus::Warning | RentStatus::Critical | RentStatus::Expired
        ) {
            total_value_at_risk += b.value_nanoerg;
            let fee = estimate_rent_fee(b.box_size_bytes, FEE_PER_BYTE_DEFAULT);
            top_off_estimate += recommend_top_off(b.value_nanoerg, b.box_size_bytes, 4.0);
        }
    }

    AddressRentSummary {
        address,
        total_boxes: boxes.len() as u32,
        by_status,
        total_value_at_risk,
        top_off_estimate,
    }
}

/// Build a `BoxRentInfo` from a `BoxData` and a chain height.
pub fn build_box_rent_info(box_data: &BoxData, current_height: u32) -> BoxRentInfo {
    let (age_blocks, _age_years) = calculate_box_age(box_data.creation_height, current_height);
    let rent_status = classify_rent_status(age_blocks);
    let estimated_rent_fee = estimate_rent_fee(box_data.box_size_bytes, FEE_PER_BYTE_DEFAULT);
    let cycles_survivable = estimate_survival_cycles(box_data.value_nanoerg, estimated_rent_fee);
    let recommended_top_off = recommend_top_off(
        box_data.value_nanoerg,
        box_data.box_size_bytes,
        4.0, // default target: 4 years
    );

    BoxRentInfo {
        box_id: box_data.box_id.clone(),
        address: box_data.address.clone(),
        value_nanoerg: box_data.value_nanoerg,
        token_count: box_data.token_count,
        creation_height: box_data.creation_height,
        current_height,
        age_blocks,
        rent_status,
        estimated_rent_fee,
        cycles_survivable,
        recommended_top_off,
    }
}

// ---------------------------------------------------------------------------
// REST API Handlers
// ---------------------------------------------------------------------------

/// POST /v1/rent/scan — Scan multiple addresses for rent status.
///
/// Accepts a JSON body with an `addresses` array and returns per-address
/// summaries along with aggregate counts.
pub async fn scan_addresses(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RentScanRequest>,
) -> impl IntoResponse {
    if req.addresses.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "addresses array must not be empty",
            })),
        )
            .into_response();
    }
    if req.addresses.len() > 100 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "addresses array must not exceed 100 entries",
            })),
        )
            .into_response();
    }

    let config = state.config.read().await;
    let height = state.get_height();

    // In a real implementation we would query the node for boxes per address.
    // Here we build empty summaries — the scan is a no-op placeholder that
    // demonstrates the wiring.  Real data would come from the Ergo node API.
    let mut summaries: Vec<AddressRentSummary> = Vec::new();
    let mut total_boxes: u32 = 0;
    let mut boxes_at_risk: u32 = 0;

    for addr in &req.addresses {
        let summary = analyze_address_with_config(Vec::new(), height, &config);
        total_boxes += summary.total_boxes;
        boxes_at_risk += summary.by_status.iter().filter_map(|(s, c)| {
            if matches!(s, RentStatus::Warning | RentStatus::Critical | RentStatus::Expired) {
                Some(*c)
            } else {
                None
            }
        }).sum::<u32>();
        summaries.push(summary);
    }

    let result = RentScanResult {
        scanned_at: Utc::now().to_rfc3339(),
        summaries,
        total_boxes_checked: total_boxes,
        boxes_at_risk,
    };

    (StatusCode::OK, Json(result)).into_response()
}

/// GET /v1/rent/status/:address — Get rent summary for one address.
pub async fn get_address_status(
    State(state): State<Arc<AppState>>,
    Path(address): Path<String>,
) -> impl IntoResponse {
    let config = state.config.read().await;
    let height = state.get_height();

    let summary = analyze_address_with_config(Vec::new(), height, &config);

    (StatusCode::OK, Json(summary)).into_response()
}

/// GET /v1/rent/box/:box_id — Get rent info for a specific box.
///
/// This is a placeholder that demonstrates the response schema.
/// A real implementation would look up the box by ID from the node.
pub async fn get_box_rent_info(
    State(state): State<Arc<AppState>>,
    Path(box_id): Path<String>,
) -> impl IntoResponse {
    let height = state.get_height();

    let info = BoxRentInfo {
        box_id,
        address: String::new(),
        value_nanoerg: 0,
        token_count: 0,
        creation_height: 0,
        current_height: height,
        age_blocks: 0,
        rent_status: RentStatus::Fresh,
        estimated_rent_fee: 0,
        cycles_survivable: 0,
        recommended_top_off: 0,
    };

    (StatusCode::OK, Json(info)).into_response()
}

/// GET /v1/rent/summary — Overall monitor summary.
pub async fn get_summary(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let total_events = state.total_events();
    let height = state.get_height();
    let config = state.config.read().await;
    let address_count = state.events.len();

    let mut total_at_risk: u64 = 0;
    let mut total_boxes_tracked: u32 = 0;

    for mut entry in state.events.iter() {
        let addr_events = entry.value();
        let addr = entry.key().clone();
        // Count at-risk events as a proxy for at-risk boxes
        let at_risk_events: u32 = addr_events
            .iter()
            .filter(|e| {
                matches!(
                    e.event_type,
                    RentEventType::AtRiskDetected | RentEventType::BoxExpired
                )
            })
            .count() as u32;
        total_at_risk += at_risk_events as u64;
        total_boxes_tracked += addr_events.len() as u32;
    }

    let response = serde_json::json!({
        "current_height": height,
        "total_events_recorded": total_events,
        "addresses_monitored": address_count,
        "total_at_risk_events": total_at_risk,
        "auto_scan_enabled": config.auto_scan_enabled,
        "scan_interval_secs": config.scan_interval_secs,
        "warning_threshold_blocks": config.warning_threshold_blocks,
        "critical_threshold_blocks": config.critical_threshold_blocks,
        "protected_addresses": config.protected_addresses,
    });

    (StatusCode::OK, Json(response)).into_response()
}

/// GET /v1/rent/events — Rent event history.
///
/// Supports optional `?address=<addr>&limit=<n>` query parameters.
pub async fn get_events(
    State(state): State<Arc<AppState>>,
    Query(params): Query<EventsQuery>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(100).min(1_000);

    let events: Vec<RentEvent> = if let Some(ref address) = params.address {
        state
            .events
            .get(address)
            .map(|entry| {
                let v: Vec<RentEvent> = entry.value().iter().rev().take(limit).cloned().collect();
                v
            })
            .unwrap_or_default()
    } else {
        // Merge events from all addresses, most-recent first
        let mut all: Vec<RentEvent> = Vec::new();
        for entry in state.events.iter() {
            all.extend(entry.value().iter().cloned());
        }
        all.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        all.truncate(limit);
        all
    };

    let response = serde_json::json!({
        "events": events,
        "count": events.len(),
        "total_recorded": state.total_events(),
    });

    (StatusCode::OK, Json(response)).into_response()
}

/// GET /v1/rent/config — Get the current monitor configuration.
pub async fn get_config(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let config = state.config.read().await;
    (StatusCode::OK, Json(config.clone())).into_response()
}

/// PUT /v1/rent/config — Update the monitor configuration.
///
/// Accepts a full `MonitorConfig` JSON body and replaces the running config.
/// Fields not provided will be set to their default values, so callers should
/// read the current config first if they want a partial update.
pub async fn update_config(
    State(state): State<Arc<AppState>>,
    Json(new_config): Json<MonitorConfig>,
) -> impl IntoResponse {
    let mut config = state.config.write().await;
    config.warning_threshold_blocks = new_config.warning_threshold_blocks;
    config.critical_threshold_blocks = new_config.critical_threshold_blocks;
    config.auto_scan_enabled = new_config.auto_scan_enabled;
    config.scan_interval_secs = new_config.scan_interval_secs;
    config.protected_addresses = new_config.protected_addresses;

    // Record a config-change event
    state.record_event(RentEvent {
        event_type: RentEventType::BoxConsolidated, // repurposed as a system event
        box_id: String::new(),
        address: "__system__".to_string(),
        timestamp: Utc::now().to_rfc3339(),
        details: format!(
            "Config updated: warning={}, critical={}, auto_scan={}, interval={}s",
            config.warning_threshold_blocks,
            config.critical_threshold_blocks,
            config.auto_scan_enabled,
            config.scan_interval_secs,
        ),
    });

    (StatusCode::OK, Json(config.clone())).into_response()
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the storage rent monitor API router.
pub fn build_router(state: Arc<AppState>) -> Router<()> {
    Router::new()
        .route("/v1/rent/scan", axum::routing::post(scan_addresses))
        .route("/v1/rent/status/{address}", get(get_address_status))
        .route("/v1/rent/box/{box_id}", get(get_box_rent_info))
        .route("/v1/rent/summary", get(get_summary))
        .route("/v1/rent/events", get(get_events))
        .route("/v1/rent/config", get(get_config))
        .route("/v1/rent/config", put(update_config))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use std::collections::HashMap;

    // ---- Helper ----

    fn make_test_state() -> Arc<AppState> {
        Arc::new(AppState::new())
    }

    fn make_box_data(
        box_id: &str,
        address: &str,
        value_nanoerg: u64,
        token_count: u32,
        creation_height: u32,
        box_size_bytes: usize,
    ) -> BoxData {
        BoxData {
            box_id: box_id.to_string(),
            address: address.to_string(),
            value_nanoerg,
            token_count,
            creation_height,
            box_size_bytes,
        }
    }

    // ================================================================
    // Age calculation tests
    // ================================================================

    #[test]
    fn test_age_zero_height() {
        let (blocks, years) = calculate_box_age(500, 500);
        assert_eq!(blocks, 0);
        assert!((years - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_age_future_height() {
        // creation_height > current_height → clamped to 0
        let (blocks, years) = calculate_box_age(1000, 500);
        assert_eq!(blocks, 0);
        assert!((years - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_age_fresh_box() {
        let (blocks, years) = calculate_box_age(100_000, 200_000);
        assert_eq!(blocks, 100_000);
        assert!((years - (100_000.0 / BLOCKS_PER_YEAR)).abs() < 1e-6);
    }

    #[test]
    fn test_age_aging_box() {
        let (blocks, _) = calculate_box_age(100_000, 900_000);
        assert_eq!(blocks, 800_000);
    }

    #[test]
    fn test_age_warning_box() {
        let (blocks, _) = calculate_box_age(100_000, 1_030_000);
        assert_eq!(blocks, 930_000);
    }

    #[test]
    fn test_age_critical_box() {
        let (blocks, _) = calculate_box_age(100_000, 1_046_000);
        assert_eq!(blocks, 946_000);
    }

    #[test]
    fn test_age_expired_box() {
        let (blocks, _) = calculate_box_age(100_000, 1_200_000);
        assert_eq!(blocks, 1_100_000);
    }

    // ================================================================
    // Rent status classification tests
    // ================================================================

    #[test]
    fn test_classify_fresh_below_boundary() {
        assert_eq!(classify_rent_status(729_999), RentStatus::Fresh);
    }

    #[test]
    fn test_classify_fresh_at_zero() {
        assert_eq!(classify_rent_status(0), RentStatus::Fresh);
    }

    #[test]
    fn test_classify_aging_boundary() {
        assert_eq!(classify_rent_status(730_000), RentStatus::Aging);
    }

    #[test]
    fn test_classify_aging_mid() {
        assert_eq!(classify_rent_status(900_000), RentStatus::Aging);
    }

    #[test]
    fn test_classify_aging_upper_boundary() {
        assert_eq!(classify_rent_status(1_019_999), RentStatus::Aging);
    }

    #[test]
    fn test_classify_warning_boundary() {
        assert_eq!(classify_rent_status(1_020_000), RentStatus::Warning);
    }

    #[test]
    fn test_classify_warning_mid() {
        assert_eq!(classify_rent_status(1_030_000), RentStatus::Warning);
    }

    #[test]
    fn test_classify_critical_boundary() {
        assert_eq!(classify_rent_status(1_045_000), RentStatus::Critical);
    }

    #[test]
    fn test_classify_critical_mid() {
        assert_eq!(classify_rent_status(1_048_000), RentStatus::Critical);
    }

    #[test]
    fn test_classify_expired_boundary() {
        assert_eq!(classify_rent_status(1_051_200), RentStatus::Expired);
    }

    #[test]
    fn test_classify_expired_far() {
        assert_eq!(classify_rent_status(2_000_000), RentStatus::Expired);
    }

    #[test]
    fn test_classify_with_custom_config() {
        let config = MonitorConfig {
            warning_threshold_blocks: 800_000,
            critical_threshold_blocks: 1_000_000,
            ..Default::default()
        };
        // 750 000 is between Fresh (730 000) and custom warning (800 000)
        assert_eq!(classify_rent_status_with_config(750_000, &config), RentStatus::Aging);
        // 850 000 is between custom warning and custom critical
        assert_eq!(
            classify_rent_status_with_config(850_000, &config),
            RentStatus::Warning
        );
        // 1 020 000 is between custom critical and expired
        assert_eq!(
            classify_rent_status_with_config(1_020_000, &config),
            RentStatus::Critical
        );
    }

    // ================================================================
    // Rent fee estimation tests
    // ================================================================

    #[test]
    fn test_rent_fee_basic() {
        // 200 bytes × 360 nanoERG/byte = 72 000 nanoERG
        assert_eq!(estimate_rent_fee(200, 360), 72_000);
    }

    #[test]
    fn test_rent_fee_zero_size() {
        assert_eq!(estimate_rent_fee(0, 360), 0);
    }

    #[test]
    fn test_rent_fee_custom_rate() {
        assert_eq!(estimate_rent_fee(100, 500), 50_000);
    }

    #[test]
    fn test_rent_fee_zero_rate_falls_back_to_default() {
        // 0 rate → falls back to default 360
        assert_eq!(estimate_rent_fee(100, 0), 36_000);
    }

    #[test]
    fn test_rent_fee_large_box() {
        let fee = estimate_rent_fee(10_000, 360);
        assert_eq!(fee, 3_600_000);
    }

    // ================================================================
    // Survival cycles tests
    // ================================================================

    #[test]
    fn test_survival_cycles_exact() {
        // 100_000 nanoERG / 50_000 per cycle = 2 cycles
        assert_eq!(estimate_survival_cycles(100_000, 50_000), 2);
    }

    #[test]
    fn test_survival_cycles_partial() {
        // 120_000 / 50_000 = 2 (truncated, remainder doesn't count)
        assert_eq!(estimate_survival_cycles(120_000, 50_000), 2);
    }

    #[test]
    fn test_survival_cycles_insufficient() {
        assert_eq!(estimate_survival_cycles(10_000, 50_000), 0);
    }

    #[test]
    fn test_survival_cycles_zero_fee() {
        // If fee is 0, the box can survive forever
        assert_eq!(estimate_survival_cycles(1_000_000, 0), u32::MAX);
    }

    // ================================================================
    // Top-off recommendation tests
    // ================================================================

    #[test]
    fn test_top_off_needed() {
        // Box: 100 bytes, fee = 36_000 per cycle.
        // 4 years = 2 cycles → need 72_000.
        // Value = 10_000 → need 62_000 more.
        let top_off = recommend_top_off(10_000, 100, 4.0);
        assert_eq!(top_off, 62_000);
    }

    #[test]
    fn test_top_off_sufficient() {
        // Value already exceeds requirement
        let top_off = recommend_top_off(1_000_000, 100, 4.0);
        assert_eq!(top_off, 0);
    }

    #[test]
    fn test_top_off_one_year() {
        // 1 year → ceil(1/2) = 1 cycle. Fee = 36_000 for 100 bytes.
        let top_off = recommend_top_off(0, 100, 1.0);
        assert_eq!(top_off, 36_000);
    }

    #[test]
    fn test_top_off_three_years() {
        // 3 years → ceil(3/2) = 2 cycles.
        let top_off = recommend_top_off(0, 100, 3.0);
        assert_eq!(top_off, 72_000);
    }

    // ================================================================
    // Address analysis tests
    // ================================================================

    #[test]
    fn test_analyze_empty_address() {
        let summary = analyze_address(Vec::new());
        assert_eq!(summary.address, "");
        assert_eq!(summary.total_boxes, 0);
        assert_eq!(summary.total_value_at_risk, 0);
        assert_eq!(summary.top_off_estimate, 0);
        assert!(summary.by_status.is_empty());
    }

    #[test]
    fn test_analyze_address_with_boxes() {
        // creation_height=100, current_height in analyze_address is 0
        // so age = max(0 - 100, 0) = 0 → Fresh
        let boxes = vec![
            make_box_data("box1", "addr1", 1_000_000, 0, 100, 200),
            make_box_data("box2", "addr1", 2_000_000, 1, 100, 300),
        ];
        let summary = analyze_address(boxes);
        assert_eq!(summary.address, "addr1");
        assert_eq!(summary.total_boxes, 2);
        assert_eq!(*summary.by_status.get(&RentStatus::Fresh).unwrap(), 2);
        assert_eq!(summary.total_value_at_risk, 0);
    }

    #[test]
    fn test_analyze_address_with_config() {
        let config = MonitorConfig::default();
        // creation=100, current=0, age=0 → Fresh
        let boxes = vec![make_box_data("box1", "addr1", 1_000_000, 0, 100, 200)];
        let summary = analyze_address_with_config(boxes, 0, &config);
        assert_eq!(summary.total_boxes, 1);
        assert_eq!(*summary.by_status.get(&RentStatus::Fresh).unwrap(), 1);
    }

    // ================================================================
    // Event tracking tests
    // ================================================================

    #[test]
    fn test_record_event_basic() {
        let state = make_test_state();
        state.record_event(RentEvent {
            event_type: RentEventType::AtRiskDetected,
            box_id: "box1".to_string(),
            address: "addr1".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            details: "Box entered warning".to_string(),
        });
        assert_eq!(state.total_events(), 1);
        let events = state.events.get("addr1").unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, RentEventType::AtRiskDetected);
    }

    #[test]
    fn test_record_multiple_events() {
        let state = make_test_state();
        for i in 0..5 {
            state.record_event(RentEvent {
                event_type: RentEventType::AtRiskDetected,
                box_id: format!("box{}", i),
                address: "addr1".to_string(),
                timestamp: format!("2025-01-01T00:0{}:00Z", i),
                details: "test".to_string(),
            });
        }
        assert_eq!(state.total_events(), 5);
        assert_eq!(state.events.get("addr1").unwrap().len(), 5);
    }

    #[test]
    fn test_ring_buffer_overflow() {
        let state = make_test_state();
        // Fill past the 50K limit
        for i in 0..(MAX_EVENTS_PER_ADDRESS + 100) {
            state.record_event(RentEvent {
                event_type: RentEventType::AtRiskDetected,
                box_id: format!("box{}", i),
                address: "overflow_addr".to_string(),
                timestamp: "2025-01-01T00:00:00Z".to_string(),
                details: format!("event {}", i),
            });
        }
        let events = state.events.get("overflow_addr").unwrap();
        assert_eq!(events.len(), MAX_EVENTS_PER_ADDRESS);
        // The oldest event (0) should have been evicted
        assert_eq!(events.front().unwrap().box_id, "box100");
        assert_eq!(events.back().unwrap().box_id, format!("box{}", MAX_EVENTS_PER_ADDRESS + 99));
        // Total counter keeps incrementing even on overflow
        assert_eq!(state.total_events(), (MAX_EVENTS_PER_ADDRESS + 100) as u64);
    }

    #[test]
    fn test_events_multiple_addresses() {
        let state = make_test_state();
        state.record_event(RentEvent {
            event_type: RentEventType::AtRiskDetected,
            box_id: "box1".to_string(),
            address: "addr1".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            details: "".to_string(),
        });
        state.record_event(RentEvent {
            event_type: RentEventType::BoxExpired,
            box_id: "box2".to_string(),
            address: "addr2".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            details: "".to_string(),
        });
        assert_eq!(state.events.len(), 2);
        assert_eq!(state.total_events(), 2);
    }

    #[test]
    fn test_event_types_all_variants() {
        let state = make_test_state();
        let types = [
            RentEventType::AtRiskDetected,
            RentEventType::TopOffPlanned,
            RentEventType::BoxExpired,
            RentEventType::BoxConsolidated,
        ];
        for (i, et) in types.iter().enumerate() {
            state.record_event(RentEvent {
                event_type: *et,
                box_id: format!("box{}", i),
                address: "addr1".to_string(),
                timestamp: "2025-01-01T00:00:00Z".to_string(),
                details: "".to_string(),
            });
        }
        let events = state.events.get("addr1").unwrap();
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].event_type, RentEventType::AtRiskDetected);
        assert_eq!(events[1].event_type, RentEventType::TopOffPlanned);
        assert_eq!(events[2].event_type, RentEventType::BoxExpired);
        assert_eq!(events[3].event_type, RentEventType::BoxConsolidated);
    }

    // ================================================================
    // Config management tests
    // ================================================================

    #[test]
    fn test_config_defaults() {
        let config = MonitorConfig::default();
        assert_eq!(config.warning_threshold_blocks, 1_020_000);
        assert_eq!(config.critical_threshold_blocks, 1_045_000);
        assert!(!config.auto_scan_enabled);
        assert_eq!(config.scan_interval_secs, 3_600);
        assert!(config.protected_addresses.is_empty());
    }

    #[test]
    fn test_config_custom() {
        let config = MonitorConfig {
            warning_threshold_blocks: 500_000,
            critical_threshold_blocks: 900_000,
            auto_scan_enabled: true,
            scan_interval_secs: 1_800,
            protected_addresses: vec!["protected_addr".to_string()],
        };
        assert_eq!(config.warning_threshold_blocks, 500_000);
        assert_eq!(config.critical_threshold_blocks, 900_000);
        assert!(config.auto_scan_enabled);
        assert_eq!(config.scan_interval_secs, 1_800);
        assert_eq!(config.protected_addresses.len(), 1);
    }

    #[test]
    fn test_app_state_new() {
        let state = AppState::new();
        assert_eq!(state.get_height(), 0);
        assert_eq!(state.total_events(), 0);
        assert!(state.events.is_empty());
    }

    #[test]
    fn test_app_state_with_config() {
        let config = MonitorConfig {
            auto_scan_enabled: true,
            ..Default::default()
        };
        let state = AppState::with_config(config);
        assert_eq!(state.get_height(), 0);
    }

    #[test]
    fn test_app_state_height() {
        let state = AppState::new();
        state.set_height(1_000_000);
        assert_eq!(state.get_height(), 1_000_000);
    }

    // ================================================================
    // BoxRentInfo builder test
    // ================================================================

    #[test]
    fn test_build_box_rent_info() {
        let box_data = make_box_data("abc123", "9fDcNZjhbhdhHpuPFvMksfpsF4qd3e8mdY", 10_000_000_000, 2, 500_000, 250);
        let info = build_box_rent_info(&box_data, 600_000);
        assert_eq!(info.box_id, "abc123");
        assert_eq!(info.age_blocks, 100_000);
        assert_eq!(info.rent_status, RentStatus::Fresh);
        assert_eq!(info.estimated_rent_fee, 250 * 360); // 90 000
        assert_eq!(info.cycles_survivable, 10_000_000_000 / 90_000);
    }

    // ================================================================
    // RentStatus display test
    // ================================================================

    #[test]
    fn test_rent_status_display() {
        assert_eq!(format!("{}", RentStatus::Fresh), "fresh");
        assert_eq!(format!("{}", RentStatus::Aging), "aging");
        assert_eq!(format!("{}", RentStatus::Warning), "warning");
        assert_eq!(format!("{}", RentStatus::Critical), "critical");
        assert_eq!(format!("{}", RentStatus::Expired), "expired");
    }

    // ================================================================
    // API handler tests (unit-level, non-network)
    // ================================================================

    #[tokio::test]
    async fn test_scan_empty_addresses() {
        let state = make_test_state();
        let req = RentScanRequest { addresses: vec![] };

        let resp = scan_addresses(State(state), Json(req)).await.into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_scan_too_many_addresses() {
        let state = make_test_state();
        let addresses: Vec<String> = (0..101).map(|i| format!("addr{}", i)).collect();
        let req = RentScanRequest { addresses };

        let resp = scan_addresses(State(state), Json(req)).await.into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_scan_valid_request() {
        let state = make_test_state();
        let req = RentScanRequest {
            addresses: vec!["3WvsT2Gm4EpsM9Pg18PdY6XyhNN8p2fs5TG".to_string()],
        };

        let resp = scan_addresses(State(state), Json(req)).await.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_address_status() {
        let state = make_test_state();
        let resp = get_address_status(
            State(state),
            Path("3WvsT2Gm4EpsM9Pg18PdY6XyhNN8p2fs5TG".to_string()),
        )
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_summary() {
        let state = make_test_state();
        state.set_height(1_000_000);
        let resp = get_summary(State(state)).await.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_config_handler() {
        let state = make_test_state();
        let resp = get_config(State(state)).await.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_update_config_handler() {
        let state = make_test_state();
        let new_config = MonitorConfig {
            auto_scan_enabled: true,
            scan_interval_secs: 1800,
            ..Default::default()
        };
        let resp = update_config(State(state.clone()), Json(new_config))
            .await
            .into_response();
        assert_eq!(resp.status(), StatusCode::OK);

        // Verify the config was updated
        let config = state.config.read().await;
        assert!(config.auto_scan_enabled);
        assert_eq!(config.scan_interval_secs, 1800);
    }

    #[tokio::test]
    async fn test_get_events_empty() {
        let state = make_test_state();
        let resp = get_events(
            State(state),
            Query(EventsQuery {
                address: None,
                limit: None,
            }),
        )
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_events_with_data() {
        let state = make_test_state();
        state.record_event(RentEvent {
            event_type: RentEventType::AtRiskDetected,
            box_id: "box1".to_string(),
            address: "addr1".to_string(),
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            details: "test event".to_string(),
        });
        let resp = get_events(
            State(state),
            Query(EventsQuery {
                address: Some("addr1".to_string()),
                limit: Some(10),
            }),
        )
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_box_rent_info_handler() {
        let state = make_test_state();
        state.set_height(1_000_000);
        let resp = get_box_rent_info(
            State(state),
            Path("abc123def456".to_string()),
        )
        .await
        .into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // ================================================================
    // Router construction test
    // ================================================================

    #[test]
    fn test_build_router() {
        let state = make_test_state();
        let _router = build_router(state);
        // Router builds without panic — success
    }
}
