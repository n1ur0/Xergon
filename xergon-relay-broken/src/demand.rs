//! Per-model demand tracking with a sliding time window.
//!
//! Records request counts per model and exposes a demand multiplier
//! (1.0 = baseline, up to 3.0 = high demand) via a sigmoid curve.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// Tracks per-model request demand over a sliding window.
pub struct DemandTracker {
    /// model_id -> list of (timestamp, count) entries
    requests: RwLock<HashMap<String, Vec<(Instant, u32)>>>,
    /// How far back to look for demand calculation
    window: Duration,
}

impl DemandTracker {
    /// Create a new demand tracker with the given window duration in seconds.
    pub fn new(window_secs: u64) -> Self {
        Self {
            requests: RwLock::new(HashMap::new()),
            window: Duration::from_secs(window_secs),
        }
    }

    /// Record that `count` requests were made for `model_id`.
    pub fn record(&self, model_id: &str, count: u32) {
        if count == 0 {
            return;
        }
        let mut map = self.requests.write().unwrap_or_else(|e| e.into_inner());
        map.entry(model_id.to_string())
            .or_default()
            .push((Instant::now(), count));
    }

    /// Get total request count for a model within the window.
    pub fn demand(&self, model_id: &str) -> u32 {
        let cutoff = Instant::now() - self.window;
        let map = self.requests.read().unwrap_or_else(|e| e.into_inner());
        map.get(model_id)
            .map(|entries| {
                entries
                    .iter()
                    .filter(|(ts, _)| *ts >= cutoff)
                    .map(|(_, c)| *c)
                    .sum()
            })
            .unwrap_or(0)
    }

    /// Get demand multiplier (1.0 = no extra demand, up to 3.0 = high demand).
    ///
    /// Uses sigmoid: `1.0 + 2.0 * (1 - 1/(1 + demand/100))`
    pub fn demand_multiplier(&self, model_id: &str) -> f64 {
        let d = self.demand(model_id);
        1.0 + 2.0 * (1.0 - 1.0 / (1.0 + d as f64 / 100.0))
    }

    /// Prune expired entries from all models. Call periodically (e.g., every 60s).
    pub fn prune(&self) {
        let cutoff = Instant::now() - self.window;
        let mut map = self.requests.write().unwrap_or_else(|e| e.into_inner());
        for entries in map.values_mut() {
            entries.retain(|(ts, _)| *ts >= cutoff);
        }
        // Remove models with no remaining entries
        map.retain(|_, entries| !entries.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_demand_tracking() {
        let tracker = DemandTracker::new(300);
        assert_eq!(tracker.demand("gpt-4"), 0);
        assert_eq!(tracker.demand_multiplier("gpt-4"), 1.0);

        tracker.record("gpt-4", 5);
        assert_eq!(tracker.demand("gpt-4"), 5);

        tracker.record("gpt-4", 3);
        assert_eq!(tracker.demand("gpt-4"), 8);
    }

    #[test]
    fn test_multiplier_saturates() {
        let tracker = DemandTracker::new(300);

        // Low demand -> close to 1.0
        tracker.record("model-a", 1);
        let mult = tracker.demand_multiplier("model-a");
        assert!(mult > 1.0 && mult < 1.1);

        // High demand -> approaching 3.0
        tracker.record("model-a", 500);
        let mult = tracker.demand_multiplier("model-a");
        assert!(mult > 2.5 && mult < 3.0);
    }

    #[test]
    fn test_zero_count_ignored() {
        let tracker = DemandTracker::new(300);
        tracker.record("gpt-4", 0);
        assert_eq!(tracker.demand("gpt-4"), 0);
    }

    #[test]
    fn test_independent_models() {
        let tracker = DemandTracker::new(300);
        tracker.record("model-a", 10);
        tracker.record("model-b", 20);
        assert_eq!(tracker.demand("model-a"), 10);
        assert_eq!(tracker.demand("model-b"), 20);
        assert_eq!(tracker.demand("model-c"), 0);
    }
}
