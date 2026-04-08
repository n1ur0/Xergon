//! Proxy Contract Manager for secure dApp interactions.
//!
//! Proxy contracts are "spending scripts" that users send funds to. The script
//! then interacts with the target dApp based on predefined rules, ensuring user
//! funds are only used as intended. Pattern used by Ergo Auction House, ErgoUtils,
//! and SigmaUSD web interface.
//!
//! # Contract Types
//!
//! - **StakingOnly**: Only allows spending to Xergon staking contract
//! - **ProviderPayment**: Payments only to registered provider addresses
//! - **GovernanceVote**: Voting only on specific proposal NFT IDs
//! - **General**: Time-limited, amount-capped, recipient-whitelisted proxy

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json, Router,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use uuid::Uuid;
use chrono::Utc;

// ─── Constants ──────────────────────────────────────────────────────

/// Max audit log entries (ring buffer).
const AUDIT_LOG_MAX: usize = 10_000;
/// Default proxy expiry: 1 year in blocks (~720 blocks/day).
const DEFAULT_EXPIRY_BLOCKS: u32 = 262_800;
/// Default max amount per spend (100 ERG).
const DEFAULT_MAX_AMOUNT_NANOERG: u64 = 100_000_000_000;
/// Min proxy box value.
const MIN_PROXY_BOX_VALUE: u64 = 1_000_000;

// ─── Data Types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProxyType {
    StakingOnly,
    ProviderPayment,
    GovernanceVote,
    General,
}

impl std::fmt::Display for ProxyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProxyType::StakingOnly => write!(f, "staking_only"),
            ProxyType::ProviderPayment => write!(f, "provider_payment"),
            ProxyType::GovernanceVote => write!(f, "governance_vote"),
            ProxyType::General => write!(f, "general"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenConstraint {
    pub token_id: String,
    pub min_amount: u64,
    pub max_amount: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub proxy_type: ProxyType,
    pub allowed_recipients: Vec<String>,
    pub max_amount_nanoerg: u64,
    pub expiry_height: u32,
    pub allowed_tokens: Vec<TokenConstraint>,
    pub description: String,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            proxy_type: ProxyType::General,
            allowed_recipients: vec![],
            max_amount_nanoerg: DEFAULT_MAX_AMOUNT_NANOERG,
            expiry_height: DEFAULT_EXPIRY_BLOCKS,
            allowed_tokens: vec![],
            description: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyTemplate {
    pub id: String,
    pub name: String,
    pub proxy_type: ProxyType,
    pub ergo_tree_hex: String,
    pub config: ProxyConfig,
    pub created_at: String,
    pub usage_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentStatus {
    Active,
    Expired,
    Revoked,
    InsufficientFunds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyDeployment {
    pub template_id: String,
    pub deployed_box_id: String,
    pub address: String,
    pub config: ProxyConfig,
    pub deployed_at: String,
    pub status: DeploymentStatus,
    pub current_value: u64,
    pub current_height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpendValidation {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub proxy_id: String,
    pub tx_id: String,
    pub input_value: u64,
    pub output_value: u64,
    pub recipient: String,
    pub timestamp: String,
    pub status: String,
}

// ─── Request/Response Types ─────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateTemplateRequest {
    pub name: String,
    pub proxy_type: ProxyType,
    pub config: Option<ProxyConfig>,
}

#[derive(Debug, Serialize)]
pub struct CreateTemplateResponse {
    pub template: ProxyTemplate,
}

#[derive(Debug, Deserialize)]
pub struct DeployProxyRequest {
    pub template_id: String,
    pub initial_value_nanoerg: u64,
}

#[derive(Debug, Serialize)]
pub struct DeployProxyResponse {
    pub deployment: ProxyDeployment,
}

#[derive(Debug, Deserialize)]
pub struct ValidateSpendRequest {
    pub proxy_id: String,
    pub recipient: String,
    pub amount_nanoerg: u64,
    pub tokens: Vec<TokenSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSpec {
    pub token_id: String,
    pub amount: u64,
}

#[derive(Debug, Serialize)]
pub struct ValidateSpendResponse {
    pub validation: SpendValidation,
}

#[derive(Debug, Deserialize)]
pub struct AuditQueryParams {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct AuditLogResponse {
    pub entries: Vec<AuditEntry>,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: String,
    pub code: u16,
}

// ─── Proxy Contract Compilation ─────────────────────────────────────

/// Compile an ErgoScript proxy contract for staking-only operations.
/// Only allows spending to Xergon staking contract (proveDlog check on R4).
pub fn compile_staking_proxy(config: &ProxyConfig) -> String {
    // Placeholder: real implementation would use sigma-rust or Fleet SDK
    // to compile ErgoScript to ErgoTree hex.
    // The script checks: output goes to staking contract AND amount <= max
    let max_val = config.max_amount_nanoerg.to_string();
    let recipients_json = serde_json::to_string(&config.allowed_recipients).unwrap_or_default();
    format!(
        "0xPLACEHOLDER_STAKING_PROXY_max{}_recipients{}",
        max_val,
        recipients_json.len()
    )
}

/// Compile proxy for provider payments only.
/// Validates recipient is in registered provider list stored in R4.
pub fn compile_provider_payment_proxy(config: &ProxyConfig) -> String {
    let max_val = config.max_amount_nanoerg.to_string();
    format!(
        "0xPLACEHOLDER_PROVIDER_PROXY_max{}",
        max_val
    )
}

/// Compile proxy for governance voting.
/// Only allows spending to proposal boxes with specific NFT IDs.
pub fn compile_governance_proxy(config: &ProxyConfig) -> String {
    let allowed_count = config.allowed_recipients.len();
    format!(
        "0xPLACEHOLDER_GOVERNANCE_PROXY_proposals{}",
        allowed_count
    )
}

/// Compile general-purpose proxy with time limit, amount cap, and recipient whitelist.
pub fn compile_general_proxy(config: &ProxyConfig) -> String {
    let max_val = config.max_amount_nanoerg.to_string();
    let recipients_count = config.allowed_recipients.len();
    let tokens_count = config.allowed_tokens.len();
    format!(
        "0xPLACEHOLDER_GENERAL_PROXY_max{}_recips{}_tokens{}",
        max_val, recipients_count, tokens_count
    )
}

/// Compile a proxy contract based on its type.
pub fn compile_proxy(template: &ProxyTemplate) -> String {
    match template.proxy_type {
        ProxyType::StakingOnly => compile_staking_proxy(&template.config),
        ProxyType::ProviderPayment => compile_provider_payment_proxy(&template.config),
        ProxyType::GovernanceVote => compile_governance_proxy(&template.config),
        ProxyType::General => compile_general_proxy(&template.config),
    }
}

// ─── Spend Validation ───────────────────────────────────────────────

/// Validate that a proposed transaction matches proxy constraints.
pub fn validate_spend(
    config: &ProxyConfig,
    recipient: &str,
    amount_nanoerg: u64,
    tokens: &[TokenSpec],
    current_height: u32,
) -> SpendValidation {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Check expiry
    if current_height >= config.expiry_height {
        errors.push(format!(
            "Proxy expired at height {}. Current: {}",
            config.expiry_height, current_height
        ));
    }

    // Check amount
    if amount_nanoerg == 0 {
        errors.push("Amount must be greater than zero".to_string());
    } else if amount_nanoerg > config.max_amount_nanoerg {
        errors.push(format!(
            "Amount {} nanoERG exceeds maximum {} nanoERG",
            amount_nanoerg, config.max_amount_nanoerg
        ));
    } else if amount_nanoerg > config.max_amount_nanoerg / 2 {
        warnings.push(format!(
            "Amount is >50% of maximum ({:.1}%)",
            (amount_nanoerg as f64 / config.max_amount_nanoerg as f64) * 100.0
        ));
    }

    // Check recipient
    if !config.allowed_recipients.is_empty() {
        if !config.allowed_recipients.iter().any(|r| {
            r.eq_ignore_ascii_case(recipient) || recipient.starts_with(r)
        }) {
            errors.push(format!(
                "Recipient {} is not in the allowed list ({} allowed)",
                recipient,
                config.allowed_recipients.len()
            ));
        }
    }

    // Check tokens
    if !config.allowed_tokens.is_empty() {
        for token in tokens {
            let constraint = config.allowed_tokens.iter().find(|c| c.token_id == token.token_id);
            match constraint {
                None => errors.push(format!(
                    "Token {} is not allowed by this proxy",
                    token.token_id
                )),
                Some(c) => {
                    if token.amount < c.min_amount {
                        errors.push(format!(
                            "Token {} amount {} is below minimum {}",
                            token.token_id, token.amount, c.min_amount
                        ));
                    }
                    if token.amount > c.max_amount {
                        errors.push(format!(
                            "Token {} amount {} exceeds maximum {}",
                            token.token_id, token.amount, c.max_amount
                        ));
                    }
                }
            }
        }
    }

    // Check min box value for output
    let output_value = amount_nanoerg.saturating_sub(MIN_PROXY_BOX_VALUE);
    if output_value < MIN_PROXY_BOX_VALUE && amount_nanoerg > MIN_PROXY_BOX_VALUE {
        warnings.push("Output value is close to minimum box value".to_string());
    }

    SpendValidation {
        valid: errors.is_empty(),
        errors,
        warnings,
    }
}

// ─── Application State ──────────────────────────────────────────────

#[derive(Debug)]
pub struct AppState {
    pub templates: Arc<DashMap<String, ProxyTemplate>>,
    pub deployments: Arc<DashMap<String, ProxyDeployment>>,
    pub audit_log: Arc<DashMap<String, VecDeque<AuditEntry>>>,
    pub audit_total: AtomicU64,
    pub current_height: AtomicU32,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            templates: Arc::new(DashMap::new()),
            deployments: Arc::new(DashMap::new()),
            audit_log: Arc::new(DashMap::new()),
            audit_total: AtomicU64::new(0),
            current_height: AtomicU32::new(0),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Handler Functions ──────────────────────────────────────────────

/// POST /api/proxy/template — Create a new proxy template.
async fn create_template(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateTemplateRequest>,
) -> Result<Json<CreateTemplateResponse>, (StatusCode, Json<ApiError>)> {
    let id = Uuid::new_v4().to_string();
    let config = req.config.unwrap_or_default();
    let proxy_type = req.proxy_type.clone();

    let mut template = ProxyTemplate {
        id: id.clone(),
        name: req.name,
        proxy_type,
        ergo_tree_hex: String::new(),
        config,
        created_at: Utc::now().to_rfc3339(),
        usage_count: 0,
    };

    template.ergo_tree_hex = compile_proxy(&template);

    state.templates.insert(id.clone(), template.clone());

    Ok(Json(CreateTemplateResponse { template }))
}

/// GET /api/proxy/templates — List all proxy templates.
async fn list_templates(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<ProxyTemplate>> {
    let templates: Vec<ProxyTemplate> = state
        .templates
        .iter()
        .map(|r| r.value().clone())
        .collect();
    Json(templates)
}

/// POST /api/proxy/deploy — Deploy a proxy contract to chain.
async fn deploy_proxy(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DeployProxyRequest>,
) -> Result<Json<DeployProxyResponse>, (StatusCode, Json<ApiError>)> {
    let template = state
        .templates
        .get(&req.template_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: format!("Template {} not found", req.template_id),
                    code: 404,
                }),
            )
        })?;

    if req.initial_value_nanoerg < MIN_PROXY_BOX_VALUE {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: format!(
                    "Initial value {} is below minimum {} nanoERG",
                    req.initial_value_nanoerg, MIN_PROXY_BOX_VALUE
                ),
                code: 400,
            }),
        ));
    }

    let box_id = Uuid::new_v4().to_string();
    let current_height = state.current_height.load(Ordering::Relaxed);

    let deployment = ProxyDeployment {
        template_id: req.template_id.clone(),
        deployed_box_id: box_id.clone(),
        address: format!("9{}", &box_id[..40]), // mock P2S address
        config: template.config.clone(),
        deployed_at: Utc::now().to_rfc3339(),
        status: DeploymentStatus::Active,
        current_value: req.initial_value_nanoerg,
        current_height,
    };

    state.deployments.insert(box_id.clone(), deployment.clone());

    // Increment usage count on template
    if let Some(mut tmpl) = state.templates.get_mut(&req.template_id) {
        tmpl.usage_count += 1;
    }

    Ok(Json(DeployProxyResponse { deployment }))
}

/// GET /api/proxy/deployments — List all deployments.
async fn list_deployments(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<ProxyDeployment>> {
    let deployments: Vec<ProxyDeployment> = state
        .deployments
        .iter()
        .map(|r| r.value().clone())
        .collect();
    Json(deployments)
}

/// GET /api/proxy/deployments/:id — Get deployment status.
async fn get_deployment(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ProxyDeployment>, (StatusCode, Json<ApiError>)> {
    state.deployments.get(&id).map(|r| Json(r.value().clone())).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: format!("Deployment {} not found", id),
                code: 404,
            }),
        )
    })
}

/// POST /api/proxy/validate — Validate a proposed spend.
async fn validate_spend_handler(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ValidateSpendRequest>,
) -> Result<Json<ValidateSpendResponse>, (StatusCode, Json<ApiError>)> {
    let deployment = state.deployments.get(&req.proxy_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: format!("Proxy deployment {} not found", req.proxy_id),
                code: 404,
            }),
        )
    })?;

    let current_height = state.current_height.load(Ordering::Relaxed);
    let validation = validate_spend(
        &deployment.config,
        &req.recipient,
        req.amount_nanoerg,
        &req.tokens,
        current_height,
    );

    // Record in audit log
    let entry = AuditEntry {
        proxy_id: req.proxy_id.clone(),
        tx_id: format!("pending_{}", Uuid::new_v4()),
        input_value: deployment.current_value,
        output_value: req.amount_nanoerg,
        recipient: req.recipient.clone(),
        timestamp: Utc::now().to_rfc3339(),
        status: if validation.valid {
            "validated".to_string()
        } else {
            "rejected".to_string()
        },
    };

    record_audit(&state, req.proxy_id.clone(), entry);

    Ok(Json(ValidateSpendResponse { validation }))
}

/// GET /api/proxy/audit/:id — Get audit log for proxy.
async fn get_audit_log(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<AuditQueryParams>,
) -> Result<Json<AuditLogResponse>, (StatusCode, Json<ApiError>)> {
    let entries = state.audit_log.get(&id).map(|r| {
        let log = r.value();
        let offset = params.offset.unwrap_or(0);
        let limit = params.limit.unwrap_or(100);
        log.iter()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect::<Vec<_>>()
    });

    match entries {
        Some(e) => {
            let total = state
                .audit_log
                .get(&id)
                .map(|r| r.value().len())
                .unwrap_or(0);
            Ok(Json(AuditLogResponse { entries: e, total }))
        }
        None => Ok(Json(AuditLogResponse {
            entries: vec![],
            total: 0,
        })),
    }
}

/// Record an audit entry with ring buffer overflow protection.
fn record_audit(state: &Arc<AppState>, proxy_id: String, entry: AuditEntry) {
    state.audit_total.fetch_add(1, Ordering::Relaxed);
    let mut log = state
        .audit_log
        .entry(proxy_id)
        .or_insert_with(VecDeque::new);
    if log.len() >= AUDIT_LOG_MAX {
        log.pop_front();
    }
    log.push_back(entry);
}

// ─── Router ─────────────────────────────────────────────────────────

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/proxy/template", axum::routing::post(create_template))
        .route("/api/proxy/templates", axum::routing::get(list_templates))
        .route("/api/proxy/deploy", axum::routing::post(deploy_proxy))
        .route("/api/proxy/deployments", axum::routing::get(list_deployments))
        .route(
            "/api/proxy/deployments/:id",
            axum::routing::get(get_deployment),
        )
        .route("/api/proxy/validate", axum::routing::post(validate_spend_handler))
        .route("/api/proxy/audit/:id", axum::routing::get(get_audit_log))
        .with_state(state)
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_staking_config() -> ProxyConfig {
        ProxyConfig {
            proxy_type: ProxyType::StakingOnly,
            allowed_recipients: vec!["9stakingContractAddress123".to_string()],
            max_amount_nanoerg: 10_000_000_000,
            expiry_height: 1_000_000,
            allowed_tokens: vec![],
            description: "Staking only proxy".to_string(),
        }
    }

    fn make_provider_config() -> ProxyConfig {
        ProxyConfig {
            proxy_type: ProxyType::ProviderPayment,
            allowed_recipients: vec![
                "9provider1Address".to_string(),
                "9provider2Address".to_string(),
            ],
            max_amount_nanoerg: 50_000_000_000,
            expiry_height: 500_000,
            allowed_tokens: vec![TokenConstraint {
                token_id: "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890".to_string(),
                min_amount: 100,
                max_amount: 1_000_000,
            }],
            description: "Provider payment proxy".to_string(),
        }
    }

    fn make_general_config() -> ProxyConfig {
        ProxyConfig {
            proxy_type: ProxyType::General,
            allowed_recipients: vec![
                "9recipient1".to_string(),
                "9recipient2".to_string(),
            ],
            max_amount_nanoerg: 100_000_000_000,
            expiry_height: 800_000,
            allowed_tokens: vec![],
            description: "General purpose proxy".to_string(),
        }
    }

    #[test]
    fn test_proxy_type_display() {
        assert_eq!(ProxyType::StakingOnly.to_string(), "staking_only");
        assert_eq!(ProxyType::ProviderPayment.to_string(), "provider_payment");
        assert_eq!(ProxyType::GovernanceVote.to_string(), "governance_vote");
        assert_eq!(ProxyType::General.to_string(), "general");
    }

    #[test]
    fn test_compile_staking_proxy() {
        let config = make_staking_config();
        let hex = compile_staking_proxy(&config);
        assert!(hex.starts_with("0xPLACEHOLDER_STAKING_PROXY"));
        assert!(hex.contains("10000000000")); // max_amount
    }

    #[test]
    fn test_compile_provider_payment_proxy() {
        let config = make_provider_config();
        let hex = compile_provider_payment_proxy(&config);
        assert!(hex.starts_with("0xPLACEHOLDER_PROVIDER_PROXY"));
    }

    #[test]
    fn test_compile_governance_proxy() {
        let config = ProxyConfig {
            proxy_type: ProxyType::GovernanceVote,
            allowed_recipients: vec!["9proposal1".to_string(), "9proposal2".to_string(), "9proposal3".to_string()],
            ..Default::default()
        };
        let hex = compile_governance_proxy(&config);
        assert!(hex.starts_with("0xPLACEHOLDER_GOVERNANCE_PROXY"));
        assert!(hex.contains("3")); // 3 proposals
    }

    #[test]
    fn test_compile_general_proxy() {
        let config = make_general_config();
        let hex = compile_general_proxy(&config);
        assert!(hex.starts_with("0xPLACEHOLDER_GENERAL_PROXY"));
        assert!(hex.contains("2")); // 2 recipients
    }

    #[test]
    fn test_compile_proxy_dispatch() {
        let staking = ProxyTemplate {
            id: "t1".to_string(),
            name: "Staking".to_string(),
            proxy_type: ProxyType::StakingOnly,
            ergo_tree_hex: String::new(),
            config: make_staking_config(),
            created_at: Utc::now().to_rfc3339(),
            usage_count: 0,
        };
        assert!(compile_proxy(&staking).contains("STAKING"));

        let general = ProxyTemplate {
            id: "t2".to_string(),
            name: "General".to_string(),
            proxy_type: ProxyType::General,
            ergo_tree_hex: String::new(),
            config: make_general_config(),
            created_at: Utc::now().to_rfc3339(),
            usage_count: 0,
        };
        assert!(compile_proxy(&general).contains("GENERAL"));
    }

    #[test]
    fn test_validate_spend_valid() {
        let config = make_staking_config();
        let validation = validate_spend(&config, "9stakingContractAddress123", 5_000_000_000, &[], 100_000);
        assert!(validation.valid);
        assert!(validation.errors.is_empty());
    }

    #[test]
    fn test_validate_spend_expired() {
        let config = make_staking_config();
        let validation = validate_spend(&config, "9stakingContractAddress123", 1_000_000_000, &[], 1_500_000);
        assert!(!validation.valid);
        assert!(validation.errors.iter().any(|e| e.contains("expired")));
    }

    #[test]
    fn test_validate_spend_amount_exceeded() {
        let config = make_staking_config();
        let validation = validate_spend(&config, "9stakingContractAddress123", 20_000_000_000, &[], 100_000);
        assert!(!validation.valid);
        assert!(validation.errors.iter().any(|e| e.contains("exceeds maximum")));
    }

    #[test]
    fn test_validate_spend_zero_amount() {
        let config = make_staking_config();
        let validation = validate_spend(&config, "9stakingContractAddress123", 0, &[], 100_000);
        assert!(!validation.valid);
        assert!(validation.errors.iter().any(|e| e.contains("greater than zero")));
    }

    #[test]
    fn test_validate_spend_recipient_not_allowed() {
        let config = make_staking_config();
        let validation = validate_spend(&config, "9unknownAddress", 1_000_000_000, &[], 100_000);
        assert!(!validation.valid);
        assert!(validation.errors.iter().any(|e| e.contains("not in the allowed list")));
    }

    #[test]
    fn test_validate_spend_token_not_allowed() {
        let config = make_provider_config();
        let tokens = vec![TokenSpec {
            token_id: "unknown_token_id".to_string(),
            amount: 500,
        }];
        let validation = validate_spend(&config, "9provider1Address", 1_000_000_000, &tokens, 100_000);
        assert!(!validation.valid);
        assert!(validation.errors.iter().any(|e| e.contains("not allowed")));
    }

    #[test]
    fn test_validate_spend_token_below_min() {
        let config = make_provider_config();
        let token_id = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890".to_string();
        let tokens = vec![TokenSpec {
            token_id: token_id.clone(),
            amount: 50, // below min of 100
        }];
        let validation = validate_spend(&config, "9provider1Address", 1_000_000_000, &tokens, 100_000);
        assert!(!validation.valid);
        assert!(validation.errors.iter().any(|e| e.contains("below minimum")));
    }

    #[test]
    fn test_validate_spend_token_above_max() {
        let config = make_provider_config();
        let token_id = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890".to_string();
        let tokens = vec![TokenSpec {
            token_id: token_id.clone(),
            amount: 2_000_000, // above max of 1_000_000
        }];
        let validation = validate_spend(&config, "9provider1Address", 1_000_000_000, &tokens, 100_000);
        assert!(!validation.valid);
        assert!(validation.errors.iter().any(|e| e.contains("exceeds maximum")));
    }

    #[test]
    fn test_validate_spend_high_amount_warning() {
        let config = make_staking_config();
        // 6B out of 10B max = 60% > 50%
        let validation = validate_spend(&config, "9stakingContractAddress123", 6_000_000_000, &[], 100_000);
        assert!(validation.valid);
        assert!(validation.warnings.iter().any(|w| w.contains(">50%")));
    }

    #[test]
    fn test_validate_spend_no_recipient_filter() {
        let config = ProxyConfig {
            proxy_type: ProxyType::General,
            allowed_recipients: vec![], // empty = any recipient
            max_amount_nanoerg: 100_000_000_000,
            expiry_height: 800_000,
            allowed_tokens: vec![],
            description: "Open proxy".to_string(),
        };
        let validation = validate_spend(&config, "9anyone", 1_000_000_000, &[], 100_000);
        assert!(validation.valid);
    }

    #[test]
    fn test_app_state_new() {
        let state = AppState::new();
        assert_eq!(state.templates.len(), 0);
        assert_eq!(state.deployments.len(), 0);
        assert_eq!(state.audit_total.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_audit_ring_buffer_overflow() {
        let state = Arc::new(AppState::new());
        let proxy_id = "test_proxy".to_string();

        for i in 0..(AUDIT_LOG_MAX + 100) {
            let entry = AuditEntry {
                proxy_id: proxy_id.clone(),
                tx_id: format!("tx_{}", i),
                input_value: 1_000_000,
                output_value: 500_000,
                recipient: "9recipient".to_string(),
                timestamp: Utc::now().to_rfc3339(),
                status: "validated".to_string(),
            };
            record_audit(&state, proxy_id.clone(), entry);
        }

        let log = state.audit_log.get(&proxy_id).unwrap();
        assert_eq!(log.len(), AUDIT_LOG_MAX);
        // First entry should have been evicted
        assert!(log.front().unwrap().tx_id != "tx_0");
        assert!(log.back().unwrap().tx_id == format!("tx_{}", AUDIT_LOG_MAX + 99));
    }

    #[test]
    fn test_proxy_config_default() {
        let config = ProxyConfig::default();
        assert_eq!(config.max_amount_nanoerg, DEFAULT_MAX_AMOUNT_NANOERG);
        assert_eq!(config.expiry_height, DEFAULT_EXPIRY_BLOCKS);
        assert!(config.allowed_recipients.is_empty());
        assert!(config.allowed_tokens.is_empty());
    }

    #[test]
    fn test_token_constraint() {
        let tc = TokenConstraint {
            token_id: "abc".to_string(),
            min_amount: 100,
            max_amount: 1000,
        };
        assert_eq!(tc.min_amount, 100);
        assert_eq!(tc.max_amount, 1000);
    }

    #[test]
    fn test_deployment_status_serde() {
        let statuses = vec![
            DeploymentStatus::Active,
            DeploymentStatus::Expired,
            DeploymentStatus::Revoked,
            DeploymentStatus::InsufficientFunds,
        ];
        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: DeploymentStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, parsed);
        }
    }

    #[test]
    fn test_proxy_type_serde() {
        let types = vec![
            ProxyType::StakingOnly,
            ProxyType::ProviderPayment,
            ProxyType::GovernanceVote,
            ProxyType::General,
        ];
        for pt in types {
            let json = serde_json::to_string(&pt).unwrap();
            let parsed: ProxyType = serde_json::from_str(&json).unwrap();
            assert_eq!(pt, parsed);
        }
    }
}
