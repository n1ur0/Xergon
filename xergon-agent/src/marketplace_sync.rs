//! Marketplace Sync System
//!
//! Periodically pushes provider information, benchmarks, GPU details, and
//! model availability to the relay for display in the marketplace web UI.
//!
//! The sync payload includes:
//! - Provider ID and endpoint
//! - Served models with pricing
//! - GPU hardware info (type, VRAM, count)
//! - Latest benchmark results (TTFT, TPS)
//! - Current capacity and load
//!
//! Config section: `[marketplace_sync]`
//!   enabled = true                       # Enable periodic sync (default: true)
//!   sync_interval_secs = 300             # Sync every 5 minutes (default: 300)
//!   include_benchmarks = true            # Include benchmark results (default: true)
//!   include_models = true                # Include served models (default: true)
//!   include_gpu_info = true              # Include GPU hardware info (default: true)
//!
//! The relay_url is inherited from `[relay].relay_url`.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};

use crate::api::AppState;
use crate::benchmark::BenchmarkResult;
use crate::hardware;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Marketplace sync configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MarketplaceSyncConfig {
    /// Enable marketplace sync (default: true)
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// How often to sync provider info to relay (seconds, default: 300 = 5 min)
    #[serde(default = "default_sync_interval")]
    pub sync_interval_secs: u64,
    /// Include latest benchmark results in sync payload (default: true)
    #[serde(default = "default_true")]
    pub include_benchmarks: bool,
    /// Include served models list in sync payload (default: true)
    #[serde(default = "default_true")]
    pub include_models: bool,
    /// Include GPU hardware info in sync payload (default: true)
    #[serde(default = "default_true")]
    pub include_gpu_info: bool,
}

fn default_enabled() -> bool {
    true
}
fn default_sync_interval() -> u64 {
    300
}
fn default_true() -> bool {
    true
}

impl Default for MarketplaceSyncConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            sync_interval_secs: default_sync_interval(),
            include_benchmarks: default_true(),
            include_models: default_true(),
            include_gpu_info: default_true(),
        }
    }
}

// ---------------------------------------------------------------------------
// Sync payload types
// ---------------------------------------------------------------------------

/// Model entry in the marketplace sync payload.
#[derive(Debug, Clone, Serialize)]
pub struct MarketplaceModel {
    pub name: String,
    pub pricing: ModelPricing,
    pub available: bool,
}

/// Per-model pricing info.
#[derive(Debug, Clone, Serialize)]
pub struct ModelPricing {
    /// Price in nanoERG per 1M tokens
    pub price_per_1m_tokens: u64,
}

/// GPU info in the marketplace sync payload.
#[derive(Debug, Clone, Serialize)]
pub struct MarketplaceGpuInfo {
    /// GPU model name (e.g., "RTX 4090")
    #[serde(rename = "type")]
    pub gpu_type: String,
    /// Total VRAM in GB
    pub vram_gb: u64,
    /// Number of GPUs
    pub count: u32,
}

/// Benchmark entry in the marketplace sync payload.
#[derive(Debug, Clone, Serialize)]
pub struct MarketplaceBenchmark {
    /// Model name
    pub model: String,
    /// Time-to-first-token in milliseconds
    pub ttft_ms: Option<f64>,
    /// Tokens per second
    pub tps: Option<f64>,
    /// Peak memory usage in MB
    pub peak_memory_mb: Option<f64>,
    /// ISO 8601 timestamp
    pub timestamp: String,
}

/// Capacity info in the marketplace sync payload.
#[derive(Debug, Clone, Serialize)]
pub struct MarketplaceCapacity {
    /// Current load as fraction (0.0 = idle, 1.0 = full)
    pub current_load: f64,
    /// Maximum concurrent requests supported
    pub max_concurrent: u32,
}

/// Full sync payload sent to relay.
#[derive(Debug, Serialize)]
pub struct MarketplaceSyncPayload {
    pub provider_id: String,
    pub endpoint: String,
    pub provider_name: String,
    pub region: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<MarketplaceModel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_info: Option<MarketplaceGpuInfo>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub benchmarks: Vec<MarketplaceBenchmark>,
    pub status: String,
    pub capacity: MarketplaceCapacity,
    /// Unix timestamp of when this payload was generated
    pub synced_at: i64,
}

// ---------------------------------------------------------------------------
// Last sync status
// ---------------------------------------------------------------------------

/// Status of the last marketplace sync attempt.
#[derive(Debug, Clone, Serialize)]
pub struct SyncStatus {
    pub last_sync_at: Option<String>,
    pub last_sync_result: Option<String>, // "success" or "error"
    pub last_error: Option<String>,
    pub next_sync_at: Option<String>,
    pub sync_interval_secs: u64,
    pub consecutive_failures: u32,
    pub total_syncs: u64,
    pub total_failures: u64,
}

impl Default for SyncStatus {
    fn default() -> Self {
        Self {
            last_sync_at: None,
            last_sync_result: None,
            last_error: None,
            next_sync_at: None,
            sync_interval_secs: 300,
            consecutive_failures: 0,
            total_syncs: 0,
            total_failures: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// MarketplaceSync
// ---------------------------------------------------------------------------

/// Marketplace sync engine — periodically pushes provider data to the relay.
pub struct MarketplaceSync {
    config: Arc<RwLock<MarketplaceSyncConfig>>,
    http_client: reqwest::Client,
    agent_state: Arc<AppState>,
    status: Arc<RwLock<SyncStatus>>,
    /// When the next sync is scheduled
    next_sync: Arc<Mutex<Instant>>,
    relay_url: String,
}

impl MarketplaceSync {
    /// Create a new marketplace sync instance.
    pub fn new(
        config: MarketplaceSyncConfig,
        relay_url: String,
        agent_state: Arc<AppState>,
    ) -> Self {
        let interval = Duration::from_secs(config.sync_interval_secs);
        Self {
            config: Arc::new(RwLock::new(config.clone())),
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to create marketplace sync HTTP client"),
            agent_state,
            status: Arc::new(RwLock::new(SyncStatus {
                sync_interval_secs: config.sync_interval_secs,
                ..Default::default()
            })),
            next_sync: Arc::new(Mutex::new(Instant::now() + interval)),
            relay_url,
        }
    }

    /// Build the sync payload from current agent state.
    pub async fn build_payload(&self) -> MarketplaceSyncPayload {
        let state = &self.agent_state;
        let cfg = self.config.read().await;

        // Provider identity
        let provider_id = state.xergon_config.provider_id.clone();
        let provider_name = state.xergon_config.provider_name.clone();
        let region = state.xergon_config.region.clone();

        // Endpoint — use env var override or listen address
        let endpoint = std::env::var("XERGON__RELAY__AGENT_ENDPOINT")
            .unwrap_or_else(|_| {
                state
                    .xergon_config
                    .provider_name
                    .clone() // fallback to provider name
            });

        // Models with pricing
        let models = if cfg.include_models {
            self.build_models_payload().await
        } else {
            Vec::new()
        };

        // GPU info
        let gpu_info = if cfg.include_gpu_info {
            self.build_gpu_info()
        } else {
            None
        };

        // Benchmarks
        let benchmarks = if cfg.include_benchmarks {
            self.build_benchmarks_payload().await
        } else {
            Vec::new()
        };

        // Capacity — estimate from loaded models and system resources
        let models_loaded = state.models_loaded.read().await;
        let model_count = models_loaded.len() as u32;
        let hw = hardware::detect_hardware();
        // Estimate max concurrent from CPU cores, cap at 16
        let max_concurrent = (hw.cpu_cores as u32).min(16).max(1);
        // Current load: fraction of models loaded vs some reasonable max
        let current_load = if model_count > 0 {
            // Use a simple heuristic: each model adds ~10% load
            (model_count as f64 * 0.1).min(0.95)
        } else {
            0.0
        };
        drop(models_loaded);

        MarketplaceSyncPayload {
            provider_id,
            endpoint,
            provider_name,
            region,
            models,
            gpu_info,
            benchmarks,
            status: "online".to_string(),
            capacity: MarketplaceCapacity {
                current_load,
                max_concurrent,
            },
            synced_at: chrono::Utc::now().timestamp(),
        }
    }

    /// Build the models section of the sync payload.
    async fn build_models_payload(&self) -> Vec<MarketplaceModel> {
        let state = &self.agent_state;
        let models_loaded = state.models_loaded.read().await;
        let pricing = state.pricing.read().await;

        let mut result = Vec::new();

        for model_name in models_loaded.iter() {
            let price = pricing
                .models
                .get(model_name)
                .copied()
                .unwrap_or(pricing.default_price_per_1m_tokens);

            result.push(MarketplaceModel {
                name: model_name.clone(),
                pricing: ModelPricing {
                    price_per_1m_tokens: price,
                },
                available: true,
            });
        }

        result
    }

    /// Build GPU info from hardware detection.
    fn build_gpu_info(&self) -> Option<MarketplaceGpuInfo> {
        let hw = hardware::detect_hardware();

        if hw.gpus.is_empty() {
            return None;
        }

        // Use the first GPU as the representative type
        let primary_gpu = &hw.gpus[0];
        Some(MarketplaceGpuInfo {
            gpu_type: primary_gpu.name.clone(),
            vram_gb: hw.total_vram_mb / 1024,
            count: hw.gpus.len() as u32,
        })
    }

    /// Build benchmarks payload from the benchmark suite.
    async fn build_benchmarks_payload(&self) -> Vec<MarketplaceBenchmark> {
        let state = &self.agent_state;

        if let Some(suite) = &state.benchmark_suite {
            // Get latest results (last 20)
            let results = suite.get_recent_results(20).await;

            // Deduplicate by model — keep the latest result per model
            let mut latest_per_model: std::collections::HashMap<String, BenchmarkResult> =
                std::collections::HashMap::new();

            for r in results {
                // Only include successful latency or full benchmarks
                if r.error.is_some() {
                    continue;
                }
                if r.benchmark_type != "latency" && r.benchmark_type != "full" {
                    continue;
                }
                latest_per_model.insert(r.model.clone(), r);
            }

            latest_per_model
                .into_values()
                .map(|r| MarketplaceBenchmark {
                    model: r.model,
                    ttft_ms: r.ttft_ms,
                    tps: r.tps,
                    peak_memory_mb: r.peak_memory_mb,
                    timestamp: r.timestamp,
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Perform one sync cycle — POST payload to relay.
    pub async fn sync_now(&self) -> Result<(), String> {
        let cfg = self.config.read().await;
        if !cfg.enabled {
            debug!("Marketplace sync is disabled, skipping");
            return Ok(());
        }

        let payload = self.build_payload().await;

        let url = format!(
            "{}/api/marketplace/sync",
            self.relay_url.trim_end_matches('/')
        );

        debug!(
            provider_id = %payload.provider_id,
            models = payload.models.len(),
            "Sending marketplace sync to relay"
        );

        let result = self
            .http_client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {e}"))?;

        if result.status().is_success() {
            let status_code = result.status().as_u16();
            debug!(status = status_code, "Marketplace sync successful");
            Ok(())
        } else {
            let status = result.status();
            let body = result.text().await.unwrap_or_default();
            Err(format!("Relay returned {status}: {body}"))
        }
    }

    /// Run one sync cycle and update the internal status.
    async fn run_sync_cycle(&self) {
        match self.sync_now().await {
            Ok(()) => {
                let mut status = self.status.write().await;
                status.last_sync_at = Some(chrono::Utc::now().to_rfc3339());
                status.last_sync_result = Some("success".to_string());
                status.last_error = None;
                status.consecutive_failures = 0;
                status.total_syncs += 1;

                let cfg = self.config.read().await;
                let next = chrono::Utc::now()
                    + chrono::Duration::seconds(cfg.sync_interval_secs as i64);
                status.next_sync_at = Some(next.to_rfc3339());
                status.sync_interval_secs = cfg.sync_interval_secs;

                drop(cfg);

                let mut next_sync = self.next_sync.lock().await;
                *next_sync = Instant::now() + Duration::from_secs(status.sync_interval_secs);

                info!(
                    total_syncs = status.total_syncs,
                    "Marketplace sync completed"
                );
            }
            Err(e) => {
                let mut status = self.status.write().await;
                status.last_sync_at = Some(chrono::Utc::now().to_rfc3339());
                status.last_sync_result = Some("error".to_string());
                status.last_error = Some(e.clone());
                status.consecutive_failures += 1;
                status.total_failures += 1;

                let cfg = self.config.read().await;
                let next = chrono::Utc::now()
                    + chrono::Duration::seconds(cfg.sync_interval_secs as i64);
                status.next_sync_at = Some(next.to_rfc3339());
                status.sync_interval_secs = cfg.sync_interval_secs;

                drop(cfg);

                let mut next_sync = self.next_sync.lock().await;
                *next_sync = Instant::now() + Duration::from_secs(status.sync_interval_secs);

                // Warn on first few failures, error after 3 consecutive
                if status.consecutive_failures <= 3 {
                    warn!(error = %e, failures = status.consecutive_failures, "Marketplace sync failed");
                } else {
                    error!(error = %e, failures = status.consecutive_failures, "Marketplace sync failed repeatedly");
                }
            }
        }
    }

    /// Spawn the periodic sync loop as a background task.
    pub fn spawn_loop(self: &Arc<Self>) {
        let sync = Arc::clone(self);

        tokio::spawn(async move {
            let cfg = sync.config.read().await;

            if !cfg.enabled {
                info!("Marketplace sync disabled — not starting sync loop");
                return;
            }

            info!(
                interval_secs = cfg.sync_interval_secs,
                "Starting marketplace sync loop"
            );
            drop(cfg);

            // Small delay on startup to let other services initialize
            tokio::time::sleep(Duration::from_secs(10)).await;

            // Initial sync
            sync.run_sync_cycle().await;

            loop {
                let next_sync = {
                    let guard = sync.next_sync.lock().await;
                    *guard
                };

                let now = Instant::now();
                if next_sync > now {
                    tokio::time::sleep(next_sync - now).await;
                }

                sync.run_sync_cycle().await;
            }
        });
    }

    /// Get the current sync status.
    pub async fn get_last_sync_status(&self) -> SyncStatus {
        self.status.read().await.clone()
    }

    /// Get the current sync configuration.
    pub async fn get_config(&self) -> MarketplaceSyncConfig {
        self.config.read().await.clone()
    }

    /// Update the sync configuration at runtime.
    pub async fn update_config(&self, new_config: MarketplaceSyncConfig) {
        let mut cfg = self.config.write().await;
        *cfg = new_config.clone();

        // Update next sync time if interval changed
        let mut next_sync = self.next_sync.lock().await;
        *next_sync = Instant::now() + Duration::from_secs(cfg.sync_interval_secs);

        // Also update the status with new interval
        let mut status = self.status.write().await;
        status.sync_interval_secs = cfg.sync_interval_secs;
        let next = chrono::Utc::now()
            + chrono::Duration::seconds(cfg.sync_interval_secs as i64);
        status.next_sync_at = Some(next.to_rfc3339());

        info!(
            enabled = cfg.enabled,
            interval_secs = cfg.sync_interval_secs,
            "Marketplace sync config updated"
        );
    }
}
