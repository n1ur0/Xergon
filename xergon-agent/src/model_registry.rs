//! Model Registry: comprehensive model versioning with automated rollback.
//!
//! Provides:
//! - Model version registration with SHA-256 checksum verification
//! - Semantic versioning comparison and range matching (^, ~, >=)
//! - Automated rollback based on health metrics (error rate, latency)
//! - Manual rollback with full audit trail
//! - Version lifecycle: Testing -> Active -> Deprecated -> Deleted
//! - Canary deployment support with percentage-based traffic splitting
//! - Per-version health monitoring with configurable thresholds
//!
//! API endpoints (prefixed with `/api/model-registry/`):
//! - POST   /versions/register            — register a new model version
//! - POST   /versions/{id}/promote        — promote testing -> active
//! - POST   /versions/{id}/deprecate      — mark as deprecated
//! - DELETE /versions/{id}                — soft-delete a version
//! - GET    /versions                     — list versions (filtered)
//! - GET    /versions/{id}                — get version details + metrics
//! - GET    /models/{name}/active         — get active version for a model
//! - POST   /models/{name}/rollback       — manual rollback
//! - GET    /models/{name}/history        — version history + rollback events
//! - POST   /versions/{id}/diff           — diff two versions
//! - GET    /versions/rollback-config     — get rollback configuration
//! - PUT    /versions/rollback-config     — update rollback configuration
//! - GET    /versions/health              — health monitor status
//! - POST   /versions/{id}/metrics        — submit metrics for a version

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};

use crate::api::AppState;

// ============================================================================
// Core Data Types
// ============================================================================

/// Lifecycle status of a model version.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum VersionStatus {
    /// Currently serving traffic.
    Active,
    /// No longer recommended; may still be serving residual traffic.
    Deprecated,
    /// Soft-deleted; metadata retained for audit.
    Deleted,
    /// Under test; not yet promoted to active.
    Testing,
}

impl std::fmt::Display for VersionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "active"),
            Self::Deprecated => write!(f, "deprecated"),
            Self::Deleted => write!(f, "deleted"),
            Self::Testing => write!(f, "testing"),
        }
    }
}

impl std::str::FromStr for VersionStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(Self::Active),
            "deprecated" => Ok(Self::Deprecated),
            "deleted" => Ok(Self::Deleted),
            "testing" => Ok(Self::Testing),
            _ => Err(format!("unknown version status: '{}'", s)),
        }
    }
}

/// A registered model version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelVersion {
    /// Unique numeric identifier for this version record.
    pub version_id: u64,
    /// Logical model name (e.g. "llama-3.1-8b").
    pub model_name: String,
    /// Semantic version string (e.g. "1.2.3").
    pub version: String,
    /// Path to the model artifact on disk.
    pub artifact_path: String,
    /// SHA-256 hex digest of the artifact.
    pub checksum: String,
    /// When this version was registered.
    pub created_at: DateTime<Utc>,
    /// Who registered this version.
    pub created_by: String,
    /// Arbitrary JSON metadata.
    #[serde(default)]
    pub metadata: serde_json::Value,
    /// Current lifecycle status.
    pub status: VersionStatus,
}

/// Runtime metrics for a version, updated by model_serving.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VersionMetrics {
    /// Total requests served since promotion/registration.
    pub request_count: u64,
    /// Total errors observed.
    pub error_count: u64,
    /// Sum of p95 latencies (ms) for computing average.
    pub total_latency_p95_ms: u64,
    /// Last time metrics were updated.
    pub last_updated: Option<DateTime<Utc>>,
}

impl VersionMetrics {
    /// Current error rate as a fraction (0.0..1.0).
    pub fn error_rate(&self) -> f64 {
        if self.request_count == 0 {
            0.0
        } else {
            self.error_count as f64 / self.request_count as f64
        }
    }

    /// Average p95 latency in ms.
    pub fn avg_latency_p95(&self) -> f64 {
        if self.request_count == 0 {
            0.0
        } else {
            self.total_latency_p95_ms as f64 / self.request_count as f64
        }
    }
}

/// Internal version entry combining metadata + metrics.
#[derive(Debug)]
pub struct VersionInfo {
    pub version: ModelVersion,
    pub metrics: Mutex<VersionMetrics>,
    /// Optional canary percentage (0.0..100.0).
    pub canary_percentage: f64,
    /// When this version was promoted to active.
    pub promoted_at: Option<DateTime<Utc>>,
    /// Version ID of the previous active version (if promoted from a rollback source).
    pub promoted_from_version_id: Option<u64>,
}

/// A record of a rollback event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackEvent {
    /// Timestamp of the rollback.
    pub timestamp: DateTime<Utc>,
    /// Version ID that was rolled back FROM.
    pub from_version_id: u64,
    /// Version ID that was rolled back TO.
    pub to_version_id: u64,
    /// Human-readable reason.
    pub reason: String,
    /// Whether this was an automatic or manual rollback.
    pub automatic: bool,
}

/// Per-model entry in the registry.
#[derive(Debug, Default)]
pub struct ModelEntry {
    /// Currently active version ID (if any).
    pub current_active_version: Option<u64>,
    /// Ordered list of version IDs for this model (newest last).
    pub version_ids: Vec<u64>,
    /// History of all rollback events.
    pub rollback_history: Vec<RollbackEvent>,
}

/// Configuration for the automated rollback system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackConfig {
    /// Error rate threshold that triggers rollback (default: 0.05 = 5%).
    pub error_rate_threshold: f64,
    /// Minimum requests before evaluating health (default: 100).
    pub min_requests_before_eval: u64,
    /// Latency degradation factor: rollback if new avg > factor * old avg (default: 2.0).
    pub latency_degradation_factor: f64,
    /// Minimum seconds between automatic rollbacks (default: 300 = 5 min).
    pub cooldown_period_secs: u64,
}

impl Default for RollbackConfig {
    fn default() -> Self {
        Self {
            error_rate_threshold: 0.05,
            min_requests_before_eval: 100,
            latency_degradation_factor: 2.0,
            cooldown_period_secs: 300,
        }
    }
}

// ============================================================================
// Semantic Versioning
// ============================================================================

/// Parsed semantic version (major.minor.patch).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SemVer {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl SemVer {
    /// Parse a semver string like "1.2.3".
    pub fn parse(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.trim_start_matches('v').split('.').collect();
        if parts.len() != 3 {
            return Err(format!("invalid semver '{}': expected major.minor.patch", s));
        }
        Ok(Self {
            major: parts[0].parse().map_err(|_| format!("invalid major '{}'", parts[0]))?,
            minor: parts[1].parse().map_err(|_| format!("invalid minor '{}'", parts[1]))?,
            patch: parts[2].parse().map_err(|_| format!("invalid patch '{}'", parts[2]))?,
        })
    }

    /// Check if this version satisfies a range specifier.
    /// Supports: "^1.2.3", "~1.2.3", ">=1.2.3", "1.2.3" (exact).
    pub fn satisfies(&self, range: &str) -> bool {
        let range = range.trim();
        if range.starts_with('^') {
            // Caret: ^1.2.3 := >=1.2.3, <2.0.0
            if let Ok(min) = SemVer::parse(&range[1..]) {
                let max = SemVer { major: min.major + 1, minor: 0, patch: 0 };
                return self >= &min && self < &max;
            }
        } else if range.starts_with('~') {
            // Tilde: ~1.2.3 := >=1.2.3, <1.3.0
            if let Ok(min) = SemVer::parse(&range[1..]) {
                let max = SemVer { major: min.major, minor: min.minor + 1, patch: 0 };
                return self >= &min && self < &max;
            }
        } else if range.starts_with(">=") {
            // Greater-or-equal: >=1.2.3
            if let Ok(min) = SemVer::parse(&range[2..]) {
                return self >= &min;
            }
        } else {
            // Exact match
            if let Ok(exact) = SemVer::parse(range) {
                return self == &exact;
            }
        }
        false
    }
}

// ============================================================================
// Model Registry
// ============================================================================

/// Comprehensive model versioning registry with automated rollback.
pub struct ModelRegistry {
    /// model_name -> ModelEntry
    entries: DashMap<String, ModelEntry>,
    /// version_id -> VersionInfo
    versions: DashMap<u64, VersionInfo>,
    /// Monotonically increasing ID generator.
    next_id: AtomicU64,
    /// Rollback configuration.
    rollback_config: RwLock<RollbackConfig>,
    /// Timestamp of the last automatic rollback (per-model cooldown).
    last_auto_rollback: Mutex<HashMap<String, DateTime<Utc>>>,
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelRegistry {
    /// Create a new empty registry with default rollback config.
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
            versions: DashMap::new(),
            next_id: AtomicU64::new(1),
            rollback_config: RwLock::new(RollbackConfig::default()),
            last_auto_rollback: Mutex::new(HashMap::new()),
        }
    }

    /// Create a new registry with a custom rollback config.
    pub fn with_config(config: RollbackConfig) -> Self {
        Self {
            entries: DashMap::new(),
            versions: DashMap::new(),
            next_id: AtomicU64::new(1),
            rollback_config: RwLock::new(config),
            last_auto_rollback: Mutex::new(HashMap::new()),
        }
    }

    // ------------------------------------------------------------------
    // ID generation
    // ------------------------------------------------------------------

    fn alloc_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    // ------------------------------------------------------------------
    // Registration
    // ------------------------------------------------------------------

    /// Register a new model version.
    ///
    /// The version starts in `Testing` status. Use `promote_version` to move
    /// it to `Active`.
    pub fn register_version(
        &self,
        model_name: String,
        version: String,
        artifact_path: String,
        checksum: String,
        created_by: String,
        metadata: serde_json::Value,
    ) -> Result<ModelVersion, String> {
        // Validate semver format
        SemVer::parse(&version)?;

        // Validate checksum format (64 hex chars)
        if checksum.len() != 64 || !checksum.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!("invalid SHA-256 checksum: must be 64 hex characters, got {} chars", checksum.len()));
        }

        let id = self.alloc_id();
        let now = Utc::now();

        let mv = ModelVersion {
            version_id: id,
            model_name: model_name.clone(),
            version: version.clone(),
            artifact_path: artifact_path.clone(),
            checksum: checksum.clone(),
            created_at: now,
            created_by: created_by.clone(),
            metadata,
            status: VersionStatus::Testing,
        };

        let info = VersionInfo {
            version: mv.clone(),
            metrics: Mutex::new(VersionMetrics::default()),
            canary_percentage: 0.0,
            promoted_at: None,
            promoted_from_version_id: None,
        };

        self.versions.insert(id, info);

        let mut entry = self.entries.entry(model_name.clone()).or_default();
        entry.version_ids.push(id);

        info!(
            version_id = id,
            model = %model_name,
            version = %version,
            "Registered model version"
        );

        Ok(mv)
    }

    // ------------------------------------------------------------------
    // Checksum verification
    // ------------------------------------------------------------------

    /// Verify that the artifact at `artifact_path` matches the expected checksum.
    pub fn verify_checksum(&self, version_id: u64) -> Result<bool, String> {
        let info = self.versions.get(&version_id)
            .ok_or_else(|| format!("version {} not found", version_id))?;

        let artifact_path = &info.version.artifact_path;
        let expected = &info.version.checksum;

        let data = std::fs::read(artifact_path)
            .map_err(|e| format!("failed to read artifact '{}': {}", artifact_path, e))?;

        let mut hasher = Sha256::new();
        hasher.update(&data);
        let digest = hex::encode(hasher.finalize());

        Ok(digest == *expected)
    }

    /// Compute SHA-256 checksum of a file.
    pub fn compute_checksum(file_path: &str) -> Result<String, String> {
        let data = std::fs::read(file_path)
            .map_err(|e| format!("failed to read '{}': {}", file_path, e))?;
        let mut hasher = Sha256::new();
        hasher.update(&data);
        Ok(hex::encode(hasher.finalize()))
    }

    // ------------------------------------------------------------------
    // Promotion (Testing -> Active)
    // ------------------------------------------------------------------

    /// Promote a version from Testing to Active.
    ///
    /// If `canary_percentage` > 0, the version is promoted but traffic is
    /// gradually shifted (the old version remains Active until canary reaches 100%).
    /// If `canary_percentage` is 0.0 or None, the version becomes the sole active.
    pub fn promote_version(
        &self,
        version_id: u64,
        canary_percentage: Option<f64>,
    ) -> Result<ModelVersion, String> {
        let mut info = self.versions.get_mut(&version_id)
            .ok_or_else(|| format!("version {} not found", version_id))?;

        if info.version.status != VersionStatus::Testing {
            return Err(format!(
                "cannot promote version {} in status '{}'; must be 'testing'",
                version_id, info.version.status
            ));
        }

        let model_name = info.version.model_name.clone();
        let old_active_id;

        {
            let mut entry = self.entries.get_mut(&model_name)
                .ok_or_else(|| format!("model '{}' not found in entries", model_name))?;

            old_active_id = entry.current_active_version;
            entry.current_active_version = Some(version_id);
            info.promoted_from_version_id = old_active_id;
        }

        info.version.status = VersionStatus::Active;
        info.promoted_at = Some(Utc::now());
        info.canary_percentage = canary_percentage.unwrap_or(100.0);

        let mv = info.version.clone();
        drop(info);

        // If fully promoted (not canary), demote old active to Deprecated
        if canary_percentage.unwrap_or(100.0) >= 100.0 {
            if let Some(old_id) = old_active_id {
                if let Some(mut old_info) = self.versions.get_mut(&old_id) {
                    if old_info.version.status == VersionStatus::Active {
                        old_info.version.status = VersionStatus::Deprecated;
                        info!(
                            old_version_id = old_id,
                            new_version_id = version_id,
                            model = %model_name,
                            "Deprecated previous active version"
                        );
                    }
                }
            }
        }

        info!(
            version_id = version_id,
            model = %model_name,
            version = %mv.version,
            canary = canary_percentage.unwrap_or(100.0),
            "Promoted version to active"
        );

        // Reset metrics for the promoted version so we start fresh
        if let Some(mut info) = self.versions.get_mut(&version_id) {
            *info.metrics.lock().unwrap() = VersionMetrics::default();
            info.metrics.lock().unwrap().last_updated = Some(Utc::now());
        }

        Ok(mv)
    }

    // ------------------------------------------------------------------
    // Deprecation
    // ------------------------------------------------------------------

    /// Mark a version as Deprecated.
    pub fn deprecate_version(&self, version_id: u64) -> Result<ModelVersion, String> {
        let mut info = self.versions.get_mut(&version_id)
            .ok_or_else(|| format!("version {} not found", version_id))?;

        match info.version.status {
            VersionStatus::Active => {
                // Cannot deprecate the current active version without promoting another.
                return Err(format!(
                    "cannot deprecate active version {}; promote another version first",
                    version_id
                ));
            }
            VersionStatus::Deprecated => {
                return Err(format!("version {} is already deprecated", version_id));
            }
            VersionStatus::Deleted => {
                return Err(format!("version {} is deleted", version_id));
            }
            VersionStatus::Testing => {}
        }

        info.version.status = VersionStatus::Deprecated;
        let mv = info.version.clone();

        info!(
            version_id = version_id,
            model = %mv.model_name,
            version = %mv.version,
            "Deprecated version"
        );

        Ok(mv)
    }

    // ------------------------------------------------------------------
    // Soft delete
    // ------------------------------------------------------------------

    /// Soft-delete a version (sets status to Deleted, retains metadata).
    pub fn delete_version(&self, version_id: u64) -> Result<ModelVersion, String> {
        let mut info = self.versions.get_mut(&version_id)
            .ok_or_else(|| format!("version {} not found", version_id))?;

        if info.version.status == VersionStatus::Active {
            return Err(format!(
                "cannot delete active version {}; promote another version first",
                version_id
            ));
        }

        info.version.status = VersionStatus::Deleted;
        let mv = info.version.clone();

        info!(
            version_id = version_id,
            model = %mv.model_name,
            version = %mv.version,
            "Soft-deleted version"
        );

        Ok(mv)
    }

    // ------------------------------------------------------------------
    // Rollback
    // ------------------------------------------------------------------

    /// Perform a manual rollback for a model to its previous version.
    pub fn rollback(&self, model_name: &str, reason: &str) -> Result<RollbackEvent, String> {
        let current_id = {
            let entry = self.entries.get(model_name)
                .ok_or_else(|| format!("model '{}' not found", model_name))?;
            entry.current_active_version
                .ok_or_else(|| format!("no active version for model '{}'", model_name))?
        };

        // Find the previous version (most recent non-active, non-deleted)
        let previous_id = {
            let entry = self.entries.get(model_name)
                .ok_or_else(|| format!("model '{}' not found", model_name))?;

            let mut candidate: Option<u64> = None;
            for &vid in entry.version_ids.iter().rev() {
                if vid == current_id {
                    continue;
                }
                if let Some(vinfo) = self.versions.get(&vid) {
                    if vinfo.version.status != VersionStatus::Deleted {
                        candidate = Some(vid);
                        break;
                    }
                }
            }
            candidate.ok_or_else(|| "no previous version available for rollback")?
        };

        self.execute_rollback(model_name, current_id, previous_id, reason, false)
    }

    /// Rollback to a specific version ID.
    pub fn rollback_to_version(
        &self,
        model_name: &str,
        target_version_id: u64,
        reason: &str,
    ) -> Result<RollbackEvent, String> {
        // Verify the target version exists and belongs to this model
        let target_info = self.versions.get(&target_version_id)
            .ok_or_else(|| format!("version {} not found", target_version_id))?;

        if target_info.version.model_name != model_name {
            return Err(format!(
                "version {} belongs to model '{}', not '{}'",
                target_version_id, target_info.version.model_name, model_name
            ));
        }

        if target_info.version.status == VersionStatus::Deleted {
            return Err(format!("cannot rollback to deleted version {}", target_version_id));
        }

        drop(target_info);

        let current_id = {
            let entry = self.entries.get(model_name)
                .ok_or_else(|| format!("model '{}' not found", model_name))?;
            entry.current_active_version
                .ok_or_else(|| format!("no active version for model '{}'", model_name))?
        };

        if current_id == target_version_id {
            return Err("target version is already the active version".into());
        }

        self.execute_rollback(model_name, current_id, target_version_id, reason, false)
    }

    /// Internal rollback execution.
    fn execute_rollback(
        &self,
        model_name: &str,
        from_id: u64,
        to_id: u64,
        reason: &str,
        automatic: bool,
    ) -> Result<RollbackEvent, String> {
        // Demote current active
        if let Some(mut info) = self.versions.get_mut(&from_id) {
            if info.version.status == VersionStatus::Active {
                info.version.status = VersionStatus::Testing;
            }
        }

        // Promote the target version
        if let Some(mut info) = self.versions.get_mut(&to_id) {
            info.version.status = VersionStatus::Active;
            info.promoted_at = Some(Utc::now());
            info.promoted_from_version_id = Some(from_id);
            info.canary_percentage = 100.0;
            *info.metrics.lock().unwrap() = VersionMetrics::default();
            info.metrics.lock().unwrap().last_updated = Some(Utc::now());
        }

        // Update entry
        {
            let mut entry = self.entries.get_mut(model_name)
                .ok_or_else(|| format!("model '{}' not found", model_name))?;
            entry.current_active_version = Some(to_id);
        }

        let event = RollbackEvent {
            timestamp: Utc::now(),
            from_version_id: from_id,
            to_version_id: to_id,
            reason: reason.to_string(),
            automatic,
        };

        // Record rollback history
        {
            let mut entry = self.entries.get_mut(model_name)
                .ok_or_else(|| format!("model '{}' not found", model_name))?;
            entry.rollback_history.push(event.clone());
        }

        info!(
            model = %model_name,
            from = from_id,
            to = to_id,
            automatic = automatic,
            reason = %reason,
            "Rollback executed"
        );

        Ok(event)
    }

    /// Check cooldown: has enough time passed since the last auto-rollback for this model?
    fn is_in_cooldown(&self, model_name: &str) -> bool {
        let config = self.rollback_config.read().unwrap();
        let last = {
            let map = self.last_auto_rollback.lock().unwrap();
            map.get(model_name).copied()
        };
        if let Some(last_time) = last {
            let elapsed = Utc::now() - last_time;
            elapsed.num_seconds() < config.cooldown_period_secs as i64
        } else {
            false
        }
    }

    // ------------------------------------------------------------------
    // Health monitoring & auto-rollback
    // ------------------------------------------------------------------

    /// Submit metrics for a version (called by model_serving.rs).
    pub fn submit_metrics(
        &self,
        version_id: u64,
        error_count: u64,
        latency_p95_ms: u64,
    ) -> Result<(), String> {
        let info = self.versions.get(&version_id)
            .ok_or_else(|| format!("version {} not found", version_id))?;

        let mut metrics = info.metrics.lock().unwrap();
        metrics.request_count += 1;
        if error_count > 0 {
            metrics.error_count += error_count;
        }
        metrics.total_latency_p95_ms += latency_p95_ms;
        metrics.last_updated = Some(Utc::now());
        drop(metrics);

        debug!(
            version_id = version_id,
            model = %info.version.model_name,
            "Metrics submitted"
        );

        // After updating, check if we should auto-rollback
        self.maybe_auto_rollback(version_id, &info.version.model_name);

        Ok(())
    }

    /// Evaluate health and trigger auto-rollback if thresholds are exceeded.
    fn maybe_auto_rollback(&self, version_id: u64, model_name: &str) {
        // Check cooldown first
        if self.is_in_cooldown(model_name) {
            debug!(model = %model_name, "Auto-rollback suppressed: within cooldown period");
            return;
        }

        let config = self.rollback_config.read().unwrap();

        // Get current version metrics
        let current_metrics = {
            let info = self.versions.get(&version_id);
            match info {
                Some(i) => {
                    let m = i.metrics.lock().unwrap().clone();
                    drop(i);
                    m
                }
                None => return,
            }
        };

        // Need minimum requests before evaluating
        if current_metrics.request_count < config.min_requests_before_eval {
            return;
        }

        let mut should_rollback = false;
        let mut reason = String::new();

        // Check error rate
        let error_rate = current_metrics.error_rate();
        if error_rate > config.error_rate_threshold {
            should_rollback = true;
            reason = format!(
                "error rate {:.2}% exceeds threshold {:.2}% ({} errors / {} requests)",
                error_rate * 100.0,
                config.error_rate_threshold * 100.0,
                current_metrics.error_count,
                current_metrics.request_count
            );
        }

        // Check latency degradation vs previous version
        if !should_rollback {
            let current_latency = current_metrics.avg_latency_p95();

            if let Some(prev_id) = {
                let entry = self.entries.get(model_name);
                entry.and_then(|e| {
                    // Get the promoted_from of current version
                    let info = self.versions.get(&version_id)?;
                    info.promoted_from_version_id
                })
            } {
                if let Some(prev_info) = self.versions.get(&prev_id) {
                    let prev_metrics = prev_info.metrics.lock().unwrap();
                    let prev_latency = prev_metrics.avg_latency_p95();
                    drop(prev_metrics);

                    if prev_latency > 0.0
                        && current_latency > prev_latency * config.latency_degradation_factor
                    {
                        should_rollback = true;
                        reason = format!(
                            "latency degradation: current p95 {:.1}ms > {:.1}x previous {:.1}ms",
                            current_latency, config.latency_degradation_factor, prev_latency
                        );
                    }
                }
            }
        }

        drop(config);

        if should_rollback {
            warn!(
                model = %model_name,
                version_id = version_id,
                reason = %reason,
                "Triggering automatic rollback"
            );

            // Record last auto-rollback time
            {
                let mut map = self.last_auto_rollback.lock().unwrap();
                map.insert(model_name.to_string(), Utc::now());
            }

            // Perform the rollback (best-effort; don't panic on failure)
            let _ = self.execute_rollback(model_name, version_id, {
                // Find the previous version
                let entry = self.entries.get(model_name);
                let prev = entry.and_then(|e| {
                    let current_info = self.versions.get(&version_id)?;
                    current_info.promoted_from_version_id
                });
                prev.unwrap_or(0) // 0 won't match any valid version
            }, &reason, true);
        }
    }

    /// Evaluate health for a specific version and return the assessment.
    pub fn evaluate_health(&self, version_id: u64) -> Result<HealthAssessment, String> {
        let info = self.versions.get(&version_id)
            .ok_or_else(|| format!("version {} not found", version_id))?;

        let metrics = info.metrics.lock().unwrap().clone();
        let config = self.rollback_config.read().unwrap();

        let assessment = HealthAssessment {
            version_id,
            model_name: info.version.model_name.clone(),
            version: info.version.version.clone(),
            status: info.version.status,
            metrics: metrics.clone(),
            healthy: {
                if metrics.request_count < config.min_requests_before_eval {
                    true // Not enough data yet
                } else {
                    metrics.error_rate() <= config.error_rate_threshold
                }
            },
            error_rate_threshold: config.error_rate_threshold,
            min_requests: config.min_requests_before_eval,
            latency_factor: config.latency_degradation_factor,
        };

        Ok(assessment)
    }

    /// Get health summary for all active versions.
    pub fn health_summary(&self) -> Vec<HealthAssessment> {
        let mut assessments = Vec::new();
        for entry in self.entries.iter() {
            if let Some(active_id) = entry.value().current_active_version {
                if let Ok(a) = self.evaluate_health(active_id) {
                    assessments.push(a);
                }
            }
        }
        assessments
    }

    // ------------------------------------------------------------------
    // Read operations
    // ------------------------------------------------------------------

    /// Get version info by ID.
    pub fn get_version(&self, version_id: u64) -> Option<VersionDetail> {
        let info = self.versions.get(&version_id)?;
        let metrics = info.metrics.lock().unwrap().clone();
        Some(VersionDetail {
            version: info.version.clone(),
            metrics,
            canary_percentage: info.canary_percentage,
            promoted_at: info.promoted_at,
            promoted_from_version_id: info.promoted_from_version_id,
        })
    }

    /// Get the active version for a model.
    pub fn get_active_version(&self, model_name: &str) -> Option<ModelVersion> {
        let entry = self.entries.get(model_name)?;
        let active_id = entry.current_active_version?;
        let info = self.versions.get(&active_id)?;
        Some(info.version.clone())
    }

    /// List all versions for a model.
    pub fn list_model_versions(&self, model_name: &str) -> Vec<ModelVersion> {
        let entry = self.entries.get(model_name);
        match entry {
            Some(e) => e.version_ids.iter()
                .filter_map(|&id| self.versions.get(&id).map(|v| v.version.clone()))
                .collect(),
            None => Vec::new(),
        }
    }

    /// List versions across all models, with optional filtering.
    pub fn list_versions(
        &self,
        filter_model: Option<&str>,
        filter_status: Option<VersionStatus>,
        limit: usize,
        offset: usize,
    ) -> Vec<ModelVersion> {
        let mut result: Vec<ModelVersion> = Vec::new();

        for entry in self.entries.iter() {
            if let Some(ref model) = filter_model {
                if entry.key() != model {
                    continue;
                }
            }
            for &vid in &entry.value().version_ids {
                if let Some(info) = self.versions.get(&vid) {
                    if let Some(ref status) = filter_status {
                        if info.version.status != *status {
                            continue;
                        }
                    }
                    result.push(info.version.clone());
                }
            }
        }

        // Sort by version_id descending (newest first)
        result.sort_by(|a, b| b.version_id.cmp(&a.version_id));

        result.into_iter().skip(offset).take(limit).collect()
    }

    /// Get version history for a model (all versions + rollback events).
    pub fn get_history(&self, model_name: &str) -> Option<ModelHistory> {
        let entry = self.entries.get(model_name)?;
        let versions: Vec<ModelVersion> = entry.version_ids.iter()
            .filter_map(|&id| self.versions.get(&id).map(|v| v.version.clone()))
            .collect();
        let current_active = entry.current_active_version;

        Some(ModelHistory {
            model_name: model_name.to_string(),
            versions,
            current_active_version_id: current_active,
            rollback_history: entry.rollback_history.clone(),
        })
    }

    /// Diff two versions by ID.
    pub fn diff_versions(&self, id_a: u64, id_b: u64) -> Result<VersionDiff, String> {
        let a = self.versions.get(&id_a)
            .ok_or_else(|| format!("version {} not found", id_a))?;
        let b = self.versions.get(&id_b)
            .ok_or_else(|| format!("version {} not found", id_b))?;

        let diff = VersionDiff {
            version_a: a.version.clone(),
            version_b: b.version.clone(),
            same_model: a.version.model_name == b.version.model_name,
            same_checksum: a.version.checksum == b.version.checksum,
            semver_comparison: compare_semver(&a.version.version, &b.version.version),
            metadata_differ: a.version.metadata != b.version.metadata,
            shared_metadata_keys: {
                let a_keys: std::collections::HashSet<String> = a.version.metadata.as_object()
                    .map(|o| o.keys().cloned().collect()).unwrap_or_default();
                let b_keys: std::collections::HashSet<String> = b.version.metadata.as_object()
                    .map(|o| o.keys().cloned().collect()).unwrap_or_default();
                a_keys.intersection(&b_keys).cloned().collect()
            },
            a_only_metadata_keys: {
                let a_keys: std::collections::HashSet<String> = a.version.metadata.as_object()
                    .map(|o| o.keys().cloned().collect()).unwrap_or_default();
                let b_keys: std::collections::HashSet<String> = b.version.metadata.as_object()
                    .map(|o| o.keys().cloned().collect()).unwrap_or_default();
                a_keys.difference(&b_keys).cloned().collect()
            },
            b_only_metadata_keys: {
                let a_keys: std::collections::HashSet<String> = a.version.metadata.as_object()
                    .map(|o| o.keys().cloned().collect()).unwrap_or_default();
                let b_keys: std::collections::HashSet<String> = b.version.metadata.as_object()
                    .map(|o| o.keys().cloned().collect()).unwrap_or_default();
                b_keys.difference(&a_keys).cloned().collect()
            },
        };

        Ok(diff)
    }

    /// Get the rollback configuration.
    pub fn get_rollback_config(&self) -> RollbackConfig {
        self.rollback_config.read().unwrap().clone()
    }

    /// Update the rollback configuration.
    pub fn set_rollback_config(&self, config: RollbackConfig) {
        *self.rollback_config.write().unwrap() = config;
        info!("Rollback configuration updated");
    }

    /// List all known model names.
    pub fn list_models(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.key().clone()).collect()
    }
}

// ============================================================================
// Comparison helpers
// ============================================================================

/// Compare two semver strings. Returns "newer", "older", or "equal".
fn compare_semver(a: &str, b: &str) -> String {
    match (SemVer::parse(a), SemVer::parse(b)) {
        (Ok(sv_a), Ok(sv_b)) => {
            if sv_a > sv_b {
                "newer".to_string()
            } else if sv_a < sv_b {
                "older".to_string()
            } else {
                "equal".to_string()
            }
        }
        _ => "uncomparable".to_string(),
    }
}

// ============================================================================
// Response types
// ============================================================================

/// Detailed version info including metrics.
#[derive(Debug, Clone, Serialize)]
pub struct VersionDetail {
    pub version: ModelVersion,
    pub metrics: VersionMetrics,
    pub canary_percentage: f64,
    pub promoted_at: Option<DateTime<Utc>>,
    pub promoted_from_version_id: Option<u64>,
}

/// Health assessment for a version.
#[derive(Debug, Clone, Serialize)]
pub struct HealthAssessment {
    pub version_id: u64,
    pub model_name: String,
    pub version: String,
    pub status: VersionStatus,
    pub metrics: VersionMetrics,
    pub healthy: bool,
    pub error_rate_threshold: f64,
    pub min_requests: u64,
    pub latency_factor: f64,
}

/// Version history for a model.
#[derive(Debug, Clone, Serialize)]
pub struct ModelHistory {
    pub model_name: String,
    pub versions: Vec<ModelVersion>,
    pub current_active_version_id: Option<u64>,
    pub rollback_history: Vec<RollbackEvent>,
}

/// Diff between two versions.
#[derive(Debug, Clone, Serialize)]
pub struct VersionDiff {
    pub version_a: ModelVersion,
    pub version_b: ModelVersion,
    pub same_model: bool,
    pub same_checksum: bool,
    pub semver_comparison: String,
    pub metadata_differ: bool,
    pub shared_metadata_keys: Vec<String>,
    pub a_only_metadata_keys: Vec<String>,
    pub b_only_metadata_keys: Vec<String>,
}

// ============================================================================
// API Request/Response types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct RegisterVersionRequest {
    pub model_name: String,
    pub version: String,
    pub artifact_path: String,
    pub checksum: String,
    #[serde(default)]
    pub created_by: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct RegisterVersionResponse {
    pub success: bool,
    pub version: ModelVersion,
}

#[derive(Debug, Deserialize)]
pub struct PromoteVersionRequest {
    pub canary_percentage: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct PromoteVersionResponse {
    pub success: bool,
    pub version: ModelVersion,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct DeprecateVersionResponse {
    pub success: bool,
    pub version: ModelVersion,
}

#[derive(Debug, Serialize)]
pub struct DeleteVersionResponse {
    pub success: bool,
    pub version: ModelVersion,
}

#[derive(Debug, Deserialize)]
pub struct RollbackRequest {
    pub reason: Option<String>,
    pub target_version_id: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct RollbackResponse {
    pub success: bool,
    pub event: RollbackEvent,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct SubmitMetricsRequest {
    pub error_count: u64,
    pub latency_p95_ms: u64,
}

#[derive(Debug, Deserialize)]
pub struct DiffRequest {
    pub other_version_id: u64,
}

#[derive(Debug, Deserialize)]
pub struct ListVersionsQuery {
    pub model: Option<String>,
    pub status: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

// ============================================================================
// Router builder
// ============================================================================

/// Build the model registry sub-router.
pub fn build_model_registry_router(state: AppState) -> Router {
    Router::new()
        .route("/api/model-registry/versions/register", post(register_handler))
        .route("/api/model-registry/versions/{id}/promote", post(promote_handler))
        .route("/api/model-registry/versions/{id}/deprecate", post(deprecate_handler))
        .route("/api/model-registry/versions/{id}", delete(delete_handler).get(get_version_handler))
        .route("/api/model-registry/versions/{id}/diff", post(diff_handler))
        .route("/api/model-registry/versions/{id}/metrics", post(submit_metrics_handler))
        .route("/api/model-registry/versions", get(list_versions_handler))
        .route("/api/model-registry/versions/rollback-config", get(get_rollback_config_handler).put(update_rollback_config_handler))
        .route("/api/model-registry/versions/health", get(health_summary_handler))
        .route("/api/model-registry/models/{name}/active", get(get_active_handler))
        .route("/api/model-registry/models/{name}/rollback", post(rollback_handler))
        .route("/api/model-registry/models/{name}/history", get(history_handler))
        .with_state(state)
}

// ============================================================================
// API Handlers
// ============================================================================

/// POST /api/model-registry/versions/register
async fn register_handler(
    State(state): State<AppState>,
    Json(req): Json<RegisterVersionRequest>,
) -> Response {
    let registry = match &state.extended_model_registry {
        Some(r) => r,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
            "error": "model registry not enabled"
        }))).into_response(),
    };

    let created_by = if req.created_by.is_empty() { "anonymous".into() } else { req.created_by };

    match registry.register_version(
        req.model_name,
        req.version,
        req.artifact_path,
        req.checksum,
        created_by,
        req.metadata,
    ) {
        Ok(version) => (StatusCode::CREATED, Json(RegisterVersionResponse {
            success: true,
            version,
        })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// POST /api/model-registry/versions/{id}/promote
async fn promote_handler(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(req): Json<PromoteVersionRequest>,
) -> Response {
    let registry = match &state.extended_model_registry {
        Some(r) => r,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
            "error": "model registry not enabled"
        }))).into_response(),
    };

    match registry.promote_version(id, req.canary_percentage) {
        Ok(version) => (StatusCode::OK, Json(PromoteVersionResponse {
            success: true,
            message: format!("Version {} promoted to active", version.version),
            version,
        })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// POST /api/model-registry/versions/{id}/deprecate
async fn deprecate_handler(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Response {
    let registry = match &state.extended_model_registry {
        Some(r) => r,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
            "error": "model registry not enabled"
        }))).into_response(),
    };

    match registry.deprecate_version(id) {
        Ok(version) => (StatusCode::OK, Json(DeprecateVersionResponse {
            success: true,
            version,
        })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// DELETE /api/model-registry/versions/{id}
async fn delete_handler(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Response {
    let registry = match &state.extended_model_registry {
        Some(r) => r,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
            "error": "model registry not enabled"
        }))).into_response(),
    };

    match registry.delete_version(id) {
        Ok(version) => (StatusCode::OK, Json(DeleteVersionResponse {
            success: true,
            version,
        })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// GET /api/model-registry/versions
async fn list_versions_handler(
    State(state): State<AppState>,
    Query(query): Query<ListVersionsQuery>,
) -> Response {
    let registry = match &state.extended_model_registry {
        Some(r) => r,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
            "error": "model registry not enabled"
        }))).into_response(),
    };

    let status = query.status.as_deref().and_then(|s| s.parse().ok());
    let versions = registry.list_versions(
        query.model.as_deref(),
        status,
        query.limit.unwrap_or(50),
        query.offset.unwrap_or(0),
    );

    Json(versions).into_response()
}

/// GET /api/model-registry/versions/{id}
async fn get_version_handler(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Response {
    let registry = match &state.extended_model_registry {
        Some(r) => r,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
            "error": "model registry not enabled"
        }))).into_response(),
    };

    match registry.get_version(id) {
        Some(detail) => Json(detail).into_response(),
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": format!("version {} not found", id)
        }))).into_response(),
    }
}

/// GET /api/model-registry/models/{name}/active
async fn get_active_handler(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Response {
    let registry = match &state.extended_model_registry {
        Some(r) => r,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
            "error": "model registry not enabled"
        }))).into_response(),
    };

    match registry.get_active_version(&name) {
        Some(version) => Json(version).into_response(),
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": format!("no active version for model '{}'", name)
        }))).into_response(),
    }
}

/// POST /api/model-registry/models/{name}/rollback
async fn rollback_handler(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(req): Json<RollbackRequest>,
) -> Response {
    let registry = match &state.extended_model_registry {
        Some(r) => r,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
            "error": "model registry not enabled"
        }))).into_response(),
    };

    let reason = req.reason.unwrap_or_else(|| "manual rollback".into());

    let result = if let Some(target_id) = req.target_version_id {
        registry.rollback_to_version(&name, target_id, &reason)
    } else {
        registry.rollback(&name, &reason)
    };

    match result {
        Ok(event) => (StatusCode::OK, Json(RollbackResponse {
            success: true,
            message: format!("Rolled back to version {}", event.to_version_id),
            event,
        })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// GET /api/model-registry/models/{name}/history
async fn history_handler(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Response {
    let registry = match &state.extended_model_registry {
        Some(r) => r,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
            "error": "model registry not enabled"
        }))).into_response(),
    };

    match registry.get_history(&name) {
        Some(history) => Json(history).into_response(),
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": format!("model '{}' not found", name)
        }))).into_response(),
    }
}

/// POST /api/model-registry/versions/{id}/diff
async fn diff_handler(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(req): Json<DiffRequest>,
) -> Response {
    let registry = match &state.extended_model_registry {
        Some(r) => r,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
            "error": "model registry not enabled"
        }))).into_response(),
    };

    match registry.diff_versions(id, req.other_version_id) {
        Ok(diff) => Json(diff).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

/// GET /api/model-registry/versions/rollback-config
async fn get_rollback_config_handler(
    State(state): State<AppState>,
) -> Response {
    let registry = match &state.extended_model_registry {
        Some(r) => r,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
            "error": "model registry not enabled"
        }))).into_response(),
    };

    Json(registry.get_rollback_config()).into_response()
}

/// PUT /api/model-registry/versions/rollback-config
async fn update_rollback_config_handler(
    State(state): State<AppState>,
    Json(config): Json<RollbackConfig>,
) -> Response {
    let registry = match &state.extended_model_registry {
        Some(r) => r,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
            "error": "model registry not enabled"
        }))).into_response(),
    };

    registry.set_rollback_config(config);
    (StatusCode::OK, Json(serde_json::json!({"success": true}))).into_response()
}

/// GET /api/model-registry/versions/health
async fn health_summary_handler(
    State(state): State<AppState>,
) -> Response {
    let registry = match &state.extended_model_registry {
        Some(r) => r,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
            "error": "model registry not enabled"
        }))).into_response(),
    };

    Json(registry.health_summary()).into_response()
}

/// POST /api/model-registry/versions/{id}/metrics
async fn submit_metrics_handler(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(req): Json<SubmitMetricsRequest>,
) -> Response {
    let registry = match &state.extended_model_registry {
        Some(r) => r,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
            "error": "model registry not enabled"
        }))).into_response(),
    };

    match registry.submit_metrics(id, req.error_count, req.latency_p95_ms) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"success": true}))).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn make_registry() -> ModelRegistry {
        ModelRegistry::new()
    }

    fn register_test_version(
        reg: &ModelRegistry,
        model: &str,
        version: &str,
        created_by: &str,
    ) -> u64 {
        reg.register_version(
            model.into(),
            version.into(),
            format!("/tmp/models/{}/{}", model, version),
            format!("a").repeat(64), // valid hex checksum placeholder
            created_by.into(),
            serde_json::json!({"test": true}),
        )
        .unwrap()
        .version_id
    }

    // ------------------------------------------------------------------
    // Version registration tests
    // ------------------------------------------------------------------

    #[test]
    fn test_register_version_success() {
        let reg = make_registry();
        let mv = reg.register_version(
            "llama-3.1-8b".into(),
            "1.0.0".into(),
            "/tmp/model.bin".into(),
            "a".repeat(64),
            "admin".into(),
            serde_json::json!({}),
        ).unwrap();

        assert_eq!(mv.model_name, "llama-3.1-8b");
        assert_eq!(mv.version, "1.0.0");
        assert_eq!(mv.status, VersionStatus::Testing);
        assert!(mv.version_id > 0);
    }

    #[test]
    fn test_register_version_invalid_semver() {
        let reg = make_registry();
        let result = reg.register_version(
            "test".into(), "not-semver".into(),
            "/tmp/x".into(), "a".repeat(64), "admin".into(), serde_json::json!({}),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_register_version_invalid_checksum_too_short() {
        let reg = make_registry();
        let result = reg.register_version(
            "test".into(), "1.0.0".into(),
            "/tmp/x".into(), "abc".into(), "admin".into(), serde_json::json!({}),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_register_version_invalid_checksum_not_hex() {
        let reg = make_registry();
        let result = reg.register_version(
            "test".into(), "1.0.0".into(),
            "/tmp/x".into(), "z".repeat(64), "admin".into(), serde_json::json!({}),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_register_multiple_versions_sequential_ids() {
        let reg = make_registry();
        let id1 = register_test_version(&reg, "model-a", "1.0.0", "admin");
        let id2 = register_test_version(&reg, "model-a", "1.1.0", "admin");
        let id3 = register_test_version(&reg, "model-b", "1.0.0", "admin");

        assert!(id2 > id1);
        assert!(id3 > id2);
    }

    // ------------------------------------------------------------------
    // Promotion tests
    // ------------------------------------------------------------------

    #[test]
    fn test_promote_version_success() {
        let reg = make_registry();
        let id = register_test_version(&reg, "model-a", "1.0.0", "admin");

        let mv = reg.promote_version(id, None).unwrap();
        assert_eq!(mv.status, VersionStatus::Active);

        let active = reg.get_active_version("model-a").unwrap();
        assert_eq!(active.version_id, id);
    }

    #[test]
    fn test_promote_with_canary() {
        let reg = make_registry();
        let id1 = register_test_version(&reg, "model-a", "1.0.0", "admin");
        let id2 = register_test_version(&reg, "model-a", "1.1.0", "admin");

        // Promote v1 first
        reg.promote_version(id1, None).unwrap();

        // Promote v2 with 10% canary
        let mv = reg.promote_version(id2, Some(10.0)).unwrap();
        assert_eq!(mv.status, VersionStatus::Active);

        let detail = reg.get_version(id2).unwrap();
        assert_eq!(detail.canary_percentage, 10.0);

        // Old version should still be active since canary < 100%
        let old = reg.get_version(id1).unwrap();
        assert_eq!(old.version.status, VersionStatus::Active);
    }

    #[test]
    fn test_promote_fails_if_not_testing() {
        let reg = make_registry();
        let id = register_test_version(&reg, "model-a", "1.0.0", "admin");

        reg.promote_version(id, None).unwrap();

        // Already active, trying again should fail
        let result = reg.promote_version(id, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_promote_demotes_previous_active() {
        let reg = make_registry();
        let id1 = register_test_version(&reg, "model-a", "1.0.0", "admin");
        let id2 = register_test_version(&reg, "model-a", "1.1.0", "admin");

        reg.promote_version(id1, None).unwrap();
        reg.promote_version(id2, None).unwrap();

        let old = reg.get_version(id1).unwrap();
        assert_eq!(old.version.status, VersionStatus::Deprecated);
    }

    // ------------------------------------------------------------------
    // Deprecation tests
    // ------------------------------------------------------------------

    #[test]
    fn test_deprecate_version() {
        let reg = make_registry();
        let id = register_test_version(&reg, "model-a", "1.0.0", "admin");

        let mv = reg.deprecate_version(id).unwrap();
        assert_eq!(mv.status, VersionStatus::Deprecated);
    }

    #[test]
    fn test_deprecate_active_version_fails() {
        let reg = make_registry();
        let id = register_test_version(&reg, "model-a", "1.0.0", "admin");
        reg.promote_version(id, None).unwrap();

        let result = reg.deprecate_version(id);
        assert!(result.is_err());
    }

    #[test]
    fn test_deprecate_already_deprecated_fails() {
        let reg = make_registry();
        let id = register_test_version(&reg, "model-a", "1.0.0", "admin");
        reg.deprecate_version(id).unwrap();

        let result = reg.deprecate_version(id);
        assert!(result.is_err());
    }

    // ------------------------------------------------------------------
    // Soft delete tests
    // ------------------------------------------------------------------

    #[test]
    fn test_delete_version() {
        let reg = make_registry();
        let id = register_test_version(&reg, "model-a", "1.0.0", "admin");

        let mv = reg.delete_version(id).unwrap();
        assert_eq!(mv.status, VersionStatus::Deleted);

        // Version info should still be accessible
        let detail = reg.get_version(id);
        assert!(detail.is_some());
        assert_eq!(detail.unwrap().version.status, VersionStatus::Deleted);
    }

    #[test]
    fn test_delete_active_version_fails() {
        let reg = make_registry();
        let id = register_test_version(&reg, "model-a", "1.0.0", "admin");
        reg.promote_version(id, None).unwrap();

        let result = reg.delete_version(id);
        assert!(result.is_err());
    }

    // ------------------------------------------------------------------
    // Semver tests
    // ------------------------------------------------------------------

    #[test]
    fn test_semver_parse() {
        let sv = SemVer::parse("1.2.3").unwrap();
        assert_eq!(sv.major, 1);
        assert_eq!(sv.minor, 2);
        assert_eq!(sv.patch, 3);
    }

    #[test]
    fn test_semver_parse_with_v_prefix() {
        let sv = SemVer::parse("v2.10.0").unwrap();
        assert_eq!(sv.major, 2);
        assert_eq!(sv.minor, 10);
        assert_eq!(sv.patch, 0);
    }

    #[test]
    fn test_semver_parse_invalid() {
        assert!(SemVer::parse("1.2").is_err());
        assert!(SemVer::parse("abc").is_err());
        assert!(SemVer::parse("1.2.3.4").is_err());
    }

    #[test]
    fn test_semver_caret_range() {
        let sv = SemVer::parse("1.2.3").unwrap();
        assert!(sv.satisfies("^1.2.3"));   // 1.2.3 == 1.2.3
        assert!(sv.satisfies("^1.0.0"));   // 1.2.3 >= 1.0.0 and < 2.0.0
        assert!(!sv.satisfies("^2.0.0"));  // 1.2.3 < 2.0.0
    }

    #[test]
    fn test_semver_tilde_range() {
        let sv = SemVer::parse("1.2.3").unwrap();
        assert!(sv.satisfies("~1.2.0"));   // 1.2.3 >= 1.2.0 and < 1.3.0
        assert!(!sv.satisfies("~1.3.0"));  // 1.2.3 < 1.3.0 boundary
    }

    #[test]
    fn test_semver_gte_range() {
        let sv = SemVer::parse("1.2.3").unwrap();
        assert!(sv.satisfies(">=1.0.0"));
        assert!(sv.satisfies(">=1.2.3"));
        assert!(!sv.satisfies(">=1.2.4"));
    }

    #[test]
    fn test_semver_exact_match() {
        let sv = SemVer::parse("1.2.3").unwrap();
        assert!(sv.satisfies("1.2.3"));
        assert!(!sv.satisfies("1.2.4"));
    }

    #[test]
    fn test_semver_ordering() {
        let a = SemVer::parse("1.2.3").unwrap();
        let b = SemVer::parse("1.10.0").unwrap();
        let c = SemVer::parse("2.0.0").unwrap();
        assert!(a < b);
        assert!(b < c);
        assert!(a < c);
    }

    // ------------------------------------------------------------------
    // Checksum tests
    // ------------------------------------------------------------------

    #[test]
    fn test_compute_checksum() {
        // Write a temp file
        let tmp = std::env::temp_dir().join("model_registry_test_checksum.bin");
        std::fs::write(&tmp, b"hello world").unwrap();

        let checksum = ModelRegistry::compute_checksum(tmp.to_str().unwrap()).unwrap();
        // SHA-256 of "hello world"
        assert_eq!(checksum, "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9");

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_verify_checksum_valid() {
        let reg = make_registry();
        let tmp = std::env::temp_dir().join("model_registry_test_verify.bin");
        std::fs::write(&tmp, b"test data").unwrap();

        let expected = ModelRegistry::compute_checksum(tmp.to_str().unwrap()).unwrap();
        let id = reg.register_version(
            "model-a".into(), "1.0.0".into(),
            tmp.to_str().unwrap().into(),
            expected, "admin".into(), serde_json::json!({}),
        ).unwrap().version_id;

        assert!(reg.verify_checksum(id).unwrap());

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_verify_checksum_invalid() {
        let reg = make_registry();
        let tmp = std::env::temp_dir().join("model_registry_test_verify_bad.bin");
        std::fs::write(&tmp, b"test data").unwrap();

        let id = reg.register_version(
            "model-a".into(), "1.0.0".into(),
            tmp.to_str().unwrap().into(),
            "b".repeat(64), // wrong checksum
            "admin".into(), serde_json::json!({}),
        ).unwrap().version_id;

        assert!(!reg.verify_checksum(id).unwrap());

        let _ = std::fs::remove_file(&tmp);
    }

    // ------------------------------------------------------------------
    // Rollback tests
    // ------------------------------------------------------------------

    #[test]
    fn test_manual_rollback() {
        let reg = make_registry();
        let id1 = register_test_version(&reg, "model-a", "1.0.0", "admin");
        let id2 = register_test_version(&reg, "model-a", "1.1.0", "admin");

        reg.promote_version(id1, None).unwrap();
        reg.promote_version(id2, None).unwrap();

        let event = reg.rollback("model-a", "performance degradation").unwrap();
        assert_eq!(event.from_version_id, id2);
        assert_eq!(event.to_version_id, id1);
        assert!(!event.automatic);

        let active = reg.get_active_version("model-a").unwrap();
        assert_eq!(active.version_id, id1);
    }

    #[test]
    fn test_rollback_to_specific_version() {
        let reg = make_registry();
        let id1 = register_test_version(&reg, "model-a", "1.0.0", "admin");
        let id2 = register_test_version(&reg, "model-a", "1.1.0", "admin");
        let id3 = register_test_version(&reg, "model-a", "1.2.0", "admin");

        reg.promote_version(id1, None).unwrap();
        reg.promote_version(id2, None).unwrap();
        reg.promote_version(id3, None).unwrap();

        let event = reg.rollback_to_version("model-a", id1, "skip v1.1.0").unwrap();
        assert_eq!(event.to_version_id, id1);
    }

    #[test]
    fn test_rollback_records_history() {
        let reg = make_registry();
        let id1 = register_test_version(&reg, "model-a", "1.0.0", "admin");
        let id2 = register_test_version(&reg, "model-a", "1.1.0", "admin");

        reg.promote_version(id1, None).unwrap();
        reg.promote_version(id2, None).unwrap();
        reg.rollback("model-a", "test").unwrap();

        let history = reg.get_history("model-a").unwrap();
        assert_eq!(history.rollback_history.len(), 1);
        assert_eq!(history.rollback_history[0].reason, "test");
    }

    #[test]
    fn test_rollback_no_previous_version_fails() {
        let reg = make_registry();
        let id = register_test_version(&reg, "model-a", "1.0.0", "admin");
        reg.promote_version(id, None).unwrap();

        let result = reg.rollback("model-a", "test");
        assert!(result.is_err());
    }

    // ------------------------------------------------------------------
    // Auto-rollback (error rate) tests
    // ------------------------------------------------------------------

    #[test]
    fn test_auto_rollback_error_rate() {
        let reg = make_registry();
        let config = RollbackConfig {
            error_rate_threshold: 0.05,
            min_requests_before_eval: 10,
            latency_degradation_factor: 2.0,
            cooldown_period_secs: 0, // no cooldown for testing
        };
        reg.set_rollback_config(config);

        let id1 = register_test_version(&reg, "model-a", "1.0.0", "admin");
        let id2 = register_test_version(&reg, "model-a", "1.1.0", "admin");

        reg.promote_version(id1, None).unwrap();
        reg.promote_version(id2, None).unwrap();

        // Submit 100 requests with 10% error rate (exceeds 5% threshold)
        for _ in 0..100 {
            reg.submit_metrics(id2, 1, 50).unwrap();
        }

        // Should have auto-rolled back to id1
        let active = reg.get_active_version("model-a").unwrap();
        assert_eq!(active.version_id, id1);

        let history = reg.get_history("model-a").unwrap();
        assert_eq!(history.rollback_history.len(), 1);
        assert!(history.rollback_history[0].automatic);
    }

    #[test]
    fn test_auto_rollback_latency_degradation() {
        let reg = make_registry();
        let config = RollbackConfig {
            error_rate_threshold: 1.0, // very high threshold (won't trigger on errors)
            min_requests_before_eval: 10,
            latency_degradation_factor: 2.0,
            cooldown_period_secs: 0,
        };
        reg.set_rollback_config(config);

        let id1 = register_test_version(&reg, "model-a", "1.0.0", "admin");
        let id2 = register_test_version(&reg, "model-a", "1.1.0", "admin");

        reg.promote_version(id1, None).unwrap();

        // Establish baseline for v1: 100 requests at 50ms p95
        for _ in 0..20 {
            reg.submit_metrics(id1, 0, 50).unwrap();
        }

        reg.promote_version(id2, None).unwrap();

        // Submit requests for v2 with high latency: 200ms p95 (4x degradation)
        for _ in 0..20 {
            reg.submit_metrics(id2, 0, 200).unwrap();
        }

        // Should have auto-rolled back to id1
        let active = reg.get_active_version("model-a").unwrap();
        assert_eq!(active.version_id, id1);
    }

    #[test]
    fn test_auto_rollback_cooldown_enforced() {
        let reg = make_registry();
        let config = RollbackConfig {
            error_rate_threshold: 0.05,
            min_requests_before_eval: 10,
            latency_degradation_factor: 2.0,
            cooldown_period_secs: 3600, // 1 hour cooldown
        };
        reg.set_rollback_config(config);

        let id1 = register_test_version(&reg, "model-a", "1.0.0", "admin");
        let id2 = register_test_version(&reg, "model-a", "1.1.0", "admin");

        reg.promote_version(id1, None).unwrap();
        reg.promote_version(id2, None).unwrap();

        // Trigger first auto-rollback
        for _ in 0..100 {
            reg.submit_metrics(id2, 1, 50).unwrap();
        }

        // First rollback should succeed
        let active = reg.get_active_version("model-a").unwrap();
        assert_eq!(active.version_id, id1);

        // Promote id2 again
        // First need to set it back to testing
        let mut info = reg.versions.get_mut(&id2).unwrap();
        info.version.status = VersionStatus::Testing;
        drop(info);
        reg.promote_version(id2, None).unwrap();

        // Submit more bad metrics - should NOT rollback due to cooldown
        for _ in 0..100 {
            reg.submit_metrics(id2, 1, 50).unwrap();
        }

        // Should still be id2 because cooldown prevented second rollback
        let active = reg.get_active_version("model-a").unwrap();
        assert_eq!(active.version_id, id2);
    }

    // ------------------------------------------------------------------
    // Version listing/filtering tests
    // ------------------------------------------------------------------

    #[test]
    fn test_list_versions_filter_by_model() {
        let reg = make_registry();
        register_test_version(&reg, "model-a", "1.0.0", "admin");
        register_test_version(&reg, "model-a", "1.1.0", "admin");
        register_test_version(&reg, "model-b", "1.0.0", "admin");

        let versions = reg.list_versions(Some("model-a"), None, 100, 0);
        assert_eq!(versions.len(), 2);
        assert!(versions.iter().all(|v| v.model_name == "model-a"));
    }

    #[test]
    fn test_list_versions_filter_by_status() {
        let reg = make_registry();
        let id1 = register_test_version(&reg, "model-a", "1.0.0", "admin");
        register_test_version(&reg, "model-a", "1.1.0", "admin");

        reg.promote_version(id1, None).unwrap();

        let active = reg.list_versions(None, Some(VersionStatus::Active), 100, 0);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].version_id, id1);

        let testing = reg.list_versions(None, Some(VersionStatus::Testing), 100, 0);
        assert_eq!(testing.len(), 1);
    }

    #[test]
    fn test_list_versions_pagination() {
        let reg = make_registry();
        for i in 0..10 {
            register_test_version(&reg, "model-a", &format!("1.{}.0", i), "admin");
        }

        let page1 = reg.list_versions(Some("model-a"), None, 3, 0);
        assert_eq!(page1.len(), 3);

        let page2 = reg.list_versions(Some("model-a"), None, 3, 3);
        assert_eq!(page2.len(), 3);

        // Pages should not overlap (sorted by ID desc)
        let page1_ids: Vec<u64> = page1.iter().map(|v| v.version_id).collect();
        let page2_ids: Vec<u64> = page2.iter().map(|v| v.version_id).collect();
        assert!(!page1_ids.iter().any(|id| page2_ids.contains(id)));
    }

    // ------------------------------------------------------------------
    // Diff tests
    // ------------------------------------------------------------------

    #[test]
    fn test_diff_same_version() {
        let reg = make_registry();
        let id = register_test_version(&reg, "model-a", "1.0.0", "admin");

        let diff = reg.diff_versions(id, id).unwrap();
        assert!(diff.same_model);
        assert!(diff.same_checksum);
        assert_eq!(diff.semver_comparison, "equal");
        assert!(!diff.metadata_differ);
    }

    #[test]
    fn test_diff_different_versions() {
        let reg = make_registry();
        let id1 = reg.register_version(
            "model-a".into(), "1.0.0".into(),
            "/tmp/v1.bin".into(), "a".repeat(64),
            "admin".into(), serde_json::json!({"key1": "val1"}),
        ).unwrap().version_id;

        let id2 = reg.register_version(
            "model-a".into(), "2.0.0".into(),
            "/tmp/v2.bin".into(), "b".repeat(64),
            "admin".into(), serde_json::json!({"key2": "val2"}),
        ).unwrap().version_id;

        let diff = reg.diff_versions(id1, id2).unwrap();
        assert!(diff.same_model);
        assert!(!diff.same_checksum);
        assert_eq!(diff.semver_comparison, "older");
        assert!(diff.metadata_differ);
        assert!(diff.a_only_metadata_keys.contains(&"key1".to_string()));
        assert!(diff.b_only_metadata_keys.contains(&"key2".to_string()));
    }

    // ------------------------------------------------------------------
    // History tests
    // ------------------------------------------------------------------

    #[test]
    fn test_history_includes_all_versions() {
        let reg = make_registry();
        register_test_version(&reg, "model-a", "1.0.0", "admin");
        register_test_version(&reg, "model-a", "1.1.0", "admin");
        register_test_version(&reg, "model-a", "1.2.0", "admin");

        let history = reg.get_history("model-a").unwrap();
        assert_eq!(history.versions.len(), 3);
    }

    #[test]
    fn test_history_nonexistent_model() {
        let reg = make_registry();
        let history = reg.get_history("nonexistent");
        assert!(history.is_none());
    }

    // ------------------------------------------------------------------
    // Metrics tests
    // ------------------------------------------------------------------

    #[test]
    fn test_submit_metrics_accumulates() {
        let reg = make_registry();
        let id = register_test_version(&reg, "model-a", "1.0.0", "admin");

        for _ in 0..10 {
            reg.submit_metrics(id, 0, 50).unwrap();
        }

        let detail = reg.get_version(id).unwrap();
        assert_eq!(detail.metrics.request_count, 10);
        assert_eq!(detail.metrics.error_count, 0);
        assert_eq!(detail.metrics.total_latency_p95_ms, 500);
        assert_eq!(detail.metrics.avg_latency_p95(), 50.0);
    }

    #[test]
    fn test_submit_metrics_error_counting() {
        let reg = make_registry();
        let id = register_test_version(&reg, "model-a", "1.0.0", "admin");

        reg.submit_metrics(id, 1, 50).unwrap();
        reg.submit_metrics(id, 0, 50).unwrap();
        reg.submit_metrics(id, 1, 50).unwrap();

        let detail = reg.get_version(id).unwrap();
        assert_eq!(detail.metrics.request_count, 3);
        assert_eq!(detail.metrics.error_count, 2);
        assert!((detail.metrics.error_rate() - 2.0 / 3.0).abs() < 0.001);
    }

    // ------------------------------------------------------------------
    // Health assessment tests
    // ------------------------------------------------------------------

    #[test]
    fn test_health_assessment_healthy() {
        let reg = make_registry();
        let id = register_test_version(&reg, "model-a", "1.0.0", "admin");
        reg.promote_version(id, None).unwrap();

        for _ in 0..200 {
            reg.submit_metrics(id, 0, 50).unwrap();
        }

        let assessment = reg.evaluate_health(id).unwrap();
        assert!(assessment.healthy);
    }

    #[test]
    fn test_health_assessment_unhealthy() {
        let reg = make_registry();
        let config = RollbackConfig {
            error_rate_threshold: 0.05,
            min_requests_before_eval: 10,
            ..Default::default()
        };
        reg.set_rollback_config(config);

        let id = register_test_version(&reg, "model-a", "1.0.0", "admin");
        reg.promote_version(id, None).unwrap();

        for _ in 0..100 {
            reg.submit_metrics(id, 1, 50).unwrap(); // 10% error rate
        }

        let assessment = reg.evaluate_health(id).unwrap();
        assert!(!assessment.healthy);
    }

    // ------------------------------------------------------------------
    // Rollback config tests
    // ------------------------------------------------------------------

    #[test]
    fn test_get_and_set_rollback_config() {
        let reg = make_registry();
        let default = reg.get_rollback_config();
        assert_eq!(default.error_rate_threshold, 0.05);
        assert_eq!(default.min_requests_before_eval, 100);

        let custom = RollbackConfig {
            error_rate_threshold: 0.10,
            min_requests_before_eval: 50,
            latency_degradation_factor: 3.0,
            cooldown_period_secs: 600,
        };
        reg.set_rollback_config(custom.clone());

        let retrieved = reg.get_rollback_config();
        assert_eq!(retrieved.error_rate_threshold, 0.10);
        assert_eq!(retrieved.min_requests_before_eval, 50);
        assert_eq!(retrieved.latency_degradation_factor, 3.0);
        assert_eq!(retrieved.cooldown_period_secs, 600);
    }

    // ------------------------------------------------------------------
    // Concurrent access tests
    // ------------------------------------------------------------------

    #[test]
    fn test_concurrent_registrations() {
        use std::thread;

        let reg = Arc::new(make_registry());
        let mut handles = Vec::new();

        for i in 0..10 {
            let reg = reg.clone();
            handles.push(thread::spawn(move || {
                for j in 0..10 {
                    let _ = reg.register_version(
                        format!("model-{}", i),
                        format!("1.{}.0", j),
                        "/tmp/x".into(),
                        "a".repeat(64),
                        "admin".into(),
                        serde_json::json!({}),
                    );
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // Should have 100 versions total
        let all = reg.list_versions(None, None, 1000, 0);
        assert_eq!(all.len(), 100);

        // 10 different models
        let models = reg.list_models();
        assert_eq!(models.len(), 10);
    }

    #[test]
    fn test_concurrent_metrics_submission() {
        use std::thread;

        let reg = Arc::new(make_registry());
        let id = Arc::new(register_test_version(&reg, "model-a", "1.0.0", "admin"));

        let mut handles = Vec::new();
        for _ in 0..10 {
            let reg = reg.clone();
            let id = id.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    let _ = reg.submit_metrics(*id, 0, 50);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let detail = reg.get_version(*id).unwrap();
        assert_eq!(detail.metrics.request_count, 1000);
    }

    // ------------------------------------------------------------------
    // Edge case tests
    // ------------------------------------------------------------------

    #[test]
    fn test_get_nonexistent_version() {
        let reg = make_registry();
        assert!(reg.get_version(99999).is_none());
    }

    #[test]
    fn test_get_active_no_versions() {
        let reg = make_registry();
        assert!(reg.get_active_version("nonexistent").is_none());
    }

    #[test]
    fn test_list_models_empty() {
        let reg = make_registry();
        assert!(reg.list_models().is_empty());
    }

    #[test]
    fn test_rollback_same_version_fails() {
        let reg = make_registry();
        let id = register_test_version(&reg, "model-a", "1.0.0", "admin");
        reg.promote_version(id, None).unwrap();

        let result = reg.rollback_to_version("model-a", id, "already active");
        assert!(result.is_err());
    }

    #[test]
    fn test_version_status_display() {
        assert_eq!(VersionStatus::Active.to_string(), "active");
        assert_eq!(VersionStatus::Testing.to_string(), "testing");
        assert_eq!(VersionStatus::Deprecated.to_string(), "deprecated");
        assert_eq!(VersionStatus::Deleted.to_string(), "deleted");
    }

    #[test]
    fn test_version_status_from_str() {
        assert_eq!("active".parse::<VersionStatus>().unwrap(), VersionStatus::Active);
        assert_eq!("TESTING".parse::<VersionStatus>().unwrap(), VersionStatus::Testing);
        assert!("invalid".parse::<VersionStatus>().is_err());
    }
}
