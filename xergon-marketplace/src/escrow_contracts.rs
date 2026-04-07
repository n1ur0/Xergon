use axum::{
    extract::{Path, Query, State},
    response::Json,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// EscrowStatus
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum EscrowStatus {
    Created,
    Funded,
    Active,
    Disputed,
    Released,
    Refunded,
    Expired,
}

impl EscrowStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Created => "Created",
            Self::Funded => "Funded",
            Self::Active => "Active",
            Self::Disputed => "Disputed",
            Self::Released => "Released",
            Self::Refunded => "Refunded",
            Self::Expired => "Expired",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "Created" => Some(Self::Created),
            "Funded" => Some(Self::Funded),
            "Active" => Some(Self::Active),
            "Disputed" => Some(Self::Disputed),
            "Released" => Some(Self::Released),
            "Refunded" => Some(Self::Refunded),
            "Expired" => Some(Self::Expired),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// EscrowConditionType
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub enum EscrowConditionType {
    DeliveryConfirmation,
    Milestone,
    TimeLock,
    DisputeResolution,
}

impl EscrowConditionType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::DeliveryConfirmation => "DeliveryConfirmation",
            Self::Milestone => "Milestone",
            Self::TimeLock => "TimeLock",
            Self::DisputeResolution => "DisputeResolution",
        }
    }
}

// ---------------------------------------------------------------------------
// EscrowCondition
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EscrowCondition {
    pub condition_id: String,
    pub condition_type: EscrowConditionType,
    pub parameter: String,
    pub met: bool,
    pub created_at: DateTime<Utc>,
}

impl EscrowCondition {
    pub fn new(condition_type: EscrowConditionType, parameter: &str) -> Self {
        Self {
            condition_id: uuid::Uuid::new_v4().to_string(),
            condition_type,
            parameter: parameter.to_string(),
            met: false,
            created_at: Utc::now(),
        }
    }

    pub fn mark_met(&mut self) {
        self.met = true;
    }
}

// ---------------------------------------------------------------------------
// EscrowContract
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EscrowContract {
    pub escrow_id: String,
    pub buyer_id: String,
    pub seller_id: String,
    pub amount_nanoerg: u64,
    pub fee_nanoerg: u64,
    pub status: EscrowStatus,
    pub box_id: String,
    pub contract_hash: [u8; 32],
    pub created_at: DateTime<Utc>,
    pub funded_at: Option<DateTime<Utc>>,
    pub release_height: Option<u64>,
    pub timeout_height: u64,
    pub conditions: Vec<EscrowCondition>,
}

impl EscrowContract {
    pub fn new(
        buyer_id: &str,
        seller_id: &str,
        amount_nanoerg: u64,
        fee_nanoerg: u64,
        timeout_height: u64,
    ) -> Self {
        // Contract hash is derived from key parameters
        let hash_input = format!("{}:{}:{}:{}", buyer_id, seller_id, amount_nanoerg, timeout_height);
        let contract_hash = Self::compute_hash(&hash_input);

        Self {
            escrow_id: uuid::Uuid::new_v4().to_string(),
            buyer_id: buyer_id.to_string(),
            seller_id: seller_id.to_string(),
            amount_nanoerg,
            fee_nanoerg,
            status: EscrowStatus::Created,
            box_id: String::new(),
            contract_hash,
            created_at: Utc::now(),
            funded_at: None,
            release_height: None,
            timeout_height,
            conditions: Vec::new(),
        }
    }

    fn compute_hash(input: &str) -> [u8; 32] {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        input.hash(&mut hasher);
        let h1 = hasher.finish();
        let mut hasher2 = DefaultHasher::new();
        h1.hash(&mut hasher2);
        let h2 = hasher2.finish();
        let b1 = h1.to_le_bytes();
        let b2 = h2.to_le_bytes();
        let mut out = [0u8; 32];
        out[0..8].copy_from_slice(&b1);
        out[8..16].copy_from_slice(&b2);
        out[16..24].copy_from_slice(&b1);
        out[24..32].copy_from_slice(&b2);
        out
    }

    /// Total amount (principal + fee) in nanoERG.
    pub fn total_amount(&self) -> u64 {
        self.amount_nanoerg.saturating_add(self.fee_nanoerg)
    }

    /// Amount to release to seller (principal minus fee).
    pub fn seller_amount(&self) -> u64 {
        self.amount_nanoerg.saturating_sub(self.fee_nanoerg)
    }
}

// ---------------------------------------------------------------------------
// EscrowRelease
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EscrowRelease {
    pub release_id: String,
    pub escrow_id: String,
    pub recipient_id: String,
    pub amount_nanoerg: u64,
    pub tx_id: String,
    pub released_at: DateTime<Utc>,
    pub reason: String,
}

impl EscrowRelease {
    pub fn new(
        escrow_id: &str,
        recipient_id: &str,
        amount_nanoerg: u64,
        reason: &str,
    ) -> Self {
        Self {
            release_id: uuid::Uuid::new_v4().to_string(),
            escrow_id: escrow_id.to_string(),
            recipient_id: recipient_id.to_string(),
            amount_nanoerg,
            tx_id: uuid::Uuid::new_v4().to_string(),
            released_at: Utc::now(),
            reason: reason.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// EscrowConfig
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EscrowConfig {
    pub min_deposit_nanoerg: u64,
    pub fee_rate_pct: f64,
    pub default_timeout_blocks: u64,
    pub max_escrow_amount_nanoerg: u64,
    pub auto_release: bool,
}

impl Default for EscrowConfig {
    fn default() -> Self {
        Self {
            min_deposit_nanoerg: 1_000_000,
            fee_rate_pct: 0.1,
            default_timeout_blocks: 720,
            max_escrow_amount_nanoerg: 100_000_000_000,
            auto_release: true,
        }
    }
}

// ---------------------------------------------------------------------------
// CreateEscrowRequest
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CreateEscrowRequest {
    pub buyer_id: String,
    pub seller_id: String,
    pub amount_nanoerg: u64,
    pub timeout_blocks: Option<u64>,
    pub conditions: Option<Vec<CreateConditionRequest>>,
}

// ---------------------------------------------------------------------------
// CreateConditionRequest
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CreateConditionRequest {
    pub condition_type: String,
    pub parameter: String,
}

// ---------------------------------------------------------------------------
// FundEscrowRequest
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FundEscrowRequest {
    pub box_id: String,
    pub current_height: Option<u64>,
}

// ---------------------------------------------------------------------------
// ReleaseFundsRequest
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReleaseFundsRequest {
    pub recipient_id: Option<String>,
    pub amount_nanoerg: Option<u64>,
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// EscrowManager
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct EscrowManager {
    escrows: DashMap<String, EscrowContract>,
    releases: DashMap<String, Vec<EscrowRelease>>,
    config: DashMap<String, EscrowConfig>,
}

impl EscrowManager {
    pub fn new() -> Self {
        let config = DashMap::new();
        config.insert("default".to_string(), EscrowConfig::default());
        Self {
            escrows: DashMap::new(),
            releases: DashMap::new(),
            config,
        }
    }

    pub fn default() -> Self {
        Self::new()
    }

    pub fn get_config(&self) -> EscrowConfig {
        self.config
            .get("default")
            .map(|c| c.clone())
            .unwrap_or_default()
    }

    pub fn update_config(&self, new_config: EscrowConfig) {
        self.config.insert("default".to_string(), new_config);
    }

    /// Calculate fee for a given amount.
    pub fn calculate_fee(&self, amount_nanoerg: u64) -> u64 {
        let config = self.get_config();
        let fee = (amount_nanoerg as f64 * config.fee_rate_pct / 100.0) as u64;
        fee.max(1) // minimum 1 nanoERG fee
    }

    /// Create a new escrow contract.
    pub fn create_escrow(&self, req: &CreateEscrowRequest) -> Result<EscrowContract, String> {
        let config = self.get_config();

        if req.amount_nanoerg < config.min_deposit_nanoerg {
            return Err(format!(
                "Amount {} below minimum {}",
                req.amount_nanoerg, config.min_deposit_nanoerg
            ));
        }

        if req.amount_nanoerg > config.max_escrow_amount_nanoerg {
            return Err(format!(
                "Amount {} exceeds maximum {}",
                req.amount_nanoerg, config.max_escrow_amount_nanoerg
            ));
        }

        let fee_nanoerg = self.calculate_fee(req.amount_nanoerg);
        let timeout_blocks = req.timeout_blocks.unwrap_or(config.default_timeout_blocks);

        let mut contract = EscrowContract::new(
            &req.buyer_id,
            &req.seller_id,
            req.amount_nanoerg,
            fee_nanoerg,
            timeout_blocks,
        );

        // Add conditions if provided
        if let Some(conditions) = &req.conditions {
            for cond in conditions {
                let cond_type = match cond.condition_type.as_str() {
                    "DeliveryConfirmation" => EscrowConditionType::DeliveryConfirmation,
                    "Milestone" => EscrowConditionType::Milestone,
                    "TimeLock" => EscrowConditionType::TimeLock,
                    "DisputeResolution" => EscrowConditionType::DisputeResolution,
                    _ => EscrowConditionType::DeliveryConfirmation,
                };
                contract.conditions.push(EscrowCondition::new(cond_type, &cond.parameter));
            }
        }

        let id = contract.escrow_id.clone();
        self.escrows.insert(id.clone(), contract);
        self.releases.insert(id.clone(), Vec::new());

        self.escrows
            .get(&id)
            .map(|e| e.clone())
            .ok_or_else(|| "Failed to create escrow".to_string())
    }

    /// Fund an escrow contract (link a UTXO box).
    pub fn fund_escrow(&self, escrow_id: &str, req: &FundEscrowRequest) -> Result<EscrowContract, String> {
        if let Some(mut escrow) = self.escrows.get_mut(escrow_id) {
            if escrow.status != EscrowStatus::Created {
                return Err(format!(
                    "Cannot fund escrow in {} status",
                    escrow.status.as_str()
                ));
            }

            escrow.box_id = req.box_id.clone();
            escrow.status = EscrowStatus::Funded;
            escrow.funded_at = Some(Utc::now());

            let current_height = req.current_height.unwrap_or(0);
            escrow.release_height = Some(current_height + escrow.timeout_height);

            // If no conditions, auto-activate
            if escrow.conditions.is_empty() {
                escrow.status = EscrowStatus::Active;
            }

            Ok(escrow.clone())
        } else {
            Err("Escrow not found".to_string())
        }
    }

    /// Release funds from an escrow.
    pub fn release_funds(&self, escrow_id: &str, req: &ReleaseFundsRequest) -> Result<EscrowRelease, String> {
        if let Some(mut escrow) = self.escrows.get_mut(escrow_id) {
            if escrow.status != EscrowStatus::Active && escrow.status != EscrowStatus::Funded {
                return Err(format!(
                    "Cannot release funds in {} status",
                    escrow.status.as_str()
                ));
            }

            let recipient_id = req
                .recipient_id
                .as_deref()
                .unwrap_or(&escrow.seller_id);
            let amount = req
                .amount_nanoerg
                .unwrap_or(escrow.seller_amount());
            let reason = req
                .reason
                .as_deref()
                .unwrap_or("Funds released");

            let release = EscrowRelease::new(escrow_id, recipient_id, amount, reason);
            let release_clone = release.clone();

            escrow.status = EscrowStatus::Released;

            if let Some(mut releases) = self.releases.get_mut(escrow_id) {
                releases.push(release_clone);
            }

            Ok(release)
        } else {
            Err("Escrow not found".to_string())
        }
    }

    /// Refund an escrow (return funds to buyer).
    pub fn refund(&self, escrow_id: &str, reason: Option<&str>) -> Result<EscrowRelease, String> {
        if let Some(mut escrow) = self.escrows.get_mut(escrow_id) {
            if escrow.status != EscrowStatus::Active
                && escrow.status != EscrowStatus::Funded
                && escrow.status != EscrowStatus::Disputed
            {
                return Err(format!(
                    "Cannot refund escrow in {} status",
                    escrow.status.as_str()
                ));
            }

            let refund_reason = reason.unwrap_or("Escrow refunded");
            let release = EscrowRelease::new(
                escrow_id,
                &escrow.buyer_id,
                escrow.amount_nanoerg,
                refund_reason,
            );
            let release_clone = release.clone();

            escrow.status = EscrowStatus::Refunded;

            if let Some(mut releases) = self.releases.get_mut(escrow_id) {
                releases.push(release_clone);
            }

            Ok(release)
        } else {
            Err("Escrow not found".to_string())
        }
    }

    /// Mark an escrow as disputed.
    pub fn dispute(&self, escrow_id: &str) -> Result<EscrowContract, String> {
        if let Some(mut escrow) = self.escrows.get_mut(escrow_id) {
            if escrow.status != EscrowStatus::Active && escrow.status != EscrowStatus::Funded {
                return Err(format!(
                    "Cannot dispute escrow in {} status",
                    escrow.status.as_str()
                ));
            }
            escrow.status = EscrowStatus::Disputed;
            Ok(escrow.clone())
        } else {
            Err("Escrow not found".to_string())
        }
    }

    /// Check and update condition status for an escrow.
    pub fn check_conditions(&self, escrow_id: &str, condition_id: &str) -> Result<EscrowContract, String> {
        if let Some(mut escrow) = self.escrows.get_mut(escrow_id) {
            let mut found = false;
            for cond in &mut escrow.conditions {
                if cond.condition_id == condition_id {
                    cond.mark_met();
                    found = true;
                    break;
                }
            }

            if !found {
                return Err("Condition not found".to_string());
            }

            // If all conditions are met, auto-activate if config allows
            if escrow.conditions.iter().all(|c| c.met) && self.get_config().auto_release {
                escrow.status = EscrowStatus::Active;
            }

            Ok(escrow.clone())
        } else {
            Err("Escrow not found".to_string())
        }
    }

    /// Get an escrow by ID.
    pub fn get_escrow(&self, escrow_id: &str) -> Option<EscrowContract> {
        self.escrows.get(escrow_id).map(|e| e.clone())
    }

    /// List escrows with optional status filter.
    pub fn list_escrows(
        &self,
        status: Option<&EscrowStatus>,
        limit: usize,
        offset: usize,
    ) -> Vec<EscrowContract> {
        let mut all: Vec<EscrowContract> = self
            .escrows
            .iter()
            .map(|e| e.value().clone())
            .filter(|e| status.is_none() || status == Some(&e.status))
            .collect();
        all.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        all.into_iter().skip(offset).take(limit).collect()
    }

    /// Get balance: total nanoERG held in active escrows.
    pub fn get_balance(&self) -> EscrowBalance {
        let mut total_held = 0u64;
        let mut total_fees = 0u64;
        let mut active_count = 0usize;
        let mut total_escrows = 0usize;

        for entry in self.escrows.iter() {
            let e = entry.value();
            total_escrows += 1;
            match e.status {
                EscrowStatus::Funded | EscrowStatus::Active => {
                    total_held += e.amount_nanoerg;
                    total_fees += e.fee_nanoerg;
                    active_count += 1;
                }
                _ => {}
            }
        }

        EscrowBalance {
            total_held_nanoerg: total_held,
            total_fees_nanoerg: total_fees,
            active_escrows: active_count,
            total_escrows,
        }
    }

    /// Get release history for an escrow.
    pub fn get_history(&self, escrow_id: &str) -> Vec<EscrowRelease> {
        self.releases
            .get(escrow_id)
            .map(|r| r.clone())
            .unwrap_or_default()
    }

    /// Check for timed-out escrows and refund them.
    pub fn timeout_check(&self, current_height: u64) -> usize {
        let mut count = 0;

        for mut entry in self.escrows.iter_mut() {
            let escrow = entry.value_mut();
            match escrow.status {
                EscrowStatus::Funded | EscrowStatus::Active => {
                    let release_h = escrow.release_height.unwrap_or(0);
                    if current_height >= release_h && release_h > 0 {
                        escrow.status = EscrowStatus::Expired;
                        let release = EscrowRelease::new(
                            &escrow.escrow_id,
                            &escrow.buyer_id,
                            escrow.amount_nanoerg,
                            "Timed out - auto refund",
                        );
                        if let Some(mut releases) = self.releases.get_mut(&escrow.escrow_id) {
                            releases.push(release);
                        }
                        count += 1;
                    }
                }
                _ => {}
            }
        }

        count
    }
}

// ---------------------------------------------------------------------------
// EscrowBalance
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct EscrowBalance {
    pub total_held_nanoerg: u64,
    pub total_fees_nanoerg: u64,
    pub active_escrows: usize,
    pub total_escrows: usize,
}

// ---------------------------------------------------------------------------
// REST Handlers
// ---------------------------------------------------------------------------

pub async fn create_escrow_handler(
    State(state): State<super::proxy::AppState>,
    Json(req): Json<CreateEscrowRequest>,
) -> Json<serde_json::Value> {
    match state.escrow_manager.create_escrow(&req) {
        Ok(escrow) => Json(serde_json::to_value(escrow).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

pub async fn fund_escrow_handler(
    State(state): State<super::proxy::AppState>,
    Path(id): Path<String>,
    Json(req): Json<FundEscrowRequest>,
) -> Json<serde_json::Value> {
    match state.escrow_manager.fund_escrow(&id, &req) {
        Ok(escrow) => Json(serde_json::to_value(escrow).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

pub async fn release_funds_handler(
    State(state): State<super::proxy::AppState>,
    Path(id): Path<String>,
    Json(req): Json<ReleaseFundsRequest>,
) -> Json<serde_json::Value> {
    match state.escrow_manager.release_funds(&id, &req) {
        Ok(release) => Json(serde_json::to_value(release).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

pub async fn refund_handler(
    State(state): State<super::proxy::AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.escrow_manager.refund(&id, None) {
        Ok(release) => Json(serde_json::to_value(release).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

pub async fn dispute_escrow_handler(
    State(state): State<super::proxy::AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.escrow_manager.dispute(&id) {
        Ok(escrow) => Json(serde_json::to_value(escrow).unwrap_or_default()),
        Err(e) => Json(serde_json::json!({ "error": e })),
    }
}

pub async fn get_escrow_handler(
    State(state): State<super::proxy::AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match state.escrow_manager.get_escrow(&id) {
        Some(escrow) => Json(serde_json::to_value(escrow).unwrap_or_default()),
        None => Json(serde_json::json!({ "error": "not_found" })),
    }
}

#[derive(Deserialize)]
pub struct EscrowListQuery {
    pub status: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

pub async fn list_escrows_handler(
    State(state): State<super::proxy::AppState>,
    Query(params): Query<EscrowListQuery>,
) -> Json<serde_json::Value> {
    let status = params.status.as_deref().and_then(EscrowStatus::from_str);
    let limit = params.limit.unwrap_or(20);
    let offset = params.offset.unwrap_or(0);
    let escrows = state.escrow_manager.list_escrows(status.as_ref(), limit, offset);
    Json(serde_json::json!({
        "escrows": escrows,
        "limit": limit,
        "offset": offset,
    }))
}

pub async fn get_escrow_history_handler(
    State(state): State<super::proxy::AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let history = state.escrow_manager.get_history(&id);
    Json(serde_json::json!({
        "escrow_id": id,
        "history": history,
    }))
}

pub async fn get_balance_handler(
    State(state): State<super::proxy::AppState>,
) -> Json<serde_json::Value> {
    let balance = state.escrow_manager.get_balance();
    Json(serde_json::to_value(balance).unwrap_or_default())
}

#[derive(Deserialize)]
pub struct TimeoutCheckBody {
    pub current_height: u64,
}

pub async fn timeout_check_handler(
    State(state): State<super::proxy::AppState>,
    Json(body): Json<TimeoutCheckBody>,
) -> Json<serde_json::Value> {
    let count = state.escrow_manager.timeout_check(body.current_height);
    Json(serde_json::json!({
        "status": "checked",
        "expired_count": count,
        "current_height": body.current_height,
    }))
}

pub async fn escrow_config_handler(
    State(state): State<super::proxy::AppState>,
) -> Json<serde_json::Value> {
    let config = state.escrow_manager.get_config();
    Json(serde_json::to_value(config).unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manager() -> EscrowManager {
        EscrowManager::new()
    }

    fn make_create_req() -> CreateEscrowRequest {
        CreateEscrowRequest {
            buyer_id: "buyer-1".to_string(),
            seller_id: "seller-1".to_string(),
            amount_nanoerg: 10_000_000,
            timeout_blocks: None,
            conditions: None,
        }
    }

    #[test]
    fn test_create_escrow() {
        let manager = make_manager();
        let escrow = manager.create_escrow(&make_create_req()).unwrap();
        assert_eq!(escrow.status, EscrowStatus::Created);
        assert_eq!(escrow.buyer_id, "buyer-1");
        assert_eq!(escrow.seller_id, "seller-1");
        assert_eq!(escrow.amount_nanoerg, 10_000_000);
        assert!(escrow.fee_nanoerg > 0);
        assert_ne!(escrow.contract_hash, [0u8; 32]);
    }

    #[test]
    fn test_create_escrow_below_minimum() {
        let manager = make_manager();
        let req = CreateEscrowRequest {
            buyer_id: "b1".to_string(),
            seller_id: "s1".to_string(),
            amount_nanoerg: 100,
            timeout_blocks: None,
            conditions: None,
        };
        let result = manager.create_escrow(&req);
        assert!(result.is_err());
    }

    #[test]
    fn test_fund_escrow() {
        let manager = make_manager();
        let escrow = manager.create_escrow(&make_create_req()).unwrap();
        let req = FundEscrowRequest {
            box_id: "box-abc123".to_string(),
            current_height: Some(1000),
        };
        let funded = manager.fund_escrow(&escrow.escrow_id, &req).unwrap();
        assert_eq!(funded.status, EscrowStatus::Active); // No conditions -> auto-active
        assert_eq!(funded.box_id, "box-abc123");
        assert!(funded.funded_at.is_some());
        assert_eq!(funded.release_height, Some(1000 + 720));
    }

    #[test]
    fn test_fund_with_conditions() {
        let manager = make_manager();
        let req = CreateEscrowRequest {
            buyer_id: "b1".to_string(),
            seller_id: "s1".to_string(),
            amount_nanoerg: 10_000_000,
            timeout_blocks: None,
            conditions: Some(vec![CreateConditionRequest {
                condition_type: "DeliveryConfirmation".to_string(),
                parameter: "delivery-verified".to_string(),
            }]),
        };
        let escrow = manager.create_escrow(&req).unwrap();
        let funded = manager.fund_escrow(&escrow.escrow_id, &FundEscrowRequest {
            box_id: "box-1".to_string(),
            current_height: Some(1000),
        }).unwrap();
        // With conditions, stays in Funded status until conditions met
        assert_eq!(funded.status, EscrowStatus::Funded);
        assert_eq!(funded.conditions.len(), 1);
    }

    #[test]
    fn test_release_funds() {
        let manager = make_manager();
        let escrow = manager.create_escrow(&make_create_req()).unwrap();
        manager.fund_escrow(&escrow.escrow_id, &FundEscrowRequest {
            box_id: "box-1".to_string(),
            current_height: Some(1000),
        }).unwrap();

        let release = manager.release_funds(&escrow.escrow_id, &ReleaseFundsRequest {
            recipient_id: None,
            amount_nanoerg: None,
            reason: None,
        }).unwrap();

        assert_eq!(release.recipient_id, "seller-1");
        assert!(release.amount_nanoerg > 0);
        assert_eq!(release.reason, "Funds released");

        let updated = manager.get_escrow(&escrow.escrow_id).unwrap();
        assert_eq!(updated.status, EscrowStatus::Released);
    }

    #[test]
    fn test_refund() {
        let manager = make_manager();
        let escrow = manager.create_escrow(&make_create_req()).unwrap();
        manager.fund_escrow(&escrow.escrow_id, &FundEscrowRequest {
            box_id: "box-1".to_string(),
            current_height: Some(1000),
        }).unwrap();

        let refund = manager.refund(&escrow.escrow_id, Some("Buyer requested")).unwrap();
        assert_eq!(refund.recipient_id, "buyer-1");
        assert_eq!(refund.amount_nanoerg, 10_000_000);

        let updated = manager.get_escrow(&escrow.escrow_id).unwrap();
        assert_eq!(updated.status, EscrowStatus::Refunded);
    }

    #[test]
    fn test_dispute() {
        let manager = make_manager();
        let escrow = manager.create_escrow(&make_create_req()).unwrap();
        manager.fund_escrow(&escrow.escrow_id, &FundEscrowRequest {
            box_id: "box-1".to_string(),
            current_height: Some(1000),
        }).unwrap();

        let disputed = manager.dispute(&escrow.escrow_id).unwrap();
        assert_eq!(disputed.status, EscrowStatus::Disputed);
    }

    #[test]
    fn test_check_conditions() {
        let manager = make_manager();
        let req = CreateEscrowRequest {
            buyer_id: "b1".to_string(),
            seller_id: "s1".to_string(),
            amount_nanoerg: 10_000_000,
            timeout_blocks: None,
            conditions: Some(vec![CreateConditionRequest {
                condition_type: "DeliveryConfirmation".to_string(),
                parameter: "delivered".to_string(),
            }]),
        };
        let escrow = manager.create_escrow(&req).unwrap();
        manager.fund_escrow(&escrow.escrow_id, &FundEscrowRequest {
            box_id: "box-1".to_string(),
            current_height: Some(1000),
        }).unwrap();

        let cond_id = escrow.conditions[0].condition_id.clone();
        let result = manager.check_conditions(&escrow.escrow_id, &cond_id).unwrap();
        // All conditions met -> auto release to Active
        assert_eq!(result.status, EscrowStatus::Active);
    }

    #[test]
    fn test_list_escrows() {
        let manager = make_manager();
        manager.create_escrow(&make_create_req()).unwrap();
        manager.create_escrow(&CreateEscrowRequest {
            buyer_id: "b2".to_string(),
            seller_id: "s2".to_string(),
            amount_nanoerg: 5_000_000,
            timeout_blocks: None,
            conditions: None,
        }).unwrap();

        let all = manager.list_escrows(None, 10, 0);
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_get_balance() {
        let manager = make_manager();
        let escrow = manager.create_escrow(&make_create_req()).unwrap();
        manager.fund_escrow(&escrow.escrow_id, &FundEscrowRequest {
            box_id: "box-1".to_string(),
            current_height: Some(1000),
        }).unwrap();

        let balance = manager.get_balance();
        assert_eq!(balance.active_escrows, 1);
        assert_eq!(balance.total_held_nanoerg, 10_000_000);
    }

    #[test]
    fn test_timeout_check() {
        let manager = make_manager();
        let escrow = manager.create_escrow(&make_create_req()).unwrap();
        manager.fund_escrow(&escrow.escrow_id, &FundEscrowRequest {
            box_id: "box-1".to_string(),
            current_height: Some(100),
        }).unwrap();

        // release_height = 100 + 720 = 820. Check at 900 -> should expire
        let count = manager.timeout_check(900);
        assert_eq!(count, 1);

        let updated = manager.get_escrow(&escrow.escrow_id).unwrap();
        assert_eq!(updated.status, EscrowStatus::Expired);
    }

    #[test]
    fn test_fee_calculation() {
        let manager = make_manager();
        let fee = manager.calculate_fee(1_000_000_000);
        assert_eq!(fee, 1_000_000); // 0.1% of 1B
    }

    #[test]
    fn test_escrow_config_defaults() {
        let config = EscrowConfig::default();
        assert_eq!(config.min_deposit_nanoerg, 1_000_000);
        assert_eq!(config.fee_rate_pct, 0.1);
        assert_eq!(config.default_timeout_blocks, 720);
        assert_eq!(config.max_escrow_amount_nanoerg, 100_000_000_000);
        assert!(config.auto_release);
    }

    #[test]
    fn test_seller_amount() {
        let manager = make_manager();
        let escrow = manager.create_escrow(&make_create_req()).unwrap();
        // seller_amount = amount - fee
        assert_eq!(escrow.seller_amount(), escrow.amount_nanoerg - escrow.fee_nanoerg);
        assert!(escrow.total_amount() > escrow.amount_nanoerg);
    }

    #[test]
    fn test_get_history() {
        let manager = make_manager();
        let escrow = manager.create_escrow(&make_create_req()).unwrap();
        manager.fund_escrow(&escrow.escrow_id, &FundEscrowRequest {
            box_id: "box-1".to_string(),
            current_height: Some(1000),
        }).unwrap();
        manager.release_funds(&escrow.escrow_id, &ReleaseFundsRequest {
            recipient_id: None,
            amount_nanoerg: None,
            reason: None,
        }).unwrap();

        let history = manager.get_history(&escrow.escrow_id);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].recipient_id, "seller-1");
    }

    #[test]
    fn test_escrow_status_from_str() {
        assert_eq!(EscrowStatus::from_str("Created"), Some(EscrowStatus::Created));
        assert_eq!(EscrowStatus::from_str("Released"), Some(EscrowStatus::Released));
        assert_eq!(EscrowStatus::from_str("Invalid"), None);
    }

    #[test]
    fn test_condition_types() {
        assert_eq!(EscrowConditionType::DeliveryConfirmation.as_str(), "DeliveryConfirmation");
        assert_eq!(EscrowConditionType::Milestone.as_str(), "Milestone");
        assert_eq!(EscrowConditionType::TimeLock.as_str(), "TimeLock");
        assert_eq!(EscrowConditionType::DisputeResolution.as_str(), "DisputeResolution");
    }
}
