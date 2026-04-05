//! Usage proof accumulator — batches proofs and submits them on-chain.
//!
//! Inference completions add proofs to an in-memory queue. A background task
//! periodically drains the queue and submits a batched transaction via the
//! Ergo node wallet API.

use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::chain::client::ErgoNodeClient;
use crate::chain::transactions::{self, PendingUsageProof};
use crate::config::ChainTxConfig;

/// Accumulates usage proofs and periodically submits them on-chain.
pub struct UsageProofAccumulator {
    config: ChainTxConfig,
    client: ErgoNodeClient,
    /// Queue of pending proofs waiting to be submitted.
    queue: Arc<Mutex<Vec<PendingUsageProof>>>,
    /// Total tokens served (for heartbeat tx R5 register).
    total_tokens: Arc<std::sync::atomic::AtomicI64>,
    /// Total requests served (for heartbeat tx R6 register).
    total_requests: Arc<std::sync::atomic::AtomicI64>,
}

impl UsageProofAccumulator {
    /// Create a new accumulator.
    pub fn new(config: ChainTxConfig, client: ErgoNodeClient) -> Self {
        Self {
            config,
            client,
            queue: Arc::new(Mutex::new(Vec::new())),
            total_tokens: Arc::new(std::sync::atomic::AtomicI64::new(0)),
            total_requests: Arc::new(std::sync::atomic::AtomicI64::new(0)),
        }
    }

    /// Add a usage proof to the queue (called after each inference completion).
    pub async fn add_proof(&self, proof: PendingUsageProof) {
        self.total_tokens
            .fetch_add(proof.token_count, std::sync::atomic::Ordering::Relaxed);
        self.total_requests
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        if !self.config.usage_proof_tx_enabled {
            return;
        }

        let mut queue = self.queue.lock().await;
        queue.push(proof);

        // Don't let the queue grow unbounded
        if queue.len() > 1000 {
            let drop_count = queue.len() - 500;
            warn!(
                queue_len = queue.len(),
                drop_count,
                "Usage proof queue is very large — dropping oldest proofs"
            );
            queue.drain(..drop_count);
        }
    }

    /// Get the total tokens served so far.
    pub fn total_tokens(&self) -> i64 {
        self.total_tokens.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Get the total requests served so far.
    pub fn total_requests(&self) -> i64 {
        self.total_requests.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Drain the queue and submit all pending proofs as a batched transaction.
    ///
    /// Called by the background batch task. Returns the number of proofs submitted.
    pub async fn flush(&self) -> usize {
        let proofs: Vec<PendingUsageProof> = {
            let mut queue = self.queue.lock().await;
            std::mem::take(&mut *queue)
        };

        if proofs.is_empty() {
            return 0;
        }

        match transactions::submit_usage_proof_batch(
            &self.client,
            &proofs,
            &self.config.usage_proof_tree_hex,
            self.config.usage_proof_min_value_nanoerg,
        )
        .await
        {
            Ok(tx_id) => {
                info!(
                    tx_id = %tx_id,
                    count = proofs.len(),
                    "Flushed usage proof batch"
                );
            }
            Err(e) => {
                warn!(
                    error = %e,
                    count = proofs.len(),
                    "Failed to flush usage proof batch — proofs dropped"
                );
            }
        }

        proofs.len()
    }

    /// Spawn the background batch submission task.
    pub fn spawn_batch_loop(self: Arc<Self>) {
        let interval_secs = self.config.usage_proof_batch_interval_secs;

        tokio::spawn(async move {
            let interval = std::time::Duration::from_secs(interval_secs.max(10));

            // Wait before first flush
            tokio::time::sleep(interval).await;

            loop {
                let count = self.flush().await;
                if count > 0 {
                    info!(count, "Batch flush complete");
                }

                tokio::time::sleep(interval).await;
            }
        });
    }
}
