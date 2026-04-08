#![allow(dead_code)]
//! Multi-Region Routing
//!
//! Routes inference requests across geographic regions using configurable
//! strategies (nearest, lowest latency, lowest cost, balanced, sticky,
//! failover). Includes health monitoring, capacity-aware routing, and
//! automatic cross-region failover.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Routing strategy for multi-region provider selection.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RegionRoutingStrategy {
    /// Route to geographically closest region.
    Nearest,
    /// Route to region with lowest measured latency.
    LowestLatency,
    /// Route to cheapest region (considering cross-region penalty).
    LowestCost,
    /// Weighted score of latency, cost, and capacity.
    Balanced,
    /// Prefer same region as previous request from this client.
    Sticky,
    /// Use primary region, failover to backup.
    Failover,
}

impl Default for RegionRoutingStrategy {
    fn default() -> Self {
        Self::Nearest
    }
}

impl std::fmt::Display for RegionRoutingStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nearest => write!(f, "nearest"),
            Self::LowestLatency => write!(f, "lowest_latency"),
            Self::LowestCost => write!(f, "lowest_cost"),
            Self::Balanced => write!(f, "balanced"),
            Self::Sticky => write!(f, "sticky"),
            Self::Failover => write!(f, "failover"),
        }
    }
}

/// Configuration for multi-region routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionConfig {
    pub regions: Vec<Region>,
    pub routing_strategy: RegionRoutingStrategy,
    pub failover_enabled: bool,
    pub latency_threshold_secs: f64,
    pub health_check_interval_secs: u64,
    pub cross_region_cost_multiplier: f64,
}

impl Default for RegionConfig {
    fn default() -> Self {
        Self {
            regions: Vec::new(),
            routing_strategy: RegionRoutingStrategy::default(),
            failover_enabled: true,
            latency_threshold_secs: 0.5,
            health_check_interval_secs: 30,
            cross_region_cost_multiplier: 1.5,
        }
    }
}

// ---------------------------------------------------------------------------
// Region
// ---------------------------------------------------------------------------

/// A geographic region with provider capacity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Region {
    pub id: String,
    pub name: String,
    pub endpoint: String,
    pub latitude: f64,
    pub longitude: f64,
    pub priority: u8,
    pub enabled: bool,
    pub provider_count: usize,
    pub available_capacity: u64,
}

impl Region {
    /// Haversine distance in km to another point.
    pub fn distance_km_to(&self, lat: f64, lon: f64) -> f64 {
        haversine_km(self.latitude, self.longitude, lat, lon)
    }
}

// ---------------------------------------------------------------------------
// Region Health
// ---------------------------------------------------------------------------

/// Health status of a region.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RegionStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Draining,
}

impl std::fmt::Display for RegionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Healthy => write!(f, "healthy"),
            Self::Degraded => write!(f, "degraded"),
            Self::Unhealthy => write!(f, "unhealthy"),
            Self::Draining => write!(f, "draining"),
        }
    }
}

/// Per-region health metrics.
#[derive(Debug, Clone, Serialize)]
pub struct RegionHealth {
    pub region_id: String,
    pub status: RegionStatus,
    pub avg_latency: Duration,
    pub error_rate: f64,
    pub active_requests: u32,
    pub last_check: DateTime<Utc>,
    pub total_requests: u64,
    pub total_errors: u64,
}

impl Default for RegionHealth {
    fn default() -> Self {
        Self {
            region_id: String::new(),
            status: RegionStatus::Healthy,
            avg_latency: Duration::from_millis(0),
            error_rate: 0.0,
            active_requests: 0,
            last_check: Utc::now(),
            total_requests: 0,
            total_errors: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Routing Statistics
// ---------------------------------------------------------------------------

/// Lock-free routing statistics.
#[derive(Debug, Default, Serialize)]
pub struct RegionRoutingStats {
    pub requests_routed: AtomicU64,
    pub cross_region_requests: AtomicU64,
    pub failovers_triggered: AtomicU64,
    pub avg_latency_ms: AtomicU64,
}

impl RegionRoutingStats {
    pub fn snapshot(&self) -> RegionRoutingStatsSnapshot {
        RegionRoutingStatsSnapshot {
            requests_routed: self.requests_routed.load(Ordering::Relaxed),
            cross_region_requests: self.cross_region_requests.load(Ordering::Relaxed),
            failovers_triggered: self.failovers_triggered.load(Ordering::Relaxed),
            avg_latency_ms: self.avg_latency_ms.load(Ordering::Relaxed),
        }
    }

    /// Record a routed request and update rolling average latency.
    pub fn record_request(&self, cross_region: bool, latency_ms: u64) {
        self.requests_routed.fetch_add(1, Ordering::Relaxed);
        if cross_region {
            self.cross_region_requests.fetch_add(1, Ordering::Relaxed);
        }
        // Exponential moving average for latency
        loop {
            let current = self.avg_latency_ms.load(Ordering::Relaxed);
            let count = self.requests_routed.load(Ordering::Relaxed);
            let alpha = if count <= 1 { 1.0 } else { 2.0 / (count as f64 + 1.0) };
            let updated = (current as f64 * (1.0 - alpha) + latency_ms as f64 * alpha).round() as u64;
            if self.avg_latency_ms.compare_exchange_weak(
                current, updated, Ordering::Relaxed, Ordering::Relaxed,
            ).is_ok() {
                break;
            }
        }
    }

    pub fn record_failover(&self) {
        self.failovers_triggered.fetch_add(1, Ordering::Relaxed);
    }
}

/// Point-in-time routing stats snapshot.
#[derive(Debug, Serialize)]
pub struct RegionRoutingStatsSnapshot {
    pub requests_routed: u64,
    pub cross_region_requests: u64,
    pub failovers_triggered: u64,
    pub avg_latency_ms: u64,
}

// ---------------------------------------------------------------------------
// MultiRegionRouter
// ---------------------------------------------------------------------------

/// The main multi-region router.
pub struct MultiRegionRouter {
    config: std::sync::RwLock<RegionConfig>,
    regions: DashMap<String, Region>,
    health: DashMap<String, RegionHealth>,
    /// Client ID -> region ID (for sticky routing).
    client_region_cache: DashMap<String, String>,
    stats: RegionRoutingStats,
}

impl MultiRegionRouter {
    /// Create a new multi-region router with the given config.
    pub fn new(config: RegionConfig) -> Self {
        let regions = DashMap::new();
        let health = DashMap::new();

        // Seed from config
        for region in &config.regions {
            health.insert(region.id.clone(), RegionHealth {
                region_id: region.id.clone(),
                ..Default::default()
            });
            regions.insert(region.id.clone(), region.clone());
        }

        Self {
            config: std::sync::RwLock::new(config),
            regions,
            health,
            client_region_cache: DashMap::new(),
            stats: RegionRoutingStats::default(),
        }
    }

    // -- Public API ----------------------------------------------------------

    /// Add or update a region.
    pub fn upsert_region(&self, region: Region) {
        let id = region.id.clone();
        self.health.entry(id.clone()).or_insert_with(|| RegionHealth {
            region_id: id.clone(),
            ..Default::default()
        });
        self.regions.insert(id, region);
    }

    /// Remove a region.
    pub fn remove_region(&self, id: &str) {
        self.regions.remove(id);
        self.health.remove(id);
        // Clear sticky cache entries pointing to this region
        self.client_region_cache.retain(|_, v| v != id);
    }

    /// Get a region by ID.
    pub fn get_region(&self, id: &str) -> Option<Region> {
        self.regions.get(id).map(|r| r.value().clone())
    }

    /// List all regions.
    pub fn list_regions(&self) -> Vec<Region> {
        self.regions.iter().map(|r| r.value().clone()).collect()
    }

    /// Get all regions as a read-only reference list.
    fn list_regions_ref(&self) -> Vec<std::sync::Arc<Region>> {
        // We can't return references into DashMap, so collect clones wrapped in Arc
        self.regions.iter().map(|r| std::sync::Arc::new(r.value().clone())).collect()
    }

    /// Update region config (partial).
    pub fn update_region(&self, id: &str, update: RegionUpdate) -> bool {
        let mut region = match self.regions.get_mut(id) {
            Some(r) => r,
            None => return false,
        };
        if let Some(name) = update.name {
            region.name = name;
        }
        if let Some(endpoint) = update.endpoint {
            region.endpoint = endpoint;
        }
        if let Some(lat) = update.latitude {
            region.latitude = lat;
        }
        if let Some(lon) = update.longitude {
            region.longitude = lon;
        }
        if let Some(priority) = update.priority {
            region.priority = priority;
        }
        if let Some(enabled) = update.enabled {
            region.enabled = enabled;
        }
        if let Some(cap) = update.available_capacity {
            region.available_capacity = cap;
        }
        true
    }

    /// Start draining a region.
    pub fn start_drain(&self, id: &str) -> bool {
        match self.health.get_mut(id) {
            Some(mut h) => {
                h.status = RegionStatus::Draining;
                true
            }
            None => false,
        }
    }

    /// Activate a previously drained region.
    pub fn activate(&self, id: &str) -> bool {
        match self.health.get_mut(id) {
            Some(mut h) => {
                h.status = RegionStatus::Healthy;
                true
            }
            None => false,
        }
    }

    /// Get health of a specific region.
    pub fn get_health(&self, id: &str) -> Option<RegionHealth> {
        self.health.get(id).map(|h| h.value().clone())
    }

    /// Get health of all regions.
    pub fn get_all_health(&self) -> Vec<RegionHealth> {
        self.health.iter().map(|h| h.value().clone()).collect()
    }

    /// Get routing statistics.
    pub fn get_stats(&self) -> RegionRoutingStatsSnapshot {
        self.stats.snapshot()
    }

    /// Get current routing strategy.
    pub fn get_routing_strategy(&self) -> RegionRoutingStrategy {
        self.config.read().unwrap().routing_strategy
    }

    /// Change routing strategy at runtime.
    pub fn set_routing_strategy(&self, strategy: RegionRoutingStrategy) {
        self.config.write().unwrap().routing_strategy = strategy;
        info!(strategy = %strategy, "Routing strategy changed");
    }

    /// Get the full config.
    pub fn get_config(&self) -> RegionConfig {
        self.config.read().unwrap().clone()
    }

    // -- Routing logic ------------------------------------------------------

    /// Route a request to the best region.
    ///
    /// `client_lat`/`client_lon`: client geolocation (0,0 if unknown).
    /// `client_id`: opaque client identifier for sticky routing.
    /// `preferred_region`: optional override from client headers.
    ///
    /// Returns the selected region ID.
    pub fn route(
        &self,
        client_lat: f64,
        client_lon: f64,
        client_id: Option<&str>,
        preferred_region: Option<&str>,
    ) -> Option<String> {
        let cfg = self.config.read().unwrap();
        let candidates: Vec<Region> = self.regions
            .iter()
            .filter(|r| {
                if !r.value().enabled { return false; }
                let health = self.health.get(&r.value().id);
                match health {
                    Some(h) => h.value().status != RegionStatus::Unhealthy,
                    None => true,
                }
            })
            .map(|r| r.value().clone())
            .collect();

        if candidates.is_empty() {
            warn!("No healthy regions available for routing");
            return None;
        }

        let selected_idx = match cfg.routing_strategy {
            RegionRoutingStrategy::Nearest => {
                Self::route_nearest_idx(&candidates, client_lat, client_lon)
            }
            RegionRoutingStrategy::LowestLatency => {
                Self::route_lowest_latency_idx(&candidates, &self.health)
            }
            RegionRoutingStrategy::LowestCost => {
                Self::route_lowest_cost_idx(&candidates)
            }
            RegionRoutingStrategy::Balanced => {
                Self::route_balanced_idx(&candidates, client_lat, client_lon, &cfg, &self.health)
            }
            RegionRoutingStrategy::Sticky => {
                self.route_sticky_idx(&candidates, client_id, client_lat, client_lon)
            }
            RegionRoutingStrategy::Failover => {
                self.route_failover_idx(&candidates, preferred_region, &self.health)
            }
        };

        let region_id = selected_idx.map(|i| candidates[i].id.clone());

        // Update sticky cache
        if let (Some(ref rid), Some(cid)) = (&region_id, client_id) {
            self.client_region_cache.insert(cid.to_string(), rid.clone());
        }

        // Cross-region detection
        if let Some(ref rid) = region_id {
            if let Some(cid) = client_id {
                let prev = self.client_region_cache.get(cid).map(|v| v.value().clone());
                if let Some(prev_id) = prev {
                    if prev_id != *rid {
                        self.stats.record_request(true, 0);
                    } else {
                        self.stats.record_request(false, 0);
                    }
                } else {
                    self.stats.record_request(false, 0);
                }
            }
        }

        region_id
    }

    // -- Strategy implementations (index-based) -----------------------------

    fn route_nearest_idx(candidates: &[Region], client_lat: f64, client_lon: f64) -> Option<usize> {
        candidates
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let da = a.distance_km_to(client_lat, client_lon);
                let db = b.distance_km_to(client_lat, client_lon);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
    }

    fn route_lowest_latency_idx(candidates: &[Region], health: &DashMap<String, RegionHealth>) -> Option<usize> {
        candidates
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let la = health.get(&a.id).map(|h| h.value().avg_latency).unwrap_or(Duration::from_secs(999));
                let lb = health.get(&b.id).map(|h| h.value().avg_latency).unwrap_or(Duration::from_secs(999));
                la.cmp(&lb)
            })
            .map(|(i, _)| i)
    }

    fn route_lowest_cost_idx(candidates: &[Region]) -> Option<usize> {
        candidates
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let cost_a = if a.available_capacity > 0 { 1.0 / a.available_capacity as f64 } else { f64::MAX };
                let cost_b = if b.available_capacity > 0 { 1.0 / b.available_capacity as f64 } else { f64::MAX };
                cost_a.partial_cmp(&cost_b).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
    }

    fn route_balanced_idx(
        candidates: &[Region],
        client_lat: f64,
        client_lon: f64,
        cfg: &RegionConfig,
        health: &DashMap<String, RegionHealth>,
    ) -> Option<usize> {
        candidates
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let score_a = Self::balanced_score(a, client_lat, client_lon, cfg, health);
                let score_b = Self::balanced_score(b, client_lat, client_lon, cfg, health);
                score_a.partial_cmp(&score_b).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
    }

    fn balanced_score(region: &Region, client_lat: f64, client_lon: f64, cfg: &RegionConfig, health: &DashMap<String, RegionHealth>) -> f64 {
        // Distance component (normalized: 0..1, lower = better)
        let dist = region.distance_km_to(client_lat, client_lon);
        let dist_score = (dist / 20000.0).min(1.0); // 20k km = max

        // Latency component
        let latency = health.get(&region.id)
            .map(|h| h.value().avg_latency.as_secs_f64())
            .unwrap_or(1.0);
        let latency_score = (latency / cfg.latency_threshold_secs).min(1.0);

        // Capacity component (inverse: more capacity = lower score)
        let cap_score = if region.available_capacity > 0 {
            1.0 - (region.available_capacity as f64 / 10000.0).min(1.0)
        } else {
            1.0
        };

        // Weighted: latency 40%, distance 30%, capacity 30%
        0.4 * latency_score + 0.3 * dist_score + 0.3 * cap_score
    }

    fn route_sticky_idx(
        &self,
        candidates: &[Region],
        client_id: Option<&str>,
        client_lat: f64,
        client_lon: f64,
    ) -> Option<usize> {
        // Try sticky first
        if let Some(cid) = client_id {
            if let Some(sticky) = self.client_region_cache.get(cid) {
                let sticky_id = sticky.value().clone();
                if let Some(idx) = candidates.iter().position(|r| r.id == sticky_id) {
                    // Verify region is still healthy
                    if let Some(h) = self.health.get(&sticky_id) {
                        if h.value().status == RegionStatus::Healthy {
                            return Some(idx);
                        }
                    }
                }
            }
        }
        // Fallback to nearest
        Self::route_nearest_idx(candidates, client_lat, client_lon)
    }

    fn route_failover_idx(
        &self,
        candidates: &[Region],
        preferred_region: Option<&str>,
        health: &DashMap<String, RegionHealth>,
    ) -> Option<usize> {
        // Try preferred region first
        if let Some(pref) = preferred_region {
            if let Some(idx) = candidates.iter().position(|r| r.id == pref) {
                if let Some(h) = health.get(&candidates[idx].id) {
                    if h.value().status == RegionStatus::Healthy {
                        return Some(idx);
                    }
                }
            }
        }

        // Try primary (lowest priority number = highest priority)
        if let Some((idx, primary)) = candidates.iter().enumerate().min_by_key(|(_, r)| r.priority) {
            if let Some(h) = health.get(&primary.id) {
                if h.value().status == RegionStatus::Healthy {
                    return Some(idx);
                }
            }
        }

        // Failover to any healthy region
        candidates.iter().position(|r| {
            health.get(&r.id)
                .map(|h| h.value().status == RegionStatus::Healthy)
                .unwrap_or(false)
        })
    }

    // -- Health monitoring ---------------------------------------------------

    /// Record a successful request to a region.
    pub fn record_success(&self, region_id: &str, latency: Duration) {
        if let Some(mut h) = self.health.get_mut(region_id) {
            h.total_requests += 1;
            // EMA for avg latency
            let alpha = if h.total_requests <= 1 { 1.0 } else { 2.0 / (h.total_requests as f64 + 1.0) };
            let current_ms = h.avg_latency.as_millis() as f64;
            let new_ms = latency.as_millis() as f64;
            h.avg_latency = Duration::from_millis(
                (current_ms * (1.0 - alpha) + new_ms * alpha).round() as u64
            );
            h.error_rate = if h.total_requests > 0 {
                h.total_errors as f64 / h.total_requests as f64
            } else {
                0.0
            };
            h.last_check = Utc::now();
            // Auto-recover from Degraded/Unhealthy
            if h.status == RegionStatus::Degraded && h.error_rate < 0.1 {
                h.status = RegionStatus::Healthy;
                info!(region = %region_id, "Region recovered to Healthy");
            }
        }
    }

    /// Record a failed request to a region.
    pub fn record_error(&self, region_id: &str) {
        if let Some(mut h) = self.health.get_mut(region_id) {
            h.total_requests += 1;
            h.total_errors += 1;
            h.error_rate = h.total_errors as f64 / h.total_requests as f64;
            h.last_check = Utc::now();
            // Auto-degrade
            if h.error_rate > 0.5 && h.total_requests > 10 {
                h.status = RegionStatus::Unhealthy;
                warn!(region = %region_id, error_rate = h.error_rate, "Region marked Unhealthy");
            } else if h.error_rate > 0.2 && h.total_requests > 5 {
                h.status = RegionStatus::Degraded;
                warn!(region = %region_id, error_rate = h.error_rate, "Region marked Degraded");
            }
        }
    }

    /// Start background health check task.
    pub fn start_health_check_task(self: &std::sync::Arc<Self>, client: reqwest::Client) -> tokio::task::JoinHandle<()> {
        let router = self.clone();
        tokio::spawn(async move {
            loop {
                let interval_secs = {
                    let cfg = router.config.read().unwrap();
                    cfg.health_check_interval_secs
                };
                tokio::time::sleep(Duration::from_secs(interval_secs)).await;
                router.run_health_checks(&client).await;
            }
        })
    }

    async fn run_health_checks(&self, client: &reqwest::Client) {
        let threshold = {
            let cfg = self.config.read().unwrap();
            Duration::from_secs_f64(cfg.latency_threshold_secs)
        };

        let regions: Vec<String> = self.regions.iter().map(|r| r.key().clone()).collect();

        for region_id in &regions {
            let region = match self.regions.get(region_id) {
                Some(r) => r.value().clone(),
                None => continue,
            };

            if !region.enabled {
                continue;
            }

            // Probe the region endpoint
            let url = format!("{}/health", region.endpoint.trim_end_matches('/'));
            let start = std::time::Instant::now();
            let result = client
                .get(&url)
                .timeout(Duration::from_secs(5))
                .send()
                .await;

            match result {
                Ok(resp) if resp.status().is_success() => {
                    let latency = start.elapsed();
                    self.record_success(region_id, latency);

                    if latency > threshold {
                        if let Some(mut h) = self.health.get_mut(region_id) {
                            if h.status == RegionStatus::Healthy {
                                h.status = RegionStatus::Degraded;
                                warn!(region = %region_id, latency_ms = latency.as_millis(), "Region degraded (high latency)");
                            }
                        }
                    }
                }
                Ok(resp) => {
                    warn!(region = %region_id, status = %resp.status(), "Region health check failed");
                    self.record_error(region_id);
                }
                Err(e) => {
                    warn!(region = %region_id, error = %e, "Region health check error");
                    self.record_error(region_id);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Region update (partial)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RegionUpdate {
    pub name: Option<String>,
    pub endpoint: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub priority: Option<u8>,
    pub enabled: Option<bool>,
    pub available_capacity: Option<u64>,
}

// ---------------------------------------------------------------------------
// Haversine
// ---------------------------------------------------------------------------

fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6371.0;

    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();

    let a = (dlat / 2.0).sin() * (dlat / 2.0).sin()
        + lat1.to_radians().cos()
            * lat2.to_radians().cos()
            * (dlon / 2.0).sin()
            * (dlon / 2.0).sin();

    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());

    EARTH_RADIUS_KM * c
}

// ---------------------------------------------------------------------------
// Axum handlers
// ---------------------------------------------------------------------------

use axum::{
    extract::{
        Path,
        State
    },
    response::IntoResponse,
    routing::{get, patch, post},
    Json, Router,
};

/// Build the multi-region router.
pub fn build_region_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/regions", get(list_regions_handler))
        .route("/api/regions/{id}", get(get_region_handler))
        .route("/api/regions/{id}", patch(update_region_handler))
        .route("/api/regions/{id}/drain", post(drain_region_handler))
        .route("/api/regions/{id}/activate", post(activate_region_handler))
        .route("/api/regions/routing/stats", get(routing_stats_handler))
        .route("/api/regions/routing/strategy", patch(set_routing_strategy_handler))
        .route("/api/regions/health", get(regions_health_handler))
        .with_state(state)
}

/// GET /api/regions — list all regions with health status
async fn list_regions_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let router = match &state.multi_region_router {
        Some(r) => r,
        None => {
            return Json(serde_json::json!({
                "regions": [],
                "error": "multi_region_router not initialized"
            }));
        }
    };

    let regions = router.list_regions();
    let health = router.get_all_health();
    let health_map: std::collections::HashMap<_, _> = health
        .into_iter()
        .map(|h| (h.region_id.clone(), h))
        .collect();

    let enriched: Vec<serde_json::Value> = regions.iter().map(|r| {
        let h = health_map.get(&r.id);
        serde_json::json!({
            "id": r.id,
            "name": r.name,
            "endpoint": r.endpoint,
            "latitude": r.latitude,
            "longitude": r.longitude,
            "priority": r.priority,
            "enabled": r.enabled,
            "provider_count": r.provider_count,
            "available_capacity": r.available_capacity,
            "health": h,
        })
    }).collect();

    Json(serde_json::json!({ "regions": enriched }))
}

/// GET /api/regions/{id} — region detail with providers
async fn get_region_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match &state.multi_region_router {
        Some(router) => {
            match router.get_region(&id) {
                Some(region) => {
                    let health = router.get_health(&id);
                    (axum::http::StatusCode::OK, axum::Json(serde_json::json!({
                        "region": region,
                        "health": health,
                    })))
                }
                None => (axum::http::StatusCode::NOT_FOUND, axum::Json(serde_json::json!({
                    "error": format!("region {} not found", id)
                }))),
            }
        }
        None => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(serde_json::json!({ "error": "multi_region_router not initialized" })),
        ),
    }
}

/// PATCH /api/regions/{id} — update region config
async fn update_region_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    match &state.multi_region_router {
        Some(router) => {
            let update = RegionUpdate {
                name: body.get("name").and_then(|v| v.as_str()).map(String::from),
                endpoint: body.get("endpoint").and_then(|v| v.as_str()).map(String::from),
                latitude: body.get("latitude").and_then(|v| v.as_f64()),
                longitude: body.get("longitude").and_then(|v| v.as_f64()),
                priority: body.get("priority").and_then(|v| v.as_u64().map(|n| n as u8)),
                enabled: body.get("enabled").and_then(|v| v.as_bool()),
                available_capacity: body.get("available_capacity").and_then(|v| v.as_u64()),
            };

            if router.update_region(&id, update) {
                let region = router.get_region(&id);
                (axum::http::StatusCode::OK, axum::Json(serde_json::json!({
                    "updated": true,
                    "region": region,
                })))
            } else {
                (axum::http::StatusCode::NOT_FOUND, axum::Json(serde_json::json!({
                    "error": format!("region {} not found", id)
                })))
            }
        }
        None => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(serde_json::json!({ "error": "multi_region_router not initialized" })),
        ),
    }
}

/// POST /api/regions/{id}/drain — start draining
async fn drain_region_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match &state.multi_region_router {
        Some(router) => {
            if router.start_drain(&id) {
                (axum::http::StatusCode::OK, axum::Json(serde_json::json!({
                    "region_id": id,
                    "status": "draining",
                })))
            } else {
                (axum::http::StatusCode::NOT_FOUND, axum::Json(serde_json::json!({
                    "error": format!("region {} not found", id)
                })))
            }
        }
        None => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(serde_json::json!({ "error": "multi_region_router not initialized" })),
        ),
    }
}

/// POST /api/regions/{id}/activate — activate drained region
async fn activate_region_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match &state.multi_region_router {
        Some(router) => {
            if router.activate(&id) {
                (axum::http::StatusCode::OK, axum::Json(serde_json::json!({
                    "region_id": id,
                    "status": "healthy",
                })))
            } else {
                (axum::http::StatusCode::NOT_FOUND, axum::Json(serde_json::json!({
                    "error": format!("region {} not found", id)
                })))
            }
        }
        None => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(serde_json::json!({ "error": "multi_region_router not initialized" })),
        ),
    }
}

/// GET /api/regions/routing/stats — routing statistics
async fn routing_stats_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    match &state.multi_region_router {
        Some(router) => {
            let stats = router.get_stats();
            Json(serde_json::json!({
                "strategy": router.get_routing_strategy().to_string(),
                "stats": stats,
            }))
        }
        None => Json(serde_json::json!({
            "error": "multi_region_router not initialized"
        })),
    }
}

/// PATCH /api/regions/routing/strategy — change routing strategy
async fn set_routing_strategy_handler(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let strategy_str = body.get("strategy").and_then(|v| v.as_str()).unwrap_or("");
    let strategy = match strategy_str {
        "nearest" => RegionRoutingStrategy::Nearest,
        "lowest_latency" => RegionRoutingStrategy::LowestLatency,
        "lowest_cost" => RegionRoutingStrategy::LowestCost,
        "balanced" => RegionRoutingStrategy::Balanced,
        "sticky" => RegionRoutingStrategy::Sticky,
        "failover" => RegionRoutingStrategy::Failover,
        _ => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({
                    "error": format!("unknown strategy: {}", strategy_str),
                    "valid_strategies": ["nearest", "lowest_latency", "lowest_cost", "balanced", "sticky", "failover"],
                })),
            );
        }
    };

    match &state.multi_region_router {
        Some(router) => {
            router.set_routing_strategy(strategy);
            (axum::http::StatusCode::OK, axum::Json(serde_json::json!({
                "strategy": strategy.to_string(),
            })))
        }
        None => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(serde_json::json!({ "error": "multi_region_router not initialized" })),
        ),
    }
}

/// GET /api/regions/health — health status of all regions
async fn regions_health_handler(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    match &state.multi_region_router {
        Some(router) => {
            let health = router.get_all_health();
            Json(serde_json::json!({ "regions": health }))
        }
        None => Json(serde_json::json!({
            "error": "multi_region_router not initialized"
        })),
    }
}
