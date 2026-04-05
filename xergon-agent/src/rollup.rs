//! Usage proof rollup system with Merkle tree commitment batching.
//!
//! Instead of creating a new box for every inference request, this system
//! batches multiple proofs into a single commitment box containing a Merkle root.
//!
//! Flow:
//! 1. Inference completions add proofs to a pending buffer (per provider).
//! 2. A background task periodically checks if an epoch is ready to commit.
//! 3. When ready, it builds a Merkle tree of all proofs and submits a single
//!    commitment box on-chain with the Merkle root in R8.
//!
//! An epoch commits when:
//! - Minimum number of proofs accumulated (min_proofs_per_commitment), OR
//! - Maximum time elapsed since first proof in epoch (epoch_duration_blocks worth of time)
//!
//! The commitment box contract (contracts/usage_commitment.es) stores:
//! - R4: provider public key (SigmaProp)
//! - R5: epoch start block height (Int)
//! - R6: epoch end block height (Int)
//! - R7: proof count (Int)
//! - R8: Merkle root (Coll[Byte]) — blake2b256 of all proof hashes
//! - tokens(0): commitment NFT (preserved across spends)

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::chain::client::ErgoNodeClient;
use crate::chain::merkle::MerkleTree;
use crate::chain::transactions::{encode_coll_byte, encode_int, PendingUsageProof};
use crate::config::RollupConfig;
use crate::protocol::tx_safety::{validate_payment_request, validate_token_id};

/// A single usage proof entry for Merkle tree leaf computation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageProof {
    /// User public key (hex)
    pub user_pk: String,
    /// Provider public key (hex)
    pub provider_pk: String,
    /// Model name
    pub model: String,
    /// Token count
    pub token_count: i64,
    /// Unix timestamp (ms)
    pub timestamp_ms: i64,
    /// Rarity multiplier (1.0 = no bonus)
    #[serde(default)]
    pub rarity_multiplier: f64,
}

impl UsageProof {
    /// Serialize this proof for Merkle tree leaf hashing.
    ///
    /// Format: user_pk || provider_pk || model || token_count (4 bytes BE) || timestamp (8 bytes BE)
    fn to_leaf_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        // User PK (hex decoded)
        if let Ok(pk_bytes) = hex::decode(&self.user_pk) {
            buf.extend_from_slice(&pk_bytes);
        } else {
            buf.extend_from_slice(self.user_pk.as_bytes());
        }

        // Provider PK (hex decoded)
        if let Ok(pk_bytes) = hex::decode(&self.provider_pk) {
            buf.extend_from_slice(&pk_bytes);
        } else {
            buf.extend_from_slice(self.provider_pk.as_bytes());
        }

        // Model name (UTF-8)
        buf.extend_from_slice(self.model.as_bytes());

        // Delimiter byte
        buf.push(0x00);

        // Token count (8 bytes big-endian)
        buf.extend_from_slice(&self.token_count.to_be_bytes());

        // Timestamp (8 bytes big-endian)
        buf.extend_from_slice(&self.timestamp_ms.to_be_bytes());

        buf
    }
}

impl From<&PendingUsageProof> for UsageProof {
    fn from(p: &PendingUsageProof) -> Self {
        Self {
            user_pk: p.user_pk.clone(),
            provider_pk: String::new(), // filled in by rollup
            model: p.model.clone(),
            token_count: p.token_count,
            timestamp_ms: p.timestamp_ms,
            rarity_multiplier: p.rarity_multiplier,
        }
    }
}

/// State of an epoch batch for a provider.
#[derive(Debug)]
struct EpochState {
    /// Pending proofs for this epoch.
    proofs: Vec<UsageProof>,
    /// When the first proof was added to this epoch.
    epoch_start: Instant,
    /// Total tokens in this epoch.
    total_tokens: i64,
}

impl EpochState {
    fn new() -> Self {
        Self {
            proofs: Vec::new(),
            epoch_start: Instant::now(),
            total_tokens: 0,
        }
    }

    fn is_empty(&self) -> bool {
        self.proofs.is_empty()
    }
}

/// Result of a commitment submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitmentResult {
    /// Transaction ID of the commitment box creation.
    pub tx_id: String,
    /// Number of proofs in this commitment.
    pub proof_count: usize,
    /// Merkle root hex.
    pub merkle_root_hex: String,
    /// Provider ID.
    pub provider_pk: String,
    /// Timestamp (ISO 8601).
    pub timestamp: String,
    /// Total tokens in this batch.
    pub total_tokens: i64,
}

/// Usage proof rollup system.
///
/// Buffers proofs and periodically commits them as Merkle root commitments on-chain.
pub struct UsageRollup {
    config: RollupConfig,
    /// Pending proofs keyed by provider PK.
    pending: DashMap<String, EpochState>,
    /// History of recent commitments (for status endpoint).
    recent_commitments: Mutex<Vec<CommitmentResult>>,
    /// Total proofs committed across all epochs.
    total_committed: std::sync::atomic::AtomicU64,
    /// Total tokens committed across all epochs.
    total_tokens_committed: std::sync::atomic::AtomicI64,
}

impl UsageRollup {
    /// Create a new usage rollup system.
    pub fn new(config: RollupConfig) -> Self {
        Self {
            config,
            pending: DashMap::new(),
            recent_commitments: Mutex::new(Vec::new()),
            total_committed: std::sync::atomic::AtomicU64::new(0),
            total_tokens_committed: std::sync::atomic::AtomicI64::new(0),
        }
    }

    /// Add a usage proof to the pending batch for the given provider.
    pub fn add_proof(&self, provider_pk: &str, proof: UsageProof) {
        let mut proof_with_provider = proof.clone();
        proof_with_provider.provider_pk = provider_pk.to_string();

        let mut entry = self
            .pending
            .entry(provider_pk.to_string())
            .or_insert_with(EpochState::new);

        entry.total_tokens += proof_with_provider.token_count;

        // Cap at max_proofs_per_commitment
        if entry.proofs.len() >= self.config.max_proofs_per_commitment as usize {
            debug!(
                provider = %provider_pk,
                count = entry.proofs.len(),
                "Epoch at max capacity, dropping oldest proof"
            );
            entry.proofs.remove(0);
        }

        entry.proofs.push(proof_with_provider);
        debug!(
            provider = %provider_pk,
            epoch_size = entry.proofs.len(),
            total_tokens = entry.total_tokens,
            "Added proof to rollup epoch"
        );
    }

    /// Check if an epoch is ready to commit for the given provider.
    ///
    /// Returns true if:
    /// - Enough proofs accumulated (>= min_proofs_per_commitment), OR
    /// - Enough time elapsed since epoch start (>= epoch_duration_secs)
    pub fn should_commit(&self, provider_pk: &str) -> bool {
        if let Some(entry) = self.pending.get(provider_pk) {
            if entry.is_empty() {
                return false;
            }

            let proof_count = entry.proofs.len();
            let elapsed_secs = entry.epoch_start.elapsed().as_secs();

            proof_count >= self.config.min_proofs_per_commitment as usize
                || elapsed_secs >= self.config.epoch_duration_secs as u64
        } else {
            false
        }
    }

    /// Build and submit a commitment transaction for the given provider.
    ///
    /// Creates a commitment box with:
    /// - R4: provider_pk (SigmaProp)
    /// - R5: epoch start (Int)
    /// - R6: epoch end (Int)
    /// - R7: proof count (Int)
    /// - R8: Merkle root (Coll[Byte])
    /// - tokens(0): commitment NFT
    pub async fn build_commitment_tx(
        &self,
        client: &ErgoNodeClient,
        provider_pk: &str,
    ) -> Result<CommitmentResult> {
        // Drain proofs for this provider
        let (proofs, total_tokens) = {
            let mut entry = self
                .pending
                .get_mut(provider_pk)
                .context("No pending proofs for provider")?;

            if entry.is_empty() {
                anyhow::bail!("No pending proofs for provider {}", provider_pk);
            }

            let proofs = std::mem::take(&mut entry.proofs);
            let tokens = entry.total_tokens;
            entry.total_tokens = 0;
            entry.epoch_start = Instant::now();
            (proofs, tokens)
        };

        if proofs.is_empty() {
            anyhow::bail!("No proofs to commit");
        }

        // Build Merkle tree
        let leaf_data: Vec<Vec<u8>> = proofs.iter().map(|p| p.to_leaf_bytes()).collect();
        let leaf_refs: Vec<&[u8]> = leaf_data.iter().map(|d| d.as_slice()).collect();
        let tree = MerkleTree::from_data(&leaf_refs);
        let merkle_root = tree.root();

        // Get current block height
        let current_height = client
            .get_height()
            .await
            .context("Failed to get current block height")?;

        let epoch_start = current_height as i32 - (proofs.len() as i32).min(current_height as i32);
        let epoch_end = current_height as i32;

        // Encode register values
        let provider_pk_bytes = hex::decode(provider_pk).unwrap_or_default();
        let provider_pk_hex = encode_coll_byte(&provider_pk_bytes);
        let epoch_start_hex = encode_int(epoch_start);
        let epoch_end_hex = encode_int(epoch_end);
        let proof_count_hex = encode_int(proofs.len() as i32);
        let merkle_root_hex = encode_coll_byte(&merkle_root.0);

        // Build the commitment box request
        let commitment_tree = &self.config.commitment_tree_hex;
        if commitment_tree.is_empty() {
            anyhow::bail!("Commitment tree hex not configured — cannot submit commitment tx");
        }

        // Validate NFT token ID if configured
        if !self.config.commitment_nft_token_id.is_empty() {
            validate_token_id(&self.config.commitment_nft_token_id)
                .context("Invalid commitment NFT token ID")?;
        }

        let payment_request = serde_json::json!({
            "requests": [{
                "address": commitment_tree,
                "value": self.config.commitment_min_value_nanoerg.to_string(),
                "assets": [{
                    "tokenId": self.config.commitment_nft_token_id,
                    "amount": 1
                }],
                "registers": {
                    "R4": provider_pk_hex,
                    "R5": epoch_start_hex,
                    "R6": epoch_end_hex,
                    "R7": proof_count_hex,
                    "R8": merkle_root_hex
                }
            }],
            "fee": 1100000  // 0.0011 ERG fee
        });

        debug!(
            provider = %provider_pk,
            proof_count = proofs.len(),
            merkle_root = %merkle_root,
            "Submitting commitment transaction"
        );

        validate_payment_request(&payment_request)
            .context("Commitment transaction safety validation failed")?;

        let tx_id = client
            .wallet_payment_send(&payment_request)
            .await
            .context("Failed to submit commitment transaction via wallet")?;

        let result = CommitmentResult {
            tx_id: tx_id.clone(),
            proof_count: proofs.len(),
            merkle_root_hex: merkle_root.hex(),
            provider_pk: provider_pk.to_string(),
            timestamp: Utc::now().to_rfc3339(),
            total_tokens,
        };

        // Update stats
        self.total_committed
            .fetch_add(proofs.len() as u64, std::sync::atomic::Ordering::Relaxed);
        self.total_tokens_committed
            .fetch_add(total_tokens, std::sync::atomic::Ordering::Relaxed);

        // Save to recent commitments (keep last 100)
        {
            let mut recent = self.recent_commitments.lock().await;
            recent.push(result.clone());
            if recent.len() > 100 {
                recent.remove(0);
            }
        }

        info!(
            tx_id = %tx_id,
            provider = %provider_pk,
            proof_count = proofs.len(),
            total_tokens,
            merkle_root = %merkle_root,
            "Commitment transaction submitted"
        );

        Ok(result)
    }

    /// Spawn the background commitment loop.
    ///
    /// Periodically checks all providers for epochs ready to commit.
    pub fn spawn_commitment_loop(self: Arc<Self>, client: ErgoNodeClient) {
        let check_interval = Duration::from_secs(
            self.config
                .epoch_duration_secs
                .max(30)
                .min(3600) as u64, // Check at least every hour
        );

        tokio::spawn(async move {
            // Wait before first check
            tokio::time::sleep(check_interval).await;

            loop {
                // Collect providers with pending proofs
                let providers: Vec<String> = self
                    .pending
                    .iter()
                    .filter(|entry| self.should_commit(entry.key()))
                    .map(|entry| entry.key().clone())
                    .collect();

                for provider_pk in providers {
                    match self.build_commitment_tx(&client, &provider_pk).await {
                        Ok(result) => {
                            info!(
                                tx_id = %result.tx_id,
                                proof_count = result.proof_count,
                                "Epoch committed"
                            );
                        }
                        Err(e) => {
                            warn!(
                                provider = %provider_pk,
                                error = %e,
                                "Failed to commit epoch (will retry next cycle)"
                            );
                        }
                    }
                }

                tokio::time::sleep(check_interval).await;
            }
        });
    }

    /// Get the number of pending proofs across all providers.
    pub fn pending_count(&self) -> usize {
        self.pending.iter().map(|e| e.value().proofs.len()).sum()
    }

    /// Get total proofs committed.
    pub fn total_committed(&self) -> u64 {
        self.total_committed.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get total tokens committed.
    pub fn total_tokens_committed(&self) -> i64 {
        self.total_tokens_committed.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get recent commitments.
    pub async fn recent_commitments(&self) -> Vec<CommitmentResult> {
        self.recent_commitments.lock().await.clone()
    }

    /// Get pending proof count per provider.
    pub fn pending_per_provider(&self) -> HashMap<String, usize> {
        self.pending
            .iter()
            .map(|e| (e.key().clone(), e.value().proofs.len()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> RollupConfig {
        RollupConfig {
            enabled: true,
            epoch_duration_secs: 300,
            min_proofs_per_commitment: 5,
            max_proofs_per_commitment: 1000,
            commitment_tree_hex: String::new(),
            commitment_nft_token_id: String::new(),
            commitment_min_value_nanoerg: 1_000_000,
        }
    }

    #[test]
    fn test_usage_proof_to_leaf_bytes() {
        let proof = UsageProof {
            user_pk: "02abcdef".to_string(),
            provider_pk: "03fedcba".to_string(),
            model: "llama3-8b".to_string(),
            token_count: 100,
            timestamp_ms: 1700000000000,
            rarity_multiplier: 1.0,
        };

        let bytes = proof.to_leaf_bytes();
        let bytes_str = String::from_utf8_lossy(&bytes);
        assert!(bytes_str.contains("llama3-8b"));
    }

    #[test]
    fn test_epoch_should_commit_by_count() {
        let rollup = UsageRollup::new(make_config());
        let provider = "test_provider";

        // Add fewer than min
        for i in 0..4 {
            rollup.add_proof(
                provider,
                UsageProof {
                    user_pk: format!("02user{}", i),
                    provider_pk: provider.to_string(),
                    model: "test-model".to_string(),
                    token_count: 10,
                    timestamp_ms: 1700000000000 + i as i64,
                    rarity_multiplier: 1.0,
                },
            );
        }
        assert!(!rollup.should_commit(provider));

        // Add one more to reach min
        rollup.add_proof(
            provider,
            UsageProof {
                user_pk: "02user4".to_string(),
                provider_pk: provider.to_string(),
                model: "test-model".to_string(),
                token_count: 10,
                timestamp_ms: 1700000000004,
                rarity_multiplier: 1.0,
            },
        );
        assert!(rollup.should_commit(provider));
    }

    #[test]
    fn test_epoch_should_not_commit_empty() {
        let rollup = UsageRollup::new(make_config());
        assert!(!rollup.should_commit("nonexistent_provider"));
    }

    #[test]
    fn test_pending_count() {
        let rollup = UsageRollup::new(make_config());

        assert_eq!(rollup.pending_count(), 0);

        rollup.add_proof(
            "p1",
            UsageProof {
                user_pk: "02u1".to_string(),
                provider_pk: "p1".to_string(),
                model: "m".to_string(),
                token_count: 10,
                timestamp_ms: 1000,
                rarity_multiplier: 1.0,
            },
        );
        rollup.add_proof(
            "p2",
            UsageProof {
                user_pk: "02u2".to_string(),
                provider_pk: "p2".to_string(),
                model: "m".to_string(),
                token_count: 10,
                timestamp_ms: 1000,
                rarity_multiplier: 1.0,
            },
        );

        assert_eq!(rollup.pending_count(), 2);
    }

    #[test]
    fn test_max_proofs_per_commitment() {
        let mut config = make_config();
        config.max_proofs_per_commitment = 3;

        let rollup = UsageRollup::new(config);

        for i in 0..5 {
            rollup.add_proof(
                "p1",
                UsageProof {
                    user_pk: format!("02u{}", i),
                    provider_pk: "p1".to_string(),
                    model: "m".to_string(),
                    token_count: 10,
                    timestamp_ms: 1000 + i,
                    rarity_multiplier: 1.0,
                },
            );
        }

        // Should cap at max, dropping oldest
        assert_eq!(rollup.pending_count(), 3);
    }
}
