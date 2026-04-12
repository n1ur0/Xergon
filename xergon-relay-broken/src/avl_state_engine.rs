//! AVL State Commitment Engine for Xergon Relay
//!
//! Provides an in-memory AVL-tree-style state commitment layer that tracks
//! the Xergon provider registry and generates BLAKE2b256 Merkle proofs.
//! The tree is simulated via sorted key iteration; the digest is a BLAKE3
//! hash over all entries, producing a deterministic commitment root.

// ================================================================
// Imports
// ================================================================

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use tracing::{debug, info, warn};

use crate::proxy;

// ================================================================
// Data Types
// ================================================================

/// Provider state tracked inside the AVL tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvlProviderState {
    pub provider_pk: String,
    pub endpoint: String,
    pub models: Vec<String>,
    pub pown_score: f64,
    pub region: String,
    pub last_heartbeat: u64,
    pub total_tokens: u64,
    pub total_requests: u64,
    pub is_active: bool,
}

/// A single entry in the AVL tree keyed by provider_id (hex).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvlTreeEntry {
    pub key: String,
    pub value: AvlProviderState,
    pub inserted_at: u64,
    pub updated_at: u64,
}

/// Merkle-style state proof for a single provider entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateProof {
    pub root_digest: String,
    pub entries_hash: String,
    pub block_height: u64,
    pub timestamp: i64,
    pub proof_bytes: String,
}

/// Diff between two tree snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub modified: Vec<String>,
    pub root_before: String,
    pub root_after: String,
}

/// Aggregated batch proof for multiple providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchProof {
    pub proofs: Vec<StateProof>,
    pub aggregated_digest: String,
    pub entry_count: u32,
}

/// Statistics about the current AVL tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvlTreeStats {
    pub total_entries: u64,
    pub tree_depth: u32,
    pub root_digest: String,
    pub last_update_height: u64,
    pub storage_bytes: u64,
}

// ================================================================
// Internal snapshot record
// ================================================================

#[derive(Debug, Clone)]
struct TreeSnapshot {
    root_digest: String,
    keys: Vec<String>,
    height: u64,
}

// ================================================================
// AVL State Engine
// ================================================================

/// Core state engine.  Uses a `DashMap` for concurrent access and
/// computes BLAKE3 digests over the sorted entry set.
pub struct AvlStateEngine {
    /// Primary store: provider_id hex -> AvlTreeEntry
    entries: DashMap<String, AvlTreeEntry>,
    /// Monotonically increasing height counter (simulates block height).
    current_height: AtomicU64,
    /// Last computed root digest (cached).
    cached_root: std::sync::RwLock<String>,
    /// Snapshot history for diff computation.
    snapshots: std::sync::RwLock<Vec<TreeSnapshot>>,
}

impl AvlStateEngine {
    /// Create a new empty engine.
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
            current_height: AtomicU64::new(0),
            cached_root: std::sync::RwLock::new(Self::compute_empty_digest()),
            snapshots: std::sync::RwLock::new(Vec::new()),
        }
    }

    /// The digest of an empty tree (BLAKE3 of the literal string "empty").
    fn compute_empty_digest() -> String {
        blake3::hash(b"empty").to_hex().to_string()
    }

    // ----------------------------------------------------------
    // Height
    // ----------------------------------------------------------

    /// Advance the height by one and return the new value.
    fn next_height(&self) -> u64 {
        self.current_height.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Return the current height without advancing.
    fn height(&self) -> u64 {
        self.current_height.load(Ordering::SeqCst)
    }

    // ----------------------------------------------------------
    // Insert / Remove
    // ----------------------------------------------------------

    /// Insert or update a provider entry.  Returns the new height.
    pub fn insert_provider(
        &self,
        provider_id: &str,
        state: AvlProviderState,
    ) -> u64 {
        let height = self.next_height();
        let mut entry = self
            .entries
            .get_mut(provider_id)
            .map(|mut r| r.value().clone())
            .unwrap_or_else(|| AvlTreeEntry {
                key: provider_id.to_string(),
                value: AvlProviderState {
                    provider_pk: String::new(),
                    endpoint: String::new(),
                    models: Vec::new(),
                    pown_score: 0.0,
                    region: String::new(),
                    last_heartbeat: 0,
                    total_tokens: 0,
                    total_requests: 0,
                    is_active: false,
                },
                inserted_at: height,
                updated_at: height,
            });

        entry.value = state;
        entry.updated_at = height;
        if entry.inserted_at == 0 {
            entry.inserted_at = height;
        }

        self.entries.insert(provider_id.to_string(), entry);
        self.refresh_cached_root();
        debug!(provider_id = %provider_id, height, "provider inserted/updated");
        height
    }

    /// Remove a provider.  Returns true if the key existed.
    pub fn remove_provider(&self, provider_id: &str) -> bool {
        let existed = self.entries.remove(provider_id).is_some();
        if existed {
            let _ = self.next_height();
            self.refresh_cached_root();
            debug!(provider_id = %provider_id, "provider removed");
        }
        existed
    }

    // ----------------------------------------------------------
    // Digest
    // ----------------------------------------------------------

    /// Compute BLAKE3 digest over all entries sorted by key.
    /// Format: "key:json_value\n" per entry.
    pub fn compute_root_digest(&self) -> String {
        let mut keys: Vec<String> = self.entries.iter().map(|r| r.key().clone()).collect();
        keys.sort();

        if keys.is_empty() {
            return Self::compute_empty_digest();
        }

        let mut hasher = blake3::Hasher::new();
        for key in &keys {
            if let Some(entry) = self.entries.get(key) {
                let line = format!("{}:{}", key, serde_json::to_string(&entry.value).unwrap_or_default());
                hasher.update(line.as_bytes());
                hasher.update(b"\n");
            }
        }
        hasher.finalize().to_hex().to_string()
    }

    /// Recompute the cached root digest from current entries.
    fn refresh_cached_root(&self) {
        let digest = self.compute_root_digest();
        *self.cached_root.write().unwrap() = digest;
    }

    /// Return the cached root digest.
    pub fn cached_root_digest(&self) -> String {
        self.cached_root.read().unwrap().clone()
    }

    // ----------------------------------------------------------
    // Proofs
    // ----------------------------------------------------------

    /// Generate a Merkle-style proof for a single provider.
    pub fn get_proof(&self, provider_id: &str) -> Option<StateProof> {
        let entry = self.entries.get(provider_id)?;
        let entries_hash = blake3::hash(
            serde_json::to_string(&entry.value).unwrap_or_default().as_bytes(),
        )
        .to_hex()
        .to_string();

        // Build proof bytes: serialize the entry + sibling hashes
        // In a real AVL tree this would contain path hashes; here we
        // include the entry data and the root digest for verification.
        let proof_data = serde_json::json!({
            "provider_id": provider_id,
            "entry": entry.value,
            "entries_hash": &entries_hash,
            "root_digest": self.cached_root_digest(),
        });
        let proof_bytes = blake3::hash(
            serde_json::to_string(&proof_data).unwrap_or_default().as_bytes(),
        )
        .to_hex()
        .to_string();

        Some(StateProof {
            root_digest: self.cached_root_digest(),
            entries_hash,
            block_height: self.height(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            proof_bytes,
        })
    }

    /// Generate proofs for multiple providers at once.
    pub fn get_batch_proof(&self, provider_ids: &[String]) -> BatchProof {
        let proofs: Vec<StateProof> = provider_ids
            .iter()
            .filter_map(|id| self.get_proof(id))
            .collect();

        // Aggregated digest over all individual proof bytes.
        let mut hasher = blake3::Hasher::new();
        for p in &proofs {
            hasher.update(p.proof_bytes.as_bytes());
        }
        let aggregated_digest = hasher.finalize().to_hex().to_string();

        BatchProof {
            entry_count: proofs.len() as u32,
            proofs,
            aggregated_digest,
        }
    }

    // ----------------------------------------------------------
    // Verification
    // ----------------------------------------------------------

    /// Verify a proof against the current root digest.
    /// Returns true if the proof's root_digest matches the current tree root
    /// and the entries_hash is consistent with the claimed entry.
    pub fn verify_proof(
        &self,
        proof: &StateProof,
        provider_id: &str,
        state: &AvlProviderState,
    ) -> bool {
        // Step 1: check root matches current tree root.
        let current_root = self.cached_root_digest();
        if proof.root_digest != current_root {
            warn!(
                expected = %current_root,
                got = %proof.root_digest,
                "root digest mismatch"
            );
            return false;
        }

        // Step 2: recompute entries_hash from the provided state.
        let recomputed = blake3::hash(
            serde_json::to_string(state).unwrap_or_default().as_bytes(),
        )
        .to_hex()
        .to_string();

        if recomputed != proof.entries_hash {
            warn!("entries_hash mismatch");
            return false;
        }

        // Step 3: recompute proof_bytes and verify.
        let proof_data = serde_json::json!({
            "provider_id": provider_id,
            "entry": state,
            "entries_hash": &proof.entries_hash,
            "root_digest": &proof.root_digest,
        });
        let recomputed_proof_bytes = blake3::hash(
            serde_json::to_string(&proof_data).unwrap_or_default().as_bytes(),
        )
        .to_hex()
        .to_string();

        if recomputed_proof_bytes != proof.proof_bytes {
            warn!("proof_bytes mismatch");
            return false;
        }

        true
    }

    // ----------------------------------------------------------
    // Snapshots & Diff
    // ----------------------------------------------------------

    /// Save the current state as a named snapshot for later diffing.
    pub fn snapshot(&self) -> String {
        let keys: Vec<String> = self.entries.iter().map(|r| r.key().clone()).collect();
        let root = self.cached_root_digest();
        let height = self.height();

        let snap = TreeSnapshot {
            root_digest: root.clone(),
            keys,
            height,
        };

        self.snapshots.write().unwrap().push(snap);
        info!(root = %root, height, "snapshot saved");
        root
    }

    /// Compare current tree state to the most recent snapshot.
    /// Returns None if no snapshot exists.
    pub fn get_tree_diff(&self) -> Option<TreeDiff> {
        let snapshots = self.snapshots.read().unwrap();
        let latest = snapshots.last()?;

        let current_keys: std::collections::HashSet<String> =
            self.entries.iter().map(|r| r.key().clone()).collect();
        let snap_keys: std::collections::HashSet<String> =
            latest.keys.iter().cloned().collect();

        let added: Vec<String> = current_keys.difference(&snap_keys).cloned().collect();
        let removed: Vec<String> = snap_keys.difference(&current_keys).cloned().collect();

        // Modified = keys present in both but with different state.
        let mut modified = Vec::new();
        for key in current_keys.intersection(&snap_keys) {
            if let Some(entry) = self.entries.get(key) {
                // Recompute what the hash would have been; if keys are the same
                // but the value JSON differs, mark as modified.
                // For simplicity we always mark intersection keys as potentially
                // modified if they have been updated since the snapshot height.
                if entry.updated_at > latest.height {
                    modified.push(key.clone());
                }
            }
        }

        let mut added = added;
        let mut removed = removed;
        let mut modified = modified;
        added.sort();
        removed.sort();
        modified.sort();

        Some(TreeDiff {
            added,
            removed,
            modified,
            root_before: latest.root_digest.clone(),
            root_after: self.cached_root_digest(),
        })
    }

    // ----------------------------------------------------------
    // Stats
    // ----------------------------------------------------------

    /// Return tree statistics.
    pub fn get_stats(&self) -> AvlTreeStats {
        let total_entries = self.entries.len() as u64;

        // Estimate depth as ceil(log2(n+1)) — the theoretical AVL depth.
        let tree_depth = if total_entries == 0 {
            0
        } else {
            (total_entries as f64).log2().ceil() as u32
        };

        // Estimate storage bytes.
        let storage_bytes: u64 = self
            .entries
            .iter()
            .map(|r| {
                serde_json::to_string(r.value()).map(|s| s.len() as u64).unwrap_or(0)
            })
            .sum();

        AvlTreeStats {
            total_entries,
            tree_depth,
            root_digest: self.cached_root_digest(),
            last_update_height: self.height(),
            storage_bytes,
        }
    }

    // ----------------------------------------------------------
    // Listing
    // ----------------------------------------------------------

    /// Return all provider entries sorted by key.
    pub fn get_all_providers(&self) -> Vec<AvlTreeEntry> {
        let mut entries: Vec<AvlTreeEntry> = self
            .entries
            .iter()
            .map(|r| r.value().clone())
            .collect();
        entries.sort_by(|a, b| a.key.cmp(&b.key));
        entries
    }
}

impl Default for AvlStateEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ================================================================
// REST API Request / Response types
// ================================================================

#[derive(Debug, Clone, Deserialize)]
pub struct InsertProviderRequest {
    pub provider_id: String,
    pub provider_pk: String,
    pub endpoint: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub pown_score: f64,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub last_heartbeat: u64,
    #[serde(default)]
    pub total_tokens: u64,
    #[serde(default)]
    pub total_requests: u64,
    #[serde(default = "default_true")]
    pub is_active: bool,
}

fn default_true() -> bool {
    true
}

impl From<InsertProviderRequest> for AvlProviderState {
    fn from(r: InsertProviderRequest) -> Self {
        Self {
            provider_pk: r.provider_pk,
            endpoint: r.endpoint,
            models: r.models,
            pown_score: r.pown_score,
            region: r.region,
            last_heartbeat: r.last_heartbeat,
            total_tokens: r.total_tokens,
            total_requests: r.total_requests,
            is_active: r.is_active,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct InsertProviderResponse {
    pub provider_id: String,
    pub height: u64,
    pub root_digest: String,
}

#[derive(Debug, Serialize)]
pub struct RemoveProviderResponse {
    pub provider_id: String,
    pub removed: bool,
    pub root_digest: String,
}

#[derive(Debug, Serialize)]
pub struct RootResponse {
    pub root_digest: String,
    pub stats: AvlTreeStats,
}

#[derive(Debug, Deserialize)]
pub struct BatchProofRequest {
    pub provider_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct VerifyProofRequest {
    pub proof: StateProof,
    pub provider_id: String,
    pub state: AvlProviderState,
}

#[derive(Debug, Serialize)]
pub struct VerifyProofResponse {
    pub valid: bool,
}

#[derive(Debug, Serialize)]
pub struct SnapshotResponse {
    pub root_digest: String,
    pub height: u64,
}

#[derive(Debug, Serialize)]
pub struct DiffResponse {
    pub diff: Option<TreeDiff>,
}

#[derive(Debug, Serialize)]
pub struct AllProvidersResponse {
    pub providers: Vec<AvlTreeEntry>,
    pub count: usize,
}

// ================================================================
// Shared state wrapper for axum handlers
// ================================================================

#[derive(Clone)]
pub struct AvlEngineState {
    pub engine: Arc<AvlStateEngine>,
}

// ================================================================
// REST Handlers
// ================================================================

/// POST /v1/avl-state/provider — Insert or update a provider.
async fn insert_provider_handler(
    State(state): State<proxy::AppState>,
    Json(req): Json<InsertProviderRequest>,
) -> Result<Json<InsertProviderResponse>, (StatusCode, String)> {
    let engine = &state.avl_engine.engine;
    let provider_state = AvlProviderState::from(req.clone());
    let height = engine.insert_provider(&req.provider_id, provider_state);
    let root = engine.cached_root_digest();

    Ok(Json(InsertProviderResponse {
        provider_id: req.provider_id,
        height,
        root_digest: root,
    }))
}

/// DELETE /v1/avl-state/provider/:id — Remove a provider.
async fn remove_provider_handler(
    State(state): State<proxy::AppState>,
    Path(provider_id): Path<String>,
) -> Result<Json<RemoveProviderResponse>, (StatusCode, String)> {
    let engine = &state.avl_engine.engine;
    let removed = engine.remove_provider(&provider_id);
    let root = engine.cached_root_digest();

    Ok(Json(RemoveProviderResponse {
        provider_id,
        removed,
        root_digest: root,
    }))
}

/// GET /v1/avl-state/proof/:id — Get proof for a provider.
async fn get_proof_handler(
    State(state): State<proxy::AppState>,
    Path(provider_id): Path<String>,
) -> Result<Json<StateProof>, (StatusCode, String)> {
    let engine = &state.avl_engine.engine;
    match engine.get_proof(&provider_id) {
        Some(proof) => Ok(Json(proof)),
        None => Err((
            StatusCode::NOT_FOUND,
            format!("provider {} not found", provider_id),
        )),
    }
}

/// POST /v1/avl-state/batch-proof — Batch proof request.
async fn batch_proof_handler(
    State(state): State<proxy::AppState>,
    Json(req): Json<BatchProofRequest>,
) -> Json<BatchProof> {
    let engine = &state.avl_engine.engine;
    let batch = engine.get_batch_proof(&req.provider_ids);
    Json(batch)
}

/// GET /v1/avl-state/root — Get current root digest + stats.
async fn get_root_handler(
    State(state): State<proxy::AppState>,
) -> Json<RootResponse> {
    let engine = &state.avl_engine.engine;
    let stats = engine.get_stats();
    Json(RootResponse {
        root_digest: stats.root_digest.clone(),
        stats,
    })
}

/// GET /v1/avl-state/diff — Get tree diff since last snapshot.
async fn get_diff_handler(
    State(state): State<proxy::AppState>,
) -> Json<DiffResponse> {
    let engine = &state.avl_engine.engine;
    let diff = engine.get_tree_diff();
    Json(DiffResponse { diff })
}

/// POST /v1/avl-state/snapshot — Save current state as snapshot.
async fn snapshot_handler(
    State(state): State<proxy::AppState>,
) -> Json<SnapshotResponse> {
    let engine = &state.avl_engine.engine;
    let root = engine.snapshot();
    Json(SnapshotResponse {
        root_digest: root,
        height: engine.height(),
    })
}

/// POST /v1/avl-state/verify — Verify a proof.
async fn verify_proof_handler(
    State(state): State<proxy::AppState>,
    Json(req): Json<VerifyProofRequest>,
) -> Json<VerifyProofResponse> {
    let engine = &state.avl_engine.engine;
    let valid = engine.verify_proof(&req.proof, &req.provider_id, &req.state);
    Json(VerifyProofResponse { valid })
}

/// GET /v1/avl-state/providers — List all providers.
async fn get_all_providers_handler(
    State(state): State<proxy::AppState>,
) -> Json<AllProvidersResponse> {
    let engine = &state.avl_engine.engine;
    let providers = engine.get_all_providers();
    let count = providers.len();
    Json(AllProvidersResponse { providers, count })
}

// ================================================================
// Router Builder
// ================================================================

pub fn build_router(state: proxy::AppState) -> Router<proxy::AppState> {
    Router::new()
        .route("/v1/avl-state/provider", post(insert_provider_handler))
        .route("/v1/avl-state/provider/{id}", delete(remove_provider_handler))
        .route("/v1/avl-state/proof/{id}", get(get_proof_handler))
        .route("/v1/avl-state/batch-proof", post(batch_proof_handler))
        .route("/v1/avl-state/root", get(get_root_handler))
        .route("/v1/avl-state/diff", get(get_diff_handler))
        .route("/v1/avl-state/snapshot", post(snapshot_handler))
        .route("/v1/avl-state/verify", post(verify_proof_handler))
        .route("/v1/avl-state/providers", get(get_all_providers_handler))
}

// ================================================================
// Unit Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    /// Helper: build a sample provider state for testing.
    fn sample_provider(pk: &str) -> AvlProviderState {
        AvlProviderState {
            provider_pk: pk.to_string(),
            endpoint: format!("https://{}.xergon.io", pk),
            models: vec!["llama-3-70b".to_string()],
            pown_score: 0.95,
            region: "us-east".to_string(),
            last_heartbeat: 1000,
            total_tokens: 500_000,
            total_requests: 1200,
            is_active: true,
        }
    }

    #[test]
    fn test_insert_provider() {
        let engine = AvlStateEngine::new();
        let state = sample_provider("pk1");
        let height = engine.insert_provider("abc123", state);

        assert_eq!(height, 1);
        assert_eq!(engine.entries.len(), 1);
        let entry = engine.entries.get("abc123").unwrap();
        assert_eq!(entry.key, "abc123");
        assert_eq!(entry.value.provider_pk, "pk1");
        assert_eq!(entry.inserted_at, 1);
        assert_eq!(entry.updated_at, 1);
    }

    #[test]
    fn test_remove_provider() {
        let engine = AvlStateEngine::new();
        engine.insert_provider("abc123", sample_provider("pk1"));
        assert_eq!(engine.entries.len(), 1);

        let removed = engine.remove_provider("abc123");
        assert!(removed);
        assert_eq!(engine.entries.len(), 0);

        // Removing non-existent returns false.
        let removed2 = engine.remove_provider("nonexistent");
        assert!(!removed2);
    }

    #[test]
    fn test_get_proof_valid() {
        let engine = AvlStateEngine::new();
        engine.insert_provider("abc123", sample_provider("pk1"));

        let proof = engine.get_proof("abc123");
        assert!(proof.is_some());

        let proof = proof.unwrap();
        assert!(!proof.root_digest.is_empty());
        assert!(!proof.entries_hash.is_empty());
        assert!(!proof.proof_bytes.is_empty());
        assert_eq!(proof.block_height, 1);
    }

    #[test]
    fn test_get_proof_missing() {
        let engine = AvlStateEngine::new();
        let proof = engine.get_proof("nonexistent");
        assert!(proof.is_none());
    }

    #[test]
    fn test_batch_proof() {
        let engine = AvlStateEngine::new();
        engine.insert_provider("aaa", sample_provider("pk1"));
        engine.insert_provider("bbb", sample_provider("pk2"));
        engine.insert_provider("ccc", sample_provider("pk3"));

        let ids = vec!["aaa".to_string(), "bbb".to_string(), "ccc".to_string()];
        let batch = engine.get_batch_proof(&ids);

        assert_eq!(batch.entry_count, 3);
        assert_eq!(batch.proofs.len(), 3);
        assert!(!batch.aggregated_digest.is_empty());
    }

    #[test]
    fn test_root_digest_changes_on_update() {
        let engine = AvlStateEngine::new();
        engine.insert_provider("abc", sample_provider("pk1"));
        let root1 = engine.cached_root_digest();

        // Update the same provider with different state.
        let mut state = sample_provider("pk1");
        state.pown_score = 0.5;
        engine.insert_provider("abc", state);
        let root2 = engine.cached_root_digest();

        assert_ne!(root1, root2);
    }

    #[test]
    fn test_root_digest_deterministic() {
        let engine1 = AvlStateEngine::new();
        let engine2 = AvlStateEngine::new();

        engine1.insert_provider("abc", sample_provider("pk1"));
        engine1.insert_provider("def", sample_provider("pk2"));

        engine2.insert_provider("abc", sample_provider("pk1"));
        engine2.insert_provider("def", sample_provider("pk2"));

        assert_eq!(engine1.cached_root_digest(), engine2.cached_root_digest());
    }

    #[test]
    fn test_tree_diff_no_changes() {
        let engine = AvlStateEngine::new();
        engine.insert_provider("abc", sample_provider("pk1"));
        engine.snapshot();

        let diff = engine.get_tree_diff();
        assert!(diff.is_some());
        let diff = diff.unwrap();
        assert!(diff.added.is_empty());
        assert!(diff.removed.is_empty());
        assert!(diff.modified.is_empty());
        // Root before == root after.
        assert_eq!(diff.root_before, diff.root_after);
    }

    #[test]
    fn test_tree_diff_with_changes() {
        let engine = AvlStateEngine::new();
        engine.insert_provider("abc", sample_provider("pk1"));
        engine.snapshot();

        // Add a new provider.
        engine.insert_provider("def", sample_provider("pk2"));

        let diff = engine.get_tree_diff().unwrap();
        assert_eq!(diff.added, vec!["def".to_string()]);
        assert!(diff.removed.is_empty());
    }

    #[test]
    fn test_verify_proof_valid() {
        let engine = AvlStateEngine::new();
        let state = sample_provider("pk1");
        engine.insert_provider("abc123", state.clone());

        let proof = engine.get_proof("abc123").unwrap();
        let valid = engine.verify_proof(&proof, "abc123", &state);
        assert!(valid);
    }

    #[test]
    fn test_verify_proof_invalid() {
        let engine = AvlStateEngine::new();
        let state = sample_provider("pk1");
        engine.insert_provider("abc123", state);

        // Create a proof, then mutate the state and try to verify.
        let proof = engine.get_proof("abc123").unwrap();
        let tampered = AvlProviderState {
            provider_pk: "tampered".to_string(),
            endpoint: "https://evil.io".to_string(),
            models: vec![],
            pown_score: 0.0,
            region: String::new(),
            last_heartbeat: 0,
            total_tokens: 0,
            total_requests: 0,
            is_active: false,
        };

        let valid = engine.verify_proof(&proof, "abc123", &tampered);
        assert!(!valid);
    }

    #[test]
    fn test_snapshot_and_diff() {
        let engine = AvlStateEngine::new();

        // Empty tree snapshot.
        let root0 = engine.snapshot();
        assert_eq!(root0, AvlStateEngine::compute_empty_digest());

        // Add providers.
        engine.insert_provider("a", sample_provider("pk1"));
        engine.insert_provider("b", sample_provider("pk2"));

        // Snapshot after additions.
        engine.snapshot();

        // Remove one and add another.
        engine.remove_provider("a");
        engine.insert_provider("c", sample_provider("pk3"));

        let diff = engine.get_tree_diff().unwrap();
        assert_eq!(diff.added, vec!["c".to_string()]);
        assert_eq!(diff.removed, vec!["a".to_string()]);
    }

    #[test]
    fn test_get_stats() {
        let engine = AvlStateEngine::new();

        // Empty tree.
        let stats = engine.get_stats();
        assert_eq!(stats.total_entries, 0);
        assert_eq!(stats.tree_depth, 0);
        assert_eq!(stats.root_digest, AvlStateEngine::compute_empty_digest());
        assert_eq!(stats.storage_bytes, 0);

        // With entries.
        engine.insert_provider("abc", sample_provider("pk1"));
        engine.insert_provider("def", sample_provider("pk2"));

        let stats = engine.get_stats();
        assert_eq!(stats.total_entries, 2);
        assert!(stats.tree_depth >= 1);
        assert!(stats.storage_bytes > 0);
    }

    #[test]
    fn test_empty_tree_digest() {
        let engine = AvlStateEngine::new();
        let digest = engine.compute_root_digest();
        assert_eq!(digest, AvlStateEngine::compute_empty_digest());
        assert!(!digest.is_empty());
    }

    #[test]
    fn test_concurrent_access() {
        let engine = Arc::new(AvlStateEngine::new());
        let mut handles = Vec::new();

        for i in 0..10 {
            let eng = Arc::clone(&engine);
            let handle = thread::spawn(move || {
                let provider_id = format!("provider_{:02}", i);
                let state = sample_provider(&format!("pk{}", i));
                eng.insert_provider(&provider_id, state);
            });
            handles.push(handle);
        }

        for h in handles {
            h.join().unwrap();
        }

        // All 10 inserts should be present.
        assert_eq!(engine.entries.len(), 10);

        // Digest should be deterministic.
        let root1 = engine.cached_root_digest();
        let root2 = engine.compute_root_digest();
        assert_eq!(root1, root2);
    }
}
