//! Resource Quotas
//!
//! Per-user/API-key resource quotas with usage tracking, alerts, and rate limiting.
//!
//! Features:
//! - Per-user/API-key resource quotas
//! - Track concurrent requests, tokens/min, VRAM, storage, loaded models, bandwidth
//! - Quota status: Active -> Warning (80%) -> Exceeded (100%) -> Suspended (repeated abuse)
//! - Burst allowance: temporarily exceed quota by multiplier
//! - Alerts: generate alerts when approaching or exceeding quotas
//! - Admin: set custom quotas per subject, view all quotas, reset usage
//! - Rate limiting integration: deny requests when quota exceeded
//!
//! API endpoints:
//! - GET  /api/quotas                  -- list all quota entries
//! - GET  /api/quotas/{subject_id}     -- get specific quota
//! - PUT  /api/quotas/{subject_id}     -- set custom quota
//! - GET  /api/quotas/{subject_id}/usage -- current usage
//! - POST /api/quotas/{subject_id}/reset -- reset usage counters
//! - GET  /api/quotas/alerts           -- recent alerts
//! - GET  /api/quotas/config           -- default quota config
//! - PATCH /api/quotas/config          -- update default quota

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, AtomicU64};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A resource quota definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceQuota {
    /// Maximum concurrent requests.
    pub max_concurrent_requests: u32,
    /// Maximum tokens per minute.
    pub max_tokens_per_minute: u32,
    /// Maximum VRAM in MiB.
    pub max_vram_mb: u64,
    /// Maximum storage in MiB.
    pub max_storage_mb: u64,
    /// Maximum models that can be loaded simultaneously.
    pub max_models_loaded: u32,
    /// Maximum bandwidth in Mbps.
    pub max_bandwidth_mbps: u64,
}

impl Default for ResourceQuota {
    fn default() -> Self {
        Self {
            max_concurrent_requests: 10,
            max_tokens_per_minute: 100_000,
            max_vram_mb: 8192,
            max_storage_mb: 50_000,
            max_models_loaded: 3,
            max_bandwidth_mbps: 1000,
        }
    }
}

/// Configuration for the quota manager.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaConfig {
    /// Default quota for new subjects.
    pub default_quota: ResourceQuota,
    /// Maximum allowable quota (ceiling for custom quotas).
    pub max_quota: ResourceQuota,
    /// Burst allowance multiplier (0.0-2.0).
    pub burst_allowance: f64,
}

impl Default for QuotaConfig {
    fn default() -> Self {
        Self {
            default_quota: ResourceQuota::default(),
            max_quota: ResourceQuota {
                max_concurrent_requests: 100,
                max_tokens_per_minute: 1_000_000,
                max_vram_mb: 32_768,
                max_storage_mb: 500_000,
                max_models_loaded: 20,
                max_bandwidth_mbps: 10_000,
            },
            burst_allowance: 1.5,
        }
    }
}

/// Current resource usage (lock-free atomic counters).
#[derive(Debug, Default)]
pub struct ResourceUsage {
    pub concurrent_requests: AtomicU32,
    pub tokens_this_minute: AtomicU32,
    pub vram_used_mb: AtomicU64,
    pub storage_used_mb: AtomicU64,
    pub models_loaded: AtomicU32,
    pub bandwidth_mbps: AtomicU64,
}

impl ResourceUsage {
    /// Snapshot of current usage for serialization.
    pub fn snapshot(&self) -> ResourceUsageSnapshot {
        ResourceUsageSnapshot {
            concurrent_requests: self.concurrent_requests.load(std::sync::atomic::Ordering::Relaxed),
            tokens_this_minute: self.tokens_this_minute.load(std::sync::atomic::Ordering::Relaxed),
            vram_used_mb: self.vram_used_mb.load(std::sync::atomic::Ordering::Relaxed),
            storage_used_mb: self.storage_used_mb.load(std::sync::atomic::Ordering::Relaxed),
            models_loaded: self.models_loaded.load(std::sync::atomic::Ordering::Relaxed),
            bandwidth_mbps: self.bandwidth_mbps.load(std::sync::atomic::Ordering::Relaxed),
        }
    }

    /// Reset all counters to zero.
    pub fn reset(&self) {
        self.concurrent_requests.store(0, std::sync::atomic::Ordering::Relaxed);
        self.tokens_this_minute.store(0, std::sync::atomic::Ordering::Relaxed);
        self.vram_used_mb.store(0, std::sync::atomic::Ordering::Relaxed);
        self.storage_used_mb.store(0, std::sync::atomic::Ordering::Relaxed);
        self.models_loaded.store(0, std::sync::atomic::Ordering::Relaxed);
        self.bandwidth_mbps.store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Snapshot of resource usage (serializable).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUsageSnapshot {
    pub concurrent_requests: u32,
    pub tokens_this_minute: u32,
    pub vram_used_mb: u64,
    pub storage_used_mb: u64,
    pub models_loaded: u32,
    pub bandwidth_mbps: u64,
}

/// Quota status levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuotaStatus {
    /// Usage is within normal limits.
    Active,
    /// Usage is approaching limits (>= 80%).
    Warning,
    /// Usage has exceeded limits (>= 100%).
    Exceeded,
    /// Subject has been suspended due to repeated abuse.
    Suspended,
}

impl std::fmt::Display for QuotaStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuotaStatus::Active => write!(f, "active"),
            QuotaStatus::Warning => write!(f, "warning"),
            QuotaStatus::Exceeded => write!(f, "exceeded"),
            QuotaStatus::Suspended => write!(f, "suspended"),
        }
    }
}

/// A quota alert event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaAlert {
    pub subject_id: String,
    pub resource: String,
    pub current: u64,
    pub limit: u64,
    pub percentage: f64,
    pub timestamp: DateTime<Utc>,
    pub alert_type: QuotaAlertType,
}

/// Type of quota alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuotaAlertType {
    Warning,
    Exceeded,
    Suspended,
}

/// A quota entry for a specific subject (user or API key).
#[derive(Debug)]
pub struct QuotaEntry {
    pub subject_id: String,
    pub quota: tokio::sync::RwLock<ResourceQuota>,
    pub current_usage: ResourceUsage,
    pub status: tokio::sync::RwLock<QuotaStatus>,
    pub last_updated: tokio::sync::RwLock<DateTime<Utc>>,
    /// Number of times quota was exceeded (for suspension logic).
    pub exceed_count: AtomicU32,
}

/// Snapshot of a quota entry for API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaEntrySnapshot {
    pub subject_id: String,
    pub quota: ResourceQuota,
    pub usage: ResourceUsageSnapshot,
    pub status: String,
    pub last_updated: DateTime<Utc>,
    pub exceed_count: u32,
}

/// Request to set a custom quota for a subject.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SetQuotaRequest {
    pub quota: ResourceQuota,
}

/// Request to update the default quota config.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QuotaConfigUpdate {
    pub default_quota: Option<ResourceQuota>,
    pub max_quota: Option<ResourceQuota>,
    pub burst_allowance: Option<f64>,
}

/// Response for quota check (can this request proceed?).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaCheckResult {
    pub allowed: bool,
    pub status: String,
    pub reason: Option<String>,
}

/// Response for quota reset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaResetResponse {
    pub reset: bool,
    pub subject_id: String,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Resource Quota Manager
// ---------------------------------------------------------------------------

/// Maximum number of alerts to retain per subject.
const MAX_ALERTS_PER_SUBJECT: usize = 100;
/// Number of exceed events before suspension.
const SUSPEND_THRESHOLD: u32 = 5;

/// Manages per-subject resource quotas with usage tracking and alerts.
pub struct ResourceQuotaManager {
    config: tokio::sync::RwLock<QuotaConfig>,
    entries: DashMap<String, Arc<QuotaEntry>>,
    alerts: DashMap<String, VecDeque<QuotaAlert>>,
}

impl ResourceQuotaManager {
    /// Create a new quota manager with the given config.
    pub fn new(config: QuotaConfig) -> Self {
        Self {
            config: tokio::sync::RwLock::new(config),
            entries: DashMap::new(),
            alerts: DashMap::new(),
        }
    }

    /// Get the current config.
    pub async fn get_config(&self) -> QuotaConfig {
        self.config.read().await.clone()
    }

    /// Update the quota configuration.
    pub async fn update_config(&self, update: QuotaConfigUpdate) -> QuotaConfig {
        let mut cfg = self.config.write().await;
        if let Some(default_quota) = update.default_quota {
            cfg.default_quota = default_quota;
        }
        if let Some(max_quota) = update.max_quota {
            cfg.max_quota = max_quota;
        }
        if let Some(burst_allowance) = update.burst_allowance {
            cfg.burst_allowance = burst_allowance.max(0.0).min(2.0);
        }
        cfg.clone()
    }

    /// Get or create a quota entry for a subject.
    fn get_or_create_entry(&self, subject_id: &str) -> Arc<QuotaEntry> {
        if let Some(entry) = self.entries.get(subject_id) {
            return entry.value().clone();
        }

        let quota = tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let cfg = self.config.read().await;
                cfg.default_quota.clone()
            })
        });

        let entry = Arc::new(QuotaEntry {
            subject_id: subject_id.to_string(),
            quota: tokio::sync::RwLock::new(quota),
            current_usage: ResourceUsage::default(),
            status: tokio::sync::RwLock::new(QuotaStatus::Active),
            last_updated: tokio::sync::RwLock::new(Utc::now()),
            exceed_count: AtomicU32::new(0),
        });

        self.entries
            .entry(subject_id.to_string())
            .or_insert_with(|| entry.clone());
        entry
    }

    /// Check if a request is allowed under the subject's quota.
    ///
    /// Checks concurrent requests and tokens/min. Returns a QuotaCheckResult.
    pub async fn check_quota(&self, subject_id: &str) -> QuotaCheckResult {
        let entry = self.get_or_create_entry(subject_id);
        let quota = entry.quota.read().await;
        let status = *entry.status.read().await;

        // If suspended, deny immediately
        if status == QuotaStatus::Suspended {
            return QuotaCheckResult {
                allowed: false,
                status: "suspended".to_string(),
                reason: Some("Subject has been suspended due to repeated quota violations".to_string()),
            };
        }

        let usage = &entry.current_usage;
        let concurrent = usage.concurrent_requests.load(std::sync::atomic::Ordering::Relaxed);
        let tokens = usage.tokens_this_minute.load(std::sync::atomic::Ordering::Relaxed);

        // Apply burst allowance
        let cfg = self.config.read().await;
        let burst = cfg.burst_allowance;
        drop(cfg);

        let effective_max_concurrent = (quota.max_concurrent_requests as f64 * burst) as u32;
        let effective_max_tokens = (quota.max_tokens_per_minute as f64 * burst) as u32;

        if concurrent >= effective_max_concurrent {
            self.record_exceed(subject_id, "concurrent_requests", concurrent as u64, quota.max_concurrent_requests as u64)
                .await;
            return QuotaCheckResult {
                allowed: false,
                status: "exceeded".to_string(),
                reason: Some(format!(
                    "Concurrent request limit exceeded: {}/{}",
                    concurrent, quota.max_concurrent_requests
                )),
            };
        }

        if tokens >= effective_max_tokens {
            self.record_exceed(subject_id, "tokens_per_minute", tokens as u64, quota.max_tokens_per_minute as u64)
                .await;
            return QuotaCheckResult {
                allowed: false,
                status: "exceeded".to_string(),
                reason: Some(format!(
                    "Token rate limit exceeded: {}/{} per minute",
                    tokens, quota.max_tokens_per_minute
                )),
            };
        }

        QuotaCheckResult {
            allowed: true,
            status: "active".to_string(),
            reason: None,
        }
    }

    /// Record that a resource was exceeded and potentially update status.
    async fn record_exceed(&self, subject_id: &str, resource: &str, current: u64, limit: u64) {
        let entry = self.get_or_create_entry(subject_id);

        let count = entry.exceed_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;

        // Generate alert
        let alert = QuotaAlert {
            subject_id: subject_id.to_string(),
            resource: resource.to_string(),
            current,
            limit,
            percentage: if limit > 0 { (current as f64 / limit as f64) * 100.0 } else { 100.0 },
            timestamp: Utc::now(),
            alert_type: QuotaAlertType::Exceeded,
        };
        self.push_alert(subject_id, alert).await;

        // Update status
        if count >= SUSPEND_THRESHOLD {
            let mut status = entry.status.write().await;
            *status = QuotaStatus::Suspended;

            let suspend_alert = QuotaAlert {
                subject_id: subject_id.to_string(),
                resource: resource.to_string(),
                current,
                limit,
                percentage: if limit > 0 { (current as f64 / limit as f64) * 100.0 } else { 100.0 },
                timestamp: Utc::now(),
                alert_type: QuotaAlertType::Suspended,
            };
            self.push_alert(subject_id, suspend_alert).await;

            warn!(
                subject_id = subject_id,
                exceed_count = count,
                "Subject suspended due to repeated quota violations"
            );
        } else {
            let mut status = entry.status.write().await;
            *status = QuotaStatus::Exceeded;
        }

        *entry.last_updated.write().await = Utc::now();
    }

    /// Record usage increment for a resource.
    pub fn increment_usage(&self, subject_id: &str, resource: &str, amount: u32) {
        let entry = self.get_or_create_entry(subject_id);
        match resource {
            "concurrent_requests" => {
                entry.current_usage.concurrent_requests.fetch_add(amount, std::sync::atomic::Ordering::Relaxed);
            }
            "tokens" => {
                entry.current_usage.tokens_this_minute.fetch_add(amount, std::sync::atomic::Ordering::Relaxed);
            }
            "vram_mb" => {
                entry.current_usage.vram_used_mb.fetch_add(amount as u64, std::sync::atomic::Ordering::Relaxed);
            }
            "storage_mb" => {
                entry.current_usage.storage_used_mb.fetch_add(amount as u64, std::sync::atomic::Ordering::Relaxed);
            }
            "models_loaded" => {
                entry.current_usage.models_loaded.fetch_add(amount, std::sync::atomic::Ordering::Relaxed);
            }
            "bandwidth_mbps" => {
                entry.current_usage.bandwidth_mbps.fetch_add(amount as u64, std::sync::atomic::Ordering::Relaxed);
            }
            _ => {}
        }
    }

    /// Decrement usage for a resource.
    pub fn decrement_usage(&self, subject_id: &str, resource: &str, amount: u32) {
        let entry = self.get_or_create_entry(subject_id);
        match resource {
            "concurrent_requests" => {
                let _ = entry.current_usage.concurrent_requests.fetch_update(
                    std::sync::atomic::Ordering::Relaxed,
                    std::sync::atomic::Ordering::Relaxed,
                    |v| v.checked_sub(amount),
                );
            }
            "tokens" => {
                let _ = entry.current_usage.tokens_this_minute.fetch_update(
                    std::sync::atomic::Ordering::Relaxed,
                    std::sync::atomic::Ordering::Relaxed,
                    |v| v.checked_sub(amount),
                );
            }
            "vram_mb" => {
                let _ = entry.current_usage.vram_used_mb.fetch_update(
                    std::sync::atomic::Ordering::Relaxed,
                    std::sync::atomic::Ordering::Relaxed,
                    |v| v.checked_sub(amount as u64),
                );
            }
            "storage_mb" => {
                let _ = entry.current_usage.storage_used_mb.fetch_update(
                    std::sync::atomic::Ordering::Relaxed,
                    std::sync::atomic::Ordering::Relaxed,
                    |v| v.checked_sub(amount as u64),
                );
            }
            "models_loaded" => {
                let _ = entry.current_usage.models_loaded.fetch_update(
                    std::sync::atomic::Ordering::Relaxed,
                    std::sync::atomic::Ordering::Relaxed,
                    |v| v.checked_sub(amount),
                );
            }
            "bandwidth_mbps" => {
                let _ = entry.current_usage.bandwidth_mbps.fetch_update(
                    std::sync::atomic::Ordering::Relaxed,
                    std::sync::atomic::Ordering::Relaxed,
                    |v| v.checked_sub(amount as u64),
                );
            }
            _ => {}
        }
    }

    /// Reset usage counters for a subject.
    pub async fn reset_usage(&self, subject_id: &str) -> QuotaResetResponse {
        let entry = self.get_or_create_entry(subject_id);
        entry.current_usage.reset();
        entry.exceed_count.store(0, std::sync::atomic::Ordering::Relaxed);
        *entry.status.write().await = QuotaStatus::Active;
        *entry.last_updated.write().await = Utc::now();

        info!(subject_id = subject_id, "Quota usage reset");
        QuotaResetResponse {
            reset: true,
            subject_id: subject_id.to_string(),
            message: "Usage counters reset successfully".to_string(),
        }
    }

    /// Set a custom quota for a subject.
    pub async fn set_quota(&self, subject_id: &str, quota: ResourceQuota) -> bool {
        // Clamp to max_quota
        let cfg = self.config.read().await;
        let clamped = ResourceQuota {
            max_concurrent_requests: quota.max_concurrent_requests.min(cfg.max_quota.max_concurrent_requests),
            max_tokens_per_minute: quota.max_tokens_per_minute.min(cfg.max_quota.max_tokens_per_minute),
            max_vram_mb: quota.max_vram_mb.min(cfg.max_quota.max_vram_mb),
            max_storage_mb: quota.max_storage_mb.min(cfg.max_quota.max_storage_mb),
            max_models_loaded: quota.max_models_loaded.min(cfg.max_quota.max_models_loaded),
            max_bandwidth_mbps: quota.max_bandwidth_mbps.min(cfg.max_quota.max_bandwidth_mbps),
        };
        drop(cfg);

        let entry = self.get_or_create_entry(subject_id);
        *entry.quota.write().await = clamped;
        info!(subject_id = subject_id, "Custom quota set");
        true
    }

    /// Get quota details for a specific subject.
    pub async fn get_quota(&self, subject_id: &str) -> Option<QuotaEntrySnapshot> {
        self.entries.get(subject_id).map(|entry| {
            let e = entry.value();
            let quota = tokio::task::block_in_place(|| {
                let rt = tokio::runtime::Handle::current();
                rt.block_on(async {
                    let q = e.quota.read().await;
                    q.clone()
                })
            });
            let status = tokio::task::block_in_place(|| {
                let rt = tokio::runtime::Handle::current();
                rt.block_on(async {
                    e.status.read().await.to_string()
                })
            });
            let last_updated = tokio::task::block_in_place(|| {
                let rt = tokio::runtime::Handle::current();
                rt.block_on(async {
                    *e.last_updated.read().await
                })
            });

            QuotaEntrySnapshot {
                subject_id: e.subject_id.clone(),
                quota,
                usage: e.current_usage.snapshot(),
                status,
                last_updated,
                exceed_count: e.exceed_count.load(std::sync::atomic::Ordering::Relaxed),
            }
        })
    }

    /// Get usage for a specific subject.
    pub async fn get_usage(&self, subject_id: &str) -> Option<ResourceUsageSnapshot> {
        self.entries.get(subject_id).map(|entry| entry.value().current_usage.snapshot())
    }

    /// List all quota entries.
    pub async fn list_quotas(&self) -> Vec<QuotaEntrySnapshot> {
        self.entries
            .iter()
            .map(|kv| {
                let e = kv.value();
                let quota = tokio::task::block_in_place(|| {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        e.quota.read().await.clone()
                    })
                });
                let status = tokio::task::block_in_place(|| {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        e.status.read().await.to_string()
                    })
                });
                let last_updated = tokio::task::block_in_place(|| {
                    let rt = tokio::runtime::Handle::current();
                    rt.block_on(async {
                        *e.last_updated.read().await
                    })
                });

                QuotaEntrySnapshot {
                    subject_id: e.subject_id.clone(),
                    quota,
                    usage: e.current_usage.snapshot(),
                    status,
                    last_updated,
                    exceed_count: e.exceed_count.load(std::sync::atomic::Ordering::Relaxed),
                }
            })
            .collect()
    }

    /// Get recent alerts (optionally filtered by subject).
    pub async fn get_alerts(&self, subject_id: Option<&str>, limit: usize) -> Vec<QuotaAlert> {
        let mut all_alerts: Vec<QuotaAlert> = if let Some(sid) = subject_id {
            self.alerts
                .get(sid)
                .map(|alerts| alerts.iter().cloned().collect())
                .unwrap_or_default()
        } else {
            self.alerts
                .iter()
                .flat_map(|kv| {
                    let alerts: Vec<QuotaAlert> = kv.value().iter().cloned().collect();
                    alerts
                })
                .collect()
        };

        // Sort by timestamp descending
        all_alerts.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        all_alerts.truncate(limit);
        all_alerts
    }

    /// Push an alert for a subject.
    async fn push_alert(&self, subject_id: &str, alert: QuotaAlert) {
        let mut alerts = self
            .alerts
            .entry(subject_id.to_string())
            .or_insert_with(|| VecDeque::with_capacity(MAX_ALERTS_PER_SUBJECT));
        alerts.push_back(alert);
        while alerts.len() > MAX_ALERTS_PER_SUBJECT {
            alerts.pop_front();
        }
    }

    /// Unsuspend a subject (admin action).
    pub async fn unsuspend(&self, subject_id: &str) -> bool {
        if let Some(entry) = self.entries.get(subject_id) {
            *entry.status.write().await = QuotaStatus::Active;
            entry.exceed_count.store(0, std::sync::atomic::Ordering::Relaxed);
            *entry.last_updated.write().await = Utc::now();
            info!(subject_id = subject_id, "Subject unsuspended");
            true
        } else {
            false
        }
    }

    /// Periodically reset token-per-minute counters.
    /// Should be called every 60 seconds.
    pub fn reset_token_counters(&self) {
        for entry in self.entries.iter() {
            entry
                .value()
                .current_usage
                .tokens_this_minute
                .store(0, std::sync::atomic::Ordering::Relaxed);
        }
    }
}
