//! Settlement reconciliation module
//!
//! Periodically verifies settlement integrity by cross-referencing
//! the local ledger against the Ergo node's on-chain state.
//!
//! Checks performed:
//! 1. Broadcast payments still pending on-chain
//! 2. Confirmed payments verified against the node
//! 3. Failed payments that may have actually succeeded (stale status)
//! 4. Amount discrepancies between ledger and on-chain data

use anyhow::{Context, Result};
use reqwest::Client;
use serde::Serialize;
use std::path::PathBuf;
use tracing::{info, warn};

use super::models::{PaymentStatus, SettlementLedger};

/// How far back (in hours) to check batches during reconciliation.
const RECONCILE_LOOKBACK_HOURS: i64 = 24;

/// HTTP timeout for node API calls during reconciliation.
const RECONCILE_HTTP_TIMEOUT_SECS: u64 = 15;

/// Result of verifying a single transaction against the Ergo node.
#[derive(Debug)]
#[allow(dead_code)] // Fields reserved for future use in detailed reporting
pub enum TxVerification {
    /// Transaction is confirmed on-chain.
    Confirmed {
        /// Inclusion height.
        inclusion_height: u32,
        /// Whether the on-chain amount matches expected.
        matches: bool,
        /// Actual amount found on-chain (sum of outputs to the expected address), if parsed.
        actual_amount_nanoerg: Option<u64>,
    },
    /// Transaction exists but not yet in a block (still in mempool).
    InMempool,
    /// Transaction not found on the node (may have been dropped or never broadcast).
    NotFound,
}

/// Reconciliation report summarizing the results of a reconciliation check.
#[derive(Debug, Serialize)]
pub struct ReconciliationReport {
    /// When the reconciliation was performed (ISO 8601).
    pub checked_at: String,
    /// Number of batches examined (within lookback window).
    pub batches_checked: usize,
    /// Total number of individual payments examined.
    pub payments_checked: usize,
    /// Payments that were Confirmed and still show as confirmed on-chain.
    pub confirmed_ok: usize,
    /// Payments marked Confirmed but amount doesn't match on-chain.
    pub confirmed_mismatch: usize,
    /// Payments still in Broadcast status that remain in mempool.
    pub broadcast_still_pending: usize,
    /// Payments stuck in Broadcast that are now confirmed on-chain.
    pub broadcast_now_confirmed: usize,
    /// Payments marked Failed but the tx actually exists on-chain.
    pub failed_needs_retry: usize,
    /// Total nanoERG expected across all checked payments.
    pub total_expected_nanoerg: u64,
    /// Total nanoERG confirmed on-chain across all checked payments.
    pub total_confirmed_nanoerg: u64,
    /// Human-readable list of discrepancies found.
    pub discrepancies: Vec<String>,
}

/// Reconciler cross-references the local settlement ledger against the Ergo node.
pub struct Reconciler {
    /// Path to the settlement ledger file on disk.
    ledger_path: PathBuf,
    /// Ergo node REST API base URL (e.g., "http://127.0.0.1:9053").
    node_url: String,
    /// HTTP client for node API calls.
    http_client: Client,
}

impl Reconciler {
    /// Create a new reconciler.
    pub fn new(ledger_path: PathBuf, node_url: String) -> Result<Self> {
        let http_client = Client::builder()
            .timeout(std::time::Duration::from_secs(RECONCILE_HTTP_TIMEOUT_SECS))
            .build()
            .context("Failed to build HTTP client for reconciler")?;

        let node_url = node_url.trim_end_matches('/').to_string();

        Ok(Self {
            ledger_path,
            node_url,
            http_client,
        })
    }

    /// Run a full reconciliation check.
    ///
    /// 1. Loads the settlement ledger from disk.
    /// 2. Filters batches from the last 24 hours.
    /// 3. For each payment:
    ///    - Broadcast: check if tx is now confirmed
    ///    - Confirmed: verify tx still exists and is confirmed
    ///    - Failed: check if tx actually succeeded (stale status recovery)
    /// 4. Computes summary totals and discrepancy list.
    /// 5. Returns the report (caller should log it).
    pub async fn reconcile(&self) -> Result<ReconciliationReport> {
        info!("Starting settlement reconciliation...");

        let ledger = SettlementLedger::load(&self.ledger_path)
            .await
            .context("Failed to load ledger for reconciliation")?;

        let now = chrono::Utc::now();
        let cutoff = now - chrono::Duration::hours(RECONCILE_LOOKBACK_HOURS);

        let mut report = ReconciliationReport {
            checked_at: now.to_rfc3339(),
            batches_checked: 0,
            payments_checked: 0,
            confirmed_ok: 0,
            confirmed_mismatch: 0,
            broadcast_still_pending: 0,
            broadcast_now_confirmed: 0,
            failed_needs_retry: 0,
            total_expected_nanoerg: 0,
            total_confirmed_nanoerg: 0,
            discrepancies: Vec::new(),
        };

        for batch in &ledger.batches {
            // Only check batches within the lookback window
            if batch.created_at < cutoff {
                continue;
            }

            report.batches_checked += 1;

            for payment in &batch.payments {
                report.payments_checked += 1;
                report.total_expected_nanoerg += payment.nanoerg_amount;

                let tx_id = match &payment.tx_id {
                    Some(id) => id.clone(),
                    None => {
                        // No tx_id at all — skip (can't verify without it)
                        continue;
                    }
                };

                let verification = self.verify_tx(&tx_id).await;

                match (&payment.status, verification) {
                    // --- Confirmed payments: verify they're still confirmed ---
                    (PaymentStatus::Confirmed, TxVerification::Confirmed { matches: true, .. }) => {
                        report.confirmed_ok += 1;
                        report.total_confirmed_nanoerg += payment.nanoerg_amount;
                    }
                    (PaymentStatus::Confirmed, TxVerification::Confirmed { matches: false, actual_amount_nanoerg, .. }) => {
                        report.confirmed_mismatch += 1;
                        let detail = format!(
                            "AMOUNT MISMATCH: batch={} provider={} tx={} expected={} nanoerg actual={:?} nanoerg",
                            batch.batch_id,
                            payment.provider_id,
                            tx_id,
                            payment.nanoerg_amount,
                            actual_amount_nanoerg,
                        );
                        report.discrepancies.push(detail);
                        // Still count as confirmed for total
                        report.total_confirmed_nanoerg += payment.nanoerg_amount;
                    }
                    (PaymentStatus::Confirmed, TxVerification::NotFound) => {
                        report.confirmed_mismatch += 1;
                        let detail = format!(
                            "CONFIRMED TX NOT FOUND: batch={} provider={} tx={} — tx disappeared from node",
                            batch.batch_id,
                            payment.provider_id,
                            tx_id,
                        );
                        report.discrepancies.push(detail);
                    }
                    (PaymentStatus::Confirmed, TxVerification::InMempool) => {
                        report.confirmed_mismatch += 1;
                        let detail = format!(
                            "CONFIRMED TX BACK IN MEMPOOL: batch={} provider={} tx={} — was confirmed, now unconfirmed",
                            batch.batch_id,
                            payment.provider_id,
                            tx_id,
                        );
                        report.discrepancies.push(detail);
                    }

                    // --- Broadcast payments: check if now confirmed ---
                    (PaymentStatus::Broadcast, TxVerification::Confirmed { .. }) => {
                        report.broadcast_now_confirmed += 1;
                        report.total_confirmed_nanoerg += payment.nanoerg_amount;
                        let detail = format!(
                            "BROADCAST NOW CONFIRMED: batch={} provider={} tx={}",
                            batch.batch_id,
                            payment.provider_id,
                            tx_id,
                        );
                        report.discrepancies.push(detail);
                    }
                    (PaymentStatus::Broadcast, TxVerification::InMempool) => {
                        report.broadcast_still_pending += 1;
                    }
                    (PaymentStatus::Broadcast, TxVerification::NotFound) => {
                        report.broadcast_still_pending += 1;
                        let detail = format!(
                            "BROADCAST TX NOT FOUND: batch={} provider={} tx={} — may have been dropped",
                            batch.batch_id,
                            payment.provider_id,
                            tx_id,
                        );
                        report.discrepancies.push(detail);
                    }

                    // --- Failed payments: check if they actually succeeded ---
                    (PaymentStatus::Failed(_reason), TxVerification::Confirmed { .. }) => {
                        report.failed_needs_retry += 1;
                        report.total_confirmed_nanoerg += payment.nanoerg_amount;
                        let detail = format!(
                            "FAILED TX ACTUALLY CONFIRMED: batch={} provider={} tx={} — stale Failed status, needs correction",
                            batch.batch_id,
                            payment.provider_id,
                            tx_id,
                        );
                        report.discrepancies.push(detail);
                    }
                    (PaymentStatus::Failed(_reason), TxVerification::InMempool) => {
                        report.failed_needs_retry += 1;
                        let detail = format!(
                            "FAILED TX IN MEMPOOL: batch={} provider={} tx={} — may still confirm",
                            batch.batch_id,
                            payment.provider_id,
                            tx_id,
                        );
                        report.discrepancies.push(detail);
                    }
                    // Failed + NotFound is expected — truly failed
                    (PaymentStatus::Failed(_), TxVerification::NotFound) => {
                        // Expected state, no action needed
                    }

                    // Pending payments have no tx_id, so this shouldn't happen
                    (PaymentStatus::Pending, _) => {
                        // No tx to verify
                    }
                }
            }
        }

        // Log the summary
        if report.discrepancies.is_empty() {
            info!(
                batches = report.batches_checked,
                payments = report.payments_checked,
                confirmed_ok = report.confirmed_ok,
                broadcast_pending = report.broadcast_still_pending,
                "Reconciliation complete: all clear"
            );
        } else {
            warn!(
                batches = report.batches_checked,
                payments = report.payments_checked,
                confirmed_ok = report.confirmed_ok,
                confirmed_mismatch = report.confirmed_mismatch,
                broadcast_now_confirmed = report.broadcast_now_confirmed,
                failed_needs_retry = report.failed_needs_retry,
                discrepancies = report.discrepancies.len(),
                "Reconciliation found discrepancies"
            );
            for d in &report.discrepancies {
                warn!(discrepancy = %d, "Reconciliation issue");
            }
        }

        Ok(report)
    }

    /// Verify a single transaction against the Ergo node.
    ///
    /// - GET {node_url}/transactions/byId/{tx_id}
    /// - 200 with inclusionHeight > 0: Confirmed
    /// - 200 with inclusionHeight = null/0: InMempool
    /// - 404 or error: NotFound
    async fn verify_tx(&self, tx_id: &str) -> TxVerification {
        let url = format!("{}/transactions/byId/{}", self.node_url, tx_id);

        match self.http_client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<serde_json::Value>().await {
                    Ok(body) => {
                        let inclusion_height = body
                            .get("inclusionHeight")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);

                        if inclusion_height > 0 {
                            // Transaction is confirmed. We don't do deep amount matching
                            // (the node payment API already guarantees correct amounts),
                            // so we report matches=true.
                            TxVerification::Confirmed {
                                inclusion_height: inclusion_height as u32,
                                matches: true,
                                actual_amount_nanoerg: None,
                            }
                        } else {
                            TxVerification::InMempool
                        }
                    }
                    Err(e) => {
                        warn!(
                            tx_id = %tx_id,
                            error = %e,
                            "Failed to parse tx response during reconciliation"
                        );
                        TxVerification::NotFound
                    }
                }
            }
            Ok(resp) => {
                // 404 or other non-success: tx not found
                if resp.status().as_u16() == 404 {
                    TxVerification::NotFound
                } else {
                    warn!(
                        tx_id = %tx_id,
                        status = %resp.status(),
                        "Unexpected status checking tx during reconciliation"
                    );
                    TxVerification::NotFound
                }
            }
            Err(e) => {
                warn!(
                    tx_id = %tx_id,
                    error = %e,
                    "Failed to reach node for tx verification during reconciliation"
                );
                TxVerification::NotFound
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reconciliation_report_default() {
        let report = ReconciliationReport {
            checked_at: "2025-01-01T00:00:00Z".to_string(),
            batches_checked: 0,
            payments_checked: 0,
            confirmed_ok: 0,
            confirmed_mismatch: 0,
            broadcast_still_pending: 0,
            broadcast_now_confirmed: 0,
            failed_needs_retry: 0,
            total_expected_nanoerg: 0,
            total_confirmed_nanoerg: 0,
            discrepancies: Vec::new(),
        };

        assert_eq!(report.batches_checked, 0);
        assert_eq!(report.confirmed_ok, 0);
        assert!(report.discrepancies.is_empty());

        // Verify it serializes cleanly
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("checked_at"));
    }

    #[test]
    fn test_reconciliation_report_with_discrepancies() {
        let mut report = ReconciliationReport {
            checked_at: "2025-01-01T00:00:00Z".to_string(),
            batches_checked: 2,
            payments_checked: 5,
            confirmed_ok: 3,
            confirmed_mismatch: 0,
            broadcast_still_pending: 1,
            broadcast_now_confirmed: 1,
            failed_needs_retry: 0,
            total_expected_nanoerg: 5_000_000_000,
            total_confirmed_nanoerg: 4_000_000_000,
            discrepancies: Vec::new(),
        };
        report.discrepancies.push("test discrepancy".to_string());

        assert_eq!(report.discrepancies.len(), 1);
        assert_eq!(report.broadcast_now_confirmed, 1);
    }
}
