//! Storage Rent Guard for on-chain box protection.
//!
//! Monitors all tracked boxes (provider, staking, settlement, treasury) for
//! approaching storage rent deadline (4 years / 1,051,200 blocks). Provides
//! auto-topup, box migration, and emergency handling capabilities.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use chrono::Utc;

// ─── Constants ──────────────────────────────────────────────────────

pub const RENT_THRESHOLD_BLOCKS: u64 = 1_051_200;
pub const NANOERG_PER_BYTE: u64 = 360;
pub const BLOCKS_PER_DAY: u64 = 720;
pub const DEFAULT_SCAN_INTERVAL_SECS: u64 = 1800;
pub const EMERGENCY_THRESHOLD_DAYS: f64 = 30.0;
pub const TOPUP_BUFFER_FACTOR: f64 = 1.5;
pub const DEFAULT_RENT_BUDGET_NANOERG: u64 = 10_000_000_000; // 10 ERG

// ─── Data Types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BoxType {
    Provider,
    Staking,
    Settlement,
    Treasury,
    Governance,
    Oracle,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RiskLevel {
    Safe,       // > 3 years
    Warning,    // 1-3 years
    Critical,   // 90 days - 1 year
    Emergency,  // < 90 days
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::Safe => write!(f, "Safe"),
            RiskLevel::Warning => write!(f, "Warning"),
            RiskLevel::Critical => write!(f, "Critical"),
            RiskLevel::Emergency => write!(f, "Emergency"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    pub token_id: String,
    pub amount: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackedBox {
    pub box_id: String,
    pub box_type: BoxType,
    pub address: String,
    pub value: u64,
    pub creation_height: u32,
    pub byte_size: u32,
    pub tokens: Vec<TokenInfo>,
    pub registers: HashMap<String, String>,
    pub ergo_tree: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxRentStatus {
    pub box_id: String,
    pub box_type: BoxType,
    pub creation_height: u32,
    pub current_height: u32,
    pub age_blocks: u64,
    pub rent_deadline_height: u64,
    pub days_until_deadline: f64,
    pub value_nanoerg: u64,
    pub min_value_nanoerg: u64,
    pub value_deficit: i64, // negative = underfunded
    pub risk_level: RiskLevel,
    pub last_scanned: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopUpRequest {
    pub request_id: String,
    pub box_id: String,
    pub box_type: BoxType,
    pub current_value: u64,
    pub target_value: u64,
    pub amount_needed: u64,
    pub fee_estimate: u64,
    pub priority: RiskLevel,
    pub status: RequestStatus,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RequestStatus {
    Pending,
    Approved,
    Executing,
    Completed,
    Failed { reason: String },
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationPlan {
    pub plan_id: String,
    pub box_id: String,
    pub box_type: BoxType,
    pub current_value: u64,
    pub tokens: Vec<TokenInfo>,
    pub registers: HashMap<String, String>,
    pub ergo_tree: String,
    pub fee_estimate: u64,
    pub risk_level: RiskLevel,
    pub reason: String,
    pub status: RequestStatus,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RentReport {
    pub generated_at: String,
    pub current_height: u32,
    pub total_boxes: usize,
    pub safe_boxes: usize,
    pub warning_boxes: usize,
    pub critical_boxes: usize,
    pub emergency_boxes: usize,
    pub total_value_nanoerg: u64,
    pub total_deficit_nanoerg: u64,
    pub erg_needed_for_protection: u64,
    pub boxes_by_type: HashMap<String, usize>,
    pub recommendations: Vec<String>,
}

// ─── Rent Scanner ───────────────────────────────────────────────────

pub struct RentScanner {
    tracked: Arc<DashMap<String, TrackedBox>>,
    statuses: Arc<DashMap<String, BoxRentStatus>>,
    current_height: AtomicU64,
    scan_count: AtomicU64,
    last_scan_height: AtomicU64,
}

impl RentScanner {
    pub fn new(initial_height: u64) -> Self {
        Self {
            tracked: Arc::new(DashMap::new()),
            statuses: Arc::new(DashMap::new()),
            current_height: AtomicU64::new(initial_height),
            scan_count: AtomicU64::new(0),
            last_scan_height: AtomicU64::new(0),
        }
    }

    pub fn set_height(&self, height: u64) {
        self.current_height.store(height, Ordering::SeqCst);
    }

    pub fn get_height(&self) -> u64 {
        self.current_height.load(Ordering::SeqCst)
    }

    pub fn track_box(&self, box_info: TrackedBox) {
        self.tracked.insert(box_info.box_id.clone(), box_info);
    }

    pub fn untrack_box(&self, box_id: &str) {
        self.tracked.remove(box_id);
        self.statuses.remove(box_id);
    }

    pub fn box_count(&self) -> usize {
        self.tracked.len()
    }

    pub fn scan_all(&self) -> Vec<BoxRentStatus> {
        let height = self.get_height();
        let mut results = Vec::new();

        for entry in self.tracked.iter() {
            let box_info = entry.value();
            let status = self.compute_status(box_info, height);
            self.statuses.insert(box_info.box_id.clone(), status.clone());
            results.push(status);
        }

        self.scan_count.fetch_add(1, Ordering::Relaxed);
        self.last_scan_height.store(height, Ordering::Relaxed);
        results
    }

    fn compute_status(&self, box_info: &TrackedBox, current_height: u64) -> BoxRentStatus {
        let age = current_height.saturating_sub(box_info.creation_height as u64);
        let deadline = box_info.creation_height as u64 + RENT_THRESHOLD_BLOCKS;
        let remaining = RENT_THRESHOLD_BLOCKS.saturating_sub(age);
        let days_until = remaining as f64 / BLOCKS_PER_DAY as f64;
        let min_val = NANOERG_PER_BYTE * box_info.byte_size as u64;
        let deficit = box_info.value as i64 - min_val as i64;

        let risk = if days_until <= EMERGENCY_THRESHOLD_DAYS {
            RiskLevel::Emergency
        } else if days_until <= 365.0 {
            RiskLevel::Critical
        } else if days_until <= 1095.0 {
            RiskLevel::Warning
        } else {
            RiskLevel::Safe
        };

        BoxRentStatus {
            box_id: box_info.box_id.clone(),
            box_type: box_info.box_type.clone(),
            creation_height: box_info.creation_height,
            current_height: current_height as u32,
            age_blocks: age,
            rent_deadline_height: deadline,
            days_until_deadline: days_until,
            value_nanoerg: box_info.value,
            min_value_nanoerg: min_val,
            value_deficit: deficit,
            risk_level: risk,
            last_scanned: Utc::now().to_rfc3339(),
        }
    }

    pub fn get_status(&self, box_id: &str) -> Option<BoxRentStatus> {
        self.statuses.get(box_id).map(|r| r.value().clone())
    }

    pub fn get_emergency_boxes(&self) -> Vec<BoxRentStatus> {
        self.statuses.iter()
            .filter(|r| r.value().risk_level == RiskLevel::Emergency)
            .map(|r| r.value().clone())
            .collect()
    }

    pub fn scan_stats(&self) -> serde_json::Value {
        serde_json::json!({
            "scan_count": self.scan_count.load(Ordering::Relaxed),
            "last_scan_height": self.last_scan_height.load(Ordering::Relaxed),
            "boxes_tracked": self.tracked.len(),
            "current_height": self.get_height(),
        })
    }
}

// ─── Top-Up Engine ──────────────────────────────────────────────────

pub struct TopUpEngine {
    scanner: Arc<RentScanner>,
    requests: Arc<DashMap<String, TopUpRequest>>,
    auto_approve: AtomicBool,
}

impl TopUpEngine {
    pub fn new(scanner: Arc<RentScanner>) -> Self {
        Self {
            scanner,
            requests: Arc::new(DashMap::new()),
            auto_approve: AtomicBool::new(false),
        }
    }

    pub fn set_auto_approve(&self, enabled: bool) {
        self.auto_approve.store(enabled, Ordering::SeqCst);
    }

    /// Create a topup request for a box that's underfunded.
    pub fn create_topup_request(&self, box_id: &str) -> Result<TopUpRequest, String> {
        let tracked = self.scanner.tracked.get(box_id)
            .ok_or_else(|| format!("Box {} not tracked", box_id))?
            .value().clone();

        let status = self.scanner.get_status(box_id)
            .ok_or_else(|| format!("No status for box {}", box_id))?;

        if status.value_deficit >= 0 {
            return Err(format!("Box {} is not underfunded (deficit: {})", box_id, status.value_deficit));
        }

        let deficit_abs = status.value_deficit.unsigned_abs();
        let target = status.min_value_nanoerg + (status.min_value_nanoerg as f64 * TOPUP_BUFFER_FACTOR) as u64;
        let amount_needed = target.saturating_sub(status.value_nanoerg);
        let fee_estimate = 1_000_000u64;

        let request = TopUpRequest {
            request_id: Uuid::new_v4().to_string(),
            box_id: box_id.to_string(),
            box_type: tracked.box_type,
            current_value: status.value_nanoerg,
            target_value: target,
            amount_needed,
            fee_estimate,
            priority: status.risk_level,
            status: if self.auto_approve.load(Ordering::SeqCst) {
                RequestStatus::Approved
            } else {
                RequestStatus::Pending
            },
            created_at: Utc::now().to_rfc3339(),
        };

        self.requests.insert(request.request_id.clone(), request.clone());
        Ok(request)
    }

    /// Auto-generate topup requests for all underfunded boxes.
    pub fn scan_and_create_requests(&self) -> Vec<TopUpRequest> {
        let mut requests = Vec::new();
        for entry in self.scanner.statuses.iter() {
            let status = entry.value();
            if status.value_deficit < 0 {
                if let Ok(req) = self.create_topup_request(&status.box_id) {
                    requests.push(req);
                }
            }
        }
        requests
    }

    pub fn get_request(&self, request_id: &str) -> Option<TopUpRequest> {
        self.requests.get(request_id).map(|r| r.value().clone())
    }

    pub fn approve_request(&self, request_id: &str) -> Result<(), String> {
        let mut req = self.requests.get_mut(request_id)
            .ok_or_else(|| format!("Request {} not found", request_id))?;
        if req.status != RequestStatus::Pending {
            return Err(format!("Request {} is not pending (current: {:?})", request_id, req.status));
        }
        req.status = RequestStatus::Approved;
        Ok(())
    }

    pub fn reject_request(&self, request_id: &str) -> Result<(), String> {
        let mut req = self.requests.get_mut(request_id)
            .ok_or_else(|| format!("Request {} not found", request_id))?;
        req.status = RequestStatus::Rejected;
        Ok(())
    }

    pub fn complete_request(&self, request_id: &str) -> Result<(), String> {
        let mut req = self.requests.get_mut(request_id)
            .ok_or_else(|| format!("Request {} not found", request_id))?;
        req.status = RequestStatus::Completed;
        Ok(())
    }
}

// ─── Box Migration Planner ─────────────────────────────────────────

pub struct BoxMigrationPlanner {
    scanner: Arc<RentScanner>,
    plans: Arc<DashMap<String, MigrationPlan>>,
}

impl BoxMigrationPlanner {
    pub fn new(scanner: Arc<RentScanner>) -> Self {
        Self {
            scanner,
            plans: Arc::new(DashMap::new()),
        }
    }

    /// Plan migration for a box nearing rent deadline.
    pub fn create_migration_plan(&self, box_id: &str) -> Result<MigrationPlan, String> {
        let tracked = self.scanner.tracked.get(box_id)
            .ok_or_else(|| format!("Box {} not tracked", box_id))?
            .value().clone();

        let status = self.scanner.get_status(box_id)
            .ok_or_else(|| format!("No status for box {}", box_id))?;

        if status.days_until_deadline > 365.0 * 3.0 {
            return Err(format!("Box {} is not due for migration ({} days remaining)", box_id, status.days_until_deadline));
        }

        let reason = if status.days_until_deadline <= EMERGENCY_THRESHOLD_DAYS {
            format!("EMERGENCY: {} days until rent deadline. Immediate migration required.", status.days_until_deadline as u32)
        } else if status.days_until_deadline <= 90.0 {
            format!("CRITICAL: {} days until rent deadline.", status.days_until_deadline as u32)
        } else {
            format!("Warning: {} days until rent deadline. Proactive migration recommended.", status.days_until_deadline as u32)
        };

        let plan = MigrationPlan {
            plan_id: Uuid::new_v4().to_string(),
            box_id: box_id.to_string(),
            box_type: tracked.box_type.clone(),
            current_value: tracked.value,
            tokens: tracked.tokens.clone(),
            registers: tracked.registers.clone(),
            ergo_tree: tracked.ergo_tree,
            fee_estimate: 1_000_000u64,
            risk_level: status.risk_level,
            reason,
            status: RequestStatus::Pending,
            created_at: Utc::now().to_rfc3339(),
        };

        self.plans.insert(plan.plan_id.clone(), plan.clone());
        Ok(plan)
    }

    /// Auto-create migration plans for all boxes at risk.
    pub fn scan_and_create_plans(&self) -> Vec<MigrationPlan> {
        let mut plans = Vec::new();
        for entry in self.scanner.statuses.iter() {
            let status = entry.value();
            if status.risk_level == RiskLevel::Emergency || status.risk_level == RiskLevel::Critical {
                if let Ok(plan) = self.create_migration_plan(&status.box_id) {
                    plans.push(plan);
                }
            }
        }
        plans
    }

    pub fn get_plan(&self, plan_id: &str) -> Option<MigrationPlan> {
        self.plans.get(plan_id).map(|r| r.value().clone())
    }

    pub fn approve_plan(&self, plan_id: &str) -> Result<(), String> {
        let mut plan = self.plans.get_mut(plan_id)
            .ok_or_else(|| format!("Plan {} not found", plan_id))?;
        if plan.status != RequestStatus::Pending {
            return Err(format!("Plan {} is not pending", plan_id));
        }
        plan.status = RequestStatus::Approved;
        Ok(())
    }
}

// ─── Rent Budget Manager ───────────────────────────────────────────

pub struct RentBudgetManager {
    budget: AtomicU64,
    spent: AtomicU64,
    allocated: AtomicU64,
    alert_threshold_pct: AtomicU64, // percentage (0-100)
    low_budget_alert: AtomicBool,
}

impl RentBudgetManager {
    pub fn new(initial_budget: u64) -> Self {
        Self {
            budget: AtomicU64::new(initial_budget),
            spent: AtomicU64::new(0),
            allocated: AtomicU64::new(0),
            alert_threshold_pct: AtomicU64::new(20),
            low_budget_alert: AtomicBool::new(false),
        }
    }

    pub fn set_budget(&self, amount: u64) {
        self.budget.store(amount, Ordering::SeqCst);
        self.low_budget_alert.store(false, Ordering::SeqCst);
    }

    pub fn allocate(&self, amount: u64) -> Result<(), String> {
        let current_budget = self.budget.load(Ordering::SeqCst);
        let current_allocated = self.allocated.load(Ordering::SeqCst);
        let available = current_budget.saturating_sub(current_allocated);

        if amount > available {
            return Err(format!(
                "Cannot allocate {} nanoERG: only {} available (budget: {}, allocated: {})",
                amount, available, current_budget, current_allocated
            ));
        }

        self.allocated.fetch_add(amount, Ordering::SeqCst);
        Ok(())
    }

    pub fn spend(&self, amount: u64) {
        self.spent.fetch_add(amount, Ordering::SeqCst);
        let current_spent = self.spent.load(Ordering::SeqCst);
        let current_allocated = self.allocated.load(Ordering::SeqCst);
        if current_spent > current_allocated {
            self.allocated.fetch_add(current_spent - current_allocated, Ordering::SeqCst);
        }
    }

    pub fn release_allocation(&self, amount: u64) {
        self.allocated.fetch_sub(amount.saturating_sub(self.spent.load(Ordering::SeqCst)), Ordering::SeqCst);
    }

    pub fn is_low_budget(&self) -> bool {
        let budget = self.budget.load(Ordering::SeqCst);
        if budget == 0 { return false; }
        let remaining = budget.saturating_sub(self.spent.load(Ordering::SeqCst));
        let pct = (remaining as f64 / budget as f64) * 100.0;
        let threshold = self.alert_threshold_pct.load(Ordering::SeqCst) as f64;
        pct <= threshold
    }

    pub fn budget_status(&self) -> serde_json::Value {
        let budget = self.budget.load(Ordering::SeqCst);
        let spent = self.spent.load(Ordering::SeqCst);
        let allocated = self.allocated.load(Ordering::SeqCst);
        let remaining = budget.saturating_sub(spent);
        let unallocated = budget.saturating_sub(allocated);
        let pct_used = if budget > 0 { (spent as f64 / budget as f64) * 100.0 } else { 0.0 };

        serde_json::json!({
            "budget_nanoerg": budget,
            "spent_nanoerg": spent,
            "allocated_nanoerg": allocated,
            "remaining_nanoerg": remaining,
            "unallocated_nanoerg": unallocated,
            "percent_used": format!("{:.1}%", pct_used),
            "is_low": self.is_low_budget(),
            "erg_budget": budget as f64 / 1e9,
            "erg_spent": spent as f64 / 1e9,
            "erg_remaining": remaining as f64 / 1e9,
        })
    }
}

// ─── Emergency Handler ─────────────────────────────────────────────

pub struct EmergencyHandler {
    scanner: Arc<RentScanner>,
    migration_planner: Arc<BoxMigrationPlanner>,
    topup_engine: Arc<TopUpEngine>,
    budget: Arc<RentBudgetManager>,
    emergency_count: AtomicU64,
}

impl EmergencyHandler {
    pub fn new(
        scanner: Arc<RentScanner>,
        migration_planner: Arc<BoxMigrationPlanner>,
        topup_engine: Arc<TopUpEngine>,
        budget: Arc<RentBudgetManager>,
    ) -> Self {
        Self {
            scanner,
            migration_planner,
            topup_engine,
            budget,
            emergency_count: AtomicU64::new(0),
        }
    }

    /// Handle all emergency boxes -- create migration plans and topup requests.
    pub fn handle_emergencies(&self) -> serde_json::Value {
        self.scanner.scan_all();
        let emergency_boxes = self.scanner.get_emergency_boxes();
        self.emergency_count.fetch_add(emergency_boxes.len() as u64, Ordering::Relaxed);

        let mut migrations_created = 0;
        let mut topups_created = 0;
        let mut errors: Vec<String> = Vec::new();

        for box_status in &emergency_boxes {
            // Try migration first
            match self.migration_planner.create_migration_plan(&box_status.box_id) {
                Ok(_) => migrations_created += 1,
                Err(e) => errors.push(format!("Migration {} failed: {}", box_status.box_id, e)),
            }

            // Also create topup if underfunded
            if box_status.value_deficit < 0 {
                match self.topup_engine.create_topup_request(&box_status.box_id) {
                    Ok(_) => topups_created += 1,
                    Err(e) => errors.push(format!("Topup {} failed: {}", box_status.box_id, e)),
                }
            }
        }

        serde_json::json!({
            "emergency_boxes": emergency_boxes.len(),
            "migrations_created": migrations_created,
            "topups_created": topups_created,
            "errors": errors,
            "budget_low": self.budget.is_low_budget(),
        })
    }

    pub fn emergency_count(&self) -> u64 {
        self.emergency_count.load(Ordering::Relaxed)
    }
}

// ─── Rent Guard Service (main entry point) ─────────────────────────

pub struct RentGuardService {
    scanner: Arc<RentScanner>,
    topup_engine: Arc<TopUpEngine>,
    migration_planner: Arc<BoxMigrationPlanner>,
    budget: Arc<RentBudgetManager>,
    emergency_handler: Arc<EmergencyHandler>,
}

impl RentGuardService {
    pub fn new(initial_height: u64) -> Self {
        let scanner = Arc::new(RentScanner::new(initial_height));
        let topup_engine = Arc::new(TopUpEngine::new(scanner.clone()));
        let migration_planner = Arc::new(BoxMigrationPlanner::new(scanner.clone()));
        let budget = Arc::new(RentBudgetManager::new(DEFAULT_RENT_BUDGET_NANOERG));
        let emergency_handler = Arc::new(EmergencyHandler::new(
            scanner.clone(),
            migration_planner.clone(),
            topup_engine.clone(),
            budget.clone(),
        ));

        Self {
            scanner,
            topup_engine,
            migration_planner,
            budget,
            emergency_handler,
        }
    }

    pub fn track_box(&self, box_info: TrackedBox) {
        self.scanner.track_box(box_info);
    }

    pub fn set_height(&self, height: u64) {
        self.scanner.set_height(height);
    }

    /// Get rent status for all tracked boxes.
    pub fn status(&self) -> Vec<BoxRentStatus> {
        self.scanner.scan_all()
    }

    /// Get summary of rent protection status.
    pub fn summary(&self) -> serde_json::Value {
        self.scanner.scan_all();
        let statuses: Vec<BoxRentStatus> = self.scanner.statuses.iter()
            .map(|r| r.value().clone())
            .collect();

        let safe = statuses.iter().filter(|s| s.risk_level == RiskLevel::Safe).count();
        let warning = statuses.iter().filter(|s| s.risk_level == RiskLevel::Warning).count();
        let critical = statuses.iter().filter(|s| s.risk_level == RiskLevel::Critical).count();
        let emergency = statuses.iter().filter(|s| s.risk_level == RiskLevel::Emergency).count();
        let total_deficit: i64 = statuses.iter().map(|s| s.value_deficit.min(0)).sum::<i64>();

        serde_json::json!({
            "total_boxes": statuses.len(),
            "safe": safe,
            "warning": warning,
            "critical": critical,
            "emergency": emergency,
            "total_deficit_nanoerg": total_deficit.unsigned_abs(),
            "erg_needed": total_deficit.unsigned_abs() as f64 / 1e9,
            "budget": self.budget.budget_status(),
            "scan": self.scanner.scan_stats(),
            "emergencies_handled": self.emergency_handler.emergency_count(),
        })
    }

    /// Generate rent report.
    pub fn generate_report(&self) -> RentReport {
        let statuses: Vec<BoxRentStatus> = self.scanner.scan_all();
        let height = self.scanner.get_height();

        let mut by_type: HashMap<String, usize> = HashMap::new();
        let mut recommendations = Vec::new();

        for s in &statuses {
            let type_key = format!("{:?}", s.box_type);
            *by_type.entry(type_key).or_insert(0) += 1;
        }

        let emergency_count = statuses.iter().filter(|s| s.risk_level == RiskLevel::Emergency).count();
        let critical_count = statuses.iter().filter(|s| s.risk_level == RiskLevel::Critical).count();

        if emergency_count > 0 {
            recommendations.push(format!("URGENT: {} boxes within 30 days of rent deadline!", emergency_count));
        }
        if critical_count > 0 {
            recommendations.push(format!("{} boxes need migration within the next year.", critical_count));
        }

        let total_deficit: u64 = statuses.iter()
            .map(|s| if s.value_deficit < 0 { s.value_deficit.unsigned_abs() } else { 0 })
            .sum();

        RentReport {
            generated_at: Utc::now().to_rfc3339(),
            current_height: height as u32,
            total_boxes: statuses.len(),
            safe_boxes: statuses.iter().filter(|s| s.risk_level == RiskLevel::Safe).count(),
            warning_boxes: statuses.iter().filter(|s| s.risk_level == RiskLevel::Warning).count(),
            critical_boxes: statuses.iter().filter(|s| s.risk_level == RiskLevel::Critical).count(),
            emergency_boxes: statuses.iter().filter(|s| s.risk_level == RiskLevel::Emergency).count(),
            total_value_nanoerg: statuses.iter().map(|s| s.value_nanoerg).sum(),
            total_deficit_nanoerg: total_deficit,
            erg_needed_for_protection: total_deficit,
            boxes_by_type: by_type,
            recommendations,
        }
    }
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tracked_box(box_id: &str, box_type: BoxType, value: u64, creation_height: u32, byte_size: u32) -> TrackedBox {
        TrackedBox {
            box_id: box_id.to_string(),
            box_type,
            address: "9hPU9YXhJ5oJ3k1".to_string(),
            value,
            creation_height,
            byte_size,
            tokens: vec![],
            registers: HashMap::new(),
            ergo_tree: "1005040004000e36100204a00b08cd0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798".to_string(),
        }
    }

    #[test]
    fn test_constants() {
        assert_eq!(RENT_THRESHOLD_BLOCKS, 1_051_200);
        assert_eq!(BLOCKS_PER_DAY, 720);
        assert!(EMERGENCY_THRESHOLD_DAYS <= 30.0);
    }

    #[test]
    fn test_scanner_basic() {
        let scanner = RentScanner::new(500_000);
        scanner.track_box(make_tracked_box("box1", BoxType::Provider, 1_000_000, 100_000, 200));

        assert_eq!(scanner.box_count(), 1);

        let statuses = scanner.scan_all();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].box_id, "box1");
        assert_eq!(statuses[0].age_blocks, 400_000);
        assert_eq!(statuses[0].risk_level, RiskLevel::Warning);
    }

    #[test]
    fn test_scanner_emergency_detection() {
        let scanner = RentScanner::new(1_048_000);
        scanner.track_box(make_tracked_box("box1", BoxType::Treasury, 1_000_000, 18_000, 200));

        let statuses = scanner.scan_all();
        assert_eq!(statuses[0].risk_level, RiskLevel::Emergency);
        assert!(statuses[0].days_until_deadline < 30.0);

        let emergency = scanner.get_emergency_boxes();
        assert_eq!(emergency.len(), 1);
    }

    #[test]
    fn test_scanner_safe_box() {
        let scanner = RentScanner::new(200_000);
        scanner.track_box(make_tracked_box("box1", BoxType::Provider, 1_000_000, 100_000, 200));

        let statuses = scanner.scan_all();
        assert_eq!(statuses[0].risk_level, RiskLevel::Safe);
    }

    #[test]
    fn test_topup_engine_underfunded() {
        let scanner = Arc::new(RentScanner::new(500_000));
        // Box with value below min_box_value (360 * 200 = 72_000)
        scanner.track_box(make_tracked_box("box1", BoxType::Provider, 50_000, 100_000, 200));
        scanner.scan_all();

        let engine = TopUpEngine::new(scanner);
        let req = engine.create_topup_request("box1").unwrap();

        assert!(req.amount_needed > 0);
        assert_eq!(req.priority, RiskLevel::Warning);
    }

    #[test]
    fn test_topup_engine_funded_box() {
        let scanner = Arc::new(RentScanner::new(500_000));
        scanner.track_box(make_tracked_box("box1", BoxType::Provider, 500_000, 100_000, 200));
        scanner.scan_all();

        let engine = TopUpEngine::new(scanner);
        let result = engine.create_topup_request("box1");
        assert!(result.is_err());
    }

    #[test]
    fn test_migration_planner() {
        let scanner = Arc::new(RentScanner::new(1_048_000));
        scanner.track_box(make_tracked_box("box1", BoxType::Treasury, 1_000_000, 18_000, 200));
        scanner.scan_all();

        let planner = BoxMigrationPlanner::new(scanner);
        let plan = planner.create_migration_plan("box1").unwrap();

        assert_eq!(plan.box_id, "box1");
        assert_eq!(plan.risk_level, RiskLevel::Emergency);
        assert!(plan.reason.contains("EMERGENCY"));
    }

    #[test]
    fn test_migration_planner_safe_box_rejected() {
        let scanner = Arc::new(RentScanner::new(200_000));
        scanner.track_box(make_tracked_box("box1", BoxType::Provider, 1_000_000, 100_000, 200));
        scanner.scan_all();

        let planner = BoxMigrationPlanner::new(scanner);
        let result = planner.create_migration_plan("box1");
        assert!(result.is_err());
    }

    #[test]
    fn test_budget_manager() {
        let budget = RentBudgetManager::new(1_000_000_000);

        budget.allocate(100_000_000).unwrap();
        assert_eq!(budget.budget_status()["allocated_nanoerg"], 100_000_000);

        budget.spend(50_000_000);
        assert_eq!(budget.budget_status()["spent_nanoerg"], 50_000_000);

        assert!(!budget.is_low_budget());
    }

    #[test]
    fn test_budget_manager_overflow() {
        let budget = RentBudgetManager::new(100_000);
        let result = budget.allocate(200_000);
        assert!(result.is_err());
    }

    #[test]
    fn test_rent_guard_service_summary() {
        let service = RentGuardService::new(500_000);
        service.track_box(make_tracked_box("box1", BoxType::Provider, 1_000_000, 100_000, 200));
        service.track_box(make_tracked_box("box2", BoxType::Treasury, 500_000, 200_000, 300));

        let summary = service.summary();
        assert_eq!(summary["total_boxes"], 2);
    }

    #[test]
    fn test_rent_guard_report() {
        let service = RentGuardService::new(1_048_000);
        service.track_box(make_tracked_box("box1", BoxType::Treasury, 50_000, 18_000, 200));

        let report = service.generate_report();
        assert_eq!(report.total_boxes, 1);
        assert_eq!(report.emergency_boxes, 1);
        assert!(!report.recommendations.is_empty());
    }

    #[test]
    fn test_emergency_handler() {
        let service = RentGuardService::new(1_048_000);
        service.track_box(make_tracked_box("box1", BoxType::Treasury, 50_000, 18_000, 200));

        let result = service.emergency_handler.handle_emergencies();
        assert_eq!(result["emergency_boxes"], 1);
        assert_eq!(result["migrations_created"], 1);
        assert_eq!(result["topups_created"], 1);
    }

    #[test]
    fn test_risk_level_display() {
        assert_eq!(format!("{}", RiskLevel::Safe), "Safe");
        assert_eq!(format!("{}", RiskLevel::Emergency), "Emergency");
    }
}
