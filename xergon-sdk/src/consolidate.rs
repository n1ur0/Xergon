use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// ConsolidateConfig
// ---------------------------------------------------------------------------

/// Configuration for a UTXO consolidation run.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConsolidateConfig {
    /// Target Ergo address to scan for dust boxes.
    pub address: String,
    /// Dust threshold in nanoERG. Boxes with value below this are considered dust.
    pub threshold: u64,
    /// Maximum number of input boxes per consolidation transaction.
    pub max_inputs: u32,
    /// Fee in nanoERG for each consolidation transaction.
    pub fee: u64,
    /// If true, only simulate and report; do not broadcast transactions.
    pub dry_run: bool,
}

impl Default for ConsolidateConfig {
    fn default() -> Self {
        Self {
            address: String::new(),
            threshold: 1_000_000, // 0.001 ERG
            max_inputs: 50,
            fee: 1_000_000, // 0.001 ERG
            dry_run: true,
        }
    }
}

// ---------------------------------------------------------------------------
// DustBox
// ---------------------------------------------------------------------------

/// A UTXO box identified as dust.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DustBox {
    /// Box ID (base16).
    pub box_id: String,
    /// Hex-encoded ErgoTree (contract) of the box.
    pub ergo_tree: String,
    /// Box value in nanoERG.
    pub value: u64,
    /// Number of tokens in the box.
    pub token_count: u32,
    /// Creation height of the box.
    pub creation_height: u32,
}

// ---------------------------------------------------------------------------
// ConsolidationGroup
// ---------------------------------------------------------------------------

/// A group of dust boxes sharing the same ErgoTree, suitable for a single
/// consolidation transaction.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConsolidationGroup {
    /// Hex-encoded ErgoTree shared by all boxes in the group.
    pub ergo_tree: String,
    /// Total value across all boxes in the group (nanoERG).
    pub total_value: u64,
    /// Number of boxes in the group.
    pub box_count: u32,
    /// The individual dust boxes.
    pub boxes: Vec<DustBox>,
}

// ---------------------------------------------------------------------------
// ConsolidationTx
// ---------------------------------------------------------------------------

/// A single consolidation transaction (simulated).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConsolidationTx {
    /// Simulated transaction ID (base16).
    pub tx_id: String,
    /// Number of input boxes consumed.
    pub inputs_consumed: u32,
    /// Total value of inputs (nanoERG).
    pub input_value: u64,
    /// Number of output boxes produced.
    pub outputs_created: u32,
    /// Total value of outputs (nanoERG), net of fee.
    pub output_value: u64,
    /// Fee paid (nanoERG).
    pub fee: u64,
    /// Whether this was a dry-run.
    pub dry_run: bool,
}

// ---------------------------------------------------------------------------
// ConsolidationSummary
// ---------------------------------------------------------------------------

/// Summary of a consolidation run.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConsolidationSummary {
    /// Configuration used for this run.
    pub config: ConsolidateConfig,
    /// Total number of dust boxes found across all groups.
    pub boxes_found: u32,
    /// Total number of boxes consolidated into new outputs.
    pub boxes_consolidated: u32,
    /// Total number of consolidation transactions.
    pub tx_count: u32,
    /// Total fees that would be / were paid (nanoERG).
    pub total_fees: u64,
    /// Estimated ERG saved in future fees by reducing box count (nanoERG).
    pub erg_saved: u64,
    /// Consolidation groups by ErgoTree.
    pub groups: Vec<ConsolidationGroup>,
    /// Individual consolidation transactions.
    pub transactions: Vec<ConsolidationTx>,
    /// Timestamp of the run.
    pub timestamp: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Consolidator
// ---------------------------------------------------------------------------

/// Scans an address for dust UTXO boxes and builds consolidation transactions.
///
/// In production this would query a node / explorer API; the current
/// implementation uses mock data so the crate compiles without a network
/// dependency.
pub struct Consolidator {
    config: ConsolidateConfig,
}

impl Consolidator {
    /// Create a new consolidator with the given configuration.
    pub fn new(config: ConsolidateConfig) -> Self {
        Self { config }
    }

    /// Scan for dust boxes (mock).
    ///
    /// Returns a list of [`DustBox`] entries grouped by ErgoTree.
    pub fn scan(&self) -> Vec<ConsolidationGroup> {
        // Mock: produce deterministic groups for demonstration.
        let mut groups: HashMap<String, Vec<DustBox>> = HashMap::new();

        // Simulate finding some dust boxes.
        let mock_values: &[u64] = &[
            500_000,
            300_000,
            200_000,
            800_000,
            100_000,
            600_000,
            400_000,
            150_000,
        ];

        for (i, &value) in mock_values.iter().enumerate() {
            if value >= self.config.threshold {
                continue;
            }

            // Alternate between two mock ErgoTrees.
            let ergo_tree = if i % 2 == 0 {
                "100204a00b08cd0391b1b4e9b4a017c94e0f9be8b3d5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0"
            } else {
                "100204a00b08cd021f2e3d4c5b6a7980fedcba9876543210fedcba9876543210fedcba98"
            };

            let entry = groups.entry(ergo_tree.to_string()).or_default();
            entry.push(DustBox {
                box_id: format!("{:064x}", i + 1),
                ergo_tree: ergo_tree.to_string(),
                value,
                token_count: 0,
                creation_height: 500_000 + i as u32,
            });
        }

        groups
            .into_iter()
            .map(|(ergo_tree, boxes)| {
                let total_value: u64 = boxes.iter().map(|b| b.value).sum();
                let box_count = boxes.len() as u32;
                ConsolidationGroup {
                    ergo_tree,
                    total_value,
                    box_count,
                    boxes,
                }
            })
            .collect()
    }

    /// Build consolidation transactions from the scanned groups.
    ///
    /// Each group is chunked into batches of at most `max_inputs` boxes per
    /// transaction.
    pub fn build_transactions(&self, groups: &[ConsolidationGroup]) -> Vec<ConsolidationTx> {
        let mut txs = Vec::new();
        let mut tx_counter: u64 = 0;

        for group in groups {
            let max = self.config.max_inputs as usize;
            for chunk in group.boxes.chunks(max) {
                let input_value: u64 = chunk.iter().map(|b| b.value).sum();
                let net_value = input_value.saturating_sub(self.config.fee);

                // Produce one output box (consolidated).
                let outputs_created = if net_value > 0 { 1 } else { 0 };

                tx_counter += 1;
                txs.push(ConsolidationTx {
                    tx_id: format!("{:064x}", tx_counter),
                    inputs_consumed: chunk.len() as u32,
                    input_value,
                    outputs_created,
                    output_value: net_value,
                    fee: self.config.fee,
                    dry_run: self.config.dry_run,
                });
            }
        }

        txs
    }

    /// Run the full consolidation pipeline and return a summary.
    pub fn run(&self) -> ConsolidationSummary {
        let groups = self.scan();
        let transactions = self.build_transactions(&groups);

        let boxes_found: u32 = groups.iter().map(|g| g.box_count).sum();
        let boxes_consolidated: u32 = transactions.iter().map(|t| t.inputs_consumed).sum();
        let total_fees: u64 = transactions.iter().map(|t| t.fee).sum();

        // Estimate savings: each box eliminated saves ~one future input fee.
        let boxes_eliminated = boxes_found.saturating_sub(
            transactions.iter().map(|t| t.outputs_created).sum(),
        );
        let erg_saved = boxes_eliminated as u64 * (self.config.fee / 10); // rough estimate

        ConsolidationSummary {
            config: self.config.clone(),
            boxes_found,
            boxes_consolidated,
            tx_count: transactions.len() as u32,
            total_fees,
            erg_saved,
            groups,
            transactions,
            timestamp: Utc::now(),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_values() {
        let cfg = ConsolidateConfig::default();
        assert_eq!(cfg.threshold, 1_000_000);
        assert_eq!(cfg.max_inputs, 50);
        assert_eq!(cfg.fee, 1_000_000);
        assert!(cfg.dry_run);
        assert!(cfg.address.is_empty());
    }

    #[test]
    fn consolidate_scan_finds_dust_boxes() {
        let cfg = ConsolidateConfig {
            address: "9hEQhmYXqBHfRho6GJHV2PBXwTWSE3T4mNFMGDAfeNNCuTzfd3s".into(),
            ..Default::default()
        };
        let consolidator = Consolidator::new(cfg);
        let groups = consolidator.scan();

        let total_boxes: u32 = groups.iter().map(|g| g.box_count).sum();
        assert!(total_boxes > 0, "expected at least one dust box");
    }

    #[test]
    fn consolidate_groups_by_ergo_tree() {
        let cfg = ConsolidateConfig::default();
        let consolidator = Consolidator::new(cfg);
        let groups = consolidator.scan();

        // Mock data produces at most 2 groups.
        assert!(groups.len() <= 2);

        // Each group should have a unique ErgoTree.
        let ergo_trees: Vec<&str> = groups.iter().map(|g| g.ergo_tree.as_str()).collect();
        let unique: std::collections::HashSet<&str> = ergo_trees.into_iter().collect();
        assert_eq!(unique.len(), groups.len());
    }

    #[test]
    fn consolidate_build_transactions_respects_max_inputs() {
        let cfg = ConsolidateConfig {
            max_inputs: 3,
            ..Default::default()
        };
        let consolidator = Consolidator::new(cfg);
        let groups = consolidator.scan();
        let txs = consolidator.build_transactions(&groups);

        for tx in &txs {
            assert!(
                tx.inputs_consumed <= 3,
                "tx {} has {} inputs (max 3)",
                tx.tx_id,
                tx.inputs_consumed,
            );
        }
    }

    #[test]
    fn consolidate_run_returns_summary() {
        let cfg = ConsolidateConfig {
            address: "9hEQhmYXqBHfRho6GJHV2PBXwTWSE3T4mNFMGDAfeNNCuTzfd3s".into(),
            ..Default::default()
        };
        let consolidator = Consolidator::new(cfg);
        let summary = consolidator.run();

        assert!(summary.boxes_found > 0);
        assert!(summary.boxes_consolidated > 0);
        assert!(summary.tx_count > 0);
        assert!(summary.total_fees > 0);
    }

    #[test]
    fn consolidate_dry_run_flag() {
        let cfg = ConsolidateConfig {
            dry_run: true,
            ..Default::default()
        };
        let consolidator = Consolidator::new(cfg);
        let summary = consolidator.run();

        for tx in &summary.transactions {
            assert!(tx.dry_run);
        }
    }

    #[test]
    fn consolidate_config_serialize_deserialize() {
        let cfg = ConsolidateConfig::default();
        let json = serde_json::to_string(&cfg).expect("serialize");
        let restored: ConsolidateConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.threshold, cfg.threshold);
        assert_eq!(restored.max_inputs, cfg.max_inputs);
        assert_eq!(restored.fee, cfg.fee);
        assert_eq!(restored.dry_run, cfg.dry_run);
    }
}
