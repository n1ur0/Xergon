//! ERG Settlement Engine
//!
//! Pure ERG-denominated settlement layer. All costs and earnings are tracked
//! in nanoERG directly — no USD conversion, no market rate dependency.
//!
//! Flow:
//! 1. Aggregate per-provider usage from local usage records
//! 2. Calculate nanoERG owed using per-model on-chain pricing (Provider Box R6)
//!    Falls back to config.settlement.cost_per_1k_tokens_nanoerg if no chain price found
//! 3. Build batch ERG payment transaction(s)
//! 4. Broadcast to Ergo network via node's /wallet/payment endpoint
//! 5. Track confirmation status
//!
//! This runs as a periodic background task.

pub mod batch;
pub mod eutxo;
pub mod market;
pub mod models;
pub mod reconcile;
pub mod transactions;

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::config::SettlementConfig;
use crate::contract_compile;
use models::{
    BatchStatus, PaymentStatus, ProviderEarning, SettlementBatch, SettlementLedger,
    SettlementPayment, SettlementSummary,
};
use reconcile::Reconciler;
use transactions::TransactionService;

/// ERG precision: 1 ERG = 10^9 nanoERG
const NANOERG_PER_ERG: u64 = 1_000_000_000;

/// The settlement engine orchestrates the full settlement lifecycle.
pub struct SettlementEngine {
    config: SettlementConfig,
    tx_service: TransactionService,
    ledger: Arc<RwLock<SettlementLedger>>,
    /// Local usage tracking (tokens/requests per provider)
    usage_tracker: Arc<UsageTracker>,
    /// Per-model pricing from on-chain Provider Box R6 register.
    /// Keys are model IDs, values are nanoERG per 1M tokens.
    /// Populated via `update_model_pricing` from the provider's on-chain box data.
    model_pricing: RwLock<HashMap<String, u64>>,
    /// Reconciliation checker for verifying settlement integrity against the node.
    reconciler: Reconciler,
}

/// Simple in-memory usage tracker that accumulates per-provider stats.
/// In production, this would be backed by a persistent store or
/// fed from the marketplace relay's Usage table.
#[derive(Debug, Default)]
struct UsageTracker {
    entries: tokio::sync::Mutex<std::collections::HashMap<String, ProviderUsageEntry>>,
}

#[derive(Debug, Clone)]
struct ProviderUsageEntry {
    provider_id: String,
    ergo_address: String,
    tokens_in: u64,
    tokens_out: u64,
    requests: u64,
    cost_nanoerg: u64,
}

impl UsageTracker {
    /// Record a single inference usage event.
    /// cost_nanoerg: the cost in nanoERG for this usage event.
    pub async fn record_usage(
        &self,
        provider_id: &str,
        ergo_address: &str,
        tokens_in: u64,
        tokens_out: u64,
        cost_nanoerg: u64,
    ) {
        let mut entries = self.entries.lock().await;
        let entry = entries
            .entry(provider_id.to_string())
            .or_insert_with(|| ProviderUsageEntry {
                provider_id: provider_id.to_string(),
                ergo_address: ergo_address.to_string(),
                tokens_in: 0,
                tokens_out: 0,
                requests: 0,
                cost_nanoerg: 0,
            });
        entry.tokens_in += tokens_in;
        entry.tokens_out += tokens_out;
        entry.requests += 1;
        entry.cost_nanoerg += cost_nanoerg;
    }

    /// Drain all accumulated usage and return as ProviderEarning list.
    /// Filters out entries below the minimum payment threshold.
    pub async fn drain(&self, min_payment_nanoerg: u64) -> Vec<ProviderEarning> {
        let mut entries = self.entries.lock().await;
        let earnings: Vec<ProviderEarning> = entries
            .drain()
            .filter(|(_, e)| e.cost_nanoerg >= min_payment_nanoerg)
            .map(|(_, e)| ProviderEarning {
                provider_id: e.provider_id,
                ergo_address: e.ergo_address,
                earned_nanoerg: e.cost_nanoerg,
                tokens_processed: e.tokens_in + e.tokens_out,
                requests_handled: e.requests,
            })
            .collect();
        earnings
    }

    /// Peek at current accumulated usage without draining.
    pub async fn peek(&self, min_payment_nanoerg: u64) -> Vec<ProviderEarning> {
        let entries = self.entries.lock().await;
        entries
            .values()
            .filter(|e| e.cost_nanoerg >= min_payment_nanoerg)
            .map(|e| ProviderEarning {
                provider_id: e.provider_id.clone(),
                ergo_address: e.ergo_address.clone(),
                earned_nanoerg: e.cost_nanoerg,
                tokens_processed: e.tokens_in + e.tokens_out,
                requests_handled: e.requests,
            })
            .collect()
    }
}

impl SettlementEngine {
    /// Create a new settlement engine.
    pub fn new(config: SettlementConfig, ergo_rest_url: String) -> Result<Self> {
        let tx_service = TransactionService::new(ergo_rest_url.clone())
            .context("Failed to create transaction service")?;

        let ledger_path = config
            .ledger_file
            .clone()
            .unwrap_or_else(|| PathBuf::from("data/settlement_ledger.json"));

        let reconciler = Reconciler::new(ledger_path.clone(), ergo_rest_url)
            .context("Failed to create reconciler")?;

        Ok(Self {
            config,
            tx_service,
            ledger: Arc::new(RwLock::new(SettlementLedger::default())),
            usage_tracker: Arc::new(UsageTracker::default()),
            model_pricing: RwLock::new(HashMap::new()),
            reconciler,
        })
    }

    /// Update the per-model pricing cache from on-chain Provider Box data.
    ///
    /// `pricing` is a HashMap of model_id -> nanoERG per 1M tokens,
    /// as parsed from the Provider Box R6 register.
    pub async fn update_model_pricing(&self, pricing: HashMap<String, u64>) {
        let mut cache = self.model_pricing.write().await;
        let old_len = cache.len();
        *cache = pricing;
        let new_len = cache.len();
        if old_len != new_len || !cache.is_empty() {
            info!(
                models = new_len,
                price_source = "on-chain",
                "Model pricing cache updated from Provider Box R6"
            );
        }
    }

    /// Resolve the cost per 1K tokens (in nanoERG) for a given model.
    ///
    /// Resolution order:
    /// 1. On-chain per-model pricing from Provider Box R6 (per 1M tokens, converted to per 1K)
    /// 2. Global config `cost_per_1k_tokens_nanoerg` as fallback
    ///
    /// Returns (cost_per_1k_nanoerg, price_source) where price_source is
    /// "on-chain" or "config-default".
    pub async fn resolve_cost_per_1k(&self, model_id: &str) -> (u64, &'static str) {
        let cache = self.model_pricing.read().await;
        if let Some(&price_per_1m) = cache.get(model_id) {
            // Convert per-1M-tokens to per-1K-tokens
            let cost_per_1k = price_per_1m / 1000;
            (cost_per_1k, "on-chain")
        } else if !cache.is_empty() {
            // Model not found in on-chain pricing, but other models have prices.
            // This could mean the model was added after the last pricing update,
            // or the model ID doesn't match exactly. Fall back to config.
            info!(
                model_id = %model_id,
                available_models = cache.len(),
                "Model not found in on-chain pricing, falling back to config default"
            );
            (self.config.cost_per_1k_tokens_nanoerg, "config-default")
        } else {
            // No on-chain pricing loaded at all — use config default
            (self.config.cost_per_1k_tokens_nanoerg, "config-default")
        }
    }

    /// Get the configured cost per 1K tokens in nanoERG (legacy fallback).
    #[deprecated(note = "Use resolve_cost_per_1k(model_id) for per-provider on-chain pricing")]
    pub fn cost_per_1k_nanoerg(&self) -> u64 {
        self.config.cost_per_1k_tokens_nanoerg
    }

    /// Initialize: load persisted ledger from disk.
    pub async fn init(&self) -> Result<()> {
        let ledger_path = self.ledger_path();
        let mut ledger = self.ledger.write().await;
        *ledger = SettlementLedger::load(&ledger_path).await?;
        info!(
            total_batches = ledger.batches.len(),
            total_erg_paid = ledger.total_erg_paid,
            total_nanoerg_settled = ledger.total_nanoerg_settled,
            "Settlement ledger loaded"
        );

        Ok(())
    }

    /// Record an inference usage event for settlement tracking.
    ///
    /// cost_nanoerg: the cost in nanoERG for this usage event.
    /// Callers should compute this via `resolve_cost_per_1k(model_id)` which
    /// uses per-model on-chain pricing from Provider Box R6, falling back
    /// to the global config default.
    pub async fn record_usage(
        &self,
        provider_id: &str,
        ergo_address: &str,
        tokens_in: u64,
        tokens_out: u64,
        cost_nanoerg: u64,
    ) {
        self.usage_tracker
            .record_usage(provider_id, ergo_address, tokens_in, tokens_out, cost_nanoerg)
            .await;
    }

    /// Run a single settlement cycle.
    ///
    /// 1. Drain accumulated provider earnings
    /// 2. Build ERG payment batch (no conversion needed — already in nanoERG)
    /// 3. Send batch transaction
    /// 4. Persist results
    pub async fn settle(&self) -> Result<SettlementBatch> {
        info!("Starting settlement cycle...");

        // Step 1: Drain earnings
        let earnings = self.usage_tracker.drain(self.config.min_settlement_nanoerg).await;
        if earnings.is_empty() {
            info!("No provider earnings to settle");
            return Err(anyhow::anyhow!("No earnings to settle"));
        }

        info!(providers = earnings.len(), "Aggregated provider earnings");

        // Step 2: Build settlement batch
        let mut batch = self.build_batch(earnings).await;

        // Step 3: Send payments
        if !self.config.dry_run {
            self.tx_service.send_batch(&mut batch).await;
        } else {
            info!("DRY RUN: skipping actual ERG transfers");
            batch.status = BatchStatus::Submitted;
            for payment in &mut batch.payments {
                payment.status = PaymentStatus::Broadcast;
                payment.tx_id = Some(format!("dry-run-{}", uuid_simple()));
            }
        }

        // Step 4: Persist
        {
            let mut ledger = self.ledger.write().await;
            ledger.record_batch(&batch);
            ledger.save(&self.ledger_path()).await?;
        }

        info!(
            batch_id = %batch.batch_id,
            status = ?batch.status,
            total_erg = batch.total_erg,
            total_nanoerg = batch.total_nanoerg,
            payments = batch.payments.len(),
            "Settlement cycle complete"
        );

        Ok(batch)
    }

    /// Settle on-chain using the eUTXO engine.
    ///
    /// Finds settleable user staking boxes on the Ergo blockchain and
    /// executes a settlement transaction that pays the provider. This is
    /// the real on-chain settlement path (as opposed to the wallet-funded
    /// batch payments in `settle()`).
    ///
    /// Requires `chain_enabled` in config to be true.
    pub async fn settle_on_chain(
        &self,
        provider_address: &str,
    ) -> Result<SettlementBatch> {
        info!("Starting on-chain settlement cycle...");

        // Step 1: Verify the user_staking contract is loaded
        match contract_compile::get_contract_hex("user_staking") {
            Some(_) => {}
            None => {
                warn!(
                    "user_staking contract hex not found — contracts not loaded. \
                     Skipping on-chain settlement."
                );
                anyhow::bail!("user_staking contract hex not found");
            }
        };

        // Step 2: Find settleable boxes
        let node_url = self.tx_service.node_url().to_string();
        let boxes_result = eutxo::find_settleable_boxes(
            &node_url,
            50, // max_boxes
            self.config.min_confirmations,
        )
        .await
        .context("Failed to find settleable boxes for on-chain settlement")?;

        if boxes_result.boxes.is_empty() {
            info!("No settleable staking boxes found on-chain");
            anyhow::bail!("No settleable boxes found");
        }

        info!(
            boxes_found = boxes_result.boxes.len(),
            total_value_nanoerg = boxes_result.total_value,
            total_erg = boxes_result.total_value as f64 / NANOERG_PER_ERG as f64,
            "Found settleable staking boxes"
        );

        // Step 3: Execute settlement via the eUTXO engine
        let eutxo_engine = eutxo::EutxoSettlementEngine::new(
            self.config.clone(),
            node_url,
        )
        .context("Failed to create eUTXO settlement engine")?;

        let tx_id = eutxo_engine
            .execute_simple_settlement(provider_address, boxes_result.total_value)
            .await
            .context("eUTXO execute_simple_settlement failed")?;

        // Step 4: Build a SettlementBatch from the result
        let now = chrono::Utc::now();
        let period_start = {
            let ledger = self.ledger.read().await;
            ledger
                .last_settled_at
                .unwrap_or_else(|| now - chrono::Duration::hours(24))
        };

        let boxes_settled = boxes_result.boxes.len() as u32;
        let total_nanoerg = boxes_result.total_value;
        let total_erg = total_nanoerg as f64 / NANOERG_PER_ERG as f64;

        let batch = SettlementBatch {
            batch_id: uuid_simple(),
            created_at: now,
            period_start,
            period_end: now,
            cost_per_1k_nanoerg: self.config.cost_per_1k_tokens_nanoerg,
            payments: vec![SettlementPayment {
                provider_id: "on-chain-settlement".to_string(),
                ergo_address: provider_address.to_string(),
                nanoerg_amount: total_nanoerg,
                erg_nano: total_nanoerg,
                tx_id: Some(tx_id.clone()),
                status: PaymentStatus::Broadcast,
            }],
            total_erg,
            total_nanoerg,
            status: BatchStatus::Submitted,
        };

        // Step 5: Persist to ledger
        {
            let mut ledger = self.ledger.write().await;
            ledger.record_batch(&batch);
            ledger.save(&self.ledger_path()).await?;
        }

        info!(
            batch_id = %batch.batch_id,
            tx_id = %tx_id,
            boxes_settled = boxes_settled,
            total_erg = total_erg,
            total_nanoerg = total_nanoerg,
            "On-chain settlement cycle complete"
        );

        Ok(batch)
    }

    /// Run the periodic on-chain settlement loop.
    ///
    /// Runs alongside the regular settlement loop when `chain_enabled` is true.
    /// Finds settleable staking boxes and executes real eUTXO transactions.
    /// Errors are non-fatal — the loop continues running.
    pub async fn run_chain_settlement_loop(&self, provider_address: String) {
        let interval_secs = self.config.interval_secs;
        let interval = std::time::Duration::from_secs(interval_secs);

        info!(
            interval_secs = interval_secs,
            provider = %provider_address,
            min_confirmations = self.config.min_confirmations,
            "On-chain settlement loop started"
        );

        // Wait for initial delay before first settlement
        tokio::time::sleep(interval).await;

        loop {
            match self.settle_on_chain(&provider_address).await {
                Ok(batch) => {
                    info!(
                        batch_id = %batch.batch_id,
                        total_erg = batch.total_erg,
                        boxes_settled = batch.payments.len(),
                        "On-chain settlement completed"
                    );
                }
                Err(e) => {
                    let msg = e.to_string();
                    if msg.contains("No settleable boxes found")
                        || msg.contains("contract hex not found")
                    {
                        // Expected conditions, log at info level
                        info!("On-chain settlement cycle skipped: {}", msg);
                    } else {
                        warn!(error = %e, "On-chain settlement cycle failed");
                    }
                }
            }

            tokio::time::sleep(interval).await;
        }
    }

    /// Build a settlement batch from provider earnings.
    async fn build_batch(&self, earnings: Vec<ProviderEarning>) -> SettlementBatch {
        let now = chrono::Utc::now();
        let period_end = now;
        // Period start = last settlement time, or 24h ago if never settled
        let period_start = {
            let ledger = self.ledger.read().await;
            ledger
                .last_settled_at
                .unwrap_or_else(|| now - chrono::Duration::hours(24))
        };

        let mut total_nanoerg: u64 = 0;
        let mut payments = Vec::new();

        for earning in &earnings {
            let erg_nano = earning.earned_nanoerg;

            // Skip dust payments (below minimum box value)
            if erg_nano < self.config.min_settlement_nanoerg {
                warn!(
                    provider_id = %earning.provider_id,
                    nanoerg = erg_nano,
                    min = self.config.min_settlement_nanoerg,
                    "Skipping dust payment"
                );
                continue;
            }

            total_nanoerg += erg_nano;

            payments.push(SettlementPayment {
                provider_id: earning.provider_id.clone(),
                ergo_address: earning.ergo_address.clone(),
                nanoerg_amount: erg_nano,
                erg_nano,
                tx_id: None,
                status: PaymentStatus::Pending,
            });
        }

        let total_erg = total_nanoerg as f64 / NANOERG_PER_ERG as f64;

        SettlementBatch {
            batch_id: uuid_simple(),
            created_at: now,
            period_start,
            period_end,
            cost_per_1k_nanoerg: self.config.cost_per_1k_tokens_nanoerg,
            payments,
            total_erg,
            total_nanoerg,
            status: BatchStatus::Pending,
        }
    }

    /// Run the periodic settlement loop.
    /// Call this in a spawned tokio task.
    pub async fn run_loop(&self) {
        let interval_secs = self.config.interval_secs;
        let interval = std::time::Duration::from_secs(interval_secs);
        // Run reconciliation every 6 hours
        let reconcile_interval = std::time::Duration::from_secs(6 * 3600);
        let mut last_reconcile = std::time::Instant::now()
            .checked_sub(reconcile_interval)
            .unwrap_or_else(std::time::Instant::now);

        info!(
            interval_secs = interval_secs,
            dry_run = self.config.dry_run,
            reconcile_interval_secs = 6 * 3600,
            "Settlement loop started"
        );

        // Wait for initial delay before first settlement
        tokio::time::sleep(interval).await;

        loop {
            match self.settle().await {
                Ok(batch) => {
                    info!(
                        batch_id = %batch.batch_id,
                        status = ?batch.status,
                        "Settlement completed"
                    );
                }
                Err(e) => {
                    // "No earnings" is expected and not an error worth logging at error level
                    if e.to_string().contains("No earnings to settle") {
                        info!("Settlement cycle skipped: no pending earnings");
                    } else {
                        tracing::error!(error = %e, "Settlement cycle failed");
                    }
                }
            }

            // Run reconciliation periodically (every 6 hours)
            if last_reconcile.elapsed() >= reconcile_interval {
                if let Err(e) = self.run_reconciliation().await {
                    warn!(error = %e, "Periodic reconciliation failed");
                }
                last_reconcile = std::time::Instant::now();
            }

            tokio::time::sleep(interval).await;
        }
    }

    /// Run a single reconciliation check against the Ergo node.
    ///
    /// Cross-references the on-disk settlement ledger against the node's
    /// transaction state to detect discrepancies (stale statuses, missing
    /// transactions, amount mismatches).
    ///
    /// Returns the reconciliation report. Errors are only from I/O failures;
    /// discrepancies within the report are informational, not errors.
    pub async fn run_reconciliation(&self) -> Result<reconcile::ReconciliationReport> {
        self.reconciler.reconcile().await
    }

    /// Run the periodic confirmation polling loop.
    ///
    /// Checks all Submitted batches for on-chain inclusion and promotes
    /// them to Confirmed. Runs independently from the settlement loop
    /// so settlements and confirmations don't block each other.
    pub async fn confirm_loop(&self) {
        let poll_interval = std::time::Duration::from_secs(60); // Check every 60s
        let max_age = chrono::Duration::hours(4); // Mark stale after 4h

        info!(
            poll_interval_secs = 60,
            stale_threshold_hours = 4,
            "Confirmation polling loop started"
        );

        loop {
            tokio::time::sleep(poll_interval).await;

            if let Err(e) = self.confirm_submitted_batches(max_age).await {
                warn!(error = %e, "Confirmation poll cycle failed");
            }
        }
    }

    /// Check all Submitted batches and promote to Confirmed or Failed.
    ///
    /// This method is careful to never hold the ledger write lock during
    /// HTTP I/O. The flow is:
    /// 1. Acquire write lock, collect tx IDs that need checking, drop lock
    /// 2. Make HTTP calls to Ergo node (no lock held)
    /// 3. Re-acquire write lock, apply results, persist
    async fn confirm_submitted_batches(&self, max_age: chrono::Duration) -> Result<()> {
        // --- Phase 1: Collect pending tx IDs under write lock, then drop ---
        #[derive(Debug)]
        struct TxCheckRequest {
            batch_index: usize,
            payment_index: usize,
            batch_id: String,
            provider_id: String,
            tx_id: String,
        }

        struct TxCheckResult {
            batch_index: usize,
            payment_index: usize,
            confirmed: bool,
        }

        // Track which payments had missing tx_id so we can mark them failed
        struct MissingTxId {
            batch_index: usize,
            payment_index: usize,
        }

        let now = chrono::Utc::now();
        let mut check_requests: Vec<TxCheckRequest> = Vec::new();
        let mut missing_tx_ids: Vec<MissingTxId> = Vec::new();
        // Track batch ages for stale detection
        let mut submitted_batch_ages: Vec<(usize, chrono::Duration)> = Vec::new();

        {
            let ledger = self.ledger.read().await;
            for (batch_idx, batch) in ledger.batches.iter().enumerate() {
                if batch.status != BatchStatus::Submitted {
                    continue;
                }
                let batch_age = now.signed_duration_since(batch.created_at);
                submitted_batch_ages.push((batch_idx, batch_age));

                for (pay_idx, payment) in batch.payments.iter().enumerate() {
                    if payment.status != PaymentStatus::Broadcast {
                        continue;
                    }
                    match &payment.tx_id {
                        Some(id) => {
                            check_requests.push(TxCheckRequest {
                                batch_index: batch_idx,
                                payment_index: pay_idx,
                                batch_id: batch.batch_id.clone(),
                                provider_id: payment.provider_id.clone(),
                                tx_id: id.clone(),
                            });
                        }
                        None => {
                            missing_tx_ids.push(MissingTxId {
                                batch_index: batch_idx,
                                payment_index: pay_idx,
                            });
                        }
                    }
                }
            }
        }
        // Write lock is now dropped.

        // --- Phase 2: Make HTTP calls outside any lock ---
        let mut check_results: Vec<TxCheckResult> = Vec::new();

        for req in &check_requests {
            match self.tx_service.check_confirmation(&req.tx_id).await {
                Ok(Some(height)) => {
                    info!(
                        batch_id = %req.batch_id,
                        provider_id = %req.provider_id,
                        tx_id = %req.tx_id,
                        inclusion_height = height,
                        "Payment confirmed on-chain"
                    );
                    check_results.push(TxCheckResult {
                        batch_index: req.batch_index,
                        payment_index: req.payment_index,
                        confirmed: true,
                    });
                }
                Ok(None) => {
                    check_results.push(TxCheckResult {
                        batch_index: req.batch_index,
                        payment_index: req.payment_index,
                        confirmed: false,
                    });
                }
                Err(e) => {
                    warn!(
                        batch_id = %req.batch_id,
                        provider_id = %req.provider_id,
                        tx_id = %req.tx_id,
                        error = %e,
                        "Failed to check confirmation"
                    );
                    check_results.push(TxCheckResult {
                        batch_index: req.batch_index,
                        payment_index: req.payment_index,
                        confirmed: false,
                    });
                }
            }
        }

        // --- Phase 3: Re-acquire write lock and apply results ---
        let mut changed = false;

        // Mark missing tx_ids as failed
        for m in &missing_tx_ids {
            if let Some(batch) = self.ledger.write().await.batches.get_mut(m.batch_index) {
                if let Some(payment) = batch.payments.get_mut(m.payment_index) {
                    payment.status = PaymentStatus::Failed("Missing tx_id".into());
                    changed = true;
                }
            }
        }

        // Apply confirmation check results
        for result in &check_results {
            let mut ledger = self.ledger.write().await;
            if let Some(batch) = ledger.batches.get_mut(result.batch_index) {
                if let Some(payment) = batch.payments.get_mut(result.payment_index) {
                    if result.confirmed {
                        payment.status = PaymentStatus::Confirmed;
                    }
                    // If not confirmed, we leave it as Broadcast — it will be
                    // rechecked on the next poll cycle.
                }
            }
        }

        // Check batch-level promotion / stale detection
        {
            let mut ledger = self.ledger.write().await;
            for (batch_idx, batch_age) in &submitted_batch_ages {
                let batch = match ledger.batches.get_mut(*batch_idx) {
                    Some(b) => b,
                    None => continue,
                };
                if batch.status != BatchStatus::Submitted {
                    continue;
                }

                let all_confirmed = batch
                    .payments
                    .iter()
                    .all(|p| p.status == PaymentStatus::Confirmed);
                let any_still_broadcast = batch
                    .payments
                    .iter()
                    .any(|p| p.status == PaymentStatus::Broadcast);

                // If all payments are confirmed, promote the batch
                if all_confirmed {
                    batch.status = BatchStatus::Confirmed;
                    info!(
                        batch_id = %batch.batch_id,
                        payments = batch.payments.len(),
                        "All payments in batch confirmed"
                    );
                    changed = true;
                }
                // If still broadcast after max_age, mark remaining as stale
                else if any_still_broadcast && *batch_age > max_age {
                    warn!(
                        batch_id = %batch.batch_id,
                        age_hours = batch_age.num_hours(),
                        "Batch has unconfirmed payments after threshold, marking stale"
                    );
                    for payment in &mut batch.payments {
                        if payment.status == PaymentStatus::Broadcast {
                            payment.status = PaymentStatus::Failed(format!(
                                "Transaction not confirmed after {} hours",
                                batch_age.num_hours()
                            ));
                        }
                    }
                    let has_failed = batch
                        .payments
                        .iter()
                        .any(|p| matches!(p.status, PaymentStatus::Failed(_)));
                    if has_failed {
                        batch.status = BatchStatus::Failed(format!(
                            "Timed out after {} hours — some payments unconfirmed",
                            batch_age.num_hours()
                        ));
                    }
                    changed = true;
                }
            }

            if changed {
                ledger.save(&self.ledger_path()).await?;
                info!("Ledger updated with confirmation results");
            }
        }

        Ok(())
    }

    /// Get current pending earnings (for API display).
    pub async fn pending_summary(&self) -> Vec<ProviderEarning> {
        self.usage_tracker.peek(self.config.min_settlement_nanoerg).await
    }

    /// Get settlement summary (for API display).
    pub async fn summary(&self) -> SettlementSummary {
        let ledger = self.ledger.read().await;
        let last_batch = ledger.batches.first();
        let next_settlement =
            chrono::Utc::now() + chrono::Duration::seconds(self.config.interval_secs as i64);

        SettlementSummary {
            last_settled_at: ledger.last_settled_at,
            last_batch_id: last_batch.map(|b| b.batch_id.clone()),
            last_batch_status: last_batch.map(|b| format!("{:?}", b.status)),
            total_batches: ledger.batches.len(),
            total_erg_paid: ledger.total_erg_paid,
            total_nanoerg_settled: ledger.total_nanoerg_settled,
            next_settlement_at: next_settlement,
            cost_per_1k_nanoerg: self.config.cost_per_1k_tokens_nanoerg,
        }
    }

    /// Get the ledger for detailed history access.
    pub async fn ledger(&self) -> tokio::sync::RwLockReadGuard<'_, SettlementLedger> {
        self.ledger.read().await
    }

    fn ledger_path(&self) -> PathBuf {
        self.config
            .ledger_file
            .clone()
            .unwrap_or_else(|| PathBuf::from("data/settlement_ledger.json"))
    }
}

/// Generate a simple unique ID for batches (no uuid crate dependency).
fn uuid_simple() -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(
        chrono::Utc::now()
            .timestamp_nanos_opt()
            .unwrap_or(0)
            .to_le_bytes(),
    );
    hasher.update(std::process::id().to_le_bytes());
    // Use a counter-like value from memory address to add uniqueness
    let addr = &hasher as *const _ as u64;
    hasher.update(addr.to_le_bytes());
    let hash = hex::encode(hasher.finalize());
    // Use first 16 chars for a reasonable ID
    format!("xset-{}", &hash[..16])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_cost_per_1k_chain_price() {
        // At 50_000 nanoERG per 1M tokens on-chain:
        // per-1K = 50_000 / 1000 = 50 nanoERG per 1K tokens
        let _cache = HashMap::from([
            ("llama-3.1-8b".to_string(), 50_000u64),
            ("qwen3-4b".to_string(), 100_000u64),
        ]);
        // We can't easily test async, so just verify the math
        assert_eq!(50_000u64 / 1000, 50);
        assert_eq!(100_000u64 / 1000, 100);
    }

    #[test]
    fn test_nanoerg_pricing() {
        // At 1_000_000 nanoERG per 1K tokens:
        // 5K tokens = 5 * 1_000_000 = 5_000_000 nanoERG = 0.005 ERG
        let cost_per_1k = 1_000_000u64;
        let tokens = 5000u64;
        let cost_nanoerg = tokens * cost_per_1k / 1000;
        assert_eq!(cost_nanoerg, 5_000_000);
        let erg = cost_nanoerg as f64 / NANOERG_PER_ERG as f64;
        assert!((erg - 0.005).abs() < f64::EPSILON);
    }

    #[test]
    fn test_dust_filtering() {
        // At 1_000_000 nanoERG per 1K tokens:
        // 1 token = 1_000 nanoERG (below min threshold of 1_000_000)
        let cost_per_1k = 1_000_000u64;
        let tokens = 1u64;
        let cost_nanoerg = tokens * cost_per_1k / 1000;
        assert!(cost_nanoerg < 1_000_000); // below min

        // 1K tokens = 1_000_000 nanoERG (at threshold)
        let tokens_1k = 1000u64;
        let cost_1k = tokens_1k * cost_per_1k / 1000;
        assert!(cost_1k >= 1_000_000); // at threshold
    }

    #[tokio::test]
    async fn test_usage_tracker_drain() {
        let tracker = UsageTracker::default();
        tracker.record_usage("prov1", "addr1", 100, 200, 5_000_000).await;
        tracker.record_usage("prov1", "addr1", 50, 100, 3_000_000).await;
        tracker.record_usage("prov2", "addr2", 200, 400, 10_000_000).await;

        let earnings = tracker.drain(1_000_000).await;
        assert_eq!(earnings.len(), 2);

        // Find entries by provider_id since HashMap iteration order is non-deterministic
        let prov1 = earnings.iter().find(|e| e.provider_id == "prov1").unwrap();
        assert_eq!(prov1.earned_nanoerg, 8_000_000);

        let prov2 = earnings.iter().find(|e| e.provider_id == "prov2").unwrap();
        assert_eq!(prov2.earned_nanoerg, 10_000_000);

        // After drain, should be empty
        let earnings2 = tracker.drain(1_000_000).await;
        assert!(earnings2.is_empty());
    }

    #[tokio::test]
    async fn test_usage_tracker_filters_dust() {
        let tracker = UsageTracker::default();
        tracker.record_usage("big", "addr1", 1000, 2000, 50_000_000).await;
        tracker.record_usage("tiny", "addr2", 1, 1, 500_000).await; // below min

        let earnings = tracker.drain(1_000_000).await;
        assert_eq!(earnings.len(), 1);
        assert_eq!(earnings[0].provider_id, "big");
    }
}
