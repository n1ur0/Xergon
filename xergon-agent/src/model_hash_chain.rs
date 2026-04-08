//! Model Hash Chain — Immutable, append-only hash chain for model artifacts
//!
//! Records model artifacts (weights, configs, datasets, etc.) with cryptographic
//! hashes. Each entry references the previous hash, creating a tamper-evident
//! chain similar to a blockchain for model files.
//!
//! Features:
//! - Append-only hash chain with BLAKE3 hashing
//! - Tamper-evident: any modification breaks the chain
//! - Per-model artifact tracking and history
//! - Chain verification (full and range-based)
//! - Search by artifact hash
//! - Export chain as JSON
//!
//! REST endpoints:
//! - POST /v1/hash-chain/append          — Append new artifact entry
//! - GET  /v1/hash-chain/entry/{index}   — Get entry by index
//! - GET  /v1/hash-chain/model/{model_id}— Get model entries
//! - GET  /v1/hash-chain/tip             — Get chain tip
//! - GET  /v1/hash-chain/verify          — Verify chain integrity
//! - GET  /v1/hash-chain/stats           — Get chain statistics
//! - GET  /v1/hash-chain/search          — Search by artifact hash
//! - GET  /v1/hash-chain/history/{model_id} — Get model history

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tracing::{debug, info};

// ================================================================
// Types
// ================================================================

/// Types of artifacts that can be registered in the hash chain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
    ModelWeights,
    ModelConfig,
    Dataset,
    TrainingScript,
    EvaluationResult,
    FineTuneCheckpoint,
    QuantizedModel,
    DeploymentConfig,
}

impl std::fmt::Display for ArtifactType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArtifactType::ModelWeights => write!(f, "model_weights"),
            ArtifactType::ModelConfig => write!(f, "model_config"),
            ArtifactType::Dataset => write!(f, "dataset"),
            ArtifactType::TrainingScript => write!(f, "training_script"),
            ArtifactType::EvaluationResult => write!(f, "evaluation_result"),
            ArtifactType::FineTuneCheckpoint => write!(f, "fine_tune_checkpoint"),
            ArtifactType::QuantizedModel => write!(f, "quantized_model"),
            ArtifactType::DeploymentConfig => write!(f, "deployment_config"),
        }
    }
}

/// A single entry in the hash chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashChainEntry {
    /// Sequential index of this entry in the chain.
    pub index: u64,
    /// BLAKE3 hash of this entry's contents (hex-encoded).
    pub hash: String,
    /// BLAKE3 hash of the previous entry (hex-encoded). "0000..." for genesis.
    pub previous_hash: String,
    /// Identifier of the model this artifact belongs to.
    pub model_id: String,
    /// Type of artifact being registered.
    pub artifact_type: ArtifactType,
    /// BLAKE3 hash of the actual artifact content (hex-encoded).
    pub artifact_hash: String,
    /// Arbitrary key-value metadata about this artifact.
    pub metadata: HashMap<String, String>,
    /// Identity of the creator.
    pub created_by: String,
    /// Unix timestamp (seconds) when this entry was created.
    pub created_at: i64,
    /// Block height at time of registration (external reference).
    pub block_height: u64,
}

/// The current tip of the hash chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainTip {
    /// Index of the most recent entry.
    pub latest_index: u64,
    /// Hash of the most recent entry.
    pub latest_hash: String,
    /// Total number of entries in the chain.
    pub total_entries: u64,
    /// Hash of all concatenated entry hashes (chain fingerprint).
    pub chain_hash: String,
}

/// Result of a chain integrity verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainVerificationResult {
    /// Whether the chain is fully valid.
    pub valid: bool,
    /// Number of entries that were checked.
    pub entries_checked: u64,
    /// Index of the first invalid entry, if any.
    pub first_invalid_index: Option<u64>,
    /// Index where a hash mismatch was detected, if any.
    pub mismatch_at: Option<u64>,
}

/// A reference to an artifact in the chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRef {
    /// Chain index of the entry.
    pub chain_index: u64,
    /// Type of artifact.
    pub artifact_type: ArtifactType,
    /// BLAKE3 hash of the artifact content.
    pub artifact_hash: String,
    /// Unix timestamp when the artifact was registered.
    pub registered_at: i64,
}

/// Aggregated artifact record for a single model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelArtifactRecord {
    /// Model identifier.
    pub model_id: String,
    /// All artifact references for this model.
    pub artifacts: Vec<ArtifactRef>,
    /// Unix timestamp of the first artifact registration.
    pub first_registered: i64,
    /// Unix timestamp of the most recent artifact registration.
    pub last_updated: i64,
    /// Total number of artifacts for this model.
    pub total_artifacts: u64,
}

/// Aggregate statistics for the entire hash chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainStats {
    /// Total number of entries in the chain.
    pub total_entries: u64,
    /// Total number of distinct models.
    pub total_models: u64,
    /// Count of artifacts grouped by type.
    pub artifacts_by_type: HashMap<ArtifactType, u64>,
    /// Current chain length (same as total_entries).
    pub chain_length: u64,
    /// Unix timestamp of the oldest entry.
    pub oldest_entry: i64,
    /// Unix timestamp of the newest entry.
    pub newest_entry: i64,
}

// ================================================================
// Request / Response types for REST API
// ================================================================

#[derive(Debug, Deserialize)]
pub struct AppendRequest {
    pub model_id: String,
    pub artifact_type: ArtifactType,
    pub artifact_hash: String,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    pub created_by: String,
    #[serde(default)]
    pub block_height: u64,
}

#[derive(Debug, Serialize)]
pub struct AppendResponse {
    pub success: bool,
    pub entry: HashChainEntry,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub artifact_hash: String,
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub artifact_type: Option<ArtifactType>,
}

#[derive(Debug, Deserialize)]
pub struct ExportQuery {
    pub from: Option<u64>,
    pub to: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct VerifyRangeQuery {
    pub from: Option<u64>,
    pub to: Option<u64>,
}

// ================================================================
// ModelHashChain
// ================================================================

/// An immutable, append-only hash chain for model artifacts.
///
/// Each entry cryptographically references the previous entry via BLAKE3 hashes,
/// making the chain tamper-evident: modifying any entry invalidates all
/// subsequent entries.
pub struct ModelHashChain {
    /// All entries indexed by their sequential index.
    entries: DashMap<u64, HashChainEntry>,
    /// All entries indexed by model_id -> Vec<chain_index>.
    model_indices: DashMap<String, Vec<u64>>,
    /// Artifact hash -> Vec<chain_index> for fast lookup.
    artifact_index: DashMap<String, Vec<u64>>,
    /// Monotonically increasing entry counter.
    next_index: AtomicU64,
    /// Genesis hash (all-zeros hex string).
    genesis_hash: String,
    /// Mutex to serialize appends (chain integrity requires sequential ordering).
    append_lock: Mutex<()>,
}

impl ModelHashChain {
    /// Create a new, empty hash chain.
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
            model_indices: DashMap::new(),
            artifact_index: DashMap::new(),
            next_index: AtomicU64::new(0),
            genesis_hash: "0".repeat(64),
            append_lock: Mutex::new(()),
        }
    }

    // --------------------------------------------------------
    // Core operations
    // --------------------------------------------------------

    /// Append a new entry to the hash chain.
    ///
    /// Computes the BLAKE3 hash of the entry contents and links it
    /// to the previous entry. Returns the newly created entry.
    pub fn append_entry(
        &self,
        model_id: String,
        artifact_type: ArtifactType,
        artifact_hash: String,
        metadata: HashMap<String, String>,
        created_by: String,
        block_height: u64,
    ) -> HashChainEntry {
        // Serialize appends to maintain chain linkage integrity
        let _guard = self.append_lock.lock().unwrap();

        let index = self.next_index.fetch_add(1, Ordering::SeqCst);
        let previous_hash = if index == 0 {
            self.genesis_hash.clone()
        } else {
            // Get the previous entry's hash
            self.entries
                .get(&(index - 1))
                .map(|e| e.hash.clone())
                .unwrap_or_else(|| self.genesis_hash.clone())
        };

        let created_at = Utc::now().timestamp();

        // Compute the entry hash: BLAKE3(index || previous_hash || model_id || artifact_type || artifact_hash || metadata_json || created_by || created_at || block_height)
        let metadata_json = serde_json::to_string(&metadata).unwrap_or_default();
        let hash_input = format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}",
            index,
            previous_hash,
            model_id,
            artifact_type,
            artifact_hash,
            metadata_json,
            created_by,
            created_at,
            block_height
        );

        let hash = blake3::hash(hash_input.as_bytes()).to_hex().to_string();

        let entry = HashChainEntry {
            index,
            hash: hash.clone(),
            previous_hash,
            model_id: model_id.clone(),
            artifact_type: artifact_type.clone(),
            artifact_hash: artifact_hash.clone(),
            metadata: metadata.clone(),
            created_by,
            created_at,
            block_height,
        };

        // Store the entry
        self.entries.insert(index, entry.clone());

        // Update model index
        self.model_indices
            .entry(model_id.clone())
            .or_default()
            .push(index);

        // Update artifact index
        self.artifact_index
            .entry(artifact_hash.clone())
            .or_default()
            .push(index);

        debug!(
            index = index,
            model_id = %model_id,
            artifact_type = %artifact_type,
            "Appended new hash chain entry"
        );

        entry
    }

    /// Verify the integrity of the entire chain.
    ///
    /// Checks that each entry's `previous_hash` matches the hash of the
    /// preceding entry, and that each entry's own hash is correctly computed.
    pub fn verify_chain(&self) -> ChainVerificationResult {
        let total = self.next_index.load(Ordering::SeqCst);
        if total == 0 {
            return ChainVerificationResult {
                valid: true,
                entries_checked: 0,
                first_invalid_index: None,
                mismatch_at: None,
            };
        }

        self.verify_range(0, total - 1)
    }

    /// Verify a range of entries in the chain.
    ///
    /// Checks hash linkage within the specified range (inclusive).
    pub fn verify_range(&self, from_index: u64, to_index: u64) -> ChainVerificationResult {
        let total = self.next_index.load(Ordering::SeqCst);
        if total == 0 {
            return ChainVerificationResult {
                valid: true,
                entries_checked: 0,
                first_invalid_index: None,
                mismatch_at: None,
            };
        }

        let from = from_index.min(total - 1);
        let to = to_index.min(total - 1);

        let mut entries_checked = 0u64;
        let mut first_invalid_index: Option<u64> = None;
        let mut mismatch_at: Option<u64> = None;

        for idx in from..=to {
            if let Some(entry) = self.entries.get(&idx) {
                // Verify the entry's hash is correctly computed
                let metadata_json =
                    serde_json::to_string(&entry.metadata).unwrap_or_default();
                let hash_input = format!(
                    "{}|{}|{}|{}|{}|{}|{}|{}|{}",
                    entry.index,
                    entry.previous_hash,
                    entry.model_id,
                    entry.artifact_type,
                    entry.artifact_hash,
                    metadata_json,
                    entry.created_by,
                    entry.created_at,
                    entry.block_height
                );
                let computed_hash = blake3::hash(hash_input.as_bytes()).to_hex().to_string();

                if computed_hash != entry.hash {
                    if first_invalid_index.is_none() {
                        first_invalid_index = Some(idx);
                        mismatch_at = Some(idx);
                    }
                }

                // Verify previous_hash linkage (skip genesis)
                if idx > 0 {
                    if let Some(prev_entry) = self.entries.get(&(idx - 1)) {
                        if entry.previous_hash != prev_entry.hash {
                            if first_invalid_index.is_none() {
                                first_invalid_index = Some(idx);
                                mismatch_at = Some(idx);
                            }
                        }
                    }
                } else {
                    // Genesis: previous_hash should be the genesis hash
                    if entry.previous_hash != self.genesis_hash {
                        if first_invalid_index.is_none() {
                            first_invalid_index = Some(idx);
                            mismatch_at = Some(idx);
                        }
                    }
                }
                entries_checked += 1;
            } else {
                // Missing entry
                if first_invalid_index.is_none() {
                    first_invalid_index = Some(idx);
                    mismatch_at = Some(idx);
                }
                entries_checked += 1;
            }
        }

        ChainVerificationResult {
            valid: first_invalid_index.is_none(),
            entries_checked,
            first_invalid_index,
            mismatch_at,
        }
    }

    /// Get a single entry by its chain index.
    pub fn get_entry(&self, index: u64) -> Option<HashChainEntry> {
        self.entries.get(&index).map(|e| e.clone())
    }

    /// Get all entries for a given model.
    pub fn get_entries(&self, model_id: &str) -> Vec<HashChainEntry> {
        let indices = self
            .model_indices
            .get(model_id)
            .map(|v| v.clone())
            .unwrap_or_default();

        let mut entries = Vec::with_capacity(indices.len());
        for idx in indices {
            if let Some(entry) = self.entries.get(&idx) {
                entries.push(entry.clone());
            }
        }
        entries.sort_by_key(|e| e.index);
        entries
    }

    /// Get the aggregated artifact record for a model.
    pub fn get_model_record(&self, model_id: &str) -> Option<ModelArtifactRecord> {
        let entries = self.get_entries(model_id);
        if entries.is_empty() {
            return None;
        }

        let mut first_registered = i64::MAX;
        let mut last_updated = i64::MIN;
        let mut artifacts = Vec::with_capacity(entries.len());

        for entry in &entries {
            first_registered = first_registered.min(entry.created_at);
            last_updated = last_updated.max(entry.created_at);
            artifacts.push(ArtifactRef {
                chain_index: entry.index,
                artifact_type: entry.artifact_type.clone(),
                artifact_hash: entry.artifact_hash.clone(),
                registered_at: entry.created_at,
            });
        }

        Some(ModelArtifactRecord {
            model_id: model_id.to_string(),
            artifacts,
            first_registered,
            last_updated,
            total_artifacts: entries.len() as u64,
        })
    }

    /// Get the current chain tip.
    pub fn get_tip(&self) -> ChainTip {
        let total = self.next_index.load(Ordering::SeqCst);
        if total == 0 {
            return ChainTip {
                latest_index: 0,
                latest_hash: self.genesis_hash.clone(),
                total_entries: 0,
                chain_hash: self.genesis_hash.clone(),
            };
        }

        let latest_index = total - 1;
        let latest_hash = self
            .entries
            .get(&latest_index)
            .map(|e| e.hash.clone())
            .unwrap_or_default();

        // Compute chain hash: BLAKE3 of all entry hashes concatenated
        let mut all_hashes = String::new();
        for idx in 0..total {
            if let Some(entry) = self.entries.get(&idx) {
                all_hashes.push_str(&entry.hash);
            }
        }
        let chain_hash = blake3::hash(all_hashes.as_bytes()).to_hex().to_string();

        ChainTip {
            latest_index,
            latest_hash,
            total_entries: total,
            chain_hash,
        }
    }

    /// Get aggregate statistics for the chain.
    pub fn get_stats(&self) -> ChainStats {
        let total = self.next_index.load(Ordering::SeqCst);
        let mut artifacts_by_type: HashMap<ArtifactType, u64> = HashMap::new();
        let mut oldest_entry = i64::MAX;
        let mut newest_entry = i64::MIN;

        for idx in 0..total {
            if let Some(entry) = self.entries.get(&idx) {
                *artifacts_by_type.entry(entry.artifact_type.clone()).or_insert(0) += 1;
                oldest_entry = oldest_entry.min(entry.created_at);
                newest_entry = newest_entry.max(entry.created_at);
            }
        }

        if oldest_entry == i64::MAX {
            oldest_entry = 0;
        }
        if newest_entry == i64::MIN {
            newest_entry = 0;
        }

        let total_models = self.model_indices.len() as u64;

        ChainStats {
            total_entries: total,
            total_models,
            artifacts_by_type,
            chain_length: total,
            oldest_entry,
            newest_entry,
        }
    }

    /// Search for entries by artifact hash.
    pub fn search_by_hash(&self, artifact_hash: &str) -> Vec<HashChainEntry> {
        let indices = self
            .artifact_index
            .get(artifact_hash)
            .map(|v| v.clone())
            .unwrap_or_default();

        let mut entries = Vec::with_capacity(indices.len());
        for idx in indices {
            if let Some(entry) = self.entries.get(&idx) {
                entries.push(entry.clone());
            }
        }
        entries.sort_by_key(|e| e.index);
        entries
    }

    /// Get the artifact history for a model, optionally filtered by artifact type.
    pub fn get_history(
        &self,
        model_id: &str,
        artifact_type_filter: Option<&ArtifactType>,
    ) -> Vec<ArtifactRef> {
        let entries = self.get_entries(model_id);
        let mut refs = Vec::with_capacity(entries.len());

        for entry in entries {
            if let Some(filter) = artifact_type_filter {
                if &entry.artifact_type != filter {
                    continue;
                }
            }
            refs.push(ArtifactRef {
                chain_index: entry.index,
                artifact_type: entry.artifact_type,
                artifact_hash: entry.artifact_hash,
                registered_at: entry.created_at,
            });
        }

        refs
    }

    /// Export chain entries as a JSON string, optionally limited by range.
    pub fn export_chain(&self, from: Option<u64>, to: Option<u64>) -> Vec<HashChainEntry> {
        let total = self.next_index.load(Ordering::SeqCst);
        if total == 0 {
            return Vec::new();
        }

        let start = from.unwrap_or(0).min(total - 1);
        let end = to.unwrap_or(total - 1).min(total - 1);

        let mut entries = Vec::new();
        for idx in start..=end {
            if let Some(entry) = self.entries.get(&idx) {
                entries.push(entry.clone());
            }
        }
        entries
    }

    /// Get the total number of entries in the chain.
    pub fn len(&self) -> u64 {
        self.next_index.load(Ordering::SeqCst)
    }

    /// Check if the chain is empty.
    pub fn is_empty(&self) -> bool {
        self.next_index.load(Ordering::SeqCst) == 0
    }
}

impl Default for ModelHashChain {
    fn default() -> Self {
        Self::new()
    }
}

// ================================================================
// REST Handlers
// ================================================================

/// Append a new artifact entry to the hash chain.
async fn append_handler(
    State(chain): State<Arc<ModelHashChain>>,
    Json(req): Json<AppendRequest>,
) -> Result<Json<AppendResponse>, StatusCode> {
    let entry = chain.append_entry(
        req.model_id,
        req.artifact_type,
        req.artifact_hash,
        req.metadata,
        req.created_by,
        req.block_height,
    );

    info!(
        index = entry.index,
        model_id = %entry.model_id,
        "Appended new hash chain entry via API"
    );

    Ok(Json(AppendResponse {
        success: true,
        entry,
    }))
}

/// Get a single chain entry by index.
async fn get_entry_handler(
    State(chain): State<Arc<ModelHashChain>>,
    Path(index): Path<u64>,
) -> Result<Json<HashChainEntry>, StatusCode> {
    chain
        .get_entry(index)
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// Get all entries for a model.
async fn get_model_entries_handler(
    State(chain): State<Arc<ModelHashChain>>,
    Path(model_id): Path<String>,
) -> Result<Json<Vec<HashChainEntry>>, StatusCode> {
    let entries = chain.get_entries(&model_id);
    Ok(Json(entries))
}

/// Get the current chain tip.
async fn get_tip_handler(
    State(chain): State<Arc<ModelHashChain>>,
) -> Json<ChainTip> {
    Json(chain.get_tip())
}

/// Verify chain integrity.
async fn verify_handler(
    State(chain): State<Arc<ModelHashChain>>,
    Query(params): Query<VerifyRangeQuery>,
) -> Json<ChainVerificationResult> {
    let result = match (params.from, params.to) {
        (Some(from), Some(to)) => chain.verify_range(from, to),
        _ => chain.verify_chain(),
    };
    Json(result)
}

/// Get chain statistics.
async fn get_stats_handler(
    State(chain): State<Arc<ModelHashChain>>,
) -> Json<ChainStats> {
    Json(chain.get_stats())
}

/// Search entries by artifact hash.
async fn search_handler(
    State(chain): State<Arc<ModelHashChain>>,
    Query(params): Query<SearchQuery>,
) -> Json<Vec<HashChainEntry>> {
    let entries = chain.search_by_hash(&params.artifact_hash);
    Json(entries)
}

/// Get model artifact history.
async fn get_history_handler(
    State(chain): State<Arc<ModelHashChain>>,
    Path(model_id): Path<String>,
    Query(params): Query<HistoryQuery>,
) -> Json<Vec<ArtifactRef>> {
    let refs = chain.get_history(&model_id, params.artifact_type.as_ref());
    Json(refs)
}

// ================================================================
// Router
// ================================================================

/// Build the model hash chain router.
pub fn build_hash_chain_router(state: Arc<ModelHashChain>) -> axum::Router {
    use axum::routing::{get, post};

    axum::Router::new()
        .route("/v1/hash-chain/append", post(append_handler))
        .route("/v1/hash-chain/entry/{index}", get(get_entry_handler))
        .route("/v1/hash-chain/model/{model_id}", get(get_model_entries_handler))
        .route("/v1/hash-chain/tip", get(get_tip_handler))
        .route("/v1/hash-chain/verify", get(verify_handler))
        .route("/v1/hash-chain/stats", get(get_stats_handler))
        .route("/v1/hash-chain/search", get(search_handler))
        .route("/v1/hash-chain/history/{model_id}", get(get_history_handler))
        .with_state(state)
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_chain() -> Arc<ModelHashChain> {
        Arc::new(ModelHashChain::new())
    }

    fn blake3_of(data: &str) -> String {
        blake3::hash(data.as_bytes()).to_hex().to_string()
    }

    fn default_metadata() -> HashMap<String, String> {
        HashMap::new()
    }

    // --------------------------------------------------------
    // test_genesis_entry
    // --------------------------------------------------------
    #[test]
    fn test_genesis_entry() {
        let chain = make_chain();
        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);

        let tip = chain.get_tip();
        assert_eq!(tip.total_entries, 0);
        assert_eq!(tip.latest_index, 0);
        assert_eq!(tip.latest_hash, "0".repeat(64));

        // Verify empty chain is valid
        let result = chain.verify_chain();
        assert!(result.valid);
        assert_eq!(result.entries_checked, 0);
    }

    // --------------------------------------------------------
    // test_append_entry
    // --------------------------------------------------------
    #[test]
    fn test_append_entry() {
        let chain = make_chain();

        let artifact_hash = blake3_of("model-weights-v1");
        let entry = chain.append_entry(
            "model-001".to_string(),
            ArtifactType::ModelWeights,
            artifact_hash,
            default_metadata(),
            "alice".to_string(),
            100,
        );

        assert_eq!(entry.index, 0);
        assert_eq!(entry.model_id, "model-001");
        assert_eq!(entry.artifact_type, ArtifactType::ModelWeights);
        assert_eq!(entry.previous_hash, "0".repeat(64));
        assert!(!entry.hash.is_empty());
        assert_eq!(chain.len(), 1);

        // Second entry
        let entry2 = chain.append_entry(
            "model-001".to_string(),
            ArtifactType::ModelConfig,
            blake3_of("config-v1"),
            default_metadata(),
            "bob".to_string(),
            101,
        );

        assert_eq!(entry2.index, 1);
        assert_eq!(entry2.previous_hash, entry.hash);
        assert_eq!(chain.len(), 2);
    }

    // --------------------------------------------------------
    // test_chain_hash_integrity
    // --------------------------------------------------------
    #[test]
    fn test_chain_hash_integrity() {
        let chain = make_chain();

        let e1 = chain.append_entry(
            "m1".to_string(),
            ArtifactType::ModelWeights,
            blake3_of("w1"),
            default_metadata(),
            "alice".to_string(),
            1,
        );
        let e2 = chain.append_entry(
            "m1".to_string(),
            ArtifactType::ModelConfig,
            blake3_of("c1"),
            default_metadata(),
            "alice".to_string(),
            2,
        );
        let e3 = chain.append_entry(
            "m2".to_string(),
            ArtifactType::Dataset,
            blake3_of("d1"),
            default_metadata(),
            "bob".to_string(),
            3,
        );

        // Verify linkage
        assert_eq!(e1.previous_hash, "0".repeat(64));
        assert_eq!(e2.previous_hash, e1.hash);
        assert_eq!(e3.previous_hash, e2.hash);

        // Full chain verification
        let result = chain.verify_chain();
        assert!(result.valid);
        assert_eq!(result.entries_checked, 3);
    }

    // --------------------------------------------------------
    // test_verify_valid_chain
    // --------------------------------------------------------
    #[test]
    fn test_verify_valid_chain() {
        let chain = make_chain();

        for i in 0..10 {
            chain.append_entry(
                format!("model-{}", i),
                ArtifactType::ModelWeights,
                blake3_of(&format!("weights-{}", i)),
                default_metadata(),
                "tester".to_string(),
                i * 100,
            );
        }

        let result = chain.verify_chain();
        assert!(result.valid);
        assert_eq!(result.entries_checked, 10);
        assert!(result.first_invalid_index.is_none());
        assert!(result.mismatch_at.is_none());
    }

    // --------------------------------------------------------
    // test_detect_tampered_entry
    // --------------------------------------------------------
    #[test]
    fn test_detect_tampered_entry() {
        let chain = make_chain();

        // Append three entries
        chain.append_entry(
            "m1".to_string(),
            ArtifactType::ModelWeights,
            blake3_of("w1"),
            default_metadata(),
            "alice".to_string(),
            1,
        );
        chain.append_entry(
            "m1".to_string(),
            ArtifactType::ModelConfig,
            blake3_of("c1"),
            default_metadata(),
            "alice".to_string(),
            2,
        );
        chain.append_entry(
            "m2".to_string(),
            ArtifactType::Dataset,
            blake3_of("d1"),
            default_metadata(),
            "bob".to_string(),
            3,
        );

        // Tamper with entry at index 1 by modifying it directly
        if let Some(mut entry) = chain.entries.get_mut(&1) {
            entry.hash = "deadbeef".to_string();
        }

        // Verification should detect the tampering
        let result = chain.verify_chain();
        assert!(!result.valid);
        assert!(result.first_invalid_index.is_some());
        // Entry 1's hash is now wrong, so mismatch is at 1
        assert_eq!(result.mismatch_at, Some(1));
    }

    // --------------------------------------------------------
    // test_verify_range
    // --------------------------------------------------------
    #[test]
    fn test_verify_range() {
        let chain = make_chain();

        for i in 0..10 {
            chain.append_entry(
                "m1".to_string(),
                ArtifactType::ModelWeights,
                blake3_of(&format!("w{}", i)),
                default_metadata(),
                "alice".to_string(),
                i,
            );
        }

        // Verify first half
        let result = chain.verify_range(0, 4);
        assert!(result.valid);
        assert_eq!(result.entries_checked, 5);

        // Verify second half
        let result = chain.verify_range(5, 9);
        assert!(result.valid);
        assert_eq!(result.entries_checked, 5);

        // Verify middle
        let result = chain.verify_range(3, 7);
        assert!(result.valid);
        assert_eq!(result.entries_checked, 5);

        // Out-of-bounds should clamp
        let result = chain.verify_range(8, 100);
        assert!(result.valid);
        assert_eq!(result.entries_checked, 2); // entries 8 and 9
    }

    // --------------------------------------------------------
    // test_model_record_tracking
    // --------------------------------------------------------
    #[test]
    fn test_model_record_tracking() {
        let chain = make_chain();

        // Add entries for two different models
        chain.append_entry(
            "m1".to_string(),
            ArtifactType::ModelWeights,
            blake3_of("w1"),
            default_metadata(),
            "alice".to_string(),
            1,
        );
        chain.append_entry(
            "m2".to_string(),
            ArtifactType::Dataset,
            blake3_of("d1"),
            default_metadata(),
            "bob".to_string(),
            2,
        );
        chain.append_entry(
            "m1".to_string(),
            ArtifactType::ModelConfig,
            blake3_of("c1"),
            default_metadata(),
            "alice".to_string(),
            3,
        );

        let record = chain.get_model_record("m1").unwrap();
        assert_eq!(record.model_id, "m1");
        assert_eq!(record.total_artifacts, 2);
        assert_eq!(record.artifacts.len(), 2);
        assert_eq!(record.artifacts[0].artifact_type, ArtifactType::ModelWeights);
        assert_eq!(record.artifacts[1].artifact_type, ArtifactType::ModelConfig);

        let record2 = chain.get_model_record("m2").unwrap();
        assert_eq!(record2.total_artifacts, 1);
        assert_eq!(record2.artifacts[0].artifact_type, ArtifactType::Dataset);

        // Non-existent model
        assert!(chain.get_model_record("nonexistent").is_none());
    }

    // --------------------------------------------------------
    // test_search_by_hash
    // --------------------------------------------------------
    #[test]
    fn test_search_by_hash() {
        let chain = make_chain();

        let artifact_hash = blake3_of("shared-artifact");

        chain.append_entry(
            "m1".to_string(),
            ArtifactType::ModelWeights,
            artifact_hash.clone(),
            default_metadata(),
            "alice".to_string(),
            1,
        );
        chain.append_entry(
            "m2".to_string(),
            ArtifactType::ModelWeights,
            artifact_hash.clone(),
            default_metadata(),
            "bob".to_string(),
            2,
        );
        chain.append_entry(
            "m3".to_string(),
            ArtifactType::Dataset,
            blake3_of("other"),
            default_metadata(),
            "carol".to_string(),
            3,
        );

        let results = chain.search_by_hash(&artifact_hash);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].model_id, "m1");
        assert_eq!(results[1].model_id, "m2");

        // No results for non-existent hash
        let results = chain.search_by_hash("nonexistent");
        assert!(results.is_empty());
    }

    // --------------------------------------------------------
    // test_chain_tip_updates
    // --------------------------------------------------------
    #[test]
    fn test_chain_tip_updates() {
        let chain = make_chain();

        // Empty chain tip
        let tip = chain.get_tip();
        assert_eq!(tip.total_entries, 0);

        // After first append
        let e1 = chain.append_entry(
            "m1".to_string(),
            ArtifactType::ModelWeights,
            blake3_of("w1"),
            default_metadata(),
            "alice".to_string(),
            1,
        );
        let tip = chain.get_tip();
        assert_eq!(tip.total_entries, 1);
        assert_eq!(tip.latest_index, 0);
        assert_eq!(tip.latest_hash, e1.hash);

        // After second append
        let e2 = chain.append_entry(
            "m1".to_string(),
            ArtifactType::ModelConfig,
            blake3_of("c1"),
            default_metadata(),
            "alice".to_string(),
            2,
        );
        let tip = chain.get_tip();
        assert_eq!(tip.total_entries, 2);
        assert_eq!(tip.latest_index, 1);
        assert_eq!(tip.latest_hash, e2.hash);
        assert!(!tip.chain_hash.is_empty());
    }

    // --------------------------------------------------------
    // test_stats_calculation
    // --------------------------------------------------------
    #[test]
    fn test_stats_calculation() {
        let chain = make_chain();

        chain.append_entry(
            "m1".to_string(),
            ArtifactType::ModelWeights,
            blake3_of("w1"),
            default_metadata(),
            "alice".to_string(),
            1,
        );
        chain.append_entry(
            "m1".to_string(),
            ArtifactType::ModelConfig,
            blake3_of("c1"),
            default_metadata(),
            "alice".to_string(),
            2,
        );
        chain.append_entry(
            "m2".to_string(),
            ArtifactType::ModelWeights,
            blake3_of("w2"),
            default_metadata(),
            "bob".to_string(),
            3,
        );
        chain.append_entry(
            "m2".to_string(),
            ArtifactType::Dataset,
            blake3_of("d1"),
            default_metadata(),
            "bob".to_string(),
            4,
        );
        chain.append_entry(
            "m2".to_string(),
            ArtifactType::ModelWeights,
            blake3_of("w3"),
            default_metadata(),
            "bob".to_string(),
            5,
        );

        let stats = chain.get_stats();
        assert_eq!(stats.total_entries, 5);
        assert_eq!(stats.total_models, 2);
        assert_eq!(stats.chain_length, 5);
        assert_eq!(*stats.artifacts_by_type.get(&ArtifactType::ModelWeights).unwrap(), 3);
        assert_eq!(*stats.artifacts_by_type.get(&ArtifactType::ModelConfig).unwrap(), 1);
        assert_eq!(*stats.artifacts_by_type.get(&ArtifactType::Dataset).unwrap(), 1);
        assert!(stats.oldest_entry <= stats.newest_entry);
    }

    // --------------------------------------------------------
    // test_history_filtering
    // --------------------------------------------------------
    #[test]
    fn test_history_filtering() {
        let chain = make_chain();

        chain.append_entry(
            "m1".to_string(),
            ArtifactType::ModelWeights,
            blake3_of("w1"),
            default_metadata(),
            "alice".to_string(),
            1,
        );
        chain.append_entry(
            "m1".to_string(),
            ArtifactType::ModelConfig,
            blake3_of("c1"),
            default_metadata(),
            "alice".to_string(),
            2,
        );
        chain.append_entry(
            "m1".to_string(),
            ArtifactType::ModelWeights,
            blake3_of("w2"),
            default_metadata(),
            "alice".to_string(),
            3,
        );
        chain.append_entry(
            "m1".to_string(),
            ArtifactType::Dataset,
            blake3_of("d1"),
            default_metadata(),
            "bob".to_string(),
            4,
        );

        // All history
        let all = chain.get_history("m1", None);
        assert_eq!(all.len(), 4);

        // Filtered by type
        let weights = chain.get_history("m1", Some(&ArtifactType::ModelWeights));
        assert_eq!(weights.len(), 2);

        let configs = chain.get_history("m1", Some(&ArtifactType::ModelConfig));
        assert_eq!(configs.len(), 1);

        let datasets = chain.get_history("m1", Some(&ArtifactType::Dataset));
        assert_eq!(datasets.len(), 1);

        // Non-existent type
        let scripts = chain.get_history("m1", Some(&ArtifactType::TrainingScript));
        assert!(scripts.is_empty());
    }

    // --------------------------------------------------------
    // test_export_chain
    // --------------------------------------------------------
    #[test]
    fn test_export_chain() {
        let chain = make_chain();

        for i in 0..10 {
            chain.append_entry(
                "m1".to_string(),
                ArtifactType::ModelWeights,
                blake3_of(&format!("w{}", i)),
                default_metadata(),
                "alice".to_string(),
                i,
            );
        }

        // Export all
        let all = chain.export_chain(None, None);
        assert_eq!(all.len(), 10);
        assert_eq!(all[0].index, 0);
        assert_eq!(all[9].index, 9);

        // Export range
        let partial = chain.export_chain(Some(3), Some(7));
        assert_eq!(partial.len(), 5);
        assert_eq!(partial[0].index, 3);
        assert_eq!(partial[4].index, 7);

        // Export from single index
        let single = chain.export_chain(Some(5), Some(5));
        assert_eq!(single.len(), 1);
        assert_eq!(single[0].index, 5);

        // Export empty chain
        let empty_chain = make_chain();
        let empty = empty_chain.export_chain(None, None);
        assert!(empty.is_empty());
    }

    // --------------------------------------------------------
    // test_multiple_models
    // --------------------------------------------------------
    #[test]
    fn test_multiple_models() {
        let chain = make_chain();

        // Model A: weights, config
        chain.append_entry(
            "model-a".to_string(),
            ArtifactType::ModelWeights,
            blake3_of("wa1"),
            default_metadata(),
            "alice".to_string(),
            1,
        );
        chain.append_entry(
            "model-a".to_string(),
            ArtifactType::ModelConfig,
            blake3_of("ca1"),
            default_metadata(),
            "alice".to_string(),
            2,
        );

        // Model B: weights, dataset, evaluation
        chain.append_entry(
            "model-b".to_string(),
            ArtifactType::ModelWeights,
            blake3_of("wb1"),
            default_metadata(),
            "bob".to_string(),
            3,
        );
        chain.append_entry(
            "model-b".to_string(),
            ArtifactType::Dataset,
            blake3_of("db1"),
            default_metadata(),
            "bob".to_string(),
            4,
        );
        chain.append_entry(
            "model-b".to_string(),
            ArtifactType::EvaluationResult,
            blake3_of("eb1"),
            default_metadata(),
            "bob".to_string(),
            5,
        );

        // Verify per-model queries
        let entries_a = chain.get_entries("model-a");
        assert_eq!(entries_a.len(), 2);

        let entries_b = chain.get_entries("model-b");
        assert_eq!(entries_b.len(), 3);

        // Verify stats
        let stats = chain.get_stats();
        assert_eq!(stats.total_entries, 5);
        assert_eq!(stats.total_models, 2);

        // Verify chain integrity
        let result = chain.verify_chain();
        assert!(result.valid);
    }

    // --------------------------------------------------------
    // test_concurrent_appends
    // --------------------------------------------------------
    #[test]
    fn test_concurrent_appends() {
        use std::thread;

        let chain = make_chain();
        let num_threads = 8;
        let entries_per_thread = 50;

        let mut handles = Vec::new();

        for t in 0..num_threads {
            let chain_clone = chain.clone();
            let handle = thread::spawn(move || {
                for i in 0..entries_per_thread {
                    chain_clone.append_entry(
                        format!("model-{}", t),
                        ArtifactType::ModelWeights,
                        blake3_of(&format!("w-{}-{}", t, i)),
                        default_metadata(),
                        format!("thread-{}", t),
                        (t * entries_per_thread + i) as u64,
                    );
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let expected = num_threads * entries_per_thread;
        assert_eq!(chain.len(), expected as u64);

        // Verify chain integrity after concurrent appends
        let result = chain.verify_chain();
        assert!(result.valid);
        assert_eq!(result.entries_checked, expected as u64);

        // Verify stats
        let stats = chain.get_stats();
        assert_eq!(stats.total_entries, expected as u64);
        assert_eq!(stats.total_models, num_threads as u64);
    }

    // --------------------------------------------------------
    // test_empty_chain_verification
    // --------------------------------------------------------
    #[test]
    fn test_empty_chain_verification() {
        let chain = make_chain();

        // Verify empty chain
        let result = chain.verify_chain();
        assert!(result.valid);
        assert_eq!(result.entries_checked, 0);
        assert!(result.first_invalid_index.is_none());
        assert!(result.mismatch_at.is_none());

        // Verify range on empty chain
        let result = chain.verify_range(0, 10);
        assert!(result.valid);
        assert_eq!(result.entries_checked, 0);

        // Empty chain tip
        let tip = chain.get_tip();
        assert_eq!(tip.total_entries, 0);

        // Empty chain stats
        let stats = chain.get_stats();
        assert_eq!(stats.total_entries, 0);
        assert_eq!(stats.total_models, 0);
        assert!(stats.artifacts_by_type.is_empty());
    }
}
