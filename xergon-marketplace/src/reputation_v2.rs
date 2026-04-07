use std::collections::HashMap;

use axum::{
    extract::{Path, State},
    response::Json,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ReputationFactor
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ReputationFactor {
    ResponseTime,
    Uptime,
    Accuracy,
    UserSatisfaction,
    DisputeRate,
    Volume,
    StakingAmount,
}

impl ReputationFactor {
    pub fn as_str(&self) -> &str {
        match self {
            Self::ResponseTime => "ResponseTime",
            Self::Uptime => "Uptime",
            Self::Accuracy => "Accuracy",
            Self::UserSatisfaction => "UserSatisfaction",
            Self::DisputeRate => "DisputeRate",
            Self::Volume => "Volume",
            Self::StakingAmount => "StakingAmount",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "ResponseTime" => Some(Self::ResponseTime),
            "Uptime" => Some(Self::Uptime),
            "Accuracy" => Some(Self::Accuracy),
            "UserSatisfaction" => Some(Self::UserSatisfaction),
            "DisputeRate" => Some(Self::DisputeRate),
            "Volume" => Some(Self::Volume),
            "StakingAmount" => Some(Self::StakingAmount),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// ReputationTier
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReputationTier {
    Bronze,
    Silver,
    Gold,
    Platinum,
    Diamond,
}

impl ReputationTier {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Bronze => "Bronze",
            Self::Silver => "Silver",
            Self::Gold => "Gold",
            Self::Platinum => "Platinum",
            Self::Diamond => "Diamond",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "Bronze" => Some(Self::Bronze),
            "Silver" => Some(Self::Silver),
            "Gold" => Some(Self::Gold),
            "Platinum" => Some(Self::Platinum),
            "Diamond" => Some(Self::Diamond),
            _ => None,
        }
    }

    /// Numeric value for comparison (Bronze=0 .. Diamond=4).
    pub fn level(&self) -> u8 {
        match self {
            Self::Bronze => 0,
            Self::Silver => 1,
            Self::Gold => 2,
            Self::Platinum => 3,
            Self::Diamond => 4,
        }
    }
}

// ---------------------------------------------------------------------------
// ReviewWeight
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ReviewWeight {
    pub reviewer_reputation: f64,
    pub recency_weight: f64,
    pub detail_score: f64,
    pub total_weight: f64,
}

impl ReviewWeight {
    pub fn new(reviewer_reputation: f64, recency_weight: f64, detail_score: f64) -> Self {
        let total_weight = (reviewer_reputation * 0.4 + recency_weight * 0.3 + detail_score * 0.3)
            .max(0.0)
            .min(1.0);
        Self {
            reviewer_reputation,
            recency_weight,
            detail_score,
            total_weight,
        }
    }
}

// ---------------------------------------------------------------------------
// ReputationConfig
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReputationConfig {
    pub decay_rate: f64,
    pub min_reviews: u32,
    pub tier_thresholds: HashMap<String, f64>,
    pub weighting: HashMap<String, f64>,
    pub review_window_days: i64,
}

impl Default for ReputationConfig {
    fn default() -> Self {
        let mut tier_thresholds = HashMap::new();
        tier_thresholds.insert("Bronze".to_string(), 0.0);
        tier_thresholds.insert("Silver".to_string(), 20.0);
        tier_thresholds.insert("Gold".to_string(), 50.0);
        tier_thresholds.insert("Platinum".to_string(), 75.0);
        tier_thresholds.insert("Diamond".to_string(), 90.0);

        let mut weighting = HashMap::new();
        weighting.insert("ResponseTime".to_string(), 0.15);
        weighting.insert("Uptime".to_string(), 0.20);
        weighting.insert("Accuracy".to_string(), 0.20);
        weighting.insert("UserSatisfaction".to_string(), 0.25);
        weighting.insert("DisputeRate".to_string(), 0.10);
        weighting.insert("Volume".to_string(), 0.05);
        weighting.insert("StakingAmount".to_string(), 0.05);

        Self {
            decay_rate: 0.95,
            min_reviews: 5,
            tier_thresholds,
            weighting,
            review_window_days: 90,
        }
    }
}

// ---------------------------------------------------------------------------
// ReputationHistoryEntry
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReputationHistoryEntry {
    pub timestamp: DateTime<Utc>,
    pub score_before: f64,
    pub score_after: f64,
    pub factor: String,
    pub reason: String,
}

// ---------------------------------------------------------------------------
// ReputationScore
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReputationScore {
    pub provider_id: String,
    pub overall_score: f64,
    pub category_scores: HashMap<String, f64>,
    pub weighted_factors: HashMap<String, f64>,
    pub decay_rate: f64,
    pub last_updated: DateTime<Utc>,
    pub history: Vec<ReputationHistoryEntry>,
    pub tier: ReputationTier,
    pub total_reviews: u32,
}

impl ReputationScore {
    pub fn new(provider_id: &str, config: &ReputationConfig) -> Self {
        let category_scores = config
            .weighting
            .keys()
            .map(|k| (k.clone(), 50.0))
            .collect();

        let weighted_factors = config
            .weighting
            .keys()
            .map(|k| (k.clone(), 0.0))
            .collect();

        Self {
            provider_id: provider_id.to_string(),
            overall_score: 50.0,
            category_scores,
            weighted_factors,
            decay_rate: config.decay_rate,
            last_updated: Utc::now(),
            history: Vec::new(),
            tier: ReputationTier::Bronze,
            total_reviews: 0,
        }
    }

    /// Recalculate the overall score from category scores using config weights.
    pub fn recalculate_overall(&mut self, config: &ReputationConfig) {
        let mut weighted_sum = 0.0;
        let mut total_weight = 0.0;

        for (factor, score) in &self.category_scores {
            let weight = config.weighting.get(factor).copied().unwrap_or(0.0);
            weighted_sum += score * weight;
            total_weight += weight;
        }

        let score_before = self.overall_score;
        self.overall_score = if total_weight > 0.0 {
            (weighted_sum / total_weight).clamp(0.0, 100.0)
        } else {
            50.0
        };

        // Apply decay
        let decayed = score_before * self.decay_rate + self.overall_score * (1.0 - self.decay_rate);
        self.overall_score = decayed.clamp(0.0, 100.0);

        // Update tier
        self.tier = Self::determine_tier(self.overall_score, &config.tier_thresholds);
    }

    fn determine_tier(score: f64, thresholds: &HashMap<String, f64>) -> ReputationTier {
        // Check from highest to lowest
        if let Some(&thresh) = thresholds.get("Diamond") {
            if score >= thresh {
                return ReputationTier::Diamond;
            }
        }
        if let Some(&thresh) = thresholds.get("Platinum") {
            if score >= thresh {
                return ReputationTier::Platinum;
            }
        }
        if let Some(&thresh) = thresholds.get("Gold") {
            if score >= thresh {
                return ReputationTier::Gold;
            }
        }
        if let Some(&thresh) = thresholds.get("Silver") {
            if score >= thresh {
                return ReputationTier::Silver;
            }
        }
        ReputationTier::Bronze
    }

    /// Record a history entry for this score change.
    fn record_history(&mut self, factor: &str, reason: &str, old_score: f64) {
        self.history.push(ReputationHistoryEntry {
            timestamp: Utc::now(),
            score_before: old_score,
            score_after: self.overall_score,
            factor: factor.to_string(),
            reason: reason.to_string(),
        });
        // Keep history bounded to last 200 entries
        if self.history.len() > 200 {
            self.history.drain(0..self.history.len() - 200);
        }
    }
}

// ---------------------------------------------------------------------------
// UpdateScoreRequest
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UpdateScoreRequest {
    pub provider_id: String,
    pub factor: String,
    pub score: f64,
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// BulkUpdateRequest
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BulkUpdateRequest {
    pub updates: Vec<UpdateScoreRequest>,
}

// ---------------------------------------------------------------------------
// ReputationEngine
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct ReputationEngine {
    scores: DashMap<String, ReputationScore>,
    config: DashMap<String, ReputationConfig>,
}

impl ReputationEngine {
    pub fn new() -> Self {
        let config = DashMap::new();
        config.insert("default".to_string(), ReputationConfig::default());
        Self { scores: DashMap::new(), config }
    }

    pub fn default() -> Self {
        Self::new()
    }

    /// Get the current config.
    pub fn get_config(&self) -> ReputationConfig {
        self.config
            .get("default")
            .map(|c| c.clone())
            .unwrap_or_default()
    }

    /// Update the config.
    pub fn update_config(&self, new_config: ReputationConfig) {
        self.config.insert("default".to_string(), new_config);
    }

    /// Ensure a provider entry exists; create with defaults if not.
    fn ensure_provider(&self, provider_id: &str, config: &ReputationConfig) {
        if !self.scores.contains_key(provider_id) {
            self.scores
                .insert(provider_id.to_string(), ReputationScore::new(provider_id, config));
        }
    }

    /// Update a single factor score for a provider.
    pub fn update_score(&self, req: &UpdateScoreRequest) -> Result<ReputationScore, String> {
        let config = self.get_config();
        self.ensure_provider(&req.provider_id, &config);

        if let Some(mut entry) = self.scores.get_mut(&req.provider_id) {
            let old_score = entry.overall_score;
            let clamped_score = req.score.clamp(0.0, 100.0);
            entry.category_scores.insert(req.factor.clone(), clamped_score);

            // Compute weighted factor contribution
            let weight = config.weighting.get(&req.factor).copied().unwrap_or(0.0);
            entry.weighted_factors.insert(req.factor.clone(), clamped_score * weight);

            entry.total_reviews += 1;
            entry.recalculate_overall(&config);
            entry.last_updated = Utc::now();

            let reason = req.reason.as_deref().unwrap_or("Score updated");
            entry.record_history(&req.factor, reason, old_score);

            Ok(entry.clone())
        } else {
            Err("Failed to update score".to_string())
        }
    }

    /// Get the reputation score for a provider.
    pub fn get_score(&self, provider_id: &str) -> Option<ReputationScore> {
        self.scores.get(provider_id).map(|e| e.clone())
    }

    /// Get the tier for a provider.
    pub fn get_tier(&self, provider_id: &str) -> Option<ReputationTier> {
        self.scores.get(provider_id).map(|e| e.tier.clone())
    }

    /// Get the history for a provider.
    pub fn get_history(&self, provider_id: &str, limit: usize) -> Vec<ReputationHistoryEntry> {
        self.scores
            .get(provider_id)
            .map(|e| {
                let len = e.history.len();
                let start = if len > limit { len - limit } else { 0 };
                e.history[start..].to_vec()
            })
            .unwrap_or_default()
    }

    /// Recalculate all scores (e.g. after config change).
    pub fn recalculate(&self) -> usize {
        let config = self.get_config();
        let mut count = 0;
        for mut entry in self.scores.iter_mut() {
            entry.recalculate_overall(&config);
            entry.last_updated = Utc::now();
            count += 1;
        }
        count
    }

    /// Apply time-decay to all scores.
    pub fn apply_decay(&self) -> usize {
        let config = self.get_config();
        let mut count = 0;
        for mut entry in self.scores.iter_mut() {
            let old_score = entry.overall_score;
            for (_, score) in entry.category_scores.iter_mut() {
                *score = *score * config.decay_rate + 50.0 * (1.0 - config.decay_rate);
            }
            entry.recalculate_overall(&config);
            entry.last_updated = Utc::now();
            entry.record_history("decay", "Time-based decay applied", old_score);
            count += 1;
        }
        count
    }

    /// Get leaderboard: top N providers by overall score.
    pub fn get_leaderboard(&self, limit: usize) -> Vec<(String, f64, ReputationTier)> {
        let mut all: Vec<(String, f64, ReputationTier)> = self
            .scores
            .iter()
            .map(|e| (e.provider_id.clone(), e.overall_score, e.tier.clone()))
            .collect();
        all.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        all.truncate(limit);
        all
    }

    /// Get rank within a specific category.
    pub fn get_category_rank(&self, provider_id: &str, category: &str) -> Option<usize> {
        let provider_score = self
            .scores
            .get(provider_id)
            .and_then(|e| e.category_scores.get(category).copied())?;

        let mut higher = 0usize;
        for entry in self.scores.iter() {
            if let Some(&other_score) = entry.category_scores.get(category) {
                if other_score > provider_score {
                    higher += 1;
                }
            }
        }
        Some(higher + 1)
    }

    /// Bulk update multiple provider scores.
    pub fn bulk_update(&self, req: &BulkUpdateRequest) -> Vec<(String, Result<ReputationScore, String>)> {
        req.updates
            .iter()
            .map(|u| (u.provider_id.clone(), self.update_score(u)))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// REST Handlers (public, consumed by proxy.rs)
// ---------------------------------------------------------------------------

pub async fn get_reputation_handler(
    State(state): State<super::proxy::AppState>,
    Path(provider_id): Path<String>,
) -> Json<serde_json::Value> {
    match state.reputation_engine.get_score(&provider_id) {
        Some(score) => Json(serde_json::to_value(score).unwrap_or_default()),
        None => Json(serde_json::json!({ "error": "provider_not_found" })),
    }
}

pub async fn get_reputation_tier_handler(
    State(state): State<super::proxy::AppState>,
    Path(provider_id): Path<String>,
) -> Json<serde_json::Value> {
    match state.reputation_engine.get_tier(&provider_id) {
        Some(tier) => Json(serde_json::json!({
            "provider_id": provider_id,
            "tier": tier.as_str(),
            "level": tier.level(),
        })),
        None => Json(serde_json::json!({ "error": "provider_not_found" })),
    }
}

pub async fn get_leaderboard_handler(
    State(state): State<super::proxy::AppState>,
) -> Json<serde_json::Value> {
    let leaderboard = state.reputation_engine.get_leaderboard(50);
    let entries: Vec<serde_json::Value> = leaderboard
        .into_iter()
        .enumerate()
        .map(|(i, (pid, score, tier))| {
            serde_json::json!({
                "rank": i + 1,
                "provider_id": pid,
                "score": score,
                "tier": tier.as_str(),
            })
        })
        .collect();
    Json(serde_json::json!({ "leaderboard": entries }))
}

pub async fn recalculate_handler(
    State(state): State<super::proxy::AppState>,
) -> Json<serde_json::Value> {
    let count = state.reputation_engine.recalculate();
    Json(serde_json::json!({
        "status": "recalculated",
        "providers_updated": count,
    }))
}

pub async fn get_history_handler(
    State(state): State<super::proxy::AppState>,
    Path(provider_id): Path<String>,
) -> Json<serde_json::Value> {
    let history = state.reputation_engine.get_history(&provider_id, 50);
    Json(serde_json::json!({
        "provider_id": provider_id,
        "history": history,
    }))
}

pub async fn get_config_handler(
    State(state): State<super::proxy::AppState>,
) -> Json<serde_json::Value> {
    let config = state.reputation_engine.get_config();
    Json(serde_json::to_value(config).unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> ReputationEngine {
        ReputationEngine::new()
    }

    #[test]
    fn test_default_config() {
        let config = ReputationConfig::default();
        assert_eq!(config.decay_rate, 0.95);
        assert_eq!(config.min_reviews, 5);
        assert_eq!(config.review_window_days, 90);
        assert!(config.tier_thresholds.contains_key("Gold"));
        assert!(config.weighting.contains_key("ResponseTime"));
    }

    #[test]
    fn test_new_reputation_score() {
        let config = ReputationConfig::default();
        let score = ReputationScore::new("provider-1", &config);
        assert_eq!(score.provider_id, "provider-1");
        assert_eq!(score.overall_score, 50.0);
        assert_eq!(score.tier, ReputationTier::Bronze);
    }

    #[test]
    fn test_update_score() {
        let engine = make_engine();
        let req = UpdateScoreRequest {
            provider_id: "p1".to_string(),
            factor: "Accuracy".to_string(),
            score: 90.0,
            reason: Some("Great performance".to_string()),
        };
        let result = engine.update_score(&req).unwrap();
        assert!(result.overall_score > 50.0);
        assert_eq!(result.total_reviews, 1);
        assert!(!result.history.is_empty());
    }

    #[test]
    fn test_get_score() {
        let engine = make_engine();
        assert!(engine.get_score("nonexistent").is_none());

        let req = UpdateScoreRequest {
            provider_id: "p1".to_string(),
            factor: "Uptime".to_string(),
            score: 80.0,
            reason: None,
        };
        engine.update_score(&req).unwrap();
        assert!(engine.get_score("p1").is_some());
    }

    #[test]
    fn test_get_tier() {
        let engine = make_engine();
        // No provider yet
        assert!(engine.get_tier("p1").is_none());

        let req = UpdateScoreRequest {
            provider_id: "p1".to_string(),
            factor: "UserSatisfaction".to_string(),
            score: 95.0,
            reason: None,
        };
        engine.update_score(&req).unwrap();
        let tier = engine.get_tier("p1").unwrap();
        // Should be at least Silver or higher
        assert!(tier.level() >= 1);
    }

    #[test]
    fn test_tier_determination() {
        let config = ReputationConfig::default();
        assert_eq!(
            ReputationScore::determine_tier(0.0, &config.tier_thresholds),
            ReputationTier::Bronze
        );
        assert_eq!(
            ReputationScore::determine_tier(25.0, &config.tier_thresholds),
            ReputationTier::Silver
        );
        assert_eq!(
            ReputationScore::determine_tier(60.0, &config.tier_thresholds),
            ReputationTier::Gold
        );
        assert_eq!(
            ReputationScore::determine_tier(80.0, &config.tier_thresholds),
            ReputationTier::Platinum
        );
        assert_eq!(
            ReputationScore::determine_tier(95.0, &config.tier_thresholds),
            ReputationTier::Diamond
        );
    }

    #[test]
    fn test_get_history() {
        let engine = make_engine();
        let req = UpdateScoreRequest {
            provider_id: "p1".to_string(),
            factor: "Uptime".to_string(),
            score: 80.0,
            reason: Some("test".to_string()),
        };
        engine.update_score(&req).unwrap();
        let history = engine.get_history("p1", 10);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].factor, "Uptime");
    }

    #[test]
    fn test_recalculate() {
        let engine = make_engine();
        engine.update_score(&UpdateScoreRequest {
            provider_id: "p1".to_string(),
            factor: "Uptime".to_string(),
            score: 70.0,
            reason: None,
        }).unwrap();
        engine.update_score(&UpdateScoreRequest {
            provider_id: "p2".to_string(),
            factor: "Uptime".to_string(),
            score: 90.0,
            reason: None,
        }).unwrap();
        let count = engine.recalculate();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_apply_decay() {
        let engine = make_engine();
        engine.update_score(&UpdateScoreRequest {
            provider_id: "p1".to_string(),
            factor: "Accuracy".to_string(),
            score: 100.0,
            reason: None,
        }).unwrap();
        let count = engine.apply_decay();
        assert_eq!(count, 1);
        let score = engine.get_score("p1").unwrap();
        // Score should have decayed toward 50
        assert!(score.overall_score < 100.0);
    }

    #[test]
    fn test_leaderboard() {
        let engine = make_engine();
        for i in 0..5 {
            engine.update_score(&UpdateScoreRequest {
                provider_id: format!("p{}", i),
                factor: "Accuracy".to_string(),
                score: 50.0 + (i as f64) * 10.0,
                reason: None,
            }).unwrap();
        }
        let lb = engine.get_leaderboard(3);
        assert_eq!(lb.len(), 3);
        assert_eq!(lb[0].0, "p4");
        assert_eq!(lb[2].0, "p2");
    }

    #[test]
    fn test_category_rank() {
        let engine = make_engine();
        engine.update_score(&UpdateScoreRequest {
            provider_id: "p1".to_string(),
            factor: "Uptime".to_string(),
            score: 60.0,
            reason: None,
        }).unwrap();
        engine.update_score(&UpdateScoreRequest {
            provider_id: "p2".to_string(),
            factor: "Uptime".to_string(),
            score: 90.0,
            reason: None,
        }).unwrap();
        let rank = engine.get_category_rank("p1", "Uptime").unwrap();
        assert_eq!(rank, 2);
    }

    #[test]
    fn test_bulk_update() {
        let engine = make_engine();
        let req = BulkUpdateRequest {
            updates: vec![
                UpdateScoreRequest {
                    provider_id: "p1".to_string(),
                    factor: "Uptime".to_string(),
                    score: 80.0,
                    reason: None,
                },
                UpdateScoreRequest {
                    provider_id: "p2".to_string(),
                    factor: "Uptime".to_string(),
                    score: 90.0,
                    reason: None,
                },
            ],
        };
        let results = engine.bulk_update(&req);
        assert_eq!(results.len(), 2);
        assert!(results[0].1.is_ok());
        assert!(results[1].1.is_ok());
    }

    #[test]
    fn test_review_weight() {
        let rw = ReviewWeight::new(0.8, 0.9, 0.7);
        assert!(rw.total_weight > 0.0);
        assert!(rw.total_weight <= 1.0);
    }

    #[test]
    fn test_factor_from_str() {
        assert_eq!(
            ReputationFactor::from_str("ResponseTime"),
            Some(ReputationFactor::ResponseTime)
        );
        assert_eq!(ReputationFactor::from_str("Invalid"), None);
    }
}


