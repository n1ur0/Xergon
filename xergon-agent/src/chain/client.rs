//! Ergo node REST API client.
//!
//! Thin HTTP wrapper around the Ergo node's REST API endpoints.
//! All methods are read-only (Phase 2); transaction submission is included
//! as a stub for future use.

use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::Value;
use tracing::debug;

use crate::chain::types::RawBox;

/// Default Ergo node REST API address.
pub const DEFAULT_NODE_URL: &str = "http://127.0.0.1:9053";

/// HTTP client for the Ergo node REST API.
#[derive(Debug, Clone)]
pub struct ErgoNodeClient {
    base_url: String,
    http: Client,
}

impl ErgoNodeClient {
    /// Create a new client pointing at the given Ergo node URL.
    pub fn new(base_url: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    /// Create a client using the default Ergo node URL.
    pub fn default_client() -> Self {
        Self::new(DEFAULT_NODE_URL.to_string())
    }

    /// Get current block height from `/blocks/lastHeader`.
    pub async fn get_height(&self) -> Result<i32> {
        let url = format!("{}/blocks/lastHeader", self.base_url);
        let resp: Value = self
            .http
            .get(&url)
            .send()
            .await
            .context("Failed to request last header")?
            .error_for_status()
            .context("Ergo node returned error for lastHeader")?
            .json()
            .await
            .context("Failed to parse lastHeader response")?;

        resp["height"]
            .as_i64()
            .map(|h| h as i32)
            .context("Missing 'height' field in lastHeader response")
    }

    /// Check if the node is synced by comparing fullHeight to headersHeight.
    pub async fn is_synced(&self) -> Result<bool> {
        let info = self.get_node_info().await?;
        let best = info["headersHeight"].as_u64().unwrap_or(0);
        let full = info["fullHeight"].as_u64().unwrap_or(0);
        // Synced when full height is within 2 blocks of headers
        Ok(best > 0 && full > 0 && best.saturating_sub(full) <= 2)
    }

    /// Get node info (peer count, best height, etc.) from `/info`.
    pub async fn get_node_info(&self) -> Result<Value> {
        let url = format!("{}/info", self.base_url);
        let resp: Value = self
            .http
            .get(&url)
            .send()
            .await
            .context("Failed to request node info")?
            .error_for_status()
            .context("Ergo node returned error for /info")?
            .json()
            .await
            .context("Failed to parse node info response")?;
        Ok(resp)
    }

    /// Scan UTXO set for boxes containing a specific token ID.
    /// `GET /api/v1/boxes/unspent/byTokenId/{tokenId}`
    pub async fn get_boxes_by_token_id(&self, token_id: &str) -> Result<Vec<RawBox>> {
        let url = format!(
            "{}/api/v1/boxes/unspent/byTokenId/{}",
            self.base_url, token_id
        );
        debug!(%url, "Fetching boxes by token ID");

        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("Failed to request boxes by token ID")?
            .error_for_status()
            .context("Ergo node returned error for boxes by token ID")?
            .json::<Vec<RawBox>>()
            .await
            .context("Failed to parse boxes by token ID response")?;

        Ok(resp)
    }

    /// Get a specific box by ID.
    /// `GET /api/v1/boxes/{boxId}`
    pub async fn get_box(&self, box_id: &str) -> Result<RawBox> {
        let url = format!("{}/api/v1/boxes/{}", self.base_url, box_id);
        debug!(%url, "Fetching box by ID");

        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("Failed to request box by ID")?
            .error_for_status()
            .context("Ergo node returned error for box by ID")?
            .json::<RawBox>()
            .await
            .context("Failed to parse box response")?;

        Ok(resp)
    }

    /// Get unspent boxes for a given ergoTree.
    /// `GET /api/v1/boxes/unspent/byErgoTree/{ergoTree}`
    pub async fn get_boxes_by_ergo_tree(&self, ergo_tree: &str) -> Result<Vec<RawBox>> {
        let url = format!(
            "{}/api/v1/boxes/unspent/byErgoTree/{}",
            self.base_url, ergo_tree
        );
        debug!("Fetching boxes by ergoTree");

        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("Failed to request boxes by ergoTree")?
            .error_for_status()
            .context("Ergo node returned error for boxes by ergoTree")?
            .json::<Vec<RawBox>>()
            .await
            .context("Failed to parse boxes by ergoTree response")?;

        Ok(resp)
    }

    /// Submit a transaction to the node.
    /// `POST /api/v1/transactions`
    pub async fn submit_transaction(&self, tx_json: &str) -> Result<String> {
        let url = format!("{}/api/v1/transactions", self.base_url);
        debug!("Submitting transaction");

        let resp = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .body(tx_json.to_string())
            .send()
            .await
            .context("Failed to submit transaction")?
            .error_for_status()
            .context("Ergo node returned error for transaction submission")?
            .text()
            .await
            .context("Failed to read transaction submission response")?;

        Ok(resp)
    }

    /// Get a transaction by ID (to check confirmation status).
    /// `GET /api/v1/transactions/{txId}`
    pub async fn get_transaction(&self, tx_id: &str) -> Result<Value> {
        let url = format!("{}/api/v1/transactions/{}", self.base_url, tx_id);
        debug!(%url, "Fetching transaction");

        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("Failed to request transaction")?
            .error_for_status()
            .context("Ergo node returned error for transaction lookup")?
            .json::<Value>()
            .await
            .context("Failed to parse transaction response")?;

        Ok(resp)
    }

    // -----------------------------------------------------------------------
    // EIP-1 Wallet Scan API (UTXO scanning with custom tracking rules)
    // -----------------------------------------------------------------------

    /// Register a UTXO scan on the Ergo node wallet (EIP-1).
    ///
    /// `POST /wallet/registerscan`
    ///
    /// The `tracking_rule` must be a JSON object with at least:
    /// - `"predicate"`: one of `"contains"`, `"equals"`, `"and"`, `"or"`, etc.
    /// - `"value"`: hex-encoded bytes to scan for (for contains/equals predicates)
    ///
    /// Returns the scan ID assigned by the node.
    pub async fn register_scan(
        &self,
        scan_name: &str,
        tracking_rule: Value,
    ) -> Result<i32> {
        let url = format!("{}/wallet/registerscan", self.base_url);
        debug!(%url, scan_name, "Registering wallet scan");

        let body = serde_json::json!({
            "scanName": scan_name,
            "trackingRule": tracking_rule,
        });

        let resp: Value = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to register scan")?
            .error_for_status()
            .context("Ergo node returned error for registerScan")?
            .json()
            .await
            .context("Failed to parse registerScan response")?;

        resp["scanId"]
            .as_i64()
            .map(|id| id as i32)
            .context("Missing 'scanId' in registerScan response")
    }

    /// List all registered scans on the Ergo node wallet.
    ///
    /// `GET /wallet/registerscan/listAll`
    pub async fn list_scans(&self) -> Result<Vec<Value>> {
        let url = format!("{}/wallet/registerscan/listAll", self.base_url);
        debug!(%url, "Listing registered wallet scans");

        let resp: Value = self
            .http
            .get(&url)
            .send()
            .await
            .context("Failed to list scans")?
            .error_for_status()
            .context("Ergo node returned error for listAll scans")?
            .json()
            .await
            .context("Failed to parse listAll scans response")?;

        // Response can be an array or an object with a scans key
        match resp {
            Value::Array(arr) => Ok(arr),
            Value::Object(map) => {
                if let Some(arr) = map.get("scans").and_then(|v| v.as_array()) {
                    Ok(arr.clone())
                } else {
                    // Try any key that holds an array
                    map.values()
                        .find(|v| v.is_array())
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.clone())
                        .context("No scan array found in listAll response")
                }
            }
            _ => anyhow::bail!("Unexpected listAll scans response format"),
        }
    }

    /// Get unspent boxes tracked by a registered scan.
    ///
    /// `GET /wallet/boxes/unspent/{scanId}`
    pub async fn get_scan_boxes(&self, scan_id: i32) -> Result<Vec<RawBox>> {
        let url = format!("{}/wallet/boxes/unspent/{}", self.base_url, scan_id);
        debug!(%url, scan_id, "Fetching boxes for scan");

        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("Failed to get scan boxes")?
            .error_for_status()
            .context("Ergo node returned error for scan boxes")?
            .json::<Vec<RawBox>>()
            .await
            .context("Failed to parse scan boxes response")?;

        Ok(resp)
    }

    /// Deregister a previously registered scan.
    ///
    /// `DELETE /wallet/registerscan/{scanId}`
    pub async fn deregister_scan(&self, scan_id: i32) -> Result<()> {
        let url = format!("{}/wallet/registerscan/{}", self.base_url, scan_id);
        debug!(%url, scan_id, "Deregistering wallet scan");

        self.http
            .delete(&url)
            .send()
            .await
            .context("Failed to deregister scan")?
            .error_for_status()
            .context("Ergo node returned error for deregisterScan")?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Wallet Payment API (builds + signs + sends transactions via the node)
    // -----------------------------------------------------------------------

    /// Send a payment transaction using the Ergo node wallet.
    ///
    /// `POST /wallet/payment/send`
    ///
    /// This uses the node's wallet to build, sign, and broadcast a transaction
    /// in a single call. The wallet must be unlocked and funded.
    pub async fn wallet_payment_send(&self, request: &serde_json::Value) -> Result<String> {
        let url = format!("{}/wallet/payment/send", self.base_url);
        debug!("Sending wallet payment transaction");

        let resp: serde_json::Value = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await
            .context("Failed to send wallet payment")?
            .error_for_status()
            .context("Ergo node returned error for wallet payment")?
            .json()
            .await
            .context("Failed to parse wallet payment response")?;

        // The response contains the transaction ID
        resp["id"]
            .as_str()
            .map(String::from)
            .context("Missing 'id' in wallet payment response")
    }

    /// Check if the wallet is unlocked and has at least one box.
    pub async fn wallet_status(&self) -> Result<bool> {
        let url = format!("{}/wallet/status", self.base_url);
        debug!("Checking wallet status");

        let resp: serde_json::Value = self
            .http
            .get(&url)
            .send()
            .await
            .context("Failed to request wallet status")?
            .error_for_status()
            .context("Ergo node returned error for wallet status")?
            .json()
            .await
            .context("Failed to parse wallet status response")?;

        Ok(resp["isUnlocked"].as_bool().unwrap_or(false))
    }
}
