use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// EarningsRecord
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EarningsRecord {
    pub id: String,
    pub provider_id: String,
    pub amount_nanoerg: u64,
    pub source: String,
    pub model_id: String,
    pub request_id: String,
    pub timestamp: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// ProviderEarnings
// ---------------------------------------------------------------------------

pub struct ProviderEarnings {
    provider_id: String,
    total_nanoerg: AtomicU64,
    daily_earnings: DashMap<String, u64>,
    weekly_earnings: AtomicU64,
    monthly_earnings: AtomicU64,
    pending_settlement: AtomicU64,
    settled: AtomicU64,
}

impl ProviderEarnings {
    fn new(provider_id: &str) -> Self {
        Self {
            provider_id: provider_id.to_string(),
            total_nanoerg: AtomicU64::new(0),
            daily_earnings: DashMap::new(),
            weekly_earnings: AtomicU64::new(0),
            monthly_earnings: AtomicU64::new(0),
            pending_settlement: AtomicU64::new(0),
            settled: AtomicU64::new(0),
        }
    }

    fn record(&self, amount_nanoerg: u64) {
        self.total_nanoerg.fetch_add(amount_nanoerg, Ordering::Relaxed);
        self.weekly_earnings.fetch_add(amount_nanoerg, Ordering::Relaxed);
        self.monthly_earnings.fetch_add(amount_nanoerg, Ordering::Relaxed);
        self.pending_settlement.fetch_add(amount_nanoerg, Ordering::Relaxed);

        let date_key = Utc::now().format("%Y-%m-%d").to_string();
        *self.daily_earnings.entry(date_key).or_insert(0) += amount_nanoerg;
    }

    fn settle(&self, amount_nanoerg: u64) {
        let actual = amount_nanoerg.min(self.pending_settlement.load(Ordering::Relaxed));
        self.pending_settlement.fetch_sub(actual, Ordering::Relaxed);
        self.settled.fetch_add(actual, Ordering::Relaxed);
    }

    fn snapshot(&self) -> EarningsSnapshot {
        let mut daily: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
        for entry in self.daily_earnings.iter() {
            daily.insert(entry.key().clone(), *entry.value());
        }
        EarningsSnapshot {
            provider_id: self.provider_id.clone(),
            total_nanoerg: self.total_nanoerg.load(Ordering::Relaxed),
            daily_earnings: daily,
            weekly_earnings: self.weekly_earnings.load(Ordering::Relaxed),
            monthly_earnings: self.monthly_earnings.load(Ordering::Relaxed),
            pending_settlement: self.pending_settlement.load(Ordering::Relaxed),
            settled: self.settled.load(Ordering::Relaxed),
        }
    }
}

// ---------------------------------------------------------------------------
// EarningsSnapshot
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EarningsSnapshot {
    pub provider_id: String,
    pub total_nanoerg: u64,
    pub daily_earnings: std::collections::HashMap<String, u64>,
    pub weekly_earnings: u64,
    pub monthly_earnings: u64,
    pub pending_settlement: u64,
    pub settled: u64,
}

// ---------------------------------------------------------------------------
// ModelPerformance
// ---------------------------------------------------------------------------

pub struct ModelPerformance {
    model_id: String,
    model_name: String,
    requests_total: AtomicU64,
    tokens_total: AtomicU64,
    avg_latency_ms: AtomicU64,
    error_count: AtomicU64,
    uptime_basis_points: AtomicU64,
    last_request: RwLock<Option<DateTime<Utc>>>,
}

impl ModelPerformance {
    fn new(model_id: &str, model_name: &str) -> Self {
        Self {
            model_id: model_id.to_string(),
            model_name: model_name.to_string(),
            requests_total: AtomicU64::new(0),
            tokens_total: AtomicU64::new(0),
            avg_latency_ms: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
            uptime_basis_points: AtomicU64::new(10000),
            last_request: RwLock::new(None),
        }
    }

    fn record(&self, tokens: u64, latency_ms: u64, is_error: bool) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
        self.tokens_total.fetch_add(tokens, Ordering::Relaxed);

        if is_error {
            self.error_count.fetch_add(1, Ordering::Relaxed);
        }

        // Exponential moving average for latency
        let prev = self.avg_latency_ms.load(Ordering::Relaxed);
        let new_avg = if prev == 0 {
            latency_ms
        } else {
            (prev * 9 + latency_ms) / 10
        };
        self.avg_latency_ms.store(new_avg, Ordering::Relaxed);

        *self.last_request.write().unwrap() = Some(Utc::now());
    }

    fn snapshot(&self) -> ModelPerfSnapshot {
        ModelPerfSnapshot {
            model_id: self.model_id.clone(),
            model_name: self.model_name.clone(),
            requests_total: self.requests_total.load(Ordering::Relaxed),
            tokens_total: self.tokens_total.load(Ordering::Relaxed),
            avg_latency_ms: self.avg_latency_ms.load(Ordering::Relaxed),
            error_count: self.error_count.load(Ordering::Relaxed),
            uptime_pct: self.uptime_basis_points.load(Ordering::Relaxed) as f64 / 10000.0,
            last_request: *self.last_request.read().unwrap(),
        }
    }
}

// ---------------------------------------------------------------------------
// ModelPerfSnapshot
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModelPerfSnapshot {
    pub model_id: String,
    pub model_name: String,
    pub requests_total: u64,
    pub tokens_total: u64,
    pub avg_latency_ms: u64,
    pub error_count: u64,
    pub uptime_pct: f64,
    pub last_request: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// ProviderStatus
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum ProviderStatus {
    Online,
    Offline,
    Degraded,
    Maintenance,
}

impl std::fmt::Display for ProviderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Online => write!(f, "Online"),
            Self::Offline => write!(f, "Offline"),
            Self::Degraded => write!(f, "Degraded"),
            Self::Maintenance => write!(f, "Maintenance"),
        }
    }
}

// ---------------------------------------------------------------------------
// ActivityEntry
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ActivityEntry {
    pub id: String,
    pub provider_id: String,
    pub activity_type: String,
    pub description: String,
    pub timestamp: DateTime<Utc>,
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// DashboardMetrics
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DashboardMetrics {
    pub total_models: u32,
    pub active_models: u32,
    pub total_requests: u64,
    pub total_tokens: u64,
    pub total_earnings_nanoerg: u64,
    pub avg_uptime_pct: f64,
    pub status: ProviderStatus,
}

// ---------------------------------------------------------------------------
// ProviderDashboard
// ---------------------------------------------------------------------------

pub struct ProviderDashboard {
    provider_id: String,
    earnings: ProviderEarnings,
    model_perfs: DashMap<String, Arc<ModelPerformance>>,
    activities: DashMap<String, ActivityEntry>,
    status: RwLock<ProviderStatus>,
}

impl ProviderDashboard {
    pub fn new(provider_id: &str) -> Self {
        Self {
            provider_id: provider_id.to_string(),
            earnings: ProviderEarnings::new(provider_id),
            model_perfs: DashMap::new(),
            activities: DashMap::new(),
            status: RwLock::new(ProviderStatus::Online),
        }
    }

    pub fn record_earning(
        &self,
        amount_nanoerg: u64,
        source: &str,
        model_id: &str,
        request_id: &str,
    ) -> EarningsRecord {
        self.earnings.record(amount_nanoerg);

        EarningsRecord {
            id: uuid::Uuid::new_v4().to_string(),
            provider_id: self.provider_id.clone(),
            amount_nanoerg,
            source: source.to_string(),
            model_id: model_id.to_string(),
            request_id: request_id.to_string(),
            timestamp: Utc::now(),
        }
    }

    pub fn get_earnings(&self) -> EarningsSnapshot {
        self.earnings.snapshot()
    }

    pub fn settle_earnings(&self, amount_nanoerg: u64) {
        self.earnings.settle(amount_nanoerg);
    }

    pub fn register_model(&self, model_id: &str, model_name: &str) -> ModelPerfSnapshot {
        let perf = Arc::new(ModelPerformance::new(model_id, model_name));
        let snap = perf.snapshot();
        self.model_perfs.insert(model_id.to_string(), perf);
        snap
    }

    pub fn record_request(
        &self,
        model_id: &str,
        tokens: u64,
        latency_ms: u64,
        is_error: bool,
    ) -> Result<(), String> {
        let perf = self.model_perfs.get(model_id).ok_or("Model not registered")?;
        perf.record(tokens, latency_ms, is_error);
        Ok(())
    }

    pub fn get_model_performance(&self, model_id: &str) -> Option<ModelPerfSnapshot> {
        self.model_perfs.get(model_id).map(|p| p.snapshot())
    }

    pub fn get_all_model_performance(&self) -> Vec<ModelPerfSnapshot> {
        self.model_perfs.iter().map(|p| p.snapshot()).collect()
    }

    pub fn add_activity(
        &self,
        activity_type: &str,
        description: &str,
        metadata: std::collections::HashMap<String, serde_json::Value>,
    ) -> ActivityEntry {
        let entry = ActivityEntry {
            id: uuid::Uuid::new_v4().to_string(),
            provider_id: self.provider_id.clone(),
            activity_type: activity_type.to_string(),
            description: description.to_string(),
            timestamp: Utc::now(),
            metadata,
        };
        self.activities.insert(entry.id.clone(), entry.clone());
        entry
    }

    pub fn get_activities(&self, limit: usize) -> Vec<ActivityEntry> {
        let mut entries: Vec<ActivityEntry> = self
            .activities
            .iter()
            .map(|e| e.value().clone())
            .collect();
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        entries.truncate(limit);
        entries
    }

    pub fn set_status(&self, status: ProviderStatus) -> ProviderStatus {
        let mut current = self.status.write().unwrap();
        let old = current.clone();
        *current = status;
        old
    }

    pub fn get_status(&self) -> ProviderStatus {
        self.status.read().unwrap().clone()
    }

    pub fn get_metrics(&self) -> DashboardMetrics {
        let total_models = self.model_perfs.len() as u32;
        let mut total_requests: u64 = 0;
        let mut total_tokens: u64 = 0;
        let mut total_uptime_basis: u64 = 0;
        let mut active_count: u32 = 0;

        for perf in self.model_perfs.iter() {
            let snap = perf.snapshot();
            total_requests += snap.requests_total;
            total_tokens += snap.tokens_total;
            total_uptime_basis += (snap.uptime_pct * 10000.0) as u64;
            if snap.requests_total > 0 {
                active_count += 1;
            }
        }

        let avg_uptime = if total_models > 0 {
            total_uptime_basis as f64 / total_models as f64 / 10000.0
        } else {
            0.0
        };

        let earnings = self.earnings.snapshot();

        DashboardMetrics {
            total_models,
            active_models: active_count,
            total_requests,
            total_tokens,
            total_earnings_nanoerg: earnings.total_nanoerg,
            avg_uptime_pct: avg_uptime,
            status: self.get_status(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_dashboard() -> ProviderDashboard {
        ProviderDashboard::new("test-provider")
    }

    #[test]
    fn test_new_dashboard() {
        let dash = make_dashboard();
        assert_eq!(dash.get_status(), ProviderStatus::Online);
        let metrics = dash.get_metrics();
        assert_eq!(metrics.total_models, 0);
        assert_eq!(metrics.total_requests, 0);
    }

    #[test]
    fn test_record_earning() {
        let dash = make_dashboard();
        let record = dash.record_earning(1_000_000_000, "inference", "qwen-4b", "req-1");
        assert_eq!(record.amount_nanoerg, 1_000_000_000);
        assert_eq!(record.provider_id, "test-provider");
        let earnings = dash.get_earnings();
        assert_eq!(earnings.total_nanoerg, 1_000_000_000);
        assert_eq!(earnings.pending_settlement, 1_000_000_000);
        assert_eq!(earnings.settled, 0);
    }

    #[test]
    fn test_record_multiple_earnings() {
        let dash = make_dashboard();
        dash.record_earning(500_000_000, "inference", "qwen-4b", "req-1");
        dash.record_earning(300_000_000, "inference", "qwen-4b", "req-2");
        let earnings = dash.get_earnings();
        assert_eq!(earnings.total_nanoerg, 800_000_000);
        assert_eq!(earnings.weekly_earnings, 800_000_000);
        assert_eq!(earnings.monthly_earnings, 800_000_000);
    }

    #[test]
    fn test_daily_earnings_aggregation() {
        let dash = make_dashboard();
        dash.record_earning(100_000_000, "inference", "m1", "r1");
        dash.record_earning(200_000_000, "inference", "m1", "r2");
        let earnings = dash.get_earnings();
        let today = Utc::now().format("%Y-%m-%d").to_string();
        assert_eq!(earnings.daily_earnings.get(&today), Some(&300_000_000));
    }

    #[test]
    fn test_settle_earnings() {
        let dash = make_dashboard();
        dash.record_earning(1_000_000_000, "inference", "m1", "r1");
        dash.settle_earnings(600_000_000);
        let earnings = dash.get_earnings();
        assert_eq!(earnings.pending_settlement, 400_000_000);
        assert_eq!(earnings.settled, 600_000_000);
    }

    #[test]
    fn test_settle_more_than_pending() {
        let dash = make_dashboard();
        dash.record_earning(500_000_000, "inference", "m1", "r1");
        dash.settle_earnings(999_999_999);
        let earnings = dash.get_earnings();
        assert_eq!(earnings.pending_settlement, 0);
        assert_eq!(earnings.settled, 500_000_000);
    }

    #[test]
    fn test_register_model() {
        let dash = make_dashboard();
        let snap = dash.register_model("qwen-4b", "Qwen 4B");
        assert_eq!(snap.model_id, "qwen-4b");
        assert_eq!(snap.model_name, "Qwen 4B");
        assert_eq!(snap.requests_total, 0);
    }

    #[test]
    fn test_record_request() {
        let dash = make_dashboard();
        dash.register_model("qwen-4b", "Qwen 4B");
        dash.record_request("qwen-4b", 100, 50, false).unwrap();
        let perf = dash.get_model_performance("qwen-4b").unwrap();
        assert_eq!(perf.requests_total, 1);
        assert_eq!(perf.tokens_total, 100);
        assert_eq!(perf.avg_latency_ms, 50);
        assert!(perf.last_request.is_some());
    }

    #[test]
    fn test_record_request_error() {
        let dash = make_dashboard();
        dash.register_model("qwen-4b", "Qwen 4B");
        dash.record_request("qwen-4b", 0, 100, true).unwrap();
        let perf = dash.get_model_performance("qwen-4b").unwrap();
        assert_eq!(perf.error_count, 1);
    }

    #[test]
    fn test_record_request_nonexistent_model() {
        let dash = make_dashboard();
        assert!(dash.record_request("nope", 100, 50, false).is_err());
    }

    #[test]
    fn test_latency_ema() {
        let dash = make_dashboard();
        dash.register_model("m1", "M1");
        dash.record_request("m1", 10, 100, false).unwrap();
        dash.record_request("m1", 10, 200, false).unwrap();
        let perf = dash.get_model_performance("m1").unwrap();
        // EMA: first=100, second=(100*9+200)/10 = 110
        assert_eq!(perf.avg_latency_ms, 110);
    }

    #[test]
    fn test_get_all_model_performance() {
        let dash = make_dashboard();
        dash.register_model("m1", "Model 1");
        dash.register_model("m2", "Model 2");
        let all = dash.get_all_model_performance();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_add_activity() {
        let dash = make_dashboard();
        let entry = dash.add_activity(
            "model_deployed",
            "Deployed Qwen 4B",
            HashMap::new(),
        );
        assert_eq!(entry.activity_type, "model_deployed");
        assert_eq!(entry.provider_id, "test-provider");
    }

    #[test]
    fn test_get_activities() {
        let dash = make_dashboard();
        dash.add_activity("type1", "desc1", HashMap::new());
        dash.add_activity("type2", "desc2", HashMap::new());
        dash.add_activity("type3", "desc3", HashMap::new());
        let activities = dash.get_activities(2);
        assert_eq!(activities.len(), 2);
        // Should be sorted newest first
        assert!(activities[0].timestamp >= activities[1].timestamp);
    }

    #[test]
    fn test_set_status() {
        let dash = make_dashboard();
        let old = dash.set_status(ProviderStatus::Maintenance);
        assert_eq!(old, ProviderStatus::Online);
        assert_eq!(dash.get_status(), ProviderStatus::Maintenance);
    }

    #[test]
    fn test_provider_status_display() {
        assert_eq!(ProviderStatus::Online.to_string(), "Online");
        assert_eq!(ProviderStatus::Offline.to_string(), "Offline");
        assert_eq!(ProviderStatus::Degraded.to_string(), "Degraded");
        assert_eq!(ProviderStatus::Maintenance.to_string(), "Maintenance");
    }

    #[test]
    fn test_get_metrics() {
        let dash = make_dashboard();
        dash.register_model("m1", "Model 1");
        dash.register_model("m2", "Model 2");
        dash.record_earning(1_000_000_000, "inference", "m1", "r1");
        dash.record_request("m1", 500, 100, false).unwrap();
        let metrics = dash.get_metrics();
        assert_eq!(metrics.total_models, 2);
        assert_eq!(metrics.total_requests, 1);
        assert_eq!(metrics.total_tokens, 500);
        assert_eq!(metrics.total_earnings_nanoerg, 1_000_000_000);
        assert_eq!(metrics.active_models, 1);
    }

    #[test]
    fn test_earnings_snapshot_serialization() {
        let snap = EarningsSnapshot {
            provider_id: "p1".to_string(),
            total_nanoerg: 100,
            daily_earnings: HashMap::new(),
            weekly_earnings: 50,
            monthly_earnings: 100,
            pending_settlement: 30,
            settled: 70,
        };
        let json = serde_json::to_string(&snap).unwrap();
        let back: EarningsSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back.provider_id, "p1");
    }

    #[test]
    fn test_model_perf_snapshot_serialization() {
        let snap = ModelPerfSnapshot {
            model_id: "m1".to_string(),
            model_name: "Model 1".to_string(),
            requests_total: 10,
            tokens_total: 500,
            avg_latency_ms: 50,
            error_count: 1,
            uptime_pct: 0.999,
            last_request: None,
        };
        let json = serde_json::to_string(&snap).unwrap();
        let back: ModelPerfSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back.model_id, "m1");
    }

    #[test]
    fn test_dashboard_metrics_serialization() {
        let m = DashboardMetrics {
            total_models: 3,
            active_models: 2,
            total_requests: 1000,
            total_tokens: 50000,
            total_earnings_nanoerg: 999,
            avg_uptime_pct: 0.99,
            status: ProviderStatus::Online,
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: DashboardMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(back.total_models, 3);
        assert_eq!(back.status, ProviderStatus::Online);
    }

    #[test]
    fn test_multiple_providers_independent() {
        let dash1 = ProviderDashboard::new("provider-1");
        let dash2 = ProviderDashboard::new("provider-2");
        dash1.record_earning(100, "inference", "m1", "r1");
        dash2.record_earning(200, "inference", "m2", "r2");
        assert_eq!(dash1.get_earnings().total_nanoerg, 100);
        assert_eq!(dash2.get_earnings().total_nanoerg, 200);
    }
}
