//! Settlement data models
//!
//! Types for tracking per-provider earnings, settlement batches,
//! and individual settlement records persisted to disk.
//!
//! All amounts are in nanoERG (1 ERG = 10^9 nanoERG).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single provider's aggregated earnings for a settlement period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderEarning {
    /// Provider ID (e.g., "Xergon_LT")
    pub provider_id: String,
    /// Provider's Ergo payment address
    pub ergo_address: String,
    /// Total nanoERG earned during the settlement period
    pub earned_nanoerg: u64,
    /// Total tokens processed
    pub tokens_processed: u64,
    /// Total inference requests handled
    pub requests_handled: u64,
}

/// A batch of ERG payments to send to providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementBatch {
    /// Unique batch ID (UUID v4)
    pub batch_id: String,
    /// When this batch was created
    pub created_at: DateTime<Utc>,
    /// Settlement period start
    pub period_start: DateTime<Utc>,
    /// Settlement period end
    pub period_end: DateTime<Utc>,
    /// Cost per 1K tokens in nanoERG used for this batch
    pub cost_per_1k_nanoerg: u64,
    /// Provider payments in this batch
    pub payments: Vec<SettlementPayment>,
    /// Total ERG to send in this batch (as float ERG for display)
    pub total_erg: f64,
    /// Total nanoERG in this batch
    pub total_nanoerg: u64,
    /// Batch status
    pub status: BatchStatus,
}

/// A single ERG payment within a settlement batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementPayment {
    /// Provider ID
    pub provider_id: String,
    /// Ergo address receiving payment
    pub ergo_address: String,
    /// nanoERG amount being settled
    pub nanoerg_amount: u64,
    /// ERG nano amount (1 ERG = 10^9 nanoERG) — same as nanoerg_amount
    pub erg_nano: u64,
    /// Transaction ID once broadcast (None if not yet sent)
    pub tx_id: Option<String>,
    /// Payment status
    pub status: PaymentStatus,
}

impl SettlementPayment {
    /// Convenience: total nanoERG for this payment.
    pub fn total_nanoerg(&self) -> u64 {
        self.erg_nano
    }
}

/// Status of a settlement batch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BatchStatus {
    /// Batch is being prepared
    Pending,
    /// Batch has been submitted to the Ergo node
    Submitted,
    /// All payments confirmed on-chain
    Confirmed,
    /// Some or all payments failed
    Failed(String),
}

/// Status of an individual payment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PaymentStatus {
    /// Ready to send
    Pending,
    /// Transaction broadcast to network
    Broadcast,
    /// Confirmed on-chain
    Confirmed,
    /// Failed to send
    Failed(String),
}

/// A single pending payment awaiting batch settlement.
#[derive(Debug, Clone)]
pub struct PendingPayment {
    /// User address that originated the inference request
    pub user_address: String,
    /// Provider Ergo address to receive payment
    pub provider_address: String,
    /// Amount in nanoERG
    pub amount: u64,
    /// Model identifier (e.g., "llama-3.1-8b")
    pub model: String,
    /// Unix timestamp when this payment was recorded
    pub timestamp: i64,
}

/// Result of a batch settlement flush operation.
#[derive(Debug, Clone)]
pub struct BatchSettlementResult {
    /// Transaction IDs of successfully sent payments
    pub tx_ids: Vec<String>,
    /// Total nanoERG sent across all payments
    pub total_erg: u64,
    /// Number of provider payments successfully sent
    pub payment_count: usize,
    /// Number of provider groups skipped due to dust threshold
    pub skipped_dust: usize,
}

/// Persisted settlement ledger. Stores historical batches on disk.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SettlementLedger {
    /// All settlement batches, newest first
    pub batches: Vec<SettlementBatch>,
    /// Last settlement timestamp
    pub last_settled_at: Option<DateTime<Utc>>,
    /// Running total ERG paid out (as float ERG for display)
    pub total_erg_paid: f64,
    /// Running total nanoERG settled
    pub total_nanoerg_settled: u64,
}

/// Summary of the most recent settlement (exposed via API).
#[derive(Debug, Clone, Serialize)]
pub struct SettlementSummary {
    pub last_settled_at: Option<DateTime<Utc>>,
    pub last_batch_id: Option<String>,
    pub last_batch_status: Option<String>,
    pub total_batches: usize,
    pub total_erg_paid: f64,
    pub total_nanoerg_settled: u64,
    pub next_settlement_at: DateTime<Utc>,
    pub cost_per_1k_nanoerg: u64,
}

impl SettlementLedger {
    /// Load ledger from disk, or create a new empty one.
    pub async fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        if path.exists() {
            let data = tokio::fs::read_to_string(path).await?;
            // Try new format first, fall back to legacy USD format
            match serde_json::from_str::<SettlementLedger>(&data) {
                Ok(ledger) => Ok(ledger),
                Err(_) => {
                    // Attempt to load legacy ledger and migrate
                    #[derive(Debug, Clone, serde::Deserialize)]
                    struct LegacySettlementPayment {
                        provider_id: String,
                        ergo_address: String,
                        #[serde(default)]
                        usd_amount: f64,
                        #[serde(default)]
                        erg_nano: u64,
                        tx_id: Option<String>,
                        status: PaymentStatus,
                    }

                    #[derive(Debug, Clone, serde::Deserialize)]
                    struct LegacySettlementBatch {
                        batch_id: String,
                        created_at: DateTime<Utc>,
                        period_start: DateTime<Utc>,
                        period_end: DateTime<Utc>,
                        #[serde(default)]
                        erg_usd_rate: f64,
                        payments: Vec<LegacySettlementPayment>,
                        #[serde(default)]
                        total_erg: f64,
                        #[serde(default)]
                        total_usd: f64,
                        status: BatchStatus,
                    }

                    #[derive(Debug, Clone, Default, serde::Deserialize)]
                    struct LegacySettlementLedger {
                        batches: Vec<LegacySettlementBatch>,
                        last_settled_at: Option<DateTime<Utc>>,
                        #[serde(default)]
                        total_erg_paid: f64,
                        #[serde(default)]
                        total_usd_settled: f64,
                    }

                    let legacy: LegacySettlementLedger = serde_json::from_str(&data)
                        .map_err(|e| anyhow::anyhow!("Failed to parse ledger (neither new nor legacy format): {}", e))?;

                    tracing::warn!(
                        legacy_batches = legacy.batches.len(),
                        "Migrating legacy USD-denominated ledger to nanoERG format"
                    );

                    let mut total_nanoerg: u64 = 0;
                    let batches: Vec<SettlementBatch> = legacy
                        .batches
                        .into_iter()
                        .map(|lb| {
                            let batch_nanoerg: u64 = lb.payments.iter().map(|p| p.erg_nano).sum();
                            total_nanoerg += batch_nanoerg;

                            SettlementBatch {
                                batch_id: lb.batch_id,
                                created_at: lb.created_at,
                                period_start: lb.period_start,
                                period_end: lb.period_end,
                                cost_per_1k_nanoerg: 0, // unknown from legacy data
                                payments: lb
                                    .payments
                                    .into_iter()
                                    .map(|p| SettlementPayment {
                                        provider_id: p.provider_id,
                                        ergo_address: p.ergo_address,
                                        nanoerg_amount: p.erg_nano,
                                        erg_nano: p.erg_nano,
                                        tx_id: p.tx_id,
                                        status: p.status,
                                    })
                                    .collect(),
                                total_erg: lb.total_erg,
                                total_nanoerg: batch_nanoerg,
                                status: lb.status,
                            }
                        })
                        .collect();

                    Ok(SettlementLedger {
                        batches,
                        last_settled_at: legacy.last_settled_at,
                        total_erg_paid: legacy.total_erg_paid,
                        total_nanoerg_settled: total_nanoerg,
                    })
                }
            }
        } else {
            Ok(Self::default())
        }
    }

    /// Persist ledger to disk.
    pub async fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let data = serde_json::to_string_pretty(self)?;
        tokio::fs::write(path, data).await?;
        Ok(())
    }

    /// Record a completed batch.
    pub fn record_batch(&mut self, batch: &SettlementBatch) {
        self.batches.insert(0, batch.clone());
        self.total_erg_paid += batch.total_erg;
        self.total_nanoerg_settled += batch.total_nanoerg;
        self.last_settled_at = Some(batch.created_at);

        // Keep only last 100 batches to prevent unbounded growth
        self.batches.truncate(100);
    }
}
