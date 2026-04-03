//! Settlement data models
//!
//! Types for tracking per-provider earnings, settlement batches,
//! and individual settlement records persisted to disk.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single provider's aggregated earnings for a settlement period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderEarning {
    /// Provider ID (e.g., "Xergon_LT")
    pub provider_id: String,
    /// Provider's Ergo payment address
    pub ergo_address: String,
    /// Total USD earned during the settlement period
    pub earned_usd: f64,
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
    /// ERG/USD exchange rate used for conversion
    pub erg_usd_rate: f64,
    /// Provider payments in this batch
    pub payments: Vec<SettlementPayment>,
    /// Total ERG to send in this batch
    pub total_erg: f64,
    /// Total USD value being settled
    pub total_usd: f64,
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
    /// USD amount being converted
    pub usd_amount: f64,
    /// ERG nano amount (1 ERG = 10^9 nanoERG)
    pub erg_nano: u64,
    /// Transaction ID once broadcast (None if not yet sent)
    pub tx_id: Option<String>,
    /// Payment status
    pub status: PaymentStatus,
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

/// Persisted settlement ledger. Stores historical batches on disk.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SettlementLedger {
    /// All settlement batches, newest first
    pub batches: Vec<SettlementBatch>,
    /// Last settlement timestamp
    pub last_settled_at: Option<DateTime<Utc>>,
    /// Running total ERG paid out
    pub total_erg_paid: f64,
    /// Running total USD settled
    pub total_usd_settled: f64,
}

/// Summary of the most recent settlement (exposed via API).
#[derive(Debug, Clone, Serialize)]
pub struct SettlementSummary {
    pub last_settled_at: Option<DateTime<Utc>>,
    pub last_batch_id: Option<String>,
    pub last_batch_status: Option<String>,
    pub total_batches: usize,
    pub total_erg_paid: f64,
    pub total_usd_settled: f64,
    pub next_settlement_at: DateTime<Utc>,
    pub current_erg_usd_rate: f64,
}

impl SettlementLedger {
    /// Load ledger from disk, or create a new empty one.
    pub async fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        if path.exists() {
            let data = tokio::fs::read_to_string(path).await?;
            let ledger: SettlementLedger = serde_json::from_str(&data)?;
            Ok(ledger)
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
        self.total_usd_settled += batch.total_usd;
        self.last_settled_at = Some(batch.created_at);

        // Keep only last 100 batches to prevent unbounded growth
        self.batches.truncate(100);
    }
}
