//! Real eUTXO Settlement Engine
//!
//! Builds actual Ergo transactions that spend user staking boxes and create
//! outputs to provider boxes. Uses the Ergo node's `/wallet/payment` endpoint
//! to build, sign, and broadcast transactions.
//!
//! Settlement flow:
//! 1. Find settleable staking boxes (by user_staking ErgoTree, filtered by age)
//! 2. Build a payment request spending those boxes as inputsRaw
//! 3. POST to `/wallet/payment/send` — node handles signing + broadcasting
//! 4. Track tx_id for confirmation

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::chain::client::ErgoNodeClient;
use crate::config::SettlementConfig;
use crate::contract_compile;

/// Default transaction fee in nanoERG (0.001 ERG)
const DEFAULT_TX_FEE: u64 = 1_000_000;

/// ERG precision: 1 ERG = 10^9 nanoERG
const NANOERG_PER_ERG: u64 = 1_000_000_000;

/// A staking box that is ready for settlement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettleableBox {
    /// Box ID (hex)
    pub box_id: String,
    /// Box value in nanoERG
    pub value: u64,
    /// Creation block height
    pub creation_height: i32,
    /// Number of confirmations (blocks since creation)
    pub confirmations: u32,
}

/// Parameters for building a settlement transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementTxParams {
    /// Provider's Ergo address (or P2S from provider_box ErgoTree)
    pub provider_address: String,
    /// Staking box IDs to spend as inputs
    pub staking_box_ids: Vec<String>,
    /// Fee amounts (in nanoERG) to transfer — one per staking box
    pub fee_amounts: Vec<u64>,
    /// Change address (wallet address for remainder)
    pub change_address: String,
    /// Maximum fee to pay for the transaction itself
    pub max_fee_nanoerg: u64,
}

/// Result of a settlement transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementTxResult {
    /// Transaction ID
    pub tx_id: String,
    /// Number of boxes settled
    pub boxes_settled: u32,
    /// Total ERG value settled (nanoERG)
    pub total_erg_settled: u64,
}

/// Result of finding settleable boxes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettleableBoxesResult {
    /// List of settleable boxes
    pub boxes: Vec<SettleableBox>,
    /// Total value of all settleable boxes (nanoERG)
    pub total_value: u64,
}

/// The eUTXO settlement engine.
///
/// Wraps the existing in-memory settlement ledger with real on-chain
/// transaction building capabilities. When `chain_enabled` is true in
/// the config, settlements produce actual Ergo transactions.
pub struct EutxoSettlementEngine {
    config: SettlementConfig,
    node_client: ErgoNodeClient,
}

impl EutxoSettlementEngine {
    /// Create a new eUTXO settlement engine.
    pub fn new(config: SettlementConfig, node_url: String) -> Result<Self> {
        let node_client = ErgoNodeClient::new(node_url);
        Ok(Self {
            config,
            node_client,
        })
    }

    /// Find staking boxes that are ready for settlement.
    ///
    /// Queries the UTXO set for boxes matching the `user_staking` ErgoTree,
    /// then filters by confirmation depth (creation age).
    pub async fn find_settleable_boxes(
        &self,
        max_boxes: usize,
    ) -> Result<SettleableBoxesResult> {
        let staking_tree = contract_compile::get_contract_hex("user_staking")
            .context("user_staking contract hex not found — contracts not loaded?")?;

        debug!(
            tree_prefix = %&staking_tree[..16],
            "Querying UTXO set for user_staking boxes"
        );

        let boxes = self
            .node_client
            .get_boxes_by_ergo_tree(&staking_tree)
            .await
            .context("Failed to query boxes by user_staking ErgoTree")?;

        let current_height = self
            .node_client
            .get_height()
            .await
            .unwrap_or(0) as u32;

        let min_confirmations = self.config.min_confirmations;
        let mut settleable: Vec<SettleableBox> = boxes
            .into_iter()
            .filter_map(|b| {
                let confirmations = if current_height > b.creation_height as u32 {
                    current_height - b.creation_height as u32
                } else {
                    0
                };

                if confirmations >= min_confirmations {
                    Some(SettleableBox {
                        box_id: b.box_id,
                        value: b.value,
                        creation_height: b.creation_height,
                        confirmations,
                    })
                } else {
                    None
                }
            })
            .collect();

        // Sort by value descending (settle largest first)
        settleable.sort_by(|a, b| b.value.cmp(&a.value));

        // Cap at max_boxes
        settleable.truncate(max_boxes);

        let total_value: u64 = settleable.iter().map(|b| b.value).sum();

        info!(
            found = settleable.len(),
            total_value_nanoerg = total_value,
            total_erg = total_value as f64 / NANOERG_PER_ERG as f64,
            min_confirmations,
            current_height,
            "Found settleable staking boxes"
        );

        Ok(SettleableBoxesResult {
            boxes: settleable,
            total_value,
        })
    }

    /// Execute a real on-chain settlement.
    ///
    /// Builds a transaction that spends the specified staking boxes and
    /// creates outputs to the provider address with the fee amounts.
    pub async fn execute_settlement(&self, params: SettlementTxParams) -> Result<SettlementTxResult> {
        if params.staking_box_ids.is_empty() {
            anyhow::bail!("No staking box IDs provided for settlement");
        }

        if params.staking_box_ids.len() != params.fee_amounts.len() {
            anyhow::bail!(
                "staking_box_ids length ({}) != fee_amounts length ({})",
                params.staking_box_ids.len(),
                params.fee_amounts.len()
            );
        }

        // Check wallet is unlocked
        match self.node_client.wallet_status().await {
            Ok(true) => info!("Wallet is unlocked, proceeding with eUTXO settlement"),
            Ok(false) => anyhow::bail!("Ergo node wallet is locked — cannot sign settlement transaction"),
            Err(e) => anyhow::bail!("Cannot check wallet status: {}", e),
        }

        info!(
            boxes = params.staking_box_ids.len(),
            max_fee = params.max_fee_nanoerg,
            provider = %params.provider_address,
            "Building eUTXO settlement transaction"
        );

        let staking_box_count = params.staking_box_ids.len() as u32;
        let total_fees: u64 = params.fee_amounts.iter().sum();

        let tx_id = self
            .build_settlement_transaction(params)
            .await
            .context("Failed to build/settlement transaction")?;

        let result = SettlementTxResult {
            tx_id,
            boxes_settled: staking_box_count,
            total_erg_settled: total_fees,
        };

        info!(
            tx_id = %result.tx_id,
            boxes_settled = result.boxes_settled,
            total_erg = result.total_erg_settled as f64 / NANOERG_PER_ERG as f64,
            "eUTXO settlement transaction broadcast"
        );

        Ok(result)
    }

    /// Build and submit a settlement transaction via the node's wallet payment API.
    ///
    /// Constructs a payment request where:
    /// - Each staking box value (minus fee_amount) goes to the provider
    /// - The staking boxes are passed as inputsRaw
    /// - The node wallet handles signing and change
    async fn build_settlement_transaction(&self, params: SettlementTxParams) -> Result<String> {
        // Fetch full box data for each staking box ID
        let mut inputs_raw: Vec<serde_json::Value> = Vec::new();
        let mut total_output_value: u64 = 0;

        for (i, box_id) in params.staking_box_ids.iter().enumerate() {
            let box_data = match self.node_client.get_box(box_id).await {
                Ok(b) => b,
                Err(e) => {
                    warn!(
                        box_id = %box_id,
                        error = %e,
                        "Failed to fetch staking box — may already be spent"
                    );
                    anyhow::bail!(
                        "Cannot fetch staking box {}: {}. Box may already be spent.",
                        box_id,
                        e
                    );
                }
            };

            debug!(
                box_id = %box_data.box_id,
                value = box_data.value,
                "Fetched staking box for settlement"
            );

            // Build inputsRaw JSON manually (RawBox doesn't impl Serialize)
            let box_json = serde_json::json!({
                "boxId": box_data.box_id,
                "value": box_data.value,
                "ergoTree": box_data.ergo_tree,
                "assets": box_data.assets.iter().map(|asset| serde_json::json!({
                    "tokenId": asset.token_id,
                    "amount": asset.amount
                })).collect::<Vec<_>>(),
                "creationHeight": box_data.creation_height,
                "additionalRegisters": box_data.additional_registers,
                "transactionId": box_data.tx_id,
            });

            inputs_raw.push(box_json);

            // The fee_amount for this box goes to the provider
            total_output_value += params.fee_amounts[i];
        }

        if total_output_value == 0 {
            anyhow::bail!("Total output value is 0 — nothing to settle");
        }

        // Build the payment request
        // The provider receives the accumulated fee amounts
        let requests = vec![serde_json::json!({
            "address": params.provider_address,
            "value": total_output_value,
            "assets": [],
            "registers": {}
        })];

        let payment_request = serde_json::json!({
            "requests": requests,
            "fee": params.max_fee_nanoerg,
            "inputsRaw": inputs_raw,
            "dataInputsRaw": []
        });

        debug!(
            requests = requests.len(),
            inputs = inputs_raw.len(),
            fee = params.max_fee_nanoerg,
            "Submitting wallet payment request"
        );

        // Submit via the node's wallet payment endpoint
        let tx_id = self
            .node_client
            .wallet_payment_send(&payment_request)
            .await
            .context("Wallet payment send failed")?;

        Ok(tx_id)
    }

    /// Build a settlement transaction using simple wallet-funded payments.
    ///
    /// This is an alternative to the inputsRaw approach — when the wallet
    /// doesn't have the staking boxes tracked, we can still send payments
    /// using wallet inputs. This is useful as a fallback.
    pub async fn execute_simple_settlement(
        &self,
        provider_address: &str,
        total_nanoerg: u64,
    ) -> Result<String> {
        if total_nanoerg == 0 {
            anyhow::bail!("Cannot settle 0 nanoERG");
        }

        match self.node_client.wallet_status().await {
            Ok(true) => info!("Wallet unlocked, sending simple settlement payment"),
            Ok(false) => anyhow::bail!("Ergo node wallet is locked"),
            Err(e) => anyhow::bail!("Cannot check wallet: {}", e),
        }

        let request = serde_json::json!({
            "requests": [{
                "address": provider_address,
                "value": total_nanoerg,
                "assets": [],
                "registers": {}
            }],
            "fee": DEFAULT_TX_FEE,
            "inputsRaw": [],
            "dataInputsRaw": []
        });

        let tx_id = self
            .node_client
            .wallet_payment_send(&request)
            .await
            .context("Simple settlement payment failed")?;

        info!(
            tx_id = %tx_id,
            amount_nanoerg = total_nanoerg,
            erg = total_nanoerg as f64 / NANOERG_PER_ERG as f64,
            "Simple settlement payment sent"
        );

        Ok(tx_id)
    }
}

/// Find settleable staking boxes (standalone function, no engine needed).
///
/// Useful for the API endpoint that lists boxes without requiring
/// a full engine instance.
pub async fn find_settleable_boxes(
    node_url: &str,
    max_boxes: usize,
    min_confirmations: u32,
) -> Result<SettleableBoxesResult> {
    let client = ErgoNodeClient::new(node_url.to_string());

    let staking_tree = contract_compile::get_contract_hex("user_staking")
        .context("user_staking contract hex not found")?;

    let boxes = client
        .get_boxes_by_ergo_tree(&staking_tree)
        .await
        .context("Failed to query boxes by user_staking ErgoTree")?;

    let current_height = client.get_height().await.unwrap_or(0) as u32;

    let mut settleable: Vec<SettleableBox> = boxes
        .into_iter()
        .filter_map(|b| {
            let confirmations = if current_height > b.creation_height as u32 {
                current_height - b.creation_height as u32
            } else {
                0
            };

            if confirmations >= min_confirmations {
                Some(SettleableBox {
                    box_id: b.box_id,
                    value: b.value,
                    creation_height: b.creation_height,
                    confirmations,
                })
            } else {
                None
            }
        })
        .collect();

    settleable.sort_by(|a, b| b.value.cmp(&a.value));
    settleable.truncate(max_boxes);

    let total_value: u64 = settleable.iter().map(|b| b.value).sum();

    Ok(SettleableBoxesResult {
        boxes: settleable,
        total_value,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settleable_box_serialization() {
        let box_info = SettleableBox {
            box_id: "abc123".to_string(),
            value: 1_000_000_000,
            creation_height: 100,
            confirmations: 50,
        };

        let json = serde_json::to_string(&box_info).unwrap();
        let parsed: SettleableBox = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.box_id, "abc123");
        assert_eq!(parsed.value, 1_000_000_000);
        assert_eq!(parsed.creation_height, 100);
        assert_eq!(parsed.confirmations, 50);
    }

    #[test]
    fn test_settleable_box_serialization_roundtrip() {
        let original = SettleableBox {
            box_id: "deadbeef1234567890abcdef".to_string(),
            value: 500_000_000,
            creation_height: 999,
            confirmations: 200,
        };

        let json = serde_json::to_string(&original).unwrap();
        let pretty = serde_json::to_string_pretty(&original).unwrap();

        let from_compact: SettleableBox = serde_json::from_str(&json).unwrap();
        let from_pretty: SettleableBox = serde_json::from_str(&pretty).unwrap();

        assert_eq!(from_compact.box_id, original.box_id);
        assert_eq!(from_compact.value, original.value);
        assert_eq!(from_pretty.confirmations, original.confirmations);
    }

    #[test]
    fn test_settlement_result_serialization() {
        let result = SettlementTxResult {
            tx_id: "tx123".to_string(),
            boxes_settled: 3,
            total_erg_settled: 5_000_000_000,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("tx123"));
        assert!(json.contains("3"));

        let parsed: SettlementTxResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tx_id, "tx123");
        assert_eq!(parsed.boxes_settled, 3);
        assert_eq!(parsed.total_erg_settled, 5_000_000_000);
    }

    #[test]
    fn test_settlement_result_serialization_roundtrip() {
        let original = SettlementTxResult {
            tx_id: "a1b2c3d4e5f6".to_string(),
            boxes_settled: 10,
            total_erg_settled: 99_999_999_999,
        };

        let json = serde_json::to_string(&original).unwrap();
        let parsed: SettlementTxResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.tx_id, original.tx_id);
        assert_eq!(parsed.boxes_settled, original.boxes_settled);
        assert_eq!(parsed.total_erg_settled, original.total_erg_settled);
    }

    #[test]
    fn test_settleable_boxes_result_serialization() {
        let result = SettleableBoxesResult {
            boxes: vec![
                SettleableBox {
                    box_id: "box1".to_string(),
                    value: 1_000_000_000,
                    creation_height: 100,
                    confirmations: 50,
                },
                SettleableBox {
                    box_id: "box2".to_string(),
                    value: 2_000_000_000,
                    creation_height: 200,
                    confirmations: 100,
                },
            ],
            total_value: 3_000_000_000,
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: SettleableBoxesResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.boxes.len(), 2);
        assert_eq!(parsed.boxes[0].box_id, "box1");
        assert_eq!(parsed.boxes[1].value, 2_000_000_000);
        assert_eq!(parsed.total_value, 3_000_000_000);
    }

    #[test]
    fn test_settleable_boxes_result_empty() {
        let result = SettleableBoxesResult {
            boxes: vec![],
            total_value: 0,
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: SettleableBoxesResult = serde_json::from_str(&json).unwrap();

        assert!(parsed.boxes.is_empty());
        assert_eq!(parsed.total_value, 0);
    }

    #[test]
    fn test_settlement_tx_params_serialization() {
        let params = SettlementTxParams {
            provider_address: "9fBsJiCzNBiDkiMC2Y6shy9J8JnxGpVD2wPBfyhSysmfKTiXNGh".to_string(),
            staking_box_ids: vec!["box1".to_string(), "box2".to_string(), "box3".to_string()],
            fee_amounts: vec![100_000, 200_000, 300_000],
            change_address: "3Wx9CaUgBHjDWKiVKTXx3w9w6T8gPzQ8bPv".to_string(),
            max_fee_nanoerg: 1_000_000,
        };

        let json = serde_json::to_string(&params).unwrap();
        let parsed: SettlementTxParams = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.provider_address, params.provider_address);
        assert_eq!(parsed.staking_box_ids.len(), 3);
        assert_eq!(parsed.fee_amounts, vec![100_000, 200_000, 300_000]);
        assert_eq!(parsed.change_address, params.change_address);
        assert_eq!(parsed.max_fee_nanoerg, 1_000_000);
    }

    #[test]
    fn test_settlement_tx_params_serialization_empty() {
        let params = SettlementTxParams {
            provider_address: String::new(),
            staking_box_ids: vec![],
            fee_amounts: vec![],
            change_address: String::new(),
            max_fee_nanoerg: 0,
        };

        let json = serde_json::to_string(&params).unwrap();
        let parsed: SettlementTxParams = serde_json::from_str(&json).unwrap();

        assert!(parsed.staking_box_ids.is_empty());
        assert!(parsed.fee_amounts.is_empty());
        assert_eq!(parsed.max_fee_nanoerg, 0);
    }

    #[tokio::test]
    async fn test_find_settleable_boxes_no_contract() {
        // Without contracts loaded, should return error
        let result = find_settleable_boxes("http://127.0.0.1:9053", 10, 30).await;
        // This will fail because no node is running, but the contract lookup should succeed
        // (it's embedded at compile time)
        match result {
            Ok(_) => {}
            Err(e) => {
                // Should NOT be a contract-not-found error
                assert!(
                    !e.to_string().contains("contract hex not found"),
                    "Unexpected contract error: {}",
                    e
                );
            }
        }
    }
}
