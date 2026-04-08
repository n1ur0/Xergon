//! Settlement Finality Tracker
//!
//! Tracks settlement confirmations on the Ergo blockchain, detects rollbacks,
//! manages settlement audit trails, and handles timeouts.
//!
//! Features:
//! - Full settlement lifecycle: Pending -> Submitted -> Confirming -> Confirmed -> Finalized
//! - Rollback detection with competing transaction tracking
//! - Timeout management for stale settlements
//! - Comprehensive audit trail for all state transitions
//! - Batch finality checking
//! - Provider-scoped settlement queries
//!
//! REST endpoints:
//! - POST /v1/settlement-finality/create           — Create settlement
//! - POST /v1/settlement-finality/confirmations/{id} — Update confirmations
//! - POST /v1/settlement-finality/check/{id}        — Check finality
//! - POST /v1/settlement-finality/rollback/{id}     — Mark as rolled back
//! - GET  /v1/settlement-finality/{id}               — Get settlement details
//! - GET  /v1/settlement-finality/{id}/audit         — Get audit trail
//! - GET  /v1/settlement-finality                    — List settlements
//! - GET  /v1/settlement-finality/summary            — Get summary
//! - GET  /v1/settlement-finality/pending            — Get pending confirmations
//! - POST /v1/settlement-finality/batch-check        — Batch finality check

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

// ================================================================
// Types
// ================================================================

/// Status of a settlement in the finality tracking pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SettlementStatus {
    /// Settlement created but not yet submitted to the chain.
    Pending,
    /// Transaction submitted to the Ergo node.
    Submitted,
    /// Receiving block confirmations.
    Confirming,
    /// Reached required confirmation count.
    Confirmed,
    /// Reached finality safety margin (2x required confirmations).
    Finalized,
    /// Exceeded timeout blocks without finality.
    TimedOut,
    /// Competing transaction detected at same height.
    RolledBack,
    /// Settlement failed for another reason.
    Failed,
}

impl std::fmt::Display for SettlementStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SettlementStatus::Pending => write!(f, "pending"),
            SettlementStatus::Submitted => write!(f, "submitted"),
            SettlementStatus::Confirming => write!(f, "confirming"),
            SettlementStatus::Confirmed => write!(f, "confirmed"),
            SettlementStatus::Finalized => write!(f, "finalized"),
            SettlementStatus::TimedOut => write!(f, "timed_out"),
            SettlementStatus::RolledBack => write!(f, "rolled_back"),
            SettlementStatus::Failed => write!(f, "failed"),
        }
    }
}

/// A settlement record tracked through the finality pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementRecord {
    /// Unique settlement identifier.
    pub id: String,
    /// Ergo transaction ID.
    pub tx_id: String,
    /// Provider that initiated the settlement.
    pub provider_id: String,
    /// Request ID that triggered the settlement.
    pub request_id: String,
    /// Settlement amount in nanoERG.
    pub amount: u64,
    /// Optional token ID for token-based settlements.
    pub token_id: Option<String>,
    /// Current settlement status.
    pub status: SettlementStatus,
    /// Current block confirmations received.
    pub confirmations: u32,
    /// Required confirmations for "Confirmed" status.
    pub required_confirmations: u32,
    /// Timestamp (epoch millis) when settlement was created.
    pub submitted_at: i64,
    /// Timestamp (epoch millis) when settlement reached "Confirmed".
    pub confirmed_at: Option<i64>,
    /// Timestamp (epoch millis) when settlement reached "Finalized".
    pub finalized_at: Option<i64>,
    /// Timestamp (epoch millis) when a rollback was detected.
    pub rollback_at: Option<i64>,
    /// Block height at which the settlement was included.
    pub block_height: u64,
}

impl SettlementRecord {
    /// Check if the settlement is in a terminal (non-transitioning) state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            SettlementStatus::Finalized
                | SettlementStatus::TimedOut
                | SettlementStatus::RolledBack
                | SettlementStatus::Failed
        )
    }
}

/// Configuration for settlement finality checking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalityConfig {
    /// Required confirmations before marking as "Confirmed" (default: 30 for Ergo).
    pub required_confirmations: u32,
    /// Maximum blocks before a settlement is timed out.
    pub timeout_blocks: u32,
    /// Whether to automatically check finality on confirmation updates.
    pub auto_check: bool,
    /// Interval in milliseconds between automatic checks.
    pub check_interval_ms: u64,
}

impl Default for FinalityConfig {
    fn default() -> Self {
        Self {
            required_confirmations: 30,
            timeout_blocks: 720,
            auto_check: true,
            check_interval_ms: 60_000,
        }
    }
}

/// A rollback event recorded when a competing transaction is detected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackEvent {
    /// Settlement ID that was rolled back.
    pub settlement_id: String,
    /// Original transaction ID.
    pub original_tx_id: String,
    /// Block height where the original tx was included.
    pub original_height: u64,
    /// Block height where the rollback was detected.
    pub rollback_height: u64,
    /// Competing transaction ID, if known.
    pub competing_tx_id: Option<String>,
    /// Timestamp (epoch millis) when rollback was detected.
    pub detected_at: i64,
}

/// Audit event types for settlement state transitions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AuditEvent {
    /// Settlement record created.
    Created,
    /// Transaction submitted to the node.
    Submitted,
    /// A block confirmation was received.
    ConfirmationReceived,
    /// Settlement reached finality.
    Finalized,
    /// Settlement timed out.
    TimedOut,
    /// Rollback was detected.
    RollbackDetected,
    /// Rollback was processed and settlement updated.
    RollbackProcessed,
    /// Manual status override by an operator.
    ManualOverride,
    /// Dispute opened on this settlement.
    DisputeOpened,
}

impl std::fmt::Display for AuditEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuditEvent::Created => write!(f, "created"),
            AuditEvent::Submitted => write!(f, "submitted"),
            AuditEvent::ConfirmationReceived => write!(f, "confirmation_received"),
            AuditEvent::Finalized => write!(f, "finalized"),
            AuditEvent::TimedOut => write!(f, "timed_out"),
            AuditEvent::RollbackDetected => write!(f, "rollback_detected"),
            AuditEvent::RollbackProcessed => write!(f, "rollback_processed"),
            AuditEvent::ManualOverride => write!(f, "manual_override"),
            AuditEvent::DisputeOpened => write!(f, "dispute_opened"),
        }
    }
}

/// An entry in the settlement audit trail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementAuditEntry {
    /// Unique audit entry identifier.
    pub id: String,
    /// Settlement this audit entry belongs to.
    pub settlement_id: String,
    /// Type of audit event.
    pub event: AuditEvent,
    /// Human-readable details about the event.
    pub details: String,
    /// Timestamp (epoch millis) when the event occurred.
    pub timestamp: i64,
    /// Block height at the time of the event, if applicable.
    pub block_height: Option<u64>,
}

/// Aggregated summary of all settlements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementSummary {
    /// Total number of settlements.
    pub total_settlements: u64,
    /// Currently pending settlements.
    pub pending: u64,
    /// Settlements that have been confirmed.
    pub confirmed: u64,
    /// Settlements that have been finalized.
    pub finalized: u64,
    /// Settlements that were rolled back.
    pub rolled_back: u64,
    /// Settlements that timed out.
    pub timed_out: u64,
    /// Total value (nanoERG) of all finalized settlements.
    pub total_value_settled: u64,
    /// Average confirmation blocks across finalized settlements.
    pub avg_confirmation_blocks: f64,
}

/// Result of a confirmation check for a single settlement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmationCheck {
    /// Settlement ID.
    pub settlement_id: String,
    /// Current number of confirmations.
    pub current_confirmations: u32,
    /// Required confirmations for "Confirmed" status.
    pub required: u32,
    /// Whether the settlement is now in final state.
    pub is_final: bool,
    /// Suggested timestamp for the next check (epoch millis).
    pub next_check_at: i64,
}

// ================================================================
// Request / Response types for REST endpoints
// ================================================================

#[derive(Debug, Deserialize)]
pub struct CreateSettlementRequest {
    pub tx_id: String,
    pub provider_id: String,
    pub request_id: String,
    pub amount: u64,
    pub token_id: Option<String>,
    pub block_height: u64,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConfirmationsRequest {
    pub confirmations: u32,
    pub block_height: u64,
}

#[derive(Debug, Deserialize)]
pub struct RollbackRequest {
    pub competing_tx_id: Option<String>,
    pub rollback_height: u64,
}

#[derive(Debug, Deserialize)]
pub struct ListSettlementsQuery {
    pub provider_id: Option<String>,
    pub status: Option<String>,
    pub from: Option<i64>,
    pub to: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: String,
}

impl ApiError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self { error: msg.into() }
    }
}

// ================================================================
// Settlement Finality Engine
// ================================================================

/// Core engine for tracking settlement finality on the Ergo blockchain.
///
/// Manages the lifecycle of settlement confirmations, detects rollbacks,
/// and maintains an audit trail of all state transitions.
pub struct SettlementFinalityEngine {
    /// Settlement records indexed by settlement ID.
    settlements: DashMap<String, SettlementRecord>,
    /// Audit trail entries indexed by settlement ID.
    audit_trail: DashMap<String, Vec<SettlementAuditEntry>>,
    /// Rollback events indexed by settlement ID.
    rollback_events: DashMap<String, RollbackEvent>,
    /// Finality configuration.
    config: std::sync::RwLock<FinalityConfig>,
    /// Monotonic counter for total settlements created.
    total_created: AtomicU64,
    /// Monotonic counter for total rollbacks detected.
    total_rollbacks: AtomicU64,
    /// Monotonic counter for total timeouts.
    total_timeouts: AtomicU64,
}

impl SettlementFinalityEngine {
    /// Create a new finality engine with default configuration.
    pub fn new() -> Self {
        Self {
            settlements: DashMap::new(),
            audit_trail: DashMap::new(),
            rollback_events: DashMap::new(),
            config: std::sync::RwLock::new(FinalityConfig::default()),
            total_created: AtomicU64::new(0),
            total_rollbacks: AtomicU64::new(0),
            total_timeouts: AtomicU64::new(0),
        }
    }

    /// Create a new finality engine with custom configuration.
    pub fn with_config(config: FinalityConfig) -> Self {
        Self {
            settlements: DashMap::new(),
            audit_trail: DashMap::new(),
            rollback_events: DashMap::new(),
            config: std::sync::RwLock::new(config),
            total_created: AtomicU64::new(0),
            total_rollbacks: AtomicU64::new(0),
            total_timeouts: AtomicU64::new(0),
        }
    }

    // ----------------------------------------------------------------
    // Audit helpers
    // ----------------------------------------------------------------

    /// Add an audit entry for a settlement.
    fn add_audit(
        &self,
        settlement_id: &str,
        event: AuditEvent,
        details: impl Into<String>,
        block_height: Option<u64>,
    ) {
        let entry = SettlementAuditEntry {
            id: Uuid::new_v4().to_string(),
            settlement_id: settlement_id.to_string(),
            event,
            details: details.into(),
            timestamp: Utc::now().timestamp_millis(),
            block_height,
        };
        debug!(
            settlement_id = %settlement_id,
            event = %entry.event,
            "Audit trail entry"
        );
        self.audit_trail
            .entry(settlement_id.to_string())
            .or_default()
            .push(entry);
    }

    // ----------------------------------------------------------------
    // Core operations
    // ----------------------------------------------------------------

    /// Create a new settlement record in Pending state.
    pub fn create_settlement(
        &self,
        provider_id: impl Into<String>,
        request_id: impl Into<String>,
        amount: u64,
        token_id: Option<String>,
        tx_id: impl Into<String>,
        block_height: u64,
    ) -> SettlementRecord {
        let id = Uuid::new_v4().to_string();
        let config = self.config.read().unwrap().clone();
        let now = Utc::now().timestamp_millis();

        let record = SettlementRecord {
            id: id.clone(),
            tx_id: tx_id.into(),
            provider_id: provider_id.into(),
            request_id: request_id.into(),
            amount,
            token_id,
            status: SettlementStatus::Pending,
            confirmations: 0,
            required_confirmations: config.required_confirmations,
            submitted_at: now,
            confirmed_at: None,
            finalized_at: None,
            rollback_at: None,
            block_height,
        };

        info!(
            settlement_id = %id,
            tx_id = %record.tx_id,
            provider_id = %record.provider_id,
            amount = amount,
            "Created new settlement"
        );

        self.settlements.insert(id.clone(), record.clone());
        self.total_created.fetch_add(1, Ordering::Relaxed);
        self.add_audit(&id, AuditEvent::Created, "Settlement created", Some(block_height));

        record
    }

    /// Update the confirmation count for a settlement and advance state transitions.
    ///
    /// State transitions:
    /// - Pending -> Submitted (first confirmation, count >= 1)
    /// - Submitted -> Confirming (count >= 1)
    /// - Confirming -> Confirmed (count >= required_confirmations)
    /// - Confirmed -> Finalized (count >= 2 * required_confirmations)
    pub fn update_confirmations(
        &self,
        settlement_id: &str,
        confirmations: u32,
        block_height: u64,
    ) -> Result<SettlementRecord, String> {
        let mut record = self
            .settlements
            .get_mut(settlement_id)
            .ok_or_else(|| format!("Settlement {} not found", settlement_id))?
            .clone();

        if record.is_terminal() {
            return Err(format!(
                "Settlement {} is in terminal state {}",
                settlement_id, record.status
            ));
        }

        let old_status = record.status.clone();
        record.confirmations = confirmations;
        record.block_height = block_height;

        // State transitions based on confirmation count
        if confirmations == 0 {
            // No change, stay in current status
        } else if record.status == SettlementStatus::Pending {
            record.status = SettlementStatus::Submitted;
            self.add_audit(
                settlement_id,
                AuditEvent::Submitted,
                format!("Transaction submitted, first confirmation at block {}", block_height),
                Some(block_height),
            );
        }

        if record.status == SettlementStatus::Submitted && confirmations >= 1 {
            record.status = SettlementStatus::Confirming;
            self.add_audit(
                settlement_id,
                AuditEvent::ConfirmationReceived,
                format!("Now confirming: {} / {} confirmations", confirmations, record.required_confirmations),
                Some(block_height),
            );
        }

        if record.status == SettlementStatus::Confirming
            && confirmations >= record.required_confirmations
        {
            record.status = SettlementStatus::Confirmed;
            record.confirmed_at = Some(Utc::now().timestamp_millis());
            self.add_audit(
                settlement_id,
                AuditEvent::ConfirmationReceived,
                format!(
                    "Settlement confirmed: {} / {} confirmations",
                    confirmations, record.required_confirmations
                ),
                Some(block_height),
            );
        }

        if record.status == SettlementStatus::Confirmed
            && confirmations >= record.required_confirmations * 2
        {
            record.status = SettlementStatus::Finalized;
            record.finalized_at = Some(Utc::now().timestamp_millis());
            self.add_audit(
                settlement_id,
                AuditEvent::Finalized,
                format!(
                    "Settlement finalized: {} confirmations (2x required {})",
                    confirmations, record.required_confirmations
                ),
                Some(block_height),
            );
        }

        if old_status != record.status {
            info!(
                settlement_id = %settlement_id,
                old_status = %old_status,
                new_status = %record.status,
                confirmations = confirmations,
                "Settlement state transition"
            );
        }

        self.settlements.insert(settlement_id.to_string(), record.clone());
        Ok(record)
    }

    /// Check if a specific settlement has reached finality.
    ///
    /// Returns a `ConfirmationCheck` with current status and suggested next check time.
    pub fn check_finality(&self, settlement_id: &str) -> Result<ConfirmationCheck, String> {
        let record = self
            .settlements
            .get(settlement_id)
            .ok_or_else(|| format!("Settlement {} not found", settlement_id))?;

        let config = self.config.read().unwrap().clone();
        let is_final = record.is_terminal();

        let next_check_at = if is_final {
            0 // No further checks needed
        } else {
            Utc::now().timestamp_millis() + config.check_interval_ms as i64
        };

        Ok(ConfirmationCheck {
            settlement_id: settlement_id.to_string(),
            current_confirmations: record.confirmations,
            required: record.required_confirmations,
            is_final,
            next_check_at,
        })
    }

    /// Mark a settlement as rolled back due to a competing transaction.
    pub fn detect_rollback(
        &self,
        settlement_id: &str,
        competing_tx_id: Option<String>,
    ) -> Result<RollbackEvent, String> {
        let mut record = self
            .settlements
            .get_mut(settlement_id)
            .ok_or_else(|| format!("Settlement {} not found", settlement_id))?
            .clone();

        if record.is_terminal() {
            return Err(format!(
                "Settlement {} is already in terminal state {}",
                settlement_id, record.status
            ));
        }

        let now = Utc::now().timestamp_millis();
        let original_height = record.block_height;

        self.add_audit(
            settlement_id,
            AuditEvent::RollbackDetected,
            format!(
                "Rollback detected at height {}. Original tx: {}, competing tx: {:?}",
                original_height,
                record.tx_id,
                competing_tx_id
            ),
            Some(original_height),
        );

        record.status = SettlementStatus::RolledBack;
        record.rollback_at = Some(now);

        let event = RollbackEvent {
            settlement_id: settlement_id.to_string(),
            original_tx_id: record.tx_id.clone(),
            original_height,
            rollback_height: original_height,
            competing_tx_id: competing_tx_id.clone(),
            detected_at: now,
        };

        warn!(
            settlement_id = %settlement_id,
            tx_id = %record.tx_id,
            original_height = original_height,
            competing_tx_id = ?competing_tx_id,
            "Settlement rolled back"
        );

        self.rollback_events
            .insert(settlement_id.to_string(), event.clone());
        self.settlements.insert(settlement_id.to_string(), record);
        self.total_rollbacks.fetch_add(1, Ordering::Relaxed);

        self.add_audit(
            settlement_id,
            AuditEvent::RollbackProcessed,
            "Rollback processed, settlement marked as rolled back",
            Some(original_height),
        );

        Ok(event)
    }

    /// Mark a settlement as timed out if it has not reached finality within the configured block window.
    pub fn timeout_settlement(&self, settlement_id: &str) -> Result<SettlementRecord, String> {
        let mut record = self
            .settlements
            .get_mut(settlement_id)
            .ok_or_else(|| format!("Settlement {} not found", settlement_id))?
            .clone();

        if record.is_terminal() {
            return Err(format!(
                "Settlement {} is already in terminal state {}",
                settlement_id, record.status
            ));
        }

        self.add_audit(
            settlement_id,
            AuditEvent::TimedOut,
            format!(
                "Settlement timed out after {} confirmations (required {})",
                record.confirmations, record.required_confirmations
            ),
            Some(record.block_height),
        );

        record.status = SettlementStatus::TimedOut;

        warn!(
            settlement_id = %settlement_id,
            tx_id = %record.tx_id,
            confirmations = record.confirmations,
            required = record.required_confirmations,
            "Settlement timed out"
        );

        self.settlements.insert(settlement_id.to_string(), record.clone());
        self.total_timeouts.fetch_add(1, Ordering::Relaxed);
        Ok(record)
    }

    /// Get a settlement by ID.
    pub fn get_settlement(&self, id: &str) -> Option<SettlementRecord> {
        self.settlements.get(id).map(|r| r.clone())
    }

    /// List settlements with optional filters.
    pub fn list_settlements(
        &self,
        provider_id: Option<&str>,
        status: Option<&str>,
        from: Option<i64>,
        to: Option<i64>,
    ) -> Vec<SettlementRecord> {
        self.settlements
            .iter()
            .filter(|entry| {
                let r = entry.value();
                if let Some(pid) = provider_id {
                    if r.provider_id != pid {
                        return false;
                    }
                }
                if let Some(s) = status {
                    if r.status.to_string() != s {
                        return false;
                    }
                }
                if let Some(f) = from {
                    if r.submitted_at < f {
                        return false;
                    }
                }
                if let Some(t) = to {
                    if r.submitted_at > t {
                        return false;
                    }
                }
                true
            })
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Get the audit trail for a settlement.
    pub fn get_audit_trail(&self, settlement_id: &str) -> Vec<SettlementAuditEntry> {
        self.audit_trail
            .get(settlement_id)
            .map(|entries| entries.clone())
            .unwrap_or_default()
    }

    /// Get an aggregated summary of all settlements.
    pub fn get_summary(&self) -> SettlementSummary {
        let mut total = 0u64;
        let mut pending = 0u64;
        let mut confirmed = 0u64;
        let mut finalized = 0u64;
        let mut rolled_back = 0u64;
        let mut timed_out = 0u64;
        let mut total_value = 0u64;
        let mut confirmation_sum = 0f64;
        let mut finalized_count = 0u64;

        for entry in self.settlements.iter() {
            let r = entry.value();
            total += 1;
            match r.status {
                SettlementStatus::Pending => pending += 1,
                SettlementStatus::Submitted => pending += 1,
                SettlementStatus::Confirming => pending += 1,
                SettlementStatus::Confirmed => confirmed += 1,
                SettlementStatus::Finalized => {
                    finalized += 1;
                    total_value += r.amount;
                    confirmation_sum += r.confirmations as f64;
                    finalized_count += 1;
                }
                SettlementStatus::RolledBack => rolled_back += 1,
                SettlementStatus::TimedOut => timed_out += 1,
                SettlementStatus::Failed => timed_out += 1,
            }
        }

        let avg_confirmation_blocks = if finalized_count > 0 {
            confirmation_sum / finalized_count as f64
        } else {
            0.0
        };

        SettlementSummary {
            total_settlements: total,
            pending,
            confirmed,
            finalized,
            rolled_back,
            timed_out,
            total_value_settled: total_value,
            avg_confirmation_blocks,
        }
    }

    /// Get all settlements currently waiting for confirmations (non-terminal).
    pub fn get_pending_confirmations(&self) -> Vec<ConfirmationCheck> {
        let config = self.config.read().unwrap().clone();
        self.settlements
            .iter()
            .filter(|entry| !entry.value().is_terminal())
            .map(|entry| {
                let r = entry.value();
                ConfirmationCheck {
                    settlement_id: r.id.clone(),
                    current_confirmations: r.confirmations,
                    required: r.required_confirmations,
                    is_final: false,
                    next_check_at: Utc::now().timestamp_millis() + config.check_interval_ms as i64,
                }
            })
            .collect()
    }

    /// Get the current finality configuration.
    pub fn get_config(&self) -> FinalityConfig {
        self.config.read().unwrap().clone()
    }

    /// Update the finality configuration.
    pub fn update_config(&self, config: FinalityConfig) {
        info!(
            required_confirmations = config.required_confirmations,
            timeout_blocks = config.timeout_blocks,
            auto_check = config.auto_check,
            "Finality config updated"
        );
        *self.config.write().unwrap() = config;
    }

    /// Batch-check finality for all non-terminal settlements.
    ///
    /// Returns a list of `ConfirmationCheck` results for each pending settlement.
    pub fn batch_check_finality(&self) -> Vec<ConfirmationCheck> {
        let pending = self.get_pending_confirmations();
        info!(count = pending.len(), "Batch finality check");
        pending
    }

    /// Get all settlements for a specific provider.
    pub fn get_provider_settlements(&self, provider_id: &str) -> Vec<SettlementRecord> {
        self.settlements
            .iter()
            .filter(|entry| entry.value().provider_id == provider_id)
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Get a rollback event for a settlement, if one exists.
    pub fn get_rollback_event(&self, settlement_id: &str) -> Option<RollbackEvent> {
        self.rollback_events.get(settlement_id).map(|e| e.clone())
    }

    /// Count of all settlements.
    pub fn settlement_count(&self) -> usize {
        self.settlements.len()
    }
}

impl Default for SettlementFinalityEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ================================================================
// REST Handlers
// ================================================================

async fn create_settlement_handler(
    State(engine): State<Arc<SettlementFinalityEngine>>,
    Json(body): Json<CreateSettlementRequest>,
) -> Result<(StatusCode, Json<SettlementRecord>), (StatusCode, Json<ApiError>)> {
    let record = engine.create_settlement(
        &body.provider_id,
        &body.request_id,
        body.amount,
        body.token_id.clone(),
        &body.tx_id,
        body.block_height,
    );
    Ok((StatusCode::CREATED, Json(record)))
}

async fn update_confirmations_handler(
    State(engine): State<Arc<SettlementFinalityEngine>>,
    Path(id): Path<String>,
    Json(body): Json<UpdateConfirmationsRequest>,
) -> Result<Json<SettlementRecord>, (StatusCode, Json<ApiError>)> {
    match engine.update_confirmations(&id, body.confirmations, body.block_height) {
        Ok(record) => Ok(Json(record)),
        Err(e) => Err((StatusCode::NOT_FOUND, Json(ApiError::new(e)))),
    }
}

async fn check_finality_handler(
    State(engine): State<Arc<SettlementFinalityEngine>>,
    Path(id): Path<String>,
) -> Result<Json<ConfirmationCheck>, (StatusCode, Json<ApiError>)> {
    match engine.check_finality(&id) {
        Ok(check) => Ok(Json(check)),
        Err(e) => Err((StatusCode::NOT_FOUND, Json(ApiError::new(e)))),
    }
}

async fn rollback_handler(
    State(engine): State<Arc<SettlementFinalityEngine>>,
    Path(id): Path<String>,
    Json(body): Json<RollbackRequest>,
) -> Result<Json<RollbackEvent>, (StatusCode, Json<ApiError>)> {
    match engine.detect_rollback(&id, body.competing_tx_id) {
        Ok(event) => Ok(Json(event)),
        Err(e) => Err((StatusCode::CONFLICT, Json(ApiError::new(e)))),
    }
}

async fn get_settlement_handler(
    State(engine): State<Arc<SettlementFinalityEngine>>,
    Path(id): Path<String>,
) -> Result<Json<SettlementRecord>, (StatusCode, Json<ApiError>)> {
    match engine.get_settlement(&id) {
        Some(record) => Ok(Json(record)),
        None => Err((StatusCode::NOT_FOUND, Json(ApiError::new("Settlement not found")))),
    }
}

async fn get_audit_handler(
    State(engine): State<Arc<SettlementFinalityEngine>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<SettlementAuditEntry>>, (StatusCode, Json<ApiError>)> {
    let entries = engine.get_audit_trail(&id);
    Ok(Json(entries))
}

async fn list_settlements_handler(
    State(engine): State<Arc<SettlementFinalityEngine>>,
    Query(query): Query<ListSettlementsQuery>,
) -> Json<Vec<SettlementRecord>> {
    let settlements = engine.list_settlements(
        query.provider_id.as_deref(),
        query.status.as_deref(),
        query.from,
        query.to,
    );
    Json(settlements)
}

async fn get_summary_handler(
    State(engine): State<Arc<SettlementFinalityEngine>>,
) -> Json<SettlementSummary> {
    Json(engine.get_summary())
}

async fn get_pending_handler(
    State(engine): State<Arc<SettlementFinalityEngine>>,
) -> Json<Vec<ConfirmationCheck>> {
    Json(engine.get_pending_confirmations())
}

async fn batch_check_handler(
    State(engine): State<Arc<SettlementFinalityEngine>>,
) -> Json<Vec<ConfirmationCheck>> {
    Json(engine.batch_check_finality())
}

// ================================================================
// Router
// ================================================================

/// Build the settlement finality router.
pub fn build_settlement_finality_router(state: Arc<SettlementFinalityEngine>) -> axum::Router {
    use axum::routing::{get, post};

    axum::Router::new()
        .route("/v1/settlement-finality/create", post(create_settlement_handler))
        .route(
            "/v1/settlement-finality/confirmations/{id}",
            post(update_confirmations_handler),
        )
        .route("/v1/settlement-finality/check/{id}", post(check_finality_handler))
        .route("/v1/settlement-finality/rollback/{id}", post(rollback_handler))
        .route("/v1/settlement-finality/{id}", get(get_settlement_handler))
        .route(
            "/v1/settlement-finality/{id}/audit",
            get(get_audit_handler),
        )
        .route("/v1/settlement-finality", get(list_settlements_handler))
        .route("/v1/settlement-finality/summary", get(get_summary_handler))
        .route("/v1/settlement-finality/pending", get(get_pending_handler))
        .route(
            "/v1/settlement-finality/batch-check",
            post(batch_check_handler),
        )
        .with_state(state)
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> Arc<SettlementFinalityEngine> {
        Arc::new(SettlementFinalityEngine::with_config(FinalityConfig {
            required_confirmations: 10,
            timeout_blocks: 100,
            auto_check: true,
            check_interval_ms: 5000,
        }))
    }

    #[test]
    fn test_create_settlement() {
        let engine = make_engine();
        let record = engine.create_settlement(
            "provider-1",
            "request-1",
            1_000_000,
            None,
            "tx-abc123",
            500_000,
        );

        assert_eq!(record.provider_id, "provider-1");
        assert_eq!(record.request_id, "request-1");
        assert_eq!(record.amount, 1_000_000);
        assert_eq!(record.tx_id, "tx-abc123");
        assert_eq!(record.block_height, 500_000);
        assert_eq!(record.status, SettlementStatus::Pending);
        assert_eq!(record.confirmations, 0);
        assert_eq!(record.required_confirmations, 10);
        assert!(record.confirmed_at.is_none());
        assert!(record.finalized_at.is_none());
        assert!(record.rollback_at.is_none());
        assert!(record.token_id.is_none());

        // Verify audit trail
        let audit = engine.get_audit_trail(&record.id);
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].event, AuditEvent::Created);
    }

    #[test]
    fn test_update_confirmations() {
        let engine = make_engine();
        let record = engine.create_settlement("p1", "r1", 100, None, "tx1", 100);

        // First confirmation moves Pending -> Submitted -> Confirming
        let updated = engine.update_confirmations(&record.id, 1, 101).unwrap();
        assert_eq!(updated.status, SettlementStatus::Confirming);
        assert_eq!(updated.confirmations, 1);

        // Verify audit trail has Submitted + ConfirmationReceived
        let audit = engine.get_audit_trail(&record.id);
        assert!(audit.len() >= 2);
    }

    #[test]
    fn test_confirming_to_confirmed() {
        let engine = make_engine();
        let record = engine.create_settlement("p1", "r1", 100, None, "tx1", 100);

        // Move to confirming
        engine.update_confirmations(&record.id, 1, 101).unwrap();

        // Reach required confirmations (10)
        let updated = engine.update_confirmations(&record.id, 10, 110).unwrap();
        assert_eq!(updated.status, SettlementStatus::Confirmed);
        assert!(updated.confirmed_at.is_some());
    }

    #[test]
    fn test_confirmed_to_finalized() {
        let engine = make_engine();
        let record = engine.create_settlement("p1", "r1", 100, None, "tx1", 100);

        // Move through lifecycle to confirmed
        engine.update_confirmations(&record.id, 1, 101).unwrap();
        engine.update_confirmations(&record.id, 10, 110).unwrap();

        // Reach 2x confirmations (20)
        let updated = engine.update_confirmations(&record.id, 20, 120).unwrap();
        assert_eq!(updated.status, SettlementStatus::Finalized);
        assert!(updated.finalized_at.is_some());
    }

    #[test]
    fn test_timeout_detection() {
        let engine = make_engine();
        let record = engine.create_settlement("p1", "r1", 100, None, "tx1", 100);

        // Move to confirming with only 3 confirmations
        engine.update_confirmations(&record.id, 3, 103).unwrap();

        // Timeout the settlement
        let timed_out = engine.timeout_settlement(&record.id).unwrap();
        assert_eq!(timed_out.status, SettlementStatus::TimedOut);

        // Cannot update confirmations after timeout
        let result = engine.update_confirmations(&record.id, 5, 105);
        assert!(result.is_err());
    }

    #[test]
    fn test_rollback_detection() {
        let engine = make_engine();
        let record = engine.create_settlement("p1", "r1", 100, None, "tx1", 100);

        engine.update_confirmations(&record.id, 5, 105).unwrap();

        let event = engine
            .detect_rollback(&record.id, Some("tx-competing".to_string()))
            .unwrap();
        assert_eq!(event.settlement_id, record.id);
        assert_eq!(event.original_tx_id, "tx1");
        assert_eq!(event.competing_tx_id, Some("tx-competing".to_string()));
        assert_eq!(event.original_height, 105);

        // Verify settlement status
        let updated = engine.get_settlement(&record.id).unwrap();
        assert_eq!(updated.status, SettlementStatus::RolledBack);
        assert!(updated.rollback_at.is_some());

        // Cannot update confirmations after rollback
        let result = engine.update_confirmations(&record.id, 6, 106);
        assert!(result.is_err());

        // Verify rollback event can be retrieved
        let stored_event = engine.get_rollback_event(&record.id).unwrap();
        assert_eq!(stored_event.original_tx_id, "tx1");
    }

    #[test]
    fn test_audit_trail() {
        let engine = make_engine();
        let record = engine.create_settlement("p1", "r1", 100, None, "tx1", 100);

        // Initial: Created
        let audit = engine.get_audit_trail(&record.id);
        assert_eq!(audit.len(), 1);

        // Update confirmations: adds Submitted + ConfirmationReceived
        engine.update_confirmations(&record.id, 5, 105).unwrap();
        let audit = engine.get_audit_trail(&record.id);
        assert!(audit.len() >= 3);

        // Finalize
        engine.update_confirmations(&record.id, 10, 110).unwrap();
        engine.update_confirmations(&record.id, 20, 120).unwrap();
        let audit = engine.get_audit_trail(&record.id);

        let event_names: Vec<String> = audit.iter().map(|e| e.event.to_string()).collect();
        assert!(event_names.contains(&"created".to_string()));
        assert!(event_names.contains(&"submitted".to_string()));
        assert!(event_names.contains(&"finalized".to_string()));
    }

    #[test]
    fn test_list_settlements_with_filters() {
        let engine = make_engine();

        engine.create_settlement("p1", "r1", 100, None, "tx1", 100);
        engine.create_settlement("p2", "r2", 200, None, "tx2", 101);
        engine.create_settlement("p1", "r3", 300, None, "tx3", 102);

        // Filter by provider
        let p1 = engine.list_settlements(Some("p1"), None, None, None);
        assert_eq!(p1.len(), 2);

        // Filter by non-existent provider
        let p3 = engine.list_settlements(Some("p3"), None, None, None);
        assert_eq!(p3.len(), 0);

        // All settlements
        let all = engine.list_settlements(None, None, None, None);
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_summary_calculation() {
        let engine = make_engine();

        // Create and finalize one settlement
        let r1 = engine.create_settlement("p1", "r1", 1_000, None, "tx1", 100);
        engine.update_confirmations(&r1.id, 1, 101).unwrap();
        engine.update_confirmations(&r1.id, 10, 110).unwrap();
        engine.update_confirmations(&r1.id, 20, 120).unwrap();

        // Create a pending settlement
        engine.create_settlement("p1", "r2", 500, None, "tx2", 130);

        // Create and rollback a settlement
        let r3 = engine.create_settlement("p1", "r3", 750, None, "tx3", 140);
        engine.detect_rollback(&r3.id, None).unwrap();

        let summary = engine.get_summary();
        assert_eq!(summary.total_settlements, 3);
        assert_eq!(summary.finalized, 1);
        assert_eq!(summary.pending, 1);
        assert_eq!(summary.rolled_back, 1);
        assert_eq!(summary.total_value_settled, 1_000);
        assert_eq!(summary.avg_confirmation_blocks, 20.0);
    }

    #[test]
    fn test_pending_confirmations() {
        let engine = make_engine();

        let r1 = engine.create_settlement("p1", "r1", 100, None, "tx1", 100);
        engine.update_confirmations(&r1.id, 5, 105).unwrap();

        let r2 = engine.create_settlement("p1", "r2", 200, None, "tx2", 100);
        engine.update_confirmations(&r2.id, 1, 101).unwrap();
        engine.update_confirmations(&r2.id, 20, 120).unwrap(); // Finalized

        let pending = engine.get_pending_confirmations();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].settlement_id, r1.id);
        assert_eq!(pending[0].current_confirmations, 5);
        assert!(!pending[0].is_final);
    }

    #[test]
    fn test_batch_check() {
        let engine = make_engine();

        let r1 = engine.create_settlement("p1", "r1", 100, None, "tx1", 100);
        engine.update_confirmations(&r1.id, 3, 103).unwrap();

        let r2 = engine.create_settlement("p1", "r2", 200, None, "tx2", 100);

        let results = engine.batch_check_finality();
        assert_eq!(results.len(), 2);

        let ids: Vec<&str> = results.iter().map(|r| r.settlement_id.as_str()).collect();
        assert!(ids.contains(&r1.id.as_str()));
        assert!(ids.contains(&r2.id.as_str()));
    }

    #[test]
    fn test_config_update() {
        let engine = make_engine();

        let config = engine.get_config();
        assert_eq!(config.required_confirmations, 10);

        let new_config = FinalityConfig {
            required_confirmations: 50,
            timeout_blocks: 1440,
            auto_check: false,
            check_interval_ms: 30_000,
        };
        engine.update_config(new_config);

        let config = engine.get_config();
        assert_eq!(config.required_confirmations, 50);
        assert_eq!(config.timeout_blocks, 1440);
        assert!(!config.auto_check);
        assert_eq!(config.check_interval_ms, 30_000);

        // New settlement uses updated config
        let record = engine.create_settlement("p1", "r1", 100, None, "tx1", 100);
        assert_eq!(record.required_confirmations, 50);
    }

    #[test]
    fn test_provider_settlements() {
        let engine = make_engine();

        engine.create_settlement("prov-A", "r1", 100, None, "tx1", 100);
        engine.create_settlement("prov-B", "r2", 200, None, "tx2", 101);
        engine.create_settlement("prov-A", "r3", 300, None, "tx3", 102);
        engine.create_settlement("prov-A", "r4", 400, None, "tx4", 103);

        let prov_a = engine.get_provider_settlements("prov-A");
        assert_eq!(prov_a.len(), 3);

        let prov_b = engine.get_provider_settlements("prov-B");
        assert_eq!(prov_b.len(), 1);

        let prov_c = engine.get_provider_settlements("prov-C");
        assert_eq!(prov_c.len(), 0);
    }

    #[test]
    fn test_concurrent_confirmations() {
        use std::thread;

        let engine = Arc::new(SettlementFinalityEngine::with_config(FinalityConfig {
            required_confirmations: 100,
            timeout_blocks: 1000,
            auto_check: true,
            check_interval_ms: 1000,
        }));

        let record = engine.create_settlement("p1", "r1", 100, None, "tx1", 100);
        let id = record.id.clone();

        // Spawn multiple threads updating confirmations concurrently
        let mut handles = vec![];
        for i in 0..10 {
            let eng = engine.clone();
            let sid = id.clone();
            handles.push(thread::spawn(move || {
                let _ = eng.update_confirmations(&sid, 10 + i as u32, 110 + i as u64);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // Settlement should still exist and be valid
        let final_record = engine.get_settlement(&id).unwrap();
        assert!(matches!(
            final_record.status,
            SettlementStatus::Confirming | SettlementStatus::Confirmed | SettlementStatus::Finalized
        ));
    }

    #[test]
    fn test_full_lifecycle() {
        let engine = make_engine();

        // 1. Create settlement
        let record = engine.create_settlement(
            "provider-full",
            "request-full",
            5_000_000,
            Some("token-xyz".to_string()),
            "tx-full-lifecycle",
            600_000,
        );
        assert_eq!(record.status, SettlementStatus::Pending);

        // 2. Submit (first confirmation)
        let record = engine.update_confirmations(&record.id, 1, 600_001).unwrap();
        assert_eq!(record.status, SettlementStatus::Confirming);

        // 3. Progress confirmations
        let record = engine.update_confirmations(&record.id, 5, 600_005).unwrap();
        assert_eq!(record.status, SettlementStatus::Confirming);
        assert_eq!(record.confirmations, 5);

        // 4. Reach confirmed
        let record = engine.update_confirmations(&record.id, 10, 600_010).unwrap();
        assert_eq!(record.status, SettlementStatus::Confirmed);
        assert!(record.confirmed_at.is_some());

        // 5. Reach finalized (2x = 20)
        let record = engine.update_confirmations(&record.id, 20, 600_020).unwrap();
        assert_eq!(record.status, SettlementStatus::Finalized);
        assert!(record.finalized_at.is_some());

        // 6. Check finality
        let check = engine.check_finality(&record.id).unwrap();
        assert!(check.is_final);
        assert_eq!(check.next_check_at, 0);

        // 7. Verify full audit trail
        let audit = engine.get_audit_trail(&record.id);
        let event_names: Vec<String> = audit.iter().map(|e| e.event.to_string()).collect();
        assert!(event_names.contains(&"created".to_string()));
        assert!(event_names.contains(&"submitted".to_string()));
        assert!(event_names.contains(&"confirmation_received".to_string()));
        assert!(event_names.contains(&"finalized".to_string()));

        // 8. Verify in summary
        let summary = engine.get_summary();
        assert_eq!(summary.finalized, 1);
        assert_eq!(summary.total_value_settled, 5_000_000);

        // 9. Verify it appears in provider settlements
        let provider_settlements = engine.get_provider_settlements("provider-full");
        assert_eq!(provider_settlements.len(), 1);
        assert_eq!(provider_settlements[0].token_id, Some("token-xyz".to_string()));
    }
}
