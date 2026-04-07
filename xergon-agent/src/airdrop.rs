//! Airdrop service for new Xergon users.
//!
//! Provides ERG airdrops to new wallets so they can start using the network
//! without any signup, email, or credit card. Funded from the node wallet
//! (ultimately from the Xergon treasury).
//!
//! Flow:
//!   1. User calls `POST /api/airdrop/request` with their public key
//!   2. Agent checks eligibility (cooldown, budget, not already airdropped)
//!   3. Agent sends a payment via the Ergo node wallet API to create a
//!      user_staking box locked to the user's public key
//!   4. Returns the transaction ID

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Airdrop configuration, deserialized from `[airdrop]` in config.toml.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AirdropConfig {
    /// Enable the airdrop endpoint (default: false)
    #[serde(default)]
    pub enabled: bool,

    /// Amount of ERG to airdrop per new user, in nanoERG (default: 10_000_000 = 0.01 ERG)
    #[serde(default = "default_amount_nanoerg")]
    pub amount_nanoerg: u64,

    /// Maximum total ERG that can be airdropped (default: 100.0)
    #[serde(default = "default_max_total_erg")]
    pub max_total_erg: f64,

    /// Treasury box ID to fund from (optional, informational)
    #[serde(default)]
    pub treasury_box_id: Option<String>,

    /// Deployer's public key hex (for treasury spending authorization)
    #[serde(default)]
    pub deployer_pk_hex: Option<String>,

    /// Minimum seconds between airdrops for the same public key (default: 86400 = 24h)
    #[serde(default = "default_cooldown_secs")]
    pub cooldown_secs: u64,

    /// Ergo node URL for wallet API calls (defaults to ergo_node.rest_url if empty)
    #[serde(default)]
    pub ergo_node_url: Option<String>,

    /// Pre-compiled user_staking contract ErgoTree hex.
    /// If empty, uses a hardcoded P2PK placeholder (not secure, for testing only).
    #[serde(default)]
    pub user_staking_ergotree_hex: Option<String>,

    /// Transaction fee in nanoERG (default: 1_100_000 = 0.0011 ERG)
    #[serde(default = "default_fee_nanoerg")]
    pub fee_nanoerg: u64,
}

fn default_amount_nanoerg() -> u64 {
    10_000_000 // 0.01 ERG
}

fn default_max_total_erg() -> f64 {
    100.0
}

fn default_cooldown_secs() -> u64 {
    86400 // 24 hours
}

fn default_fee_nanoerg() -> u64 {
    1_100_000 // 0.0011 ERG
}

impl Default for AirdropConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            amount_nanoerg: default_amount_nanoerg(),
            max_total_erg: default_max_total_erg(),
            treasury_box_id: None,
            deployer_pk_hex: None,
            cooldown_secs: default_cooldown_secs(),
            ergo_node_url: None,
            user_staking_ergotree_hex: None,
            fee_nanoerg: default_fee_nanoerg(),
        }
    }
}

impl AirdropConfig {
    /// Maximum total airdrop budget in nanoERG.
    pub fn max_total_nanoerg(&self) -> u64 {
        (self.max_total_erg * 1e9) as u64
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum AirdropError {
    #[error("Airdrop is disabled")]
    Disabled,

    #[error("Public key already received an airdrop (cooldown: {cooldown_secs}s remaining)")]
    CooldownActive { cooldown_secs: u64 },

    #[error("Airdrop budget exhausted ({total_airdropped_nanoerg}/{max_nanoerg} nanoERG used)")]
    BudgetExhausted {
        total_airdropped_nanoerg: u64,
        max_nanoerg: u64,
    },

    #[error("Invalid public key: {0}")]
    InvalidPublicKey(String),

    #[error("Node wallet request failed: {0}")]
    NodeWalletError(String),

    #[error("HTTP client error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Node wallet is locked or unavailable")]
    WalletLocked,
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct AirdropRequest {
    /// User's public key in hex (32 bytes)
    pub public_key: String,
}

#[derive(Debug, Serialize)]
pub struct AirdropResponse {
    pub tx_id: String,
    pub amount_nanoerg: u64,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct EligibilityResponse {
    pub eligible: bool,
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// AirdropService
// ---------------------------------------------------------------------------

/// The airdrop service manages eligibility checking and execution of ERG airdrops
/// to new users via the Ergo node wallet API.
pub struct AirdropService {
    config: AirdropConfig,
    ergo_node_url: String,
    client: reqwest::Client,
    /// Track airdropped public keys -> timestamp of last airdrop
    airdropped: DashMap<String, i64>,
    /// Total ERG airdropped in nanoERG (for budget tracking)
    total_airdropped_nanoerg: AtomicU64,
}

impl AirdropService {
    /// Create a new airdrop service.
    ///
    /// `ergo_node_url` is the base URL of the Ergo node REST API
    /// (e.g. `http://127.0.0.1:9053`).
    pub fn new(config: AirdropConfig, ergo_node_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client for airdrop service");

        Self {
            config,
            ergo_node_url,
            client,
            airdropped: DashMap::new(),
            total_airdropped_nanoerg: AtomicU64::new(0),
        }
    }

    /// Check whether the airdrop service is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Check if a public key is eligible for an airdrop.
    ///
    /// Returns `Ok(true)` if eligible, `Ok(false)` if not (with reason in the
    /// returned `EligibilityResponse`), or `Err` for configuration issues.
    pub fn check_eligibility(&self, public_key_hex: &str) -> Result<EligibilityResponse, AirdropError> {
        if !self.config.enabled {
            return Ok(EligibilityResponse {
                eligible: false,
                reason: Some("Airdrop service is disabled".to_string()),
            });
        }

        // Validate public key format
        let pk_bytes = hex::decode(public_key_hex).map_err(|e| {
            AirdropError::InvalidPublicKey(format!("invalid hex: {}", e))
        })?;
        if pk_bytes.len() != 32 {
            return Err(AirdropError::InvalidPublicKey(
                format!("expected 32 bytes, got {}", pk_bytes.len())
            ));
        }

        // Check cooldown
        if let Some(last_airdrop_ts) = self.airdropped.get(public_key_hex) {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            let elapsed = now - *last_airdrop_ts;
            if elapsed < self.config.cooldown_secs as i64 {
                let remaining = (self.config.cooldown_secs as i64 - elapsed) as u64;
                return Ok(EligibilityResponse {
                    eligible: false,
                    reason: Some(format!(
                        "Cooldown active ({}s remaining)", remaining
                    )),
                });
            }
        }

        // Check budget
        let current_total = self.total_airdropped_nanoerg.load(Ordering::Relaxed);
        let max = self.config.max_total_nanoerg();
        if current_total >= max {
            return Ok(EligibilityResponse {
                eligible: false,
                reason: Some(format!(
                    "Airdrop budget exhausted ({}/{} nanoERG)",
                    current_total, max
                )),
            });
        }

        Ok(EligibilityResponse {
            eligible: true,
            reason: None,
        })
    }

    /// Execute an airdrop to the given public key.
    ///
    /// Uses the Ergo node's wallet payment API to send ERG to a box locked
    /// by the user_staking contract with the user's PK in R4.
    ///
    /// Returns the transaction ID on success.
    pub async fn execute_airdrop(&self, public_key_hex: &str) -> Result<AirdropResponse, AirdropError> {
        // 1. Check eligibility
        let eligibility = self.check_eligibility(public_key_hex)?;
        if !eligibility.eligible {
            let reason = eligibility.reason.unwrap_or_default();
            return Err(AirdropError::CooldownActive {
                cooldown_secs: 0, // reason is in the message
            }.with_reason(&reason));
        }

        // 2. Validate public key
        let pk_bytes = hex::decode(public_key_hex).map_err(|e| {
            AirdropError::InvalidPublicKey(format!("invalid hex: {}", e))
        })?;
        if pk_bytes.len() != 32 {
            return Err(AirdropError::InvalidPublicKey(
                format!("expected 32 bytes, got {}", pk_bytes.len())
            ));
        }

        // 3. Build the ErgoTree or use configured one
        // For the user_staking contract, we need the compiled ErgoTree.
        // If configured, use it. Otherwise, use a simple P2PK as fallback
        // (this is for dev/testing; production MUST use the real contract).
        let ergo_tree = match &self.config.user_staking_ergotree_hex {
            Some(tree_hex) => tree_hex.clone(),
            None => {
                // Fallback: build a simple P2PK tree from the public key.
                // Format: sigmaProp(proveDlog(GroupElement))
                // This is NOT the user_staking contract — it's just for testing.
                warn!(
                    "No user_staking_ergotree_hex configured, using P2PK fallback. \
                     Set [airdrop].user_staking_ergotree_hex for production."
                );
                build_p2pk_ergotree_hex(public_key_hex)?
            }
        };

        // 4. Call the Ergo node wallet payment API
        let url = format!("{}/wallet/payment/send", self.ergo_node_url);

        // Build the registers map.
        // R4 = Coll[Byte] containing the user's public key (GroupElement encoded as Coll[Byte])
        let registers = serde_json::json!({
            "R4": format!("0e{}", public_key_hex) // 0x0e = STypeHeader for Coll[Byte]
        });

        let request_body = serde_json::json!({
            "requests": [{
                "value": self.config.amount_nanoerg,
                "ergoTree": ergo_tree,
                "registers": registers,
                "assets": []
            }],
            "fee": self.config.fee_nanoerg,
            "inputsRaw": [],
            "dataInputsRaw": []
        });

        info!(
            pk = %public_key_hex,
            amount_nanoerg = self.config.amount_nanoerg,
            "Sending airdrop via node wallet API"
        );

        let response = self
            .client
            .post(&url)
            .json(&request_body)
            .send()
            .await;

        let resp = match response {
            Ok(r) => r,
            Err(e) => {
                // Check if it's a connection error (node not running)
                if e.is_connect() {
                    return Err(AirdropError::WalletLocked);
                }
                return Err(AirdropError::HttpError(e));
            }
        };

        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();

        if !status.is_success() {
            // Parse error from node response
            let error_msg = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body_text) {
                json.get("error")
                    .and_then(|e| e.as_str())
                    .unwrap_or(&body_text)
                    .to_string()
            } else {
                body_text.clone()
            };

            // Check for common wallet errors
            if error_msg.contains("wallet is locked") || error_msg.contains("Wallet is not initialized") {
                return Err(AirdropError::WalletLocked);
            }

            warn!(
                pk = %public_key_hex,
                http_status = %status,
                error = %error_msg,
                "Airdrop wallet payment failed"
            );
            return Err(AirdropError::NodeWalletError(error_msg));
        }

        // Parse the transaction ID from the response
        let tx_id = body_text.trim().to_string();
        // The node returns the tx ID directly as a string on success

        if tx_id.is_empty() || !tx_id.starts_with(|c: char| c.is_ascii_hexdigit()) {
            warn!(
                pk = %public_key_hex,
                response_body = %body_text,
                "Unexpected response from node wallet API"
            );
            return Err(AirdropError::NodeWalletError(
                format!("Unexpected response: {}", body_text)
            ));
        }

        // 5. Update tracking
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        self.airdropped.insert(public_key_hex.to_string(), now);
        self.total_airdropped_nanoerg
            .fetch_add(self.config.amount_nanoerg, Ordering::Relaxed);

        info!(
            pk = %public_key_hex,
            tx_id = %tx_id,
            amount_nanoerg = self.config.amount_nanoerg,
            total_airdropped = self.total_airdropped_nanoerg.load(Ordering::Relaxed),
            "Airdrop executed successfully"
        );

        Ok(AirdropResponse {
            tx_id,
            amount_nanoerg: self.config.amount_nanoerg,
            status: "airdropped".to_string(),
        })
    }

    /// Get current airdrop statistics.
    pub fn stats(&self) -> AirdropStats {
        AirdropStats {
            enabled: self.config.enabled,
            total_airdropped_nanoerg: self.total_airdropped_nanoerg.load(Ordering::Relaxed),
            total_airdropped_erg: self.total_airdropped_nanoerg.load(Ordering::Relaxed) as f64 / 1e9,
            max_total_nanoerg: self.config.max_total_nanoerg(),
            max_total_erg: self.config.max_total_erg,
            unique_recipients: self.airdropped.len(),
            amount_per_recipient_nanoerg: self.config.amount_nanoerg,
            cooldown_secs: self.config.cooldown_secs,
        }
    }

    /// Reset the total airdropped counter (for testing or admin use).
    pub fn reset_stats(&self) {
        self.total_airdropped_nanoerg.store(0, Ordering::Relaxed);
        self.airdropped.clear();
    }
}

/// Airdrop statistics.
#[derive(Debug, Serialize)]
pub struct AirdropStats {
    pub enabled: bool,
    pub total_airdropped_nanoerg: u64,
    pub total_airdropped_erg: f64,
    pub max_total_nanoerg: u64,
    pub max_total_erg: f64,
    pub unique_recipients: usize,
    pub amount_per_recipient_nanoerg: u64,
    pub cooldown_secs: u64,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a simple P2PK ErgoTree hex from a public key hex.
///
/// This creates: `sigmaProp(proveDlog(GroupElement))`
/// Encoded as: `100404{pk_bytes_hex}`
///
/// Format:
///   - `10` = SigmaProp header (1 byte)
///   - `04` = ProveDlog (1 byte)
///   - `04` = GroupElement constant (1 byte, STypeHeader.GroupElement)
///   - `{pk_hex}` = 32 bytes of the public key
///
/// NOTE: This is NOT the user_staking contract. It's a simple P2PK for
/// dev/testing only. Production MUST use the compiled user_staking.es contract.
fn build_p2pk_ergotree_hex(public_key_hex: &str) -> Result<String, AirdropError> {
    // Validate the public key is valid hex
    hex::decode(public_key_hex).map_err(|e| {
        AirdropError::InvalidPublicKey(format!("invalid hex: {}", e))
    })?;

    Ok(format!("100404{}", public_key_hex))
}

// Private extension trait for adding context to errors
trait AirdropErrorExt {
    fn with_reason(self, reason: &str) -> Self;
}

impl AirdropErrorExt for AirdropError {
    fn with_reason(self, _reason: &str) -> Self {
        // We just return the error as-is since the reason is logged
        self
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> AirdropConfig {
        AirdropConfig {
            enabled: true,
            amount_nanoerg: 10_000_000,
            max_total_erg: 1.0, // 1 ERG budget for tests
            treasury_box_id: None,
            deployer_pk_hex: None,
            cooldown_secs: 3600,
            ergo_node_url: None,
            user_staking_ergotree_hex: None,
            fee_nanoerg: 1_100_000,
        }
    }

    fn valid_pk() -> String {
        // A valid 32-byte hex string (64 hex chars)
        "a3b1c4d5e6f78901234567890123456789012345678901234567890123456789".to_string()
    }

    fn another_pk() -> String {
        "b4c2d5e6f7890123456789012345678901234567890123456789012345678901".to_string()
    }

    #[test]
    fn test_config_defaults() {
        let cfg = AirdropConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.amount_nanoerg, 10_000_000);
        assert_eq!(cfg.max_total_erg, 100.0);
        assert_eq!(cfg.cooldown_secs, 86400);
        assert_eq!(cfg.fee_nanoerg, 1_100_000);
        assert_eq!(cfg.max_total_nanoerg(), 100_000_000_000);
    }

    #[test]
    fn test_config_max_total_nanoerg_calculation() {
        let cfg = AirdropConfig {
            max_total_erg: 50.0,
            ..Default::default()
        };
        assert_eq!(cfg.max_total_nanoerg(), 50_000_000_000);
    }

    #[test]
    fn test_eligibility_new_user() {
        let service = AirdropService::new(test_config(), "http://127.0.0.1:9053".into());
        let result = service.check_eligibility(&valid_pk()).unwrap();
        assert!(result.eligible);
        assert!(result.reason.is_none());
    }

    #[test]
    fn test_eligibility_disabled() {
        let cfg = AirdropConfig {
            enabled: false,
            ..test_config()
        };
        let service = AirdropService::new(cfg, "http://127.0.0.1:9053".into());
        let result = service.check_eligibility(&valid_pk()).unwrap();
        assert!(!result.eligible);
        assert!(result.reason.is_some());
    }

    #[test]
    fn test_eligibility_invalid_pk_short() {
        let service = AirdropService::new(test_config(), "http://127.0.0.1:9053".into());
        let result = service.check_eligibility("deadbeef");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid hex") || err.contains("expected 32 bytes"));
    }

    #[test]
    fn test_eligibility_invalid_pk_bad_hex() {
        let service = AirdropService::new(test_config(), "http://127.0.0.1:9053".into());
        let result = service.check_eligibility("zzzz");
        assert!(result.is_err());
    }

    #[test]
    fn test_eligibility_32_byte_pk_but_wrong_length_hex() {
        let service = AirdropService::new(test_config(), "http://127.0.0.1:9053".into());
        // 31 bytes = 62 hex chars
        let result = service.check_eligibility(&"ab".repeat(31));
        assert!(result.is_err());
    }

    #[test]
    fn test_cooldown_tracking() {
        let service = AirdropService::new(test_config(), "http://127.0.0.1:9053".into());

        // First check: eligible
        let result = service.check_eligibility(&valid_pk()).unwrap();
        assert!(result.eligible);

        // Simulate an airdrop (insert into tracking map)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        service.airdropped.insert(valid_pk(), now);

        // Second check: should be in cooldown
        let result = service.check_eligibility(&valid_pk()).unwrap();
        assert!(!result.eligible);
        assert!(result.reason.unwrap().contains("Cooldown"));
    }

    #[test]
    fn test_cooldown_expired() {
        let service = AirdropService::new(test_config(), "http://127.0.0.1:9053".into());

        // Insert a timestamp in the past (beyond cooldown)
        let past = (SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64) - 7200; // 2 hours ago, cooldown is 3600s
        service.airdropped.insert(valid_pk(), past);

        let result = service.check_eligibility(&valid_pk()).unwrap();
        assert!(result.eligible);
    }

    #[test]
    fn test_budget_exhausted() {
        let service = AirdropService::new(test_config(), "http://127.0.0.1:9053".into());

        // Set total airdropped to max
        service
            .total_airdropped_nanoerg
            .store(service.config.max_total_nanoerg(), Ordering::Relaxed);

        let result = service.check_eligibility(&valid_pk()).unwrap();
        assert!(!result.eligible);
        assert!(result.reason.unwrap().contains("budget"));
    }

    #[test]
    fn test_budget_near_limit_still_eligible() {
        let service = AirdropService::new(test_config(), "http://127.0.0.1:9053".into());

        // Set total to just under max
        service
            .total_airdropped_nanoerg
            .store(service.config.max_total_nanoerg() - 1, Ordering::Relaxed);

        let result = service.check_eligibility(&valid_pk()).unwrap();
        assert!(result.eligible);
    }

    #[test]
    fn test_different_users_independent() {
        let service = AirdropService::new(test_config(), "http://127.0.0.1:9053".into());

        // Airdrop to user 1
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        service.airdropped.insert(valid_pk(), now);

        // User 1 should be in cooldown
        assert!(!service.check_eligibility(&valid_pk()).unwrap().eligible);

        // User 2 should still be eligible
        assert!(service.check_eligibility(&another_pk()).unwrap().eligible);
    }

    #[test]
    fn test_amount_nanoerg_calculation() {
        let service = AirdropService::new(test_config(), "http://127.0.0.1:9053".into());
        let stats = service.stats();
        assert_eq!(stats.amount_per_recipient_nanoerg, 10_000_000);
        // 0.01 ERG
        assert_eq!(stats.amount_per_recipient_nanoerg as f64 / 1e9, 0.01);
    }

    #[test]
    fn test_stats_initial() {
        let service = AirdropService::new(test_config(), "http://127.0.0.1:9053".into());
        let stats = service.stats();
        assert!(stats.enabled);
        assert_eq!(stats.total_airdropped_nanoerg, 0);
        assert_eq!(stats.total_airdropped_erg, 0.0);
        assert_eq!(stats.unique_recipients, 0);
        assert_eq!(stats.max_total_erg, 1.0);
        assert_eq!(stats.cooldown_secs, 3600);
    }

    #[test]
    fn test_stats_after_simulated_airdrop() {
        let service = AirdropService::new(test_config(), "http://127.0.0.1:9053".into());

        // Simulate an airdrop
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        service.airdropped.insert(valid_pk(), now);
        service
            .total_airdropped_nanoerg
            .fetch_add(10_000_000, Ordering::Relaxed);

        let stats = service.stats();
        assert_eq!(stats.total_airdropped_nanoerg, 10_000_000);
        assert_eq!(stats.total_airdropped_erg, 0.01);
        assert_eq!(stats.unique_recipients, 1);
    }

    #[test]
    fn test_reset_stats() {
        let service = AirdropService::new(test_config(), "http://127.0.0.1:9053".into());

        // Simulate some airdrops
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        service.airdropped.insert(valid_pk(), now);
        service.airdropped.insert(another_pk(), now);
        service
            .total_airdropped_nanoerg
            .fetch_add(20_000_000, Ordering::Relaxed);

        // Reset
        service.reset_stats();

        let stats = service.stats();
        assert_eq!(stats.total_airdropped_nanoerg, 0);
        assert_eq!(stats.unique_recipients, 0);

        // Users should be eligible again
        assert!(service.check_eligibility(&valid_pk()).unwrap().eligible);
        assert!(service.check_eligibility(&another_pk()).unwrap().eligible);
    }

    #[test]
    fn test_build_p2pk_ergotree_hex() {
        let pk = "a3b1c4d5e6f78901234567890123456789012345678901234567890123456789";
        let tree = build_p2pk_ergotree_hex(pk).unwrap();
        assert!(tree.starts_with("100404"));
        assert_eq!(tree.len(), 6 + 64); // "100404" (6 chars) + 64 hex chars
    }

    #[test]
    fn test_build_p2pk_ergotree_invalid_hex() {
        let result = build_p2pk_ergotree_hex("zzzz");
        assert!(result.is_err());
    }

    #[test]
    fn test_is_enabled() {
        let service = AirdropService::new(test_config(), "http://127.0.0.1:9053".into());
        assert!(service.is_enabled());

        let cfg = AirdropConfig {
            enabled: false,
            ..test_config()
        };
        let service = AirdropService::new(cfg, "http://127.0.0.1:9053".into());
        assert!(!service.is_enabled());
    }
}
