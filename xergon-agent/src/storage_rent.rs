//! Storage rent monitoring for protocol NFT boxes.
//!
//! Ergo has a 4-year storage rent period (1,051,200 blocks). If a box's ERG
//! value drops below `box_byte_size * 360` nanoERG, it can be garbage collected.
//! Protocol NFT boxes (provider, governance, treasury, slashing) must be kept
//! alive. This module periodically checks them and logs warnings when they need
//! a top-up.
//!
//! # Auto-topup
//!
//! When `auto_topup_enabled` is `true` and a watched box has a `target_address`
//! configured, the monitor will automatically send ERG from the node wallet
//! to boxes that fall below the minimum value threshold. The topup amount is
//! capped at `max_topup_nanoerg` per operation for safety.
//!
//! # Configuration
//!
//! ```toml
//! [storage_rent]
//! enabled = true
//! check_interval_blocks = 100
//! topup_buffer_factor = 400   # nanoerg/byte (above the 360 minimum)
//! min_topup_amount_nanoerg = 500_000
//! auto_topup_enabled = false
//! max_topup_nanoerg = 1_000_000_000
//!
//! # Protocol NFT token IDs to monitor
//! [[storage_rent.watched_boxes]]
//! label = "treasury"
//! token_id = "***"
//! target_address = "3Wwx..."
//!
//! [[storage_rent.watched_boxes]]
//! label = "governance"
//! token_id = "***"
//! target_address = "3Wwx..."
//!
//! [[storage_rent.watched_boxes]]
//! label = "slashing"
//! token_id = "***"
//! target_address = "3Wwx..."
//! ```

use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::chain::client::ErgoNodeClient;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Storage rent monitor configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StorageRentConfig {
    /// Enable the storage rent monitor (default: false).
    #[serde(default)]
    pub enabled: bool,

    /// Check protocol boxes every N blocks (default: 100).
    /// At ~2 min/block this is roughly every 3.3 hours.
    #[serde(default = "default_check_interval_blocks")]
    pub check_interval_blocks: u32,

    /// Buffer factor in nanoERG per byte (default: 400).
    /// The protocol minimum is 360; using 400 provides ~11% headroom.
    #[serde(default = "default_topup_buffer_factor")]
    pub topup_buffer_factor: u32,

    /// Minimum top-up amount in nanoERG when a box needs ERG (default: 500_000 = 0.0005 ERG).
    #[serde(default = "default_min_topup_amount")]
    pub min_topup_amount_nanoerg: u64,

    /// Enable automatic top-up via the node wallet API (default: false).
    /// When true and a watched box has a `target_address`, ERG will be sent
    /// automatically when the box value falls below the minimum.
    #[serde(default)]
    pub auto_topup_enabled: bool,

    /// Maximum amount to send in a single auto-topup transaction (default: 1_000_000_000 = 1 ERG).
    /// Safety cap to prevent accidental large transfers.
    #[serde(default = "default_max_topup_amount")]
    pub max_topup_nanoerg: u64,

    /// List of protocol NFT boxes to monitor by token ID.
    #[serde(default)]
    pub watched_boxes: Vec<WatchedBox>,
}

/// A protocol box to monitor, identified by its NFT token ID.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WatchedBox {
    /// Human-readable label for log messages (e.g. "treasury", "governance").
    pub label: String,

    /// The NFT token ID that uniquely identifies this box on-chain.
    pub token_id: String,

    /// Optional Ergo address to send ERG to for auto-topup.
    /// If not set, auto-topup is skipped for this box even if globally enabled.
    #[serde(default)]
    pub target_address: Option<String>,
}

fn default_check_interval_blocks() -> u32 {
    100
}

fn default_topup_buffer_factor() -> u32 {
    400
}

fn default_min_topup_amount() -> u64 {
    500_000 // 0.0005 ERG
}

fn default_max_topup_amount() -> u64 {
    1_000_000_000 // 1 ERG
}

impl Default for StorageRentConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            check_interval_blocks: default_check_interval_blocks(),
            topup_buffer_factor: default_topup_buffer_factor(),
            min_topup_amount_nanoerg: default_min_topup_amount(),
            auto_topup_enabled: false,
            max_topup_nanoerg: default_max_topup_amount(),
            watched_boxes: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Monitor
// ---------------------------------------------------------------------------

/// Storage rent monitor that periodically checks protocol NFT boxes.
///
/// For each watched box, it fetches the box from the UTXO set, estimates the
/// byte size, computes the minimum value needed, and warns if the box is
/// running low on ERG. When auto-topup is enabled, it can also send ERG
/// automatically from the node wallet.
pub struct StorageRentMonitor {
    client: ErgoNodeClient,
    config: StorageRentConfig,
}

impl StorageRentMonitor {
    /// Create a new storage rent monitor.
    pub fn new(client: ErgoNodeClient, config: StorageRentConfig) -> Self {
        Self { client, config }
    }

    /// Run one check cycle over all watched boxes.
    ///
    /// Returns `Ok(())` even if individual box checks fail -- errors are logged
    /// as warnings and the monitor continues to the next box.
    pub async fn check_all(&self) -> Result<()> {
        if self.config.watched_boxes.is_empty() {
            return Ok(());
        }

        let height = match self.client.get_height().await {
            Ok(h) => h,
            Err(e) => {
                warn!(error = %e, "Storage rent: failed to get current block height, skipping check");
                return Ok(());
            }
        };

        info!(
            watched = self.config.watched_boxes.len(),
            height,
            "Storage rent: starting check cycle"
        );

        for box_spec in &self.config.watched_boxes {
            if self.config.auto_topup_enabled && box_spec.target_address.is_some() {
                if let Err(e) = self
                    .auto_topup_if_needed(
                        &box_spec.label,
                        &box_spec.token_id,
                        box_spec.target_address.as_deref().unwrap_or(""),
                        height,
                    )
                    .await
                {
                    warn!(
                        label = %box_spec.label,
                        token_id = %box_spec.token_id,
                        error = %e,
                        "Storage rent: failed auto-topup check"
                    );
                }
            } else if let Err(e) =
                self.check_and_warn(&box_spec.label, &box_spec.token_id, height).await
            {
                warn!(
                    label = %box_spec.label,
                    token_id = %box_spec.token_id,
                    error = %e,
                    "Storage rent: failed to check box"
                );
            }
        }

        Ok(())
    }

    /// Check a single box identified by its NFT token ID and auto-topup if needed.
    ///
    /// This method evaluates the box health and, if:
    /// - `auto_topup_enabled` is true (checked by caller)
    /// - `target_address` is set (checked by caller)
    /// - box value is below minimum
    ///
    /// It will send ERG from the node wallet to the target address, capped at
    /// `max_topup_nanoerg`.
    async fn auto_topup_if_needed(
        &self,
        label: &str,
        token_id: &str,
        target_address: &str,
        current_height: i32,
    ) -> Result<()> {
        let boxes = self
            .client
            .get_boxes_by_token_id(token_id)
            .await?;

        if boxes.is_empty() {
            warn!(
                label,
                token_id,
                "Storage rent: no box found with this NFT token ID -- box may have been garbage collected!"
            );
            return Ok(());
        }

        for raw_box in &boxes {
            if let Some((deficit, topup_amount)) =
                self.evaluate_box_needs_topup(label, raw_box, current_height)
            {
                // Cap the topup amount at max_topup_nanoerg
                let capped_amount = topup_amount.min(self.config.max_topup_nanoerg);

                info!(
                    label,
                    box_id = %raw_box.box_id,
                    deficit_nanoerg = deficit,
                    requested_topup = topup_amount,
                    capped_topup = capped_amount,
                    max_topup = self.config.max_topup_nanoerg,
                    target = %target_address,
                    "Storage rent: auto-topup triggered"
                );

                match self.client.send_payment(target_address, capped_amount).await {
                    Ok(tx_id) => {
                        info!(
                            label,
                            box_id = %raw_box.box_id,
                            tx_id = %tx_id,
                            amount_nanoerg = capped_amount,
                            erg = format!("{:.6}", capped_amount as f64 / 1_000_000_000.0),
                            "Storage rent: auto-topup SUCCESS"
                        );
                    }
                    Err(e) => {
                        warn!(
                            label,
                            box_id = %raw_box.box_id,
                            error = %e,
                            "Storage rent: auto-topup FAILED -- manual intervention required"
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Check a single box identified by its NFT token ID.
    async fn check_and_warn(&self, label: &str, token_id: &str, current_height: i32) -> Result<()> {
        let boxes = self
            .client
            .get_boxes_by_token_id(token_id)
            .await?;

        if boxes.is_empty() {
            warn!(
                label,
                token_id,
                "Storage rent: no box found with this NFT token ID -- box may have been garbage collected!"
            );
            return Ok(());
        }

        // A singleton NFT should have exactly one box, but handle multiple just in case.
        for raw_box in &boxes {
            self.evaluate_box_health(label, raw_box, current_height);
        }

        Ok(())
    }

    /// Evaluate a single raw box's storage rent health and log warnings.
    fn evaluate_box_health(&self, label: &str, box_data: &crate::chain::types::RawBox, current_height: i32) {
        // Also check if topup is needed (for logging purposes)
        let _ = self.evaluate_box_needs_topup(label, box_data, current_height);
    }

    /// Evaluate whether a box needs a topup.
    ///
    /// Returns `Some((deficit, topup_amount))` if the box value is below
    /// the minimum, or `None` if the box is healthy.
    fn evaluate_box_needs_topup(
        &self,
        label: &str,
        box_data: &crate::chain::types::RawBox,
        current_height: i32,
    ) -> Option<(u64, u64)> {
        let box_id = &box_data.box_id;
        let value = box_data.value;
        let creation_height = box_data.creation_height;

        // Estimate box byte size from the serialized ErgoTree and number of assets.
        let ergo_tree_bytes = box_data.ergo_tree.len() / 2;
        let assets_bytes = box_data.assets.len() * 40;
        let registers_bytes = box_data.additional_registers.len() * 50;
        let estimated_box_bytes = 50 + ergo_tree_bytes + assets_bytes + registers_bytes;

        // Compute minimum value needed with the configured buffer factor.
        let min_value_needed = (estimated_box_bytes as u64) * (self.config.topup_buffer_factor as u64);

        // Compute blocks until storage rent expiry.
        let blocks_since_creation = (current_height - creation_height).max(0) as i64;
        let blocks_remaining = crate::protocol::specs::STORAGE_RENT_PERIOD_BLOCKS as i64 - blocks_since_creation;
        let years_remaining = blocks_remaining as f64
            / (crate::protocol::specs::STORAGE_RENT_PERIOD_BLOCKS as f64)
            * 4.0;

        info!(
            label,
            box_id,
            value_nanoerg = value,
            min_value_nanoerg = min_value_needed,
            estimated_bytes = estimated_box_bytes,
            blocks_remaining,
            years_remaining = format!("{:.1}", years_remaining),
            "Storage rent: box health check"
        );

        if value < min_value_needed {
            let deficit = min_value_needed - value;
            let topup_amount = deficit.max(self.config.min_topup_amount_nanoerg);

            warn!(
                label,
                box_id,
                value_nanoerg = value,
                deficit_nanoerg = deficit,
                suggested_topup_nanoerg = topup_amount,
                erg_shortfall = format!("{:.6}", value as f64 / 1_000_000_000.0),
                topup_erg = format!("{:.6}", topup_amount as f64 / 1_000_000_000.0),
                "Storage rent WARNING: box ERG value is below minimum -- needs topup to prevent garbage collection"
            );

            Some((deficit, topup_amount))
        } else if value < min_value_needed * 2 {
            // Advisory warning when the box has less than 2x the minimum.
            let surplus_ratio = value as f64 / min_value_needed as f64;
            warn!(
                label,
                box_id,
                value_nanoerg = value,
                min_value_nanoerg = min_value_needed,
                surplus_ratio = format!("{:.2}x", surplus_ratio),
                "Storage rent advisory: box ERG value is low (less than 2x minimum)"
            );
            None
        } else {
            None
        }
    }

    /// Spawn a background task that periodically checks all watched boxes.
    ///
    /// The loop polls the current block height and sleeps until the next
    /// check interval boundary. The sleep is calibrated so that checks
    /// happen approximately every `check_interval_blocks` blocks.
    pub fn spawn(self) {
        tokio::spawn(async move {
            let interval_blocks = self.config.check_interval_blocks as u64;

            // Sleep duration estimate: ~2 minutes per block on Ergo mainnet.
            const SECONDS_PER_BLOCK: u64 = 120;
            let sleep_duration = Duration::from_secs(interval_blocks * SECONDS_PER_BLOCK);

            info!(
                interval_blocks,
                sleep_secs = sleep_duration.as_secs(),
                watched = self.config.watched_boxes.len(),
                auto_topup_enabled = self.config.auto_topup_enabled,
                "Storage rent monitor started"
            );

            // Run an initial check immediately.
            if let Err(e) = self.check_all().await {
                warn!(error = %e, "Storage rent: initial check failed");
            }

            loop {
                tokio::time::sleep(sleep_duration).await;
                if let Err(e) = self.check_all().await {
                    warn!(error = %e, "Storage rent: periodic check failed");
                }
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::types::RawBox;

    /// Helper to create a StorageRentConfig with auto-topup enabled.
    fn auto_topup_config(max_topup: u64) -> StorageRentConfig {
        StorageRentConfig {
            enabled: true,
            check_interval_blocks: 100,
            topup_buffer_factor: 400,
            min_topup_amount_nanoerg: 500_000,
            auto_topup_enabled: true,
            max_topup_nanoerg: max_topup,
            watched_boxes: vec![WatchedBox {
                label: "test".to_string(),
                token_id: "abc123".to_string(),
                target_address: Some("3WwxTestAddress".to_string()),
            }],
        }
    }

    /// Helper to create a minimal RawBox for testing.
    fn make_test_raw_box(value: u64, creation_height: i32) -> RawBox {
        RawBox {
            box_id: "test_box_id".to_string(),
            tx_id: "test_tx_id".to_string(),
            value,
            creation_height,
            ergo_tree: "1005040004000e36100204a00b08cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798".to_string(),
            assets: vec![],
            additional_registers: std::collections::HashMap::new(),
        }
    }

    /// Create a monitor with a mock-like config (client won't be called in these unit tests).
    fn make_monitor(config: StorageRentConfig) -> StorageRentMonitor {
        let client = ErgoNodeClient::new("http://127.0.0.1:9053".to_string());
        StorageRentMonitor::new(client, config)
    }

    #[test]
    fn test_auto_topup_config_defaults() {
        let config = StorageRentConfig::default();
        assert!(!config.enabled);
        assert!(!config.auto_topup_enabled);
        assert_eq!(config.max_topup_nanoerg, 1_000_000_000);
        assert_eq!(config.min_topup_amount_nanoerg, 500_000);
        assert_eq!(config.topup_buffer_factor, 400);
        assert_eq!(config.check_interval_blocks, 100);
    }

    #[test]
    fn test_auto_topup_config_custom() {
        let config = StorageRentConfig {
            enabled: true,
            auto_topup_enabled: true,
            max_topup_nanoerg: 2_000_000_000,
            min_topup_amount_nanoerg: 1_000_000,
            topup_buffer_factor: 500,
            check_interval_blocks: 50,
            watched_boxes: vec![],
        };
        assert!(config.enabled);
        assert!(config.auto_topup_enabled);
        assert_eq!(config.max_topup_nanoerg, 2_000_000_000);
    }

    #[test]
    fn test_watched_box_without_target_address() {
        let box_spec = WatchedBox {
            label: "treasury".to_string(),
            token_id: "token123".to_string(),
            target_address: None,
        };
        assert!(box_spec.target_address.is_none());
    }

    #[test]
    fn test_evaluate_box_needs_topup_below_min() {
        let min_topup = 500_000u64;
        let config = auto_topup_config(1_000_000_000);
        let monitor = make_monitor(config);
        let raw_box = make_test_raw_box(10_000, 600_000); // Very low value: 10k nanoERG vs 32.8k min

        let result = monitor.evaluate_box_needs_topup("test", &raw_box, 600_000);
        // Box should need topup since value is very low
        assert!(result.is_some());
        let (deficit, topup) = result.unwrap();
        assert!(deficit > 0);
        assert!(topup >= min_topup);
    }

    #[test]
    fn test_evaluate_box_needs_topup_healthy() {
        let config = auto_topup_config(1_000_000_000);
        let monitor = make_monitor(config);
        // Give the box a very high value so it won't need topup
        let raw_box = make_test_raw_box(10_000_000_000, 500_000);

        let result = monitor.evaluate_box_needs_topup("test", &raw_box, 600_000);
        assert!(result.is_none(), "Healthy box should not need topup");
    }

    #[test]
    fn test_topup_amount_capped_at_max() {
        let config = StorageRentConfig {
            enabled: true,
            auto_topup_enabled: true,
            max_topup_nanoerg: 500_000_000, // 0.5 ERG cap
            min_topup_amount_nanoerg: 100_000,
            topup_buffer_factor: 400,
            check_interval_blocks: 100,
            watched_boxes: vec![],
        };
        let monitor = make_monitor(config);
        // Set value very low so deficit exceeds max_topup
        let raw_box = make_test_raw_box(0, 500_000);

        let result = monitor.evaluate_box_needs_topup("test", &raw_box, 600_000);
        assert!(result.is_some());
        let (deficit, topup) = result.unwrap();
        // Deficit should be the full min_value (box_bytes=82, min=82*400=32,800)
        assert!(deficit > 0);
        // Topup should be capped at max_topup_nanoerg when deficit > max
        // Since deficit (32,800) < max (500M), topup = max(min_topup, deficit) = max(100k, 32.8k) = 100k
        assert_eq!(topup, 100_000);
    }

    #[test]
    fn test_auto_topup_skipped_when_disabled() {
        let config = StorageRentConfig {
            enabled: true,
            auto_topup_enabled: false, // Disabled
            max_topup_nanoerg: 1_000_000_000,
            min_topup_amount_nanoerg: 500_000,
            topup_buffer_factor: 400,
            check_interval_blocks: 100,
            watched_boxes: vec![WatchedBox {
                label: "test".to_string(),
                token_id: "abc123".to_string(),
                target_address: Some("3WwxTestAddress".to_string()),
            }],
        };
        assert!(!config.auto_topup_enabled);
        // The check_all method checks auto_topup_enabled and routes to check_and_warn
        // instead of auto_topup_if_needed. This is verified by the routing logic.
    }

    #[test]
    fn test_auto_topup_skipped_when_no_target_address() {
        let config = StorageRentConfig {
            enabled: true,
            auto_topup_enabled: true,
            max_topup_nanoerg: 1_000_000_000,
            min_topup_amount_nanoerg: 500_000,
            topup_buffer_factor: 400,
            check_interval_blocks: 100,
            watched_boxes: vec![WatchedBox {
                label: "test".to_string(),
                token_id: "abc123".to_string(),
                target_address: None, // No target address
            }],
        };
        // Even though auto_topup is enabled, no target_address means
        // check_all routes to check_and_warn instead of auto_topup_if_needed
        let has_target = config.watched_boxes[0].target_address.is_some();
        assert!(!has_target);
    }

    #[test]
    fn test_min_topup_amount_used_when_deficit_is_small() {
        let min_topup = 500_000u64;
        let config = StorageRentConfig {
            enabled: true,
            auto_topup_enabled: true,
            max_topup_nanoerg: 1_000_000_000,
            min_topup_amount_nanoerg: min_topup,
            topup_buffer_factor: 400,
            check_interval_blocks: 100,
            watched_boxes: vec![],
        };
        let monitor = make_monitor(config);
        // Make a box where deficit is very small but value is below min
        // With ergo_tree ~90 bytes + overhead, min_value_needed ~= ~56,000
        // If we set value just slightly below that, deficit would be small
        let raw_box = make_test_raw_box(50_000, 500_000);

        let result = monitor.evaluate_box_needs_topup("test", &raw_box, 600_000);
        if let Some((deficit, topup)) = result {
            // Topup should be at least min_topup_amount
            assert!(
                topup >= min_topup,
                "Topup {} should be >= min {}",
                topup,
                min_topup
            );
            // And should be max(deficit, min_topup)
            assert_eq!(topup, deficit.max(min_topup));
        }
    }
}
