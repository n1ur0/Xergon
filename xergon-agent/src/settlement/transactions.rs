//! Ergo transaction building and broadcasting
//!
//! Constructs and submits ERG payment transactions via the Ergo node's
//! REST API. Uses the node's /wallet/payment endpoint for simplified
//! transaction creation.
//!
//! Transaction flow:
//! 1. Lock wallet inputs (POST /wallet/payment with request)
//! 2. Send transaction (POST /wallet/payment/send)
//! 3. Track tx_id for confirmation

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use super::models::{PaymentStatus, SettlementBatch};
use crate::protocol::tx_safety::{validate_address_or_tree, SAFE_MIN_BOX_VALUE};

/// Ergo payment request (matches node REST API).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ErgoPaymentRequest {
    /// Amount in nanoERG
    amount: u64,
    /// Recipient Ergo address
    recipient: String,
    /// Fee in nanoERG (default 1000000 = 0.001 ERG)
    #[serde(skip_serializing_if = "Option::is_none")]
    fee: Option<u64>,
}

/// Ergo payment response from /wallet/payment.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // TODO: will be used for payment status tracking
struct ErgoPaymentResponse {
    pub tx_id: String,
    pub error: Option<u16>,
    pub detail: Option<String>,
}

/// Ergo transaction service.
pub struct TransactionService {
    ergo_rest_url: String,
    http_client: Client,
    /// Default fee in nanoERG (0.001 ERG)
    default_fee: u64,
    /// Whether the node wallet is unlocked
    wallet_unlocked: std::sync::atomic::AtomicBool,
}

impl TransactionService {
    pub fn new(ergo_rest_url: String) -> Result<Self> {
        let http_client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to build HTTP client for transaction service")?;

        Ok(Self {
            ergo_rest_url,
            http_client,
            default_fee: 1_000_000, // 0.001 ERG
            wallet_unlocked: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// Check if the node wallet is available and unlocked.
    pub async fn check_wallet(&self) -> Result<bool> {
        let url = format!("{}/wallet/status", self.ergo_rest_url.trim_end_matches('/'));

        match self.http_client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let body: serde_json::Value = resp.json().await.unwrap_or_default();
                let is_unlocked = body
                    .get("isUnlocked")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                self.wallet_unlocked
                    .store(is_unlocked, std::sync::atomic::Ordering::Relaxed);
                Ok(is_unlocked)
            }
            Ok(resp) => {
                warn!(status = %resp.status(), "Wallet status check failed");
                Ok(false)
            }
            Err(e) => {
                warn!(error = %e, "Cannot reach Ergo node wallet endpoint");
                Ok(false)
            }
        }
    }

    /// Send a single ERG payment to a recipient address.
    pub async fn send_payment(&self, recipient: &str, nanoerg_amount: u64) -> Result<String> {
        if nanoerg_amount == 0 {
            anyhow::bail!("Cannot send zero ERG");
        }

        if nanoerg_amount < SAFE_MIN_BOX_VALUE {
            anyhow::bail!(
                "Amount {} nanoERG is below safe minimum box value {} nanoERG (dust prevention)",
                nanoerg_amount,
                SAFE_MIN_BOX_VALUE
            );
        }

        validate_address_or_tree(recipient)
            .context("Invalid recipient address")?;

        let url = format!(
            "{}/wallet/payment/send",
            self.ergo_rest_url.trim_end_matches('/')
        );

        let request = ErgoPaymentRequest {
            amount: nanoerg_amount,
            recipient: recipient.to_string(),
            fee: Some(self.default_fee),
        };

        info!(
            recipient = %recipient,
            amount_nanoerg = nanoerg_amount,
            erg = nanoerg_amount as f64 / 1_000_000_000.0,
            "Sending ERG payment"
        );

        let resp = self
            .http_client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Failed to send ERG payment")?;

        let status = resp.status();
        let body: serde_json::Value = resp.json().await.unwrap_or_default();

        if !status.is_success() {
            let error_msg = body
                .get("detail")
                .or_else(|| body.get("error"))
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");

            anyhow::bail!("ERG payment failed ({}): {}", status, error_msg);
        }

        let tx_id = body
            .get("txId")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        info!(tx_id = %tx_id, "ERG payment broadcast successfully");
        Ok(tx_id)
    }

    /// Send all payments in a settlement batch.
    /// Updates payment statuses in-place.
    pub async fn send_batch(&self, batch: &mut SettlementBatch) {
        // Check wallet first
        match self.check_wallet().await {
            Ok(true) => info!("Wallet is unlocked, proceeding with settlement"),
            Ok(false) => {
                batch.status = super::models::BatchStatus::Failed(
                    "Ergo node wallet is locked or unavailable".into(),
                );
                for payment in &mut batch.payments {
                    payment.status = PaymentStatus::Failed("Wallet locked".into());
                }
                return;
            }
            Err(e) => {
                batch.status =
                    super::models::BatchStatus::Failed(format!("Cannot check wallet: {}", e));
                for payment in &mut batch.payments {
                    payment.status = PaymentStatus::Failed("Wallet check failed".into());
                }
                return;
            }
        }

        let mut all_success = true;
        let mut sent_count = 0;
        let total_payments = batch.payments.len();

        for payment in &mut batch.payments {
            match self
                .send_payment(&payment.ergo_address, payment.erg_nano)
                .await
            {
                Ok(tx_id) => {
                    payment.tx_id = Some(tx_id);
                    payment.status = PaymentStatus::Broadcast;
                    sent_count += 1;
                }
                Err(e) => {
                    warn!(
                        provider_id = %payment.provider_id,
                        error = %e,
                        "Failed to send payment to provider"
                    );
                    payment.status = PaymentStatus::Failed(e.to_string());
                    all_success = false;
                }
            }

            // Small delay between transactions to avoid rate limiting
            if sent_count < total_payments {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }

        if all_success {
            batch.status = super::models::BatchStatus::Submitted;
            info!(
                batch_id = %batch.batch_id,
                payments = sent_count,
                total_erg = batch.total_erg,
                "Settlement batch submitted successfully"
            );
        } else {
            batch.status = super::models::BatchStatus::Failed(format!(
                "{}/{} payments failed",
                batch.payments.len() - sent_count,
                batch.payments.len()
            ));
            warn!(
                batch_id = %batch.batch_id,
                succeeded = sent_count,
                failed = batch.payments.len() - sent_count,
                "Settlement batch partially failed"
            );
        }
    }

    /// Check if a transaction has been included in a block.
    /// Returns Some(height) if confirmed, None otherwise.
    pub async fn check_confirmation(&self, tx_id: &str) -> Result<Option<u32>> {
        let url = format!(
            "{}/transactions/byId/{}",
            self.ergo_rest_url.trim_end_matches('/'),
            tx_id
        );

        match self.http_client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let body: serde_json::Value = resp.json().await?;
                let inclusion_height = body
                    .get("inclusionHeight")
                    .and_then(|v| v.as_u64())
                    .map(|h| h as u32);
                Ok(inclusion_height)
            }
            Ok(_resp) => {
                // 404 = not yet in a block (still in mempool or unknown)
                Ok(None)
            }
            Err(_) => Ok(None),
        }
    }
}
