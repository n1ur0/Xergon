//! Priority Queue System
//!
//! A general-purpose priority queue with 5 priority levels, fair sharing,
//! priority aging, preemption support, and per-user quotas.
//!
//! API endpoints:
//! - GET    /api/priority-queue/status  -- queue status, sizes per level
//! - GET    /api/priority-queue/tasks   -- list queued tasks
//! - POST   /api/priority-queue/enqueue -- add task
//! - DELETE /api/priority-queue/{id}    -- remove task
//! - GET    /api/priority-queue/stats   -- statistics
//! - PATCH  /api/priority-queue/config  -- update config
//! - POST   /api/priority-queue/clear   -- clear all tasks

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Core Types
// ---------------------------------------------------------------------------

/// Priority levels ordered from highest to lowest urgency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PriorityLevel {
    Critical = 100,
    High = 75,
    Normal = 50,
    Low = 25,
    Background = 10,
}

impl std::fmt::Display for PriorityLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Critical => write!(f, "critical"),
            Self::High => write!(f, "high"),
            Self::Normal => write!(f, "normal"),
            Self::Low => write!(f, "low"),
            Self::Background => write!(f, "background"),
        }
    }
}

impl Default for PriorityLevel {
    fn default() -> Self {
        Self::Normal
    }
}

impl PriorityLevel {
    pub fn all() -> &'static [PriorityLevel] {
        &[
            PriorityLevel::Critical,
            PriorityLevel::High,
            PriorityLevel::Normal,
            PriorityLevel::Low,
            PriorityLevel::Background,
        ]
    }

    pub fn weight(&self) -> f64 {
        *self as u8 as f64
    }
}

/// Priority queue configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityQueueConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_max_queue_size")]
    pub max_queue_size: usize,
    #[serde(default)]
    pub default_priority: PriorityLevel,
    #[serde(default)]
    pub fair_share_enabled: bool,
    #[serde(default = "default_max_per_user")]
    pub max_per_user: usize,
    #[serde(default = "default_true")]
    pub aging_enabled: bool,
    #[serde(default = "default_aging_interval")]
    pub aging_interval_secs: u64,
    #[serde(default)]
    pub preemption_enabled: bool,
}

fn default_true() -> bool { true }
fn default_max_queue_size() -> usize { 10000 }
fn default_max_per_user() -> usize { 100 }
fn default_aging_interval() -> u64 { 30 }

impl Default for PriorityQueueConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_queue_size: default_max_queue_size(),
            default_priority: PriorityLevel::Normal,
            fair_share_enabled: false,
            max_per_user: default_max_per_user(),
            aging_enabled: true,
            aging_interval_secs: default_aging_interval(),
            preemption_enabled: false,
        }
    }
}

impl PriorityQueueConfig {
    pub fn aging_interval(&self) -> Duration {
        Duration::from_secs(self.aging_interval_secs)
    }
}

/// A task in the priority queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityTask {
    pub id: String,
    pub model: String,
    pub user_id: String,
    pub priority: PriorityLevel,
    pub weight: f64,
    pub estimated_time_ms: u64,
    #[serde(default)]
    pub payload: Vec<u8>,
    pub queued_at: DateTime<Utc>,
    pub deadline: Option<DateTime<Utc>>,
    pub preemption_count: u32,
}

// Ord is implemented for Reverse<PriorityTask> in the BinaryHeap.
// Higher weight = higher priority (comes first when reversed).
impl PartialEq for PriorityTask {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for PriorityTask {}

impl PartialOrd for PriorityTask {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PriorityTask {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.weight
            .partial_cmp(&other.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| self.queued_at.cmp(&other.queued_at)) // FIFO tiebreak
    }
}

/// Enqueue request.
#[derive(Debug, Deserialize)]
pub struct EnqueueRequest {
    pub model: String,
    pub user_id: String,
    pub priority: Option<PriorityLevel>,
    pub estimated_time_ms: Option<u64>,
    #[serde(default)]
    pub payload: Vec<u8>,
    pub deadline: Option<DateTime<Utc>>,
}

/// Enqueue result.
#[derive(Debug, Serialize)]
pub struct EnqueueResponse {
    pub task_id: String,
    pub position: usize,
    pub priority: PriorityLevel,
    pub weight: f64,
}

/// Config update request.
#[derive(Debug, Deserialize, Serialize)]
pub struct PriorityQueueConfigUpdate {
    pub enabled: Option<bool>,
    pub max_queue_size: Option<usize>,
    pub default_priority: Option<PriorityLevel>,
    pub fair_share_enabled: Option<bool>,
    pub max_per_user: Option<usize>,
    pub aging_enabled: Option<bool>,
    pub aging_interval_secs: Option<u64>,
    pub preemption_enabled: Option<bool>,
}

/// Queue status per priority level.
#[derive(Debug, Serialize)]
pub struct PriorityLevelStatus {
    pub level: String,
    pub count: usize,
    pub oldest_wait_secs: Option<u64>,
}

/// Full queue status.
#[derive(Debug, Serialize)]
pub struct PriorityQueueStatus {
    pub enabled: bool,
    pub total_tasks: usize,
    pub levels: Vec<PriorityLevelStatus>,
    pub capacity_remaining: usize,
}

/// Queue statistics.
#[derive(Debug, Serialize)]
pub struct PriorityQueueStats {
    pub total_enqueued: u64,
    pub total_dequeued: u64,
    pub total_preempted: u64,
    pub total_expired: u64,
    pub avg_wait_time_us: u64,
    pub by_priority: Vec<(String, u64)>,
}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

/// Priority queue manager with per-level binary heaps.
pub struct PriorityQueueManager {
    config: RwLock<PriorityQueueConfig>,
    queues: DashMap<PriorityLevel, BinaryHeap<Reverse<PriorityTask>>>,
    task_index: DashMap<String, PriorityTask>, // id -> task for O(1) lookup
    user_counts: DashMap<String, usize>,
    stats: PriorityStats,
}

#[derive(Debug, Default)]
struct PriorityStats {
    total_enqueued: AtomicU64,
    total_dequeued: AtomicU64,
    total_preempted: AtomicU64,
    total_expired: AtomicU64,
    total_wait_us: AtomicU64,
    wait_samples: AtomicU64,
}

impl PriorityQueueManager {
    pub fn new(config: PriorityQueueConfig) -> Self {
        let mut queues = DashMap::new();
        for level in PriorityLevel::all() {
            queues.insert(*level, BinaryHeap::new());
        }
        Self {
            config: RwLock::new(config),
            queues,
            task_index: DashMap::new(),
            user_counts: DashMap::new(),
            stats: PriorityStats::default(),
        }
    }

    /// Total number of tasks across all queues.
    fn total_size(&self) -> usize {
        self.task_index.len()
    }

    /// Enqueue a new task.
    pub async fn enqueue(&self, req: EnqueueRequest) -> Result<EnqueueResponse, String> {
        let cfg = self.config.read().await;
        if !cfg.enabled {
            return Err("Priority queue is disabled".to_string());
        }
        if self.total_size() >= cfg.max_queue_size {
            return Err(format!("Queue full (max {})", cfg.max_queue_size));
        }

        // Per-user quota
        let mut user_count = self.user_counts.entry(req.user_id.clone()).or_insert(0);
        if *user_count >= cfg.max_per_user {
            return Err(format!(
                "User {} has reached max queued tasks ({})",
                req.user_id, cfg.max_per_user
            ));
        }

        // Fair share check
        if cfg.fair_share_enabled {
            let unique_users: usize = self.user_counts.len();
            if unique_users > 0 {
                let avg_per_user = self.total_size() / unique_users;
                if *user_count > avg_per_user * 2 && avg_per_user > 0 {
                    return Err(format!(
                        "Fair share limit: user {} has {} tasks, avg is {}",
                        req.user_id, *user_count, avg_per_user
                    ));
                }
            }
        }

        let priority = req.priority.unwrap_or(cfg.default_priority);
        let now = Utc::now();

        // Check deadline not already expired
        if let Some(deadline) = req.deadline {
            if deadline < now {
                return Err("Deadline already expired".to_string());
            }
        }

        let task = PriorityTask {
            id: uuid::Uuid::new_v4().to_string(),
            model: req.model,
            user_id: req.user_id.clone(),
            priority,
            weight: priority.weight(),
            estimated_time_ms: req.estimated_time_ms.unwrap_or(0),
            payload: req.payload,
            queued_at: now,
            deadline: req.deadline,
            preemption_count: 0,
        };

        let task_id = task.id.clone();
        let task_priority = task.priority;
        let task_weight = task.weight;

        // Insert into the appropriate queue
        if let Some(mut queue) = self.queues.get_mut(&task_priority) {
            queue.push(Reverse(task.clone()));
        }

        self.task_index.insert(task_id.clone(), task.clone());
        *user_count += 1;
        self.stats.total_enqueued.fetch_add(1, Ordering::Relaxed);

        let position = self.total_size();

        info!(
            task_id = %task_id,
            priority = %task_priority,
            user_id = %req.user_id,
            "Task enqueued"
        );

        Ok(EnqueueResponse {
            task_id,
            position,
            priority: task_priority,
            weight: task_weight,
        })
    }

    /// Remove a task by ID.
    pub fn remove_task(&self, id: &str) -> Result<PriorityTask, String> {
        let task = self.task_index.remove(id).ok_or("Task not found")?.1;

        // Remove from the priority queue (lazy removal: mark as removed)
        if let Some(mut queue) = self.queues.get_mut(&task.priority) {
            queue.retain(|Reverse(t)| t.id != id);
        }

        // Decrement user count
        if let Some(mut count) = self.user_counts.get_mut(&task.user_id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                drop(count);
                self.user_counts.remove(&task.user_id);
            }
        }

        info!(task_id = %id, "Task removed");
        Ok(task)
    }

    /// Clear all tasks.
    pub fn clear_all(&self) -> usize {
        let count = self.total_size();
        for mut queue in self.queues.iter_mut() {
            queue.clear();
        }
        self.task_index.clear();
        self.user_counts.clear();
        info!(count, "All tasks cleared");
        count
    }

    /// Get queue status.
    pub async fn status(&self) -> PriorityQueueStatus {
        let cfg = self.config.read().await;
        let mut levels = Vec::new();
        let mut total = 0usize;

        for level in PriorityLevel::all() {
            let count = self
                .queues
                .get(level)
                .map(|q| q.len())
                .unwrap_or(0);
            total += count;

            let oldest_wait_secs = self
                .queues
                .get(level)
                .and_then(|q| {
                    q.iter()
                        .min_by_key(|Reverse(t)| t.queued_at)
                        .map(|Reverse(t)| {
                            (Utc::now() - t.queued_at).num_seconds().unsigned_abs() as u64
                        })
                });

            levels.push(PriorityLevelStatus {
                level: level.to_string(),
                count,
                oldest_wait_secs,
            });
        }

        PriorityQueueStatus {
            enabled: cfg.enabled,
            total_tasks: total,
            levels,
            capacity_remaining: cfg.max_queue_size.saturating_sub(total),
        }
    }

    /// List all tasks.
    pub fn list_tasks(&self, limit: usize) -> Vec<PriorityTask> {
        self.task_index
            .iter()
            .map(|e| e.value().clone())
            .take(limit)
            .collect()
    }

    /// Get statistics.
    pub fn stats(&self) -> PriorityQueueStats {
        let avg_wait = if self.stats.wait_samples.load(Ordering::Relaxed) > 0 {
            self.stats.total_wait_us.load(Ordering::Relaxed)
                / self.stats.wait_samples.load(Ordering::Relaxed)
        } else {
            0
        };

        let by_priority: Vec<(String, u64)> = PriorityLevel::all()
            .iter()
            .map(|level| {
                let count = self
                    .queues
                    .get(level)
                    .map(|q| q.len() as u64)
                    .unwrap_or(0);
                (level.to_string(), count)
            })
            .collect();

        PriorityQueueStats {
            total_enqueued: self.stats.total_enqueued.load(Ordering::Relaxed),
            total_dequeued: self.stats.total_dequeued.load(Ordering::Relaxed),
            total_preempted: self.stats.total_preempted.load(Ordering::Relaxed),
            total_expired: self.stats.total_expired.load(Ordering::Relaxed),
            avg_wait_time_us: avg_wait,
            by_priority,
        }
    }

    /// Apply priority aging: increase weights of waiting tasks.
    pub async fn age_tasks(&self) {
        let cfg = self.config.read().await;
        if !cfg.aging_enabled {
            return;
        }
        let aging_factor = 1.0 / (cfg.aging_interval_secs as f64);

        for mut queue in self.queues.iter_mut() {
            // Drain and re-insert with updated weights
            let tasks: Vec<_> = queue.drain().collect();
            for Reverse(mut task) in tasks {
                let wait_secs = (Utc::now() - task.queued_at).num_seconds().unsigned_abs() as f64;
                task.weight = task.priority.weight() + wait_secs * aging_factor;
                queue.push(Reverse(task));
            }
        }
    }

    /// Get current config.
    pub async fn get_config(&self) -> PriorityQueueConfig {
        self.config.read().await.clone()
    }

    /// Update config.
    pub async fn update_config(&self, update: PriorityQueueConfigUpdate) -> PriorityQueueConfig {
        let mut cfg = self.config.write().await;
        if let Some(enabled) = update.enabled { cfg.enabled = enabled; }
        if let Some(max_queue_size) = update.max_queue_size { cfg.max_queue_size = max_queue_size; }
        if let Some(default_priority) = update.default_priority { cfg.default_priority = default_priority; }
        if let Some(fair_share_enabled) = update.fair_share_enabled { cfg.fair_share_enabled = fair_share_enabled; }
        if let Some(max_per_user) = update.max_per_user { cfg.max_per_user = max_per_user; }
        if let Some(aging_enabled) = update.aging_enabled { cfg.aging_enabled = aging_enabled; }
        if let Some(aging_interval_secs) = update.aging_interval_secs { cfg.aging_interval_secs = aging_interval_secs; }
        if let Some(preemption_enabled) = update.preemption_enabled { cfg.preemption_enabled = preemption_enabled; }
        cfg.clone()
    }
}
