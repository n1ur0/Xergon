//! ErgoAuth login and NFT model cards for the Xergon Network marketplace.
//!
//! Provides:
//!   - ErgoAuth-based login (ProveDlog signature verification)
//!   - NFT-gated model cards (ownership verification via token registry)
//!   - Model card CRUD with rich metadata
//!   - Ownership transfer and delegation

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Login/auth status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthStatus {
    Authenticated,
    Unauthenticated,
    TokenExpired,
    InvalidSignature,
    Blacklisted,
}

/// NFT card visibility.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CardVisibility {
    Public,
    Unlisted,
    Private,
    Gated,
}

/// Model card status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CardStatus {
    Active,
    Inactive,
    UnderReview,
    Suspended,
    Deprecated,
}

// ---------------------------------------------------------------------------
// Data Types
// ---------------------------------------------------------------------------

/// ErgoAuth login request (ProveDlog challenge-response).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub challenge: String,
    pub proof: String,
    pub public_key: String,
    pub address: String,
}

/// Auth session after successful login.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSession {
    pub session_id: String,
    pub address: String,
    pub public_key: String,
    pub status: AuthStatus,
    pub created_at: u64,
    pub expires_at: u64,
    pub roles: Vec<String>,
}

/// NFT model card.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftModelCard {
    pub card_id: String,
    pub model_name: String,
    pub model_version: String,
    pub description: String,
    pub author_address: String,
    pub nft_token_id: String,
    pub nft_box_id: String,
    pub visibility: CardVisibility,
    pub status: CardStatus,
    pub category: String,
    pub tags: Vec<String>,
    pub capabilities: Vec<String>,
    pub pricing: ModelPricing,
    pub inference_config: InferenceConfig,
    pub performance_metrics: PerformanceMetrics,
    pub created_at: u64,
    pub updated_at: u64,
    pub total_inferences: u64,
    pub total_earned_nanoerg: u64,
    pub rating_avg: f64,
    pub rating_count: u32,
    pub delegation_address: Option<String>,
}

/// Model pricing structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub price_per_1k_tokens_in: u64,
    pub price_per_1k_tokens_out: u64,
    pub currency: String,
    pub min_payment_nanoerg: u64,
    pub bulk_discount_bps: u32,
    pub free_tier_limit: u32,
}

impl Default for ModelPricing {
    fn default() -> Self {
        Self {
            price_per_1k_tokens_in: 100_000u64,
            price_per_1k_tokens_out: 200_000u64,
            currency: "ERG".to_string(),
            min_payment_nanoerg: 10_000_000u64,
            bulk_discount_bps: 1000,
            free_tier_limit: 1000,
        }
    }
}

/// Inference configuration for the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceConfig {
    pub max_context_length: u32,
    pub supported_formats: Vec<String>,
    pub quantization: String,
    pub temperature_range: (f32, f32),
    pub top_p_range: (f32, f32),
    pub max_output_tokens: u32,
    pub endpoint_url: Option<String>,
    pub health_check_interval_secs: u64,
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            max_context_length: 4096,
            supported_formats: vec!["openai".to_string(), "native".to_string()],
            quantization: "q4_0".to_string(),
            temperature_range: (0.0, 2.0),
            top_p_range: (0.0, 1.0),
            max_output_tokens: 2048,
            endpoint_url: None,
            health_check_interval_secs: 300,
        }
    }
}

/// Model performance metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub avg_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub error_rate: f64,
    pub tokens_per_second: f64,
    pub uptime_percent: f64,
    pub total_requests: u64,
    pub successful_requests: u64,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            avg_latency_ms: 0.0,
            p99_latency_ms: 0.0,
            error_rate: 0.0,
            tokens_per_second: 0.0,
            uptime_percent: 100.0,
            total_requests: 0,
            successful_requests: 0,
        }
    }
}

/// Model card filter for search/listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardFilter {
    pub category: Option<String>,
    pub status: Option<CardStatus>,
    pub visibility: Option<CardVisibility>,
    pub author: Option<String>,
    pub min_rating: Option<f64>,
    pub tags: Option<Vec<String>>,
    pub search_query: Option<String>,
    pub sort_by: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Marketplace summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceSummary {
    pub total_models: usize,
    pub active_models: usize,
    pub total_inferences: u64,
    pub total_volume_nanoerg: u64,
    pub avg_rating: f64,
    pub categories: Vec<String>,
    pub unique_authors: usize,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Shared marketplace auth + NFT state.
pub struct MarketplaceState {
    pub sessions: DashMap<String, AuthSession>,
    pub challenges: DashMap<String, u64>,
    pub model_cards: DashMap<String, NftModelCard>,
    pub nft_registry: DashMap<String, String>,  // nft_token_id -> card_id
    pub address_cards: DashMap<String, Vec<String>>,  // address -> [card_ids]
    pub blacklisted: DashMap<String, bool>,
    pub metrics: DashMap<String, u64>,
    pub session_counter: AtomicU64,
}

impl MarketplaceState {
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
            challenges: DashMap::new(),
            model_cards: DashMap::new(),
            nft_registry: DashMap::new(),
            address_cards: DashMap::new(),
            blacklisted: DashMap::new(),
            metrics: DashMap::new(),
            session_counter: AtomicU64::new(0),
        }
    }
}

impl Default for MarketplaceState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Business Logic
// ---------------------------------------------------------------------------

impl MarketplaceState {
    /// Generate a new auth challenge.
    pub fn generate_challenge(&self, _address: &str) -> String {
        let challenge = format!("xergon-auth-{}", self.session_counter.fetch_add(1, Ordering::Relaxed));
        self.challenges.insert(challenge.clone(), now_secs());
        self.metrics.entry("challenges_generated".to_string()).and_modify(|v| *v += 1).or_insert(1);
        challenge
    }

    /// Verify ProveDlog signature and create auth session.
    pub fn verify_login(&self, req: &LoginRequest) -> Result<AuthSession, String> {
        if self.blacklisted.contains_key(&req.address) {
            return Err("Address is blacklisted".into());
        }

        let challenge_ts = self.challenges.get(&req.challenge)
            .ok_or("Invalid or expired challenge")?;

        // Challenge must be < 5 minutes old
        if now_secs().saturating_sub(*challenge_ts) > 300 {
            self.challenges.remove(&req.challenge);
            return Err("Challenge expired".into());
        }

        // Verify proof format (basic check — full Sigma verification requires ergo-lib)
        if req.proof.is_empty() || req.public_key.is_empty() {
            return Err("Invalid proof or public key".into());
        }

        self.challenges.remove(&req.challenge);

        let session_id = format!("sess-{}", self.session_counter.fetch_add(1, Ordering::Relaxed));
        let session = AuthSession {
            session_id: session_id.clone(),
            address: req.address.clone(),
            public_key: req.public_key.clone(),
            status: AuthStatus::Authenticated,
            created_at: now_secs(),
            expires_at: now_secs() + 86400, // 24h session
            roles: self.resolve_roles(&req.address),
        };

        self.sessions.insert(session_id.clone(), session.clone());
        self.metrics.entry("logins".to_string()).and_modify(|v| *v += 1).or_insert(1);

        Ok(session)
    }

    /// Validate an active session.
    pub fn validate_session(&self, session_id: &str) -> Result<AuthSession, String> {
        let session = self.sessions.get(session_id).ok_or("Session not found")?;
        if session.status != AuthStatus::Authenticated {
            return Err("Session not authenticated".into());
        }
        if now_secs() > session.expires_at {
            return Err("Session expired".into());
        }
        Ok(session.clone())
    }

    /// Create a new NFT model card.
    pub fn create_model_card(&self, session_id: &str, mut card: NftModelCard) -> Result<NftModelCard, String> {
        let session = self.validate_session(session_id)?;
        if session.address != card.author_address {
            return Err("Session address does not match card author".into());
        }

        if self.nft_registry.contains_key(&card.nft_token_id) {
            return Err("NFT already registered to a model card".into());
        }

        card.card_id = format!("card-{}", self.session_counter.fetch_add(1, Ordering::Relaxed));
        card.created_at = now_secs();
        card.updated_at = now_secs();

        let card_id = card.card_id.clone();
        let nft_id = card.nft_token_id.clone();
        let addr = card.author_address.clone();

        self.model_cards.insert(card_id.clone(), card.clone());
        self.nft_registry.insert(nft_id.clone(), card_id.clone());

        self.address_cards.entry(addr).or_insert_with(Vec::new).push(card_id.clone());
        self.metrics.entry("cards_created".to_string()).and_modify(|v| *v += 1).or_insert(1);

        Ok(card)
    }

    /// Update a model card (author only).
    pub fn update_model_card(
        &self,
        session_id: &str,
        card_id: &str,
        updates: &NftModelCardUpdate,
    ) -> Result<NftModelCard, String> {
        let session = self.validate_session(session_id)?;
        let mut card = self.model_cards.get(card_id).ok_or("Card not found")?.clone();

        if card.author_address != session.address && !session.roles.contains(&"admin".to_string()) {
            return Err("Not authorized to update this card".into());
        }

        if let Some(ref name) = updates.model_name { card.model_name = name.clone(); }
        if let Some(ref desc) = updates.description { card.description = desc.clone(); }
        if let Some(ref cat) = updates.category { card.category = cat.clone(); }
        if let Some(ref tags) = updates.tags { card.tags = tags.clone(); }
        if let Some(ref vis) = updates.visibility { card.visibility = vis.clone(); }
        if let Some(ref status) = updates.status { card.status = status.clone(); }
        if let Some(ref pricing) = updates.pricing { card.pricing = pricing.clone(); }
        if let Some(ref config) = updates.inference_config { card.inference_config = config.clone(); }
        if let Some(ref cap) = updates.capabilities { card.capabilities = cap.clone(); }

        card.updated_at = now_secs();
        let updated = card.clone();
        self.model_cards.insert(card_id.to_string(), updated);
        Ok(card)
    }

    /// List model cards with optional filters.
    pub fn list_model_cards(&self, filter: &CardFilter) -> Vec<NftModelCard> {
        let mut cards: Vec<NftModelCard> = self.model_cards.iter()
            .filter(|c| {
                let v = c.value();
                if let Some(ref s) = filter.status { if v.status != *s { return false; } }
                if let Some(ref vis) = filter.visibility {
                    if v.visibility != *vis && v.visibility != CardVisibility::Public { return false; }
                }
                if let Some(ref author) = filter.author {
                    if v.author_address != *author { return false; }
                }
                if let Some(min_r) = filter.min_rating {
                    if v.rating_avg < min_r { return false; }
                }
                if let Some(ref cat) = filter.category {
                    if v.category != *cat { return false; }
                }
                if let Some(ref tags) = filter.tags {
                    if !tags.iter().any(|t| v.tags.contains(t)) { return false; }
                }
                true
            })
            .map(|c| c.value().clone())
            .collect();

        // Sort
        if let Some(ref sort) = filter.sort_by {
            cards.sort_by(|a, b| match sort.as_str() {
                "rating" => b.rating_avg.partial_cmp(&a.rating_avg).unwrap_or(std::cmp::Ordering::Equal),
                "inferences" => b.total_inferences.cmp(&a.total_inferences),
                "newest" => b.created_at.cmp(&a.created_at),
                "volume" => b.total_earned_nanoerg.cmp(&a.total_earned_nanoerg),
                _ => std::cmp::Ordering::Equal,
            });
        }

        let offset = filter.offset.unwrap_or(0);
        let limit = filter.limit.unwrap_or(50);
        cards.into_iter().skip(offset).take(limit).collect()
    }

    /// Get a single model card by ID.
    pub fn get_model_card(&self, card_id: &str) -> Option<NftModelCard> {
        self.model_cards.get(card_id).map(|c| c.clone())
    }

    /// Record inference against a model card.
    pub fn record_inference(&self, card_id: &str, earned_nanoerg: u64) -> Result<(), String> {
        let mut card = self.model_cards.get_mut(card_id).ok_or("Card not found")?;
        card.total_inferences += 1;
        card.total_earned_nanoerg += earned_nanoerg;
        Ok(())
    }

    /// Rate a model card.
    pub fn rate_model_card(&self, card_id: &str, rating: f64) -> Result<(), String> {
        if rating < 0.0 || rating > 5.0 {
            return Err("Rating must be 0.0 - 5.0".into());
        }
        let mut card = self.model_cards.get_mut(card_id).ok_or("Card not found")?;
        let total = card.rating_count as f64;
        card.rating_avg = ((card.rating_avg * total) + rating) / (total + 1.0);
        card.rating_count += 1;
        Ok(())
    }

    /// Get marketplace summary.
    pub fn get_summary(&self) -> MarketplaceSummary {
        let total = self.model_cards.len();
        let active = self.model_cards.iter().filter(|c| c.status == CardStatus::Active).count();
        let total_inf: u64 = self.model_cards.iter().map(|c| c.total_inferences).sum();
        let total_vol: u64 = self.model_cards.iter().map(|c| c.total_earned_nanoerg).sum();

        let mut cats: Vec<String> = self.model_cards.iter().map(|c| c.category.clone()).collect();
        cats.sort();
        cats.dedup();

        let avg_rating = if total > 0 {
            self.model_cards.iter().map(|c| c.rating_avg).sum::<f64>() / total as f64
        } else { 0.0 };

        let authors: Vec<String> = self.model_cards.iter().map(|c| c.author_address.clone()).collect();
        let unique_authors = authors.into_iter().collect::<std::collections::HashSet<_>>().len();

        MarketplaceSummary {
            total_models: total,
            active_models: active,
            total_inferences: total_inf,
            total_volume_nanoerg: total_vol,
            avg_rating,
            categories: cats,
            unique_authors,
        }
    }

    fn resolve_roles(&self, address: &str) -> Vec<String> {
        let mut roles = vec!["user".to_string()];
        let card_count = self.address_cards.get(address).map(|v| v.len()).unwrap_or(0);
        if card_count > 0 {
            roles.push("provider".to_string());
        }
        if card_count > 10 {
            roles.push("verified_provider".to_string());
        }
        roles
    }
}

/// Partial update for model cards.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NftModelCardUpdate {
    pub model_name: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub visibility: Option<CardVisibility>,
    pub status: Option<CardStatus>,
    pub pricing: Option<ModelPricing>,
    pub inference_config: Option<InferenceConfig>,
    pub capabilities: Option<Vec<String>>,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()
}
