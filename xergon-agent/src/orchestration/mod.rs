//! Multi-Agent Orchestration
//!
//! Provides task scheduling, agent registration, load balancing, and
//! fan-out/fan-in orchestration for multi-agent AI compute on the Xergon network.

pub mod communication;
pub use communication::{A2AMessage, AgentLoad};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::collections::VecDeque;
use thiserror::Error;
use uuid::Uuid;
use chrono::Utc;
use tracing::{info, warn, debug};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentRole {
    Coordinator,
    Worker,
    Reviewer,
    Specialist,
    Monitor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuSpec {
    pub vram_gb: u32,
    pub gpu_type: String,
    pub compute_capability: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCapability {
    pub role: AgentRole,
    pub models: Vec<String>,
    pub max_concurrent_tasks: usize,
    pub region: String,
    pub gpu_spec: Option<GpuSpec>,
    pub reputation_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskType {
    ChatCompletion,
    CodeGeneration,
    DataAnalysis,
    ImageGeneration,
    AudioProcessing,
    BlockchainTx,
    GovernanceAction,
    HealthCheck,
    Custom(String),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    Critical = 0,
    High = 1,
    Normal = 2,
    Low = 3,
    Background = 4,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Assigned,
    Running,
    Waiting,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResult {
    pub output: serde_json::Value,
    pub tokens_used: u64,
    pub duration_ms: u64,
    pub agent_pk: String,
    pub quality_score: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub task_type: TaskType,
    pub payload: serde_json::Value,
    pub priority: TaskPriority,
    pub assigned_to: Option<String>,
    pub status: TaskStatus,
    pub created_at: chrono::DateTime<Utc>,
    pub timeout_secs: Option<u64>,
    pub max_retries: u32,
    pub retry_count: u32,
    pub parent_task_id: Option<String>,
    pub subtasks: Vec<String>,
    pub result: Option<TaskResult>,
}

impl Task {
    pub fn new(task_type: TaskType, payload: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            task_type,
            payload,
            priority: TaskPriority::Normal,
            assigned_to: None,
            status: TaskStatus::Pending,
            created_at: Utc::now(),
            timeout_secs: None,
            max_retries: 3,
            retry_count: 0,
            parent_task_id: None,
            subtasks: Vec::new(),
            result: None,
        }
    }

    pub fn with_priority(mut self, p: TaskPriority) -> Self { self.priority = p; self }
    pub fn with_timeout(mut self, secs: u64) -> Self { self.timeout_secs = Some(secs); self }
    pub fn with_max_retries(mut self, n: u32) -> Self { self.max_retries = n; self }
    pub fn with_parent(mut self, parent_id: String) -> Self { self.parent_task_id = Some(parent_id); self }
}

/// Agent registered with the orchestrator.
/// Uses `AtomicU32` for active_tasks (skip serializing, serialize manually).
#[derive(Debug)]
pub struct RegisteredAgent {
    pub pk: String,
    pub capabilities: Vec<AgentCapability>,
    pub max_tasks: usize,
    pub region: String,
    pub reputation_score: f64,
    pub last_heartbeat: chrono::DateTime<Utc>,
    pub active_tasks: AtomicU32,
}

impl Serialize for RegisteredAgent {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut out = s.serialize_struct("RegisteredAgent", 7)?;
        out.serialize_field("pk", &self.pk)?;
        out.serialize_field("capabilities", &self.capabilities)?;
        out.serialize_field("max_tasks", &self.max_tasks)?;
        out.serialize_field("region", &self.region)?;
        out.serialize_field("reputation_score", &self.reputation_score)?;
        out.serialize_field("last_heartbeat", &self.last_heartbeat)?;
        out.serialize_field("active_tasks", &self.active_tasks.load(Ordering::Relaxed))?;
        out.end()
    }
}

impl RegisteredAgent {
    /// Create a new registered agent.
    pub fn new(
        pk: String,
        capabilities: Vec<AgentCapability>,
        max_tasks: usize,
        region: String,
        reputation_score: f64,
    ) -> Self {
        Self {
            pk,
            capabilities,
            max_tasks,
            region,
            reputation_score,
            last_heartbeat: Utc::now(),
            active_tasks: AtomicU32::new(0),
        }
    }

    /// Check if the agent can accept more tasks.
    pub fn has_capacity(&self) -> bool {
        self.active_tasks.load(Ordering::Relaxed) < self.max_tasks as u32
    }

    /// Increment active task count.
    pub fn inc_tasks(&self) { self.active_tasks.fetch_add(1, Ordering::Relaxed); }

    /// Decrement active task count.
    pub fn dec_tasks(&self) {
        let prev = self.active_tasks.load(Ordering::Relaxed);
        if prev > 0 {
            self.active_tasks.fetch_sub(1, Ordering::Relaxed);
        }
    }

    /// Get a snapshot suitable for JSON (active_tasks as u32).
    pub fn snapshot(&self) -> RegisteredAgentSnapshot {
        RegisteredAgentSnapshot {
            pk: self.pk.clone(),
            capabilities: self.capabilities.clone(),
            max_tasks: self.max_tasks,
            region: self.region.clone(),
            reputation_score: self.reputation_score,
            last_heartbeat: self.last_heartbeat,
            active_tasks: self.active_tasks.load(Ordering::Relaxed),
        }
    }
}

/// Serializable snapshot of a RegisteredAgent (no AtomicU32).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredAgentSnapshot {
    pub pk: String,
    pub capabilities: Vec<AgentCapability>,
    pub max_tasks: usize,
    pub region: String,
    pub reputation_score: f64,
    pub last_heartbeat: chrono::DateTime<Utc>,
    pub active_tasks: u32,
}

// ---------------------------------------------------------------------------
// Config, Error, Stats
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationConfig {
    pub max_parallel_tasks: usize,
    pub task_timeout_secs: u64,
    pub max_retries: u32,
    pub load_balance_strategy: LoadBalanceStrategy,
    pub subtask_max_depth: u8,
}

impl Default for OrchestrationConfig {
    fn default() -> Self {
        Self {
            max_parallel_tasks: 100,
            task_timeout_secs: 300,
            max_retries: 3,
            load_balance_strategy: LoadBalanceStrategy::LeastLoaded,
            subtask_max_depth: 5,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum LoadBalanceStrategy {
    RoundRobin,
    #[default]
    LeastLoaded,
    BestFit,
    ReputationWeighted,
    GeoPreferred,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskFilter {
    pub status: Option<TaskStatus>,
    pub task_type: Option<TaskType>,
    pub assigned_to: Option<String>,
    pub priority: Option<TaskPriority>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationStats {
    pub total_tasks: usize,
    pub pending_tasks: usize,
    pub running_tasks: usize,
    pub completed_tasks: usize,
    pub failed_tasks: usize,
    pub registered_agents: usize,
}

#[derive(Error, Debug)]
pub enum OrchestrationError {
    #[error("Task not found: {0}")]
    TaskNotFound(String),
    #[error("No available agents")]
    NoAvailableAgents,
    #[error("Invalid transition: {0}")]
    InvalidTransition(String),
    #[error("Max retries exceeded for task: {0}")]
    MaxRetriesExceeded(String),
    #[error("Max parallel tasks reached: {0}")]
    MaxParallelTasks(usize),
    #[error("Subtask depth exceeded for task: {0}")]
    SubtaskDepthExceeded(String),
    #[error("Agent not found: {0}")]
    AgentNotFound(String),
}

// ---------------------------------------------------------------------------
// Orchestrator
// ---------------------------------------------------------------------------

pub struct Orchestrator {
    tasks: DashMap<String, Task>,
    agents: DashMap<String, RegisteredAgent>,
    config: OrchestrationConfig,
    round_robin_counter: AtomicU64,
}

impl Orchestrator {
    pub fn new(config: OrchestrationConfig) -> Self {
        Self {
            tasks: DashMap::new(),
            agents: DashMap::new(),
            config,
            round_robin_counter: AtomicU64::new(0),
        }
    }

    /// Submit a new task. Returns the task ID.
    pub fn submit_task(&self, task: Task) -> Result<String, OrchestrationError> {
        let active = self.tasks.iter().filter(|r| {
            let s = r.value().status;
            s == TaskStatus::Pending || s == TaskStatus::Assigned || s == TaskStatus::Running
        }).count();
        if active >= self.config.max_parallel_tasks {
            return Err(OrchestrationError::MaxParallelTasks(self.config.max_parallel_tasks));
        }
        let id = task.id.clone();
        info!(task_id = %id, "Task submitted");
        self.tasks.insert(id.clone(), task);
        Ok(id)
    }

    /// Submit a fan-out: parent task + subtasks linked together.
    pub fn submit_fanout(
        &self,
        parent: Task,
        subtasks: Vec<Task>,
    ) -> Result<String, OrchestrationError> {
        // Check depth
        if let Some(ref pid) = parent.parent_task_id {
            let depth = self.calculate_depth(pid);
            if depth >= self.config.subtask_max_depth as usize {
                return Err(OrchestrationError::SubtaskDepthExceeded(parent.id.clone()));
            }
        }

        let parent_id = parent.id.clone();
        let mut sub_ids = Vec::new();

        // Insert subtasks first
        for st in &subtasks {
            sub_ids.push(st.id.clone());
            self.tasks.insert(st.id.clone(), st.clone());
        }

        // Insert parent with subtask IDs
        let mut p = parent;
        p.subtasks = sub_ids.clone();
        let pid = p.id.clone();
        self.tasks.insert(pid.clone(), p);

        info!(parent_id = %pid, subtask_count = sub_ids.len(), "Fan-out submitted");
        Ok(parent_id)
    }

    fn calculate_depth(&self, task_id: &str) -> usize {
        let mut depth = 0;
        let mut current = task_id.to_string();
        for _ in 0..20 {
            if let Some(t) = self.tasks.get(&current) {
                if let Some(ref pid) = t.value().parent_task_id {
                    depth += 1;
                    current = pid.clone();
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        depth
    }

    /// Assign a pending task to the best available agent. Returns agent PK.
    pub fn assign_task(&self, task_id: &str) -> Result<String, OrchestrationError> {
        {
            let task = self.tasks.get(task_id)
                .ok_or_else(|| OrchestrationError::TaskNotFound(task_id.to_string()))?;

            if task.status != TaskStatus::Pending {
                return Err(OrchestrationError::InvalidTransition(format!(
                    "task {} is {:?}, expected Pending",
                    task_id, task.status
                )));
            }
        }

        let best_pk = self.select_agent(&self.tasks.get(task_id).unwrap())?;

        // Update task with get_mut
        {
            let mut task = self.tasks.get_mut(task_id).unwrap();
            task.status = TaskStatus::Assigned;
            task.assigned_to = Some(best_pk.clone());
        }

        // Increment agent load
        if let Some(agent) = self.agents.get(&best_pk) {
            agent.inc_tasks();
        }

        info!(task_id = %task_id, agent = %best_pk, "Task assigned");
        Ok(best_pk)
    }

    fn select_agent(&self, task: &Task) -> Result<String, OrchestrationError> {
        let mut capable: Vec<_> = self.agents.iter()
            .filter(|a| a.value().has_capacity())
            .collect();
        // Sort by pk for deterministic iteration order (DashMap is unordered)
        capable.sort_by_key(|a| a.value().pk.clone());

        if capable.is_empty() {
            return Err(OrchestrationError::NoAvailableAgents);
        }

        match self.config.load_balance_strategy {
            LoadBalanceStrategy::RoundRobin => {
                let idx = self.round_robin_counter.fetch_add(1, Ordering::Relaxed) as usize % capable.len();
                Ok(capable[idx].value().pk.clone())
            }
            LoadBalanceStrategy::LeastLoaded => {
                let best = capable.into_iter()
                    .min_by_key(|a| a.value().active_tasks.load(Ordering::Relaxed))
                    .expect("non-empty");
                Ok(best.value().pk.clone())
            }
            LoadBalanceStrategy::BestFit => {
                // Prefer agents whose capabilities list includes models relevant to task
                let best = capable.into_iter()
                    .max_by(|a, b| {
                        let score_a = a.value().capabilities.iter()
                            .filter(|c| !c.models.is_empty())
                            .count() as u32;
                        let score_b = b.value().capabilities.iter()
                            .filter(|c| !c.models.is_empty())
                            .count() as u32;
                        score_a.cmp(&score_b)
                    });
                Ok(best.expect("non-empty").value().pk.clone())
            }
            LoadBalanceStrategy::ReputationWeighted => {
                let best = capable.into_iter()
                    .max_by(|a, b| {
                        a.value().reputation_score.partial_cmp(&b.value().reputation_score)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                Ok(best.expect("non-empty").value().pk.clone())
            }
            LoadBalanceStrategy::GeoPreferred => {
                // Prefer agents with most capabilities (proxy for region match)
                let best = capable.into_iter()
                    .max_by(|a, b| a.value().capabilities.len().cmp(&b.value().capabilities.len()));
                Ok(best.expect("non-empty").value().pk.clone())
            }
        }
    }

    /// Mark a task as completed with a result.
    pub fn complete_task(&self, task_id: &str, result: TaskResult) -> Result<(), OrchestrationError> {
        let assigned_to = {
            let task = self.tasks.get(task_id)
                .ok_or_else(|| OrchestrationError::TaskNotFound(task_id.to_string()))?;
            if task.status != TaskStatus::Assigned && task.status != TaskStatus::Running {
                return Err(OrchestrationError::InvalidTransition(
                    format!("task {} is {:?}, cannot complete", task_id, task.status)
                ));
            }
            task.assigned_to.clone()
        };

        if let Some(ref pk) = assigned_to {
            if let Some(agent) = self.agents.get(pk) {
                agent.dec_tasks();
            }
        }

        if let Some(mut task) = self.tasks.get_mut(task_id) {
            task.status = TaskStatus::Completed;
            task.result = Some(result);
        }

        info!(task_id = %task_id, "Task completed");
        Ok(())
    }

    /// Fail a task. Retries if possible, otherwise marks as Failed.
    pub fn fail_task(&self, task_id: &str, error: String) -> Result<(), OrchestrationError> {
        let (assigned_to, max_retries) = {
            let task = self.tasks.get(task_id)
                .ok_or_else(|| OrchestrationError::TaskNotFound(task_id.to_string()))?;
            if task.status != TaskStatus::Assigned && task.status != TaskStatus::Running {
                return Err(OrchestrationError::InvalidTransition(
                    format!("task {} is {:?}, cannot fail", task_id, task.status)
                ));
            }
            (task.assigned_to.clone(), task.max_retries)
        };

        if let Some(ref pk) = assigned_to {
            if let Some(agent) = self.agents.get(pk) {
                agent.dec_tasks();
            }
        }

        if let Some(mut task) = self.tasks.get_mut(task_id) {
            task.retry_count += 1;
            if task.retry_count >= max_retries {
                task.status = TaskStatus::Failed;
                task.assigned_to = None;
                warn!(task_id = %task_id, error = %error, "Task failed permanently");
            } else {
                task.status = TaskStatus::Pending;
                task.assigned_to = None;
                info!(task_id = %task_id, retry = task.retry_count, error = %error, "Task queued for retry");
            }
        }

        Ok(())
    }

    /// Cancel a task and all its subtasks recursively.
    pub fn cancel_task(&self, task_id: &str) -> Result<(), OrchestrationError> {
        let task = self.tasks.get(task_id)
            .ok_or_else(|| OrchestrationError::TaskNotFound(task_id.to_string()))?;

        let subtasks: Vec<String> = task.value().subtasks.clone();
        let assigned_to: Option<String> = task.value().assigned_to.clone();
        drop(task);

        // Decrement agent if assigned
        if let Some(ref pk) = assigned_to {
            if let Some(agent) = self.agents.get(pk) {
                agent.dec_tasks();
            }
        }

        // Cancel this task
        if let Some(mut t) = self.tasks.get_mut(task_id) {
            t.status = TaskStatus::Cancelled;
        }

        // Recursively cancel subtasks
        for sub_id in subtasks {
            let _ = self.cancel_task(&sub_id);
        }

        info!(task_id = %task_id, "Task cancelled");
        Ok(())
    }

    /// Get a task by ID (returns a clone).
    pub fn get_task(&self, task_id: &str) -> Option<Task> {
        self.tasks.get(task_id).map(|r| r.value().clone())
    }

    /// List tasks with optional filters.
    pub fn list_tasks(&self, filter: TaskFilter) -> Vec<Task> {
        let mut tasks: Vec<Task> = self.tasks.iter().map(|r| r.value().clone()).collect();

        if let Some(ref status) = filter.status {
            tasks.retain(|t| t.status == *status);
        }
        if let Some(ref tt) = filter.task_type {
            tasks.retain(|t| t.task_type == *tt);
        }
        if let Some(ref pk) = filter.assigned_to {
            tasks.retain(|t| t.assigned_to.as_ref() == Some(pk));
        }
        if let Some(ref prio) = filter.priority {
            tasks.retain(|t| t.priority == *prio);
        }
        if let Some(limit) = filter.limit {
            tasks.truncate(limit);
        }

        tasks
    }

    /// Get orchestrator statistics.
    pub fn get_stats(&self) -> OrchestrationStats {
        let mut total = 0usize;
        let mut pending = 0usize;
        let mut running = 0usize;
        let mut completed = 0usize;
        let mut failed = 0usize;

        for entry in self.tasks.iter() {
            total += 1;
            match entry.value().status {
                TaskStatus::Pending | TaskStatus::Waiting => pending += 1,
                TaskStatus::Assigned | TaskStatus::Running => running += 1,
                TaskStatus::Completed => completed += 1,
                TaskStatus::Failed | TaskStatus::TimedOut => failed += 1,
                TaskStatus::Cancelled => {}
            }
        }

        OrchestrationStats {
            total_tasks: total,
            pending_tasks: pending,
            running_tasks: running,
            completed_tasks: completed,
            failed_tasks: failed,
            registered_agents: self.agents.len(),
        }
    }

    /// Register an agent with the orchestrator.
    pub fn register_agent(&self, agent: RegisteredAgent) {
        info!(agent = %agent.pk, "Agent registered");
        self.agents.insert(agent.pk.clone(), agent);
    }

    /// Unregister an agent.
    pub fn unregister_agent(&self, agent_pk: &str) -> Result<(), OrchestrationError> {
        if self.agents.remove(agent_pk).is_none() {
            return Err(OrchestrationError::AgentNotFound(agent_pk.to_string()));
        }
        info!(agent = %agent_pk, "Agent unregistered");
        Ok(())
    }

    /// Get all registered agents as serializable snapshots.
    pub fn get_agents(&self) -> Vec<RegisteredAgentSnapshot> {
        self.agents.iter().map(|r| r.value().snapshot()).collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_agent(pk: &str, region: &str, rep: f64) -> RegisteredAgent {
        RegisteredAgent::new(
            pk.to_string(),
            vec![AgentCapability {
                role: AgentRole::Worker,
                models: vec!["llama-3.1-8b".to_string()],
                max_concurrent_tasks: 5,
                region: region.to_string(),
                gpu_spec: None,
                reputation_score: rep,
            }],
            5,
            region.to_string(),
            rep,
        )
    }

    // 1. test_task_creation
    #[test]
    fn test_task_creation() {
        let t = Task::new(TaskType::ChatCompletion, serde_json::json!({"prompt": "hello"}));
        assert_eq!(t.status, TaskStatus::Pending);
        assert!(t.assigned_to.is_none());
        assert_eq!(t.retry_count, 0);
        assert!(t.subtasks.is_empty());
        assert!(t.result.is_none());
        assert_eq!(t.priority, TaskPriority::Normal);
    }

    // 2. test_task_builder_with_priority
    #[test]
    fn test_task_builder_with_priority() {
        let t = Task::new(TaskType::CodeGeneration, serde_json::json!({}))
            .with_priority(TaskPriority::Critical)
            .with_timeout(60)
            .with_max_retries(5);
        assert_eq!(t.priority, TaskPriority::Critical);
        assert_eq!(t.timeout_secs, Some(60));
        assert_eq!(t.max_retries, 5);
    }

    // 3. test_orchestrator_new
    #[test]
    fn test_orchestrator_new() {
        let orch = Orchestrator::new(OrchestrationConfig::default());
        assert_eq!(orch.get_stats().total_tasks, 0);
        assert_eq!(orch.get_stats().registered_agents, 0);
    }

    // 4. test_submit_task
    #[test]
    fn test_submit_task() {
        let orch = Orchestrator::new(OrchestrationConfig::default());
        let id = orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({}))).unwrap();
        assert!(!id.is_empty());
        let t = orch.get_task(&id).unwrap();
        assert_eq!(t.status, TaskStatus::Pending);
    }

    // 5. test_submit_task_auto_id
    #[test]
    fn test_submit_task_auto_id() {
        let orch = Orchestrator::new(OrchestrationConfig::default());
        let t = Task::new(TaskType::DataAnalysis, serde_json::json!({}));
        let id1 = orch.submit_task(t).unwrap();
        let id2 = orch.submit_task(Task::new(TaskType::DataAnalysis, serde_json::json!({}))).unwrap();
        assert_ne!(id1, id2);
    }

    // 6. test_get_task
    #[test]
    fn test_get_task() {
        let orch = Orchestrator::new(OrchestrationConfig::default());
        let id = orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({}))).unwrap();
        let t = orch.get_task(&id);
        assert!(t.is_some());
        assert_eq!(t.unwrap().task_type, TaskType::ChatCompletion);
    }

    // 7. test_get_task_not_found
    #[test]
    fn test_get_task_not_found() {
        let orch = Orchestrator::new(OrchestrationConfig::default());
        assert!(orch.get_task("nonexistent").is_none());
    }

    // 8. test_list_tasks_empty
    #[test]
    fn test_list_tasks_empty() {
        let orch = Orchestrator::new(OrchestrationConfig::default());
        assert!(orch.list_tasks(TaskFilter::default()).is_empty());
    }

    // 9. test_list_tasks_with_filter
    #[test]
    fn test_list_tasks_with_filter() {
        let orch = Orchestrator::new(OrchestrationConfig::default());
        orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({}))).unwrap();
        orch.submit_task(Task::new(TaskType::CodeGeneration, serde_json::json!({}))).unwrap();

        let chat = orch.list_tasks(TaskFilter {
            task_type: Some(TaskType::ChatCompletion),
            ..Default::default()
        });
        assert_eq!(chat.len(), 1);
    }

    // 10. test_list_tasks_with_limit
    #[test]
    fn test_list_tasks_with_limit() {
        let orch = Orchestrator::new(OrchestrationConfig::default());
        for _ in 0..10 {
            orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({}))).unwrap();
        }
        let tasks = orch.list_tasks(TaskFilter { limit: Some(3), ..Default::default() });
        assert_eq!(tasks.len(), 3);
    }

    // 11. test_register_agent
    #[test]
    fn test_register_agent() {
        let orch = Orchestrator::new(OrchestrationConfig::default());
        orch.register_agent(make_agent("pk1", "us-east", 0.9));
        let agents = orch.get_agents();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].pk, "pk1");
    }

    // 12. test_unregister_agent
    #[test]
    fn test_unregister_agent() {
        let orch = Orchestrator::new(OrchestrationConfig::default());
        orch.register_agent(make_agent("pk1", "us-east", 0.9));
        orch.unregister_agent("pk1").unwrap();
        assert!(orch.get_agents().is_empty());
    }

    // 13. test_get_stats_empty
    #[test]
    fn test_get_stats_empty() {
        let orch = Orchestrator::new(OrchestrationConfig::default());
        let s = orch.get_stats();
        assert_eq!(s.total_tasks, 0);
        assert_eq!(s.pending_tasks, 0);
        assert_eq!(s.registered_agents, 0);
    }

    // 14. test_assign_task_round_robin
    #[test]
    fn test_assign_task_round_robin() {
        let orch = Orchestrator::new(OrchestrationConfig {
            load_balance_strategy: LoadBalanceStrategy::RoundRobin,
            ..Default::default()
        });
        orch.register_agent(make_agent("pk1", "us-east", 0.9));
        orch.register_agent(make_agent("pk2", "eu-west", 0.8));

        let id1 = orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({}))).unwrap();
        let id2 = orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({}))).unwrap();
        let id3 = orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({}))).unwrap();

        let a1 = orch.assign_task(&id1).unwrap();
        let a2 = orch.assign_task(&id2).unwrap();
        let a3 = orch.assign_task(&id3).unwrap();

        // Round-robin: pk1, pk2, pk1
        assert_eq!(a1, "pk1");
        assert_eq!(a2, "pk2");
        assert_eq!(a3, "pk1");
    }

    // 15. test_assign_task_least_loaded
    #[test]
    fn test_assign_task_least_loaded() {
        let orch = Orchestrator::new(OrchestrationConfig {
            load_balance_strategy: LoadBalanceStrategy::LeastLoaded,
            ..Default::default()
        });
        orch.register_agent(make_agent("pk1", "us-east", 0.9));
        orch.register_agent(make_agent("pk2", "eu-west", 0.8));

        let id1 = orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({}))).unwrap();
        let id2 = orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({}))).unwrap();

        // Both have 0 tasks, picks either (DashMap iter order is non-deterministic)
        let a1 = orch.assign_task(&id1).unwrap();
        // a1 now has 1 task, the other has 0 -> picks the other
        let a2 = orch.assign_task(&id2).unwrap();
        assert_ne!(a2, a1);
    }

    // 16. test_assign_task_no_agents
    #[test]
    fn test_assign_task_no_agents() {
        let orch = Orchestrator::new(OrchestrationConfig::default());
        let id = orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({}))).unwrap();
        let result = orch.assign_task(&id);
        assert!(result.is_err());
        match result.unwrap_err() {
            OrchestrationError::NoAvailableAgents => {}
            other => panic!("expected NoAvailableAgents, got {:?}", other),
        }
    }

    // 17. test_complete_task
    #[test]
    fn test_complete_task() {
        let orch = Orchestrator::new(OrchestrationConfig::default());
        orch.register_agent(make_agent("pk1", "us-east", 0.9));
        let id = orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({}))).unwrap();
        orch.assign_task(&id).unwrap();

        let result = TaskResult {
            output: serde_json::json!("done"),
            tokens_used: 100,
            duration_ms: 50,
            agent_pk: "pk1".to_string(),
            quality_score: Some(0.95),
        };
        orch.complete_task(&id, result).unwrap();

        let t = orch.get_task(&id).unwrap();
        assert_eq!(t.status, TaskStatus::Completed);
        assert!(t.result.is_some());
        // Agent load should be decremented
        let agents = orch.get_agents();
        assert_eq!(agents[0].active_tasks, 0);
    }

    // 18. test_fail_task_retry
    #[test]
    fn test_fail_task_retry() {
        let orch = Orchestrator::new(OrchestrationConfig::default());
        orch.register_agent(make_agent("pk1", "us-east", 0.9));
        let id = orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({})).with_max_retries(3)).unwrap();
        orch.assign_task(&id).unwrap();

        orch.fail_task(&id, "timeout".to_string()).unwrap();
        let t = orch.get_task(&id).unwrap();
        assert_eq!(t.status, TaskStatus::Pending); // should be retryable
        assert_eq!(t.retry_count, 1);
    }

    // 19. test_fail_task_max_retries
    #[test]
    fn test_fail_task_max_retries() {
        let orch = Orchestrator::new(OrchestrationConfig::default());
        orch.register_agent(make_agent("pk1", "us-east", 0.9));
        let id = orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({})).with_max_retries(2)).unwrap();

        orch.assign_task(&id).unwrap();
        orch.fail_task(&id, "err".to_string()).unwrap();
        orch.assign_task(&id).unwrap();
        orch.fail_task(&id, "err".to_string()).unwrap();

        let t = orch.get_task(&id).unwrap();
        assert_eq!(t.status, TaskStatus::Failed);
        assert_eq!(t.retry_count, 2);
    }

    // 20. test_cancel_task
    #[test]
    fn test_cancel_task() {
        let orch = Orchestrator::new(OrchestrationConfig::default());
        let id = orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({}))).unwrap();
        orch.cancel_task(&id).unwrap();
        let t = orch.get_task(&id).unwrap();
        assert_eq!(t.status, TaskStatus::Cancelled);
    }

    // 21. test_cancel_task_with_subtasks
    #[test]
    fn test_cancel_task_with_subtasks() {
        let orch = Orchestrator::new(OrchestrationConfig::default());
        let parent = Task::new(TaskType::CodeGeneration, serde_json::json!({}));
        let sub1 = Task::new(TaskType::ChatCompletion, serde_json::json!({})).with_parent(parent.id.clone());
        let sub2 = Task::new(TaskType::ChatCompletion, serde_json::json!({})).with_parent(parent.id.clone());
        orch.submit_fanout(parent, vec![sub1, sub2]).unwrap();

        let tasks = orch.list_tasks(TaskFilter::default());
        assert_eq!(tasks.len(), 3);

        let parent_id = tasks.iter().find(|t| t.parent_task_id.is_none()).unwrap().id.clone();
        orch.cancel_task(&parent_id).unwrap();

        let all = orch.list_tasks(TaskFilter::default());
        assert!(all.iter().all(|t| t.status == TaskStatus::Cancelled));
    }

    // 22. test_submit_fanout
    #[test]
    fn test_submit_fanout() {
        let orch = Orchestrator::new(OrchestrationConfig::default());
        let parent = Task::new(TaskType::CodeGeneration, serde_json::json!({}));
        let sub1 = Task::new(TaskType::ChatCompletion, serde_json::json!({})).with_parent(parent.id.clone());
        let sub2 = Task::new(TaskType::ChatCompletion, serde_json::json!({})).with_parent(parent.id.clone());
        let sub3 = Task::new(TaskType::DataAnalysis, serde_json::json!({})).with_parent(parent.id.clone());

        let pid = orch.submit_fanout(parent, vec![sub1, sub2, sub3]).unwrap();
        let p = orch.get_task(&pid).unwrap();
        assert_eq!(p.subtasks.len(), 3);
        assert!(p.parent_task_id.is_none());

        // Verify subtasks have parent
        for sid in &p.subtasks {
            let st = orch.get_task(sid).unwrap();
            assert_eq!(st.parent_task_id.as_deref(), Some(pid.as_str()));
        }
    }

    // 23. test_fanout_subtask_depth
    #[test]
    fn test_fanout_subtask_depth() {
        let orch = Orchestrator::new(OrchestrationConfig {
            subtask_max_depth: 2,
            ..Default::default()
        });

        // Level 0: parent
        let parent = Task::new(TaskType::CodeGeneration, serde_json::json!({}));
        let pid = orch.submit_task(parent).unwrap();

        // Level 1: subtask of parent
        let sub1 = Task::new(TaskType::ChatCompletion, serde_json::json!({})).with_parent(pid.clone());
        let sub1_id = orch.submit_task(sub1).unwrap();

        // Level 2: subtask of subtask (depth=2, still ok)
        let sub2 = Task::new(TaskType::ChatCompletion, serde_json::json!({})).with_parent(sub1_id.clone());
        let sub2_id = orch.submit_task(sub2).unwrap();

        // Level 3: should fail (depth=3 > max=2)
        let sub3 = Task::new(TaskType::ChatCompletion, serde_json::json!({})).with_parent(sub2_id.clone());
        let result = orch.submit_fanout(sub3, vec![]);
        assert!(result.is_err());
        match result.unwrap_err() {
            OrchestrationError::SubtaskDepthExceeded(_) => {}
            other => panic!("expected SubtaskDepthExceeded, got {:?}", other),
        }
    }

    // 24. test_task_status_transitions
    #[test]
    fn test_task_status_transitions() {
        let orch = Orchestrator::new(OrchestrationConfig::default());
        orch.register_agent(make_agent("pk1", "us-east", 0.9));

        let id = orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({}))).unwrap();
        assert_eq!(orch.get_task(&id).unwrap().status, TaskStatus::Pending);

        orch.assign_task(&id).unwrap();
        assert_eq!(orch.get_task(&id).unwrap().status, TaskStatus::Assigned);

        let result = TaskResult {
            output: serde_json::json!("ok"),
            tokens_used: 50,
            duration_ms: 30,
            agent_pk: "pk1".to_string(),
            quality_score: None,
        };
        orch.complete_task(&id, result).unwrap();
        assert_eq!(orch.get_task(&id).unwrap().status, TaskStatus::Completed);
    }

    // 25. test_orchestration_stats
    #[test]
    fn test_orchestration_stats() {
        let orch = Orchestrator::new(OrchestrationConfig::default());
        orch.register_agent(make_agent("pk1", "us-east", 0.9));

        let id1 = orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({}))).unwrap();
        let id2 = orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({}))).unwrap();
        let id3 = orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({}))).unwrap();

        orch.assign_task(&id1).unwrap();
        orch.assign_task(&id2).unwrap();

        let result = TaskResult {
            output: serde_json::json!("done"),
            tokens_used: 100,
            duration_ms: 50,
            agent_pk: "pk1".to_string(),
            quality_score: None,
        };
        orch.complete_task(&id1, result).unwrap();

        orch.fail_task(&id2, "err".to_string()).unwrap();       // retry 1 -> Pending
        orch.assign_task(&id2).unwrap();                          // re-assign
        orch.fail_task(&id2, "err".to_string()).unwrap();       // retry 2 -> Pending
        orch.assign_task(&id2).unwrap();                          // re-assign
        orch.fail_task(&id2, "err".to_string()).unwrap();       // retry 3 -> Failed (max_retries=3)

        let s = orch.get_stats();
        assert_eq!(s.total_tasks, 3);
        assert_eq!(s.completed_tasks, 1);
        assert_eq!(s.failed_tasks, 1);
        assert_eq!(s.pending_tasks, 1); // only id3 still pending
        assert_eq!(s.registered_agents, 1);
    }

    // 26. test_a2a_message_serialization
    #[test]
    fn test_a2a_message_serialization() {
        let msg = A2AMessage::TaskRequest {
            task_id: "task-123".to_string(),
            payload: serde_json::json!({"prompt": "hello"}),
            timeout_ms: 5000,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: A2AMessage = serde_json::from_str(&json).unwrap();
        match decoded {
            A2AMessage::TaskRequest { task_id, .. } => assert_eq!(task_id, "task-123"),
            other => panic!("expected TaskRequest, got {:?}", other),
        }
    }

    // 27. test_heartbeat_processing
    #[test]
    fn test_heartbeat_processing() {
        let load = AgentLoad {
            active_tasks: 3,
            queue_size: 1,
            cpu_usage: 0.6,
            memory_usage: 0.4,
            gpu_usage: 0.8,
            estimated_capacity: 7,
        };
        assert_eq!(load.active_tasks, 3);
        assert_eq!(load.estimated_capacity, 7);
    }

    // 28. test_reputation_weighted_selection
    #[test]
    fn test_reputation_weighted_selection() {
        let orch = Orchestrator::new(OrchestrationConfig {
            load_balance_strategy: LoadBalanceStrategy::ReputationWeighted,
            ..Default::default()
        });
        orch.register_agent(make_agent("low-rep", "us-east", 0.3));
        orch.register_agent(make_agent("high-rep", "eu-west", 0.9));

        let id = orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({}))).unwrap();
        let chosen = orch.assign_task(&id).unwrap();
        assert_eq!(chosen, "high-rep");
    }

    // 29. test_task_priority_ordering
    #[test]
    fn test_task_priority_ordering() {
        assert!(TaskPriority::Critical < TaskPriority::High);
        assert!(TaskPriority::High < TaskPriority::Normal);
        assert!(TaskPriority::Normal < TaskPriority::Low);
        assert!(TaskPriority::Low < TaskPriority::Background);
    }

    // 30. test_registered_agent_capacity
    #[test]
    fn test_registered_agent_capacity() {
        let agent = make_agent("pk1", "us-east", 0.9);
        assert!(agent.has_capacity());
        for _ in 0..5 {
            agent.inc_tasks();
        }
        assert!(!agent.has_capacity());
        agent.dec_tasks();
        assert!(agent.has_capacity());
    }

    // 31. test_max_parallel_tasks
    #[test]
    fn test_max_parallel_tasks() {
        let orch = Orchestrator::new(OrchestrationConfig {
            max_parallel_tasks: 2,
            ..Default::default()
        });
        orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({})).with_priority(TaskPriority::Critical)).unwrap();
        orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({})).with_priority(TaskPriority::High)).unwrap();

        // Third task should fail (max parallel = 2, both pending/assigned count)
        // Note: Pending + Assigned count toward limit
        let result = orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({})));
        assert!(result.is_err());
    }
}
