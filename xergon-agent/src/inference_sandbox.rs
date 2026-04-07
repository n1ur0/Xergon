//! Inference Sandbox — Sandboxed inference execution with resource limits
//!
//! Provides isolated execution environments for model inference:
//! - SandboxConfig: configurable resource limits (CPU, memory, timeout, tokens)
//! - ResourceLimit: per-session resource caps
//! - SandboxStatus: lifecycle states for sessions
//! - InferenceSandbox: DashMap-backed sandbox manager with resource monitoring
//! - SandboxSession: individual execution session tracking
//!
//! Features:
//! - Resource limits enforcement (CPU, memory, timeout, output tokens)
//! - Timeout handling with configurable execution windows
//! - Memory tracking per session
//! - Concurrent session management
//! - Network and filesystem access controls
//!
//! REST endpoints:
//! - POST /v1/sandbox/create               — Create a sandbox session
//! - POST /v1/sandbox/{id}/execute         — Execute inference in sandbox
//! - GET  /v1/sandbox/{id}/status          — Get sandbox session status
//! - DELETE /v1/sandbox/{id}               — Terminate sandbox session
//! - GET  /v1/sandbox/active               — List active sessions
//! - GET  /v1/sandbox/{id}/resources       — Get resource usage
//! - GET  /v1/sandbox/config               — Get sandbox configuration

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// SandboxConfig
// ---------------------------------------------------------------------------

/// Configuration for the inference sandbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Maximum CPU usage percentage per session (0-100).
    pub max_cpu_percent: u32,
    /// Maximum memory in megabytes per session.
    pub max_memory_mb: u64,
    /// Maximum execution time in seconds per request.
    pub max_execution_secs: u64,
    /// Maximum output tokens per inference request.
    pub max_output_tokens: u64,
    /// Whether sessions are allowed network access.
    pub network_access: bool,
    /// Whether sessions are allowed filesystem access.
    pub allow_filesystem: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            max_cpu_percent: 80,
            max_memory_mb: 4096,
            max_execution_secs: 300,
            max_output_tokens: 8192,
            network_access: false,
            allow_filesystem: false,
        }
    }
}

// ---------------------------------------------------------------------------
// ResourceLimit
// ---------------------------------------------------------------------------

/// Resource limits for an individual sandbox session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimit {
    /// CPU quota as a percentage (0-100).
    pub cpu_quota: u32,
    /// Memory limit in bytes.
    pub memory_limit: u64,
    /// Execution timeout in seconds.
    pub timeout: u64,
    /// Maximum output token count.
    pub output_limit: u64,
}

impl From<&SandboxConfig> for ResourceLimit {
    fn from(config: &SandboxConfig) -> Self {
        Self {
            cpu_quota: config.max_cpu_percent,
            memory_limit: config.max_memory_mb * 1024 * 1024,
            timeout: config.max_execution_secs,
            output_limit: config.max_output_tokens,
        }
    }
}

// ---------------------------------------------------------------------------
// ResourceUsage
// ---------------------------------------------------------------------------

/// Current resource usage for a sandbox session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceUsage {
    /// Estimated CPU usage percentage.
    pub cpu_percent: f64,
    /// Memory used in bytes.
    pub memory_bytes: u64,
    /// Elapsed time in seconds since session start.
    pub elapsed_secs: f64,
    /// Number of output tokens generated so far.
    pub output_tokens: u64,
    /// Number of inference requests processed.
    pub requests_processed: u64,
}

// ---------------------------------------------------------------------------
// SandboxStatus
// ---------------------------------------------------------------------------

/// Lifecycle status of a sandbox session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxStatus {
    /// Session created, ready for inference.
    Ready,
    /// Inference is currently executing.
    Running,
    /// Inference completed successfully.
    Completed,
    /// Execution exceeded the time limit.
    TimedOut,
    /// Execution exceeded resource limits (memory, CPU, tokens).
    ResourceExceeded,
    /// An error occurred during execution.
    Error,
}

impl std::fmt::Display for SandboxStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SandboxStatus::Ready => write!(f, "ready"),
            SandboxStatus::Running => write!(f, "running"),
            SandboxStatus::Completed => write!(f, "completed"),
            SandboxStatus::TimedOut => write!(f, "timed_out"),
            SandboxStatus::ResourceExceeded => write!(f, "resource_exceeded"),
            SandboxStatus::Error => write!(f, "error"),
        }
    }
}

// ---------------------------------------------------------------------------
// SandboxSession
// ---------------------------------------------------------------------------

/// A single sandbox execution session.
#[derive(Debug, Clone)]
pub struct SandboxSession {
    /// Unique session identifier.
    pub session_id: String,
    /// Model being used for inference.
    pub model_id: String,
    /// Current status of the session.
    pub status: SandboxStatus,
    /// Resource limits for this session.
    pub limits: ResourceLimit,
    /// Current resource usage.
    pub resource_usage: ResourceUsage,
    /// When the session was created.
    pub start_time: Instant,
    /// When the session was created (wall clock).
    pub created_at: DateTime<Utc>,
    /// Output produced by inference.
    pub output: String,
    /// Error message if the session failed.
    pub error: Option<String>,
    /// Network access flag.
    pub network_access: bool,
    /// Filesystem access flag.
    pub allow_filesystem: bool,
}

impl SandboxSession {
    /// Create a new sandbox session.
    pub fn new(model_id: &str, limits: ResourceLimit, config: &SandboxConfig) -> Self {
        Self {
            session_id: Uuid::new_v4().to_string(),
            model_id: model_id.to_string(),
            status: SandboxStatus::Ready,
            limits,
            resource_usage: ResourceUsage::default(),
            start_time: Instant::now(),
            created_at: Utc::now(),
            output: String::new(),
            error: None,
            network_access: config.network_access,
            allow_filesystem: config.allow_filesystem,
        }
    }

    /// Check if the session has exceeded its resource limits.
    pub fn check_resource_limits(&self) -> Result<(), SandboxStatus> {
        // Check timeout
        if self.resource_usage.elapsed_secs > self.limits.timeout as f64 {
            return Err(SandboxStatus::TimedOut);
        }

        // Check memory
        if self.resource_usage.memory_bytes > self.limits.memory_limit {
            return Err(SandboxStatus::ResourceExceeded);
        }

        // Check output tokens
        if self.resource_usage.output_tokens > self.limits.output_limit {
            return Err(SandboxStatus::ResourceExceeded);
        }

        // Check CPU
        if self.resource_usage.cpu_percent > self.limits.cpu_quota as f64 {
            return Err(SandboxStatus::ResourceExceeded);
        }

        Ok(())
    }

    /// Update elapsed time based on session start.
    pub fn update_elapsed(&mut self) {
        self.resource_usage.elapsed_secs = self.start_time.elapsed().as_secs_f64();
    }
}

// ---------------------------------------------------------------------------
// InferenceSandbox
// ---------------------------------------------------------------------------

/// Sandboxed inference execution manager with DashMap-backed session storage.
#[derive(Debug, Clone)]
pub struct InferenceSandbox {
    /// Active sandbox sessions keyed by session_id.
    sessions: Arc<DashMap<String, SandboxSession>>,
    /// Global sandbox configuration.
    config: Arc<std::sync::RwLock<SandboxConfig>>,
    /// Total sessions ever created.
    total_sessions: Arc<AtomicU64>,
    /// Total sessions completed.
    total_completed: Arc<AtomicU64>,
    /// Total sessions terminated due to timeout.
    total_timed_out: Arc<AtomicU64>,
    /// Total sessions terminated due to resource limits.
    total_resource_exceeded: Arc<AtomicU64>,
    /// Total sessions that errored.
    total_errors: Arc<AtomicU64>,
}

impl InferenceSandbox {
    /// Create a new InferenceSandbox with default configuration.
    pub fn new() -> Self {
        Self::with_config(SandboxConfig::default())
    }

    /// Create a new InferenceSandbox with custom configuration.
    pub fn with_config(config: SandboxConfig) -> Self {
        Self {
            sessions: Arc::new(DashMap::new()),
            config: Arc::new(std::sync::RwLock::new(config)),
            total_sessions: Arc::new(AtomicU64::new(0)),
            total_completed: Arc::new(AtomicU64::new(0)),
            total_timed_out: Arc::new(AtomicU64::new(0)),
            total_resource_exceeded: Arc::new(AtomicU64::new(0)),
            total_errors: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Create a new sandbox session for a given model.
    pub fn create_sandbox(&self, model_id: &str) -> Result<SandboxSession, String> {
        let config = self
            .config
            .read()
            .map_err(|e| format!("Failed to read config: {}", e))?;
        let limits = ResourceLimit::from(&*config);
        let session = SandboxSession::new(model_id, limits, &config);
        let session_id = session.session_id.clone();

        self.sessions.insert(session_id.clone(), session);
        self.total_sessions.fetch_add(1, Ordering::Relaxed);

        self.sessions
            .get(&session_id)
            .map(|s| s.clone())
            .ok_or_else(|| "Session not found after creation".to_string())
    }

    /// Execute inference within a sandbox session.
    pub fn execute_inference(
        &self,
        session_id: &str,
        input: &str,
        max_tokens: Option<u64>,
    ) -> Result<String, String> {
        let mut session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| format!("Session {} not found", session_id))?;

        // Check session is in a valid state
        match session.status {
            SandboxStatus::Ready | SandboxStatus::Completed => {}
            SandboxStatus::Running => {
                return Err("Session is already running an inference".to_string());
            }
            _ => {
                return Err(format!(
                    "Session is in terminal state: {}",
                    session.status
                ));
            }
        }

        session.status = SandboxStatus::Running;
        session.update_elapsed();

        // Check resource limits before execution
        if let Err(limit_status) = session.check_resource_limits() {
            session.status = limit_status.clone();
            session.error = Some(format!("Pre-execution resource check failed: {}", limit_status));
            self.record_status(&limit_status);
            return Err(session.error.clone().unwrap_or_default());
        }

        // Apply token limit override
        let effective_max_tokens = max_tokens.unwrap_or(session.limits.output_limit);
        let remaining_tokens = session.limits.output_limit.saturating_sub(session.resource_usage.output_tokens);
        let actual_max = effective_max_tokens.min(remaining_tokens);

        // Simulate inference execution (in a real implementation, this would
        // invoke the actual model). We simulate token generation and resource
        // consumption.
        let simulated_output = self.simulate_inference(input, actual_max);

        // Update resource usage
        session.resource_usage.output_tokens += simulated_output.split_whitespace().count() as u64;
        session.resource_usage.requests_processed += 1;
        session.resource_usage.memory_bytes = (session.resource_usage.memory_bytes + input.len() as u64 + simulated_output.len() as u64)
            .min(session.limits.memory_limit);
        session.resource_usage.cpu_percent =
            (session.resource_usage.cpu_percent + 15.0).min(session.limits.cpu_quota as f64);
        session.update_elapsed();

        // Post-execution resource check
        if let Err(limit_status) = session.check_resource_limits() {
            session.output.push_str(&simulated_output);
            session.status = limit_status.clone();
            session.error = Some(format!("Post-execution resource limit exceeded: {}", limit_status));
            self.record_status(&limit_status);
            return Err(session.error.clone().unwrap_or_default());
        }

        session.output.push_str(&simulated_output);
        session.status = SandboxStatus::Completed;
        self.total_completed.fetch_add(1, Ordering::Relaxed);

        Ok(simulated_output)
    }

    /// Simulate inference (placeholder for actual model invocation).
    fn simulate_inference(&self, input: &str, max_tokens: u64) -> String {
        let word_count = input.split_whitespace().count().min(max_tokens as usize).max(1);
        // Generate a deterministic simulated response
        format!(
            "[sandbox] Processed {} tokens from input: '{}' (max_tokens={})",
            word_count,
            &input[..input.len().min(50)],
            max_tokens
        )
    }

    /// Get the status of a sandbox session.
    pub fn get_status(&self, session_id: &str) -> Result<SandboxStatus, String> {
        self.sessions
            .get(session_id)
            .map(|s| s.status.clone())
            .ok_or_else(|| format!("Session {} not found", session_id))
    }

    /// Terminate a sandbox session.
    pub fn terminate(&self, session_id: &str) -> Result<SandboxSession, String> {
        let mut session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| format!("Session {} not found", session_id))?;

        if session.status == SandboxStatus::Running {
            session.status = SandboxStatus::Error;
            session.error = Some("Session terminated by user".to_string());
            self.total_errors.fetch_add(1, Ordering::Relaxed);
        }

        let result = session.clone();
        drop(session);
        self.sessions.remove(session_id);
        Ok(result)
    }

    /// List all active (non-terminal) sessions.
    pub fn list_active(&self) -> Vec<SandboxSession> {
        self.sessions
            .iter()
            .filter(|s| matches!(s.status, SandboxStatus::Ready | SandboxStatus::Running))
            .map(|s| s.clone())
            .collect()
    }

    /// Get resource usage for a session.
    pub fn get_resource_usage(&self, session_id: &str) -> Result<ResourceUsage, String> {
        let mut session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| format!("Session {} not found", session_id))?;
        session.update_elapsed();
        Ok(session.resource_usage.clone())
    }

    /// Update the global sandbox configuration.
    pub fn update_config(&self, new_config: SandboxConfig) -> Result<(), String> {
        let mut config = self
            .config
            .write()
            .map_err(|e| format!("Failed to acquire config lock: {}", e))?;
        *config = new_config;
        Ok(())
    }

    /// Get the current sandbox configuration.
    pub fn get_config(&self) -> Result<SandboxConfig, String> {
        self.config
            .read()
            .map(|c| c.clone())
            .map_err(|e| format!("Failed to read config: {}", e))
    }

    /// Get aggregate statistics.
    pub fn get_stats(&self) -> SandboxStats {
        let active_count = self
            .sessions
            .iter()
            .filter(|s| matches!(s.status, SandboxStatus::Ready | SandboxStatus::Running))
            .count();
        SandboxStats {
            total_sessions: self.total_sessions.load(Ordering::Relaxed),
            total_completed: self.total_completed.load(Ordering::Relaxed),
            total_timed_out: self.total_timed_out.load(Ordering::Relaxed),
            total_resource_exceeded: self.total_resource_exceeded.load(Ordering::Relaxed),
            total_errors: self.total_errors.load(Ordering::Relaxed),
            active_sessions: active_count,
        }
    }

    /// Prune completed/terminal sessions older than a given duration.
    pub fn prune_sessions(&self, max_age_secs: u64) -> usize {
        let now = Utc::now();
        let cutoff = now - chrono::Duration::seconds(max_age_secs as i64);
        let mut pruned = 0;

        self.sessions.retain(|_, session| {
            let is_active = matches!(
                session.status,
                SandboxStatus::Ready | SandboxStatus::Running
            );
            let is_recent = session.created_at >= cutoff;
            if !is_active && !is_recent {
                pruned += 1;
                false
            } else {
                true
            }
        });

        pruned
    }

    /// Record a terminal status in counters.
    fn record_status(&self, status: &SandboxStatus) {
        match status {
            SandboxStatus::TimedOut => {
                self.total_timed_out.fetch_add(1, Ordering::Relaxed);
            }
            SandboxStatus::ResourceExceeded => {
                self.total_resource_exceeded.fetch_add(1, Ordering::Relaxed);
            }
            SandboxStatus::Error => {
                self.total_errors.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// SandboxStats
// ---------------------------------------------------------------------------

/// Aggregate statistics for the sandbox manager.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxStats {
    pub total_sessions: u64,
    pub total_completed: u64,
    pub total_timed_out: u64,
    pub total_resource_exceeded: u64,
    pub total_errors: u64,
    pub active_sessions: usize,
}

// ---------------------------------------------------------------------------
// REST API types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateSandboxRequest {
    pub model_id: String,
    #[serde(default)]
    pub network_access: Option<bool>,
    #[serde(default)]
    pub allow_filesystem: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct CreateSandboxResponse {
    pub session_id: String,
    pub model_id: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ExecuteInferenceRequest {
    pub input: String,
    #[serde(default)]
    pub max_tokens: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct ExecuteInferenceResponse {
    pub session_id: String,
    pub output: String,
    pub status: String,
    pub resource_usage: ResourceUsage,
}

#[derive(Debug, Serialize)]
pub struct SessionStatusResponse {
    pub session_id: String,
    pub model_id: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub resource_usage: ResourceUsage,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TerminateResponse {
    pub session_id: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ActiveSessionsResponse {
    pub sessions: Vec<SessionStatusResponse>,
    pub count: usize,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConfigRequest {
    pub max_cpu_percent: Option<u32>,
    pub max_memory_mb: Option<u64>,
    pub max_execution_secs: Option<u64>,
    pub max_output_tokens: Option<u64>,
    pub network_access: Option<bool>,
    pub allow_filesystem: Option<bool>,
}

// ---------------------------------------------------------------------------
// REST handlers
// ---------------------------------------------------------------------------

/// POST /v1/sandbox/create — Create a new sandbox session.
pub async fn create_sandbox_handler(
    State(state): State<crate::api::AppState>,
    Json(req): Json<CreateSandboxRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.inference_sandbox.create_sandbox(&req.model_id) {
        Ok(session) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "session_id": session.session_id,
                "model_id": session.model_id,
                "status": session.status.to_string(),
                "created_at": session.created_at.to_rfc3339(),
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

/// POST /v1/sandbox/{id}/execute — Execute inference in a sandbox session.
pub async fn execute_inference_handler(
    State(state): State<crate::api::AppState>,
    Path(id): Path<String>,
    Json(req): Json<ExecuteInferenceRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state
        .inference_sandbox
        .execute_inference(&id, &req.input, req.max_tokens)
    {
        Ok(output) => {
            let usage = state
                .inference_sandbox
                .get_resource_usage(&id)
                .unwrap_or_default();
            let status = state
                .inference_sandbox
                .get_status(&id)
                .unwrap_or(SandboxStatus::Error);
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "session_id": id,
                    "output": output,
                    "status": status.to_string(),
                    "resource_usage": usage,
                })),
            )
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

/// GET /v1/sandbox/{id}/status — Get sandbox session status.
pub async fn get_sandbox_status_handler(
    State(state): State<crate::api::AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.inference_sandbox.sessions.get(&id) {
        Some(session) => {
            let mut s = session.clone();
            s.update_elapsed();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "session_id": s.session_id,
                    "model_id": s.model_id,
                    "status": s.status.to_string(),
                    "created_at": s.created_at.to_rfc3339(),
                    "resource_usage": s.resource_usage,
                    "error": s.error,
                })),
            )
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("Session {} not found", id)})),
        ),
    }
}

/// DELETE /v1/sandbox/{id} — Terminate a sandbox session.
pub async fn terminate_sandbox_handler(
    State(state): State<crate::api::AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.inference_sandbox.terminate(&id) {
        Ok(session) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "session_id": session.session_id,
                "status": session.status.to_string(),
                "message": "Session terminated",
            })),
        ),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

/// GET /v1/sandbox/active — List active sandbox sessions.
pub async fn list_active_sandboxes_handler(
    State(state): State<crate::api::AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let sessions = state.inference_sandbox.list_active();
    let responses: Vec<serde_json::Value> = sessions
        .iter()
        .map(|s| {
            serde_json::json!({
                "session_id": s.session_id,
                "model_id": s.model_id,
                "status": s.status.to_string(),
                "created_at": s.created_at.to_rfc3339(),
            })
        })
        .collect();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "sessions": responses,
            "count": responses.len(),
        })),
    )
}

/// GET /v1/sandbox/{id}/resources — Get resource usage for a session.
pub async fn get_sandbox_resources_handler(
    State(state): State<crate::api::AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.inference_sandbox.get_resource_usage(&id) {
        Ok(usage) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "session_id": id,
                "resource_usage": usage,
            })),
        ),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

/// GET /v1/sandbox/config — Get sandbox configuration.
pub async fn get_sandbox_config_handler(
    State(state): State<crate::api::AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state.inference_sandbox.get_config() {
        Ok(config) => (StatusCode::OK, Json(serde_json::json!(config))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the inference sandbox router.
pub fn build_inference_sandbox_router(state: crate::api::AppState) -> axum::Router {
    use axum::routing::{delete, get, post};

    axum::Router::new()
        .route("/v1/sandbox/create", post(create_sandbox_handler))
        .route("/v1/sandbox/{id}/execute", post(execute_inference_handler))
        .route("/v1/sandbox/{id}/status", get(get_sandbox_status_handler))
        .route("/v1/sandbox/{id}", delete(terminate_sandbox_handler))
        .route("/v1/sandbox/active", get(list_active_sandboxes_handler))
        .route("/v1/sandbox/{id}/resources", get(get_sandbox_resources_handler))
        .route("/v1/sandbox/config", get(get_sandbox_config_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_sandbox() -> InferenceSandbox {
        InferenceSandbox::with_config(SandboxConfig {
            max_cpu_percent: 80,
            max_memory_mb: 512,
            max_execution_secs: 60,
            max_output_tokens: 1024,
            network_access: false,
            allow_filesystem: false,
        })
    }

    #[test]
    fn test_sandbox_config_defaults() {
        let config = SandboxConfig::default();
        assert_eq!(config.max_cpu_percent, 80);
        assert_eq!(config.max_memory_mb, 4096);
        assert_eq!(config.max_execution_secs, 300);
        assert_eq!(config.max_output_tokens, 8192);
        assert!(!config.network_access);
        assert!(!config.allow_filesystem);
    }

    #[test]
    fn test_resource_limit_from_config() {
        let config = SandboxConfig::default();
        let limits = ResourceLimit::from(&config);
        assert_eq!(limits.cpu_quota, 80);
        assert_eq!(limits.memory_limit, 4096 * 1024 * 1024);
        assert_eq!(limits.timeout, 300);
        assert_eq!(limits.output_limit, 8192);
    }

    #[test]
    fn test_create_sandbox() {
        let sandbox = create_test_sandbox();
        let session = sandbox.create_sandbox("test-model").unwrap();
        assert_eq!(session.model_id, "test-model");
        assert_eq!(session.status, SandboxStatus::Ready);
        assert!(session.session_id.len() > 0);
        assert_eq!(sandbox.get_stats().total_sessions, 1);
    }

    #[test]
    fn test_execute_inference_success() {
        let sandbox = create_test_sandbox();
        let session = sandbox.create_sandbox("test-model").unwrap();
        let result = sandbox.execute_inference(&session.session_id, "Hello world", None);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("sandbox"));
        assert_eq!(sandbox.get_status(&session.session_id).unwrap(), SandboxStatus::Completed);
    }

    #[test]
    fn test_execute_inference_invalid_session() {
        let sandbox = create_test_sandbox();
        let result = sandbox.execute_inference("nonexistent", "Hello", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_terminate_session() {
        let sandbox = create_test_sandbox();
        let session = sandbox.create_sandbox("test-model").unwrap();
        let terminated = sandbox.terminate(&session.session_id).unwrap();
        assert!(sandbox.sessions.get(&session.session_id).is_none());
        assert_eq!(terminated.session_id, session.session_id);
    }

    #[test]
    fn test_terminate_nonexistent_session() {
        let sandbox = create_test_sandbox();
        let result = sandbox.terminate("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_active_sessions() {
        let sandbox = create_test_sandbox();
        let s1 = sandbox.create_sandbox("model-a").unwrap();
        let _s2 = sandbox.create_sandbox("model-b").unwrap();
        let active = sandbox.list_active();
        assert_eq!(active.len(), 2);

        // Execute inference on s1 — it completes, so it's no longer "active"
        sandbox.execute_inference(&s1.session_id, "test", None).unwrap();
        let active = sandbox.list_active();
        assert_eq!(active.len(), 1);
    }

    #[test]
    fn test_get_resource_usage() {
        let sandbox = create_test_sandbox();
        let session = sandbox.create_sandbox("test-model").unwrap();
        let usage = sandbox.get_resource_usage(&session.session_id).unwrap();
        assert_eq!(usage.requests_processed, 0);

        sandbox.execute_inference(&session.session_id, "test input", None).unwrap();
        let usage = sandbox.get_resource_usage(&session.session_id).unwrap();
        assert_eq!(usage.requests_processed, 1);
        assert!(usage.output_tokens > 0);
    }

    #[test]
    fn test_update_config() {
        let sandbox = create_test_sandbox();
        let new_config = SandboxConfig {
            max_cpu_percent: 50,
            max_memory_mb: 1024,
            max_execution_secs: 120,
            max_output_tokens: 2048,
            network_access: true,
            allow_filesystem: true,
        };
        sandbox.update_config(new_config.clone()).unwrap();
        let read_config = sandbox.get_config().unwrap();
        assert_eq!(read_config.max_cpu_percent, 50);
        assert_eq!(read_config.max_memory_mb, 1024);
        assert!(read_config.network_access);
        assert!(read_config.allow_filesystem);
    }

    #[test]
    fn test_sandbox_stats() {
        let sandbox = create_test_sandbox();
        let s1 = sandbox.create_sandbox("model-a").unwrap();
        let s2 = sandbox.create_sandbox("model-b").unwrap();
        sandbox.execute_inference(&s1.session_id, "test", None).unwrap();
        sandbox.terminate(&s2.session_id).unwrap();

        let stats = sandbox.get_stats();
        assert_eq!(stats.total_sessions, 2);
        assert_eq!(stats.total_completed, 1);
        assert_eq!(stats.active_sessions, 0);
        // Termination is not an error
        assert_eq!(stats.total_errors, 0);
    }

    #[test]
    fn test_prune_sessions() {
        let sandbox = create_test_sandbox();
        let session = sandbox.create_sandbox("test-model").unwrap();
        sandbox.execute_inference(&session.session_id, "test", None).unwrap();
        // Prune sessions older than 0 seconds — should remove completed ones
        let pruned = sandbox.prune_sessions(0);
        assert!(pruned >= 1);
    }

    #[test]
    fn test_sandbox_status_display() {
        assert_eq!(SandboxStatus::Ready.to_string(), "ready");
        assert_eq!(SandboxStatus::Running.to_string(), "running");
        assert_eq!(SandboxStatus::Completed.to_string(), "completed");
        assert_eq!(SandboxStatus::TimedOut.to_string(), "timed_out");
        assert_eq!(SandboxStatus::ResourceExceeded.to_string(), "resource_exceeded");
        assert_eq!(SandboxStatus::Error.to_string(), "error");
    }
}
