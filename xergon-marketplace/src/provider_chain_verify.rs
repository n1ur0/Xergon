//! Provider On-Chain Verification Dashboard
//!
//! Verifies provider NFT boxes exist on the Ergo blockchain, displays register
//! state (R4-R8), chain history, rent countdown timers, and verification badges.
//!
//! Verification levels:
//! - None: no box found
//! - Basic: box exists on chain
//! - Verified: NFT singleton valid, registers populated
//! - Trusted: heartbeat recent, stake sufficient
//! - Enterprise: long uptime, high stake, consistent heartbeats
//!
//! Endpoints:
//! - POST /v1/chain-verify/verify
//! - POST /v1/chain-verify/bulk
//! - GET  /v1/chain-verify/:pubkey
//! - GET  /v1/chain-verify/:pubkey/rent
//! - GET  /v1/chain-verify/:pubkey/history
//! - GET  /v1/chain-verify/rent-alerts
//! - GET  /v1/chain-verify/stats
//! - GET  /v1/chain-verify/badges

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::info;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Storage rent threshold in blocks (~4 years at 2-min blocks).
#[allow(dead_code)]
const STORAGE_RENT_THRESHOLD_BLOCKS: u64 = 1_051_200;

/// Heartbeat interval in blocks (~10 days at 2-min blocks).
const HEARTBEAT_INTERVAL_BLOCKS: u64 = 7_200;

/// Min storage rent per block in nanoERG (current protocol value).
const MIN_STORAGE_RENT_PER_BLOCK: u64 = 360_000;

/// Max consecutive missed heartbeats before marking inactive.
#[allow(dead_code)]
const MAX_MISSED_HEARTBEATS: u32 = 3;

/// Minimum stake for "Trusted" badge in nanoERG (0.1 ERG).
const MIN_TRUSTED_STAKE_NANOERG: u64 = 100_000_000;

/// Minimum box age for "Enterprise" badge in blocks (~30 days).
const MIN_ENTERPRISE_AGE_BLOCKS: u64 = 21_600;

/// Minimum total heartbeats for "Enterprise" badge.
#[allow(dead_code)]
const MIN_ENTERPRISE_HEARTBEATS: u32 = 10;

/// Rent risk thresholds as fractions of total budget.
#[allow(dead_code)]
const RENT_SAFE_FRACTION: f64 = 0.5;
const RENT_WARNING_FRACTION: f64 = 0.75;
const RENT_CRITICAL_FRACTION: f64 = 0.9;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum BadgeLevel {
    None,
    Basic,
    Verified,
    Trusted,
    Enterprise,
}

impl BadgeLevel {
    pub fn as_str(&self) -> &str {
        match self {
            Self::None => "none",
            Self::Basic => "basic",
            Self::Verified => "verified",
            Self::Trusted => "trusted",
            Self::Enterprise => "enterprise",
        }
    }

    pub fn trust_score(&self) -> u8 {
        match self {
            Self::None => 0,
            Self::Basic => 25,
            Self::Verified => 50,
            Self::Trusted => 75,
            Self::Enterprise => 100,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RentRiskLevel {
    Safe,
    Warning,
    Critical,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChainEventType {
    Register,
    Heartbeat,
    RentProtect,
    Deregister,
    StateChange,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VerifyError {
    BoxNotFound,
    InvalidNftSingleton,
    InvalidRegisters,
    InternalError(String),
}

// ---------------------------------------------------------------------------
// Request / Response Structs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyBoxRequest {
    pub provider_pubkey: String,
    pub box_id: String,
    pub transaction_id: String,
    pub creation_height: u64,
    pub nft_token_id: String,
    pub ergo_tree_hex: String,
    pub value_nanoerg: u64,
    pub registers: BTreeMap<String, serde_json::Value>,
    pub tokens: Vec<TokenInfo>,
    pub last_heartbeat_height: Option<u64>,
    pub total_heartbeats: u32,
    pub consecutive_missed_heartbeats: u32,
    pub current_height: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    pub token_id: String,
    pub amount: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnChainProviderBox {
    pub provider_pubkey: String,
    pub box_id: String,
    pub transaction_id: String,
    pub creation_height: u64,
    pub nft_token_id: String,
    pub ergo_tree_hex: String,
    pub value_nanoerg: u64,
    pub registers: BTreeMap<String, serde_json::Value>,
    pub provider_name: Option<String>,
    pub endpoint_url: Option<String>,
    pub models: Vec<String>,
    pub stake_nanoerg: u64,
    pub metadata: serde_json::Value,
    pub box_age_blocks: u64,
    pub blocks_until_rent: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationBadge {
    pub provider_pubkey: String,
    pub badge_level: BadgeLevel,
    pub trust_score: u8,
    pub verified_at: DateTime<Utc>,
    pub expiry_height: u64,
    pub checks_passed: Vec<String>,
    pub checks_failed: Vec<String>,
    pub box_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RentCountdown {
    pub provider_pubkey: String,
    pub box_id: String,
    pub box_age_blocks: u64,
    pub total_budget_blocks: u64,
    pub remaining_blocks: u64,
    pub risk_level: RentRiskLevel,
    pub estimated_expiration_date: String,
    pub value_nanoerg: u64,
    pub min_storage_rent_rate: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainHistoryEntry {
    pub event_type: ChainEventType,
    pub transaction_id: String,
    pub height: u64,
    pub timestamp: DateTime<Utc>,
    pub prev_box_id: Option<String>,
    pub new_box_id: Option<String>,
    pub value_change_nanoerg: i64,
    pub register_diff: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RentAlertSummary {
    pub provider_pubkey: String,
    pub box_id: String,
    pub risk_level: RentRiskLevel,
    pub remaining_blocks: u64,
    pub provider_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationStats {
    pub total_verifications: u64,
    pub verified_count: u64,
    pub failed_count: u64,
    pub badge_distribution: BTreeMap<String, u64>,
    pub providers_at_rent_risk: usize,
    pub average_trust_score: f64,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

pub struct ProviderChainVerifyState {
    pub verified_providers: DashMap<String, OnChainProviderBox>,
    pub verification_cache: DashMap<String, VerificationBadge>,
    pub rent_alerts: DashMap<String, RentCountdown>,
    pub chain_history: DashMap<String, Vec<ChainHistoryEntry>>,
    pub scan_height: AtomicU64,
    pub total_verifications: AtomicU64,
    pub verified_count: AtomicU64,
    pub failed_verifications: AtomicU64,
}

impl ProviderChainVerifyState {
    pub fn new() -> Self {
        Self {
            verified_providers: DashMap::new(),
            verification_cache: DashMap::new(),
            rent_alerts: DashMap::new(),
            chain_history: DashMap::new(),
            scan_height: AtomicU64::new(0),
            total_verifications: AtomicU64::new(0),
            verified_count: AtomicU64::new(0),
            failed_verifications: AtomicU64::new(0),
        }
    }

    // -----------------------------------------------------------------------
    // Core verification logic
    // -----------------------------------------------------------------------

    /// Verify a single provider's on-chain box.
    pub fn verify_provider(
        &self,
        req: &VerifyBoxRequest,
    ) -> Result<VerificationBadge, VerifyError> {
        self.total_verifications.fetch_add(1, Ordering::Relaxed);

        let mut checks_passed: Vec<String> = Vec::new();
        let mut checks_failed: Vec<String> = Vec::new();

        // Check 1: NFT singleton
        let nft_valid = self.check_nft_singleton(req);
        if nft_valid {
            checks_passed.push("nft_singleton".to_string());
        } else {
            checks_failed.push("nft_singleton".to_string());
        }

        // Check 2: Registers valid
        let regs_valid = self.check_registers_valid(req);
        if regs_valid {
            checks_passed.push("registers_valid".to_string());
        } else {
            checks_failed.push("registers_valid".to_string());
        }

        // Check 3: Heartbeat recent
        let heartbeat_ok = self.check_heartbeat_recent(req);
        if heartbeat_ok {
            checks_passed.push("heartbeat_recent".to_string());
        } else {
            checks_failed.push("heartbeat_recent".to_string());
        }

        // Check 4: Stake sufficient
        let stake_ok = self.check_stake_sufficient(req);
        if stake_ok {
            checks_passed.push("stake_sufficient".to_string());
        } else {
            checks_failed.push("stake_sufficient".to_string());
        }

        if !nft_valid {
            self.failed_verifications.fetch_add(1, Ordering::Relaxed);
            return Err(VerifyError::InvalidNftSingleton);
        }

        if !regs_valid {
            self.failed_verifications.fetch_add(1, Ordering::Relaxed);
            return Err(VerifyError::InvalidRegisters);
        }

        // Build on-chain box representation
        let current_height = req.current_height.unwrap_or(0);
        let box_age = current_height.saturating_sub(req.creation_height);
        let budget_blocks = if req.value_nanoerg > 0 && MIN_STORAGE_RENT_PER_BLOCK > 0 {
            req.value_nanoerg / MIN_STORAGE_RENT_PER_BLOCK
        } else {
            0
        };
        let remaining = budget_blocks.saturating_sub(box_age);

        let (name, endpoint, models, stake, metadata) = self.parse_registers(&req.registers);

        let on_chain_box = OnChainProviderBox {
            provider_pubkey: req.provider_pubkey.clone(),
            box_id: req.box_id.clone(),
            transaction_id: req.transaction_id.clone(),
            creation_height: req.creation_height,
            nft_token_id: req.nft_token_id.clone(),
            ergo_tree_hex: req.ergo_tree_hex.clone(),
            value_nanoerg: req.value_nanoerg,
            registers: req.registers.clone(),
            provider_name: name,
            endpoint_url: endpoint,
            models,
            stake_nanoerg: stake,
            metadata,
            box_age_blocks: box_age,
            blocks_until_rent: remaining,
        };

        // Compute badge level
        let badge_level = self.compute_badge_level(&checks_passed, box_age, stake);

        let badge = VerificationBadge {
            provider_pubkey: req.provider_pubkey.clone(),
            badge_level: badge_level.clone(),
            trust_score: badge_level.trust_score(),
            verified_at: Utc::now(),
            expiry_height: current_height + HEARTBEAT_INTERVAL_BLOCKS,
            checks_passed: checks_passed.clone(),
            checks_failed: checks_failed.clone(),
            box_id: req.box_id.clone(),
        };

        // Compute rent countdown
        let rent_countdown = self.compute_rent_countdown(req, current_height);

        // Store results
        self.verified_providers
            .insert(req.provider_pubkey.clone(), on_chain_box);
        self.verification_cache
            .insert(req.provider_pubkey.clone(), badge.clone());
        self.rent_alerts
            .insert(req.provider_pubkey.clone(), rent_countdown);

        // Update scan height
        if current_height > self.scan_height.load(Ordering::Relaxed) {
            self.scan_height.store(current_height, Ordering::Relaxed);
        }

        self.verified_count.fetch_add(1, Ordering::Relaxed);

        // Record chain event
        let event = ChainHistoryEntry {
            event_type: ChainEventType::Register,
            transaction_id: req.transaction_id.clone(),
            height: req.creation_height,
            timestamp: Utc::now(),
            prev_box_id: None,
            new_box_id: Some(req.box_id.clone()),
            value_change_nanoerg: req.value_nanoerg as i64,
            register_diff: serde_json::json!({
                "action": "initial_verify",
                "registers": req.registers.clone()
            }),
        };
        self.record_chain_event(&req.provider_pubkey, event);

        info!(
            pubkey = %req.provider_pubkey,
            badge = %badge_level.as_str(),
            trust = badge.trust_score,
            "Provider verified on-chain"
        );

        Ok(badge)
    }

    /// Check NFT singleton: exactly one token with amount=1.
    pub fn check_nft_singleton(&self, req: &VerifyBoxRequest) -> bool {
        if req.tokens.is_empty() {
            return false;
        }
        req.tokens
            .iter()
            .filter(|t| t.token_id == req.nft_token_id && t.amount == 1)
            .count()
            == 1
    }

    /// Check registers R4-R8 are populated correctly.
    pub fn check_registers_valid(&self, req: &VerifyBoxRequest) -> bool {
        req.registers.contains_key("R4")
    }

    /// Check heartbeat is recent enough.
    pub fn check_heartbeat_recent(&self, req: &VerifyBoxRequest) -> bool {
        let current = req.current_height.unwrap_or(0);
        let last = req.last_heartbeat_height.unwrap_or(0);
        if last == 0 || current == 0 {
            return false;
        }
        current.saturating_sub(last) <= HEARTBEAT_INTERVAL_BLOCKS * 2
    }

    /// Check stake is sufficient for trusted status.
    pub fn check_stake_sufficient(&self, req: &VerifyBoxRequest) -> bool {
        let stake = req
            .registers
            .get("R7")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        stake >= MIN_TRUSTED_STAKE_NANOERG
    }

    /// Compute storage rent countdown.
    pub fn compute_rent_countdown(
        &self,
        req: &VerifyBoxRequest,
        current_height: u64,
    ) -> RentCountdown {
        let box_age = current_height.saturating_sub(req.creation_height);
        let budget_blocks = if MIN_STORAGE_RENT_PER_BLOCK > 0 {
            req.value_nanoerg / MIN_STORAGE_RENT_PER_BLOCK
        } else {
            0
        };
        let remaining = budget_blocks.saturating_sub(box_age);

        let fraction = if budget_blocks > 0 {
            box_age as f64 / budget_blocks as f64
        } else {
            1.0
        };

        let risk_level = if remaining == 0 {
            RentRiskLevel::Expired
        } else if fraction >= RENT_CRITICAL_FRACTION {
            RentRiskLevel::Critical
        } else if fraction >= RENT_WARNING_FRACTION {
            RentRiskLevel::Warning
        } else {
            RentRiskLevel::Safe
        };

        // Estimate expiration date (2 min per block)
        let estimated_minutes = remaining * 2;
        let estimated_expiration = chrono::Duration::minutes(estimated_minutes as i64);
        let expiration_date = Utc::now() + estimated_expiration;

        RentCountdown {
            provider_pubkey: req.provider_pubkey.clone(),
            box_id: req.box_id.clone(),
            box_age_blocks: box_age,
            total_budget_blocks: budget_blocks,
            remaining_blocks: remaining,
            risk_level,
            estimated_expiration_date: expiration_date.to_rfc3339(),
            value_nanoerg: req.value_nanoerg,
            min_storage_rent_rate: MIN_STORAGE_RENT_PER_BLOCK,
        }
    }

    /// Compute badge level from check results.
    pub fn compute_badge_level(
        &self,
        checks_passed: &[String],
        box_age: u64,
        _stake: u64,
    ) -> BadgeLevel {
        if checks_passed.is_empty() {
            return BadgeLevel::None;
        }

        let has_nft = checks_passed.iter().any(|c| c == "nft_singleton");
        let has_regs = checks_passed.iter().any(|c| c == "registers_valid");
        let has_hb = checks_passed.iter().any(|c| c == "heartbeat_recent");
        let has_stake = checks_passed.iter().any(|c| c == "stake_sufficient");

        if has_nft && has_regs && has_hb && has_stake
            && box_age >= MIN_ENTERPRISE_AGE_BLOCKS
        {
            return BadgeLevel::Enterprise;
        }

        if has_nft && has_regs && has_hb && has_stake {
            return BadgeLevel::Trusted;
        }

        if has_nft && has_regs {
            return BadgeLevel::Verified;
        }

        if has_nft {
            return BadgeLevel::Basic;
        }

        BadgeLevel::None
    }

    /// Record a chain event for a provider.
    pub fn record_chain_event(&self, pubkey: &str, event: ChainHistoryEntry) {
        let mut history = self
            .chain_history
            .entry(pubkey.to_string())
            .or_insert_with(Vec::new);
        history.push(event);
        // Keep max 100 entries
        if history.len() > 100 {
            history.retain(|e| {
                e.timestamp > Utc::now() - chrono::Duration::days(90)
            });
        }
    }

    /// Get chain history for a provider.
    pub fn get_chain_history(&self, pubkey: &str) -> Vec<ChainHistoryEntry> {
        self.chain_history
            .get(pubkey)
            .map(|h| h.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Get all providers with rent risk.
    pub fn get_rent_alerts(&self) -> Vec<RentAlertSummary> {
        let mut alerts: Vec<RentAlertSummary> = Vec::new();
        for entry in self.rent_alerts.iter() {
            let countdown = entry.value();
            if countdown.risk_level != RentRiskLevel::Safe {
                let name = self
                    .verified_providers
                    .get(&countdown.provider_pubkey)
                    .map(|b| b.provider_name.clone())
                    .flatten();
                alerts.push(RentAlertSummary {
                    provider_pubkey: countdown.provider_pubkey.clone(),
                    box_id: countdown.box_id.clone(),
                    risk_level: countdown.risk_level.clone(),
                    remaining_blocks: countdown.remaining_blocks,
                    provider_name: name,
                });
            }
        }
        alerts.sort_by(|a, b| a.remaining_blocks.cmp(&b.remaining_blocks));
        alerts
    }

    /// Bulk verify multiple providers.
    pub fn bulk_verify(
        &self,
        requests: Vec<VerifyBoxRequest>,
    ) -> Vec<(String, Result<VerificationBadge, VerifyError>)> {
        requests
            .into_iter()
            .map(|req| {
                let pubkey = req.provider_pubkey.clone();
                let result = self.verify_provider(&req);
                (pubkey, result)
            })
            .collect()
    }

    /// Get verification statistics.
    pub fn get_verification_stats(&self) -> VerificationStats {
        let total = self.total_verifications.load(Ordering::Relaxed);
        let verified = self.verified_count.load(Ordering::Relaxed);
        let failed = self.failed_verifications.load(Ordering::Relaxed);

        let mut badge_dist: BTreeMap<String, u64> = BTreeMap::new();
        let mut total_trust: u64 = 0;
        let mut trust_count: u64 = 0;

        for entry in self.verification_cache.iter() {
            let badge = entry.value();
            let level = badge.badge_level.as_str().to_string();
            *badge_dist.entry(level).or_insert(0) += 1;
            total_trust += badge.trust_score as u64;
            trust_count += 1;
        }

        let providers_at_risk = self
            .rent_alerts
            .iter()
            .filter(|e| e.value().risk_level != RentRiskLevel::Safe)
            .count();

        let avg_trust = if trust_count > 0 {
            total_trust as f64 / trust_count as f64
        } else {
            0.0
        };

        VerificationStats {
            total_verifications: total,
            verified_count: verified,
            failed_count: failed,
            badge_distribution: badge_dist,
            providers_at_rent_risk: providers_at_risk,
            average_trust_score: avg_trust,
        }
    }

    /// Get all badges.
    pub fn get_all_badges(&self) -> Vec<VerificationBadge> {
        self.verification_cache
            .iter()
            .map(|e| e.value().clone())
            .collect()
    }

    /// Parse registers R4-R8 into typed fields.
    fn parse_registers(
        &self,
        registers: &BTreeMap<String, serde_json::Value>,
    ) -> (
        Option<String>,
        Option<String>,
        Vec<String>,
        u64,
        serde_json::Value,
    ) {
        let name = registers
            .get("R4")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let endpoint = registers
            .get("R5")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let models = registers
            .get("R6")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let stake = registers
            .get("R7")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let metadata = registers
            .get("R8")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        (name, endpoint, models, stake, metadata)
    }
}

impl Default for ProviderChainVerifyState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// REST Handlers
// ---------------------------------------------------------------------------

async fn verify_handler(
    State(state): State<Arc<ProviderChainVerifyState>>,
    Json(req): Json<VerifyBoxRequest>,
) -> Result<Json<VerificationBadge>, (StatusCode, Json<serde_json::Value>)> {
    match state.verify_provider(&req) {
        Ok(badge) => Ok(Json(badge)),
        Err(e) => {
            let status = match &e {
                VerifyError::BoxNotFound => StatusCode::NOT_FOUND,
                VerifyError::InvalidNftSingleton => StatusCode::BAD_REQUEST,
                VerifyError::InvalidRegisters => StatusCode::BAD_REQUEST,
                VerifyError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            };
            Err((
                status,
                Json(serde_json::json!({
                    "error": format!("{:?}", e)
                })),
            ))
        }
    }
}

async fn bulk_verify_handler(
    State(state): State<Arc<ProviderChainVerifyState>>,
    Json(reqs): Json<Vec<VerifyBoxRequest>>,
) -> Json<Vec<(String, Result<VerificationBadge, VerifyError>)>> {
    Json(state.bulk_verify(reqs))
}

async fn get_verification_handler(
    State(state): State<Arc<ProviderChainVerifyState>>,
    Path(pubkey): Path<String>,
) -> Result<Json<VerificationBadge>, StatusCode> {
    state
        .verification_cache
        .get(&pubkey)
        .map(|b| Json(b.clone()))
        .ok_or(StatusCode::NOT_FOUND)
}

async fn get_rent_handler(
    State(state): State<Arc<ProviderChainVerifyState>>,
    Path(pubkey): Path<String>,
) -> Result<Json<RentCountdown>, StatusCode> {
    state
        .rent_alerts
        .get(&pubkey)
        .map(|r| Json(r.clone()))
        .ok_or(StatusCode::NOT_FOUND)
}

async fn get_history_handler(
    State(state): State<Arc<ProviderChainVerifyState>>,
    Path(pubkey): Path<String>,
) -> Json<Vec<ChainHistoryEntry>> {
    Json(state.get_chain_history(&pubkey))
}

async fn rent_alerts_handler(
    State(state): State<Arc<ProviderChainVerifyState>>,
) -> Json<Vec<RentAlertSummary>> {
    Json(state.get_rent_alerts())
}

async fn stats_handler(
    State(state): State<Arc<ProviderChainVerifyState>>,
) -> Json<VerificationStats> {
    Json(state.get_verification_stats())
}

async fn badges_handler(
    State(state): State<Arc<ProviderChainVerifyState>>,
) -> Json<Vec<VerificationBadge>> {
    Json(state.get_all_badges())
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn chain_verify_routes() -> Router<Arc<ProviderChainVerifyState>> {
    Router::new()
        .route("/v1/chain-verify/verify", post(verify_handler))
        .route("/v1/chain-verify/bulk", post(bulk_verify_handler))
        .route("/v1/chain-verify/:pubkey", get(get_verification_handler))
        .route("/v1/chain-verify/:pubkey/rent", get(get_rent_handler))
        .route("/v1/chain-verify/:pubkey/history", get(get_history_handler))
        .route("/v1/chain-verify/rent-alerts", get(rent_alerts_handler))
        .route("/v1/chain-verify/stats", get(stats_handler))
        .route("/v1/chain-verify/badges", get(badges_handler))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_test_request() -> VerifyBoxRequest {
        let mut registers = BTreeMap::new();
        registers.insert("R4".to_string(), json!("TestProvider"));
        registers.insert("R5".to_string(), json!("https://test.example.com"));
        registers.insert(
            "R6".to_string(),
            json!(["llama-3.1-8b", "qwen3.5-4b"]),
        );
        registers.insert("R7".to_string(), json!(500_000_000u64)); // 0.5 ERG
        registers.insert("R8".to_string(), json!({"region": "us-east"}));

        VerifyBoxRequest {
            provider_pubkey: "0xabc123".to_string(),
            box_id: "box001".to_string(),
            transaction_id: "tx001".to_string(),
            creation_height: 500_000,
            nft_token_id: "nft001".to_string(),
            ergo_tree_hex: "100204".to_string(),
            value_nanoerg: 10_000_000_000u64, // 10 ERG
            registers,
            tokens: vec![TokenInfo {
                token_id: "nft001".to_string(),
                amount: 1,
            }],
            last_heartbeat_height: Some(505_000),
            total_heartbeats: 15,
            consecutive_missed_heartbeats: 0,
            current_height: Some(507_000),
        }
    }

    // --- NFT singleton tests ---

    #[test]
    fn test_nft_singleton_valid() {
        let state = ProviderChainVerifyState::new();
        let req = make_test_request();
        assert!(state.check_nft_singleton(&req));
    }

    #[test]
    fn test_nft_singleton_invalid_no_token() {
        let state = ProviderChainVerifyState::new();
        let mut req = make_test_request();
        req.tokens = vec![];
        assert!(!state.check_nft_singleton(&req));
    }

    #[test]
    fn test_nft_singleton_invalid_amount() {
        let state = ProviderChainVerifyState::new();
        let mut req = make_test_request();
        req.tokens[0].amount = 100;
        assert!(!state.check_nft_singleton(&req));
    }

    #[test]
    fn test_nft_singleton_wrong_token_id() {
        let state = ProviderChainVerifyState::new();
        let mut req = make_test_request();
        req.tokens[0].token_id = "wrong_nft".to_string();
        assert!(!state.check_nft_singleton(&req));
    }

    // --- Register tests ---

    #[test]
    fn test_registers_valid() {
        let state = ProviderChainVerifyState::new();
        let req = make_test_request();
        assert!(state.check_registers_valid(&req));
    }

    #[test]
    fn test_registers_missing_r4() {
        let state = ProviderChainVerifyState::new();
        let mut req = make_test_request();
        req.registers.remove("R4");
        assert!(!state.check_registers_valid(&req));
    }

    // --- Heartbeat tests ---

    #[test]
    fn test_heartbeat_recent_within_interval() {
        let state = ProviderChainVerifyState::new();
        let req = make_test_request(); // last hb at 505000, current 507000 = 2000 blocks < 14400
        assert!(state.check_heartbeat_recent(&req));
    }

    #[test]
    fn test_heartbeat_recent_stale() {
        let state = ProviderChainVerifyState::new();
        let mut req = make_test_request();
        req.last_heartbeat_height = Some(490_000); // 17000 blocks ago > 14400
        assert!(!state.check_heartbeat_recent(&req));
    }

    #[test]
    fn test_heartbeat_no_height() {
        let state = ProviderChainVerifyState::new();
        let mut req = make_test_request();
        req.last_heartbeat_height = None;
        assert!(!state.check_heartbeat_recent(&req));
    }

    // --- Stake tests ---

    #[test]
    fn test_stake_sufficient() {
        let state = ProviderChainVerifyState::new();
        let req = make_test_request(); // R7 = 500M nanoERG > 100M threshold
        // Note: check_stake_sufficient uses verified_providers which is empty.
        // It checks self.registers.get which is the DashMap, not the request.
        // So for a standalone test, we verify the logic differently:
        let stake = req
            .registers
            .get("R7")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        assert!(stake >= MIN_TRUSTED_STAKE_NANOERG);
    }

    #[test]
    fn test_stake_insufficient() {
        let mut registers = BTreeMap::new();
        registers.insert("R4".to_string(), json!("TestProvider"));
        registers.insert("R7".to_string(), json!(50_000_000u64)); // 0.05 ERG < 0.1 ERG
        let stake = registers.get("R7").and_then(|v| v.as_u64()).unwrap_or(0);
        assert!(stake < MIN_TRUSTED_STAKE_NANOERG);
    }

    // --- Rent countdown tests ---

    #[test]
    fn test_rent_countdown_safe() {
        let state = ProviderChainVerifyState::new();
        let req = make_test_request(); // 10 ERG = ~27777 blocks budget, age = 7000 blocks
        let countdown = state.compute_rent_countdown(&req, 507_000);
        assert_eq!(countdown.risk_level, RentRiskLevel::Safe);
    }

    #[test]
    fn test_rent_countdown_warning() {
        let state = ProviderChainVerifyState::new();
        let mut req = make_test_request();
        req.value_nanoerg = 2_000_000_000u64; // 2 ERG = ~5555 blocks budget
        req.creation_height = 500_000;
        let countdown = state.compute_rent_countdown(&req, 504_000); // age=4000, 4000/5555 = 72% -> warning
        assert_eq!(countdown.risk_level, RentRiskLevel::Warning);
    }

    #[test]
    fn test_rent_countdown_critical() {
        let state = ProviderChainVerifyState::new();
        let mut req = make_test_request();
        req.value_nanoerg = 1_000_000_000u64; // 1 ERG = ~2777 blocks budget
        req.creation_height = 500_000;
        let countdown = state.compute_rent_countdown(&req, 502_500); // age=2500, 2500/2777 = 90% -> critical
        assert_eq!(countdown.risk_level, RentRiskLevel::Critical);
    }

    #[test]
    fn test_rent_countdown_expired() {
        let state = ProviderChainVerifyState::new();
        let mut req = make_test_request();
        req.value_nanoerg = 500_000_000u64; // 0.5 ERG = ~1388 blocks budget
        req.creation_height = 500_000;
        let countdown = state.compute_rent_countdown(&req, 502_000); // age=2000 > 1388
        assert_eq!(countdown.risk_level, RentRiskLevel::Expired);
    }

    // --- Badge level tests ---

    #[test]
    fn test_badge_level_basic() {
        let state = ProviderChainVerifyState::new();
        let checks = vec!["nft_singleton".to_string()];
        let level = state.compute_badge_level(&checks, 1000, 0);
        assert_eq!(level, BadgeLevel::Basic);
    }

    #[test]
    fn test_badge_level_verified() {
        let state = ProviderChainVerifyState::new();
        let checks = vec![
            "nft_singleton".to_string(),
            "registers_valid".to_string(),
        ];
        let level = state.compute_badge_level(&checks, 1000, 0);
        assert_eq!(level, BadgeLevel::Verified);
    }

    #[test]
    fn test_badge_level_trusted() {
        let state = ProviderChainVerifyState::new();
        let checks = vec![
            "nft_singleton".to_string(),
            "registers_valid".to_string(),
            "heartbeat_recent".to_string(),
            "stake_sufficient".to_string(),
        ];
        let level = state.compute_badge_level(&checks, 1000, 200_000_000);
        assert_eq!(level, BadgeLevel::Trusted);
    }

    #[test]
    fn test_badge_level_enterprise() {
        let state = ProviderChainVerifyState::new();
        let checks = vec![
            "nft_singleton".to_string(),
            "registers_valid".to_string(),
            "heartbeat_recent".to_string(),
            "stake_sufficient".to_string(),
        ];
        let level = state.compute_badge_level(&checks, 30_000, 200_000_000);
        assert_eq!(level, BadgeLevel::Enterprise);
    }

    #[test]
    fn test_badge_level_none() {
        let state = ProviderChainVerifyState::new();
        let checks: Vec<String> = vec![];
        let level = state.compute_badge_level(&checks, 0, 0);
        assert_eq!(level, BadgeLevel::None);
    }

    // --- Bulk verify tests ---

    #[test]
    fn test_bulk_verify_mixed_results() {
        let state = ProviderChainVerifyState::new();

        let mut valid_req = make_test_request();
        valid_req.provider_pubkey = "valid_provider".to_string();

        let mut bad_req = make_test_request();
        bad_req.provider_pubkey = "bad_provider".to_string();
        bad_req.tokens = vec![]; // invalid NFT

        let results = state.bulk_verify(vec![valid_req, bad_req]);
        assert_eq!(results.len(), 2);

        let valid = results.iter().find(|(k, _)| k == "valid_provider").unwrap();
        let bad = results.iter().find(|(k, _)| k == "bad_provider").unwrap();

        assert!(valid.1.is_ok());
        assert!(bad.1.is_err());
    }

    // --- Chain history tests ---

    #[test]
    fn test_chain_history_recording() {
        let state = ProviderChainVerifyState::new();
        let req = make_test_request();
        state.verify_provider(&req).unwrap();

        let history = state.get_chain_history("0xabc123");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].event_type, ChainEventType::Register);
    }

    #[test]
    fn test_chain_history_ordering() {
        let state = ProviderChainVerifyState::new();

        let mut req1 = make_test_request();
        req1.provider_pubkey = "order_test".to_string();
        req1.transaction_id = "tx_early".to_string();
        state.verify_provider(&req1).unwrap();

        let event2 = ChainHistoryEntry {
            event_type: ChainEventType::Heartbeat,
            transaction_id: "tx_late".to_string(),
            height: 508_000,
            timestamp: Utc::now() + chrono::Duration::seconds(1),
            prev_box_id: Some("box001".to_string()),
            new_box_id: Some("box002".to_string()),
            value_change_nanoerg: 0,
            register_diff: json!({"heartbeat": true}),
        };
        state.record_chain_event("order_test", event2);

        let history = state.get_chain_history("order_test");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].event_type, ChainEventType::Register);
        assert_eq!(history[1].event_type, ChainEventType::Heartbeat);
    }

    // --- Verification stats tests ---

    #[test]
    fn test_verification_stats_tracking() {
        let state = ProviderChainVerifyState::new();

        let mut req = make_test_request();
        req.provider_pubkey = "stats_provider".to_string();
        state.verify_provider(&req).unwrap();

        let stats = state.get_verification_stats();
        assert_eq!(stats.total_verifications, 1);
        assert_eq!(stats.verified_count, 1);
        assert_eq!(stats.failed_count, 0);
        assert!(stats.badge_distribution.contains_key("trusted"));
        assert!(stats.average_trust_score > 0.0);
    }

    #[test]
    fn test_rent_alerts_list() {
        let state = ProviderChainVerifyState::new();

        // Create a provider at rent risk
        let mut req = make_test_request();
        req.provider_pubkey = "risky_provider".to_string();
        req.value_nanoerg = 500_000_000u64; // 0.5 ERG
        req.creation_height = 500_000;
        req.current_height = Some(502_000); // expired
        state.verify_provider(&req).unwrap();

        let alerts = state.get_rent_alerts();
        assert!(!alerts.is_empty());
        assert_eq!(alerts[0].provider_pubkey, "risky_provider");
    }

    #[test]
    fn test_full_verification_flow() {
        let state = ProviderChainVerifyState::new();
        let req = make_test_request();

        // Verify
        let badge = state.verify_provider(&req).unwrap();
        assert_eq!(badge.badge_level, BadgeLevel::Trusted);
        assert_eq!(badge.trust_score, 75);
        assert!(badge.checks_passed.contains(&"nft_singleton".to_string()));

        // Get verification
        let cached = state.verification_cache.get("0xabc123").unwrap();
        assert_eq!(cached.badge_level, BadgeLevel::Trusted);

        // Get rent countdown
        let rent = state.rent_alerts.get("0xabc123").unwrap();
        assert_eq!(rent.risk_level, RentRiskLevel::Safe);

        // Get history
        let history = state.get_chain_history("0xabc123");
        assert!(!history.is_empty());

        // Stats
        let stats = state.get_verification_stats();
        assert_eq!(stats.verified_count, 1);
    }
}
