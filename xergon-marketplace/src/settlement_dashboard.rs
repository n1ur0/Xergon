use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ================================================================
// Types
// ================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SettlementEntry {
    pub id: String,
    pub tx_id: String,
    pub provider_id: String,
    pub amount: u64,
    pub token_id: Option<String>,
    pub status: String,
    pub confirmations: u32,
    pub required_confirmations: u32,
    pub submitted_at: i64,
    pub confirmed_at: Option<i64>,
    pub finalized_at: Option<i64>,
    pub value_erg: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HeatmapBucket {
    pub range: String,
    pub count: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConfirmationHeatmap {
    pub buckets: Vec<HeatmapBucket>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProviderSettlementStats {
    pub provider_id: String,
    pub total_settlements: u64,
    pub total_value: u64,
    pub success_rate: f64,
    pub avg_confirmations: f64,
    pub avg_settlement_time_ms: u64,
    pub last_settlement_at: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SettlementAnalytics {
    pub total_value_settled_24h: u64,
    pub total_value_settled_7d: u64,
    pub avg_confirmation_time: f64,
    pub settlement_velocity: f64,
    pub success_rate: f64,
    pub rollback_rate: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SettlementDashboardConfig {
    pub refresh_interval_ms: u64,
    pub max_history_items: u32,
    pub heatmap_bucket_size: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ValueFlowPoint {
    pub timestamp: i64,
    pub value: u64,
    pub count: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StatusGroup {
    pub status: String,
    pub count: u64,
    pub total_value: u64,
}

// ================================================================
// Query parameter structs
// ================================================================

#[derive(Deserialize, Clone, Debug)]
pub struct ListSettlementsQuery {
    pub provider_id: Option<String>,
    pub status: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct AnalyticsQuery {
    pub from: Option<i64>,
    pub to: Option<i64>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct RecentQuery {
    pub limit: Option<u32>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ValueFlowQuery {
    pub from: Option<i64>,
    pub to: Option<i64>,
}

// ================================================================
// SettlementDashboard
// ================================================================

pub struct SettlementDashboard {
    settlements: DashMap<String, SettlementEntry>,
    config: SettlementDashboardConfig,
}

impl SettlementDashboard {
    /// Create a new settlement dashboard with default configuration.
    pub fn new() -> Self {
        Self::with_config(SettlementDashboardConfig {
            refresh_interval_ms: 30_000,
            max_history_items: 10_000,
            heatmap_bucket_size: 5,
        })
    }

    /// Create a new settlement dashboard with the given configuration.
    pub fn with_config(config: SettlementDashboardConfig) -> Self {
        Self {
            settlements: DashMap::new(),
            config,
        }
    }

    /// Record a new settlement.
    pub fn record_settlement(&self, entry: SettlementEntry) {
        self.settlements.insert(entry.id.clone(), entry);
    }

    /// Update a settlement's confirmations and status.
    pub fn update_settlement(
        &self,
        id: &str,
        confirmations: u32,
        status: &str,
    ) -> bool {
        if let Some(mut entry) = self.settlements.get_mut(id) {
            entry.confirmations = confirmations;
            entry.status = status.to_string();
            if status == "confirmed" && entry.confirmed_at.is_none() {
                entry.confirmed_at = Some(Utc::now().timestamp_millis());
            }
            if status == "finalized" && entry.finalized_at.is_none() {
                entry.finalized_at = Some(Utc::now().timestamp_millis());
            }
            true
        } else {
            false
        }
    }

    /// Get a settlement by ID.
    pub fn get_settlement(&self, id: &str) -> Option<SettlementEntry> {
        self.settlements.get(id).map(|e| e.clone())
    }

    /// List settlements with optional filters.
    pub fn list_settlements(
        &self,
        provider_id: Option<&str>,
        status: Option<&str>,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Vec<SettlementEntry> {
        let max = limit.unwrap_or(50) as usize;
        let skip = offset.unwrap_or(0) as usize;
        let mut results: Vec<SettlementEntry> = self
            .settlements
            .iter()
            .filter(|e| {
                if let Some(pid) = provider_id {
                    if e.value().provider_id != pid {
                        return false;
                    }
                }
                if let Some(s) = status {
                    if e.value().status != s {
                        return false;
                    }
                }
                true
            })
            .map(|e| e.value().clone())
            .collect();

        // Sort by submitted_at descending (most recent first)
        results.sort_by(|a, b| b.submitted_at.cmp(&a.submitted_at));

        results.into_iter().skip(skip).take(max).collect()
    }

    /// Get the confirmation distribution heatmap.
    pub fn get_confirmation_heatmap(&self) -> ConfirmationHeatmap {
        let bucket_size = self.config.heatmap_bucket_size as u32;
        let mut bucket_counts: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();

        for entry in self.settlements.iter() {
            let confs = entry.value().confirmations;
            let bucket_start = (confs / bucket_size) * bucket_size;
            let bucket_end = bucket_start + bucket_size - 1;
            let range = format!("{}-{}", bucket_start, bucket_end);
            *bucket_counts.entry(range).or_insert(0) += 1;
        }

        // Sort buckets by range
        let mut buckets: Vec<HeatmapBucket> = bucket_counts
            .into_iter()
            .map(|(range, count)| HeatmapBucket { range, count })
            .collect();

        buckets.sort_by(|a, b| {
            let a_start: u32 = a.range.split('-').next().unwrap_or("0").parse().unwrap_or(0);
            let b_start: u32 = b.range.split('-').next().unwrap_or("0").parse().unwrap_or(0);
            a_start.cmp(&b_start)
        });

        ConfirmationHeatmap { buckets }
    }

    /// Get settlement statistics for a specific provider.
    pub fn get_provider_stats(&self, provider_id: &str) -> ProviderSettlementStats {
        let entries: Vec<SettlementEntry> = self
            .settlements
            .iter()
            .filter(|e| e.value().provider_id == provider_id)
            .map(|e| e.value().clone())
            .collect();

        let total_settlements = entries.len() as u64;
        let total_value: u64 = entries.iter().map(|e| e.amount).sum();
        let success_count = entries
            .iter()
            .filter(|e| e.status == "confirmed" || e.status == "finalized")
            .count();
        let success_rate = if total_settlements > 0 {
            (success_count as f64 / total_settlements as f64) * 100.0
        } else {
            0.0
        };

        let total_confs: u32 = entries.iter().map(|e| e.confirmations).sum();
        let avg_confirmations = if total_settlements > 0 {
            total_confs as f64 / total_settlements as f64
        } else {
            0.0
        };

        let mut settlement_times: Vec<i64> = Vec::new();
        for entry in &entries {
            if let Some(confirmed) = entry.confirmed_at {
                settlement_times.push(confirmed - entry.submitted_at);
            }
        }
        let avg_settlement_time_ms = if !settlement_times.is_empty() {
            (settlement_times.iter().sum::<i64>() / settlement_times.len() as i64) as u64
        } else {
            0
        };

        let last_settlement_at = entries
            .iter()
            .map(|e| e.submitted_at)
            .max();

        ProviderSettlementStats {
            provider_id: provider_id.to_string(),
            total_settlements,
            total_value,
            success_rate,
            avg_confirmations,
            avg_settlement_time_ms,
            last_settlement_at,
        }
    }

    /// Get settlement analytics.
    pub fn get_analytics(&self, from: Option<i64>, to: Option<i64>) -> SettlementAnalytics {
        let now = Utc::now().timestamp_millis();
        let from_ts = from.unwrap_or(now - 86_400_000); // default 24h
        let to_ts = to.unwrap_or(now);

        let entries: Vec<SettlementEntry> = self
            .settlements
            .iter()
            .filter(|e| {
                let ts = e.value().submitted_at;
                ts >= from_ts && ts <= to_ts
            })
            .map(|e| e.value().clone())
            .collect();

        let total_value_settled_24h = entries
            .iter()
            .filter(|e| {
                e.status == "confirmed"
                    || e.status == "finalized"
                    && e.submitted_at >= (now - 86_400_000)
            })
            .map(|e| e.amount)
            .sum();

        let total_value_settled_7d = entries
            .iter()
            .filter(|e| {
                (e.status == "confirmed" || e.status == "finalized")
                    && e.submitted_at >= (now - 604_800_000)
            })
            .map(|e| e.amount)
            .sum();

        // Average confirmation time (ms from submitted to confirmed)
        let mut conf_times: Vec<i64> = Vec::new();
        for entry in &entries {
            if let Some(confirmed) = entry.confirmed_at {
                conf_times.push(confirmed - entry.submitted_at);
            }
        }
        let avg_confirmation_time = if !conf_times.is_empty() {
            conf_times.iter().sum::<i64>() as f64 / conf_times.len() as f64
        } else {
            0.0
        };

        // Settlement velocity: settlements per hour
        let time_range_hours = if to_ts > from_ts {
            ((to_ts - from_ts) as f64) / 3_600_000.0
        } else {
            1.0
        };
        let settlement_velocity = entries.len() as f64 / time_range_hours;

        let success_count = entries
            .iter()
            .filter(|e| e.status == "confirmed" || e.status == "finalized")
            .count();
        let success_rate = if !entries.is_empty() {
            (success_count as f64 / entries.len() as f64) * 100.0
        } else {
            0.0
        };

        let rollback_count = entries.iter().filter(|e| e.status == "rolled_back").count();
        let rollback_rate = if !entries.is_empty() {
            (rollback_count as f64 / entries.len() as f64) * 100.0
        } else {
            0.0
        };

        SettlementAnalytics {
            total_value_settled_24h,
            total_value_settled_7d,
            avg_confirmation_time,
            settlement_velocity,
            success_rate,
            rollback_rate,
        }
    }

    /// Get the most recent settlements.
    pub fn get_recent_settlements(&self, limit: u32) -> Vec<SettlementEntry> {
        let mut entries: Vec<SettlementEntry> = self
            .settlements
            .iter()
            .map(|e| e.value().clone())
            .collect();

        entries.sort_by(|a, b| b.submitted_at.cmp(&a.submitted_at));
        entries.truncate(limit as usize);
        entries
    }

    /// Get settlements grouped by status.
    pub fn get_settlements_by_status(&self) -> Vec<StatusGroup> {
        let mut groups: std::collections::HashMap<String, (u64, u64)> =
            std::collections::HashMap::new();

        for entry in self.settlements.iter() {
            let e = entry.value();
            let (count, value) = groups.entry(e.status.clone()).or_insert((0, 0));
            *count += 1;
            *value += e.amount;
        }

        let mut result: Vec<StatusGroup> = groups
            .into_iter()
            .map(|(status, (count, total_value))| StatusGroup {
                status,
                count,
                total_value,
            })
            .collect();

        result.sort_by(|a, b| b.count.cmp(&a.count));
        result
    }

    /// Get value flow over time.
    pub fn get_value_flow(&self, from: Option<i64>, to: Option<i64>) -> Vec<ValueFlowPoint> {
        let now = Utc::now().timestamp_millis();
        let from_ts = from.unwrap_or(now - 86_400_000);
        let to_ts = to.unwrap_or(now);

        let bucket_ms = 3_600_000i64; // 1 hour buckets
        let mut flow: std::collections::HashMap<i64, (u64, u64)> =
            std::collections::HashMap::new();

        for entry in self.settlements.iter() {
            let e = entry.value();
            if e.submitted_at < from_ts || e.submitted_at > to_ts {
                continue;
            }
            let bucket = (e.submitted_at / bucket_ms) * bucket_ms;
            let (value, count) = flow.entry(bucket).or_insert((0, 0));
            *value += e.amount;
            *count += 1;
        }

        let mut points: Vec<ValueFlowPoint> = flow
            .into_iter()
            .map(|(timestamp, (value, count))| ValueFlowPoint {
                timestamp,
                value,
                count,
            })
            .collect();

        points.sort_by_key(|p| p.timestamp);
        points
    }
}

impl Default for SettlementDashboard {
    fn default() -> Self {
        Self::new()
    }
}

// ================================================================
// App state for axum
// ================================================================

pub type SharedSettlementDashboard = Arc<SettlementDashboard>;

#[derive(Clone)]
pub struct SettlementState {
    pub dashboard: SharedSettlementDashboard,
}

// ================================================================
// REST Handlers
// ================================================================

async fn get_settlement_detail(
    State(state): State<SettlementState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.dashboard.get_settlement(&id) {
        Some(entry) => Json(serde_json::to_value(entry).unwrap_or(serde_json::json!({}))),
        None => Json(serde_json::json!({"error": "Settlement not found", "id": id})),
    }
}

async fn list_settlements(
    State(state): State<SettlementState>,
    Query(q): Query<ListSettlementsQuery>,
) -> Json<Vec<SettlementEntry>> {
    let results = state.dashboard.list_settlements(
        q.provider_id.as_deref(),
        q.status.as_deref(),
        q.limit,
        q.offset,
    );
    Json(results)
}

async fn get_heatmap(
    State(state): State<SettlementState>,
) -> Json<ConfirmationHeatmap> {
    Json(state.dashboard.get_confirmation_heatmap())
}

async fn get_provider_stats(
    State(state): State<SettlementState>,
    Path(provider_id): Path<String>,
) -> Json<ProviderSettlementStats> {
    Json(state.dashboard.get_provider_stats(&provider_id))
}

async fn get_analytics(
    State(state): State<SettlementState>,
    Query(q): Query<AnalyticsQuery>,
) -> Json<SettlementAnalytics> {
    Json(state.dashboard.get_analytics(q.from, q.to))
}

async fn get_recent(
    State(state): State<SettlementState>,
    Query(q): Query<RecentQuery>,
) -> Json<Vec<SettlementEntry>> {
    Json(state.dashboard.get_recent_settlements(q.limit.unwrap_or(20)))
}

async fn get_value_flow(
    State(state): State<SettlementState>,
    Query(q): Query<ValueFlowQuery>,
) -> Json<Vec<ValueFlowPoint>> {
    Json(state.dashboard.get_value_flow(q.from, q.to))
}

async fn get_by_status(
    State(state): State<SettlementState>,
) -> Json<Vec<StatusGroup>> {
    Json(state.dashboard.get_settlements_by_status())
}

// ================================================================
// Router
// ================================================================

pub fn settlement_router(dashboard: SharedSettlementDashboard) -> Router {
    let state = SettlementState { dashboard };
    Router::new()
        .route("/v1/settlements", get(list_settlements))
        .route("/v1/settlements/heatmap", get(get_heatmap))
        .route("/v1/settlements/analytics", get(get_analytics))
        .route("/v1/settlements/recent", get(get_recent))
        .route("/v1/settlements/value-flow", get(get_value_flow))
        .route("/v1/settlements/by-status", get(get_by_status))
        .route("/v1/settlements/provider/:provider_id/stats", get(get_provider_stats))
        .route("/v1/settlements/:id", get(get_settlement_detail))
        .with_state(state)
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(id: &str, provider_id: &str, status: &str, confirmations: u32) -> SettlementEntry {
        let now = Utc::now().timestamp_millis();
        SettlementEntry {
            id: id.to_string(),
            tx_id: format!("tx-{}", id),
            provider_id: provider_id.to_string(),
            amount: 1_000_000_000u64,
            token_id: None,
            status: status.to_string(),
            confirmations,
            required_confirmations: 10,
            submitted_at: now - (confirmations as i64 * 120_000),
            confirmed_at: if status == "confirmed" || status == "finalized" {
                Some(now - 60_000)
            } else {
                None
            },
            finalized_at: if status == "finalized" { Some(now) } else { None },
            value_erg: 1.0,
        }
    }

    #[test]
    fn test_record_settlement() {
        let dash = SettlementDashboard::new();
        let entry = make_entry("s1", "p1", "pending", 0);
        dash.record_settlement(entry.clone());

        let retrieved = dash.get_settlement("s1");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, "s1");
    }

    #[test]
    fn test_update_settlement() {
        let dash = SettlementDashboard::new();
        dash.record_settlement(make_entry("s1", "p1", "pending", 0));

        let updated = dash.update_settlement("s1", 5, "confirming");
        assert!(updated);

        let entry = dash.get_settlement("s1").unwrap();
        assert_eq!(entry.confirmations, 5);
        assert_eq!(entry.status, "confirming");
        assert!(entry.confirmed_at.is_none());
    }

    #[test]
    fn test_update_settlement_confirmed() {
        let dash = SettlementDashboard::new();
        dash.record_settlement(make_entry("s1", "p1", "pending", 0));

        dash.update_settlement("s1", 10, "confirmed");

        let entry = dash.get_settlement("s1").unwrap();
        assert!(entry.confirmed_at.is_some());
    }

    #[test]
    fn test_update_nonexistent() {
        let dash = SettlementDashboard::new();
        let result = dash.update_settlement("nonexistent", 5, "confirmed");
        assert!(!result);
    }

    #[test]
    fn test_confirmation_heatmap() {
        let dash = SettlementDashboard::new();
        dash.record_settlement(make_entry("s1", "p1", "confirmed", 3));
        dash.record_settlement(make_entry("s2", "p1", "confirmed", 7));
        dash.record_settlement(make_entry("s3", "p1", "confirmed", 12));

        let heatmap = dash.get_confirmation_heatmap();
        assert!(!heatmap.buckets.is_empty());

        // With bucket_size=5: s1 -> 0-4, s2 -> 5-9, s3 -> 10-14
        let total: u64 = heatmap.buckets.iter().map(|b| b.count).sum();
        assert_eq!(total, 3);
    }

    #[test]
    fn test_provider_stats() {
        let dash = SettlementDashboard::new();
        dash.record_settlement(make_entry("s1", "p1", "confirmed", 10));
        dash.record_settlement(make_entry("s2", "p1", "confirmed", 15));
        dash.record_settlement(make_entry("s3", "p1", "failed", 2));

        let stats = dash.get_provider_stats("p1");
        assert_eq!(stats.total_settlements, 3);
        assert_eq!(stats.total_value, 3_000_000_000);
        assert!((stats.success_rate - 66.666).abs() < 0.01);
        assert!(stats.avg_confirmations > 0.0);
    }

    #[test]
    fn test_analytics() {
        let dash = SettlementDashboard::new();
        dash.record_settlement(make_entry("s1", "p1", "confirmed", 10));
        dash.record_settlement(make_entry("s2", "p2", "rolled_back", 3));

        let analytics = dash.get_analytics(None, None);
        assert!(analytics.total_value_settled_24h > 0);
        assert!(analytics.rollback_rate > 0.0);
    }

    #[test]
    fn test_recent_settlements() {
        let dash = SettlementDashboard::new();
        for i in 0..10 {
            dash.record_settlement(make_entry(&format!("s{}", i), "p1", "confirmed", 10));
        }

        let recent = dash.get_recent_settlements(3);
        assert_eq!(recent.len(), 3);
    }

    #[test]
    fn test_list_with_filters() {
        let dash = SettlementDashboard::new();
        dash.record_settlement(make_entry("s1", "p1", "confirmed", 10));
        dash.record_settlement(make_entry("s2", "p2", "pending", 3));
        dash.record_settlement(make_entry("s3", "p1", "pending", 1));

        // Filter by provider
        let p1 = dash.list_settlements(Some("p1"), None, None, None);
        assert_eq!(p1.len(), 2);

        // Filter by status
        let pending = dash.list_settlements(None, Some("pending"), None, None);
        assert_eq!(pending.len(), 2);

        // Filter by both
        let p1_pending = dash.list_settlements(Some("p1"), Some("pending"), None, None);
        assert_eq!(p1_pending.len(), 1);
    }

    #[test]
    fn test_value_flow() {
        let dash = SettlementDashboard::new();
        dash.record_settlement(make_entry("s1", "p1", "confirmed", 10));
        dash.record_settlement(make_entry("s2", "p1", "confirmed", 10));

        let flow = dash.get_value_flow(None, None);
        // Both should be in the same hour bucket
        assert!(!flow.is_empty());
        let total_value: u64 = flow.iter().map(|p| p.value).sum();
        assert_eq!(total_value, 2_000_000_000);
    }

    #[test]
    fn test_by_status() {
        let dash = SettlementDashboard::new();
        dash.record_settlement(make_entry("s1", "p1", "confirmed", 10));
        dash.record_settlement(make_entry("s2", "p1", "confirmed", 10));
        dash.record_settlement(make_entry("s3", "p1", "pending", 2));

        let groups = dash.get_settlements_by_status();
        assert_eq!(groups.len(), 2); // confirmed, pending
        let confirmed = groups.iter().find(|g| g.status == "confirmed").unwrap();
        assert_eq!(confirmed.count, 2);
    }

    #[test]
    fn test_concurrent_updates() {
        use std::thread;

        let dash = Arc::new(SettlementDashboard::new());
        dash.record_settlement(make_entry("s1", "p1", "pending", 0));

        let mut handles = Vec::new();
        for i in 0..100u32 {
            let d = Arc::clone(&dash);
            handles.push(thread::spawn(move || {
                d.update_settlement("s1", i, "confirming");
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let entry = dash.get_settlement("s1").unwrap();
        assert_eq!(entry.status, "confirming");
        // confirmations should be one of the values written
        assert!(entry.confirmations < 100);
    }

    #[test]
    fn test_empty_dashboard() {
        let dash = SettlementDashboard::new();

        assert!(dash.get_settlement("nonexistent").is_none());
        assert!(dash.list_settlements(None, None, None, None).is_empty());
        assert!(dash.get_recent_settlements(10).is_empty());
        assert!(dash.get_settlements_by_status().is_empty());
        assert!(dash.get_value_flow(None, None).is_empty());

        let heatmap = dash.get_confirmation_heatmap();
        assert!(heatmap.buckets.is_empty());

        let stats = dash.get_provider_stats("nonexistent");
        assert_eq!(stats.total_settlements, 0);
        assert_eq!(stats.success_rate, 0.0);

        let analytics = dash.get_analytics(None, None);
        assert_eq!(analytics.total_value_settled_24h, 0);
        assert_eq!(analytics.success_rate, 0.0);
    }

    #[test]
    fn test_large_volume() {
        let dash = SettlementDashboard::new();
        let count = 5_000u32;

        for i in 0..count {
            let status = if i % 10 == 0 { "failed" } else { "confirmed" };
            let entry = SettlementEntry {
                id: format!("s-{}", i),
                tx_id: format!("tx-{}", i),
                provider_id: format!("p-{}", i % 50),
                amount: 500_000_000 + (i as u64 * 100),
                token_id: None,
                status: status.to_string(),
                confirmations: if status == "confirmed" { 10 } else { 3 },
                required_confirmations: 10,
                submitted_at: Utc::now().timestamp_millis() - (i as i64 * 60_000),
                confirmed_at: if status == "confirmed" {
                    Some(Utc::now().timestamp_millis() - (i as i64 * 30_000))
                } else {
                    None
                },
                finalized_at: None,
                value_erg: 0.5 + (i as f64 * 0.0001),
            };
            dash.record_settlement(entry);
        }

        assert_eq!(dash.list_settlements(None, None, Some(100), None).len(), 100);

        let recent = dash.get_recent_settlements(10);
        assert_eq!(recent.len(), 10);

        let by_status = dash.get_settlements_by_status();
        let confirmed = by_status.iter().find(|g| g.status == "confirmed").unwrap();
        assert_eq!(confirmed.count, (count as u64 * 9) / 10);

        let flow = dash.get_value_flow(None, None);
        assert!(!flow.is_empty());
    }
}
