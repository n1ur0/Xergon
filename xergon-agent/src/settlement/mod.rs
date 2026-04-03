//! ERG Settlement Engine
//!
//! The invisible settlement layer between fiat credits (user-facing)
//! and ERG payments (provider-facing).
//!
//! Flow:
//! 1. Aggregate per-provider usage from local usage records
//! 2. Convert USD earnings to ERG at current market rate
//! 3. Build batch ERG payment transaction(s)
//! 4. Broadcast to Ergo network via node's /wallet/payment endpoint
//! 5. Track confirmation status
//!
//! This runs as a periodic background task. User never sees ERG.
//! Provider receives ERG to their configured address from xergon-agent config.

pub mod market;
pub mod models;
pub mod transactions;

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::config::SettlementConfig;
use market::MarketRateProvider;
use models::{
    BatchStatus, PaymentStatus, ProviderEarning, SettlementBatch, SettlementLedger,
    SettlementPayment, SettlementSummary,
};
use transactions::TransactionService;

/// Minimum USD threshold for a provider payment (below this, we skip).
const MIN_PAYMENT_USD: f64 = 0.01;
/// Minimum ERG nano amount for a payment (below this, we skip).
const MIN_PAYMENT_NANOERG: u64 = 1_000_000; // 0.001 ERG
/// ERG precision: 1 ERG = 10^9 nanoERG
const NANOERG_PER_ERG: u64 = 1_000_000_000;

/// The settlement engine orchestrates the full settlement lifecycle.
pub struct SettlementEngine {
    config: SettlementConfig,
    market: MarketRateProvider,
    tx_service: TransactionService,
    ledger: Arc<RwLock<SettlementLedger>>,
    /// Local usage tracking (tokens/requests per provider)
    usage_tracker: Arc<UsageTracker>,
    /// Current market rate (cached for API)
    current_rate: Arc<std::sync::Mutex<f64>>,
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
    cost_usd: f64,
}

impl UsageTracker {
    /// Record a single inference usage event.
    pub async fn record_usage(
        &self,
        provider_id: &str,
        ergo_address: &str,
        tokens_in: u64,
        tokens_out: u64,
        cost_usd: f64,
    ) {
        let mut entries = self.entries.lock().await;
        let entry = entries.entry(provider_id.to_string()).or_insert_with(|| ProviderUsageEntry {
            provider_id: provider_id.to_string(),
            ergo_address: ergo_address.to_string(),
            tokens_in: 0,
            tokens_out: 0,
            requests: 0,
            cost_usd: 0.0,
        });
        entry.tokens_in += tokens_in;
        entry.tokens_out += tokens_out;
        entry.requests += 1;
        entry.cost_usd += cost_usd;
    }

    /// Drain all accumulated usage and return as ProviderEarning list.
    pub async fn drain(&self) -> Vec<ProviderEarning> {
        let mut entries = self.entries.lock().await;
        let earnings: Vec<ProviderEarning> = entries
            .drain()
            .filter(|(_, e)| e.cost_usd >= MIN_PAYMENT_USD)
            .map(|(_, e)| ProviderEarning {
                provider_id: e.provider_id,
                ergo_address: e.ergo_address,
                earned_usd: e.cost_usd,
                tokens_processed: e.tokens_in + e.tokens_out,
                requests_handled: e.requests,
            })
            .collect();
        earnings
    }

    /// Peek at current accumulated usage without draining.
    pub async fn peek(&self) -> Vec<ProviderEarning> {
        let entries = self.entries.lock().await;
        entries
            .values()
            .filter(|e| e.cost_usd >= MIN_PAYMENT_USD)
            .map(|e| ProviderEarning {
                provider_id: e.provider_id.clone(),
                ergo_address: e.ergo_address.clone(),
                earned_usd: e.cost_usd,
                tokens_processed: e.tokens_in + e.tokens_out,
                requests_handled: e.requests,
            })
            .collect()
    }
}

impl SettlementEngine {
    /// Create a new settlement engine.
    pub fn new(
        config: SettlementConfig,
        ergo_rest_url: String,
    ) -> Result<Self> {
        // Persist rate next to the ledger file, or default to data/
        let persist_dir = config
            .ledger_file
            .as_ref()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("data"));
        let rate_path = persist_dir.join("last_erg_rate.json");

        let market = MarketRateProvider::with_persist_path(rate_path)
            .context("Failed to create market rate provider")?;
        let tx_service = TransactionService::new(ergo_rest_url)
            .context("Failed to create transaction service")?;

        Ok(Self {
            config,
            market,
            tx_service,
            ledger: Arc::new(RwLock::new(SettlementLedger::default())),
            usage_tracker: Arc::new(UsageTracker::default()),
            current_rate: Arc::new(std::sync::Mutex::new(0.0)),
        })
    }

    /// Initialize: load persisted ledger from disk and seed rate cache.
    pub async fn init(&self) -> Result<()> {
        // Load persisted rate so we have a fallback if CoinGecko is down
        self.market.load_persisted().await;

        let ledger_path = self.ledger_path();
        let mut ledger = self.ledger.write().await;
        *ledger = SettlementLedger::load(&ledger_path).await?;
        info!(
            total_batches = ledger.batches.len(),
            total_erg_paid = ledger.total_erg_paid,
            total_usd_settled = ledger.total_usd_settled,
            "Settlement ledger loaded"
        );

        // Try to fetch initial rate
        match self.market.get_rate().await {
            Ok(rate) => {
                *self.current_rate.lock().unwrap() = rate;
                info!(rate = rate, "Initial ERG/USD rate fetched");
            }
            Err(e) => {
                warn!(error = %e, "Failed to fetch initial ERG/USD rate, will retry on next settlement");
            }
        }

        Ok(())
    }

    /// Record an inference usage event for settlement tracking.
    pub async fn record_usage(
        &self,
        provider_id: &str,
        ergo_address: &str,
        tokens_in: u64,
        tokens_out: u64,
        cost_usd: f64,
    ) {
        self.usage_tracker
            .record_usage(provider_id, ergo_address, tokens_in, tokens_out, cost_usd)
            .await;
    }

    /// Run a single settlement cycle.
    ///
    /// 1. Drain accumulated provider earnings
    /// 2. Fetch current ERG/USD rate
    /// 3. Convert USD to ERG
    /// 4. Build and send batch transaction
    /// 5. Persist results
    pub async fn settle(&self) -> Result<SettlementBatch> {
        info!("Starting settlement cycle...");

        // Step 1: Drain earnings
        let earnings = self.usage_tracker.drain().await;
        if earnings.is_empty() {
            info!("No provider earnings to settle");
            return Err(anyhow::anyhow!("No earnings to settle"));
        }

        info!(providers = earnings.len(), "Aggregated provider earnings");

        // Step 2: Fetch market rate
        let erg_usd_rate = self.market.get_rate().await
            .context("Failed to fetch ERG/USD market rate")?;
        *self.current_rate.lock().unwrap() = erg_usd_rate;

        info!(rate = erg_usd_rate, "Using ERG/USD rate for settlement");

        // Step 3: Build settlement batch
        let mut batch = self.build_batch(earnings, erg_usd_rate).await;

        // Step 4: Send payments
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

        // Step 5: Persist
        {
            let mut ledger = self.ledger.write().await;
            ledger.record_batch(&batch);
            ledger.save(&self.ledger_path()).await?;
        }

        info!(
            batch_id = %batch.batch_id,
            status = ?batch.status,
            total_erg = batch.total_erg,
            total_usd = batch.total_usd,
            payments = batch.payments.len(),
            "Settlement cycle complete"
        );

        Ok(batch)
    }

    /// Build a settlement batch from provider earnings.
    async fn build_batch(
        &self,
        earnings: Vec<ProviderEarning>,
        erg_usd_rate: f64,
    ) -> SettlementBatch {
        let now = chrono::Utc::now();
        let period_end = now;
        // Period start = last settlement time, or 24h ago if never settled
        let period_start = {
            let ledger = self.ledger.read().await;
            ledger.last_settled_at.unwrap_or_else(|| now - chrono::Duration::hours(24))
        };

        let mut total_erg = 0.0;
        let mut total_usd = 0.0;
        let mut payments = Vec::new();

        for earning in &earnings {
            let erg_amount = earning.earned_usd / erg_usd_rate;
            let erg_nano = (erg_amount * NANOERG_PER_ERG as f64) as u64;

            // Skip dust payments
            if erg_nano < MIN_PAYMENT_NANOERG {
                warn!(
                    provider_id = %earning.provider_id,
                    usd = earning.earned_usd,
                    erg_nano = erg_nano,
                    "Skipping dust payment"
                );
                continue;
            }

            total_erg += erg_nano as f64 / NANOERG_PER_ERG as f64;
            total_usd += earning.earned_usd;

            payments.push(SettlementPayment {
                provider_id: earning.provider_id.clone(),
                ergo_address: earning.ergo_address.clone(),
                usd_amount: earning.earned_usd,
                erg_nano,
                tx_id: None,
                status: PaymentStatus::Pending,
            });
        }

        SettlementBatch {
            batch_id: uuid_simple(),
            created_at: now,
            period_start,
            period_end,
            erg_usd_rate,
            payments,
            total_erg,
            total_usd,
            status: BatchStatus::Pending,
        }
    }

    /// Run the periodic settlement loop.
    /// Call this in a spawned tokio task.
    pub async fn run_loop(&self) {
        let interval_secs = self.config.interval_secs;
        let interval = std::time::Duration::from_secs(interval_secs);

        info!(
            interval_secs = interval_secs,
            dry_run = self.config.dry_run,
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
                        error!(error = %e, "Settlement cycle failed");
                    }
                }
            }

            tokio::time::sleep(interval).await;
        }
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
    async fn confirm_submitted_batches(
        &self,
        max_age: chrono::Duration,
    ) -> Result<()> {
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

                let all_confirmed = batch.payments.iter().all(|p| p.status == PaymentStatus::Confirmed);
                let any_still_broadcast = batch.payments.iter().any(|p| p.status == PaymentStatus::Broadcast);

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
                            payment.status = PaymentStatus::Failed(
                                format!("Transaction not confirmed after {} hours", batch_age.num_hours())
                            );
                        }
                    }
                    let has_failed = batch.payments.iter().any(|p| matches!(p.status, PaymentStatus::Failed(_)));
                    if has_failed {
                        batch.status = BatchStatus::Failed(
                            format!("Timed out after {} hours — some payments unconfirmed", batch_age.num_hours())
                        );
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
        self.usage_tracker.peek().await
    }

    /// Get settlement summary (for API display).
    pub async fn summary(&self) -> SettlementSummary {
        let ledger = self.ledger.read().await;
        let current_rate = *self.current_rate.lock().unwrap();
        let last_batch = ledger.batches.first();
        let next_settlement = chrono::Utc::now() + chrono::Duration::seconds(self.config.interval_secs as i64);

        SettlementSummary {
            last_settled_at: ledger.last_settled_at,
            last_batch_id: last_batch.map(|b| b.batch_id.clone()),
            last_batch_status: last_batch.map(|b| format!("{:?}", b.status)),
            total_batches: ledger.batches.len(),
            total_erg_paid: ledger.total_erg_paid,
            total_usd_settled: ledger.total_usd_settled,
            next_settlement_at: next_settlement,
            current_erg_usd_rate: if current_rate > 0.0 { current_rate } else { 0.0 },
        }
    }

    /// Get the ledger for detailed history access.
    pub async fn ledger(&self) -> tokio::sync::RwLockReadGuard<'_, SettlementLedger> {
        self.ledger.read().await
    }

    fn ledger_path(&self) -> PathBuf {
        self.config.ledger_file
            .clone()
            .unwrap_or_else(|| PathBuf::from("data/settlement_ledger.json"))
    }
}

/// Generate a simple unique ID for batches (no uuid crate dependency).
fn uuid_simple() -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0).to_le_bytes());
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
    fn test_usd_to_erg_conversion() {
        // At $1/ERG, $5.00 = 5 ERG = 5_000_000_000 nanoERG
        let usd = 5.0;
        let rate = 1.0;
        let erg_amount = usd / rate;
        let erg_nano = (erg_amount * NANOERG_PER_ERG as f64) as u64;
        assert_eq!(erg_nano, 5_000_000_000);
    }

    #[test]
    fn test_usd_to_erg_at_realistic_rate() {
        // At $0.50/ERG, $2.50 = 5 ERG = 5_000_000_000 nanoERG
        let usd = 2.50;
        let rate = 0.50;
        let erg_amount = usd / rate;
        let erg_nano = (erg_amount * NANOERG_PER_ERG as f64) as u64;
        assert_eq!(erg_nano, 5_000_000_000);
    }

    #[test]
    fn test_dust_filtering() {
        // At $1/ERG, $0.001 = 0.001 ERG = 1_000_000 nanoERG (exactly at threshold)
        let usd = 0.001;
        let rate = 1.0;
        let erg_nano = (usd / rate * NANOERG_PER_ERG as f64) as u64;
        assert!(erg_nano >= MIN_PAYMENT_NANOERG);

        // $0.0001 would be 100_000 nanoERG (below threshold)
        let usd_tiny = 0.0001;
        let erg_nano_tiny = (usd_tiny / rate * NANOERG_PER_ERG as f64) as u64;
        assert!(erg_nano_tiny < MIN_PAYMENT_NANOERG);
    }

    #[tokio::test]
    async fn test_usage_tracker_drain() {
        let tracker = UsageTracker::default();
        tracker.record_usage("prov1", "addr1", 100, 200, 0.05).await;
        tracker.record_usage("prov1", "addr1", 50, 100, 0.03).await;
        tracker.record_usage("prov2", "addr2", 200, 400, 0.10).await;

        let earnings = tracker.drain().await;
        assert_eq!(earnings.len(), 2);

        // After drain, should be empty
        let earnings2 = tracker.drain().await;
        assert!(earnings2.is_empty());
    }
}
