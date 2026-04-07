use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// AnalyticsEvent
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AnalyticsEvent {
    pub id: String,
    pub event_type: String,
    pub model_id: String,
    pub user_id: String,
    pub timestamp: DateTime<Utc>,
    pub metadata: HashMap<String, serde_json::Value>,
    pub value: f64,
}

impl AnalyticsEvent {
    pub fn new(
        event_type: &str,
        model_id: &str,
        user_id: &str,
        metadata: HashMap<String, serde_json::Value>,
        value: f64,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            event_type: event_type.to_string(),
            model_id: model_id.to_string(),
            user_id: user_id.to_string(),
            timestamp: Utc::now(),
            metadata,
            value,
        }
    }
}

// ---------------------------------------------------------------------------
// TimeRange
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TimeRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl TimeRange {
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        Self { start, end }
    }

    /// Create a time range covering the last `n` seconds up to now.
    pub fn last_seconds(seconds: i64) -> Self {
        let end = Utc::now();
        let start = end - chrono::Duration::seconds(seconds);
        Self { start, end }
    }

    /// Create a time range covering the last `n` minutes up to now.
    pub fn last_minutes(minutes: i64) -> Self {
        Self::last_seconds(minutes * 60)
    }

    /// Create a time range covering the last `n` hours up to now.
    pub fn last_hours(hours: i64) -> Self {
        Self::last_seconds(hours * 3600)
    }

    /// Create a time range covering the last `n` days up to now.
    pub fn last_days(days: i64) -> Self {
        Self::last_seconds(days * 86400)
    }

    /// Check whether a timestamp falls within this range.
    pub fn contains(&self, ts: DateTime<Utc>) -> bool {
        ts >= self.start && ts <= self.end
    }
}

// ---------------------------------------------------------------------------
// AnalyticsSummary
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AnalyticsSummary {
    pub total_events: u64,
    pub unique_models: u32,
    pub unique_users: u32,
    pub top_models: Vec<(String, u64)>,
    pub event_type_breakdown: HashMap<String, u64>,
    pub total_value: f64,
    pub avg_value: f64,
}

// ---------------------------------------------------------------------------
// ModelAnalytics
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ModelAnalytics {
    pub model_id: String,
    pub downloads: u64,
    pub views: u64,
    pub ratings: u64,
    pub avg_rating: f64,
    pub revenue: f64,
    pub conversion_rate: f64,
}

impl ModelAnalytics {
    pub fn new(model_id: &str) -> Self {
        Self {
            model_id: model_id.to_string(),
            downloads: 0,
            views: 0,
            ratings: 0,
            avg_rating: 0.0,
            revenue: 0.0,
            conversion_rate: 0.0,
        }
    }

    /// Recalculate conversion rate as downloads / views.
    pub fn recalc_conversion(&mut self) {
        if self.views > 0 {
            self.conversion_rate = self.downloads as f64 / self.views as f64;
        }
    }
}

// ---------------------------------------------------------------------------
// AnalyticsMetricsSnapshot
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AnalyticsMetricsSnapshot {
    pub total_events: u64,
    pub unique_models_tracked: u64,
    pub unique_users: u64,
    pub event_types_count: usize,
    pub total_revenue: f64,
}

// ---------------------------------------------------------------------------
// AnalyticsDashboard
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct AnalyticsDashboard {
    /// All events keyed by UUID.
    events: DashMap<String, AnalyticsEvent>,
    /// Per-model aggregated stats.
    model_stats: DashMap<String, ModelAnalytics>,
    total_events: AtomicU64,
}

impl AnalyticsDashboard {
    /// Create a new empty analytics dashboard.
    pub fn new() -> Self {
        Self {
            events: DashMap::new(),
            model_stats: DashMap::new(),
            total_events: AtomicU64::new(0),
        }
    }

    /// Record a new analytics event and update model stats.
    pub fn record_event(
        &self,
        event_type: &str,
        model_id: &str,
        user_id: &str,
        metadata: HashMap<String, serde_json::Value>,
        value: f64,
    ) -> AnalyticsEvent {
        let event = AnalyticsEvent::new(event_type, model_id, user_id, metadata, value);
        self.total_events.fetch_add(1, Ordering::Relaxed);

        // Update model stats based on event type
        {
            let mut stats = self
                .model_stats
                .entry(model_id.to_string())
                .or_insert_with(|| ModelAnalytics::new(model_id));

            match event_type {
                "download" => {
                    stats.downloads += 1;
                    stats.revenue += value;
                }
                "view" => {
                    stats.views += 1;
                }
                "rating" => {
                    stats.ratings += 1;
                    // Running average of rating values
                    let old_avg = stats.avg_rating;
                    let old_count = stats.ratings - 1;
                    if old_count > 0 {
                        stats.avg_rating =
                            (old_avg * old_count as f64 + value) / stats.ratings as f64;
                    } else {
                        stats.avg_rating = value;
                    }
                }
                _ => {}
            }
            stats.recalc_conversion();
        }

        let id = event.id.clone();
        self.events.insert(id, event.clone());
        event
    }

    /// Get a summary of all events within a time range.
    pub fn get_summary(&self, range: &TimeRange) -> AnalyticsSummary {
        let mut total_events: u64 = 0;
        let mut unique_models = std::collections::HashSet::new();
        let mut unique_users = std::collections::HashSet::new();
        let mut model_counts: HashMap<String, u64> = HashMap::new();
        let mut event_type_breakdown: HashMap<String, u64> = HashMap::new();
        let mut total_value: f64 = 0.0;

        for entry in self.events.iter() {
            let event = entry.value();
            if range.contains(event.timestamp) {
                total_events += 1;
                unique_models.insert(event.model_id.clone());
                unique_users.insert(event.user_id.clone());
                *model_counts.entry(event.model_id.clone()).or_insert(0) += 1;
                *event_type_breakdown
                    .entry(event.event_type.clone())
                    .or_insert(0) += 1;
                total_value += event.value;
            }
        }

        // Sort top models by event count descending, take top 10
        let mut top_models: Vec<(String, u64)> = model_counts.into_iter().collect();
        top_models.sort_by(|a, b| b.1.cmp(&a.1));
        top_models.truncate(10);

        let avg_value = if total_events > 0 {
            total_value / total_events as f64
        } else {
            0.0
        };

        AnalyticsSummary {
            total_events,
            unique_models: unique_models.len() as u32,
            unique_users: unique_users.len() as u32,
            top_models,
            event_type_breakdown,
            total_value,
            avg_value,
        }
    }

    /// Get aggregated analytics for a specific model.
    pub fn get_model_analytics(&self, model_id: &str) -> Option<ModelAnalytics> {
        self.model_stats.get(model_id).map(|r| r.value().clone())
    }

    /// Get top models ranked by downloads, up to `limit`.
    pub fn get_top_models(&self, limit: usize) -> Vec<ModelAnalytics> {
        let mut models: Vec<ModelAnalytics> = self
            .model_stats
            .iter()
            .map(|r| r.value().clone())
            .collect();
        models.sort_by(|a, b| b.downloads.cmp(&a.downloads));
        models.truncate(limit);
        models
    }

    /// Get a list of distinct event types that have been recorded.
    pub fn get_event_types(&self) -> Vec<String> {
        let mut types = std::collections::HashSet::new();
        for entry in self.events.iter() {
            types.insert(entry.value().event_type.clone());
        }
        let mut result: Vec<String> = types.into_iter().collect();
        result.sort();
        result
    }

    /// Get all events for a specific user.
    pub fn get_user_activity(&self, user_id: &str) -> Vec<AnalyticsEvent> {
        self.events
            .iter()
            .filter(|r| r.value().user_id == user_id)
            .map(|r| r.value().clone())
            .collect()
    }

    /// Get all events for a specific model.
    pub fn get_model_activity(&self, model_id: &str) -> Vec<AnalyticsEvent> {
        self.events
            .iter()
            .filter(|r| r.value().model_id == model_id)
            .map(|r| r.value().clone())
            .collect()
    }

    /// Get a snapshot of overall dashboard metrics.
    pub fn get_metrics(&self) -> AnalyticsMetricsSnapshot {
        let unique_models_tracked = self.model_stats.len() as u64;
        let mut unique_users = std::collections::HashSet::new();
        let mut total_revenue: f64 = 0.0;

        for entry in self.events.iter() {
            unique_users.insert(entry.value().user_id.clone());
        }

        for entry in self.model_stats.iter() {
            total_revenue += entry.value().revenue;
        }

        AnalyticsMetricsSnapshot {
            total_events: self.total_events.load(Ordering::Relaxed),
            unique_models_tracked,
            unique_users: unique_users.len() as u64,
            event_types_count: self.get_event_types().len(),
            total_revenue,
        }
    }
}

impl Default for AnalyticsDashboard {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_dashboard() -> AnalyticsDashboard {
        AnalyticsDashboard::new()
    }

    // -- record_event --

    #[test]
    fn test_record_event() {
        let dash = make_dashboard();
        let event = dash.record_event(
            "download",
            "model-1",
            "user-1",
            HashMap::new(),
            9.99,
        );

        assert_eq!(event.event_type, "download");
        assert_eq!(event.model_id, "model-1");
        assert_eq!(event.user_id, "user-1");
        assert_eq!(event.value, 9.99);
        assert!(!event.id.is_empty());
    }

    #[test]
    fn test_record_multiple_events() {
        let dash = make_dashboard();
        dash.record_event("view", "m1", "u1", HashMap::new(), 0.0);
        dash.record_event("download", "m1", "u1", HashMap::new(), 5.0);
        dash.record_event("rating", "m1", "u2", HashMap::new(), 4.5);

        assert_eq!(dash.total_events.load(Ordering::Relaxed), 3);
    }

    // -- get_summary --

    #[test]
    fn test_get_summary_empty() {
        let dash = make_dashboard();
        let range = TimeRange::last_days(1);
        let summary = dash.get_summary(&range);

        assert_eq!(summary.total_events, 0);
        assert_eq!(summary.unique_models, 0);
        assert_eq!(summary.unique_users, 0);
        assert!(summary.top_models.is_empty());
        assert!(summary.event_type_breakdown.is_empty());
        assert_eq!(summary.total_value, 0.0);
        assert_eq!(summary.avg_value, 0.0);
    }

    #[test]
    fn test_get_summary_with_events() {
        let dash = make_dashboard();
        dash.record_event("download", "m1", "u1", HashMap::new(), 10.0);
        dash.record_event("download", "m1", "u2", HashMap::new(), 10.0);
        dash.record_event("view", "m2", "u1", HashMap::new(), 0.0);

        let range = TimeRange::last_days(1);
        let summary = dash.get_summary(&range);

        assert_eq!(summary.total_events, 3);
        assert_eq!(summary.unique_models, 2);
        assert_eq!(summary.unique_users, 2);
        assert_eq!(summary.total_value, 20.0);
        assert!((summary.avg_value - 20.0 / 3.0).abs() < f64::EPSILON);

        // m1 should be top model with 2 events
        assert_eq!(summary.top_models.len(), 2);
        assert_eq!(summary.top_models[0], ("m1".to_string(), 2));

        // Event type breakdown
        assert_eq!(summary.event_type_breakdown.get("download"), Some(&2));
        assert_eq!(summary.event_type_breakdown.get("view"), Some(&1));
    }

    #[test]
    fn test_get_summary_time_filtered() {
        let dash = make_dashboard();
        dash.record_event("download", "m1", "u1", HashMap::new(), 5.0);

        // Query a range in the past that should not include the event
        let now = Utc::now();
        let range = TimeRange::new(
            now - chrono::Duration::days(7),
            now - chrono::Duration::seconds(10),
        );
        let summary = dash.get_summary(&range);

        assert_eq!(summary.total_events, 0);
    }

    // -- model analytics --

    #[test]
    fn test_get_model_analytics() {
        let dash = make_dashboard();
        dash.record_event("view", "m1", "u1", HashMap::new(), 0.0);
        dash.record_event("download", "m1", "u1", HashMap::new(), 15.0);

        let stats = dash.get_model_analytics("m1").unwrap();
        assert_eq!(stats.model_id, "m1");
        assert_eq!(stats.views, 1);
        assert_eq!(stats.downloads, 1);
        assert_eq!(stats.revenue, 15.0);
        assert!((stats.conversion_rate - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_get_model_analytics_not_found() {
        let dash = make_dashboard();
        assert!(dash.get_model_analytics("nonexistent").is_none());
    }

    #[test]
    fn test_model_rating_average() {
        let dash = make_dashboard();
        dash.record_event("rating", "m1", "u1", HashMap::new(), 4.0);
        dash.record_event("rating", "m1", "u2", HashMap::new(), 5.0);

        let stats = dash.get_model_analytics("m1").unwrap();
        assert_eq!(stats.ratings, 2);
        assert!((stats.avg_rating - 4.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_conversion_rate_zero_views() {
        let dash = make_dashboard();
        dash.record_event("download", "m1", "u1", HashMap::new(), 5.0);

        let stats = dash.get_model_analytics("m1").unwrap();
        assert_eq!(stats.conversion_rate, 0.0); // no views
    }

    // -- top models --

    #[test]
    fn test_get_top_models() {
        let dash = make_dashboard();
        // m1 gets 3 downloads
        dash.record_event("download", "m1", "u1", HashMap::new(), 0.0);
        dash.record_event("download", "m1", "u2", HashMap::new(), 0.0);
        dash.record_event("download", "m1", "u3", HashMap::new(), 0.0);
        // m2 gets 1 download
        dash.record_event("download", "m2", "u1", HashMap::new(), 0.0);
        // m3 gets 2 downloads
        dash.record_event("download", "m3", "u1", HashMap::new(), 0.0);
        dash.record_event("download", "m3", "u2", HashMap::new(), 0.0);

        let top = dash.get_top_models(2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].model_id, "m1");
        assert_eq!(top[0].downloads, 3);
        assert_eq!(top[1].model_id, "m3");
        assert_eq!(top[1].downloads, 2);
    }

    #[test]
    fn test_get_top_models_empty() {
        let dash = make_dashboard();
        let top = dash.get_top_models(10);
        assert!(top.is_empty());
    }

    // -- event types --

    #[test]
    fn test_get_event_types() {
        let dash = make_dashboard();
        dash.record_event("download", "m1", "u1", HashMap::new(), 0.0);
        dash.record_event("view", "m1", "u1", HashMap::new(), 0.0);
        dash.record_event("download", "m2", "u2", HashMap::new(), 0.0);
        dash.record_event("purchase", "m1", "u3", HashMap::new(), 0.0);

        let types = dash.get_event_types();
        assert_eq!(types, vec!["download", "purchase", "view"]);
    }

    #[test]
    fn test_get_event_types_empty() {
        let dash = make_dashboard();
        let types = dash.get_event_types();
        assert!(types.is_empty());
    }

    // -- user / model activity --

    #[test]
    fn test_get_user_activity() {
        let dash = make_dashboard();
        dash.record_event("download", "m1", "u1", HashMap::new(), 0.0);
        dash.record_event("view", "m2", "u1", HashMap::new(), 0.0);
        dash.record_event("download", "m1", "u2", HashMap::new(), 0.0);

        let activity = dash.get_user_activity("u1");
        assert_eq!(activity.len(), 2);
        assert!(activity.iter().all(|e| e.user_id == "u1"));
    }

    #[test]
    fn test_get_user_activity_no_events() {
        let dash = make_dashboard();
        let activity = dash.get_user_activity("ghost");
        assert!(activity.is_empty());
    }

    #[test]
    fn test_get_model_activity() {
        let dash = make_dashboard();
        dash.record_event("download", "m1", "u1", HashMap::new(), 0.0);
        dash.record_event("view", "m1", "u2", HashMap::new(), 0.0);
        dash.record_event("download", "m2", "u1", HashMap::new(), 0.0);

        let activity = dash.get_model_activity("m1");
        assert_eq!(activity.len(), 2);
        assert!(activity.iter().all(|e| e.model_id == "m1"));
    }

    // -- metrics --

    #[test]
    fn test_get_metrics() {
        let dash = make_dashboard();
        dash.record_event("download", "m1", "u1", HashMap::new(), 10.0);
        dash.record_event("download", "m1", "u2", HashMap::new(), 10.0);
        dash.record_event("view", "m2", "u3", HashMap::new(), 0.0);

        let metrics = dash.get_metrics();
        assert_eq!(metrics.total_events, 3);
        assert_eq!(metrics.unique_models_tracked, 2);
        assert_eq!(metrics.unique_users, 3);
        assert_eq!(metrics.event_types_count, 2);
        assert_eq!(metrics.total_revenue, 20.0);
    }

    #[test]
    fn test_get_metrics_empty() {
        let dash = make_dashboard();
        let metrics = dash.get_metrics();
        assert_eq!(metrics.total_events, 0);
        assert_eq!(metrics.unique_models_tracked, 0);
        assert_eq!(metrics.unique_users, 0);
        assert_eq!(metrics.event_types_count, 0);
        assert_eq!(metrics.total_revenue, 0.0);
    }

    // -- TimeRange helpers --

    #[test]
    fn test_time_range_contains() {
        let start = Utc::now() - chrono::Duration::hours(1);
        let end = Utc::now();
        let range = TimeRange::new(start, end);

        let inside = start + chrono::Duration::minutes(30);
        assert!(range.contains(inside));

        let before = start - chrono::Duration::seconds(1);
        assert!(!range.contains(before));

        let after = end + chrono::Duration::seconds(1);
        assert!(!range.contains(after));
    }

    #[test]
    fn test_time_range_last_seconds() {
        let range = TimeRange::last_seconds(60);
        let diff = (range.end - range.start).num_seconds();
        assert!(diff <= 61); // small timing variance
    }

    #[test]
    fn test_time_range_last_days() {
        let range = TimeRange::last_days(7);
        let diff = (range.end - range.start).num_days();
        assert!(diff <= 7);
    }

    // -- default --

    #[test]
    fn test_default_dashboard() {
        let dash = AnalyticsDashboard::default();
        assert_eq!(dash.events.len(), 0);
        assert_eq!(dash.model_stats.len(), 0);
    }

    // -- event metadata --

    #[test]
    fn test_event_metadata_stored() {
        let dash = make_dashboard();
        let mut meta = HashMap::new();
        meta.insert("source".to_string(), serde_json::Value::String("web".to_string()));
        meta.insert("version".to_string(), serde_json::Value::Number(2.into()));

        let event = dash.record_event("download", "m1", "u1", meta, 5.0);
        assert_eq!(event.metadata.get("source").unwrap().as_str().unwrap(), "web");
        assert_eq!(event.metadata.get("version").unwrap().as_i64().unwrap(), 2);
    }
}
