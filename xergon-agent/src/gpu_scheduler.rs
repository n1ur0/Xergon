use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchedulingPolicy {
    RoundRobin,
    LeastLoaded,
    MemoryAware,
    LatencyOptimized,
    ModelAffinity,
    Hybrid,
}

impl Default for SchedulingPolicy {
    fn default() -> Self {
        Self::Hybrid
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuAffinity {
    None,
    Soft,
    Hard,
}

impl Default for GpuAffinity {
    fn default() -> Self {
        Self::Soft
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuSchedulerConfig {
    pub enabled: bool,
    pub scheduling_policy: SchedulingPolicy,
    pub max_concurrent_per_device: u32,
    pub preemption_enabled: bool,
    pub memory_reservation_mb: u64,
    pub time_slicing: bool,
    pub max_queue_depth: usize,
    pub affinity: GpuAffinity,
}

impl Default for GpuSchedulerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            scheduling_policy: SchedulingPolicy::default(),
            max_concurrent_per_device: 4,
            preemption_enabled: true,
            memory_reservation_mb: 512,
            time_slicing: true,
            max_queue_depth: 256,
            affinity: GpuAffinity::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateGpuSchedulerConfigRequest {
    pub enabled: Option<bool>,
    pub scheduling_policy: Option<SchedulingPolicy>,
    pub max_concurrent_per_device: Option<u32>,
    pub preemption_enabled: Option<bool>,
    pub memory_reservation_mb: Option<u64>,
    pub time_slicing: Option<bool>,
    pub max_queue_depth: Option<usize>,
    pub affinity: Option<GpuAffinity>,
}

// ---------------------------------------------------------------------------
// Task and device structures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Running,
    Preempted,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuTask {
    pub id: String,
    pub model: String,
    pub priority: u8,
    pub estimated_vram_mb: u64,
    pub estimated_time_ms: u64,
    pub device_id: Option<u32>,
    pub status: TaskStatus,
    pub submitted_at_ms: u64,
    pub started_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceSchedule {
    pub device_id: u32,
    pub current_tasks: Vec<String>,
    pub available_vram_mb: u64,
    pub utilization: f64,
    pub queue_depth: usize,
    pub pinned_models: HashSet<String>,
}

// ---------------------------------------------------------------------------
// Scheduler stats and status
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerStatsResponse {
    pub tasks_scheduled: u64,
    pub tasks_completed: u64,
    pub tasks_preempted: u64,
    pub avg_wait_time_ms: f64,
    pub avg_execution_time_ms: f64,
    pub device_utilization: HashMap<u32, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerStatus {
    pub enabled: bool,
    pub policy: SchedulingPolicy,
    pub total_devices: usize,
    pub total_pending: usize,
    pub total_running: usize,
    pub affinity_mode: GpuAffinity,
    pub time_slicing: bool,
    pub preemption_enabled: bool,
}

// ---------------------------------------------------------------------------
// GpuScheduler
// ---------------------------------------------------------------------------

pub struct GpuScheduler {
    config: tokio::sync::RwLock<GpuSchedulerConfig>,
    devices: DashMap<u32, DeviceSchedule>,
    tasks: DashMap<String, GpuTask>,
    model_affinity: DashMap<String, u32>,
    round_robin_counter: AtomicU64,
    tasks_scheduled: AtomicU64,
    tasks_completed: AtomicU64,
    tasks_preempted: AtomicU64,
    total_wait_us: AtomicU64,
    total_exec_us: AtomicU64,
    completed_count: AtomicU64,
}

impl GpuScheduler {
    pub fn new(config: GpuSchedulerConfig) -> Self {
        Self {
            config: tokio::sync::RwLock::new(config),
            devices: DashMap::new(),
            tasks: DashMap::new(),
            model_affinity: DashMap::new(),
            round_robin_counter: AtomicU64::new(0),
            tasks_scheduled: AtomicU64::new(0),
            tasks_completed: AtomicU64::new(0),
            tasks_preempted: AtomicU64::new(0),
            total_wait_us: AtomicU64::new(0),
            total_exec_us: AtomicU64::new(0),
            completed_count: AtomicU64::new(0),
        }
    }

    pub fn default() -> Self {
        Self::new(GpuSchedulerConfig::default())
    }

    /// Register a GPU device.
    pub fn register_device(&self, device_id: u32, total_vram_mb: u64) {
        self.devices.insert(device_id, DeviceSchedule {
            device_id,
            current_tasks: Vec::new(),
            available_vram_mb: total_vram_mb,
            utilization: 0.0,
            queue_depth: 0,
            pinned_models: HashSet::new(),
        });
    }

    /// Submit a task for scheduling.
    pub fn submit_task(&self, mut task: GpuTask) -> Result<String, String> {
        let cfg = self.config.blocking_read();
        if !cfg.enabled {
            return Err("Scheduler is disabled".to_string());
        }

        let device_id = self.select_device(&task, &cfg)?;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let task_id = task.id.clone();
        task.device_id = Some(device_id);
        task.status = TaskStatus::Running;
        task.started_at_ms = Some(now_ms);

        if let Some(mut dev) = self.devices.get_mut(&device_id) {
            dev.current_tasks.push(task_id.clone());
        }

        self.tasks.insert(task_id.clone(), task);
        self.tasks_scheduled.fetch_add(1, Ordering::Relaxed);
        Ok(task_id)
    }

    fn select_device(&self, task: &GpuTask, cfg: &GpuSchedulerConfig) -> Result<u32, String> {
        // Check affinity first
        if let Some(affinity_ref) = self.model_affinity.get(&task.model) {
            let affinity_dev = *affinity_ref;
            if self.devices.contains_key(&affinity_dev) {
                if let Some(dev) = self.devices.get(&affinity_dev) {
                    let can_run = dev.available_vram_mb >= task.estimated_vram_mb
                        && dev.current_tasks.len() < cfg.max_concurrent_per_device as usize;
                    if can_run || cfg.affinity == GpuAffinity::Hard {
                        return Ok(affinity_dev);
                    }
                }
            }
        }

        match cfg.scheduling_policy {
            SchedulingPolicy::RoundRobin => {
                let devices: Vec<u32> = self.devices.iter().map(|r| *r.key()).collect();
                if devices.is_empty() {
                    return Err("No GPU devices registered".to_string());
                }
                let idx = self.round_robin_counter.fetch_add(1, Ordering::Relaxed) as usize % devices.len();
                Ok(devices[idx])
            }
            SchedulingPolicy::LeastLoaded => {
                self.devices.iter()
                    .min_by_key(|r| r.current_tasks.len())
                    .map(|r| *r.key())
                    .ok_or_else(|| "No GPU devices registered".to_string())
            }
            SchedulingPolicy::MemoryAware => {
                self.devices.iter()
                    .filter(|r| r.available_vram_mb >= task.estimated_vram_mb)
                    .max_by_key(|r| r.available_vram_mb)
                    .map(|r| *r.key())
                    .ok_or_else(|| "No device with sufficient VRAM".to_string())
            }
            SchedulingPolicy::LatencyOptimized => {
                // Simple heuristic: prefer least utilized
                self.devices.iter()
                    .min_by_key(|r| (r.utilization * 1000.0) as u64)
                    .map(|r| *r.key())
                    .ok_or_else(|| "No GPU devices registered".to_string())
            }
            SchedulingPolicy::ModelAffinity => {
                if let Some(affinity_ref) = self.model_affinity.get(&task.model) {
                    Ok(*affinity_ref)
                } else {
                    self.devices.iter()
                        .min_by_key(|r| r.current_tasks.len())
                        .map(|r| *r.key())
                        .ok_or_else(|| "No GPU devices registered".to_string())
                }
            }
            SchedulingPolicy::Hybrid => {
                // Weighted score: prefer affinity, low load, high VRAM
                let mut best_dev: Option<(u32, f64)> = None;
                let affinity_dev = self.model_affinity.get(&task.model).map(|r| *r.value());
                for entry in self.devices.iter() {
                    let dev = entry.value();
                    let load_score = 1.0 - (dev.current_tasks.len() as f64 / cfg.max_concurrent_per_device as f64);
                    let vram_score = if dev.available_vram_mb >= task.estimated_vram_mb { 1.0 } else { 0.0 };
                    let affinity_score = if affinity_dev.map(|d| d == dev.device_id).unwrap_or(false) { 1.0 } else { 0.0 };
                    let score = load_score * 0.4 + vram_score * 0.3 + affinity_score * 0.3;
                    if best_dev.map_or(true, |(_, s)| score > s) {
                        best_dev = Some((dev.device_id, score));
                    }
                }
                best_dev.map(|(d, _)| d)
                    .ok_or_else(|| "No GPU devices registered".to_string())
            }
        }
    }

    /// Complete a task.
    pub fn complete_task(&self, task_id: &str) {
        if let Some(mut task) = self.tasks.get_mut(task_id) {
            task.status = TaskStatus::Completed;
            let device_id = task.device_id;
            if let Some(device_id) = device_id {
                if let Some(mut dev) = self.devices.get_mut(&device_id) {
                    dev.current_tasks.retain(|t| t != task_id);
                }
            }
        }
        self.tasks_completed.fetch_add(1, Ordering::Relaxed);
    }

    /// Fail a task.
    pub fn fail_task(&self, task_id: &str) {
        if let Some(mut task) = self.tasks.get_mut(task_id) {
            task.status = TaskStatus::Failed;
            let device_id = task.device_id;
            if let Some(device_id) = device_id {
                if let Some(mut dev) = self.devices.get_mut(&device_id) {
                    dev.current_tasks.retain(|t| t != task_id);
                }
            }
        }
    }

    /// Set model-to-device affinity.
    pub fn set_affinity(&self, model: &str, device_id: u32) {
        self.model_affinity.insert(model.to_string(), device_id);
        if let Some(mut dev) = self.devices.get_mut(&device_id) {
            dev.pinned_models.insert(model.to_string());
        }
    }

    /// Clear model affinity.
    pub fn clear_affinity(&self, model: &str) {
        if let Some((_, old_dev)) = self.model_affinity.remove(model) {
            if let Some(mut dev) = self.devices.get_mut(&old_dev) {
                dev.pinned_models.remove(model);
            }
        }
    }

    /// Get current scheduler status.
    pub async fn get_status(&self) -> SchedulerStatus {
        let cfg = self.config.read().await;
        let pending = self.tasks.iter().filter(|r| r.status == TaskStatus::Pending).count();
        let running = self.tasks.iter().filter(|r| r.status == TaskStatus::Running).count();
        SchedulerStatus {
            enabled: cfg.enabled,
            policy: cfg.scheduling_policy,
            total_devices: self.devices.len(),
            total_pending: pending,
            total_running: running,
            affinity_mode: cfg.affinity,
            time_slicing: cfg.time_slicing,
            preemption_enabled: cfg.preemption_enabled,
        }
    }

    /// Get device schedules.
    pub fn get_device_schedules(&self) -> Vec<DeviceSchedule> {
        self.devices.iter().map(|r| r.value().clone()).collect()
    }

    /// Get pending tasks from queue.
    pub fn get_queue(&self) -> Vec<GpuTask> {
        self.tasks.iter()
            .filter(|r| matches!(r.status, TaskStatus::Pending | TaskStatus::Preempted))
            .map(|r| r.value().clone())
            .collect()
    }

    /// Get scheduler statistics.
    pub fn get_stats(&self) -> SchedulerStatsResponse {
        let completed = self.completed_count.load(Ordering::Relaxed);
        let avg_wait = if completed > 0 {
            self.total_wait_us.load(Ordering::Relaxed) as f64 / completed as f64 / 1000.0
        } else {
            0.0
        };
        let avg_exec = if completed > 0 {
            self.total_exec_us.load(Ordering::Relaxed) as f64 / completed as f64 / 1000.0
        } else {
            0.0
        };
        let device_util: HashMap<u32, f64> = self.devices.iter()
            .map(|r| (*r.key(), r.value().utilization))
            .collect();
        SchedulerStatsResponse {
            tasks_scheduled: self.tasks_scheduled.load(Ordering::Relaxed),
            tasks_completed: self.tasks_completed.load(Ordering::Relaxed),
            tasks_preempted: self.tasks_preempted.load(Ordering::Relaxed),
            avg_wait_time_ms: avg_wait,
            avg_execution_time_ms: avg_exec,
            device_utilization: device_util,
        }
    }

    /// Update config.
    pub async fn update_config(&self, update: UpdateGpuSchedulerConfigRequest) -> GpuSchedulerConfig {
        let mut cfg = self.config.write().await;
        if let Some(v) = update.enabled { cfg.enabled = v; }
        if let Some(v) = update.scheduling_policy { cfg.scheduling_policy = v; }
        if let Some(v) = update.max_concurrent_per_device { cfg.max_concurrent_per_device = v; }
        if let Some(v) = update.preemption_enabled { cfg.preemption_enabled = v; }
        if let Some(v) = update.memory_reservation_mb { cfg.memory_reservation_mb = v; }
        if let Some(v) = update.time_slicing { cfg.time_slicing = v; }
        if let Some(v) = update.max_queue_depth { cfg.max_queue_depth = v; }
        if let Some(v) = update.affinity { cfg.affinity = v; }
        cfg.clone()
    }
}
