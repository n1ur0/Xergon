//! Ergo chain-state provider discovery
//!
//! Periodically scans the Ergo blockchain for Provider Box instances
//! and exposes their metadata for syncing into the ProviderRegistry.
//!
//! This is best-effort: if the Ergo node is unreachable, the relay
//! continues operating with the static known_endpoints from config.

use chrono::Utc;
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};

use crate::config::ChainConfig;

/// Structured R6 payload: per-model pricing.
///
/// New R6 format:
/// ```json
/// {"models":[{"id":"llama-3.1-8b","price_per_1m_tokens":50000}]}
/// ```
#[derive(Debug, Deserialize)]
struct ModelsPayload {
    #[serde(default)]
    models: Vec<ModelEntry>,
}

/// A single model entry with optional per-1M-token pricing.
#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
    #[serde(default)]
    price_per_1m_tokens: u64,
}

/// A provider discovered from on-chain Provider Box state.
///
/// Register layout (from provider_box.ergo contract):
///   R4: GroupElement -- Provider public key
///   R5: Coll[Byte]  -- Endpoint URL (UTF-8)
///   R6: Coll[Byte]  -- Models served (JSON: structured object or plain array)
///   R7: Int         -- PoNW score (0-1000)
///   R8: Int         -- Last heartbeat height
///   R9: Coll[Byte]  -- Region (UTF-8)
#[allow(dead_code)] // Fields will be used for provider metadata display and routing
#[derive(Debug, Clone)]
pub struct ChainProvider {
    /// The Ergo box ID containing this provider registration
    pub box_id: String,
    /// Provider public key hex (R4 register, GroupElement)
    pub provider_pk: String,
    /// Provider endpoint URL (R5 register, Coll[Byte] UTF-8)
    pub endpoint: String,
    /// AI models this provider supports (R6 register, parsed from JSON)
    pub models: Vec<String>,
    /// Per-model pricing: model_id -> nanoERG per 1M tokens (R6 register, structured JSON)
    /// Models from old-format R6 (plain array) default to price 0 (free tier).
    pub model_pricing: HashMap<String, u64>,
    /// PoNW reputation score 0-1000 (R7 register, Int)
    pub pown_score: i32,
    /// Last heartbeat block height (R8 register, Int)
    pub last_heartbeat: i32,
    /// Provider region (R9 register, Coll[Byte] UTF-8)
    pub region: String,
    /// Price per 1M tokens in nanoERG (optional legacy pricing field)
    pub pricing_nanoerg_per_million_tokens: Option<u64>,
    /// NanoERG value in the box
    pub value_nanoerg: u64,
}

/// A GPU listing discovered from on-chain GPU Rental Listing Box state.
///
/// Register layout (from gpu_rental_listing.es contract):
///   R4: listing_id (String)
///   R5: provider_pk (String — Ergo address or public key of the GPU provider)
///   R6: gpu_type (String — e.g. "RTX-4090", "A100-80GB")
///   R7: gpu_specs (String — JSON-encoded GPU specifications)
///   R8: price_per_hour (Long — nanoERG per hour)
///   R9: region (String — e.g. "us-east", "eu-west")
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct GpuListing {
    /// The Ergo box ID containing this GPU listing
    pub box_id: String,
    /// Unique listing identifier (R4 register)
    pub listing_id: String,
    /// Provider public key / Ergo address (R5 register)
    pub provider_pk: String,
    /// GPU model/type (R6 register)
    pub gpu_type: String,
    /// JSON-encoded GPU specs: vram_gb, cuda_cores, etc. (R7 register)
    pub gpu_specs_json: String,
    /// Price per hour in nanoERG (R8 register)
    pub price_per_hour_nanoerg: u64,
    /// Provider region (R9 register)
    pub region: String,
    /// NanoERG value in the box (stake / deposit)
    pub value_nanoerg: u64,
}

/// Parsed GPU specs from R7 JSON.
#[allow(dead_code)]
#[derive(Debug, Clone, serde::Deserialize)]
pub struct GpuSpecs {
    #[serde(default)]
    pub vram_gb: u32,
    #[serde(default)]
    pub cuda_cores: u32,
    #[serde(default)]
    pub memory_bandwidth_gbps: f64,
    #[serde(default)]
    pub tdp_watts: u32,
    #[serde(default)]
    pub pcie_gen: String,
}

/// A GPU rental discovered from on-chain GPU Rental Box state.
///
/// Register layout (from gpu_rental.es contract):
///   R4: rental_id (String)
///   R5: renter_pk (String — public key of the renter)
///   R6: listing_box_id (String — the original listing box)
///   R7: provider_pk (String — public key of the GPU provider)
///   R8: deadline_height (Long — block height when rental expires)
///   R9: hours_rented (Long — number of hours rented)
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct GpuRental {
    /// The Ergo box ID containing this rental
    pub box_id: String,
    /// Unique rental identifier (R4 register)
    pub rental_id: String,
    /// Renter public key (R5 register)
    pub renter_pk: String,
    /// Original listing box ID (R6 register)
    pub listing_box_id: String,
    /// Provider public key (R7 register)
    pub provider_pk: String,
    /// Block height when rental expires (R8 register)
    pub deadline_height: u64,
    /// Number of hours rented (R9 register)
    pub hours_rented: u64,
    /// NanoERG value in the box (rental payment)
    pub value_nanoerg: u64,
}

/// Scans the Ergo blockchain for Provider Boxes.
pub struct ChainScanner {
    http_client: Client,
    config: Arc<ChainConfig>,
    /// Cached results from the last successful scan
    cached_providers: tokio::sync::RwLock<Vec<ChainProvider>>,
    /// Timestamp of last successful scan
    last_scan_at: tokio::sync::RwLock<chrono::DateTime<Utc>>,
}

// ── Ergo node response types ──────────────────────────────────────────

/// Response from /utxo/withproof for a single box (we only need registers).
#[derive(Debug, Deserialize)]
struct UtxoBox {
    #[serde(rename = "boxId")]
    box_id: String,
    #[serde(default)]
    value: u64,
    #[serde(default)]
    registers: std::collections::BTreeMap<String, serde_json::Value>,
}

/// Response from the /scan API (EIP-1 registered scans).
#[derive(Debug, Deserialize)]
struct ScanResponse {
    #[serde(default)]
    items: Vec<ScanItem>,
}

#[derive(Debug, Deserialize)]
struct ScanItem {
    #[serde(rename = "boxId")]
    box_id: String,
    #[serde(default)]
    value: u64,
    #[serde(rename = "additionalRegisters", default)]
    additional_registers: std::collections::BTreeMap<String, serde_json::Value>,
}

impl ChainScanner {
    /// Create a new ChainScanner.
    pub fn new(config: Arc<ChainConfig>) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .expect("Failed to build HTTP client for ChainScanner");

        Self {
            http_client,
            config,
            cached_providers: tokio::sync::RwLock::new(Vec::new()),
            last_scan_at: tokio::sync::RwLock::new(Utc::now()),
        }
    }

    /// Perform a scan of the Ergo blockchain for Provider Boxes.
    ///
    /// Uses EIP-1 registered scans with a CONTAINS predicate on the
    /// provider box ErgoTree bytes (hex-encoded in config).
    ///
    /// Falls back gracefully if the node is unreachable.
    pub async fn scan(&self) -> Vec<ChainProvider> {
        let base_url = self.config.ergo_node_url.trim_end_matches('/');

        let tree_hex = self.config.provider_tree_bytes.trim();
        let providers = if tree_hex.is_empty() {
            debug!("No provider_tree_bytes configured — skipping chain scan (placeholder mode)");
            Vec::new()
        } else {
            self.scan_with_predicate(base_url, tree_hex).await
        };

        // Update cache regardless of outcome
        {
            let mut cached = self.cached_providers.write().await;
            *cached = providers.clone();
        }
        {
            let mut last = self.last_scan_at.write().await;
            *last = Utc::now();
        }

        providers
    }

    /// Use EIP-1 registered scan with CONTAINS predicate.
    async fn scan_with_predicate(&self, base_url: &str, tree_hex: &str) -> Vec<ChainProvider> {
        // Build the scan request body:
        // POST /scan with a CONTAINS predicate on ErgoTree bytes
        let scan_body = serde_json::json!({
            "scanRequests": [{
                "boxSelector": {
                    "filter": {
                        "predicate": "CONTAINS",
                        " ErgoTree": tree_hex,
                        "parameters": []
                    }
                }
            }]
        });

        // Try EIP-1 registered scan endpoint first
        let url = format!("{}/scan", base_url);
        match self
            .http_client
            .post(&url)
            .json(&scan_body)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<ScanResponse>().await {
                    Ok(scan_resp) => {
                        let providers: Vec<ChainProvider> = scan_resp
                            .items
                            .into_iter()
                            .filter_map(|item| {
                                Self::parse_scan_item(&item).map(|mut p| {
                                    p.value_nanoerg = item.value;
                                    p
                                })
                            })
                            .collect();
                        debug!(
                            count = providers.len(),
                            "Chain scan found providers via /scan"
                        );
                        return providers;
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to parse /scan response");
                    }
                }
            }
            Ok(resp) => {
                warn!(status = %resp.status(), "/scan endpoint returned error");
            }
            Err(e) => {
                warn!(error = %e, "Failed to reach Ergo node /scan endpoint");
            }
        }

        // Fallback: try /utxo/withproof (less efficient but works without registered scan)
        self.fallback_utxo_scan(base_url, tree_hex).await
    }

    /// Fallback: enumerate UTXO set via /utxo/withproof with a filter.
    /// This is a placeholder — real implementation depends on deployed contracts.
    async fn fallback_utxo_scan(&self, base_url: &str, _tree_hex: &str) -> Vec<ChainProvider> {
        debug!("Fallback UTXO scan not yet implemented — returning empty results");
        // When contracts are deployed, this could use:
        // GET /utxo/withproof?ergoTree=<hex>
        // or GET /utxo/byErgoTree/<hex>
        let url = format!("{}/utxo/byErgoTree/{}", base_url, _tree_hex);
        match self.http_client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<Vec<UtxoBox>>().await {
                    Ok(boxes) => {
                        let providers: Vec<ChainProvider> = boxes
                            .into_iter()
                            .filter_map(|b| Self::parse_utxo_box(&b))
                            .collect();
                        debug!(
                            count = providers.len(),
                            "Chain scan found providers via /utxo/byErgoTree"
                        );
                        return providers;
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to parse /utxo/byErgoTree response");
                    }
                }
            }
            Ok(resp) => {
                debug!(status = %resp.status(), "/utxo/byErgoTree returned non-success (expected during placeholder phase)");
            }
            Err(e) => {
                warn!(error = %e, "Failed to reach Ergo node /utxo/byErgoTree endpoint");
            }
        }
        Vec::new()
    }

    /// Parse provider metadata from a ScanItem's additionalRegisters.
    ///
    /// Uses Sigma-type hex parsing matching the agent's specs.rs approach:
    /// - R4 (GroupElement): `0e 21 <33 bytes>` -> hex string of the 33-byte PK
    /// - R5 (Coll[Byte]):   `0e <vlb_len> <data>` -> UTF-8 endpoint URL
    /// - R6 (Coll[Byte]):   `0e <vlb_len> <data>` -> UTF-8 JSON array of models
    /// - R7 (Int):          `04 <4 bytes BE>`    -> PoNW score i32
    /// - R8 (Int):          `04 <4 bytes BE>`    -> last heartbeat height i32
    /// - R9 (Coll[Byte]):   `0e <vlb_len> <data>` -> UTF-8 region string
    fn parse_scan_item(item: &ScanItem) -> Option<ChainProvider> {
        let registers = &item.additional_registers;

        // R4: Provider public key (GroupElement)
        let provider_pk = Self::extract_group_element_hex(registers, "R4")?;

        // R5: Endpoint URL (Coll[Byte] UTF-8)
        let endpoint = Self::extract_coll_byte_string(registers, "R5")?;

        // Validate endpoint URL
        if url::Url::parse(&endpoint).is_err() {
            warn!(endpoint, "Invalid endpoint URL from chain — skipping");
            return None;
        }

        // R6: Models (Coll[Byte] JSON -- structured object or plain array)
        let models_json = Self::extract_coll_byte_string(registers, "R6").unwrap_or_else(|| "[]".to_string());
        let (models, model_pricing) = Self::parse_r6_models(&models_json);

        // R7: PoNW score (Int)
        let pown_score = Self::extract_sigma_int(registers, "R7").unwrap_or(0);

        // R8: Last heartbeat height (Int)
        let last_heartbeat = Self::extract_sigma_int(registers, "R8").unwrap_or(0);

        // R9: Region (Coll[Byte] UTF-8)
        let region = Self::extract_coll_byte_string(registers, "R9").unwrap_or_else(|| "unknown".to_string());

        Some(ChainProvider {
            box_id: item.box_id.clone(),
            provider_pk,
            endpoint,
            models,
            model_pricing,
            pown_score,
            last_heartbeat,
            region,
            pricing_nanoerg_per_million_tokens: None,
            value_nanoerg: item.value,
        })
    }

    /// Parse provider metadata from a UtxoBox's registers.
    ///
    /// Same register layout as parse_scan_item — see that method for docs.
    fn parse_utxo_box(box_val: &UtxoBox) -> Option<ChainProvider> {
        let registers = &box_val.registers;

        // R4: Provider public key (GroupElement)
        let provider_pk = Self::extract_group_element_hex(registers, "R4")?;

        // R5: Endpoint URL (Coll[Byte] UTF-8)
        let endpoint = Self::extract_coll_byte_string(registers, "R5")?;

        // Validate endpoint URL
        if url::Url::parse(&endpoint).is_err() {
            warn!(endpoint, "Invalid endpoint URL from chain — skipping");
            return None;
        }

        // R6: Models (Coll[Byte] JSON -- structured object or plain array)
        let models_json = Self::extract_coll_byte_string(registers, "R6").unwrap_or_else(|| "[]".to_string());
        let (models, model_pricing) = Self::parse_r6_models(&models_json);

        // R7: PoNW score (Int)
        let pown_score = Self::extract_sigma_int(registers, "R7").unwrap_or(0);

        // R8: Last heartbeat height (Int)
        let last_heartbeat = Self::extract_sigma_int(registers, "R8").unwrap_or(0);

        // R9: Region (Coll[Byte] UTF-8)
        let region = Self::extract_coll_byte_string(registers, "R9").unwrap_or_else(|| "unknown".to_string());

        Some(ChainProvider {
            box_id: box_val.box_id.clone(),
            provider_pk,
            endpoint,
            models,
            model_pricing,
            pown_score,
            last_heartbeat,
            region,
            pricing_nanoerg_per_million_tokens: None,
            value_nanoerg: box_val.value,
        })
    }

    /// Parse R6 models JSON with backward compatibility.
    ///
    /// Tries structured format first:
    ///   {"models":[{"id":"llama-3.1-8b","price_per_1m_tokens":50000}]}
    ///
    /// Falls back to plain array (old format):
    ///   ["llama-3.1-8b","qwen3.5-4b"]
    ///
    /// Returns (model_ids, model_id -> price_per_1m_tokens).
    /// Old-format models get price 0 (free tier).
    fn parse_r6_models(raw: &str) -> (Vec<String>, HashMap<String, u64>) {
        // Try structured format first
        if let Ok(payload) = serde_json::from_str::<ModelsPayload>(raw) {
            if !payload.models.is_empty() {
                let models: Vec<String> = payload.models.iter().map(|m| m.id.clone()).collect();
                let model_pricing: HashMap<String, u64> = payload
                    .models
                    .into_iter()
                    .map(|m| (m.id, m.price_per_1m_tokens))
                    .collect();
                return (models, model_pricing);
            }
        }

        // Fallback: try plain JSON array (old format)
        if let Ok(ids) = serde_json::from_str::<Vec<String>>(raw) {
            let model_pricing: HashMap<String, u64> = ids.iter().map(|id| (id.clone(), 0)).collect();
            return (ids, model_pricing);
        }

        // Last resort: comma-separated
        let models: Vec<String> = raw
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let model_pricing: HashMap<String, u64> = models.iter().map(|id| (id.clone(), 0)).collect();
        (models, model_pricing)
    }

    /// Extract a string value from an Ergo register (legacy format).
    /// Ergo registers are wrapped as: { "type": "SString", "value": "..." } or similar.
    /// NOTE: This is kept for GPU listing/rental parsing which uses the simpler format.
    fn extract_string_register(
        registers: &std::collections::BTreeMap<String, serde_json::Value>,
        key: &str,
    ) -> Option<String> {
        registers.get(key).and_then(|v| {
            // Could be a direct string, or an object with "value" field
            if v.is_string() {
                v.as_str().map(String::from)
            } else {
                v.get("value").and_then(|inner| {
                    if inner.is_string() {
                        inner.as_str().map(String::from)
                    } else {
                        None
                    }
                })
            }
        })
    }

    /// Extract a numeric value from an Ergo register (legacy format).
    /// NOTE: This is kept for GPU listing/rental parsing which uses the simpler format.
    fn extract_numeric_register(
        registers: &std::collections::BTreeMap<String, serde_json::Value>,
        key: &str,
    ) -> Option<f64> {
        registers.get(key).and_then(|v| {
            if v.is_number() {
                v.as_f64()
            } else {
                v.get("value").and_then(|inner| inner.as_f64())
            }
        })
    }

    // ── Sigma-type hex parsing (matches agent's scanner.rs approach) ─────

    /// Extract the serialized hex string from a register value.
    ///
    /// Handles both formats from the Ergo node API:
    /// 1. Compact: `"0e2102..."`  (raw hex string)
    /// 2. Expanded: `{"serializedValue": "0e2102...", "sigmaType": "...", "renderedValue": "..."}`
    fn extract_serialized_hex(val: &serde_json::Value) -> Option<String> {
        match val {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Object(map) => {
                map.get("serializedValue")?.as_str().map(String::from)
            }
            _ => None,
        }
    }

    /// Parse a Sigma `GroupElement` from a register value.
    ///
    /// Format: `0e 21 <33 bytes>` (Coll[Byte] of length 33)
    /// Returns the 33-byte hex representation of the compressed public key.
    fn extract_group_element_hex(
        registers: &std::collections::BTreeMap<String, serde_json::Value>,
        key: &str,
    ) -> Option<String> {
        let val = registers.get(key)?;
        let hex = Self::extract_serialized_hex(val)?;
        let bytes = hex::decode(&hex).ok()?;

        // GroupElement is stored as Coll[Byte] with tag 0x0e, length 0x21 (33), then 33 bytes.
        if bytes.len() < 35 || bytes[0] != 0x0e || bytes[1] != 0x21 {
            return None;
        }

        Some(hex::encode(&bytes[2..35]))
    }

    /// Parse a Sigma `Coll[Byte]` from a register value as a UTF-8 string.
    ///
    /// Format: `0e <vlb_length> <data_bytes...>`
    fn extract_coll_byte_string(
        registers: &std::collections::BTreeMap<String, serde_json::Value>,
        key: &str,
    ) -> Option<String> {
        let val = registers.get(key)?;
        let hex = Self::extract_serialized_hex(val)?;
        let bytes = hex::decode(&hex).ok()?;

        // Coll[Byte] starts with 0x0e, followed by a VLB-encoded length, then the data.
        if bytes.len() < 2 || bytes[0] != 0x0e {
            return None;
        }

        // Decode VLB (variable-length byte) length encoding used by Sigma.
        let (data_offset, data_len) = Self::decode_vlb(&bytes[1..])?;
        if bytes.len() < 1 + data_offset + data_len {
            return None;
        }

        String::from_utf8(bytes[1 + data_offset..1 + data_offset + data_len].to_vec()).ok()
    }

    /// Parse a Sigma `Int` (4 bytes big-endian) from a register value.
    ///
    /// Format: `04 <4 bytes big-endian>`
    fn extract_sigma_int(
        registers: &std::collections::BTreeMap<String, serde_json::Value>,
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

    /// Parse a Sigma `Long` (8 bytes big-endian) from a register value.
    ///
    /// Format: `05 <8 bytes big-endian>`
    fn extract_sigma_long(
        registers: &std::collections::BTreeMap<String, serde_json::Value>,
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

    /// Decode a Sigma VLB (variable-length byte) encoded integer.
    /// Returns (number_of_bytes_consumed, decoded_value).
    fn decode_vlb(data: &[u8]) -> Option<(usize, usize)> {
        if data.is_empty() {
            return None;
        }

        let first = data[0];
        let continuation = (first & 0x80) != 0;
        let value = (first & 0x7F) as usize;

        if !continuation {
            // Single byte: value is the length
            Some((1, value))
        } else if data.len() >= 2 {
            // Two bytes: first byte lower 7 bits << 7 | second byte
            let second = data[1] as usize;
            let combined = (value << 7) | second;
            Some((2, combined))
        } else {
            None
        }
    }

    /// Get the cached providers from the last scan.
    /// Returns the cached results without hitting the Ergo node.
    #[allow(dead_code)] // Public API for future use by handlers/dashboard
    pub async fn get_chain_providers(&self) -> Vec<ChainProvider> {
        self.cached_providers.read().await.clone()
    }

    /// Check if the Ergo node is reachable.
    pub async fn check_node_health(&self) -> bool {
        let url = format!(
            "{}/info",
            self.config.ergo_node_url.trim_end_matches('/')
        );
        self.http_client
            .get(&url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    // ── Oracle integration ────────────────────────────────────────────

    /// Fetch the current ERG/USD rate from an oracle pool box.
    ///
    /// Looks up the oracle pool by its NFT token ID, reads R4 (Long) which
    /// contains the aggregated rate in nanoERG per cent, and converts to
    /// ERG per USD: `rate = R4_value / 1e9 / 100`.
    ///
    /// Returns `None` if the oracle is not configured, the node is
    /// unreachable, or the pool box cannot be parsed.
    pub async fn fetch_oracle_rate(&self, pool_nft_id: &str) -> Result<Option<f64>, String> {
        let base_url = self.config.ergo_node_url.trim_end_matches('/');
        if pool_nft_id.trim().is_empty() {
            return Ok(None);
        }

        // Step 1: Find the oracle pool box by its NFT token ID.
        // The Ergo node /utxo/byTokenId/{tokenId} endpoint returns boxes
        // containing that token.
        let url = format!("{}/utxo/byTokenId/{}", base_url, pool_nft_id.trim());
        let resp = self
            .http_client
            .get(&url)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| format!("Oracle request failed: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("Oracle node returned status {}", resp.status()));
        }

        let boxes: Vec<UtxoBox> = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse oracle response: {}", e))?;

        if boxes.is_empty() {
            debug!("Oracle pool box not found for NFT token ID");
            return Ok(None);
        }

        // Use the first box (oracle pool box)
        let box_val = &boxes[0];
        let registers = &box_val.registers;

        // Step 2: Extract R4 (Long) — the aggregated oracle rate.
        // The value is in nanoERG per cent (10^-2 USD).
        let rate_raw = match Self::extract_sigma_long(registers, "R4") {
            Some(v) => v,
            None => {
                debug!("Oracle pool box R4 (Long) not found or unparseable");
                return Ok(None);
            }
        };

        if rate_raw <= 0 {
            debug!("Oracle rate is non-positive, ignoring");
            return Ok(None);
        }

        // Step 3: Convert nanoERG/cent to ERG/USD.
        // rate_raw is in nanoERG per cent (10^-2 USD).
        // ERG/USD = rate_raw / 1e9 (nanoERG -> ERG) / 100 (cent -> USD)
        let erg_per_usd = (rate_raw as f64) / 1_000_000_000.0 / 100.0;

        debug!(
            rate_raw,
            erg_per_usd = erg_per_usd,
            box_id = %box_val.box_id,
            "Oracle ERG/USD rate fetched"
        );

        Ok(Some(erg_per_usd))
    }

    // ── GPU listing scanning ──────────────────────────────────────────

    /// Scan the chain for GPU Rental Listing Boxes.
    ///
    /// Uses the same EIP-1 registered scan pattern as provider scanning,
    /// but with the GPU listing tree bytes.
    pub async fn scan_gpu_listings(&self) -> Vec<GpuListing> {
        let tree_hex = self.config.gpu_listing_tree_bytes.trim();
        if tree_hex.is_empty() {
            debug!("No gpu_listing_tree_bytes configured — skipping GPU listing scan");
            return Vec::new();
        }

        let base_url = self.config.ergo_node_url.trim_end_matches('/');
        let scan_body = serde_json::json!({
            "scanRequests": [{
                "boxSelector": {
                    "filter": {
                        "predicate": "CONTAINS",
                        "ErgoTree": tree_hex,
                        "parameters": []
                    }
                }
            }]
        });

        let url = format!("{}/scan", base_url);
        match self.http_client.post(&url).json(&scan_body).send().await {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<ScanResponse>().await {
                    Ok(scan_resp) => {
                        let listings: Vec<GpuListing> = scan_resp
                            .items
                            .into_iter()
                            .filter_map(|item| Self::parse_gpu_listing(&item))
                            .collect();
                        debug!(count = listings.len(), "GPU listing scan found listings");
                        return listings;
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to parse GPU listing /scan response");
                    }
                }
            }
            Ok(resp) => {
                debug!(status = %resp.status(), "GPU listing /scan returned error");
            }
            Err(e) => {
                warn!(error = %e, "Failed to reach Ergo node for GPU listing scan");
            }
        }

        // Fallback: try /utxo/byErgoTree
        self.fallback_utxo_scan_gpu_listings(base_url, tree_hex)
            .await
    }

    /// Fallback GPU listing scan via /utxo/byErgoTree.
    async fn fallback_utxo_scan_gpu_listings(
        &self,
        base_url: &str,
        tree_hex: &str,
    ) -> Vec<GpuListing> {
        let url = format!("{}/utxo/byErgoTree/{}", base_url, tree_hex);
        match self.http_client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<Vec<UtxoBox>>().await {
                    Ok(boxes) => {
                        let listings: Vec<GpuListing> = boxes
                            .into_iter()
                            .filter_map(|b| Self::parse_gpu_listing_utxo(&b))
                            .collect();
                        debug!(count = listings.len(), "GPU listing scan found listings via /utxo/byErgoTree");
                        return listings;
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to parse GPU listing /utxo/byErgoTree response");
                    }
                }
            }
            Ok(resp) => {
                debug!(status = %resp.status(), "GPU listing /utxo/byErgoTree returned non-success");
            }
            Err(e) => {
                warn!(error = %e, "Failed to reach Ergo node for GPU listing /utxo/byErgoTree");
            }
        }
        Vec::new()
    }

    /// Parse GPU listing from a ScanItem's additionalRegisters.
    fn parse_gpu_listing(item: &ScanItem) -> Option<GpuListing> {
        let registers = &item.additional_registers;

        let listing_id = Self::extract_string_register(registers, "R4")?;
        let provider_pk = Self::extract_string_register(registers, "R5")?;
        let gpu_type = Self::extract_string_register(registers, "R6")?;
        let gpu_specs_json = Self::extract_string_register(registers, "R7").unwrap_or_default();
        let price_per_hour_nanoerg = Self::extract_numeric_register(registers, "R8")
            .map(|v| v as u64)
            .unwrap_or(0);
        let region = Self::extract_string_register(registers, "R9").unwrap_or_default();

        Some(GpuListing {
            box_id: item.box_id.clone(),
            listing_id,
            provider_pk,
            gpu_type,
            gpu_specs_json,
            price_per_hour_nanoerg,
            region,
            value_nanoerg: item.value,
        })
    }

    /// Parse GPU listing from a UtxoBox's registers.
    fn parse_gpu_listing_utxo(box_val: &UtxoBox) -> Option<GpuListing> {
        let registers = &box_val.registers;

        let listing_id = Self::extract_string_register(registers, "R4")?;
        let provider_pk = Self::extract_string_register(registers, "R5")?;
        let gpu_type = Self::extract_string_register(registers, "R6")?;
        let gpu_specs_json = Self::extract_string_register(registers, "R7").unwrap_or_default();
        let price_per_hour_nanoerg = Self::extract_numeric_register(registers, "R8")
            .map(|v| v as u64)
            .unwrap_or(0);
        let region = Self::extract_string_register(registers, "R9").unwrap_or_default();

        Some(GpuListing {
            box_id: box_val.box_id.clone(),
            listing_id,
            provider_pk,
            gpu_type,
            gpu_specs_json,
            price_per_hour_nanoerg,
            region,
            value_nanoerg: box_val.value,
        })
    }

    // ── GPU rental scanning ────────────────────────────────────────────

    /// Scan the chain for GPU Rental Boxes (active rentals).
    ///
    /// Uses the GPU rental tree bytes from config.
    pub async fn scan_gpu_rentals(&self) -> Vec<GpuRental> {
        let tree_hex = self.config.gpu_rental_tree_bytes.trim();
        if tree_hex.is_empty() {
            debug!("No gpu_rental_tree_bytes configured — skipping GPU rental scan");
            return Vec::new();
        }

        let base_url = self.config.ergo_node_url.trim_end_matches('/');
        let scan_body = serde_json::json!({
            "scanRequests": [{
                "boxSelector": {
                    "filter": {
                        "predicate": "CONTAINS",
                        "ErgoTree": tree_hex,
                        "parameters": []
                    }
                }
            }]
        });

        let url = format!("{}/scan", base_url);
        match self.http_client.post(&url).json(&scan_body).send().await {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<ScanResponse>().await {
                    Ok(scan_resp) => {
                        let rentals: Vec<GpuRental> = scan_resp
                            .items
                            .into_iter()
                            .filter_map(|item| Self::parse_gpu_rental(&item))
                            .collect();
                        debug!(count = rentals.len(), "GPU rental scan found rentals");
                        return rentals;
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to parse GPU rental /scan response");
                    }
                }
            }
            Ok(resp) => {
                debug!(status = %resp.status(), "GPU rental /scan returned error");
            }
            Err(e) => {
                warn!(error = %e, "Failed to reach Ergo node for GPU rental scan");
            }
        }

        // Fallback
        self.fallback_utxo_scan_gpu_rentals(base_url, tree_hex)
            .await
    }

    /// Fallback GPU rental scan via /utxo/byErgoTree.
    async fn fallback_utxo_scan_gpu_rentals(
        &self,
        base_url: &str,
        tree_hex: &str,
    ) -> Vec<GpuRental> {
        let url = format!("{}/utxo/byErgoTree/{}", base_url, tree_hex);
        match self.http_client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<Vec<UtxoBox>>().await {
                    Ok(boxes) => {
                        let rentals: Vec<GpuRental> = boxes
                            .into_iter()
                            .filter_map(|b| Self::parse_gpu_rental_utxo(&b))
                            .collect();
                        debug!(count = rentals.len(), "GPU rental scan found rentals via /utxo/byErgoTree");
                        return rentals;
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to parse GPU rental /utxo/byErgoTree response");
                    }
                }
            }
            Ok(resp) => {
                debug!(status = %resp.status(), "GPU rental /utxo/byErgoTree returned non-success");
            }
            Err(e) => {
                warn!(error = %e, "Failed to reach Ergo node for GPU rental /utxo/byErgoTree");
            }
        }
        Vec::new()
    }

    /// Parse GPU rental from a ScanItem's additionalRegisters.
    fn parse_gpu_rental(item: &ScanItem) -> Option<GpuRental> {
        let registers = &item.additional_registers;

        let rental_id = Self::extract_string_register(registers, "R4")?;
        let renter_pk = Self::extract_string_register(registers, "R5")?;
        let listing_box_id = Self::extract_string_register(registers, "R6")?;
        let provider_pk = Self::extract_string_register(registers, "R7")?;
        let deadline_height = Self::extract_numeric_register(registers, "R8")
            .map(|v| v as u64)
            .unwrap_or(0);
        let hours_rented = Self::extract_numeric_register(registers, "R9")
            .map(|v| v as u64)
            .unwrap_or(0);

        Some(GpuRental {
            box_id: item.box_id.clone(),
            rental_id,
            renter_pk,
            listing_box_id,
            provider_pk,
            deadline_height,
            hours_rented,
            value_nanoerg: item.value,
        })
    }

    /// Parse GPU rental from a UtxoBox's registers.
    fn parse_gpu_rental_utxo(box_val: &UtxoBox) -> Option<GpuRental> {
        let registers = &box_val.registers;

        let rental_id = Self::extract_string_register(registers, "R4")?;
        let renter_pk = Self::extract_string_register(registers, "R5")?;
        let listing_box_id = Self::extract_string_register(registers, "R6")?;
        let provider_pk = Self::extract_string_register(registers, "R7")?;
        let deadline_height = Self::extract_numeric_register(registers, "R8")
            .map(|v| v as u64)
            .unwrap_or(0);
        let hours_rented = Self::extract_numeric_register(registers, "R9")
            .map(|v| v as u64)
            .unwrap_or(0);

        Some(GpuRental {
            box_id: box_val.box_id.clone(),
            rental_id,
            renter_pk,
            listing_box_id,
            provider_pk,
            deadline_height,
            hours_rented,
            value_nanoerg: box_val.value,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_string_register_direct() {
        let mut regs = std::collections::BTreeMap::new();
        regs.insert("R4".into(), serde_json::json!("provider-123"));
        assert_eq!(
            ChainScanner::extract_string_register(&regs, "R4"),
            Some("provider-123".into())
        );
    }

    #[test]
    fn test_extract_string_register_wrapped() {
        let mut regs = std::collections::BTreeMap::new();
        regs.insert(
            "R5".into(),
            serde_json::json!({"type": "SString", "value": "http://1.2.3.4:9099"}),
        );
        assert_eq!(
            ChainScanner::extract_string_register(&regs, "R5"),
            Some("http://1.2.3.4:9099".into())
        );
    }

    #[test]
    fn test_extract_string_register_missing() {
        let regs = std::collections::BTreeMap::new();
        assert_eq!(ChainScanner::extract_string_register(&regs, "R4"), None);
    }

    #[test]
    fn test_extract_numeric_register_direct() {
        let mut regs = std::collections::BTreeMap::new();
        regs.insert("R8".into(), serde_json::json!(42.5));
        assert_eq!(
            ChainScanner::extract_numeric_register(&regs, "R8"),
            Some(42.5)
        );
    }

    #[test]
    fn test_extract_numeric_register_wrapped() {
        let mut regs = std::collections::BTreeMap::new();
        regs.insert(
            "R8".into(),
            serde_json::json!({"type": "SInt", "value": 88.0}),
        );
        assert_eq!(
            ChainScanner::extract_numeric_register(&regs, "R8"),
            Some(88.0)
        );
    }

    #[test]
    fn test_chain_provider_from_scan_item() {
        let mut regs = std::collections::BTreeMap::new();
        // R4: Provider PK (GroupElement = Coll[Byte] of 33 bytes)
        // 0e = Coll[Byte] tag, 21 = 33 bytes, then 33 bytes of PK
        let pk_hex = "0e21".to_string() + &"02".repeat(1) + &"00".repeat(32);
        regs.insert("R4".into(), serde_json::json!(pk_hex));
        // R5: Endpoint (Coll[Byte] UTF-8)
        let endpoint_bytes = b"http://192.168.1.1:9099";
        let mut endpoint_hex = format!("0e{:02x}", endpoint_bytes.len());
        endpoint_hex.push_str(&hex::encode(endpoint_bytes));
        regs.insert("R5".into(), serde_json::json!(endpoint_hex));
        // R6: Models JSON (Coll[Byte] UTF-8)
        let models_str = r#"["llama-3","gpt-4"]"#;
        let mut models_hex = format!("0e{:02x}", models_str.len());
        models_hex.push_str(&hex::encode(models_str.as_bytes()));
        regs.insert("R6".into(), serde_json::json!(models_hex));
        // R7: PoNW score (Int = 04 + 4 bytes big-endian)
        regs.insert("R7".into(), serde_json::json!(hex::encode([0x04u8, 0, 0, 0, 75])));
        // R8: Heartbeat height (Int)
        regs.insert("R8".into(), serde_json::json!(hex::encode([0x04u8, 0x00, 0x00, 0x01, 0xea])));
        // R9: Region (Coll[Byte] UTF-8)
        let region_bytes = b"us-east";
        let mut region_hex = format!("0e{:02x}", region_bytes.len());
        region_hex.push_str(&hex::encode(region_bytes));
        regs.insert("R9".into(), serde_json::json!(region_hex));

        let item = ScanItem {
            box_id: "abc123".into(),
            value: 1_000_000_000,
            additional_registers: regs,
        };

        let provider = ChainScanner::parse_scan_item(&item).unwrap();
        assert_eq!(provider.box_id, "abc123");
        assert_eq!(provider.provider_pk, "02".to_owned() + &"00".repeat(32));
        assert_eq!(provider.endpoint, "http://192.168.1.1:9099");
        assert_eq!(provider.models, vec!["llama-3", "gpt-4"]);
        assert_eq!(provider.pown_score, 75);
        assert_eq!(provider.last_heartbeat, 490);
        assert_eq!(provider.region, "us-east");
        assert_eq!(provider.value_nanoerg, 1_000_000_000);
        assert_eq!(provider.pricing_nanoerg_per_million_tokens, None);
        // Old-format plain array -> all models priced at 0 (free tier)
        assert_eq!(provider.model_pricing.get("llama-3"), Some(&0));
        assert_eq!(provider.model_pricing.get("gpt-4"), Some(&0));
    }

    #[test]
    fn test_chain_provider_rejects_invalid_url() {
        let mut regs = std::collections::BTreeMap::new();
        // R4: Provider PK (GroupElement)
        let pk_hex = "0e21".to_string() + &"02".repeat(1) + &"00".repeat(32);
        regs.insert("R4".into(), serde_json::json!(pk_hex));
        // R5: Invalid endpoint (Coll[Byte] UTF-8)
        let bad_url = b"not-a-valid-url";
        let mut bad_hex = format!("0e{:02x}", bad_url.len());
        bad_hex.push_str(&hex::encode(bad_url));
        regs.insert("R5".into(), serde_json::json!(bad_hex));
        // R6-R9: minimal valid registers
        let empty_coll = "0e00";
        regs.insert("R6".into(), serde_json::json!(empty_coll));
        regs.insert("R7".into(), serde_json::json!(hex::encode([0x04u8, 0, 0, 0, 0])));
        regs.insert("R8".into(), serde_json::json!(hex::encode([0x04u8, 0, 0, 0, 0])));
        regs.insert("R9".into(), serde_json::json!(empty_coll));

        let item = ScanItem {
            box_id: "bad123".into(),
            value: 500_000_000,
            additional_registers: regs,
        };

        assert!(ChainScanner::parse_scan_item(&item).is_none());
    }

    #[tokio::test]
    async fn test_chain_scanner_returns_empty_when_no_tree_configured() {
        let config = ChainConfig::default();
        let scanner = ChainScanner::new(Arc::new(config));
        let providers = scanner.scan().await;
        assert!(providers.is_empty());
    }

    // ── Sigma-type parsing unit tests ──────────────────────────────────

    #[test]
    fn test_extract_group_element_hex() {
        let mut regs = std::collections::BTreeMap::new();
        // Valid GroupElement: 0e 21 <33 bytes>
        let pk_hex = "0e21".to_owned() + &"03".repeat(1) + &"ab".repeat(32);
        regs.insert("R4".into(), serde_json::json!(pk_hex));

        let result = ChainScanner::extract_group_element_hex(&regs, "R4").unwrap();
        assert_eq!(result, "03".to_owned() + &"ab".repeat(32));
        assert_eq!(result.len(), 66); // 33 bytes = 66 hex chars
    }

    #[test]
    fn test_extract_group_element_hex_expanded_format() {
        let mut regs = std::collections::BTreeMap::new();
        let pk_hex = "0e21".to_owned() + &"02".repeat(1) + &"ff".repeat(32);
        regs.insert("R4".into(), serde_json::json!({
            "serializedValue": pk_hex,
            "sigmaType": "SGroupElement",
            "renderedValue": "..."
        }));

        let result = ChainScanner::extract_group_element_hex(&regs, "R4").unwrap();
        assert_eq!(result, "02".to_owned() + &"ff".repeat(32));
    }

    #[test]
    fn test_extract_coll_byte_string() {
        let mut regs = std::collections::BTreeMap::new();
        let data = b"http://localhost:9099";
        let mut hex = format!("0e{:02x}", data.len());
        hex.push_str(&hex::encode(data));
        regs.insert("R5".into(), serde_json::json!(hex));

        let result = ChainScanner::extract_coll_byte_string(&regs, "R5").unwrap();
        assert_eq!(result, "http://localhost:9099");
    }

    #[test]
    fn test_extract_coll_byte_string_empty() {
        let mut regs = std::collections::BTreeMap::new();
        regs.insert("R6".into(), serde_json::json!("0e00"));

        let result = ChainScanner::extract_coll_byte_string(&regs, "R6").unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_extract_sigma_int() {
        let mut regs = std::collections::BTreeMap::new();
        // Int 750 = 0x000002EE
        regs.insert("R7".into(), serde_json::json!(hex::encode([0x04u8, 0, 0, 0x02, 0xEE])));

        let result = ChainScanner::extract_sigma_int(&regs, "R7").unwrap();
        assert_eq!(result, 750);
    }

    #[test]
    fn test_extract_sigma_int_zero() {
        let mut regs = std::collections::BTreeMap::new();
        regs.insert("R8".into(), serde_json::json!(hex::encode([0x04u8, 0, 0, 0, 0])));

        let result = ChainScanner::extract_sigma_int(&regs, "R8").unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn test_extract_sigma_int_negative() {
        let mut regs = std::collections::BTreeMap::new();
        // -1 in 4-byte big-endian two's complement
        regs.insert("R7".into(), serde_json::json!(hex::encode([0x04u8, 0xFF, 0xFF, 0xFF, 0xFF])));

        let result = ChainScanner::extract_sigma_int(&regs, "R7").unwrap();
        assert_eq!(result, -1);
    }

    #[test]
    fn test_decode_vlb_single_byte() {
        assert_eq!(ChainScanner::decode_vlb(&[0x15]), Some((1, 21)));
        assert_eq!(ChainScanner::decode_vlb(&[0x7F]), Some((1, 127)));
    }

    #[test]
    fn test_decode_vlb_two_bytes() {
        // 0x80 0x15 = (0 << 7) | 21 = 21
        assert_eq!(ChainScanner::decode_vlb(&[0x80, 0x15]), Some((2, 21)));
        // 0x81 0x00 = (1 << 7) | 0 = 128
        assert_eq!(ChainScanner::decode_vlb(&[0x81, 0x00]), Some((2, 128)));
    }

    #[test]
    fn test_chain_provider_models_json_parsing() {
        let mut regs = std::collections::BTreeMap::new();
        let pk_hex = "0e21".to_string() + &"02".repeat(1) + &"00".repeat(32);
        regs.insert("R4".into(), serde_json::json!(pk_hex));

        let endpoint_bytes = b"http://example.com:9099";
        let mut endpoint_hex = format!("0e{:02x}", endpoint_bytes.len());
        endpoint_hex.push_str(&hex::encode(endpoint_bytes));
        regs.insert("R5".into(), serde_json::json!(endpoint_hex));

        // R6: Valid JSON array
        let models_str = r#"["qwen3.5-4b","llama-3.1-8b"]"#;
        let mut models_hex = format!("0e{:02x}", models_str.len());
        models_hex.push_str(&hex::encode(models_str.as_bytes()));
        regs.insert("R6".into(), serde_json::json!(models_hex));

        regs.insert("R7".into(), serde_json::json!(hex::encode([0x04u8, 0, 0, 0, 100])));
        regs.insert("R8".into(), serde_json::json!(hex::encode([0x04u8, 0x00, 0x00, 0x01, 0xf4])));

        let region_bytes = b"eu-west";
        let mut region_hex = format!("0e{:02x}", region_bytes.len());
        region_hex.push_str(&hex::encode(region_bytes));
        regs.insert("R9".into(), serde_json::json!(region_hex));

        let item = ScanItem {
            box_id: "json-test".into(),
            value: 1_000_000_000,
            additional_registers: regs,
        };

        let provider = ChainScanner::parse_scan_item(&item).unwrap();
        assert_eq!(provider.models, vec!["qwen3.5-4b", "llama-3.1-8b"]);
        // Old-format plain array -> all models priced at 0 (free tier)
        assert_eq!(provider.model_pricing.get("qwen3.5-4b"), Some(&0));
        assert_eq!(provider.model_pricing.get("llama-3.1-8b"), Some(&0));
    }

    #[test]
    fn test_chain_provider_models_comma_fallback() {
        let mut regs = std::collections::BTreeMap::new();
        let pk_hex = "0e21".to_string() + &"02".repeat(1) + &"00".repeat(32);
        regs.insert("R4".into(), serde_json::json!(pk_hex));

        let endpoint_bytes = b"http://example.com:9099";
        let mut endpoint_hex = format!("0e{:02x}", endpoint_bytes.len());
        endpoint_hex.push_str(&hex::encode(endpoint_bytes));
        regs.insert("R5".into(), serde_json::json!(endpoint_hex));

        // R6: Not valid JSON — should fall back to comma-separated
        let models_str = "llama-3,mistral-7b,qwen-4b";
        let mut models_hex = format!("0e{:02x}", models_str.len());
        models_hex.push_str(&hex::encode(models_str.as_bytes()));
        regs.insert("R6".into(), serde_json::json!(models_hex));

        regs.insert("R7".into(), serde_json::json!(hex::encode([0x04u8, 0, 0, 0, 100])));
        regs.insert("R8".into(), serde_json::json!(hex::encode([0x04u8, 0x00, 0x00, 0x01, 0xf4])));
        regs.insert("R9".into(), serde_json::json!("0e00"));

        let item = ScanItem {
            box_id: "comma-test".into(),
            value: 1_000_000_000,
            additional_registers: regs,
        };

        let provider = ChainScanner::parse_scan_item(&item).unwrap();
        assert_eq!(provider.models, vec!["llama-3", "mistral-7b", "qwen-4b"]);
        // Comma-separated fallback -> all models priced at 0
        assert_eq!(provider.model_pricing.get("llama-3"), Some(&0));
        assert_eq!(provider.model_pricing.get("mistral-7b"), Some(&0));
    }

    #[test]
    fn test_chain_provider_structured_r6_pricing() {
        let mut regs = std::collections::BTreeMap::new();
        let pk_hex = "0e21".to_string() + &"02".repeat(1) + &"00".repeat(32);
        regs.insert("R4".into(), serde_json::json!(pk_hex));

        let endpoint_bytes = b"http://example.com:9099";
        let mut endpoint_hex = format!("0e{:02x}", endpoint_bytes.len());
        endpoint_hex.push_str(&hex::encode(endpoint_bytes));
        regs.insert("R5".into(), serde_json::json!(endpoint_hex));

        // R6: New structured format with per-model pricing
        let models_str = r#"{"models":[{"id":"llama-3.1-8b","price_per_1m_tokens":50000},{"id":"qwen3.5-4b","price_per_1m_tokens":30000}]}"#;
        let mut models_hex = format!("0e{:02x}", models_str.len());
        models_hex.push_str(&hex::encode(models_str.as_bytes()));
        regs.insert("R6".into(), serde_json::json!(models_hex));

        regs.insert("R7".into(), serde_json::json!(hex::encode([0x04u8, 0, 0, 0, 100])));
        regs.insert("R8".into(), serde_json::json!(hex::encode([0x04u8, 0x00, 0x00, 0x01, 0xf4])));

        let region_bytes = b"us-west";
        let mut region_hex = format!("0e{:02x}", region_bytes.len());
        region_hex.push_str(&hex::encode(region_bytes));
        regs.insert("R9".into(), serde_json::json!(region_hex));

        let item = ScanItem {
            box_id: "structured-test".into(),
            value: 1_000_000_000,
            additional_registers: regs,
        };

        let provider = ChainScanner::parse_scan_item(&item).unwrap();
        assert_eq!(provider.models, vec!["llama-3.1-8b", "qwen3.5-4b"]);
        assert_eq!(provider.model_pricing.get("llama-3.1-8b"), Some(&50000));
        assert_eq!(provider.model_pricing.get("qwen3.5-4b"), Some(&30000));
        assert_eq!(provider.model_pricing.len(), 2);
    }

    #[test]
    fn test_chain_provider_structured_r6_default_price() {
        let mut regs = std::collections::BTreeMap::new();
        let pk_hex = "0e21".to_string() + &"02".repeat(1) + &"00".repeat(32);
        regs.insert("R4".into(), serde_json::json!(pk_hex));

        let endpoint_bytes = b"http://example.com:9099";
        let mut endpoint_hex = format!("0e{:02x}", endpoint_bytes.len());
        endpoint_hex.push_str(&hex::encode(endpoint_bytes));
        regs.insert("R5".into(), serde_json::json!(endpoint_hex));

        // R6: Structured format but missing price_per_1m_tokens -> defaults to 0
        let models_str = r#"{"models":[{"id":"free-model"}]}"#;
        let mut models_hex = format!("0e{:02x}", models_str.len());
        models_hex.push_str(&hex::encode(models_str.as_bytes()));
        regs.insert("R6".into(), serde_json::json!(models_hex));

        regs.insert("R7".into(), serde_json::json!(hex::encode([0x04u8, 0, 0, 0, 0])));
        regs.insert("R8".into(), serde_json::json!(hex::encode([0x04u8, 0, 0, 0, 0])));
        regs.insert("R9".into(), serde_json::json!("0e00"));

        let item = ScanItem {
            box_id: "default-price-test".into(),
            value: 1_000_000_000,
            additional_registers: regs,
        };

        let provider = ChainScanner::parse_scan_item(&item).unwrap();
        assert_eq!(provider.models, vec!["free-model"]);
        assert_eq!(provider.model_pricing.get("free-model"), Some(&0));
    }

    #[test]
    fn test_extract_sigma_long() {
        let mut regs = std::collections::BTreeMap::new();
        // Long 5_000_000_000 = 0x000000012A05F200
        // This represents 5000 nanoERG per cent = 0.05 ERG/USD
        regs.insert("R4".into(), serde_json::json!(hex::encode([
            0x05u8, 0x00, 0x00, 0x00, 0x01, 0x2A, 0x05, 0xF2, 0x00
        ])));

        let result = ChainScanner::extract_sigma_long(&regs, "R4").unwrap();
        assert_eq!(result, 5_000_000_000);
    }

    #[test]
    fn test_extract_sigma_long_zero() {
        let mut regs = std::collections::BTreeMap::new();
        regs.insert("R4".into(), serde_json::json!(hex::encode([
            0x05u8, 0, 0, 0, 0, 0, 0, 0, 0
        ])));

        let result = ChainScanner::extract_sigma_long(&regs, "R4").unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn test_extract_sigma_long_missing() {
        let regs = std::collections::BTreeMap::new();
        assert!(ChainScanner::extract_sigma_long(&regs, "R4").is_none());
    }
}
