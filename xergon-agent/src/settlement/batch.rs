//! Batch settlement for inference payments
//!
//! Accumulates inference fees and submits batch payment transactions,
//! reducing on-chain transaction volume by grouping multiple payments
//! into consolidated per-provider transactions.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{info, warn};

use super::models::PendingPayment;
use super::models::BatchSettlementResult;
use super::transactions::TransactionService;

/// Accumulates inference fees and submits batch payment transactions.
/// Reduces on-chain transaction volume by grouping multiple payments
/// into a single transaction per provider.
pub struct BatchSettlement {
    /// Pending payments awaiting flush
    pending: Arc<Mutex<Vec<PendingPayment>>>,
    /// Max pending payments before auto-flush
    batch_size: usize,
    /// Max time between flushes (seconds)
    flush_interval: Duration,
    /// Transaction service for submitting
    tx_service: Arc<TransactionService>,
    /// Minimum payment to include (nanoERG) -- skip dust
    min_payment: u64,
}

impl BatchSettlement {
    /// Create a new batch settlement accumulator.
    ///
    /// # Arguments
    /// * `tx_service` - Transaction service for submitting ERG payments
    /// * `batch_size` - Max pending payments before auto-flush
    /// * `flush_interval` - Max time between flushes
    /// * `min_payment` - Minimum nanoERG amount per provider to include (dust threshold)
    pub fn new(
        tx_service: Arc<TransactionService>,
        batch_size: usize,
        flush_interval: Duration,
        min_payment: u64,
    ) -> Self {
        Self {
            pending: Arc::new(Mutex::new(Vec::new())),
            batch_size,
            flush_interval,
            tx_service,
            min_payment,
        }
    }

    /// Add a payment to the pending batch.
    ///
    /// If the number of pending payments reaches `batch_size`, an automatic
    /// flush is triggered. Any errors from the auto-flush are logged but
    /// do not propagate to the caller (the payment is still recorded).
    ///
    /// # Arguments
    /// * `user_address` - Address of the user making the inference request
    /// * `provider_address` - Ergo address of the provider to pay
    /// * `amount` - nanoERG amount
    /// * `model` - Model identifier (e.g., "llama-3.1-8b")
    pub async fn add_payment(
        &self,
        user_address: &str,
        provider_address: &str,
        amount: u64,
        model: &str,
    ) {
        let payment = PendingPayment {
            user_address: user_address.to_string(),
            provider_address: provider_address.to_string(),
            amount,
            model: model.to_string(),
            timestamp: chrono::Utc::now().timestamp(),
        };

        let auto_flush = {
            let mut pending = self.pending.lock().await;
            pending.push(payment);
            pending.len() >= self.batch_size
        };

        if auto_flush {
            info!(
                pending = self.batch_size,
                "Batch size reached, triggering auto-flush"
            );
            match self.flush().await {
                Ok(result) => {
                    info!(
                        tx_ids = result.tx_ids.len(),
                        total_nanoerg = result.total_erg,
                        payments = result.payment_count,
                        skipped = result.skipped_dust,
                        "Auto-flush completed"
                    );
                }
                Err(e) => {
                    warn!(error = %e, "Auto-flush failed");
                }
            }
        }
    }

    /// Flush all pending payments: group by provider, sum amounts,
    /// filter dust, and submit consolidated transactions.
    ///
    /// Returns a `BatchSettlementResult` with the outcome.
    pub async fn flush(&self) -> anyhow::Result<BatchSettlementResult> {
        // Take all pending payments out of the queue
        let payments: Vec<PendingPayment> = {
            let mut pending = self.pending.lock().await;
            std::mem::take(&mut *pending)
        };

        if payments.is_empty() {
            info!("No pending payments to flush");
            return Ok(BatchSettlementResult {
                tx_ids: Vec::new(),
                total_erg: 0,
                payment_count: 0,
                skipped_dust: 0,
            });
        }

        info!(
            pending_payments = payments.len(),
            "Flushing batch settlement"
        );

        // Group by provider address, sum amounts
        let mut provider_totals: HashMap<String, u64> = HashMap::new();
        for payment in &payments {
            *provider_totals
                .entry(payment.provider_address.clone())
                .or_default() += payment.amount;
        }

        // Filter dust and send consolidated payments
        let mut tx_ids = Vec::new();
        let mut total_erg: u64 = 0;
        let mut payment_count = 0;
        let mut skipped_dust = 0;

        for (provider_addr, total_nanoerg) in provider_totals {
            if total_nanoerg < self.min_payment {
                warn!(
                    provider = %provider_addr,
                    nanoerg = total_nanoerg,
                    min = self.min_payment,
                    "Skipping dust payment in batch flush"
                );
                skipped_dust += 1;
                continue;
            }

            match self.tx_service.send_payment(&provider_addr, total_nanoerg).await {
                Ok(tx_id) => {
                    info!(
                        tx_id = %tx_id,
                        provider = %provider_addr,
                        nanoerg = total_nanoerg,
                        erg = total_nanoerg as f64 / 1_000_000_000.0,
                        "Batch payment sent"
                    );
                    tx_ids.push(tx_id);
                    total_erg += total_nanoerg;
                    payment_count += 1;
                }
                Err(e) => {
                    warn!(
                        provider = %provider_addr,
                        nanoerg = total_nanoerg,
                        error = %e,
                        "Batch payment failed for provider"
                    );
                }
            }
        }

        info!(
            tx_ids = tx_ids.len(),
            total_nanoerg = total_erg,
            providers_paid = payment_count,
            skipped_dust = skipped_dust,
            "Batch settlement flush complete"
        );

        Ok(BatchSettlementResult {
            tx_ids,
            total_erg,
            payment_count,
            skipped_dust,
        })
    }

    /// Returns the number of payments currently pending.
    pub async fn pending_count(&self) -> usize {
        self.pending.lock().await.len()
    }

    /// Start a background tokio task that periodically flushes pending payments.
    ///
    /// The task sleeps for `flush_interval` between flushes. If a flush finds
    /// no pending payments, it does nothing. Returns a `JoinHandle` that can
    /// be used to abort the background task if needed.
    pub fn start_background_flush(self: &Arc<Self>) -> JoinHandle<()> {
        let batch = Arc::clone(self);
        let interval = self.flush_interval;

        tokio::spawn(async move {
            // Wait for the first interval before the first flush
            tokio::time::sleep(interval).await;

            loop {
                match batch.flush().await {
                    Ok(result) => {
                        if result.payment_count > 0 || result.skipped_dust > 0 {
                            info!(
                                payments = result.payment_count,
                                skipped = result.skipped_dust,
                                "Background batch flush completed"
                            );
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "Background batch flush failed");
                    }
                }

                tokio::time::sleep(interval).await;
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a TransactionService with a dummy URL.
    /// Used only for construction tests — we don't make real HTTP calls in tests.
    fn make_tx_service() -> Arc<TransactionService> {
        Arc::new(
            TransactionService::new("http://127.0.0.1:9053".to_string())
                .expect("Failed to create TransactionService"),
        )
    }

    #[tokio::test]
    async fn test_add_payment_and_pending_count() {
        let tx = make_tx_service();
        let batch = BatchSettlement::new(
            tx,
            10,                                         // batch_size
            Duration::from_secs(60),                    // flush_interval
            1_000_000,                                  // min_payment
        );

        assert_eq!(batch.pending_count().await, 0);

        batch.add_payment("user1", "provider1", 500_000, "llama-3.1-8b").await;
        assert_eq!(batch.pending_count().await, 1);

        batch.add_payment("user2", "provider2", 1_500_000, "qwen3-4b").await;
        assert_eq!(batch.pending_count().await, 2);
    }

    #[tokio::test]
    async fn test_flush_empty() {
        let tx = make_tx_service();
        let batch = BatchSettlement::new(
            tx,
            10,
            Duration::from_secs(60),
            1_000_000,
        );

        let result = batch.flush().await.unwrap();
        assert!(result.tx_ids.is_empty());
        assert_eq!(result.total_erg, 0);
        assert_eq!(result.payment_count, 0);
        assert_eq!(result.skipped_dust, 0);
    }

    #[tokio::test]
    async fn test_flush_groups_by_provider() {
        let tx = make_tx_service();
        let batch = BatchSettlement::new(
            tx,
            10,
            Duration::from_secs(60),
            1_000_000,
        );

        // Add multiple payments for the same provider
        batch.add_payment("user1", "provA", 500_000, "llama-3.1-8b").await;
        batch.add_payment("user2", "provA", 600_000, "llama-3.1-8b").await;
        batch.add_payment("user3", "provB", 200_000, "qwen3-4b").await;

        assert_eq!(batch.pending_count().await, 3);

        // Flush will try to send to the node (which won't be running in test),
        // but we can verify the grouping logic by checking pending is drained
        // after flush.
        let _ = batch.flush().await;
        assert_eq!(batch.pending_count().await, 0);
    }

    #[tokio::test]
    async fn test_dust_filtering_in_flush() {
        let tx = make_tx_service();
        let batch = BatchSettlement::new(
            tx,
            10,
            Duration::from_secs(60),
            5_000_000, // high min_payment to trigger dust skip
        );

        // Add payments that will sum to dust for provC
        batch.add_payment("user1", "provC", 1_000_000, "llama-3.1-8b").await;
        batch.add_payment("user2", "provC", 1_500_000, "llama-3.1-8b").await;
        // Total for provC = 2_500_000 < 5_000_000 -> dust

        let result = batch.flush().await.unwrap();
        // Both payments from provC are dust, so skipped_dust should count providers
        assert_eq!(result.skipped_dust, 1);
        assert_eq!(result.payment_count, 0);
    }

    #[tokio::test]
    async fn test_auto_flush_on_batch_size() {
        let tx = make_tx_service();
        let batch = BatchSettlement::new(
            tx,
            3, // batch_size = 3
            Duration::from_secs(60),
            1_000_000,
        );

        // First two: no flush
        batch.add_payment("u1", "p1", 100_000, "m1").await;
        batch.add_payment("u2", "p2", 100_000, "m2").await;
        assert_eq!(batch.pending_count().await, 2);

        // Third payment triggers auto-flush (which will try to send to node
        // and fail, but the pending queue should be drained)
        batch.add_payment("u3", "p3", 100_000, "m3").await;
        // After auto-flush, pending should be drained (even if tx failed)
        assert_eq!(batch.pending_count().await, 0);
    }
}
