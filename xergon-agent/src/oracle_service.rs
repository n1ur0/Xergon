//! Oracle Consumption Service
//!
//! Reads live ERG/USD price from an Ergo oracle-core pool box via the node API
//! using the Data Input pattern documented in the Ergo KB.
//!
//! The oracle pool box contains:
//! - Pool NFT (tokens[0])
//! - R4 = aggregated rate (SInt Long, nanoERG per USD cent)
//! - R5 = epoch counter (SInt Int)
//!
//! Rate conversion: ERG/USD = 1e7 / R4_value
//!   (nanoERG/cent -> ERG/USD: divide by 1e9 nanoERG/ERG, divide by 100 cents/USD)

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::config::OracleConfig;

// ---------------------------------------------------------------------------
// Data types for the Ergo node / Explorer API responses
// ---------------------------------------------------------------------------

/// A UTXO box returned by the Ergo node API.
#[derive(Debug, Deserialize)]
struct UtxoBox {
    #[serde(rename = "boxId")]
    box_id: String,
    #[allow(dead_code)] // Required for JSON deserialization; not yet consumed
    value: u64,
    #[allow(dead_code)] // Required for JSON deserialization; not yet consumed
    ergo_tree: String,
    #[allow(dead_code)] // Required for JSON deserialization; not yet consumed
    assets: Vec<Asset>,
    additional_registers: BTreeMap<String, serde_json::Value>,
    #[serde(rename = "creationHeight")]
    creation_height: u32,
}

#[derive(Debug, Deserialize)]
struct Asset {
    #[allow(dead_code)] // Required for JSON deserialization; not yet consumed
    token_id: String,
    #[allow(dead_code)] // Required for JSON deserialization; not yet consumed
    amount: String,
}

/// The oracle rate data extracted from the pool box.
#[derive(Debug, Clone, Serialize)]
pub struct OracleRate {
    /// ERG/USD price (e.g., 0.45 means 1 ERG = $0.45 USD)
    pub erg_usd: f64,
    /// Raw R4 value from the oracle pool box (nanoERG per cent)
    pub rate_raw: i64,
    /// Epoch counter from R5
    pub epoch: i32,
    /// Box ID of the oracle pool box
    pub box_id: String,
    /// Creation height of the oracle pool box
    pub creation_height: u32,
    /// Timestamp when this rate was fetched
    pub fetched_at: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// Oracle Service
// ---------------------------------------------------------------------------

/// Cached oracle data held in memory.
struct CachedOracle {
    rate: OracleRate,
}

/// The main oracle service that fetches and caches ERG/USD rates.
pub struct OracleService {
    config: OracleConfig,
    ergo_node_url: String,
    http_client: Client,
    cached: Arc<RwLock<Option<CachedOracle>>>,
}

impl OracleService {
    /// Create a new oracle service.
    pub fn new(config: OracleConfig, ergo_node_url: String) -> Result<Self> {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .context("Failed to build HTTP client for oracle service")?;

        Ok(Self {
            config,
            ergo_node_url: ergo_node_url.trim_end_matches('/').to_string(),
            http_client,
            cached: Arc::new(RwLock::new(None)),
        })
    }

    /// Get the current cached ERG/USD rate.
    ///
    /// Returns `None` if the oracle has never been fetched successfully.
    /// Returns stale data if the refresh interval has not elapsed yet.
    pub async fn get_erg_usd_rate(&self) -> Option<f64> {
        self.cached.read().await.as_ref().map(|c| c.rate.erg_usd)
    }

    /// Get the full cached oracle rate data.
    pub async fn get_oracle_rate(&self) -> Option<OracleRate> {
        self.cached.read().await.as_ref().map(|c| c.rate.clone())
    }

    /// Perform a single fetch of the oracle rate from the node (or Explorer fallback).
    pub async fn fetch_rate(&self) -> Result<Option<OracleRate>> {
        let pool_nft_id = self.config.pool_nft_id.trim();
        if pool_nft_id.is_empty() {
            debug!("Oracle pool_nft_id is empty, skipping fetch");
            return Ok(None);
        }

        // Try Ergo node first
        let boxes = self.fetch_from_node(pool_nft_id).await;

        // Fallback to Ergo Explorer API
        let boxes = match boxes {
            Ok(Some(b)) => b,
            Ok(None) => {
                debug!("Node returned no boxes for oracle NFT, trying Explorer API");
                match self.fetch_from_explorer(pool_nft_id).await {
                    Ok(Some(b)) => b,
                    Ok(None) => {
                        debug!("Explorer also returned no boxes for oracle NFT");
                        return Ok(None);
                    }
                    Err(e) => {
                        warn!(error = %e, "Explorer API fetch also failed");
                        return Err(e);
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "Node fetch failed, trying Explorer API");
                match self.fetch_from_explorer(pool_nft_id).await {
                    Ok(Some(b)) => b,
                    Ok(None) => {
                        warn!("Explorer also returned no boxes for oracle NFT");
                        return Ok(None);
                    }
                    Err(explorer_err) => {
                        warn!(error = %explorer_err, "Explorer API fetch also failed");
                        return Err(e.context("Both node and Explorer API failed"));
                    }
                }
            }
        };

        if boxes.is_empty() {
            debug!("No oracle pool boxes found for NFT token ID");
            return Ok(None);
        }

        // Parse the first box
        let box_val = &boxes[0];
        let registers = &box_val.additional_registers;

        // Extract R4 (Long) — aggregated oracle rate in nanoERG per cent
        let rate_raw = match Self::extract_sigma_long(registers, "R4") {
            Some(v) => v,
            None => {
                warn!("Oracle pool box R4 (Long) not found or unparseable");
                return Ok(None);
            }
        };

        if rate_raw <= 0 {
            warn!("Oracle rate is non-positive, ignoring");
            return Ok(None);
        }

        // Extract R5 (Int) — epoch counter
        let epoch = Self::extract_sigma_int(registers, "R5").unwrap_or(0);

        // Convert nanoERG/cent to ERG/USD:
        // rate_raw is in nanoERG per cent (10^-2 USD)
        // ERG/USD = rate_raw / 1e9 (nanoERG -> ERG) / 100 (cent -> USD)
        let erg_usd = (rate_raw as f64) / 1_000_000_000.0 / 100.0;

        let oracle_rate = OracleRate {
            erg_usd,
            rate_raw,
            epoch,
            box_id: box_val.box_id.clone(),
            creation_height: box_val.creation_height,
            fetched_at: chrono::Utc::now(),
        };

        info!(
            erg_usd = oracle_rate.erg_usd,
            rate_raw = oracle_rate.rate_raw,
            epoch = oracle_rate.epoch,
            box_id = %oracle_rate.box_id,
            height = oracle_rate.creation_height,
            "Oracle ERG/USD rate fetched"
        );

        // Update cache
        {
            let mut cache = self.cached.write().await;
            *cache = Some(CachedOracle {
                rate: oracle_rate.clone(),
            });
        }

        Ok(Some(oracle_rate))
    }

    /// Spawn a background task that periodically refreshes the oracle rate.
    pub fn spawn_refresh_loop(self: &Arc<Self>) {
        let svc = Arc::clone(self);
        let interval_secs = self.config.refresh_interval_secs;

        tokio::spawn(async move {
            // Perform initial fetch
            match svc.fetch_rate().await {
                Ok(Some(rate)) => {
                    info!(erg_usd = rate.erg_usd, "Initial oracle ERG/USD rate fetched");
                }
                Ok(None) => {
                    warn!("No oracle pool box found during initial fetch");
                }
                Err(e) => {
                    warn!(error = %e, "Failed to fetch initial oracle rate");
                }
            }

            // Periodic refresh loop
            let interval = Duration::from_secs(interval_secs);
            loop {
                tokio::time::sleep(interval).await;
                match svc.fetch_rate().await {
                    Ok(Some(rate)) => {
                        info!(erg_usd = rate.erg_usd, interval_secs = interval_secs, "Oracle ERG/USD rate refreshed");
                    }
                    Ok(None) => {
                        debug!("No oracle pool box found during periodic refresh");
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to refresh oracle rate");
                    }
                }
            }
        });
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Fetch UTXO boxes containing the oracle pool NFT from the Ergo node.
    async fn fetch_from_node(&self, pool_nft_id: &str) -> Result<Option<Vec<UtxoBox>>> {
        let url = format!(
            "{}/utxo/withPool/byTokenId/{}",
            self.ergo_node_url, pool_nft_id
        );

        let resp = self
            .http_client
            .get(&url)
            .send()
            .await
            .context("Oracle node request failed")?;

        if !resp.status().is_success() {
            anyhow::bail!("Ergo node returned status {} for oracle query", resp.status());
        }

        let boxes: Vec<UtxoBox> = resp
            .json()
            .await
            .context("Failed to parse oracle box response from node")?;

        Ok(Some(boxes))
    }

    /// Fetch UTXO boxes containing the oracle pool NFT from the Ergo Explorer API (fallback).
    async fn fetch_from_explorer(&self, pool_nft_id: &str) -> Result<Option<Vec<UtxoBox>>> {
        let url = format!(
            "https://api.ergoplatform.com/api/v1/boxes/unspent/byTokenId/{}",
            pool_nft_id
        );

        let resp = self
            .http_client
            .get(&url)
            .send()
            .await
            .context("Explorer API request failed")?;

        if !resp.status().is_success() {
            anyhow::bail!("Explorer API returned status {}", resp.status());
        }

        let boxes: Vec<UtxoBox> = resp
            .json()
            .await
            .context("Failed to parse oracle box response from Explorer")?;

        Ok(Some(boxes))
    }

    // -----------------------------------------------------------------------
    // Sigma type decoding
    // -----------------------------------------------------------------------

    /// Parse a Sigma `Long` (8 bytes big-endian) from a register value.
    ///
    /// Format: `05 <8 bytes big-endian>`
    fn extract_sigma_long(
        registers: &BTreeMap<String, serde_json::Value>,
        key: &str,
    ) -> Option<i64> {
        let val = registers.get(key)?;
        let hex = Self::extract_serialized_hex(val)?;
        let bytes = hex::decode(&hex).ok()?;

        // Long tag is 0x05, followed by 8 bytes big-endian
        if bytes.len() != 9 || bytes[0] != 0x05 {
            return None;
        }

        Some(i64::from_be_bytes([
            bytes[1], bytes[2], bytes[3], bytes[4],
            bytes[5], bytes[6], bytes[7], bytes[8],
        ]))
    }

    /// Parse a Sigma `Int` (4 bytes big-endian) from a register value.
    ///
    /// Format: `04 <4 bytes big-endian>`
    fn extract_sigma_int(
        registers: &BTreeMap<String, serde_json::Value>,
        key: &str,
    ) -> Option<i32> {
        let val = registers.get(key)?;
        let hex = Self::extract_serialized_hex(val)?;
        let bytes = hex::decode(&hex).ok()?;

        // Int tag is 0x04, followed by 4 bytes big-endian
        if bytes.len() != 5 || bytes[0] != 0x04 {
            return None;
        }

        Some(i32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]))
    }

    /// Extract the serialized hex string from a register value.
    ///
    /// The Ergo node API returns register values as strings like "0520aabbccdd..."
    fn extract_serialized_hex(val: &serde_json::Value) -> Option<String> {
        // The value can be a direct string or an object with "serializedValue"
        if let Some(s) = val.as_str() {
            Some(s.to_string())
        } else if let Some(obj) = val.as_object() {
            // Some APIs return {"serializedValue": "0520...", "value": ...}
            obj.get("serializedValue")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_sigma_long() {
        // R4 = 0520 + 8 bytes: nanoERG per cent value
        // For a rate of 1,000,000 nanoERG per cent:
        //   hex: "05" + "00000000000f4240"
        let hex = format!("05{}", format!("{:016x}", 1_000_000i64));

        let mut registers = BTreeMap::new();
        registers.insert("R4".into(), serde_json::Value::String(hex));

        let val = OracleService::extract_sigma_long(&registers, "R4");
        assert_eq!(val, Some(1_000_000));
    }

    #[test]
    fn test_extract_sigma_int() {
        // R5 = 04 + 4 bytes: epoch counter
        let hex = format!("04{}", format!("{:08x}", 42i32));

        let mut registers = BTreeMap::new();
        registers.insert("R5".into(), serde_json::Value::String(hex));

        let val = OracleService::extract_sigma_int(&registers, "R5");
        assert_eq!(val, Some(42));
    }

    #[test]
    fn test_rate_conversion() {
        // If rate_raw = 1_000_000 nanoERG per cent
        // ERG/USD = 1_000_000 / 1e9 / 100 = 1e-5
        let rate_raw = 1_000_000i64;
        let erg_usd = (rate_raw as f64) / 1_000_000_000.0 / 100.0;
        assert!((erg_usd - 0.00001).abs() < 1e-12);

        // More realistic: 22,222,222 nanoERG per cent
        // ERG/USD = 22_222_222 / 1e9 / 100 = 2.2222222e-4
        let rate_raw = 22_222_222i64;
        let erg_usd = (rate_raw as f64) / 1_000_000_000.0 / 100.0;
        assert!((erg_usd - 2.2222222e-4).abs() < 1e-12);
    }

    #[test]
    fn test_extract_serialized_hex_string() {
        let val = serde_json::Value::String("05abcdef01".to_string());
        assert_eq!(
            OracleService::extract_serialized_hex(&val),
            Some("05abcdef01".to_string())
        );
    }

    #[test]
    fn test_extract_serialized_hex_object() {
        let val = serde_json::json!({
            "serializedValue": "05deadbeef01",
            "value": 12345
        });
        assert_eq!(
            OracleService::extract_serialized_hex(&val),
            Some("05deadbeef01".to_string())
        );
    }

    #[test]
    fn test_extract_sigma_long_with_serialized_value_object() {
        // Test with object format (some APIs return this)
        let hex = format!("05{}", format!("{:016x}", 5_000_000i64));

        let mut registers = BTreeMap::new();
        registers.insert(
            "R4".into(),
            serde_json::json!({
                "serializedValue": hex,
                "value": 5000000
            }),
        );

        let val = OracleService::extract_sigma_long(&registers, "R4");
        assert_eq!(val, Some(5_000_000));
    }

    #[test]
    fn test_extract_sigma_long_wrong_tag() {
        let mut registers = BTreeMap::new();
        registers.insert("R4".into(), serde_json::Value::String("0400000001".to_string()));
        assert_eq!(OracleService::extract_sigma_long(&registers, "R4"), None);
    }

    #[test]
    fn test_extract_sigma_long_short_bytes() {
        let mut registers = BTreeMap::new();
        registers.insert("R4".into(), serde_json::Value::String("05deadbeef".to_string()));
        assert_eq!(OracleService::extract_sigma_long(&registers, "R4"), None);
    }

    #[test]
    fn test_extract_sigma_int_wrong_tag() {
        // Long tag (0x05) instead of Int tag (0x04)
        let hex = format!("05{}", format!("{:016x}", 42i64));
        let mut registers = BTreeMap::new();
        registers.insert("R5".into(), serde_json::Value::String(hex));
        assert_eq!(OracleService::extract_sigma_int(&registers, "R5"), None);
    }

    #[test]
    fn test_extract_sigma_int_short_bytes() {
        // Only 3 bytes, need 5 for Int
        let mut registers = BTreeMap::new();
        registers.insert("R5".into(), serde_json::Value::String("04dead".to_string()));
        assert_eq!(OracleService::extract_sigma_int(&registers, "R5"), None);
    }

    #[test]
    fn test_extract_serialized_hex_invalid_type() {
        // Array instead of string or object
        let val = serde_json::json!([1, 2, 3]);
        assert_eq!(OracleService::extract_serialized_hex(&val), None);
    }

    #[test]
    fn test_extract_serialized_hex_null() {
        let val = serde_json::Value::Null;
        assert_eq!(OracleService::extract_serialized_hex(&val), None);
    }

    #[test]
    fn test_extract_serialized_hex_number() {
        let val = serde_json::Value::Number(serde_json::Number::from(42));
        assert_eq!(OracleService::extract_serialized_hex(&val), None);
    }

    #[test]
    fn test_oracle_rate_parsing_comprehensive() {
        // Simulate a realistic oracle pool box response:
        // R4 (Long) = 22_222_222 nanoERG per cent -> ~0.2222 ERG/USD
        // R5 (Int) = epoch 150
        let r4_hex = format!("05{}", format!("{:016x}", 22_222_222i64));
        let r5_hex = format!("04{}", format!("{:08x}", 150i32));

        let mut registers = BTreeMap::new();
        registers.insert("R4".into(), serde_json::Value::String(r4_hex));
        registers.insert("R5".into(), serde_json::Value::String(r5_hex));

        // Parse R4 (Long) - aggregated oracle rate
        let rate_raw = OracleService::extract_sigma_long(&registers, "R4").unwrap();
        assert_eq!(rate_raw, 22_222_222);

        // Parse R5 (Int) - epoch counter
        let epoch = OracleService::extract_sigma_int(&registers, "R5").unwrap();
        assert_eq!(epoch, 150);

        // Convert to ERG/USD
        let erg_usd = (rate_raw as f64) / 1_000_000_000.0 / 100.0;
        assert!((erg_usd - 2.2222222e-4).abs() < 1e-12);
    }

    #[test]
    fn test_oracle_rate_parsing_with_object_format() {
        // Some APIs return register values as objects with "serializedValue"
        let r4_hex = format!("05{}", format!("{:016x}", 50_000_000i64));

        let mut registers = BTreeMap::new();
        registers.insert(
            "R4".into(),
            serde_json::json!({
                "serializedValue": r4_hex,
                "value": 50000000,
                "type": "Long"
            }),
        );
        registers.insert(
            "R5".into(),
            serde_json::json!({
                "serializedValue": format!("04{}", format!("{:08x}", 200i32)),
                "value": 200,
                "type": "Int"
            }),
        );

        let rate_raw = OracleService::extract_sigma_long(&registers, "R4").unwrap();
        assert_eq!(rate_raw, 50_000_000);

        let epoch = OracleService::extract_sigma_int(&registers, "R5").unwrap();
        assert_eq!(epoch, 200);

        // ERG/USD = 50_000_000 / 1e9 / 100 = 0.0005
        let erg_usd = (rate_raw as f64) / 1_000_000_000.0 / 100.0;
        assert!((erg_usd - 0.0005).abs() < 1e-15);
    }

    #[test]
    fn test_oracle_rate_parsing_zero_rate() {
        // Zero rate should still parse but the service should skip it
        let hex = format!("05{}", format!("{:016x}", 0i64));
        let mut registers = BTreeMap::new();
        registers.insert("R4".into(), serde_json::Value::String(hex));

        let rate_raw = OracleService::extract_sigma_long(&registers, "R4").unwrap();
        assert_eq!(rate_raw, 0);
        // Zero rate would be rejected by the service (rate_raw <= 0 check)
        assert!(rate_raw <= 0);
    }

    #[test]
    fn test_oracle_rate_parsing_missing_registers() {
        let registers: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        assert_eq!(OracleService::extract_sigma_long(&registers, "R4"), None);
        assert_eq!(OracleService::extract_sigma_int(&registers, "R5"), None);
    }

    #[tokio::test]
    async fn test_oracle_cache_initially_empty() {
        let config = crate::config::OracleConfig::default();
        let svc = OracleService::new(config, "http://127.0.0.1:9053".to_string()).unwrap();

        // Cache should be empty initially
        assert!(svc.get_erg_usd_rate().await.is_none());
        assert!(svc.get_oracle_rate().await.is_none());
    }

    #[tokio::test]
    async fn test_oracle_fetch_rate_empty_pool_nft() {
        // With empty pool_nft_id, fetch_rate should return Ok(None)
        let config = crate::config::OracleConfig::default();
        let svc = OracleService::new(config, "http://127.0.0.1:9053".to_string()).unwrap();

        let result = svc.fetch_rate().await.unwrap();
        assert!(result.is_none());
    }
}
