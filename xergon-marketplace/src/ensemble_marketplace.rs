//! Ensemble Marketplace for the Xergon Network marketplace.
//!
//! Provides model group bundles, performance comparison, and routing strategy
//! marketplace features. Users can discover curated model ensembles, compare
//! their performance metrics, and purchase or list routing strategies.
//!
//! Uses a dark theme consistent with other marketplace pages.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Html,
    Json, Router,
};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// Data Types
// ---------------------------------------------------------------------------

/// Category for model group bundles.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum BundleCategory {
    Inference,
    Creative,
    Code,
    Embedding,
}

impl std::fmt::Display for BundleCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BundleCategory::Inference => write!(f, "Inference"),
            BundleCategory::Creative => write!(f, "Creative"),
            BundleCategory::Code => write!(f, "Code"),
            BundleCategory::Embedding => write!(f, "Embedding"),
        }
    }
}

/// Pricing strategy for bundles.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum PricingStrategy {
    PerToken,
    PerRequest,
    Bid,
}

impl std::fmt::Display for PricingStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PricingStrategy::PerToken => write!(f, "PerToken"),
            PricingStrategy::PerRequest => write!(f, "PerRequest"),
            PricingStrategy::Bid => write!(f, "Bid"),
        }
    }
}

/// Comparison metric type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum ComparisonType {
    Latency,
    Throughput,
    Quality,
    Cost,
}

impl std::fmt::Display for ComparisonType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComparisonType::Latency => write!(f, "Latency"),
            ComparisonType::Throughput => write!(f, "Throughput"),
            ComparisonType::Quality => write!(f, "Quality"),
            ComparisonType::Cost => write!(f, "Cost"),
        }
    }
}

/// Routing strategy type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum StrategyType {
    RoundRobin,
    LeastLatency,
    CostOptimized,
    QualityFirst,
    Adaptive,
}

impl std::fmt::Display for StrategyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StrategyType::RoundRobin => write!(f, "RoundRobin"),
            StrategyType::LeastLatency => write!(f, "LeastLatency"),
            StrategyType::CostOptimized => write!(f, "CostOptimized"),
            StrategyType::QualityFirst => write!(f, "QualityFirst"),
            StrategyType::Adaptive => write!(f, "Adaptive"),
        }
    }
}

/// Review target type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum ReviewType {
    Bundle,
    Strategy,
}

/// A curated group of models sold as a bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelGroupBundle {
    pub id: String,
    pub name: String,
    pub description: String,
    pub model_ids: Vec<String>,
    pub category: BundleCategory,
    pub pricing_strategy: PricingStrategy,
    pub base_price: f64,
    pub performance_score: f64,
    pub popularity: u64,
    pub rating: f64,
    pub created_at: String,
    pub updated_at: String,
    pub featured: bool,
}

/// A single performance metric comparison entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceComparison {
    pub id: String,
    pub bundle_id: String,
    pub metric_name: String,
    pub model_id: String,
    pub value: f64,
    pub timestamp: String,
    pub comparison_type: ComparisonType,
}

/// A routing strategy listed on the marketplace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingStrategyListing {
    pub id: String,
    pub name: String,
    pub description: String,
    pub strategy_type: StrategyType,
    pub configuration: serde_json::Value,
    pub price_per_request: f64,
    pub rating: f64,
    pub created_at: String,
    pub author: String,
}

/// A user review for a bundle or strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Review {
    pub id: String,
    pub item_id: String,
    pub review_type: ReviewType,
    pub rating: u8,
    pub comment: String,
    pub reviewer: String,
    pub created_at: String,
}

/// Activity feed item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityItem {
    pub id: String,
    pub activity_type: String,
    pub description: String,
    pub timestamp: u64,
}

/// Marketplace statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnsembleMarketplaceStats {
    pub total_bundles: u64,
    pub total_strategies: u64,
    pub total_reviews: u64,
    pub average_bundle_rating: f64,
    pub average_strategy_rating: f64,
    pub featured_bundles: u64,
    pub top_category: String,
}

// ---------------------------------------------------------------------------
// Request / Query Types
// ---------------------------------------------------------------------------

/// Input for creating a new bundle.
#[derive(Debug, Deserialize)]
pub struct CreateBundleRequest {
    pub name: String,
    pub description: String,
    pub model_ids: Vec<String>,
    pub category: BundleCategory,
    pub pricing_strategy: PricingStrategy,
    pub base_price: f64,
}

/// Input for updating a bundle.
#[derive(Debug, Deserialize)]
pub struct UpdateBundleRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub model_ids: Option<Vec<String>>,
    pub category: Option<BundleCategory>,
    pub pricing_strategy: Option<PricingStrategy>,
    pub base_price: Option<f64>,
    pub featured: Option<bool>,
}

/// Query parameters for listing bundles.
#[derive(Debug, Deserialize)]
pub struct BundleListQuery {
    pub category: Option<String>,
    pub search: Option<String>,
    pub sort: Option<String>,
    pub limit: Option<usize>,
}

/// Input for creating a routing strategy listing.
#[derive(Debug, Deserialize)]
pub struct CreateStrategyRequest {
    pub name: String,
    pub description: String,
    pub strategy_type: StrategyType,
    pub configuration: serde_json::Value,
    pub price_per_request: f64,
    pub author: String,
}

/// Input for recording a performance comparison.
#[derive(Debug, Deserialize)]
pub struct RecordComparisonRequest {
    pub bundle_id: String,
    pub metric_name: String,
    pub model_id: String,
    pub value: f64,
    pub comparison_type: ComparisonType,
}

/// Input for adding a review.
#[derive(Debug, Deserialize)]
pub struct AddReviewRequest {
    pub item_id: String,
    pub review_type: ReviewType,
    pub rating: u8,
    pub comment: String,
    pub reviewer: String,
}

/// Leaderboard query parameters.
#[derive(Debug, Deserialize)]
pub struct LeaderboardQuery {
    pub metric: Option<String>,
    pub limit: Option<usize>,
}

// ---------------------------------------------------------------------------
// EnsembleMarketplace
// ---------------------------------------------------------------------------

/// The main ensemble marketplace managing bundles, strategies, comparisons,
/// and reviews.
pub struct EnsembleMarketplace {
    bundles: DashMap<String, ModelGroupBundle>,
    comparisons: DashMap<String, PerformanceComparison>,
    strategies: DashMap<String, RoutingStrategyListing>,
    reviews: DashMap<String, Review>,
    activity: Mutex<VecDeque<ActivityItem>>,
    activity_counter: AtomicU64,
    comparison_counter: AtomicU64,
    review_counter: AtomicU64,
    max_activity: usize,
}

impl EnsembleMarketplace {
    /// Create a new empty ensemble marketplace.
    pub fn new() -> Self {
        Self {
            bundles: DashMap::new(),
            comparisons: DashMap::new(),
            strategies: DashMap::new(),
            reviews: DashMap::new(),
            activity: Mutex::new(VecDeque::with_capacity(500)),
            activity_counter: AtomicU64::new(0),
            comparison_counter: AtomicU64::new(0),
            review_counter: AtomicU64::new(0),
            max_activity: 500,
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn now_iso() -> String {
        Utc::now().to_rfc3339()
    }

    fn now_millis() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    fn record_activity(&self, activity_type: &str, description: &str) {
        let mut feed = self.activity.lock().unwrap_or_else(|e| e.into_inner());
        if feed.len() >= self.max_activity {
            feed.pop_front();
        }
        feed.push_back(ActivityItem {
            id: format!(
                "ea_{}",
                self.activity_counter.fetch_add(1, Ordering::Relaxed)
            ),
            activity_type: activity_type.to_string(),
            description: description.to_string(),
            timestamp: Self::now_millis(),
        });
    }

    // -----------------------------------------------------------------------
    // Bundle CRUD
    // -----------------------------------------------------------------------

    /// Create a new model group bundle.
    pub fn create_bundle(&self, req: CreateBundleRequest) -> ModelGroupBundle {
        let now = Self::now_iso();
        let bundle = ModelGroupBundle {
            id: format!("bundle_{}", uuid::Uuid::new_v4().simple()),
            name: req.name,
            description: req.description,
            model_ids: req.model_ids,
            category: req.category,
            pricing_strategy: req.pricing_strategy,
            base_price: req.base_price,
            performance_score: 0.0,
            popularity: 0,
            rating: 0.0,
            created_at: now.clone(),
            updated_at: now,
            featured: false,
        };
        self.record_activity(
            "bundle_created",
            &format!("Bundle '{}' created", bundle.name),
        );
        info!(bundle_id = %bundle.id, "Created new ensemble bundle");
        self.bundles.insert(bundle.id.clone(), bundle.clone());
        bundle
    }

    /// List bundles with optional filtering and sorting.
    pub fn list_bundles(
        &self,
        category: Option<&str>,
        search: Option<&str>,
        sort: Option<&str>,
        limit: usize,
    ) -> Vec<ModelGroupBundle> {
        let search_lower = search.map(|s| s.to_lowercase());
        let mut results: Vec<ModelGroupBundle> = self
            .bundles
            .iter()
            .filter(|e| {
                let b = e.value();
                if let Some(ref cat) = category {
                    if b.category.to_string() != *cat {
                        return false;
                    }
                }
                if let Some(ref q) = search_lower {
                    if !b.name.to_lowercase().contains(q)
                        && !b.description.to_lowercase().contains(q)
                    {
                        return false;
                    }
                }
                true
            })
            .map(|e| e.value().clone())
            .collect();

        match sort.unwrap_or("created_at") {
            "rating" => results.sort_by(|a, b| b.rating.partial_cmp(&a.rating).unwrap_or(std::cmp::Ordering::Equal)),
            "performance" => results.sort_by(|a, b| b.performance_score.partial_cmp(&a.performance_score).unwrap_or(std::cmp::Ordering::Equal)),
            "popularity" => results.sort_by(|a, b| b.popularity.cmp(&a.popularity)),
            "price_asc" => results.sort_by(|a, b| a.base_price.partial_cmp(&b.base_price).unwrap_or(std::cmp::Ordering::Equal)),
            "price_desc" => results.sort_by(|a, b| b.base_price.partial_cmp(&a.base_price).unwrap_or(std::cmp::Ordering::Equal)),
            _ => results.sort_by(|a, b| b.created_at.cmp(&a.created_at)),
        }

        results.truncate(limit);
        results
    }

    /// Get a single bundle by ID.
    pub fn get_bundle(&self, id: &str) -> Option<ModelGroupBundle> {
        self.bundles.get(id).map(|r| r.value().clone())
    }

    /// Update an existing bundle.
    pub fn update_bundle(&self, id: &str, req: UpdateBundleRequest) -> Result<ModelGroupBundle, String> {
        let mut bundle = self
            .bundles
            .get_mut(id)
            .ok_or_else(|| "Bundle not found".to_string())?;

        if let Some(name) = req.name {
            bundle.name = name;
        }
        if let Some(desc) = req.description {
            bundle.description = desc;
        }
        if let Some(ids) = req.model_ids {
            bundle.model_ids = ids;
        }
        if let Some(cat) = req.category {
            bundle.category = cat;
        }
        if let Some(ps) = req.pricing_strategy {
            bundle.pricing_strategy = ps;
        }
        if let Some(price) = req.base_price {
            bundle.base_price = price;
        }
        if let Some(feat) = req.featured {
            bundle.featured = feat;
        }
        bundle.updated_at = Self::now_iso();

        self.record_activity(
            "bundle_updated",
            &format!("Bundle '{}' updated", bundle.name),
        );
        debug!(bundle_id = id, "Bundle updated");
        Ok(bundle.clone())
    }

    /// Delete a bundle by ID.
    pub fn delete_bundle(&self, id: &str) -> Result<(), String> {
        let removed = self.bundles.remove(id);
        match removed {
            Some((_, bundle)) => {
                self.record_activity(
                    "bundle_deleted",
                    &format!("Bundle '{}' deleted", bundle.name),
                );
                info!(bundle_id = id, "Bundle deleted");
                Ok(())
            }
            None => Err("Bundle not found".to_string()),
        }
    }

    /// Search bundles by a text query.
    pub fn search_bundles(&self, query: &str, limit: usize) -> Vec<ModelGroupBundle> {
        let q = query.to_lowercase();
        self.bundles
            .iter()
            .filter(|e| {
                let b = e.value();
                b.name.to_lowercase().contains(&q)
                    || b.description.to_lowercase().contains(&q)
            })
            .map(|e| e.value().clone())
            .take(limit)
            .collect()
    }

    /// Toggle the featured flag on a bundle.
    pub fn feature_bundle(&self, id: &str, featured: bool) -> Result<(), String> {
        let mut bundle = self
            .bundles
            .get_mut(id)
            .ok_or_else(|| "Bundle not found".to_string())?;
        bundle.featured = featured;
        bundle.updated_at = Self::now_iso();
        let action = if featured { "featured" } else { "unfeatured" };
        self.record_activity(
            "bundle_featured",
            &format!("Bundle '{}' {}", bundle.name, action),
        );
        info!(bundle_id = id, featured = featured, "Bundle featured status changed");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Performance Comparisons
    // -----------------------------------------------------------------------

    /// Record a performance comparison data point.
    pub fn record_comparison(&self, req: RecordComparisonRequest) -> PerformanceComparison {
        let comp = PerformanceComparison {
            id: format!(
                "cmp_{}",
                self.comparison_counter.fetch_add(1, Ordering::Relaxed)
            ),
            bundle_id: req.bundle_id,
            metric_name: req.metric_name,
            model_id: req.model_id,
            value: req.value,
            timestamp: Self::now_iso(),
            comparison_type: req.comparison_type,
        };
        self.comparisons.insert(comp.id.clone(), comp.clone());
        self.record_activity(
            "comparison_recorded",
            &format!(
                "Comparison '{}' recorded for bundle '{}'",
                comp.metric_name, comp.bundle_id
            ),
        );
        comp
    }

    /// Get all comparisons for a specific bundle.
    pub fn get_comparisons_for_bundle(&self, bundle_id: &str) -> Vec<PerformanceComparison> {
        self.comparisons
            .iter()
            .filter(|e| e.value().bundle_id == bundle_id)
            .map(|e| e.value().clone())
            .collect()
    }

    /// Get a leaderboard of models for a given metric.
    pub fn get_leaderboard(&self, metric: Option<&str>, limit: usize) -> Vec<PerformanceComparison> {
        let mut results: Vec<PerformanceComparison> = self
            .comparisons
            .iter()
            .filter(|e| {
                metric
                    .map(|m| e.value().metric_name == m)
                    .unwrap_or(true)
            })
            .map(|e| e.value().clone())
            .collect();
        // Higher is better, sort descending
        results.sort_by(|a, b| b.value.partial_cmp(&a.value).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);
        results
    }

    // -----------------------------------------------------------------------
    // Routing Strategy CRUD
    // -----------------------------------------------------------------------

    /// Create a new routing strategy listing.
    pub fn create_strategy(&self, req: CreateStrategyRequest) -> RoutingStrategyListing {
        let strategy = RoutingStrategyListing {
            id: format!("strat_{}", uuid::Uuid::new_v4().simple()),
            name: req.name,
            description: req.description,
            strategy_type: req.strategy_type,
            configuration: req.configuration,
            price_per_request: req.price_per_request,
            rating: 0.0,
            created_at: Self::now_iso(),
            author: req.author,
        };
        self.record_activity(
            "strategy_created",
            &format!("Strategy '{}' created by '{}'", strategy.name, strategy.author),
        );
        info!(strategy_id = %strategy.id, "Created new routing strategy listing");
        self.strategies.insert(strategy.id.clone(), strategy.clone());
        strategy
    }

    /// List all routing strategies.
    pub fn list_strategies(&self, limit: usize) -> Vec<RoutingStrategyListing> {
        let mut results: Vec<RoutingStrategyListing> = self
            .strategies
            .iter()
            .map(|e| e.value().clone())
            .collect();
        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        results.truncate(limit);
        results
    }

    /// Get a single routing strategy by ID.
    pub fn get_strategy(&self, id: &str) -> Option<RoutingStrategyListing> {
        self.strategies.get(id).map(|r| r.value().clone())
    }

    /// Update an existing routing strategy.
    pub fn update_strategy(
        &self,
        id: &str,
        name: Option<String>,
        description: Option<String>,
        configuration: Option<serde_json::Value>,
        price_per_request: Option<f64>,
    ) -> Result<RoutingStrategyListing, String> {
        let mut strat = self
            .strategies
            .get_mut(id)
            .ok_or_else(|| "Strategy not found".to_string())?;

        if let Some(n) = name {
            strat.name = n;
        }
        if let Some(d) = description {
            strat.description = d;
        }
        if let Some(c) = configuration {
            strat.configuration = c;
        }
        if let Some(p) = price_per_request {
            strat.price_per_request = p;
        }

        self.record_activity(
            "strategy_updated",
            &format!("Strategy '{}' updated", strat.name),
        );
        debug!(strategy_id = id, "Strategy updated");
        Ok(strat.clone())
    }

    /// Delete a routing strategy by ID.
    pub fn delete_strategy(&self, id: &str) -> Result<(), String> {
        let removed = self.strategies.remove(id);
        match removed {
            Some((_, strat)) => {
                self.record_activity(
                    "strategy_deleted",
                    &format!("Strategy '{}' deleted", strat.name),
                );
                info!(strategy_id = id, "Strategy deleted");
                Ok(())
            }
            None => Err("Strategy not found".to_string()),
        }
    }

    // -----------------------------------------------------------------------
    // Reviews
    // -----------------------------------------------------------------------

    /// Add a review for a bundle or strategy.
    pub fn add_review(&self, req: AddReviewRequest) -> Result<Review, String> {
        if req.rating < 1 || req.rating > 5 {
            return Err("Rating must be between 1 and 5".to_string());
        }

        let item_id = req.item_id.clone();
        let review_type = req.review_type.clone();
        let reviewer = req.reviewer.clone();

        let review = Review {
            id: format!(
                "rev_{}",
                self.review_counter.fetch_add(1, Ordering::Relaxed)
            ),
            item_id: req.item_id,
            review_type: req.review_type,
            rating: req.rating,
            comment: req.comment,
            reviewer: req.reviewer,
            created_at: Self::now_iso(),
        };

        // Insert the review first so it's included in the average calculation
        self.reviews.insert(review.id.clone(), review.clone());

        // Update the rating on the target item
        match review_type {
            ReviewType::Bundle => {
                if let Some(mut bundle) = self.bundles.get_mut(&item_id) {
                    let reviews = self.list_reviews_for_item(&item_id, &ReviewType::Bundle);
                    let total: f64 = reviews.iter().map(|r| r.rating as f64).sum();
                    let count = reviews.len() as f64;
                    bundle.rating = total / count;
                }
            }
            ReviewType::Strategy => {
                if let Some(mut strat) = self.strategies.get_mut(&item_id) {
                    let reviews = self.list_reviews_for_item(&item_id, &ReviewType::Strategy);
                    let total: f64 = reviews.iter().map(|r| r.rating as f64).sum();
                    let count = reviews.len() as f64;
                    strat.rating = total / count;
                }
            }
        }

        self.record_activity(
            "review_added",
            &format!(
                "{} review ({} stars) by '{}'",
                match review_type {
                    ReviewType::Bundle => "Bundle",
                    ReviewType::Strategy => "Strategy",
                },
                review.rating,
                reviewer
            ),
        );
        Ok(review)
    }

    /// List reviews for a specific item.
    pub fn list_reviews_for_item(&self, item_id: &str, review_type: &ReviewType) -> Vec<Review> {
        self.reviews
            .iter()
            .filter(|e| e.value().item_id == item_id && e.value().review_type == *review_type)
            .map(|e| e.value().clone())
            .collect()
    }

    /// Get the average rating for an item.
    pub fn get_average_rating(&self, item_id: &str, review_type: &ReviewType) -> Option<f64> {
        let reviews = self.list_reviews_for_item(item_id, review_type);
        if reviews.is_empty() {
            return None;
        }
        let total: f64 = reviews.iter().map(|r| r.rating as f64).sum();
        let count = reviews.len() as f64;
        Some(total / count)
    }

    // -----------------------------------------------------------------------
    // Stats
    // -----------------------------------------------------------------------

    /// Get marketplace-wide statistics.
    pub fn get_stats(&self) -> EnsembleMarketplaceStats {
        let total_bundles = self.bundles.len() as u64;
        let total_strategies = self.strategies.len() as u64;
        let total_reviews = self.reviews.len() as u64;

        let avg_bundle_rating: f64 = if total_bundles > 0 {
            let sum: f64 = self.bundles.iter().map(|e| e.value().rating).sum();
            sum / total_bundles as f64
        } else {
            0.0
        };

        let avg_strategy_rating: f64 = if total_strategies > 0 {
            let sum: f64 = self.strategies.iter().map(|e| e.value().rating).sum();
            sum / total_strategies as f64
        } else {
            0.0
        };

        let featured = self
            .bundles
            .iter()
            .filter(|e| e.value().featured)
            .count() as u64;

        // Determine the top category by count
        let mut cat_counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
        for entry in self.bundles.iter() {
            *cat_counts
                .entry(entry.value().category.to_string())
                .or_insert(0) += 1;
        }
        let top_category = cat_counts
            .into_iter()
            .max_by_key(|(_, c)| *c)
            .map(|(k, _)| k)
            .unwrap_or_else(|| "N/A".to_string());

        EnsembleMarketplaceStats {
            total_bundles,
            total_strategies,
            total_reviews,
            average_bundle_rating: avg_bundle_rating,
            average_strategy_rating: avg_strategy_rating,
            featured_bundles: featured,
            top_category,
        }
    }

    /// Get the activity feed.
    pub fn get_activity(&self, limit: usize) -> Vec<ActivityItem> {
        let feed = self.activity.lock().unwrap_or_else(|e| e.into_inner());
        feed.iter().rev().take(limit).cloned().collect()
    }
}

impl Default for EnsembleMarketplace {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// HTML Page
// ---------------------------------------------------------------------------

/// Generate the ensemble marketplace HTML page.
pub fn ensemble_marketplace_page(
    stats: &EnsembleMarketplaceStats,
    bundles: &[ModelGroupBundle],
    strategies: &[RoutingStrategyListing],
) -> String {
    let mut bundle_cards = String::new();
    for b in bundles.iter().take(12) {
        let cat_color = match b.category {
            BundleCategory::Inference => "#3b82f6",
            BundleCategory::Creative => "#a855f7",
            BundleCategory::Code => "#10b981",
            BundleCategory::Embedding => "#f59e0b",
        };
        let feat_badge = if b.featured {
            r#" <span style="background:#f59e0b;color:#000;padding:2px 6px;border-radius:4px;font-size:10px;font-weight:700;">FEATURED</span>"#
        } else {
            ""
        };
        let rating_str = format!("{:.1}", b.rating);
        let perf_str = format!("{:.1}", b.performance_score);
        let price_str = format!("{:.4}", b.base_price);
        bundle_cards.push_str(&format!(r#"
        <div style="background:#141414;border:1px solid #262626;border-radius:8px;padding:16px;margin-bottom:12px;">
          <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:8px;">
            <span style="font-weight:600;color:#e5e5e5;font-size:14px;">{name}</span>
            <div>
              <span style="background:{color};color:#000;padding:2px 8px;border-radius:4px;font-size:11px;font-weight:600;">{category}</span>{feat}
            </div>
          </div>
          <div style="color:#737373;font-size:12px;margin-bottom:8px;">{desc}</div>
          <div style="display:flex;gap:16px;font-size:12px;">
            <span style="color:#f59e0b;">Rating: {rating}/5</span>
            <span style="color:#10b981;">Perf: {perf}</span>
            <span style="color:#3b82f6;">Price: {price} ERG</span>
            <span style="color:#737373;">Models: {model_count}</span>
          </div>
        </div>"#,
            name = html_escape(&b.name),
            category = b.category,
            color = cat_color,
            desc = html_escape(&b.description),
            feat = feat_badge,
            rating = rating_str,
            perf = perf_str,
            price = price_str,
            model_count = b.model_ids.len(),
        ));
    }

    if bundle_cards.is_empty() {
        bundle_cards = r#"<div style="text-align:center;color:#737373;padding:40px;">No bundles listed yet</div>"#.to_string();
    }

    let mut strategy_cards = String::new();
    for s in strategies.iter().take(6) {
        let strat_strat_color = match s.strategy_type {
            StrategyType::RoundRobin => "#3b82f6",
            StrategyType::LeastLatency => "#10b981",
            StrategyType::CostOptimized => "#f59e0b",
            StrategyType::QualityFirst => "#a855f7",
            StrategyType::Adaptive => "#ef4444",
        };
        let s_rating_str = format!("{:.1}", s.rating);
        let s_price_str = format!("{:.6}", s.price_per_request);
        strategy_cards.push_str(&format!(r#"
        <div style="background:#141414;border:1px solid #262626;border-radius:8px;padding:16px;margin-bottom:12px;">
          <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:8px;">
            <span style="font-weight:600;color:#e5e5e5;font-size:14px;">{name}</span>
            <span style="background:{color};color:#000;padding:2px 8px;border-radius:4px;font-size:11px;font-weight:600;">{stype}</span>
          </div>
          <div style="color:#737373;font-size:12px;margin-bottom:8px;">{desc} &middot; by {author}</div>
          <div style="display:flex;gap:16px;font-size:12px;">
            <span style="color:#f59e0b;">Rating: {rating}/5</span>
            <span style="color:#3b82f6;">Price: {price} ERG/req</span>
          </div>
        </div>"#,
            name = html_escape(&s.name),
            stype = s.strategy_type,
            color = strat_strat_color,
            desc = html_escape(&s.description),
            author = html_escape(&s.author),
            rating = s_rating_str,
            price = s_price_str,
        ));
    }

    if strategy_cards.is_empty() {
        strategy_cards = r#"<div style="text-align:center;color:#737373;padding:40px;">No strategies listed yet</div>"#.to_string();
    }

    let avg_bundle_str = format!("{:.2}", stats.average_bundle_rating);
    let avg_strat_str = format!("{:.2}", stats.average_strategy_rating);

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Xergon Ensemble Marketplace</title>
</head>
<body style="background:#0a0a0a;color:#e5e5e5;font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;margin:0;padding:20px;">
<div style="max-width:1200px;margin:0 auto;">
  <h1 style="font-size:24px;font-weight:700;margin-bottom:4px;">Xergon Ensemble Marketplace</h1>
  <p style="color:#737373;font-size:13px;margin-bottom:24px;">Model group bundles, performance comparisons, and routing strategies</p>

  <!-- Stats Bar -->
  <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(150px,1fr));gap:12px;margin-bottom:24px;">
    <div style="background:#141414;border:1px solid #262626;border-radius:8px;padding:16px;">
      <div style="color:#737373;font-size:11px;text-transform:uppercase;letter-spacing:0.5px;">Bundles</div>
      <div style="font-size:24px;font-weight:700;color:#3b82f6;">{total_bundles}</div>
    </div>
    <div style="background:#141414;border:1px solid #262626;border-radius:8px;padding:16px;">
      <div style="color:#737373;font-size:11px;text-transform:uppercase;letter-spacing:0.5px;">Strategies</div>
      <div style="font-size:24px;font-weight:700;color:#10b981;">{total_strategies}</div>
    </div>
    <div style="background:#141414;border:1px solid #262626;border-radius:8px;padding:16px;">
      <div style="color:#737373;font-size:11px;text-transform:uppercase;letter-spacing:0.5px;">Reviews</div>
      <div style="font-size:24px;font-weight:700;color:#f59e0b;">{total_reviews}</div>
    </div>
    <div style="background:#141414;border:1px solid #262626;border-radius:8px;padding:16px;">
      <div style="color:#737373;font-size:11px;text-transform:uppercase;letter-spacing:0.5px;">Avg Bundle Rating</div>
      <div style="font-size:24px;font-weight:700;color:#a855f7;">{avg_bundle}</div>
    </div>
    <div style="background:#141414;border:1px solid #262626;border-radius:8px;padding:16px;">
      <div style="color:#737373;font-size:11px;text-transform:uppercase;letter-spacing:0.5px;">Avg Strategy Rating</div>
      <div style="font-size:24px;font-weight:700;color:#3b82f6;">{avg_strat}</div>
    </div>
    <div style="background:#141414;border:1px solid #262626;border-radius:8px;padding:16px;">
      <div style="color:#737373;font-size:11px;text-transform:uppercase;letter-spacing:0.5px;">Featured</div>
      <div style="font-size:24px;font-weight:700;color:#f59e0b;">{featured}</div>
    </div>
  </div>

  <!-- Main Grid -->
  <div style="display:grid;grid-template-columns:2fr 1fr;gap:20px;">
    <div>
      <h2 style="font-size:16px;font-weight:600;margin-bottom:12px;">Model Bundles</h2>
      {bundle_cards}
    </div>
    <div>
      <h2 style="font-size:16px;font-weight:600;margin-bottom:12px;">Routing Strategies</h2>
      {strategy_cards}
      <h2 style="font-size:16px;font-weight:600;margin:20px 0 12px;">Activity Feed</h2>
      <div id="activity-feed" style="background:#141414;border:1px solid #262626;border-radius:8px;padding:16px;min-height:200px;">
        <div style="color:#737373;font-size:13px;">Loading activity...</div>
      </div>
    </div>
  </div>
</div>
</body>
</html>"#,
        total_bundles = stats.total_bundles,
        total_strategies = stats.total_strategies,
        total_reviews = stats.total_reviews,
        avg_bundle = avg_bundle_str,
        avg_strat = avg_strat_str,
        featured = stats.featured_bundles,
        bundle_cards = bundle_cards,
        strategy_cards = strategy_cards,
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ---------------------------------------------------------------------------
// API Handlers
// ---------------------------------------------------------------------------

/// Shared state type for handlers.
pub type EnsembleState = Arc<EnsembleMarketplace>;

/// Handler: POST /api/ensemble/bundles
pub async fn create_bundle_handler(
    State(marketplace): State<EnsembleState>,
    Json(req): Json<CreateBundleRequest>,
) -> (StatusCode, Json<ModelGroupBundle>) {
    let bundle = marketplace.create_bundle(req);
    (StatusCode::CREATED, Json(bundle))
}

/// Handler: GET /api/ensemble/bundles
pub async fn list_bundles_handler(
    State(marketplace): State<EnsembleState>,
    Query(query): Query<BundleListQuery>,
) -> Json<Vec<ModelGroupBundle>> {
    let category = query.category.as_deref();
    let search = query.search.as_deref();
    let sort = query.sort.as_deref();
    let limit = query.limit.unwrap_or(50);
    let bundles = marketplace.list_bundles(category, search, sort, limit);
    Json(bundles)
}

/// Handler: GET /api/ensemble/bundles/:id
pub async fn get_bundle_handler(
    State(marketplace): State<EnsembleState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<Option<ModelGroupBundle>>) {
    let bundle = marketplace.get_bundle(&id);
    let status = if bundle.is_some() { StatusCode::OK } else { StatusCode::NOT_FOUND };
    (status, Json(bundle))
}

/// Handler: PUT /api/ensemble/bundles/:id
pub async fn update_bundle_handler(
    State(marketplace): State<EnsembleState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateBundleRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match marketplace.update_bundle(&id, req) {
        Ok(bundle) => (StatusCode::OK, Json(serde_json::json!({
            "status": "ok",
            "bundle": bundle,
        }))),
        Err(err) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"status": "error", "message": err})),
        ),
    }
}

/// Handler: DELETE /api/ensemble/bundles/:id
pub async fn delete_bundle_handler(
    State(marketplace): State<EnsembleState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match marketplace.delete_bundle(&id) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "ok", "message": "Bundle deleted"})),
        ),
        Err(err) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"status": "error", "message": err})),
        ),
    }
}

/// Handler: GET /api/ensemble/bundles/:id/comparisons
pub async fn get_comparisons_handler(
    State(marketplace): State<EnsembleState>,
    Path(id): Path<String>,
) -> Json<Vec<PerformanceComparison>> {
    let comparisons = marketplace.get_comparisons_for_bundle(&id);
    Json(comparisons)
}

/// Handler: POST /api/ensemble/strategies
pub async fn create_strategy_handler(
    State(marketplace): State<EnsembleState>,
    Json(req): Json<CreateStrategyRequest>,
) -> (StatusCode, Json<RoutingStrategyListing>) {
    let strategy = marketplace.create_strategy(req);
    (StatusCode::CREATED, Json(strategy))
}

/// Handler: GET /api/ensemble/strategies
pub async fn list_strategies_handler(
    State(marketplace): State<EnsembleState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Json<Vec<RoutingStrategyListing>> {
    let limit: usize = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);
    let strategies = marketplace.list_strategies(limit);
    Json(strategies)
}

/// Handler: GET /api/ensemble/strategies/:id
pub async fn get_strategy_handler(
    State(marketplace): State<EnsembleState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<Option<RoutingStrategyListing>>) {
    let strategy = marketplace.get_strategy(&id);
    let status = if strategy.is_some() { StatusCode::OK } else { StatusCode::NOT_FOUND };
    (status, Json(strategy))
}

/// Handler: PUT /api/ensemble/strategies/:id
pub async fn update_strategy_handler(
    State(marketplace): State<EnsembleState>,
    Path(id): Path<String>,
    Json(req): Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    let name = req.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
    let description = req.get("description").and_then(|v| v.as_str()).map(|s| s.to_string());
    let configuration = req.get("configuration").cloned();
    let price_per_request = req.get("price_per_request").and_then(|v| v.as_f64());
    match marketplace.update_strategy(&id, name, description, configuration, price_per_request) {
        Ok(strat) => (StatusCode::OK, Json(serde_json::json!({
            "status": "ok",
            "strategy": strat,
        }))),
        Err(err) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"status": "error", "message": err})),
        ),
    }
}

/// Handler: DELETE /api/ensemble/strategies/:id
pub async fn delete_strategy_handler(
    State(marketplace): State<EnsembleState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match marketplace.delete_strategy(&id) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "ok", "message": "Strategy deleted"})),
        ),
        Err(err) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"status": "error", "message": err})),
        ),
    }
}

/// Handler: GET /api/ensemble/stats
pub async fn get_stats_handler(
    State(marketplace): State<EnsembleState>,
) -> Json<EnsembleMarketplaceStats> {
    let stats = marketplace.get_stats();
    Json(stats)
}

/// Handler: POST /api/ensemble/reviews
pub async fn add_review_handler(
    State(marketplace): State<EnsembleState>,
    Json(req): Json<AddReviewRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match marketplace.add_review(req) {
        Ok(review) => (StatusCode::CREATED, Json(serde_json::json!({
            "status": "ok",
            "review": review,
        }))),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"status": "error", "message": err})),
        ),
    }
}

/// Handler: GET /api/ensemble/leaderboard
pub async fn get_leaderboard_handler(
    State(marketplace): State<EnsembleState>,
    Query(query): Query<LeaderboardQuery>,
) -> Json<Vec<PerformanceComparison>> {
    let metric = query.metric.as_deref();
    let limit = query.limit.unwrap_or(20);
    let leaderboard = marketplace.get_leaderboard(metric, limit);
    Json(leaderboard)
}

/// Handler: GET /api/ensemble/activity
pub async fn get_activity_handler(
    State(marketplace): State<EnsembleState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Json<Vec<ActivityItem>> {
    let limit: usize = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);
    let activity = marketplace.get_activity(limit);
    Json(activity)
}

/// Handler: GET /ensemble (HTML page)
pub async fn ensemble_page_handler(
    State(marketplace): State<EnsembleState>,
) -> Html<String> {
    let stats = marketplace.get_stats();
    let bundles = marketplace.list_bundles(None, None, Some("rating"), 12);
    let strategies = marketplace.list_strategies(6);
    let html = ensemble_marketplace_page(&stats, &bundles, &strategies);
    Html(html)
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn ensemble_marketplace_routes() -> Router<EnsembleState> {
    Router::new()
        .route("/api/ensemble/bundles", axum::routing::post(create_bundle_handler))
        .route("/api/ensemble/bundles", axum::routing::get(list_bundles_handler))
        .route("/api/ensemble/bundles/:id", axum::routing::get(get_bundle_handler))
        .route("/api/ensemble/bundles/:id", axum::routing::put(update_bundle_handler))
        .route("/api/ensemble/bundles/:id", axum::routing::delete(delete_bundle_handler))
        .route(
            "/api/ensemble/bundles/:id/comparisons",
            axum::routing::get(get_comparisons_handler),
        )
        .route(
            "/api/ensemble/strategies",
            axum::routing::post(create_strategy_handler),
        )
        .route(
            "/api/ensemble/strategies",
            axum::routing::get(list_strategies_handler),
        )
        .route(
            "/api/ensemble/strategies/:id",
            axum::routing::get(get_strategy_handler),
        )
        .route(
            "/api/ensemble/strategies/:id",
            axum::routing::put(update_strategy_handler),
        )
        .route(
            "/api/ensemble/strategies/:id",
            axum::routing::delete(delete_strategy_handler),
        )
        .route("/api/ensemble/stats", axum::routing::get(get_stats_handler))
        .route("/api/ensemble/reviews", axum::routing::post(add_review_handler))
        .route(
            "/api/ensemble/leaderboard",
            axum::routing::get(get_leaderboard_handler),
        )
        .route(
            "/api/ensemble/activity",
            axum::routing::get(get_activity_handler),
        )
        .route("/ensemble", axum::routing::get(ensemble_page_handler))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_marketplace() -> EnsembleMarketplace {
        EnsembleMarketplace::new()
    }

    fn sample_bundle_req(name: &str, category: BundleCategory) -> CreateBundleRequest {
        CreateBundleRequest {
            name: name.to_string(),
            description: format!("{} bundle description", name),
            model_ids: vec!["model_a".to_string(), "model_b".to_string()],
            category,
            pricing_strategy: PricingStrategy::PerRequest,
            base_price: 0.05,
        }
    }

    fn sample_strategy_req(name: &str, author: &str) -> CreateStrategyRequest {
        CreateStrategyRequest {
            name: name.to_string(),
            description: format!("{} strategy", name),
            strategy_type: StrategyType::LeastLatency,
            configuration: serde_json::json!({"timeout_ms": 5000}),
            price_per_request: 0.001,
            author: author.to_string(),
        }
    }

    // -----------------------------------------------------------------------
    // Bundle CRUD tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_bundle() {
        let mp = make_marketplace();
        let bundle = mp.create_bundle(sample_bundle_req("Inference Pro", BundleCategory::Inference));
        assert!(!bundle.id.is_empty());
        assert_eq!(bundle.name, "Inference Pro");
        assert_eq!(bundle.category, BundleCategory::Inference);
        assert_eq!(bundle.model_ids.len(), 2);
        assert!(!bundle.featured);
        assert_eq!(bundle.performance_score, 0.0);
    }

    #[test]
    fn test_get_bundle() {
        let mp = make_marketplace();
        let created = mp.create_bundle(sample_bundle_req("Code Gen", BundleCategory::Code));
        let fetched = mp.get_bundle(&created.id).unwrap();
        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.name, "Code Gen");
        let missing = mp.get_bundle("nonexistent");
        assert!(missing.is_none());
    }

    #[test]
    fn test_list_bundles_with_category_filter() {
        let mp = make_marketplace();
        mp.create_bundle(sample_bundle_req("Inf 1", BundleCategory::Inference));
        mp.create_bundle(sample_bundle_req("Code 1", BundleCategory::Code));
        mp.create_bundle(sample_bundle_req("Inf 2", BundleCategory::Inference));
        let inference = mp.list_bundles(Some("Inference"), None, None, 50);
        assert_eq!(inference.len(), 2);
        let code = mp.list_bundles(Some("Code"), None, None, 50);
        assert_eq!(code.len(), 1);
    }

    #[test]
    fn test_list_bundles_with_search() {
        let mp = make_marketplace();
        mp.create_bundle(sample_bundle_req("Alpha Bundle", BundleCategory::Inference));
        mp.create_bundle(sample_bundle_req("Beta Bundle", BundleCategory::Code));
        mp.create_bundle(sample_bundle_req("Gamma Bundle", BundleCategory::Creative));
        let results = mp.list_bundles(None, Some("alpha"), None, 50);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Alpha Bundle");
    }

    #[test]
    fn test_list_bundles_sort_by_popularity() {
        let mp = make_marketplace();
        let mut b1 = mp.create_bundle(sample_bundle_req("Popular", BundleCategory::Inference));
        let mut b2 = mp.create_bundle(sample_bundle_req("Niche", BundleCategory::Code));
        b1.popularity = 100;
        b2.popularity = 5;
        mp.bundles.insert(b1.id.clone(), b1.clone());
        mp.bundles.insert(b2.id.clone(), b2.clone());
        let results = mp.list_bundles(None, None, Some("popularity"), 50);
        assert_eq!(results[0].name, "Popular");
        assert_eq!(results[1].name, "Niche");
    }

    #[test]
    fn test_update_bundle() {
        let mp = make_marketplace();
        let created = mp.create_bundle(sample_bundle_req("Original", BundleCategory::Inference));
        let updated = mp.update_bundle(
            &created.id,
            UpdateBundleRequest {
                name: Some("Updated Name".to_string()),
                description: None,
                model_ids: None,
                category: None,
                pricing_strategy: None,
                base_price: Some(0.10),
                featured: Some(true),
            },
        ).unwrap();
        assert_eq!(updated.name, "Updated Name");
        assert!((updated.base_price - 0.10).abs() < 1e-9);
        assert!(updated.featured);
    }

    #[test]
    fn test_delete_bundle() {
        let mp = make_marketplace();
        let created = mp.create_bundle(sample_bundle_req("To Delete", BundleCategory::Creative));
        assert!(mp.get_bundle(&created.id).is_some());
        mp.delete_bundle(&created.id).unwrap();
        assert!(mp.get_bundle(&created.id).is_none());
        let err = mp.delete_bundle("nonexistent");
        assert!(err.is_err());
    }

    #[test]
    fn test_feature_bundle() {
        let mp = make_marketplace();
        let created = mp.create_bundle(sample_bundle_req("Feat Me", BundleCategory::Code));
        assert!(!created.featured);
        mp.feature_bundle(&created.id, true).unwrap();
        let b = mp.get_bundle(&created.id).unwrap();
        assert!(b.featured);
        mp.feature_bundle(&created.id, false).unwrap();
        let b = mp.get_bundle(&created.id).unwrap();
        assert!(!b.featured);
    }

    #[test]
    fn test_search_bundles() {
        let mp = make_marketplace();
        mp.create_bundle(sample_bundle_req("Fast Inference Pack", BundleCategory::Inference));
        mp.create_bundle(sample_bundle_req("Creative Studio", BundleCategory::Creative));
        let results = mp.search_bundles("fast", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Fast Inference Pack");
    }

    // -----------------------------------------------------------------------
    // Performance comparison tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_record_and_get_comparisons() {
        let mp = make_marketplace();
        let bundle = mp.create_bundle(sample_bundle_req("Comp Test", BundleCategory::Inference));
        mp.record_comparison(RecordComparisonRequest {
            bundle_id: bundle.id.clone(),
            metric_name: "latency_p99".to_string(),
            model_id: "model_a".to_string(),
            value: 42.5,
            comparison_type: ComparisonType::Latency,
        });
        mp.record_comparison(RecordComparisonRequest {
            bundle_id: bundle.id.clone(),
            metric_name: "throughput".to_string(),
            model_id: "model_b".to_string(),
            value: 1000.0,
            comparison_type: ComparisonType::Throughput,
        });
        let comps = mp.get_comparisons_for_bundle(&bundle.id);
        assert_eq!(comps.len(), 2);
    }

    #[test]
    fn test_leaderboard() {
        let mp = make_marketplace();
        let bundle = mp.create_bundle(sample_bundle_req("LB Test", BundleCategory::Code));
        mp.record_comparison(RecordComparisonRequest {
            bundle_id: bundle.id.clone(),
            metric_name: "quality_score".to_string(),
            model_id: "model_a".to_string(),
            value: 0.85,
            comparison_type: ComparisonType::Quality,
        });
        mp.record_comparison(RecordComparisonRequest {
            bundle_id: bundle.id.clone(),
            metric_name: "quality_score".to_string(),
            model_id: "model_b".to_string(),
            value: 0.92,
            comparison_type: ComparisonType::Quality,
        });
        let lb = mp.get_leaderboard(Some("quality_score"), 10);
        assert_eq!(lb.len(), 2);
        assert!((lb[0].value - 0.92).abs() < 1e-9);
        assert!((lb[1].value - 0.85).abs() < 1e-9);
    }

    // -----------------------------------------------------------------------
    // Strategy CRUD tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_create_and_get_strategy() {
        let mp = make_marketplace();
        let strat = mp.create_strategy(sample_strategy_req("Fast Router", "alice"));
        assert!(!strat.id.is_empty());
        assert_eq!(strat.name, "Fast Router");
        assert_eq!(strat.strategy_type, StrategyType::LeastLatency);
        let fetched = mp.get_strategy(&strat.id).unwrap();
        assert_eq!(fetched.author, "alice");
    }

    #[test]
    fn test_list_strategies() {
        let mp = make_marketplace();
        mp.create_strategy(sample_strategy_req("Strat 1", "alice"));
        mp.create_strategy(sample_strategy_req("Strat 2", "bob"));
        let list = mp.list_strategies(50);
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_update_strategy() {
        let mp = make_marketplace();
        let strat = mp.create_strategy(sample_strategy_req("Original", "alice"));
        let updated = mp.update_strategy(
            &strat.id,
            Some("Renamed".to_string()),
            None,
            Some(serde_json::json!({"timeout_ms": 3000})),
            Some(0.002),
        ).unwrap();
        assert_eq!(updated.name, "Renamed");
        assert!((updated.price_per_request - 0.002).abs() < 1e-9);
    }

    #[test]
    fn test_delete_strategy() {
        let mp = make_marketplace();
        let strat = mp.create_strategy(sample_strategy_req("To Delete", "alice"));
        mp.delete_strategy(&strat.id).unwrap();
        assert!(mp.get_strategy(&strat.id).is_none());
    }

    // -----------------------------------------------------------------------
    // Review tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_review_and_average_rating() {
        let mp = make_marketplace();
        let bundle = mp.create_bundle(sample_bundle_req("Reviewed", BundleCategory::Inference));
        mp.add_review(AddReviewRequest {
            item_id: bundle.id.clone(),
            review_type: ReviewType::Bundle,
            rating: 4,
            comment: "Great bundle".to_string(),
            reviewer: "alice".to_string(),
        }).unwrap();
        mp.add_review(AddReviewRequest {
            item_id: bundle.id.clone(),
            review_type: ReviewType::Bundle,
            rating: 5,
            comment: "Excellent".to_string(),
            reviewer: "bob".to_string(),
        }).unwrap();
        let avg = mp.get_average_rating(&bundle.id, &ReviewType::Bundle).unwrap();
        assert!((avg - 4.5).abs() < 1e-9);
        let reviews = mp.list_reviews_for_item(&bundle.id, &ReviewType::Bundle);
        assert_eq!(reviews.len(), 2);
    }

    #[test]
    fn test_review_invalid_rating() {
        let mp = make_marketplace();
        let bundle = mp.create_bundle(sample_bundle_req("Bad Review", BundleCategory::Code));
        let result = mp.add_review(AddReviewRequest {
            item_id: bundle.id.clone(),
            review_type: ReviewType::Bundle,
            rating: 0,
            comment: "Invalid".to_string(),
            reviewer: "alice".to_string(),
        });
        assert!(result.is_err());
        let result = mp.add_review(AddReviewRequest {
            item_id: bundle.id.clone(),
            review_type: ReviewType::Bundle,
            rating: 6,
            comment: "Also invalid".to_string(),
            reviewer: "alice".to_string(),
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_strategy_review() {
        let mp = make_marketplace();
        let strat = mp.create_strategy(sample_strategy_req("Reviewed Strat", "alice"));
        mp.add_review(AddReviewRequest {
            item_id: strat.id.clone(),
            review_type: ReviewType::Strategy,
            rating: 3,
            comment: "Decent".to_string(),
            reviewer: "bob".to_string(),
        }).unwrap();
        let avg = mp.get_average_rating(&strat.id, &ReviewType::Strategy).unwrap();
        assert!((avg - 3.0).abs() < 1e-9);
        let updated = mp.get_strategy(&strat.id).unwrap();
        assert!((updated.rating - 3.0).abs() < 1e-9);
    }

    // -----------------------------------------------------------------------
    // Stats tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_marketplace_stats() {
        let mp = make_marketplace();
        let stats = mp.get_stats();
        assert_eq!(stats.total_bundles, 0);
        assert_eq!(stats.total_strategies, 0);
        assert_eq!(stats.total_reviews, 0);

        mp.create_bundle(sample_bundle_req("S1", BundleCategory::Inference));
        mp.create_bundle(sample_bundle_req("S2", BundleCategory::Code));
        mp.create_bundle(sample_bundle_req("S3", BundleCategory::Inference));
        mp.create_strategy(sample_strategy_req("R1", "alice"));

        let stats = mp.get_stats();
        assert_eq!(stats.total_bundles, 3);
        assert_eq!(stats.total_strategies, 1);
        assert_eq!(stats.top_category, "Inference");
    }

    #[test]
    fn test_stats_average_ratings() {
        let mp = make_marketplace();
        let b = mp.create_bundle(sample_bundle_req("R", BundleCategory::Code));
        mp.add_review(AddReviewRequest {
            item_id: b.id.clone(),
            review_type: ReviewType::Bundle,
            rating: 4,
            comment: "Ok".to_string(),
            reviewer: "a".to_string(),
        }).unwrap();
        let stats = mp.get_stats();
        assert!((stats.average_bundle_rating - 4.0).abs() < 1e-9);
    }

    // -----------------------------------------------------------------------
    // Activity feed tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_activity_feed_recording() {
        let mp = make_marketplace();
        mp.create_bundle(sample_bundle_req("A1", BundleCategory::Inference));
        mp.create_strategy(sample_strategy_req("A2", "alice"));
        let activity = mp.get_activity(10);
        assert_eq!(activity.len(), 2);
        assert_eq!(activity[0].activity_type, "strategy_created");
        assert_eq!(activity[1].activity_type, "bundle_created");
    }

    // -----------------------------------------------------------------------
    // Concurrent access tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_concurrent_bundle_creation() {
        let mp = Arc::new(make_marketplace());
        let mut handles = vec![];
        for i in 0..10 {
            let m = mp.clone();
            handles.push(std::thread::spawn(move || {
                m.create_bundle(sample_bundle_req(
                    &format!("Concurrent {}", i),
                    BundleCategory::Inference,
                ));
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let all = mp.list_bundles(None, None, None, 100);
        assert_eq!(all.len(), 10);
    }

    #[test]
    fn test_concurrent_reviews() {
        let mp = Arc::new(make_marketplace());
        let bundle = mp.create_bundle(sample_bundle_req("CR", BundleCategory::Code));
        let bid = bundle.id.clone();
        let mut handles = vec![];
        for i in 0..5 {
            let m = mp.clone();
            let b_id = bid.clone();
            handles.push(std::thread::spawn(move || {
                m.add_review(AddReviewRequest {
                    item_id: b_id,
                    review_type: ReviewType::Bundle,
                    rating: (i % 5) as u8 + 1,
                    comment: format!("Review {}", i),
                    reviewer: format!("reviewer_{}", i),
                }).unwrap();
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let reviews = mp.list_reviews_for_item(&bid, &ReviewType::Bundle);
        assert_eq!(reviews.len(), 5);
    }

    // -----------------------------------------------------------------------
    // Serialization tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_bundle_serialization_roundtrip() {
        let mp = make_marketplace();
        let bundle = mp.create_bundle(sample_bundle_req("Ser", BundleCategory::Embedding));
        let json = serde_json::to_string(&bundle).unwrap();
        let decoded: ModelGroupBundle = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, bundle.id);
        assert_eq!(decoded.name, "Ser");
        assert_eq!(decoded.category, BundleCategory::Embedding);
    }

    #[test]
    fn test_strategy_serialization_roundtrip() {
        let mp = make_marketplace();
        let strat = mp.create_strategy(sample_strategy_req("Ser Strat", "alice"));
        let json = serde_json::to_string(&strat).unwrap();
        let decoded: RoutingStrategyListing = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, strat.id);
        assert_eq!(decoded.strategy_type, StrategyType::LeastLatency);
    }

    #[test]
    fn test_comparison_serialization_roundtrip() {
        let comp = PerformanceComparison {
            id: "cmp_1".to_string(),
            bundle_id: "bundle_1".to_string(),
            metric_name: "latency".to_string(),
            model_id: "m1".to_string(),
            value: 123.45,
            timestamp: "2025-01-01T00:00:00Z".to_string(),
            comparison_type: ComparisonType::Latency,
        };
        let json = serde_json::to_string(&comp).unwrap();
        let decoded: PerformanceComparison = serde_json::from_str(&json).unwrap();
        assert!((decoded.value - 123.45).abs() < 1e-9);
        assert_eq!(decoded.comparison_type, ComparisonType::Latency);
    }

    #[test]
    fn test_stats_serialization() {
        let stats = EnsembleMarketplaceStats {
            total_bundles: 5,
            total_strategies: 3,
            total_reviews: 20,
            average_bundle_rating: 4.2,
            average_strategy_rating: 3.8,
            featured_bundles: 2,
            top_category: "Inference".to_string(),
        };
        let json = serde_json::to_string(&stats).unwrap();
        let decoded: EnsembleMarketplaceStats = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_bundles, 5);
        assert!((decoded.average_bundle_rating - 4.2).abs() < 1e-9);
    }

    // -----------------------------------------------------------------------
    // HTML page tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_html_page_generation() {
        let stats = EnsembleMarketplaceStats {
            total_bundles: 2,
            total_strategies: 1,
            total_reviews: 5,
            average_bundle_rating: 4.5,
            average_strategy_rating: 3.8,
            featured_bundles: 1,
            top_category: "Code".to_string(),
        };
        let mp = make_marketplace();
        let bundles = mp.list_bundles(None, None, None, 0);
        let strategies = mp.list_strategies(0);
        let html = ensemble_marketplace_page(&stats, &bundles, &strategies);
        assert!(html.contains("Xergon Ensemble Marketplace"));
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("No bundles listed yet"));
    }

    #[test]
    fn test_html_page_with_data() {
        let mp = make_marketplace();
        let b = mp.create_bundle(sample_bundle_req("HTML Test", BundleCategory::Creative));
        mp.feature_bundle(&b.id, true).unwrap();
        let s = mp.create_strategy(sample_strategy_req("HTML Strat", "alice"));
        let stats = mp.get_stats();
        let bundles = mp.list_bundles(None, None, None, 12);
        let strategies = mp.list_strategies(6);
        let html = ensemble_marketplace_page(&stats, &bundles, &strategies);
        assert!(html.contains("HTML Test"));
        assert!(html.contains("FEATURED"));
        assert!(html.contains("HTML Strat"));
    }
}
