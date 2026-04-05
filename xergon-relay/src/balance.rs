//! On-chain user balance verification from Staking Boxes
//!
//! Checks a user's ERG balance by querying the Ergo node for boxes
//! that contain the user's public key in R4. Results are cached
//! for a configurable TTL to reduce load on the node.

use dashmap::DashMap;
use reqwest::Client;
use serde::Deserialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};

use crate::config::BalanceConfig;

/// A cached balance entry with an expiry timestamp.
struct CacheEntry {
    balance_nanoerg: u64,
    box_count: u32,
    inserted_at: std::time::Instant,
}

/// Checks user ERG balance from on-chain Staking Boxes.
pub struct BalanceChecker {
    http_client: Client,
    ergo_node_url: String,
    /// user_pk_hex -> (balance_nanoerg, box_count, inserted_at)
    cache: Arc<DashMap<String, CacheEntry>>,
    cache_ttl_secs: u64,
    /// Hex-encoded ErgoTree for the Staking Box contract (CONTAINS predicate).
    staking_tree_bytes: String,
    /// Free tier request counter per user identity.
    free_tier_usage: Arc<DashMap<String, AtomicU64>>,
    free_tier_max: u64,
}

/// Response for the /v1/balance endpoint.
#[derive(Debug, serde::Serialize)]
pub struct BalanceResponse {
    pub user_pk: String,
    pub balance_nanoerg: u64,
    pub balance_erg: f64,
    pub staking_boxes_count: u32,
    pub sufficient: bool,
    pub min_balance_nanoerg: u64,
}

/// Detailed result from a balance check.
pub struct BalanceCheckResult {
    pub balance_nanoerg: u64,
    pub staking_boxes_count: u32,
    pub is_free_tier: bool,
}

// ── Ergo node response types ──────────────────────────────────────────

/// Response from POST /wallet/registerscan
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterScanResponse {
    scan_id: String,
}

/// Box returned from GET /wallet/boxes/unspent/{scanId}
#[derive(Debug, Deserialize)]
struct StakingBox {
    #[serde(rename = "boxId")]
    #[allow(dead_code)]
    box_id: String,
    #[serde(default)]
    value: u64,
    #[serde(default)]
    additional_registers: std::collections::BTreeMap<String, serde_json::Value>,
}

/// Error type for balance checking.
#[derive(Debug, thiserror::Error)]
pub enum BalanceError {
    #[error("Ergo node unavailable: {0}")]
    NodeUnavailable(String),

    #[error("Failed to register scan: {0}")]
    ScanRegistration(String),

    #[error("Failed to query boxes: {0}")]
    BoxQuery(String),

    #[error("Insufficient ERG balance: have {have_nanoerg} nanoERG, need {min_nanoerg} nanoERG")]
    InsufficientBalance { have_nanoerg: u64, min_nanoerg: u64 },
}

impl BalanceChecker {
    /// Create a new BalanceChecker.
    pub fn new(config: &BalanceConfig) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .expect("Failed to build HTTP client for BalanceChecker");

        Self {
            http_client,
            ergo_node_url: config.ergo_node_url.trim_end_matches('/').to_string(),
            cache: Arc::new(DashMap::new()),
            cache_ttl_secs: config.cache_ttl_secs,
            staking_tree_bytes: config.staking_tree_bytes.clone(),
            free_tier_usage: Arc::new(DashMap::new()),
            free_tier_max: config.free_tier_requests as u64,
        }
    }

    /// Check a user's balance. Returns the balance and whether the user
    /// is in the free tier.
    ///
    /// Flow:
    /// 1. Check free tier — if user hasn't exceeded free_tier_requests, allow.
    /// 2. Check cache for recent balance.
    /// 3. Query Ergo node for staking boxes containing the user's PK in R4.
    /// 4. Sum the ERG value of all matching boxes.
    /// 5. Cache the result.
    pub async fn check_balance(
        &self,
        user_id: &str,
        min_balance_nanoerg: u64,
    ) -> Result<BalanceCheckResult, BalanceError> {
        // 1. Check free tier
        if self.check_free_tier(user_id) {
            debug!(user = %user_id, "User is in free tier");
            return Ok(BalanceCheckResult {
                balance_nanoerg: 0,
                staking_boxes_count: 0,
                is_free_tier: true,
            });
        }

        // 2. Check cache
        if let Some(cached) = self.cache.get(user_id) {
            if cached.inserted_at.elapsed().as_secs() < self.cache_ttl_secs {
                let balance = cached.balance_nanoerg;
                let box_count = cached.box_count;
                if balance < min_balance_nanoerg {
                    return Err(BalanceError::InsufficientBalance {
                        have_nanoerg: balance,
                        min_nanoerg: min_balance_nanoerg,
                    });
                }
                return Ok(BalanceCheckResult {
                    balance_nanoerg: balance,
                    staking_boxes_count: box_count,
                    is_free_tier: false,
                });
            }
        }

        // 3. Query Ergo node
        let (balance, box_count) = self.query_on_chain_balance(user_id).await?;

        // 4. Cache the result
        self.cache.insert(
            user_id.to_string(),
            CacheEntry {
                balance_nanoerg: balance,
                box_count,
                inserted_at: std::time::Instant::now(),
            },
        );

        // 5. Check against minimum
        if balance < min_balance_nanoerg {
            return Err(BalanceError::InsufficientBalance {
                have_nanoerg: balance,
                min_nanoerg: min_balance_nanoerg,
            });
        }

        Ok(BalanceCheckResult {
            balance_nanoerg: balance,
            staking_boxes_count: box_count,
            is_free_tier: false,
        })
    }

    /// Get the balance for a user (for the /v1/balance endpoint, no minimum check).
    pub async fn get_balance(&self, user_id: &str) -> Result<(u64, u32), BalanceError> {
        // Check cache first
        if let Some(cached) = self.cache.get(user_id) {
            if cached.inserted_at.elapsed().as_secs() < self.cache_ttl_secs {
                return Ok((cached.balance_nanoerg, cached.box_count));
            }
        }

        // Query on-chain
        let (balance, box_count) = self.query_on_chain_balance(user_id).await?;

        // Cache
        self.cache.insert(
            user_id.to_string(),
            CacheEntry {
                balance_nanoerg: balance,
                box_count,
                inserted_at: std::time::Instant::now(),
            },
        );

        Ok((balance, box_count))
    }

    /// Query the Ergo node for staking boxes belonging to a user.
    ///
    /// Strategy:
    /// - If staking_tree_bytes is configured, register a scan with CONTAINS
    ///   predicate on the tree bytes, then filter results for boxes where
    ///   R4 matches the user's PK.
    /// - Otherwise, use /utxo/withproof as a fallback with a generic filter.
    async fn query_on_chain_balance(
        &self,
        user_pk_hex: &str,
    ) -> Result<(u64, u32), BalanceError> {
        let base_url = &self.ergo_node_url;

        if self.staking_tree_bytes.is_empty() {
            // Placeholder mode: no staking contract configured.
            // Return 0 balance — users need to stake before using the service.
            debug!("No staking_tree_bytes configured — returning 0 balance (placeholder mode)");
            return Ok((0, 0));
        }

        // Register an EIP-1 scan for staking boxes
        let scan_body = serde_json::json!({
            "scanRequests": [{
                "boxSelector": {
                    "filter": {
                        "predicate": "CONTAINS",
                        "ErgoTree": self.staking_tree_bytes,
                        "parameters": []
                    }
                }
            }]
        });

        let scan_url = format!("{}/wallet/registerscan", base_url);
        let scan_resp = self
            .http_client
            .post(&scan_url)
            .json(&scan_body)
            .send()
            .await
            .map_err(|e| BalanceError::NodeUnavailable(e.to_string()))?;

        if !scan_resp.status().is_success() {
            let status = scan_resp.status();
            let body = scan_resp.text().await.unwrap_or_default();
            warn!(
                status = %status,
                body = %body,
                "Failed to register scan for balance check"
            );
            return Err(BalanceError::ScanRegistration(format!(
                "Node returned {}: {}",
                status, body
            )));
        }

        let scan_result: RegisterScanResponse = scan_resp
            .json()
            .await
            .map_err(|e| BalanceError::ScanRegistration(e.to_string()))?;

        let scan_id = scan_result.scan_id;

        // Query unspent boxes for this scan
        let boxes_url = format!("{}/wallet/boxes/unspent/{}", base_url, scan_id);
        let boxes_resp = self
            .http_client
            .get(&boxes_url)
            .send()
            .await
            .map_err(|e| BalanceError::NodeUnavailable(e.to_string()))?;

        if !boxes_resp.status().is_success() {
            let status = boxes_resp.status();
            let body = boxes_resp.text().await.unwrap_or_default();
            warn!(
                status = %status,
                body = %body,
                "Failed to query boxes for balance check"
            );
            return Err(BalanceError::BoxQuery(format!(
                "Node returned {}: {}",
                status, body
            )));
        }

        let boxes: Vec<StakingBox> = boxes_resp
            .json()
            .await
            .map_err(|e| BalanceError::BoxQuery(e.to_string()))?;

        // Filter boxes where R4 contains the user's public key
        let mut total_balance: u64 = 0;
        let mut matching_count: u32 = 0;

        for b in &boxes {
            if let Some(r4_value) = b.additional_registers.get("R4") {
                let pk = extract_pk_from_register(r4_value);
                if pk.as_deref() == Some(user_pk_hex) {
                    total_balance += b.value;
                    matching_count += 1;
                }
            }
        }

        debug!(
            user = %user_pk_hex,
            balance_nanoerg = total_balance,
            box_count = matching_count,
            "Balance check complete"
        );

        Ok((total_balance, matching_count))
    }

    /// Check if a user is still within the free tier limit.
    fn check_free_tier(&self, user_id: &str) -> bool {
        if self.free_tier_max == 0 {
            return false;
        }

        let counter = self
            .free_tier_usage
            .entry(user_id.to_string())
            .or_insert_with(|| AtomicU64::new(0));

        let current = counter.load(Ordering::Relaxed);
        if current < self.free_tier_max {
            counter.fetch_add(1, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// Get the number of free tier requests remaining for a user.
    #[allow(dead_code)]
    pub fn free_tier_remaining(&self, user_id: &str) -> u64 {
        if self.free_tier_max == 0 {
            return 0;
        }
        let used = self
            .free_tier_usage
            .get(user_id)
            .map(|c| c.load(Ordering::Relaxed))
            .unwrap_or(0);
        self.free_tier_max.saturating_sub(used)
    }

    /// Clear the free tier counter for a user (e.g., after staking).
    #[allow(dead_code)]
    pub fn reset_free_tier(&self, user_id: &str) {
        self.free_tier_usage.remove(user_id);
    }
}

/// Extract a public key hex string from a register value.
/// Handles both direct string and wrapped {"value": "..."} formats.
fn extract_pk_from_register(value: &serde_json::Value) -> Option<String> {
    if value.is_string() {
        value.as_str().map(String::from)
    } else {
        value.get("value").and_then(|inner| {
            if inner.is_string() {
                inner.as_str().map(String::from)
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> BalanceConfig {
        BalanceConfig {
            enabled: true,
            ergo_node_url: "http://127.0.0.1:9053".into(),
            min_balance_nanoerg: 1_000_000,
            cache_ttl_secs: 30,
            free_tier_enabled: true,
            free_tier_requests: 3,
            staking_tree_bytes: String::new(),
        }
    }

    #[test]
    fn test_balance_checker_creation() {
        let config = test_config();
        let checker = BalanceChecker::new(&config);
        assert_eq!(checker.ergo_node_url, "http://127.0.0.1:9053");
        assert_eq!(checker.cache_ttl_secs, 30);
        assert_eq!(checker.free_tier_max, 3);
    }

    #[test]
    fn test_free_tier_allows_up_to_limit() {
        let config = test_config();
        let checker = BalanceChecker::new(&config);

        // First 3 requests should be free
        assert!(checker.check_free_tier("user1"));
        assert!(checker.check_free_tier("user1"));
        assert!(checker.check_free_tier("user1"));

        // 4th should not be free
        assert!(!checker.check_free_tier("user1"));
    }

    #[test]
    fn test_free_tier_per_user() {
        let config = test_config();
        let checker = BalanceChecker::new(&config);

        assert!(checker.check_free_tier("user_a"));
        assert!(checker.check_free_tier("user_b"));
        assert!(checker.check_free_tier("user_a")); // user_a has 1 more free left
        assert!(checker.check_free_tier("user_a")); // user_a's last free
        assert!(!checker.check_free_tier("user_a")); // user_a exhausted
        assert!(checker.check_free_tier("user_b")); // user_b still has 2 more free
    }

    #[test]
    fn test_free_tier_disabled_when_max_is_zero() {
        let mut config = test_config();
        config.free_tier_requests = 0;
        let checker = BalanceChecker::new(&config);

        assert!(!checker.check_free_tier("user1"));
    }

    #[test]
    fn test_extract_pk_from_register_direct() {
        let value = serde_json::json!("deadbeef1234");
        assert_eq!(
            extract_pk_from_register(&value),
            Some("deadbeef1234".into())
        );
    }

    #[test]
    fn test_extract_pk_from_register_wrapped() {
        let value = serde_json::json!({"type": "SString", "value": "deadbeef1234"});
        assert_eq!(
            extract_pk_from_register(&value),
            Some("deadbeef1234".into())
        );
    }

    #[test]
    fn test_extract_pk_from_register_numeric() {
        let value = serde_json::json!(42);
        assert_eq!(extract_pk_from_register(&value), None);
    }

    #[test]
    fn test_extract_pk_from_register_null() {
        let value = serde_json::Value::Null;
        assert_eq!(extract_pk_from_register(&value), None);
    }

    #[tokio::test]
    async fn test_query_returns_zero_when_no_tree_configured() {
        let config = test_config();
        let checker = BalanceChecker::new(&config);
        let (balance, count) = checker.query_on_chain_balance("somepk").await.unwrap();
        assert_eq!(balance, 0);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_free_tier_remaining() {
        let config = test_config();
        let checker = BalanceChecker::new(&config);

        assert_eq!(checker.free_tier_remaining("new_user"), 3);
        checker.check_free_tier("new_user");
        assert_eq!(checker.free_tier_remaining("new_user"), 2);
        checker.check_free_tier("new_user");
        checker.check_free_tier("new_user");
        assert_eq!(checker.free_tier_remaining("new_user"), 0);
    }
}
