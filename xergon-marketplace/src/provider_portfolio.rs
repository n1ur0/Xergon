use std::collections::HashMap;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ===========================================================================
// PortfolioSection
// ===========================================================================

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum PortfolioSection {
    Overview,
    Models,
    Performance,
    Reviews,
    Pricing,
    Infrastructure,
}

impl PortfolioSection {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Overview => "overview",
            Self::Models => "models",
            Self::Performance => "performance",
            Self::Reviews => "reviews",
            Self::Pricing => "pricing",
            Self::Infrastructure => "infrastructure",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "overview" => Some(Self::Overview),
            "models" => Some(Self::Models),
            "performance" => Some(Self::Performance),
            "reviews" => Some(Self::Reviews),
            "pricing" => Some(Self::Pricing),
            "infrastructure" => Some(Self::Infrastructure),
            _ => None,
        }
    }
}

// ===========================================================================
// ProviderStats
// ===========================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProviderStats {
    pub total_models: u64,
    pub total_inferences: u64,
    pub avg_rating: f64,
    pub uptime_pct: f64,
    pub total_earnings: f64,
    pub active_since: DateTime<Utc>,
}

impl Default for ProviderStats {
    fn default() -> Self {
        Self {
            total_models: 0,
            total_inferences: 0,
            avg_rating: 0.0,
            uptime_pct: 100.0,
            total_earnings: 0.0,
            active_since: Utc::now(),
        }
    }
}

// ===========================================================================
// ModelCard
// ===========================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModelCard {
    pub model_id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub pricing: ModelCardPricing,
    pub rating: f64,
    pub inference_count: u64,
    pub status: ModelStatus,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ModelStatus {
    Active,
    Inactive,
    Beta,
    Deprecated,
}

impl std::fmt::Display for ModelStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Inactive => write!(f, "inactive"),
            Self::Beta => write!(f, "beta"),
            Self::Deprecated => write!(f, "deprecated"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ModelCardPricing {
    pub input_price: f64,
    pub output_price: f64,
    pub unit: String,
}

// ===========================================================================
// ReviewSummary
// ===========================================================================

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReviewSummary {
    pub avg_rating: f64,
    pub count: u64,
    pub distribution: HashMap<u32, u32>,
    pub recent_reviews: Vec<RecentReview>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RecentReview {
    pub review_id: String,
    pub user_id: String,
    pub rating: u32,
    pub text: String,
    pub created_at: DateTime<Utc>,
}

impl Default for ReviewSummary {
    fn default() -> Self {
        Self {
            avg_rating: 0.0,
            count: 0,
            distribution: HashMap::new(),
            recent_reviews: Vec::new(),
        }
    }
}

// ===========================================================================
// CachedPortfolio
// ===========================================================================

#[derive(Debug)]
struct CachedPortfolio {
    data: serde_json::Value,
    cached_at: DateTime<Utc>,
}

// ===========================================================================
// ProviderPortfolioConfig
// ===========================================================================

#[derive(Clone, Debug)]
pub struct ProviderPortfolioConfig {
    pub cache_ttl_secs: i64,
    pub max_recent_reviews: usize,
    pub max_featured_providers: usize,
}

impl Default for ProviderPortfolioConfig {
    fn default() -> Self {
        Self {
            cache_ttl_secs: 300, // 5 minutes
            max_recent_reviews: 10,
            max_featured_providers: 20,
        }
    }
}

// ===========================================================================
// ProviderPortfolio
// ===========================================================================

#[derive(Debug)]
pub struct ProviderPortfolio {
    providers: DashMap<String, ProviderStats>,
    model_cards: DashMap<String, Vec<ModelCard>>,
    review_summaries: DashMap<String, ReviewSummary>,
    cache: DashMap<String, CachedPortfolio>,
    config: ProviderPortfolioConfig,
    featured_list: DashMap<String, DateTime<Utc>>,
}

impl ProviderPortfolio {
    pub fn new() -> Self {
        Self {
            providers: DashMap::new(),
            model_cards: DashMap::new(),
            review_summaries: DashMap::new(),
            cache: DashMap::new(),
            config: ProviderPortfolioConfig::default(),
            featured_list: DashMap::new(),
        }
    }

    pub fn with_config(config: ProviderPortfolioConfig) -> Self {
        Self {
            config,
            ..Self::new()
        }
    }

    /// Get the full portfolio for a provider.
    pub fn get_portfolio(&self, provider_id: &str) -> Option<serde_json::Value> {
        // Check cache first
        if let Some(cached) = self.cache.get(provider_id) {
            let elapsed = Utc::now()
                .signed_duration_since(cached.cached_at)
                .num_seconds();
            if elapsed < self.config.cache_ttl_secs {
                return Some(cached.data.clone());
            }
        }

        let stats = self.providers.get(provider_id).map(|s| s.value().clone())?;
        let cards = self
            .model_cards
            .get(provider_id)
            .map(|c| c.value().clone())
            .unwrap_or_default();
        let reviews = self
            .review_summaries
            .get(provider_id)
            .map(|r| r.value().clone())
            .unwrap_or_default();

        let portfolio = serde_json::json!({
            "provider_id": provider_id,
            "stats": stats,
            "models": cards,
            "reviews": reviews,
        });

        // Cache it
        self.cache.insert(
            provider_id.to_string(),
            CachedPortfolio {
                data: portfolio.clone(),
                cached_at: Utc::now(),
            },
        );

        Some(portfolio)
    }

    /// Get a specific section of a provider's portfolio.
    pub fn get_section(
        &self,
        provider_id: &str,
        section: &str,
    ) -> Result<serde_json::Value, String> {
        let section_enum =
            PortfolioSection::from_str(section).ok_or("invalid section")?;

        match section_enum {
            PortfolioSection::Overview => {
                self.get_portfolio(provider_id)
                    .ok_or("provider not found".to_string())
            }
            PortfolioSection::Models => {
                let cards = self
                    .model_cards
                    .get(provider_id)
                    .map(|c| {
                        serde_json::to_value(c.value().clone()).unwrap_or_default()
                    })
                    .unwrap_or(serde_json::json!([]));
                Ok(cards)
            }
            PortfolioSection::Performance => {
                let stats = self
                    .providers
                    .get(provider_id)
                    .map(|s| {
                        serde_json::json!({
                            "total_inferences": s.total_inferences,
                            "avg_rating": s.avg_rating,
                            "uptime_pct": s.uptime_pct,
                        })
                    })
                    .unwrap_or(serde_json::json!(null));
                Ok(stats)
            }
            PortfolioSection::Reviews => {
                let reviews = self
                    .review_summaries
                    .get(provider_id)
                    .map(|r| {
                        serde_json::to_value(r.value().clone()).unwrap_or_default()
                    })
                    .unwrap_or(serde_json::json!({"avg_rating": 0, "count": 0, "distribution": {}, "recent_reviews": []}));
                Ok(reviews)
            }
            PortfolioSection::Pricing => {
                let cards = self
                    .model_cards
                    .get(provider_id)
                    .map(|c| {
                        let pricing: Vec<_> = c
                            .value()
                            .iter()
                            .map(|m| {
                                serde_json::json!({
                                    "model_id": m.model_id,
                                    "name": m.name,
                                    "pricing": m.pricing,
                                })
                            })
                            .collect();
                        serde_json::json!(pricing)
                    })
                    .unwrap_or(serde_json::json!([]));
                Ok(cards)
            }
            PortfolioSection::Infrastructure => {
                let stats = self
                    .providers
                    .get(provider_id)
                    .map(|s| {
                        serde_json::json!({
                            "active_since": s.active_since,
                            "uptime_pct": s.uptime_pct,
                            "total_inferences": s.total_inferences,
                        })
                    })
                    .unwrap_or(serde_json::json!(null));
                Ok(stats)
            }
        }
    }

    /// Update or set provider stats.
    pub fn update_stats(&self, provider_id: &str, stats: ProviderStats) {
        self.providers.insert(provider_id.to_string(), stats);
        // Invalidate cache
        self.cache.remove(provider_id);
    }

    /// Add a model card to a provider's portfolio.
    pub fn add_model_card(&self, provider_id: &str, card: ModelCard) {
        let mut cards = self
            .model_cards
            .entry(provider_id.to_string())
            .or_insert_with(Vec::new);
        // Remove existing card with same model_id
        cards.retain(|c| c.model_id != card.model_id);
        cards.push(card);
        // Invalidate cache
        self.cache.remove(provider_id);
    }

    /// Update the review summary for a provider.
    pub fn update_review_summary(&self, provider_id: &str, summary: ReviewSummary) {
        self.review_summaries
            .insert(provider_id.to_string(), summary);
        self.cache.remove(provider_id);
    }

    /// Search providers by name/stats criteria.
    pub fn search_providers(
        &self,
        query: &str,
        limit: usize,
    ) -> Vec<serde_json::Value> {
        let query_lower = query.to_lowercase();
        let limit = limit.min(100).max(1);

        self.providers
            .iter()
            .filter(|entry| entry.key().to_lowercase().contains(&query_lower))
            .take(limit)
            .map(|entry| {
                let pid = entry.key().clone();
                let stats = entry.value().clone();
                serde_json::json!({
                    "provider_id": pid,
                    "stats": stats,
                })
            })
            .collect()
    }

    /// Get featured providers.
    pub fn get_featured(&self) -> Vec<serde_json::Value> {
        let max = self.config.max_featured_providers;
        let mut featured: Vec<(String, DateTime<Utc>)> = self
            .featured_list
            .iter()
            .map(|e| (e.key().clone(), *e.value()))
            .collect();

        // Sort by most recently featured
        featured.sort_by(|a, b| b.1.cmp(&a.1));

        featured
            .into_iter()
            .take(max)
            .filter_map(|(pid, _)| {
                self.get_portfolio(&pid).map(|portfolio| {
                    serde_json::json!({
                        "provider_id": pid,
                        "portfolio": portfolio,
                    })
                })
            })
            .collect()
    }

    /// Add a provider to the featured list.
    pub fn set_featured(&self, provider_id: &str) {
        self.featured_list
            .insert(provider_id.to_string(), Utc::now());
    }

    /// Remove a provider from the featured list.
    pub fn remove_featured(&self, provider_id: &str) -> bool {
        self.featured_list.remove(provider_id).is_some()
    }

    /// Get model cards for a provider.
    pub fn get_models(&self, provider_id: &str) -> Vec<ModelCard> {
        self.model_cards
            .get(provider_id)
            .map(|c| c.value().clone())
            .unwrap_or_default()
    }

    /// Invalidate cache for a provider.
    pub fn invalidate_cache(&self, provider_id: &str) -> bool {
        self.cache.remove(provider_id).is_some()
    }

    /// Get total number of registered providers.
    pub fn provider_count(&self) -> usize {
        self.providers.len()
    }
}

impl Default for ProviderPortfolio {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Request / Response DTOs
// ===========================================================================

#[derive(Deserialize)]
pub struct UpdateStatsRequest {
    pub total_models: Option<u64>,
    pub total_inferences: Option<u64>,
    pub avg_rating: Option<f64>,
    pub uptime_pct: Option<f64>,
    pub total_earnings: Option<f64>,
    pub active_since: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
pub struct AddModelCardRequest {
    pub model_id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
    pub rating: Option<f64>,
    pub inference_count: Option<u64>,
    pub status: Option<String>,
}

#[derive(Deserialize)]
pub struct SearchProvidersQuery {
    pub q: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
pub struct UpdateReviewSummaryRequest {
    pub avg_rating: Option<f64>,
    pub count: Option<u64>,
    pub distribution: Option<HashMap<u32, u32>>,
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_portfolio() -> ProviderPortfolio {
        ProviderPortfolio::new()
    }

    fn make_stats() -> ProviderStats {
        ProviderStats {
            total_models: 5,
            total_inferences: 10000,
            avg_rating: 4.5,
            uptime_pct: 99.9,
            total_earnings: 5000.0,
            active_since: Utc::now() - chrono::Duration::days(90),
        }
    }

    fn make_card(model_id: &str, name: &str) -> ModelCard {
        ModelCard {
            model_id: model_id.to_string(),
            name: name.to_string(),
            description: format!("{} description", name),
            category: "llm".to_string(),
            pricing: ModelCardPricing {
                input_price: 0.5,
                output_price: 1.5,
                unit: "per_1m_tokens".to_string(),
            },
            rating: 4.5,
            inference_count: 1000,
            status: ModelStatus::Active,
        }
    }

    fn make_review_summary() -> ReviewSummary {
        let mut distribution = HashMap::new();
        distribution.insert(5, 10);
        distribution.insert(4, 5);
        distribution.insert(3, 2);
        ReviewSummary {
            avg_rating: 4.3,
            count: 17,
            distribution,
            recent_reviews: vec![RecentReview {
                review_id: "r1".to_string(),
                user_id: "u1".to_string(),
                rating: 5,
                text: "Great model!".to_string(),
                created_at: Utc::now(),
            }],
        }
    }

    #[test]
    fn test_update_and_get_stats() {
        let portfolio = make_portfolio();
        portfolio.update_stats("provider1", make_stats());
        let p = portfolio.get_portfolio("provider1").unwrap();
        assert_eq!(p["stats"]["total_models"], 5);
        assert_eq!(p["stats"]["avg_rating"], 4.5);
    }

    #[test]
    fn test_add_model_cards() {
        let portfolio = make_portfolio();
        portfolio.update_stats("provider1", make_stats());
        portfolio.add_model_card("provider1", make_card("m1", "Model A"));
        portfolio.add_model_card("provider1", make_card("m2", "Model B"));

        let p = portfolio.get_portfolio("provider1").unwrap();
        let models = p["models"].as_array().unwrap();
        assert_eq!(models.len(), 2);
    }

    #[test]
    fn test_get_section_models() {
        let portfolio = make_portfolio();
        portfolio.update_stats("provider1", make_stats());
        portfolio.add_model_card("provider1", make_card("m1", "Model A"));

        let section = portfolio.get_section("provider1", "models").unwrap();
        let models = section.as_array().unwrap();
        assert_eq!(models.len(), 1);
    }

    #[test]
    fn test_get_section_performance() {
        let portfolio = make_portfolio();
        portfolio.update_stats("provider1", make_stats());

        let section = portfolio.get_section("provider1", "performance").unwrap();
        assert_eq!(section["avg_rating"], 4.5);
        assert_eq!(section["uptime_pct"], 99.9);
    }

    #[test]
    fn test_get_section_reviews() {
        let portfolio = make_portfolio();
        portfolio.update_stats("provider1", make_stats());
        portfolio.update_review_summary("provider1", make_review_summary());

        let section = portfolio.get_section("provider1", "reviews").unwrap();
        assert_eq!(section["avg_rating"], 4.3);
        assert_eq!(section["count"], 17);
    }

    #[test]
    fn test_invalid_section() {
        let portfolio = make_portfolio();
        let result = portfolio.get_section("provider1", "nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_update_review_summary() {
        let portfolio = make_portfolio();
        portfolio.update_stats("provider1", make_stats());
        portfolio.update_review_summary("provider1", make_review_summary());

        let p = portfolio.get_portfolio("provider1").unwrap();
        assert_eq!(p["reviews"]["count"], 17);
        assert_eq!(p["reviews"]["avg_rating"], 4.3);
    }

    #[test]
    fn test_search_providers() {
        let portfolio = make_portfolio();
        portfolio.update_stats("acme-ai", make_stats());
        portfolio.update_stats("beta-models", make_stats());
        portfolio.update_stats("gamma-llm", make_stats());

        let results = portfolio.search_providers("acme", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["provider_id"], "acme-ai");
    }

    #[test]
    fn test_featured_providers() {
        let portfolio = make_portfolio();
        portfolio.update_stats("p1", make_stats());
        portfolio.update_stats("p2", make_stats());
        portfolio.set_featured("p1");
        portfolio.set_featured("p2");

        let featured = portfolio.get_featured();
        assert_eq!(featured.len(), 2);
    }

    #[test]
    fn test_remove_featured() {
        let portfolio = make_portfolio();
        portfolio.update_stats("p1", make_stats());
        portfolio.set_featured("p1");
        assert_eq!(portfolio.get_featured().len(), 1);
        portfolio.remove_featured("p1");
        assert_eq!(portfolio.get_featured().len(), 0);
    }

    #[test]
    fn test_cache_invalidation() {
        let portfolio = make_portfolio();
        portfolio.update_stats("p1", make_stats());
        // First call caches
        let _ = portfolio.get_portfolio("p1");
        // Update stats should invalidate
        let mut new_stats = make_stats();
        new_stats.total_models = 10;
        portfolio.update_stats("p1", new_stats);
        // Next get should reflect updated data
        let p = portfolio.get_portfolio("p1").unwrap();
        assert_eq!(p["stats"]["total_models"], 10);
    }

    #[test]
    fn test_model_card_replacement() {
        let portfolio = make_portfolio();
        portfolio.update_stats("p1", make_stats());

        // Add a card
        let mut card = make_card("m1", "Model A");
        card.rating = 3.0;
        portfolio.add_model_card("p1", card);

        // Replace with updated card
        let mut updated_card = make_card("m1", "Model A");
        updated_card.rating = 5.0;
        portfolio.add_model_card("p1", updated_card);

        let cards = portfolio.get_models("p1");
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].rating, 5.0);
    }
}
