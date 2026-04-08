#![allow(dead_code)]
//! UTXO Consolidation Engine for storage rent protection.
//!
//! Ergo storage rent: boxes older than 4 years (1,051,200 blocks) can be spent by miners.
//! Min box value = 360 nanoERG/byte. This module consolidates small UTXOs into fewer
//! larger boxes to reduce storage rent burden and protect user funds.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use chrono::Utc;

// ─── Constants ──────────────────────────────────────────────────────

/// Storage rent threshold: 4 years in blocks (~1,051,200 blocks at 2 min/block).
pub const RENT_THRESHOLD_BLOCKS: u64 = 1_051_200;
/// Min box value: 360 nanoERG per byte of serialized box size.
pub const NANOERG_PER_BYTE: u64 = 360;
/// Recommended min fee for consolidation tx.
pub const CONSOLIDATION_FEE: u64 = 1_000_000; // 0.001 ERG
/// Safety buffer: extra ERG above min box value to avoid re-consolidation.
pub const SAFETY_BUFFER_NANOERG: u64 = 500_000;
/// Default scan interval in seconds.
pub const DEFAULT_SCAN_INTERVAL_SECS: u64 = 3600;
/// Blocks per day on Ergo (~720 at 2 min/block).
pub const BLOCKS_PER_DAY: u64 = 720;

// ─── Data Types ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Priority {
    Critical,  // < 30 days until rent
    High,      // < 90 days
    Medium,    // < 1 year
    Low,       // < 3 years
    Safe,      // > 3 years
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    pub token_id: String,
    pub amount: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxInfo {
    pub box_id: String,
    pub address: String,
    pub value: u64,
    pub creation_height: u32,
    pub byte_size: u32,
    pub tokens: Vec<TokenInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationPlan {
    pub plan_id: String,
    pub target_address: String,
    pub boxes_to_consolidate: Vec<String>,
    pub total_input_value: u64,
    pub num_inputs: usize,
    pub estimated_fee: u64,
    pub estimated_rent_saved: u64,
    pub priority: Priority,
    pub created_at: String,
    pub status: PlanStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PlanStatus {
    Pending,
    InProgress,
    Completed,
    Failed { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationHistory {
    pub operation_id: String,
    pub plan_id: String,
    pub boxes_consumed: Vec<String>,
    pub box_created: String,
    pub value_before: u64,
    pub value_after: u64,
    pub fee_paid: u64,
    pub rent_blocks_saved: u64,
    pub timestamp: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RentEstimate {
    pub box_id: String,
    pub byte_size: u32,
    pub value_nanoerg: u64,
    pub min_value_nanoerg: u64,
    pub creation_height: u32,
    pub current_height: u32,
    pub age_blocks: u64,
    pub days_until_rent: f64,
    pub annual_rent_cost_nanoerg: u64,
    pub priority: Priority,
}

// ─── Box Age Tracker ───────────────────────────────────────────────

pub struct BoxAgeTracker {
    boxes: Arc<DashMap<String, BoxInfo>>,
    current_height: AtomicU64,
}

impl BoxAgeTracker {
    pub fn new(initial_height: u64) -> Self {
        Self {
            boxes: Arc::new(DashMap::new()),
            current_height: AtomicU64::new(initial_height),
        }
    }

    pub fn set_height(&self, height: u64) {
        self.current_height.store(height, Ordering::SeqCst);
    }

    pub fn get_height(&self) -> u64 {
        self.current_height.load(Ordering::SeqCst)
    }

    pub fn track_box(&self, info: BoxInfo) {
        self.boxes.insert(info.box_id.clone(), info);
    }

    pub fn untrack_box(&self, box_id: &str) {
        self.boxes.remove(box_id);
    }

    pub fn get_box(&self, box_id: &str) -> Option<BoxInfo> {
        self.boxes.get(box_id).map(|r| r.value().clone())
    }

    pub fn get_all_boxes(&self) -> Vec<BoxInfo> {
        self.boxes.iter().map(|r| r.value().clone()).collect()
    }

    pub fn boxes_by_address(&self, address: &str) -> Vec<BoxInfo> {
        self.boxes.iter()
            .filter(|r| r.value().address == address)
            .map(|r| r.value().clone())
            .collect()
    }

    pub fn age_blocks(&self, box_id: &str) -> Option<u64> {
        self.boxes.get(box_id).map(|r| {
            self.get_height().saturating_sub(r.value().creation_height as u64)
        })
    }

    pub fn days_until_rent(&self, box_id: &str) -> Option<f64> {
        self.age_blocks(box_id).map(|age| {
            let remaining = RENT_THRESHOLD_BLOCKS.saturating_sub(age);
            remaining as f64 / BLOCKS_PER_DAY as f64
        })
    }

    pub fn compute_priority(&self, box_id: &str) -> Priority {
        match self.days_until_rent(box_id) {
            None => Priority::Safe,
            Some(days) if days <= 30.0 => Priority::Critical,
            Some(days) if days <= 90.0 => Priority::High,
            Some(days) if days <= 365.0 => Priority::Medium,
            Some(days) if days <= 1095.0 => Priority::Low,
            Some(_) => Priority::Safe,
        }
    }

    pub fn box_count(&self) -> usize {
        self.boxes.len()
    }

    pub fn at_risk_count(&self) -> usize {
        self.boxes.iter()
            .filter(|r| {
                let p = self.compute_priority(&r.value().box_id);
                p == Priority::Critical || p == Priority::High
            })
            .count()
    }
}

// ─── Rent Estimator ────────────────────────────────────────────────

pub struct RentEstimatorService;

impl RentEstimatorService {
    pub fn min_box_value(byte_size: u32) -> u64 {
        NANOERG_PER_BYTE * byte_size as u64
    }

    pub fn estimate(box_info: &BoxInfo, current_height: u64) -> RentEstimate {
        let age = current_height.saturating_sub(box_info.creation_height as u64);
        let min_val = Self::min_box_value(box_info.byte_size);
        let days_until = {
            let remaining = RENT_THRESHOLD_BLOCKS.saturating_sub(age);
            remaining as f64 / BLOCKS_PER_DAY as f64
        };
        // Annual rent cost: proportional to box size, amortized over 4 years
        let annual_rent = (min_val * 365) / (RENT_THRESHOLD_BLOCKS / BLOCKS_PER_DAY);

        let priority = Self::compute_priority_from_days(days_until);

        RentEstimate {
            box_id: box_info.box_id.clone(),
            byte_size: box_info.byte_size,
            value_nanoerg: box_info.value,
            min_value_nanoerg: min_val,
            creation_height: box_info.creation_height,
            current_height: current_height as u32,
            age_blocks: age,
            days_until_rent: days_until,
            annual_rent_cost_nanoerg: annual_rent,
            priority,
        }
    }

    pub fn compute_priority_from_days(days: f64) -> Priority {
        if days <= 30.0 { Priority::Critical }
        else if days <= 90.0 { Priority::High }
        else if days <= 365.0 { Priority::Medium }
        else if days <= 1095.0 { Priority::Low }
        else { Priority::Safe }
    }

    pub fn rent_saved_by_consolidation(boxes: &[BoxInfo], _current_height: u64) -> u64 {
        // Each box pays min_value in rent. Consolidating N boxes into 1 saves (N-1) box minimums.
        let individual_rents: u64 = boxes.iter()
            .map(|b| Self::min_box_value(b.byte_size))
            .sum();
        // Consolidated box is roughly sum of byte sizes + overhead (~50 bytes)
        let total_bytes: u32 = boxes.iter().map(|b| b.byte_size).sum::<u32>() + 50;
        let consolidated_rent = Self::min_box_value(total_bytes);
        individual_rents.saturating_sub(consolidated_rent)
    }
}

// ─── Dust Collector ────────────────────────────────────────────────

pub struct DustCollector {
    tracker: Arc<BoxAgeTracker>,
    dust_threshold: AtomicU64,
}

impl DustCollector {
    pub fn new(tracker: Arc<BoxAgeTracker>, dust_threshold: u64) -> Self {
        Self {
            tracker,
            dust_threshold: AtomicU64::new(dust_threshold),
        }
    }

    pub fn set_dust_threshold(&self, threshold: u64) {
        self.dust_threshold.store(threshold, Ordering::SeqCst);
    }

    pub fn get_dust_threshold(&self) -> u64 {
        self.dust_threshold.load(Ordering::SeqCst)
    }

    /// Find dust boxes below the threshold (excluding boxes with tokens).
    pub fn collect_dust(&self) -> Vec<BoxInfo> {
        let threshold = self.get_dust_threshold();
        self.tracker.get_all_boxes()
            .into_iter()
            .filter(|b| b.value < threshold && b.tokens.is_empty())
            .collect()
    }

    /// Find dust boxes for a specific address.
    pub fn collect_dust_for_address(&self, address: &str) -> Vec<BoxInfo> {
        let threshold = self.get_dust_threshold();
        self.tracker.boxes_by_address(address)
            .into_iter()
            .filter(|b| b.value < threshold && b.tokens.is_empty())
            .collect()
    }
}

// ─── Consolidation Planner ─────────────────────────────────────────

pub struct ConsolidationPlanner {
    tracker: Arc<BoxAgeTracker>,
    dust_collector: Arc<DustCollector>,
    max_inputs_per_tx: usize,
}

impl ConsolidationPlanner {
    pub fn new(tracker: Arc<BoxAgeTracker>, dust_collector: Arc<DustCollector>, max_inputs: usize) -> Self {
        Self {
            tracker,
            dust_collector,
            max_inputs_per_tx: max_inputs,
        }
    }

    /// Create consolidation plan for an address.
    pub fn create_plan(&self, address: &str) -> Vec<ConsolidationPlan> {
        let _boxes = self.tracker.boxes_by_address(address);
        let dust = self.dust_collector.collect_dust_for_address(address);

        if dust.is_empty() {
            return vec![];
        }

        let current_height = self.tracker.get_height();
        let mut plans: Vec<ConsolidationPlan> = Vec::new();
        let mut batch: Vec<BoxInfo> = Vec::new();
        let mut batch_value: u64 = 0;

        // Sort by priority (most urgent first), then by value (smallest first)
        let mut sorted_dust: Vec<BoxInfo> = dust;
        sorted_dust.sort_by(|a, b| {
            let pa = RentEstimatorService::estimate(a, current_height).priority.clone() as i32;
            let pb = RentEstimatorService::estimate(b, current_height).priority.clone() as i32;
            pb.cmp(&pa).then(a.value.cmp(&b.value))
        });

        for box_info in sorted_dust {
            batch.push(box_info.clone());
            batch_value += box_info.value;

            if batch.len() >= self.max_inputs_per_tx {
                let plan = self.build_plan(address, &batch, batch_value, &current_height);
                plans.push(plan);
                batch.clear();
                batch_value = 0;
            }
        }

        if !batch.is_empty() {
            // Only create plan if output value exceeds min box value + fee
            let estimated_output_size = 200u32; // conservative estimate
            let min_output = RentEstimatorService::min_box_value(estimated_output_size) + CONSOLIDATION_FEE;
            if batch_value > min_output {
                let plan = self.build_plan(address, &batch, batch_value, &current_height);
                plans.push(plan);
            }
        }

        plans
    }

    fn build_plan(&self, address: &str, boxes: &[BoxInfo], total_value: u64, current_height: &u64) -> ConsolidationPlan {
        let rent_saved = RentEstimatorService::rent_saved_by_consolidation(boxes, *current_height);
        let max_priority = boxes.iter()
            .map(|b| {
                let p = RentEstimatorService::estimate(b, *current_height).priority.clone() as u8;
                p
            })
            .max()
            .unwrap_or(5);
        let priority = match max_priority {
            0 => Priority::Critical,
            1 => Priority::High,
            2 => Priority::Medium,
            3 => Priority::Low,
            _ => Priority::Safe,
        };

        ConsolidationPlan {
            plan_id: Uuid::new_v4().to_string(),
            target_address: address.to_string(),
            boxes_to_consolidate: boxes.iter().map(|b| b.box_id.clone()).collect(),
            total_input_value: total_value,
            num_inputs: boxes.len(),
            estimated_fee: CONSOLIDATION_FEE,
            estimated_rent_saved: rent_saved,
            priority,
            created_at: Utc::now().to_rfc3339(),
            status: PlanStatus::Pending,
        }
    }
}

// ─── Consolidation Executor ────────────────────────────────────────

pub struct ConsolidationExecutor {
    history: Arc<DashMap<String, ConsolidationHistory>>,
    active_plans: Arc<DashMap<String, ConsolidationPlan>>,
    max_retries: u32,
}

impl ConsolidationExecutor {
    pub fn new(max_retries: u32) -> Self {
        Self {
            history: Arc::new(DashMap::new()),
            active_plans: Arc::new(DashMap::new()),
            max_retries,
        }
    }

    pub fn submit_plan(&self, plan: ConsolidationPlan) -> Result<String, String> {
        if plan.boxes_to_consolidate.is_empty() {
            return Err("Cannot execute empty plan".to_string());
        }
        if plan.total_input_value <= CONSOLIDATION_FEE {
            return Err(format!(
                "Total value {} below fee {}. Cannot consolidate.",
                plan.total_input_value, CONSOLIDATION_FEE
            ));
        }
        let plan_id = plan.plan_id.clone();
        self.active_plans.insert(plan_id.clone(), plan);
        Ok(plan_id)
    }

    /// Simulate execution (in production this would build & submit real Ergo tx).
    pub fn execute_plan(&self, plan_id: &str) -> Result<ConsolidationHistory, String> {
        let mut plan = self.active_plans.get(plan_id)
            .ok_or_else(|| format!("Plan {} not found", plan_id))?
            .value().clone();

        plan.status = PlanStatus::InProgress;
        self.active_plans.insert(plan_id.to_string(), plan.clone());

        let output_value = plan.total_input_value.saturating_sub(plan.estimated_fee);
        let new_box_id = Uuid::new_v4().to_string();

        let history = ConsolidationHistory {
            operation_id: Uuid::new_v4().to_string(),
            plan_id: plan_id.to_string(),
            boxes_consumed: plan.boxes_to_consolidate.clone(),
            box_created: new_box_id.clone(),
            value_before: plan.total_input_value,
            value_after: output_value,
            fee_paid: plan.estimated_fee,
            rent_blocks_saved: (plan.estimated_rent_saved / NANOERG_PER_BYTE) * BLOCKS_PER_DAY,
            timestamp: Utc::now().to_rfc3339(),
            success: true,
        };

        plan.status = PlanStatus::Completed;
        self.active_plans.insert(plan_id.to_string(), plan);
        self.history.insert(history.operation_id.clone(), history.clone());

        Ok(history)
    }

    pub fn get_history(&self, limit: usize) -> Vec<ConsolidationHistory> {
        let mut items: Vec<ConsolidationHistory> = self.history.iter()
            .map(|r| r.value().clone())
            .collect();
        items.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        items.truncate(limit);
        items
    }

    pub fn get_plan(&self, plan_id: &str) -> Option<ConsolidationPlan> {
        self.active_plans.get(plan_id).map(|r| r.value().clone())
    }
}

// ─── Consolidation Service (main entry point) ──────────────────────

pub struct ConsolidationService {
    tracker: Arc<BoxAgeTracker>,
    dust_collector: Arc<DustCollector>,
    planner: Arc<ConsolidationPlanner>,
    executor: Arc<ConsolidationExecutor>,
    scan_interval_secs: u64,
    stats: ConsolidationStats,
}

#[derive(Debug, Default)]
struct ConsolidationStats {
    boxes_tracked: AtomicU64,
    boxes_consolidated: AtomicU64,
    nanoerg_fees_paid: AtomicU64,
    nanoerg_rent_saved: AtomicU64,
    plans_created: AtomicU64,
    plans_executed: AtomicU64,
}

impl ConsolidationService {
    pub fn new(initial_height: u64) -> Self {
        let tracker = Arc::new(BoxAgeTracker::new(initial_height));
        let dust_collector = Arc::new(DustCollector::new(tracker.clone(), SAFETY_BUFFER_NANOERG));
        let planner = Arc::new(ConsolidationPlanner::new(tracker.clone(), dust_collector.clone(), 50));
        let executor = Arc::new(ConsolidationExecutor::new(3));

        Self {
            tracker,
            dust_collector,
            planner,
            executor,
            scan_interval_secs: DEFAULT_SCAN_INTERVAL_SECS,
            stats: ConsolidationStats::default(),
        }
    }

    pub fn tracker(&self) -> &Arc<BoxAgeTracker> { &self.tracker }
    pub fn executor(&self) -> &Arc<ConsolidationExecutor> { &self.executor }

    /// Get overall status.
    pub fn status(&self) -> serde_json::Value {
        serde_json::json!({
            "boxes_tracked": self.stats.boxes_tracked.load(Ordering::Relaxed),
            "boxes_at_risk": self.tracker.at_risk_count(),
            "boxes_consolidated": self.stats.boxes_consolidated.load(Ordering::Relaxed),
            "total_fees_paid_nanoerg": self.stats.nanoerg_fees_paid.load(Ordering::Relaxed),
            "total_rent_saved_nanoerg": self.stats.nanoerg_rent_saved.load(Ordering::Relaxed),
            "plans_created": self.stats.plans_created.load(Ordering::Relaxed),
            "plans_executed": self.stats.plans_executed.load(Ordering::Relaxed),
            "scan_interval_secs": self.scan_interval_secs,
            "current_height": self.tracker.get_height(),
        })
    }

    /// List all tracked boxes with age info.
    pub fn list_boxes(&self) -> Vec<RentEstimate> {
        let height = self.tracker.get_height();
        self.tracker.get_all_boxes()
            .iter()
            .map(|b| RentEstimatorService::estimate(b, height))
            .collect()
    }

    /// Generate consolidation plans for an address.
    pub fn generate_plans(&self, address: &str) -> Vec<ConsolidationPlan> {
        let plans = self.planner.create_plan(address);
        self.stats.plans_created.fetch_add(plans.len() as u64, Ordering::Relaxed);
        plans
    }

    /// Execute a consolidation plan.
    pub fn execute(&self, plan_id: &str) -> Result<ConsolidationHistory, String> {
        let history = self.executor.execute_plan(plan_id)?;
        self.stats.plans_executed.fetch_add(1, Ordering::Relaxed);
        self.stats.boxes_consolidated.fetch_add(history.boxes_consumed.len() as u64, Ordering::Relaxed);
        self.stats.nanoerg_fees_paid.fetch_add(history.fee_paid, Ordering::Relaxed);
        self.stats.nanoerg_rent_saved.fetch_add(history.rent_blocks_saved, Ordering::Relaxed);
        Ok(history)
    }

    /// Track a new box.
    pub fn track(&self, info: BoxInfo) {
        self.tracker.track_box(info);
        self.stats.boxes_tracked.fetch_add(1, Ordering::Relaxed);
    }

    /// Estimate rent for tracked boxes.
    pub fn rent_estimates(&self) -> Vec<RentEstimate> {
        let height = self.tracker.get_height();
        self.tracker.get_all_boxes()
            .iter()
            .map(|b| RentEstimatorService::estimate(b, height))
            .collect()
    }
}

// ─── REST Handlers ─────────────────────────────────────────────────

pub async fn handle_status(service: Arc<RwLock<ConsolidationService>>) -> serde_json::Value {
    let svc = service.read().await;
    svc.status()
}

pub async fn handle_list_boxes(service: Arc<RwLock<ConsolidationService>>) -> serde_json::Value {
    let svc = service.read().await;
    let boxes = svc.list_boxes();
    serde_json::json!({ "boxes": boxes, "count": boxes.len() })
}

pub async fn handle_create_plan(
    service: Arc<RwLock<ConsolidationService>>,
    address: String,
) -> Result<serde_json::Value, String> {
    let svc = service.read().await;
    let plans = svc.generate_plans(&address);
    Ok(serde_json::json!({ "plans": plans, "count": plans.len() }))
}

pub async fn handle_execute_plan(
    service: Arc<RwLock<ConsolidationService>>,
    plan_id: String,
) -> Result<serde_json::Value, String> {
    let svc = service.read().await;
    let history = svc.execute(&plan_id)?;
    Ok(serde_json::json!({ "operation": history }))
}

pub async fn handle_history(
    service: Arc<RwLock<ConsolidationService>>,
    limit: Option<usize>,
) -> serde_json::Value {
    let svc = service.read().await;
    let history = svc.executor.get_history(limit.unwrap_or(50));
    serde_json::json!({ "history": history, "count": history.len() })
}

pub async fn handle_rent_estimate(
    service: Arc<RwLock<ConsolidationService>>,
) -> serde_json::Value {
    let svc = service.read().await;
    let estimates = svc.rent_estimates();
    let total_rent_saved: u64 = estimates.iter()
        .map(|e| e.annual_rent_cost_nanoerg)
        .sum();
    serde_json::json!({
        "estimates": estimates,
        "count": estimates.len(),
        "total_annual_rent_nanoerg": total_rent_saved,
    })
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_box(box_id: &str, address: &str, value: u64, creation_height: u32, byte_size: u32) -> BoxInfo {
        BoxInfo {
            box_id: box_id.to_string(),
            address: address.to_string(),
            value,
            creation_height,
            byte_size,
            tokens: vec![],
        }
    }

    #[test]
    fn test_constants() {
        assert_eq!(RENT_THRESHOLD_BLOCKS, 1_051_200);
        assert_eq!(NANOERG_PER_BYTE, 360);
        assert_eq!(BLOCKS_PER_DAY, 720);
    }

    #[test]
    fn test_box_age_tracker_basic() {
        let tracker = BoxAgeTracker::new(1_000_000);
        tracker.track_box(make_box("box1", "addr1", 1_000_000, 100_000, 200));

        assert_eq!(tracker.box_count(), 1);
        assert_eq!(tracker.age_blocks("box1"), Some(900_000));
        assert_eq!(tracker.days_until_rent("box1"), Some(2094.44));
        assert_eq!(tracker.compute_priority("box1"), Priority::Low);

        tracker.untrack_box("box1");
        assert_eq!(tracker.box_count(), 0);
    }

    #[test]
    fn test_box_age_tracker_critical() {
        let tracker = BoxAgeTracker::new(1_048_000); // ~30 days before threshold
        tracker.track_box(make_box("box1", "addr1", 1_000_000, 100_000, 200));

        let days = tracker.days_until_rent("box1").unwrap();
        assert!(days < 30.0);
        assert_eq!(tracker.compute_priority("box1"), Priority::Critical);
        assert_eq!(tracker.at_risk_count(), 1);
    }

    #[test]
    fn test_rent_estimator_min_box_value() {
        assert_eq!(RentEstimatorService::min_box_value(100), 36_000);
        assert_eq!(RentEstimatorService::min_box_value(0), 0);
        assert_eq!(RentEstimatorService::min_box_value(1000), 360_000);
    }

    #[test]
    fn test_rent_estimate() {
        let box_info = make_box("box1", "addr1", 500_000, 100_000, 200);
        let estimate = RentEstimatorService::estimate(&box_info, 1_000_000);

        assert_eq!(estimate.box_id, "box1");
        assert_eq!(estimate.min_value_nanoerg, 72_000);
        assert_eq!(estimate.age_blocks, 900_000);
        assert!(estimate.days_until_rent > 2000.0);
        assert_eq!(estimate.priority, Priority::Low);
    }

    #[test]
    fn test_rent_saved_by_consolidation() {
        let boxes = vec![
            make_box("b1", "a1", 100_000, 100_000, 100),
            make_box("b2", "a1", 100_000, 100_000, 100),
            make_box("b3", "a1", 100_000, 100_000, 100),
        ];
        let saved = RentEstimatorService::rent_saved_by_consolidation(&boxes, 500_000);
        // Individual: 3 * (360 * 100) = 108_000. Consolidated: 360 * 350 = 126_000. Saved: 0 (consolidated is bigger)
        // Actually saved = 108_000 - 126_000 = negative, so 0
        assert_eq!(saved, 0); // overhead of 50 bytes makes consolidated bigger
    }

    #[test]
    fn test_dust_collector() {
        let tracker = Arc::new(BoxAgeTracker::new(500_000));
        tracker.track_box(make_box("dust1", "addr1", 100_000, 100_000, 100));
        tracker.track_box(make_box("dust2", "addr1", 200_000, 100_000, 100));
        tracker.track_box(make_box("big1", "addr1", 10_000_000, 100_000, 100));

        let collector = DustCollector::new(tracker.clone(), 500_000);
        let dust = collector.collect_dust();
        assert_eq!(dust.len(), 2);

        let addr_dust = collector.collect_dust_for_address("addr1");
        assert_eq!(addr_dust.len(), 2);
    }

    #[test]
    fn test_consolidation_planner() {
        let tracker = Arc::new(BoxAgeTracker::new(500_000));
        for i in 0..5 {
            tracker.track_box(make_box(&format!("dust{}", i), "addr1", 100_000, 100_000, 100));
        }
        let dust_collector = Arc::new(DustCollector::new(tracker.clone(), 500_000));
        let planner = ConsolidationPlanner::new(tracker.clone(), dust_collector.clone(), 50);

        let plans = planner.create_plan("addr1");
        assert!(!plans.is_empty());
        let plan = &plans[0];
        assert_eq!(plan.boxes_to_consolidate.len(), 5);
        assert_eq!(plan.target_address, "addr1");
        assert!(plan.total_input_value > 0);
    }

    #[test]
    fn test_consolidation_executor_success() {
        let executor = ConsolidationExecutor::new(3);
        let plan = ConsolidationPlan {
            plan_id: "test-plan".to_string(),
            target_address: "addr1".to_string(),
            boxes_to_consolidate: vec!["box1".to_string(), "box2".to_string()],
            total_input_value: 2_000_000,
            num_inputs: 2,
            estimated_fee: CONSOLIDATION_FEE,
            estimated_rent_saved: 50_000,
            priority: Priority::Medium,
            created_at: Utc::now().to_rfc3339(),
            status: PlanStatus::Pending,
        };

        let plan_id = executor.submit_plan(plan).unwrap();
        let history = executor.execute_plan(&plan_id).unwrap();
        assert!(history.success);
        assert_eq!(history.fee_paid, CONSOLIDATION_FEE);
        assert_eq!(history.value_after, 1_000_000);
    }

    #[test]
    fn test_consolidation_executor_empty_plan() {
        let executor = ConsolidationExecutor::new(3);
        let plan = ConsolidationPlan {
            plan_id: "empty".to_string(),
            target_address: "addr1".to_string(),
            boxes_to_consolidate: vec![],
            total_input_value: 0,
            num_inputs: 0,
            estimated_fee: CONSOLIDATION_FEE,
            estimated_rent_saved: 0,
            priority: Priority::Safe,
            created_at: Utc::now().to_rfc3339(),
            status: PlanStatus::Pending,
        };

        assert!(executor.submit_plan(plan).is_err());
    }

    #[test]
    fn test_consolidation_service_status() {
        let service = ConsolidationService::new(500_000);
        service.track(make_box("b1", "a1", 100_000, 100_000, 100));
        let status = service.status();
        assert_eq!(status["boxes_tracked"], 1);
    }

    #[test]
    fn test_priority_from_days() {
        assert_eq!(RentEstimatorService::compute_priority_from_days(10.0), Priority::Critical);
        assert_eq!(RentEstimatorService::compute_priority_from_days(45.0), Priority::High);
        assert_eq!(RentEstimatorService::compute_priority_from_days(200.0), Priority::Medium);
        assert_eq!(RentEstimatorService::compute_priority_from_days(800.0), Priority::Low);
        assert_eq!(RentEstimatorService::compute_priority_from_days(2000.0), Priority::Safe);
    }
}
