use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// EventType
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum EventType {
    Inference,
    Training,
    Embedding,
    FineTune,
    HealthCheck,
    Other,
}

impl EventType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Inference => "Inference",
            Self::Training => "Training",
            Self::Embedding => "Embedding",
            Self::FineTune => "FineTune",
            Self::HealthCheck => "HealthCheck",
            Self::Other => "Other",
        }
    }
}

// ---------------------------------------------------------------------------
// AnalyticsBucket
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum AnalyticsBucket {
    Hourly,
    Daily,
    Weekly,
    Monthly,
}

impl AnalyticsBucket {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Hourly => "Hourly",
            Self::Daily => "Daily",
            Self::Weekly => "Weekly",
            Self::Monthly => "Monthly",
        }
    }

    pub fn duration(&self) -> Duration {
        match self {
            Self::Hourly => Duration::hours(1),
            Self::Daily => Duration::days(1),
            Self::Weekly => Duration::weeks(1),
            Self::Monthly => Duration::days(30),
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "hourly" | "Hourly" => Some(Self::Hourly),
            "daily" | "Daily" => Some(Self::Daily),
            "weekly" | "Weekly" => Some(Self::Weekly),
            "monthly" | "Monthly" => Some(Self::Monthly),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// UsageEvent
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UsageEvent {
    pub event_id: String,
    pub timestamp: DateTime<Utc>,
    pub user_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub event_type: EventType,
    pub tokens_used: u64,
    pub latency_ms: u64,
    pub cost: f64,
    pub region: String,
    pub status_code: u16,
    pub error_message: Option<String>,
}

// ---------------------------------------------------------------------------
// AggregatedUsage
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AggregatedUsage {
    pub bucket: AnalyticsBucket,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub total_requests: u64,
    pub total_tokens: u64,
    pub total_cost: f64,
    pub avg_latency: f64,
    pub p50_latency: f64,
    pub p95_latency: f64,
    pub p99_latency: f64,
    pub unique_users: usize,
    pub unique_providers: usize,
    pub error_count: u64,
    pub error_rate: f64,
    pub by_model: HashMap<String, ModelUsage>,
    pub by_region: HashMap<String, RegionUsage>,
    pub by_event_type: HashMap<String, u64>,
}

// ---------------------------------------------------------------------------
// ModelUsage
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModelUsage {
    pub model_id: String,
    pub requests: u64,
    pub tokens: u64,
    pub cost: f64,
    pub avg_latency: f64,
}

// ---------------------------------------------------------------------------
// RegionUsage
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RegionUsage {
    pub region: String,
    pub requests: u64,
    pub tokens: u64,
    pub cost: f64,
    pub avg_latency: f64,
}

// ---------------------------------------------------------------------------
// TrendPoint
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TrendPoint {
    pub period: String,
    pub timestamp: DateTime<Utc>,
    pub requests: u64,
    pub tokens: u64,
    pub cost: f64,
    pub change_pct: f64,
}

// ---------------------------------------------------------------------------
// ModelRanking
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModelRanking {
    pub model_id: String,
    pub rank: usize,
    pub total_requests: u64,
    pub total_tokens: u64,
    pub total_cost: f64,
    pub avg_latency: f64,
    pub unique_users: usize,
    pub score: f64,
}

// ---------------------------------------------------------------------------
// UserRanking
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UserRanking {
    pub user_id: String,
    pub rank: usize,
    pub total_requests: u64,
    pub total_tokens: u64,
    pub total_cost: f64,
}

// ---------------------------------------------------------------------------
// UsageReport
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UsageReport {
    pub period: AnalyticsBucket,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub aggregated: AggregatedUsage,
    pub trends: Vec<TrendPoint>,
    pub top_models: Vec<ModelRanking>,
    pub top_users: Vec<UserRanking>,
}

// ---------------------------------------------------------------------------
// PipelineConfig
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PipelineConfig {
    pub retention_days: i64,
    pub aggregation_interval_secs: u64,
    pub flush_interval_secs: u64,
    pub buffer_size: usize,
    pub default_region: String,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            retention_days: 365,
            aggregation_interval_secs: 300,
            flush_interval_secs: 60,
            buffer_size: 10_000,
            default_region: "global".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// IngestRequest
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct IngestRequest {
    pub user_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub event_type: EventType,
    pub tokens_used: u64,
    pub latency_ms: u64,
    pub cost: f64,
    pub region: Option<String>,
    pub status_code: Option<u16>,
    pub error_message: Option<String>,
}

// ---------------------------------------------------------------------------
// UsagePipeline
// ---------------------------------------------------------------------------

static EVENT_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub struct UsagePipeline {
    event_buffer: Arc<DashMap<String, UsageEvent>>,
    aggregated: Arc<DashMap<String, AggregatedUsage>>,
    all_events: Arc<DashMap<String, UsageEvent>>,
    config: Arc<std::sync::RwLock<PipelineConfig>>,
}

use std::sync::Arc;

impl Default for UsagePipeline {
    fn default() -> Self {
        Self::new(PipelineConfig::default())
    }
}

impl UsagePipeline {
    pub fn new(config: PipelineConfig) -> Self {
        Self {
            event_buffer: Arc::new(DashMap::new()),
            aggregated: Arc::new(DashMap::new()),
            all_events: Arc::new(DashMap::new()),
            config: Arc::new(std::sync::RwLock::new(config)),
        }
    }

    // ---- ingest_event ----

    pub fn ingest_event(&self, req: &IngestRequest) -> Result<UsageEvent, String> {
        let config = self.config.read().map_err(|e| e.to_string())?;

        let event_id = EVENT_COUNTER.fetch_add(1, Ordering::Relaxed).to_string();
        let event = UsageEvent {
            event_id: event_id.clone(),
            timestamp: Utc::now(),
            user_id: req.user_id.clone(),
            provider_id: req.provider_id.clone(),
            model_id: req.model_id.clone(),
            event_type: req.event_type.clone(),
            tokens_used: req.tokens_used,
            latency_ms: req.latency_ms,
            cost: req.cost,
            region: req
                .region
                .clone()
                .unwrap_or_else(|| config.default_region.clone()),
            status_code: req.status_code.unwrap_or(200),
            error_message: req.error_message.clone(),
        };

        self.all_events
            .entry(event_id.clone())
            .or_insert_with(|| event.clone());

        self.event_buffer
            .entry(event_id.clone())
            .or_insert_with(|| event.clone());

        Ok(event)
    }

    // ---- flush ----

    pub fn flush(&self) -> Result<usize, String> {
        let buffer_len = self.event_buffer.len();
        if buffer_len == 0 {
            return Ok(0);
        }

        let events: Vec<UsageEvent> = self
            .event_buffer
            .iter()
            .map(|r| r.value().clone())
            .collect();

        // Aggregate into daily bucket
        let agg = self.aggregate_events(&events, &AnalyticsBucket::Daily);

        let bucket_key = format!(
            "daily_{}",
            agg.period_start.format("%Y-%m-%d").to_string()
        );

        // Merge with existing if present
        if let Some(mut existing) = self.aggregated.get_mut(&bucket_key) {
            existing.total_requests += agg.total_requests;
            existing.total_tokens += agg.total_tokens;
            existing.total_cost += agg.total_cost;
            existing.error_count += agg.error_count;
            // Recalculate averages
            let total = existing.total_requests as f64;
            existing.avg_latency = if total > 0.0 {
                (existing.avg_latency * (total - agg.total_requests as f64)
                    + agg.avg_latency * agg.total_requests as f64)
                    / total
            } else {
                0.0
            };
            for (model_id, mu) in agg.by_model {
                let entry = existing
                    .by_model
                    .entry(model_id)
                    .or_insert(ModelUsage {
                        model_id: String::new(),
                        requests: 0,
                        tokens: 0,
                        cost: 0.0,
                        avg_latency: 0.0,
                    });
                entry.requests += mu.requests;
                entry.tokens += mu.tokens;
                entry.cost += mu.cost;
            }
            for (region, ru) in agg.by_region {
                let entry = existing
                    .by_region
                    .entry(region)
                    .or_insert(RegionUsage {
                        region: String::new(),
                        requests: 0,
                        tokens: 0,
                        cost: 0.0,
                        avg_latency: 0.0,
                    });
                entry.requests += ru.requests;
                entry.tokens += ru.tokens;
                entry.cost += ru.cost;
            }
        } else {
            self.aggregated.insert(bucket_key, agg);
        }

        // Clear buffer
        self.event_buffer.clear();

        Ok(buffer_len)
    }

    fn aggregate_events(
        &self,
        events: &[UsageEvent],
        bucket: &AnalyticsBucket,
    ) -> AggregatedUsage {
        if events.is_empty() {
            let now = Utc::now();
            return AggregatedUsage {
                bucket: bucket.clone(),
                period_start: now,
                period_end: now,
                total_requests: 0,
                total_tokens: 0,
                total_cost: 0.0,
                avg_latency: 0.0,
                p50_latency: 0.0,
                p95_latency: 0.0,
                p99_latency: 0.0,
                unique_users: 0,
                unique_providers: 0,
                error_count: 0,
                error_rate: 0.0,
                by_model: HashMap::new(),
                by_region: HashMap::new(),
                by_event_type: HashMap::new(),
            };
        }

        let period_start = events
            .iter()
            .map(|e| e.timestamp)
            .min()
            .unwrap_or_else(Utc::now);
        let period_end = events
            .iter()
            .map(|e| e.timestamp)
            .max()
            .unwrap_or_else(Utc::now);

        let total_requests = events.len() as u64;
        let total_tokens: u64 = events.iter().map(|e| e.tokens_used).sum();
        let total_cost: f64 = events.iter().map(|e| e.cost).sum();

        let mut latencies: Vec<u64> = events.iter().map(|e| e.latency_ms).collect();
        latencies.sort();

        let avg_latency = if !latencies.is_empty() {
            latencies.iter().sum::<u64>() as f64 / latencies.len() as f64
        } else {
            0.0
        };

        let p50 = percentile(&latencies, 50.0);
        let p95 = percentile(&latencies, 95.0);
        let p99 = percentile(&latencies, 99.0);

        let mut unique_users = std::collections::HashSet::new();
        let mut unique_providers = std::collections::HashSet::new();
        let mut by_model: HashMap<String, ModelUsage> = HashMap::new();
        let mut by_region: HashMap<String, RegionUsage> = HashMap::new();
        let mut by_event_type: HashMap<String, u64> = HashMap::new();
        let mut error_count = 0u64;

        for event in events {
            unique_users.insert(event.user_id.clone());
            unique_providers.insert(event.provider_id.clone());

            if event.status_code >= 400 {
                error_count += 1;
            }

            *by_event_type
                .entry(event.event_type.as_str().to_string())
                .or_insert(0) += 1;

            let mu = by_model
                .entry(event.model_id.clone())
                .or_insert(ModelUsage {
                    model_id: event.model_id.clone(),
                    requests: 0,
                    tokens: 0,
                    cost: 0.0,
                    avg_latency: 0.0,
                });
            mu.requests += 1;
            mu.tokens += event.tokens_used;
            mu.cost += event.cost;

            let ru = by_region
                .entry(event.region.clone())
                .or_insert(RegionUsage {
                    region: event.region.clone(),
                    requests: 0,
                    tokens: 0,
                    cost: 0.0,
                    avg_latency: 0.0,
                });
            ru.requests += 1;
            ru.tokens += event.tokens_used;
            ru.cost += event.cost;
        }

        // Recalculate model/region avg latencies
        for mu in by_model.values_mut() {
            let model_events: Vec<u64> = events
                .iter()
                .filter(|e| e.model_id == mu.model_id)
                .map(|e| e.latency_ms)
                .collect();
            if !model_events.is_empty() {
                mu.avg_latency = model_events.iter().sum::<u64>() as f64 / model_events.len() as f64;
            }
        }

        for ru in by_region.values_mut() {
            let region_events: Vec<u64> = events
                .iter()
                .filter(|e| e.region == ru.region)
                .map(|e| e.latency_ms)
                .collect();
            if !region_events.is_empty() {
                ru.avg_latency = region_events.iter().sum::<u64>() as f64 / region_events.len() as f64;
            }
        }

        let error_rate = if total_requests > 0 {
            error_count as f64 / total_requests as f64
        } else {
            0.0
        };

        AggregatedUsage {
            bucket: bucket.clone(),
            period_start,
            period_end,
            total_requests,
            total_tokens,
            total_cost,
            avg_latency,
            p50_latency: p50,
            p95_latency: p95,
            p99_latency: p99,
            unique_users: unique_users.len(),
            unique_providers: unique_providers.len(),
            error_count,
            error_rate,
            by_model,
            by_region,
            by_event_type,
        }
    }

    // ---- get_aggregated ----

    pub fn get_aggregated(&self, bucket: &AnalyticsBucket) -> Vec<AggregatedUsage> {
        let prefix = match bucket {
            AnalyticsBucket::Hourly => "hourly_",
            AnalyticsBucket::Daily => "daily_",
            AnalyticsBucket::Weekly => "weekly_",
            AnalyticsBucket::Monthly => "monthly_",
        };

        self.aggregated
            .iter()
            .filter(|r| r.key().starts_with(prefix))
            .map(|r| r.value().clone())
            .collect()
    }

    // ---- get_trends ----

    pub fn get_trends(&self, bucket: &AnalyticsBucket, limit: usize) -> Vec<TrendPoint> {
        let aggs = self.get_aggregated(bucket);
        let mut points: Vec<TrendPoint> = aggs
            .iter()
            .map(|a| TrendPoint {
                period: a.period_start.format("%Y-%m-%d").to_string(),
                timestamp: a.period_start,
                requests: a.total_requests,
                tokens: a.total_tokens,
                cost: a.total_cost,
                change_pct: 0.0,
            })
            .collect();

        // Calculate change percentages
        for i in 1..points.len() {
            let prev_cost = points[i - 1].cost;
            if prev_cost > 0.0 {
                points[i].change_pct =
                    ((points[i].cost - prev_cost) / prev_cost) * 100.0;
            }
        }

        points.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        points.into_iter().take(limit).collect()
    }

    // ---- get_usage_report ----

    pub fn get_usage_report(
        &self,
        bucket: &AnalyticsBucket,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> UsageReport {
        let all_events: Vec<UsageEvent> = self
            .all_events
            .iter()
            .filter(|r| {
                let e = r.value();
                e.timestamp >= start && e.timestamp < end
            })
            .map(|r| r.value().clone())
            .collect();

        let aggregated = self.aggregate_events(&all_events, bucket);
        let trends = self.get_trends(bucket, 30);
        let top_models = self.get_model_rankings(10);
        let top_users = self.get_user_rankings(10);

        UsageReport {
            period: bucket.clone(),
            start,
            end,
            aggregated,
            trends,
            top_models,
            top_users,
        }
    }

    // ---- get_model_rankings ----

    pub fn get_model_rankings(&self, limit: usize) -> Vec<ModelRanking> {
        let mut by_model: HashMap<String, (u64, u64, f64, Vec<u64>, std::collections::HashSet<String>)> =
            HashMap::new();

        for entry in self.all_events.iter() {
            let event = entry.value();
            let data = by_model
                .entry(event.model_id.clone())
                .or_insert((0, 0, 0.0, Vec::new(), std::collections::HashSet::new()));
            data.0 += 1; // requests
            data.1 += event.tokens_used; // tokens
            data.2 += event.cost; // cost
            data.3.push(event.latency_ms); // latencies
            data.4.insert(event.user_id.clone()); // unique users
        }

        let mut rankings: Vec<ModelRanking> = by_model
            .into_iter()
            .map(|(model_id, (reqs, tokens, cost, latencies, users))| {
                let avg_lat = if !latencies.is_empty() {
                    latencies.iter().sum::<u64>() as f64 / latencies.len() as f64
                } else {
                    0.0
                };
                ModelRanking {
                    model_id,
                    rank: 0,
                    total_requests: reqs,
                    total_tokens: tokens,
                    total_cost: cost,
                    avg_latency: avg_lat,
                    unique_users: users.len(),
                    score: cost * 0.4 + reqs as f64 * 0.3 + tokens as f64 * 0.0001 * 0.3,
                }
            })
            .collect();

        rankings.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        for (i, r) in rankings.iter_mut().enumerate() {
            r.rank = i + 1;
        }
        rankings.into_iter().take(limit).collect()
    }

    // ---- get_user_rankings ----

    pub fn get_user_rankings(&self, limit: usize) -> Vec<UserRanking> {
        let mut by_user: HashMap<String, (u64, u64, f64)> = HashMap::new();

        for entry in self.all_events.iter() {
            let event = entry.value();
            let data = by_user
                .entry(event.user_id.clone())
                .or_insert((0, 0, 0.0));
            data.0 += 1;
            data.1 += event.tokens_used;
            data.2 += event.cost;
        }

        let mut rankings: Vec<UserRanking> = by_user
            .into_iter()
            .map(|(user_id, (reqs, tokens, cost))| UserRanking {
                user_id,
                rank: 0,
                total_requests: reqs,
                total_tokens: tokens,
                total_cost: cost,
            })
            .collect();

        rankings.sort_by(|a, b| b.total_cost.partial_cmp(&a.total_cost).unwrap());
        for (i, r) in rankings.iter_mut().enumerate() {
            r.rank = i + 1;
        }
        rankings.into_iter().take(limit).collect()
    }

    // ---- prune_old_data ----

    pub fn prune_old_data(&self) -> Result<usize, String> {
        let config = self.config.read().map_err(|e| e.to_string())?;
        let cutoff = Utc::now() - Duration::days(config.retention_days);

        let keys_to_remove: Vec<String> = self
            .all_events
            .iter()
            .filter(|r| r.value().timestamp < cutoff)
            .map(|r| r.key().clone())
            .collect();

        let count = keys_to_remove.len();
        for key in keys_to_remove {
            self.all_events.remove(&key);
        }

        // Also prune aggregated data
        let agg_keys: Vec<String> = self
            .aggregated
            .iter()
            .filter(|r| r.value().period_end < cutoff)
            .map(|r| r.key().clone())
            .collect();

        let agg_count = agg_keys.len();
        for key in agg_keys {
            self.aggregated.remove(&key);
        }

        Ok(count + agg_count)
    }

    // ---- get_config ----

    pub fn get_config(&self) -> Result<PipelineConfig, String> {
        self.config.read().map(|c| c.clone()).map_err(|e| e.to_string())
    }

    // ---- update_config ----

    pub fn update_config(&self, new_config: PipelineConfig) -> Result<(), String> {
        let mut config = self.config.write().map_err(|e| e.to_string())?;
        *config = new_config;
        Ok(())
    }

    // ---- get_event_count ----

    pub fn get_event_count(&self) -> usize {
        self.all_events.len()
    }

    // ---- get_buffer_size ----

    pub fn get_buffer_size(&self) -> usize {
        self.event_buffer.len()
    }
}

fn percentile(sorted: &[u64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = (p / 100.0) * (sorted.len() - 1) as f64;
    let lower = idx.floor() as usize;
    let upper = (lower + 1).min(sorted.len() - 1);
    let frac = idx - lower as f64;
    sorted[lower] as f64 * (1.0 - frac) + sorted[upper] as f64 * frac
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> UsagePipeline {
        UsagePipeline::default()
    }

    fn make_ingest(user: &str, provider: &str, model: &str) -> IngestRequest {
        IngestRequest {
            user_id: user.to_string(),
            provider_id: provider.to_string(),
            model_id: model.to_string(),
            event_type: EventType::Inference,
            tokens_used: 100,
            latency_ms: 50,
            cost: 0.01,
            region: Some("us-east".to_string()),
            status_code: Some(200),
            error_message: None,
        }
    }

    #[test]
    fn test_ingest_event() {
        let pipeline = setup();
        let req = make_ingest("user-1", "prov-1", "model-A");
        let event = pipeline.ingest_event(&req).unwrap();
        assert_eq!(event.user_id, "user-1");
        assert_eq!(event.model_id, "model-A");
        assert!(!event.event_id.is_empty());
    }

    #[test]
    fn test_flush_empty_buffer() {
        let pipeline = setup();
        let count = pipeline.flush().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_flush_with_events() {
        let pipeline = setup();
        pipeline.ingest_event(&make_ingest("u1", "p1", "m1")).unwrap();
        pipeline.ingest_event(&make_ingest("u2", "p1", "m1")).unwrap();
        let count = pipeline.flush().unwrap();
        assert_eq!(count, 2);
        assert_eq!(pipeline.get_buffer_size(), 0);
    }

    #[test]
    fn test_get_aggregated() {
        let pipeline = setup();
        pipeline.ingest_event(&make_ingest("u1", "p1", "m1")).unwrap();
        pipeline.flush().unwrap();
        let daily = pipeline.get_aggregated(&AnalyticsBucket::Daily);
        assert_eq!(daily.len(), 1);
    }

    #[test]
    fn test_get_trends() {
        let pipeline = setup();
        for i in 0..5 {
            let mut req = make_ingest(&format!("u{}", i), "p1", "m1");
            req.cost = 0.01 * (i + 1) as f64;
            pipeline.ingest_event(&req).unwrap();
        }
        pipeline.flush().unwrap();
        let trends = pipeline.get_trends(&AnalyticsBucket::Daily, 10);
        assert!(!trends.is_empty());
    }

    #[test]
    fn test_get_usage_report() {
        let pipeline = setup();
        pipeline.ingest_event(&make_ingest("u1", "p1", "m1")).unwrap();
        pipeline.flush().unwrap();
        let now = Utc::now();
        let report = pipeline.get_usage_report(
            &AnalyticsBucket::Daily,
            now - Duration::hours(1),
            now + Duration::hours(1),
        );
        assert_eq!(report.period, AnalyticsBucket::Daily);
        assert_eq!(report.aggregated.total_requests, 1);
    }

    #[test]
    fn test_get_model_rankings() {
        let pipeline = setup();
        pipeline.ingest_event(&make_ingest("u1", "p1", "model-A")).unwrap();
        pipeline.ingest_event(&make_ingest("u2", "p1", "model-B")).unwrap();
        pipeline.ingest_event(&make_ingest("u3", "p1", "model-B")).unwrap();
        let rankings = pipeline.get_model_rankings(10);
        assert_eq!(rankings.len(), 2);
        assert_eq!(rankings[0].rank, 1);
        assert_eq!(rankings[0].model_id, "model-B"); // more requests
    }

    #[test]
    fn test_get_user_rankings() {
        let pipeline = setup();
        pipeline.ingest_event(&make_ingest("u1", "p1", "m1")).unwrap();
        pipeline.ingest_event(&make_ingest("u2", "p1", "m1")).unwrap();
        let rankings = pipeline.get_user_rankings(10);
        assert_eq!(rankings.len(), 2);
    }

    #[test]
    fn test_prune_old_data() {
        let pipeline = setup();
        pipeline.ingest_event(&make_ingest("u1", "p1", "m1")).unwrap();
        pipeline.flush().unwrap();
        // No data should be pruned since it's all recent
        let pruned = pipeline.prune_old_data().unwrap();
        // The flushed event in all_events is recent so shouldn't be pruned
        assert!(pruned <= 1); // The aggregated bucket might be pruned based on period_end
    }

    #[test]
    fn test_percentile_calculation() {
        let latencies: Vec<u64> = vec![10, 20, 30, 40, 50];
        let p50 = percentile(&latencies, 50.0);
        assert!((p50 - 30.0).abs() < 0.1);
    }

    #[test]
    fn test_percentile_empty() {
        let latencies: Vec<u64> = vec![];
        let p50 = percentile(&latencies, 50.0);
        assert!((p50 - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_multiple_event_types() {
        let pipeline = setup();
        let mut req = make_ingest("u1", "p1", "m1");
        req.event_type = EventType::Training;
        pipeline.ingest_event(&req).unwrap();
        let mut req2 = make_ingest("u2", "p1", "m1");
        req2.event_type = EventType::Embedding;
        pipeline.ingest_event(&req2).unwrap();
        pipeline.flush().unwrap();
        let daily = pipeline.get_aggregated(&AnalyticsBucket::Daily);
        assert_eq!(daily.len(), 1);
        assert!(daily[0].by_event_type.contains_key("Training"));
        assert!(daily[0].by_event_type.contains_key("Embedding"));
    }
}
