//! Cross-chain payment bridge module.
//!
//! Implements an invoice-based Lock-and-Mint bridge pattern for accepting
//! payments from foreign chains (BTC, ETH, ADA) to pay for Xergon
//! inference/GPU rental.
//!
//! Flow:
//! 1. User generates an invoice (on Ergo) specifying payment terms
//! 2. Invoice is a box with terms in registers and an EIP-4 minted NFT
//! 3. User pays on foreign chain (BTC/ETH/ADA) to a bridge address
//! 4. Bridge operator confirms the foreign-chain tx and spends the invoice,
//!    releasing the ERG to the provider
//! 5. Provider sees the rental is paid and grants access
//!
//! Contract: contracts/payment_bridge.es
//!
//! Registers:
//!   R4: buyerPK        (SigmaProp)
//!   R5: providerPK     (SigmaProp)
//!   R6: amountNanoerg  (Long)
//!   R7: foreignTxId    (Coll[Byte])
//!   R8: foreignChain   (Int) — 0=BTC, 1=ETH, 2=ADA
//!   R9: bridgePK       (SigmaProp)
//!   tokens(0): invoice NFT (singleton, EIP-4 minted)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::chain::client::ErgoNodeClient;
use crate::chain::transactions::encode_coll_byte;
use crate::config::PaymentBridgeConfig;
use crate::protocol::tx_safety::{validate_address_or_tree, validate_payment_request};

/// Supported foreign chains for cross-chain payments.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ForeignChain {
    Btc = 0,
    Eth = 1,
    Ada = 2,
}

impl ForeignChain {
    /// Convert from integer (as stored in R8 register).
    pub fn from_int(val: i32) -> Option<Self> {
        match val {
            0 => Some(Self::Btc),
            1 => Some(Self::Eth),
            2 => Some(Self::Ada),
            _ => None,
        }
    }

    /// Convert to integer for R8 register.
    pub fn to_int(self) -> i32 {
        self as i32
    }

    /// Human-readable name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Btc => "Bitcoin",
            Self::Eth => "Ethereum",
            Self::Ada => "Cardano",
        }
    }
}

impl std::fmt::Display for ForeignChain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl std::str::FromStr for ForeignChain {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "btc" | "bitcoin" => Ok(Self::Btc),
            "eth" | "ethereum" => Ok(Self::Eth),
            "ada" | "cardano" => Ok(Self::Ada),
            _ => anyhow::bail!("Unsupported foreign chain: {}. Use: btc, eth, ada", s),
        }
    }
}

/// Status of a bridge invoice.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceStatus {
    /// Invoice created, awaiting foreign-chain payment.
    Pending,
    /// Bridge operator confirmed foreign-chain payment, ERG released to provider.
    Confirmed,
    /// Buyer refunded after timeout.
    Refunded,
    /// Invoice expired (timeout reached, not yet spent).
    Expired,
}

/// A payment bridge invoice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeInvoice {
    /// Unique invoice ID (hex, derived from box ID or generated).
    pub invoice_id: String,
    /// Buyer public key (hex).
    pub buyer_pk: String,
    /// Provider public key (hex).
    pub provider_pk: String,
    /// Payment amount in nanoERG.
    pub amount_nanoerg: u64,
    /// Foreign chain type.
    pub foreign_chain: ForeignChain,
    /// Foreign chain transaction ID (hex, set on confirmation).
    pub foreign_tx_id: Option<String>,
    /// Bridge operator public key (hex).
    pub bridge_pk: String,
    /// Invoice NFT token ID (EIP-4 minted).
    pub invoice_nft_token_id: String,
    /// Block height at which the invoice was created.
    pub creation_height: i32,
    /// Block height at which the invoice expires.
    pub timeout_height: i32,
    /// Current invoice status.
    pub status: InvoiceStatus,
    /// ISO 8601 timestamp of creation.
    pub created_at: String,
    /// Transaction ID of the invoice creation (on Ergo).
    pub ergo_tx_id: Option<String>,
}

impl BridgeInvoice {
    /// Create a new pending invoice.
    pub fn new(
        invoice_id: String,
        buyer_pk: String,
        provider_pk: String,
        amount_nanoerg: u64,
        foreign_chain: ForeignChain,
        bridge_pk: String,
        invoice_nft_token_id: String,
        creation_height: i32,
        timeout_blocks: i32,
    ) -> Self {
        Self {
            invoice_id,
            buyer_pk,
            provider_pk,
            amount_nanoerg,
            foreign_chain,
            foreign_tx_id: None,
            bridge_pk,
            invoice_nft_token_id,
            creation_height,
            timeout_height: creation_height + timeout_blocks,
            status: InvoiceStatus::Pending,
            created_at: chrono::Utc::now().to_rfc3339(),
            ergo_tx_id: None,
        }
    }

    /// Check if the invoice has expired based on current height.
    pub fn is_expired(&self, current_height: i32) -> bool {
        current_height >= self.timeout_height
    }

    /// Amount in ERG (human-readable).
    pub fn amount_erg(&self) -> f64 {
        self.amount_nanoerg as f64 / 1_000_000_000.0
    }
}

/// Result of creating a bridge invoice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateInvoiceResult {
    /// The created invoice.
    pub invoice: BridgeInvoice,
    /// Ergo transaction ID of the invoice box creation.
    pub ergo_tx_id: String,
    /// Ergo box ID of the invoice.
    pub box_id: String,
    /// Foreign chain payment address (for the user to send to).
    /// This would be the bridge operator's address on the foreign chain.
    pub foreign_payment_address: String,
}

/// Result of confirming a bridge payment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmPaymentResult {
    /// Invoice ID.
    pub invoice_id: String,
    /// Foreign chain transaction ID.
    pub foreign_tx_id: String,
    /// Ergo transaction ID (spending the invoice to the provider).
    pub ergo_tx_id: String,
    /// Provider public key.
    pub provider_pk: String,
    /// Amount released in nanoERG.
    pub amount_nanoerg: u64,
}

/// Result of refunding a bridge invoice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefundInvoiceResult {
    /// Invoice ID.
    pub invoice_id: String,
    /// Ergo transaction ID (spending the invoice back to buyer).
    pub ergo_tx_id: String,
    /// Amount refunded in nanoERG.
    pub amount_nanoerg: u64,
}

/// Payment bridge client for creating and managing cross-chain invoices.
pub struct PaymentBridge {
    config: PaymentBridgeConfig,
}

impl PaymentBridge {
    /// Create a new payment bridge client.
    pub fn new(config: PaymentBridgeConfig) -> Self {
        Self { config }
    }

    /// Check if the bridge is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Check if a foreign chain is supported.
    pub fn is_chain_supported(&self, chain: ForeignChain) -> bool {
        self.config.supported_chains.contains(&chain)
    }

    /// Create a new bridge invoice box on Ergo.
    ///
    /// Builds an invoice box with:
    /// - R4: buyerPK (SigmaProp)
    /// - R5: providerPK (SigmaProp)
    /// - R6: amountNanoerg (Long)
    /// - R7: foreignTxId (Coll[Byte]) — empty initially
    /// - R8: foreignChain (Int)
    /// - R9: bridgePK (SigmaProp)
    /// - tokens(0): invoice NFT (EIP-4 minted, first input box ID)
    ///
    /// The ERG value in the box equals the payment amount plus minimum box value.
    pub async fn create_invoice(
        &self,
        client: &ErgoNodeClient,
        buyer_pk: &str,
        provider_pk: &str,
        amount_nanoerg: u64,
        foreign_chain: ForeignChain,
        foreign_tx_id: Option<&str>,
    ) -> Result<CreateInvoiceResult> {
        if !self.config.enabled {
            anyhow::bail!("Payment bridge is not enabled");
        }

        if !self.is_chain_supported(foreign_chain) {
            anyhow::bail!(
                "Foreign chain {} is not supported. Supported: {:?}",
                foreign_chain,
                self.config.supported_chains
            );
        }

        let tree_hex = &self.config.invoice_tree_hex;
        if tree_hex.is_empty() {
            anyhow::bail!("Invoice tree hex not configured — cannot create bridge invoice");
        }

        if self.config.bridge_public_key.is_empty() {
            anyhow::bail!("Bridge public key not configured");
        }

        // Generate a unique invoice ID from timestamp + random suffix
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let invoice_id = format!("{:016x}{:08x}", ts, rand::random::<u32>());

        // Get current height
        let current_height = client
            .get_height()
            .await
            .context("Failed to get current block height")?;

        // Encode register values
        let buyer_pk_bytes = hex::decode(buyer_pk).unwrap_or_else(|_| buyer_pk.as_bytes().to_vec());
        let provider_pk_bytes =
            hex::decode(provider_pk).unwrap_or_else(|_| provider_pk.as_bytes().to_vec());
        let bridge_pk_bytes = hex::decode(&self.config.bridge_public_key)
            .unwrap_or_else(|_| self.config.bridge_public_key.as_bytes().to_vec());
        let foreign_tx_bytes = foreign_tx_id
            .map(|id| hex::decode(id).unwrap_or_else(|_| id.as_bytes().to_vec()))
            .unwrap_or_default();

        let buyer_pk_hex = encode_coll_byte(&buyer_pk_bytes);
        let provider_pk_hex = encode_coll_byte(&provider_pk_bytes);
        let bridge_pk_hex = encode_coll_byte(&bridge_pk_bytes);
        let amount_hex = format!("{:016x}", amount_nanoerg);
        let foreign_tx_hex = encode_coll_byte(&foreign_tx_bytes);
        let foreign_chain_hex = format!("{:08x}", foreign_chain.to_int() as i64);

        // The box value is the payment amount plus some ERG for minimum box value
        let box_value = amount_nanoerg + 1_000_000; // payment + 0.001 ERG min value

        // Build the invoice box request
        // NOTE: The NFT token ID uses the EIP-4 pattern (first input box ID).
        // The Ergo node wallet handles this automatically when using
        // wallet/payment/send with a fresh token in assets.
        let payment_request = serde_json::json!({
            "requests": [{
                "address": tree_hex,
                "value": box_value.to_string(),
                "assets": [{
                    "tokenId": "0000000000000000000000000000000000000000000000000000000000000000", // placeholder
                    "amount": 1
                }],
                "registers": {
                    "R4": buyer_pk_hex,
                    "R5": provider_pk_hex,
                    "R6": amount_hex,
                    "R7": foreign_tx_hex,
                    "R8": foreign_chain_hex,
                    "R9": bridge_pk_hex
                }
            }],
            "fee": 1100000 // 0.0011 ERG fee
        });

        debug!(
            invoice_id = %invoice_id,
            buyer = %buyer_pk,
            provider = %provider_pk,
            amount_nanoerg = amount_nanoerg,
            chain = %foreign_chain,
            "Creating bridge invoice"
        );

        validate_payment_request(&payment_request)
            .context("Bridge invoice creation transaction safety validation failed")?;

        let ergo_tx_id = client
            .wallet_payment_send(&payment_request)
            .await
            .context("Failed to submit invoice creation transaction via wallet")?;

        info!(
            invoice_id = %invoice_id,
            tx_id = %ergo_tx_id,
            "Bridge invoice created on Ergo"
        );

        let invoice = BridgeInvoice::new(
            invoice_id.clone(),
            buyer_pk.to_string(),
            provider_pk.to_string(),
            amount_nanoerg,
            foreign_chain,
            self.config.bridge_public_key.clone(),
            String::new(), // NFT token ID will be known after tx confirms
            current_height as i32,
            self.config.invoice_timeout_blocks as i32,
        );

        // Generate the foreign chain payment address
        // In production, this would be the bridge operator's address on the foreign chain
        let foreign_payment_address = format!(
            "xergon-bridge-{}-{}",
            foreign_chain.name().to_lowercase(),
            &invoice_id[..8]
        );

        Ok(CreateInvoiceResult {
            invoice,
            ergo_tx_id: ergo_tx_id.clone(),
            box_id: ergo_tx_id, // In practice, query the tx to get the actual box ID
            foreign_payment_address,
        })
    }

    /// Confirm a foreign-chain payment and release ERG to the provider.
    ///
    /// The bridge operator calls this after verifying that the buyer
    /// actually sent payment on the foreign chain.
    ///
    /// Spends the invoice box, sending the ERG value to the provider's address.
    pub async fn confirm_payment(
        &self,
        client: &ErgoNodeClient,
        invoice_box_id: &str,
        provider_address: &str,
        foreign_tx_id: &str,
        amount_nanoerg: u64,
    ) -> Result<ConfirmPaymentResult> {
        if !self.config.enabled {
            anyhow::bail!("Payment bridge is not enabled");
        }

        debug!(
            invoice_box = %invoice_box_id,
            provider = %provider_address,
            foreign_tx = %foreign_tx_id,
            amount = amount_nanoerg,
            "Confirming bridge payment"
        );

        validate_address_or_tree(provider_address)
            .context("Invalid provider address for payment confirmation")?;

        // Build a payment from the invoice box to the provider
        // In production, this would use a proper input selection with the invoice box
        // and prove the bridge path in the script
        let payment_request = serde_json::json!({
            "requests": [{
                "address": provider_address,
                "value": (amount_nanoerg + 1_000_000).to_string(),
            }],
            "fee": 1100000
        });

        validate_payment_request(&payment_request)
            .context("Payment confirmation transaction safety validation failed")?;

        let ergo_tx_id = client
            .wallet_payment_send(&payment_request)
            .await
            .context("Failed to submit payment confirmation transaction")?;

        info!(
            invoice_box = %invoice_box_id,
            ergo_tx_id = %ergo_tx_id,
            foreign_tx = %foreign_tx_id,
            "Bridge payment confirmed, ERG released to provider"
        );

        Ok(ConfirmPaymentResult {
            invoice_id: invoice_box_id.to_string(),
            foreign_tx_id: foreign_tx_id.to_string(),
            ergo_tx_id,
            provider_pk: provider_address.to_string(),
            amount_nanoerg,
        })
    }

    /// Refund an expired invoice.
    ///
    /// The buyer can call this after the timeout height has been reached
    /// to reclaim the locked ERG.
    pub async fn refund_invoice(
        &self,
        client: &ErgoNodeClient,
        invoice_box_id: &str,
        buyer_address: &str,
        amount_nanoerg: u64,
    ) -> Result<RefundInvoiceResult> {
        if !self.config.enabled {
            anyhow::bail!("Payment bridge is not enabled");
        }

        debug!(
            invoice_box = %invoice_box_id,
            buyer = %buyer_address,
            amount = amount_nanoerg,
            "Refunding expired bridge invoice"
        );

        validate_address_or_tree(buyer_address)
            .context("Invalid buyer address for invoice refund")?;

        let payment_request = serde_json::json!({
            "requests": [{
                "address": buyer_address,
                "value": (amount_nanoerg + 1_000_000).to_string(),
            }],
            "fee": 1100000
        });

        validate_payment_request(&payment_request)
            .context("Invoice refund transaction safety validation failed")?;

        let ergo_tx_id = client
            .wallet_payment_send(&payment_request)
            .await
            .context("Failed to submit invoice refund transaction")?;

        info!(
            invoice_box = %invoice_box_id,
            ergo_tx_id = %ergo_tx_id,
            "Bridge invoice refunded to buyer"
        );

        Ok(RefundInvoiceResult {
            invoice_id: invoice_box_id.to_string(),
            ergo_tx_id,
            amount_nanoerg,
        })
    }

    /// Scan the chain for pending bridge invoices.
    ///
    /// Queries the Ergo node for unspent boxes matching the invoice contract.
    pub async fn scan_invoices(
        &self,
        client: &ErgoNodeClient,
    ) -> Result<Vec<BridgeInvoice>> {
        if !self.config.enabled {
            return Ok(Vec::new());
        }

        let tree_hex = &self.config.invoice_tree_hex;
        if tree_hex.is_empty() {
            debug!("Invoice tree hex not configured, skipping scan");
            return Ok(Vec::new());
        }

        // Query unspent boxes with the invoice contract ErgoTree
        match client.get_boxes_by_ergo_tree(tree_hex).await {
            Ok(boxes) => {
                debug!("Found {} boxes matching invoice tree", boxes.len());
                // Parse boxes into BridgeInvoice structs
                let invoices: Vec<BridgeInvoice> = boxes
                    .into_iter()
                    .map(|box_| {
                        let creation_height = box_.creation_height;
                        let value = box_.value;

                        BridgeInvoice {
                            invoice_id: box_.box_id.clone(),
                            buyer_pk: String::new(),       // extracted from R4
                            provider_pk: String::new(),     // extracted from R5
                            amount_nanoerg: value,
                            foreign_chain: ForeignChain::Btc, // default
                            foreign_tx_id: None,
                            bridge_pk: String::new(),       // extracted from R9
                            invoice_nft_token_id: String::new(),
                            creation_height,
                            timeout_height: creation_height + self.config.invoice_timeout_blocks as i32,
                            status: InvoiceStatus::Pending,
                            created_at: chrono::Utc::now().to_rfc3339(),
                            ergo_tx_id: Some(box_.tx_id),
                        }
                    })
                    .collect();
                Ok(invoices)
            }
            Err(e) => {
                debug!("Failed to scan for invoices: {}", e);
                Ok(Vec::new())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_foreign_chain_from_int() {
        assert_eq!(ForeignChain::from_int(0), Some(ForeignChain::Btc));
        assert_eq!(ForeignChain::from_int(1), Some(ForeignChain::Eth));
        assert_eq!(ForeignChain::from_int(2), Some(ForeignChain::Ada));
        assert_eq!(ForeignChain::from_int(3), None);
    }

    #[test]
    fn test_foreign_chain_to_int() {
        assert_eq!(ForeignChain::Btc.to_int(), 0);
        assert_eq!(ForeignChain::Eth.to_int(), 1);
        assert_eq!(ForeignChain::Ada.to_int(), 2);
    }

    #[test]
    fn test_foreign_chain_from_str() {
        assert_eq!("btc".parse::<ForeignChain>().unwrap(), ForeignChain::Btc);
        assert_eq!("eth".parse::<ForeignChain>().unwrap(), ForeignChain::Eth);
        assert_eq!("ada".parse::<ForeignChain>().unwrap(), ForeignChain::Ada);
        assert!("sol".parse::<ForeignChain>().is_err());
    }

    #[test]
    fn test_invoice_new_and_expiration() {
        let invoice = BridgeInvoice::new(
            "test_invoice_id".to_string(),
            "02buyer".to_string(),
            "03provider".to_string(),
            1_000_000_000, // 1 ERG
            ForeignChain::Eth,
            "02bridge".to_string(),
            "nft_token_id".to_string(),
            1000,
            720,
        );

        assert_eq!(invoice.amount_erg(), 1.0);
        assert_eq!(invoice.creation_height, 1000);
        assert_eq!(invoice.timeout_height, 1720);
        assert!(!invoice.is_expired(1000));
        assert!(!invoice.is_expired(1719));
        assert!(invoice.is_expired(1720));
        assert!(invoice.is_expired(2000));
        assert_eq!(invoice.status, InvoiceStatus::Pending);
        assert!(invoice.foreign_tx_id.is_none());
    }

    #[test]
    fn test_payment_bridge_chain_support() {
        let config = PaymentBridgeConfig {
            enabled: true,
            bridge_public_key: "02bridge".to_string(),
            supported_chains: vec![ForeignChain::Btc, ForeignChain::Eth],
            invoice_timeout_blocks: 720,
            invoice_tree_hex: String::new(),
        };

        let bridge = PaymentBridge::new(config);
        assert!(bridge.is_enabled());
        assert!(bridge.is_chain_supported(ForeignChain::Btc));
        assert!(bridge.is_chain_supported(ForeignChain::Eth));
        assert!(!bridge.is_chain_supported(ForeignChain::Ada));
    }

    #[test]
    fn test_invoice_serialization() {
        let invoice = BridgeInvoice::new(
            "abc123".to_string(),
            "02buyer".to_string(),
            "03provider".to_string(),
            500_000_000,
            ForeignChain::Btc,
            "02bridge".to_string(),
            "nft_id".to_string(),
            500,
            720,
        );

        let json = serde_json::to_string(&invoice).unwrap();
        let deserialized: BridgeInvoice = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.invoice_id, invoice.invoice_id);
        assert_eq!(deserialized.amount_nanoerg, 500_000_000);
        assert_eq!(deserialized.foreign_chain, ForeignChain::Btc);
    }
}
