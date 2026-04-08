//! Deployment Dashboard for the Xergon Network marketplace.
//!
//! Provides REST API handlers for managing deployments, health checks,
//! blue-green slot switching, canary deployments, rollback, and
//! environment configuration.
//!
//! Endpoints are nested under `/v1/deployments`.

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json, Router,
    routing::{get, post},
};
use chrono::Utc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

// ================================================================
// Data Types
// ================================================================

/// Deployment status lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeployStatus {
    Pending,
    Running,
    Canary,
    Healthy,
    Failed,
    RolledBack,
    Aborted,
}

/// Blue-green slot name.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SlotName {
    Blue,
    Green,
}

impl std::fmt::Display for SlotName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SlotName::Blue => write!(f, "blue"),
            SlotName::Green => write!(f, "green"),
        }
    }
}

/// Information about a single blue-green slot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotInfo {
    pub version: String,
    pub url: String,
    pub healthy: bool,
    pub last_health_check: i64,
    pub error_rate: f64,
}

impl Default for SlotInfo {
    fn default() -> Self {
        Self {
            version: String::new(),
            url: String::new(),
            healthy: false,
            last_health_check: 0,
            error_rate: 0.0,
        }
    }
}

/// Blue-green deployment slots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploySlots {
    pub blue: SlotInfo,
    pub green: SlotInfo,
    pub active: SlotName,
}

impl Default for DeploySlots {
    fn default() -> Self {
        Self {
            blue: SlotInfo::default(),
            green: SlotInfo::default(),
            active: SlotName::Blue,
        }
    }
}

/// A deployment record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deployment {
    pub id: String,
    pub env: String,
    pub version: String,
    pub status: DeployStatus,
    pub started_at: i64,
    pub completed_at: Option<i64>,
    pub deployed_by: String,
    pub health_score: f64,
    pub canary_percent: u8,
    pub slots: DeploySlots,
}

/// Deployment event types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeployEventType {
    Started,
    HealthCheckPassed,
    HealthCheckFailed,
    CanaryStarted,
    CanaryPassed,
    CanaryFailed,
    SwitchedTraffic,
    RolledBack,
    Completed,
    Failed,
}

impl std::fmt::Display for DeployEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeployEventType::Started => write!(f, "started"),
            DeployEventType::HealthCheckPassed => write!(f, "health_check_passed"),
            DeployEventType::HealthCheckFailed => write!(f, "health_check_failed"),
            DeployEventType::CanaryStarted => write!(f, "canary_started"),
            DeployEventType::CanaryPassed => write!(f, "canary_passed"),
            DeployEventType::CanaryFailed => write!(f, "canary_failed"),
            DeployEventType::SwitchedTraffic => write!(f, "switched_traffic"),
            DeployEventType::RolledBack => write!(f, "rolled_back"),
            DeployEventType::Completed => write!(f, "completed"),
            DeployEventType::Failed => write!(f, "failed"),
        }
    }
}

/// An event in the deployment lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployEvent {
    pub id: String,
    pub deployment_id: String,
    pub event_type: DeployEventType,
    pub message: String,
    pub timestamp: i64,
    pub details: serde_json::Value,
}

/// Environment configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentConfig {
    pub env: String,
    pub relay_url: String,
    pub agent_url: String,
    pub health_check_path: String,
    pub health_check_timeout_secs: u64,
    pub max_retries: u32,
    pub rollback_on_failure: bool,
    pub canary_percent: u8,
}

impl Default for EnvironmentConfig {
    fn default() -> Self {
        Self {
            env: String::new(),
            relay_url: String::new(),
            agent_url: String::new(),
            health_check_path: "/health".to_string(),
            health_check_timeout_secs: 30,
            max_retries: 3,
            rollback_on_failure: true,
            canary_percent: 10,
        }
    }
}

/// Deployment summary for an environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploySummary {
    pub total_deployments: u64,
    pub success_rate: f64,
    pub avg_deploy_time_secs: u64,
    pub current_versions: HashMap<String, String>,
    pub last_failure: Option<String>,
}

// ================================================================
// Query / Request Types
// ================================================================

#[derive(Debug, Deserialize)]
pub struct CreateDeployRequest {
    pub env: String,
    pub version: String,
    pub deployed_by: Option<String>,
    pub canary_percent: Option<u8>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateConfigRequest {
    pub relay_url: Option<String>,
    pub agent_url: Option<String>,
    pub health_check_path: Option<String>,
    pub health_check_timeout_secs: Option<u64>,
    pub max_retries: Option<u32>,
    pub rollback_on_failure: Option<bool>,
    pub canary_percent: Option<u8>,
}

#[derive(Debug, Deserialize)]
pub struct ListDeploymentsQuery {
    pub env: Option<String>,
    pub limit: Option<usize>,
}

// ================================================================
// Deploy Dashboard
// ================================================================

/// Deployment dashboard managing blue-green slots, canary, health checks, and events.
pub struct DeployDashboard {
    deployments: DashMap<String, Deployment>,
    events: DashMap<String, Vec<DeployEvent>>,
    configs: DashMap<String, EnvironmentConfig>,
    event_counter: std::sync::atomic::AtomicU64,
}

impl DeployDashboard {
    /// Create a new deployment dashboard.
    pub fn new() -> Self {
        Self {
            deployments: DashMap::new(),
            events: DashMap::new(),
            configs: DashMap::new(),
            event_counter: std::sync::atomic::AtomicU64::new(0),
        }
    }

    // ============================================================
    // Core Operations
    // ============================================================

    /// Start a new deployment.
    pub fn create_deployment(
        &self,
        env: &str,
        version: &str,
        deployed_by: &str,
        canary_percent: u8,
    ) -> Deployment {
        let now = Utc::now().timestamp_millis();
        let uuid_str = uuid::Uuid::new_v4().to_string().replace('-', "");
        let id = format!("dep_{}", &uuid_str[..uuid_str.len().min(16)]);

        // Determine the inactive slot for the new version
        let _config = self.configs.get(env).map(|c| c.value().clone());
        let current = self
            .deployments
            .iter()
            .find(|e| e.value().env == env && e.value().status == DeployStatus::Healthy)
            .map(|e| e.value().clone());

        let (blue_slot, green_slot, active_slot) = match &current {
            Some(cur) => {
                let new_inactive = if cur.slots.active == SlotName::Blue {
                    SlotName::Green
                } else {
                    SlotName::Blue
                };
                let mut new_slot = SlotInfo::default();
                new_slot.version = version.to_string();
                new_slot.healthy = false;

                match new_inactive {
                    SlotName::Blue => (new_slot, cur.slots.blue.clone(), cur.slots.active),
                    SlotName::Green => (cur.slots.green.clone(), new_slot, cur.slots.active),
                }
            }
            None => {
                let mut blue_slot = SlotInfo::default();
                blue_slot.version = version.to_string();
                blue_slot.healthy = false;
                let green_slot = SlotInfo::default();
                (blue_slot, green_slot, SlotName::Blue)
            }
        };

        let deployment = Deployment {
            id: id.clone(),
            env: env.to_string(),
            version: version.to_string(),
            status: if canary_percent > 0 {
                DeployStatus::Canary
            } else {
                DeployStatus::Running
            },
            started_at: now,
            completed_at: None,
            deployed_by: deployed_by.to_string(),
            health_score: 0.0,
            canary_percent,
            slots: DeploySlots {
                blue: blue_slot,
                green: green_slot,
                active: active_slot,
            },
        };

        self.record_event(&id, DeployEventType::Started, &format!("Deployment {} started for env {}", version, env), serde_json::json!({ "env": env, "version": version }));

        if canary_percent > 0 {
            self.record_event(&id, DeployEventType::CanaryStarted, &format!("Canary deployment at {}%", canary_percent), serde_json::json!({ "canary_percent": canary_percent }));
        }

        self.deployments.insert(id.clone(), deployment.clone());
        info!(deployment_id = %id, env = %env, version = %version, "Deployment created");
        deployment
    }

    /// Run health check on a deployment.
    pub fn run_health_check(&self, deployment_id: &str) -> Result<(bool, f64), String> {
        let (healthy, health_score, event_type, event_msg, event_details) = {
            let mut dep_ref = self
                .deployments
                .get_mut(deployment_id)
                .ok_or("Deployment not found")?;
            let deployment = dep_ref.value_mut();

            if deployment.status != DeployStatus::Running && deployment.status != DeployStatus::Canary {
                return Err("Deployment is not in a state that accepts health checks".to_string());
            }

            // Simulate health check: check the inactive slot
            let inactive_slot = if deployment.slots.active == SlotName::Blue {
                &mut deployment.slots.green
            } else {
                &mut deployment.slots.blue
            };

            let now = Utc::now().timestamp_millis();
            // Simulate: if version contains "bad" it fails, otherwise passes
            let healthy = !inactive_slot.version.contains("bad");
            inactive_slot.healthy = healthy;
            inactive_slot.last_health_check = now;

            // Simulate latency
            let latency = 50.0 + (now as f64 % 200.0);
            let health_score = if healthy { 100.0 } else { 0.0 };

            deployment.health_score = health_score;

            if healthy {
                (
                    healthy,
                    health_score,
                    DeployEventType::HealthCheckPassed,
                    "Health check passed".to_string(),
                    serde_json::json!({ "score": health_score, "latency_ms": latency }),
                )
            } else {
                (
                    healthy,
                    health_score,
                    DeployEventType::HealthCheckFailed,
                    "Health check failed".to_string(),
                    serde_json::json!({ "score": health_score }),
                )
            }
        }; // dep_ref dropped here

        self.record_event(deployment_id, event_type, &event_msg, event_details);
        Ok((healthy, health_score))
    }

    /// Switch traffic between blue/green slots.
    pub fn switch_traffic(&self, deployment_id: &str) -> Result<Deployment, String> {
        let (events, result) = {
            let mut dep_ref = self
                .deployments
                .get_mut(deployment_id)
                .ok_or("Deployment not found")?;
            let deployment = dep_ref.value_mut();

            if deployment.status != DeployStatus::Running && deployment.status != DeployStatus::Canary {
                return Err("Deployment is not in a state that allows traffic switch".to_string());
            }

            // Check that the inactive slot is healthy
            let inactive_slot = if deployment.slots.active == SlotName::Blue {
                &deployment.slots.green
            } else {
                &deployment.slots.blue
            };

            if !inactive_slot.healthy {
                return Err("Cannot switch traffic: inactive slot is not healthy".to_string());
            }

            // Switch active slot
            deployment.slots.active = if deployment.slots.active == SlotName::Blue {
                SlotName::Green
            } else {
                SlotName::Blue
            };

            deployment.status = DeployStatus::Healthy;
            deployment.completed_at = Some(Utc::now().timestamp_millis());
            deployment.health_score = 100.0;

            let active_slot_str = deployment.slots.active.to_string();
            let version = deployment.version.clone();

            let events = vec![
                (
                    DeployEventType::SwitchedTraffic,
                    format!("Traffic switched to {} slot", active_slot_str),
                    serde_json::json!({ "active_slot": active_slot_str }),
                ),
                (
                    DeployEventType::Completed,
                    "Deployment completed successfully".to_string(),
                    serde_json::json!({ "version": version }),
                ),
            ];

            (events, deployment.clone())
        }; // dep_ref dropped here

        for (event_type, message, details) in events {
            self.record_event(deployment_id, event_type, &message, details);
        }

        info!(deployment_id = %deployment_id, slot = %result.slots.active, "Traffic switched");
        Ok(result)
    }

    /// Roll back a deployment.
    pub fn rollback(&self, deployment_id: &str) -> Result<Deployment, String> {
        let (event_data, result) = {
            let mut dep_ref = self
                .deployments
                .get_mut(deployment_id)
                .ok_or("Deployment not found")?;
            let deployment = dep_ref.value_mut();

            if deployment.status == DeployStatus::RolledBack {
                return Err("Deployment already rolled back".to_string());
            }

            let previous_version = deployment.version.clone();
            deployment.status = DeployStatus::RolledBack;
            deployment.completed_at = Some(Utc::now().timestamp_millis());

            // Switch back to the old slot
            deployment.slots.active = if deployment.slots.active == SlotName::Blue {
                SlotName::Green
            } else {
                SlotName::Blue
            };

            let event_data = (
                DeployEventType::RolledBack,
                format!("Rolled back from version {}", previous_version),
                serde_json::json!({ "previous_version": previous_version }),
            );

            (event_data, deployment.clone())
        }; // dep_ref dropped here

        self.record_event(deployment_id, event_data.0, &event_data.1, event_data.2);

        warn!(deployment_id = %deployment_id, "Deployment rolled back");
        Ok(result)
    }

    /// Get a deployment by ID.
    pub fn get_deployment(&self, id: &str) -> Option<Deployment> {
        self.deployments.get(id).map(|r| r.value().clone())
    }

    /// List deployments, optionally filtered by environment.
    pub fn list_deployments(&self, env: Option<&str>, limit: usize) -> Vec<Deployment> {
        let mut results: Vec<Deployment> = self
            .deployments
            .iter()
            .filter(|e| env.map(|s| e.value().env == s).unwrap_or(true))
            .map(|e| e.value().clone())
            .collect();
        results.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        results.truncate(limit);
        results
    }

    /// Get events for a deployment.
    pub fn get_events(&self, deployment_id: &str) -> Vec<DeployEvent> {
        self.events
            .get(deployment_id)
            .map(|e| e.value().clone())
            .unwrap_or_default()
    }

    /// Get deployment summary for an environment.
    pub fn get_summary(&self, env: &str) -> DeploySummary {
        let env_deployments: Vec<Deployment> = self
            .deployments
            .iter()
            .filter(|e| e.value().env == env)
            .map(|e| e.value().clone())
            .collect();

        let total = env_deployments.len() as u64;
        let success_count = env_deployments
            .iter()
            .filter(|d| d.status == DeployStatus::Healthy)
            .count() as u64;
        let success_rate = if total > 0 {
            (success_count as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let avg_deploy_time: u64 = {
            let times: Vec<i64> = env_deployments
                .iter()
                .filter_map(|d| {
                    d.completed_at.map(|c| (c - d.started_at).abs())
                })
                .collect();
            if times.is_empty() {
                0
            } else {
                (times.iter().sum::<i64>() / times.len() as i64 / 1000).max(0) as u64
            }
        };

        let current_versions: HashMap<String, String> = env_deployments
            .iter()
            .filter(|d| d.status == DeployStatus::Healthy)
            .map(|d| (d.version.clone(), d.id.clone()))
            .collect();

        let last_failure = env_deployments
            .iter()
            .find(|d| d.status == DeployStatus::Failed)
            .map(|d| d.id.clone());

        DeploySummary {
            total_deployments: total,
            success_rate,
            avg_deploy_time_secs: avg_deploy_time,
            current_versions,
            last_failure,
        }
    }

    /// Update environment configuration.
    pub fn update_config(&self, env: &str, update: UpdateConfigRequest) -> EnvironmentConfig {
        let mut config = self
            .configs
            .get(env)
            .map(|c| c.value().clone())
            .unwrap_or_else(|| EnvironmentConfig {
                env: env.to_string(),
                ..Default::default()
            });

        if let Some(relay_url) = update.relay_url {
            config.relay_url = relay_url;
        }
        if let Some(agent_url) = update.agent_url {
            config.agent_url = agent_url;
        }
        if let Some(health_check_path) = update.health_check_path {
            config.health_check_path = health_check_path;
        }
        if let Some(health_check_timeout_secs) = update.health_check_timeout_secs {
            config.health_check_timeout_secs = health_check_timeout_secs;
        }
        if let Some(max_retries) = update.max_retries {
            config.max_retries = max_retries;
        }
        if let Some(rollback_on_failure) = update.rollback_on_failure {
            config.rollback_on_failure = rollback_on_failure;
        }
        if let Some(canary_percent) = update.canary_percent {
            config.canary_percent = canary_percent;
        }

        config.env = env.to_string();
        self.configs.insert(env.to_string(), config.clone());
        debug!(env = %env, "Environment config updated");
        config
    }

    /// Get environment configuration.
    pub fn get_config(&self, env: &str) -> Option<EnvironmentConfig> {
        self.configs.get(env).map(|c| c.value().clone())
    }

    // ============================================================
    // Internal
    // ============================================================

    fn record_event(
        &self,
        deployment_id: &str,
        event_type: DeployEventType,
        message: &str,
        details: serde_json::Value,
    ) {
        let counter = self
            .event_counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let event = DeployEvent {
            id: format!("evt_{}", counter),
            deployment_id: deployment_id.to_string(),
            event_type,
            message: message.to_string(),
            timestamp: Utc::now().timestamp_millis(),
            details,
        };

        self.events
            .entry(deployment_id.to_string())
            .or_insert_with(Vec::new)
            .push(event);
    }
}

impl Default for DeployDashboard {
    fn default() -> Self {
        Self::new()
    }
}

// ================================================================
// API Handlers
// ================================================================

/// Shared state type for handlers.
pub type DashboardState = Arc<DeployDashboard>;

/// Handler: POST /v1/deployments
pub async fn create_deployment_handler(
    State(dashboard): State<DashboardState>,
    Json(req): Json<CreateDeployRequest>,
) -> (StatusCode, Json<Deployment>) {
    let deployed_by = req.deployed_by.unwrap_or_else(|| "api_user".to_string());
    let canary_percent = req.canary_percent.unwrap_or(0);
    let deployment = dashboard.create_deployment(&req.env, &req.version, &deployed_by, canary_percent);
    (StatusCode::CREATED, Json(deployment))
}

/// Handler: GET /v1/deployments/:id
pub async fn get_deployment_handler(
    State(dashboard): State<DashboardState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<Option<Deployment>>) {
    let deployment = dashboard.get_deployment(&id);
    let status = if deployment.is_some() {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    };
    (status, Json(deployment))
}

/// Handler: POST /v1/deployments/:id/health-check
pub async fn health_check_handler(
    State(dashboard): State<DashboardState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match dashboard.run_health_check(&id) {
        Ok((healthy, score)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "healthy": healthy,
                "health_score": score,
            })),
        ),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

/// Handler: POST /v1/deployments/:id/switch
pub async fn switch_traffic_handler(
    State(dashboard): State<DashboardState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match dashboard.switch_traffic(&id) {
        Ok(deployment) => (StatusCode::OK, Json(serde_json::json!(deployment))),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

/// Handler: POST /v1/deployments/:id/rollback
pub async fn rollback_handler(
    State(dashboard): State<DashboardState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    match dashboard.rollback(&id) {
        Ok(deployment) => (StatusCode::OK, Json(serde_json::json!(deployment))),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

/// Handler: GET /v1/deployments/:id/events
pub async fn get_events_handler(
    State(dashboard): State<DashboardState>,
    Path(id): Path<String>,
) -> Json<Vec<DeployEvent>> {
    let events = dashboard.get_events(&id);
    Json(events)
}

/// Handler: GET /v1/deployments
pub async fn list_deployments_handler(
    State(dashboard): State<DashboardState>,
    Query(query): Query<ListDeploymentsQuery>,
) -> Json<Vec<Deployment>> {
    let env = query.env.as_deref();
    let limit = query.limit.unwrap_or(50);
    let deployments = dashboard.list_deployments(env, limit);
    Json(deployments)
}

/// Handler: GET /v1/deployments/summary/:env
pub async fn get_summary_handler(
    State(dashboard): State<DashboardState>,
    Path(env): Path<String>,
) -> Json<DeploySummary> {
    let summary = dashboard.get_summary(&env);
    Json(summary)
}

/// Handler: PUT /v1/deployments/config/:env
pub async fn update_config_handler(
    State(dashboard): State<DashboardState>,
    Path(env): Path<String>,
    Json(req): Json<UpdateConfigRequest>,
) -> (StatusCode, Json<EnvironmentConfig>) {
    let config = dashboard.update_config(&env, req);
    (StatusCode::OK, Json(config))
}

/// Handler: GET /v1/deployments/config/:env
pub async fn get_config_handler(
    State(dashboard): State<DashboardState>,
    Path(env): Path<String>,
) -> (StatusCode, Json<Option<EnvironmentConfig>>) {
    let config = dashboard.get_config(&env);
    let status = if config.is_some() {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    };
    (status, Json(config))
}

// ================================================================
// Router
// ================================================================

pub fn deployment_dashboard_routes() -> Router<DashboardState> {
    Router::new()
        .route("/v1/deployments", post(create_deployment_handler).get(list_deployments_handler))
        .route("/v1/deployments/summary/:env", get(get_summary_handler))
        .route("/v1/deployments/config/:env", get(get_config_handler).put(update_config_handler))
        .route("/v1/deployments/:id", get(get_deployment_handler))
        .route("/v1/deployments/:id/health-check", post(health_check_handler))
        .route("/v1/deployments/:id/switch", post(switch_traffic_handler))
        .route("/v1/deployments/:id/rollback", post(rollback_handler))
        .route("/v1/deployments/:id/events", get(get_events_handler))
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_dashboard() -> DeployDashboard {
        DeployDashboard::new()
    }

    // ----------------------------------------------------------------
    // test_create_deployment
    // ----------------------------------------------------------------
    #[test]
    fn test_create_deployment() {
        let dash = make_dashboard();
        let dep = dash.create_deployment("prod", "1.2.0", "alice", 0);
        assert_eq!(dep.env, "prod");
        assert_eq!(dep.version, "1.2.0");
        assert_eq!(dep.deployed_by, "alice");
        assert_eq!(dep.status, DeployStatus::Running);
        assert!(dep.started_at > 0);
        assert!(dep.completed_at.is_none());
        assert!(dep.id.starts_with("dep_"));
    }

    // ----------------------------------------------------------------
    // test_health_check_pass
    // ----------------------------------------------------------------
    #[test]
    fn test_health_check_pass() {
        let dash = make_dashboard();
        let dep = dash.create_deployment("prod", "1.2.0", "alice", 0);
        let result = dash.run_health_check(&dep.id);
        assert!(result.is_ok());
        let (healthy, score) = result.unwrap();
        assert!(healthy);
        assert_eq!(score, 100.0);
    }

    // ----------------------------------------------------------------
    // test_health_check_fail
    // ----------------------------------------------------------------
    #[test]
    fn test_health_check_fail() {
        let dash = make_dashboard();
        let dep = dash.create_deployment("prod", "bad-version", "alice", 0);
        let result = dash.run_health_check(&dep.id);
        assert!(result.is_ok());
        let (healthy, score) = result.unwrap();
        assert!(!healthy);
        assert_eq!(score, 0.0);
    }

    // ----------------------------------------------------------------
    // test_switch_traffic
    // ----------------------------------------------------------------
    #[test]
    fn test_switch_traffic() {
        let dash = make_dashboard();
        let dep = dash.create_deployment("prod", "1.2.0", "alice", 0);
        // Health check must pass first
        dash.run_health_check(&dep.id).unwrap();
        let result = dash.switch_traffic(&dep.id);
        assert!(result.is_ok());
        let updated = result.unwrap();
        assert_eq!(updated.status, DeployStatus::Healthy);
        assert!(updated.completed_at.is_some());
        // Active slot should have switched
        let original = dash.get_deployment(&dep.id).unwrap();
        assert_eq!(original.status, DeployStatus::Healthy);
    }

    // ----------------------------------------------------------------
    // test_rollback
    // ----------------------------------------------------------------
    #[test]
    fn test_rollback() {
        let dash = make_dashboard();
        let dep = dash.create_deployment("prod", "1.2.0", "alice", 0);
        dash.run_health_check(&dep.id).unwrap();
        dash.switch_traffic(&dep.id).unwrap();

        let result = dash.rollback(&dep.id);
        assert!(result.is_ok());
        let rolled_back = result.unwrap();
        assert_eq!(rolled_back.status, DeployStatus::RolledBack);
        assert!(rolled_back.completed_at.is_some());
    }

    // ----------------------------------------------------------------
    // test_deployment_events
    // ----------------------------------------------------------------
    #[test]
    fn test_deployment_events() {
        let dash = make_dashboard();
        let dep = dash.create_deployment("prod", "1.2.0", "alice", 0);
        let events = dash.get_events(&dep.id);
        assert!(!events.is_empty());
        assert_eq!(events[0].event_type, DeployEventType::Started);

        dash.run_health_check(&dep.id).unwrap();
        let events = dash.get_events(&dep.id);
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].event_type, DeployEventType::HealthCheckPassed);
    }

    // ----------------------------------------------------------------
    // test_list_deployments_with_filter
    // ----------------------------------------------------------------
    #[test]
    fn test_list_deployments_with_filter() {
        let dash = make_dashboard();
        dash.create_deployment("prod", "1.0.0", "alice", 0);
        dash.create_deployment("staging", "2.0.0", "bob", 0);
        dash.create_deployment("prod", "1.1.0", "carol", 0);

        let all = dash.list_deployments(None, 100);
        assert_eq!(all.len(), 3);

        let prod = dash.list_deployments(Some("prod"), 100);
        assert_eq!(prod.len(), 2);

        let staging = dash.list_deployments(Some("staging"), 100);
        assert_eq!(staging.len(), 1);
        assert_eq!(staging[0].version, "2.0.0");
    }

    // ----------------------------------------------------------------
    // test_summary_calculation
    // ----------------------------------------------------------------
    #[test]
    fn test_summary_calculation() {
        let dash = make_dashboard();
        dash.create_deployment("prod", "1.0.0", "alice", 0);
        let dep2 = dash.create_deployment("prod", "1.1.0", "bob", 0);
        dash.run_health_check(&dep2.id).unwrap();
        dash.switch_traffic(&dep2.id).unwrap();

        let summary = dash.get_summary("prod");
        assert_eq!(summary.total_deployments, 2);
        // 1 healthy, 1 running = 50% success
        assert!((summary.success_rate - 50.0).abs() < 0.1);
    }

    // ----------------------------------------------------------------
    // test_config_management
    // ----------------------------------------------------------------
    #[test]
    fn test_config_management() {
        let dash = make_dashboard();

        // No config initially
        assert!(dash.get_config("prod").is_none());

        // Update config
        let config = dash.update_config(
            "prod",
            UpdateConfigRequest {
                relay_url: Some("https://relay.prod.xergon.network".to_string()),
                agent_url: Some("https://agent.prod.xergon.network".to_string()),
                health_check_path: Some("/healthz".to_string()),
                health_check_timeout_secs: Some(60),
                max_retries: Some(5),
                rollback_on_failure: Some(false),
                canary_percent: Some(25),
            },
        );

        assert_eq!(config.relay_url, "https://relay.prod.xergon.network");
        assert_eq!(config.agent_url, "https://agent.prod.xergon.network");
        assert_eq!(config.health_check_path, "/healthz");
        assert_eq!(config.health_check_timeout_secs, 60);
        assert_eq!(config.max_retries, 5);
        assert!(!config.rollback_on_failure);
        assert_eq!(config.canary_percent, 25);

        // Retrieve config
        let retrieved = dash.get_config("prod").unwrap();
        assert_eq!(retrieved.relay_url, config.relay_url);
    }

    // ----------------------------------------------------------------
    // test_deploy_status_transitions
    // ----------------------------------------------------------------
    #[test]
    fn test_deploy_status_transitions() {
        let dash = make_dashboard();

        // Create -> Running
        let dep = dash.create_deployment("prod", "1.0.0", "alice", 0);
        assert_eq!(dep.status, DeployStatus::Running);

        // Running -> (health check pass) -> switch -> Healthy
        dash.run_health_check(&dep.id).unwrap();
        dash.switch_traffic(&dep.id).unwrap();
        let d = dash.get_deployment(&dep.id).unwrap();
        assert_eq!(d.status, DeployStatus::Healthy);

        // Healthy -> RolledBack
        dash.rollback(&dep.id).unwrap();
        let d = dash.get_deployment(&dep.id).unwrap();
        assert_eq!(d.status, DeployStatus::RolledBack);

        // RolledBack -> error on rollback again
        let result = dash.rollback(&dep.id);
        assert!(result.is_err());
    }

    // ----------------------------------------------------------------
    // test_concurrent_deployments
    // ----------------------------------------------------------------
    #[test]
    fn test_concurrent_deployments() {
        use std::sync::Arc;

        let dash = Arc::new(make_dashboard());
        let mut handles = vec![];

        for i in 0..10 {
            let d = dash.clone();
            handles.push(std::thread::spawn(move || {
                let version = format!("1.{}.0", i);
                d.create_deployment("prod", &version, "concurrent", 0);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let all = dash.list_deployments(Some("prod"), 100);
        assert_eq!(all.len(), 10);
    }

    // ----------------------------------------------------------------
    // test_canary_flow
    // ----------------------------------------------------------------
    #[test]
    fn test_canary_flow() {
        let dash = make_dashboard();

        // Create with canary
        let dep = dash.create_deployment("prod", "2.0.0", "alice", 10);
        assert_eq!(dep.status, DeployStatus::Canary);
        assert_eq!(dep.canary_percent, 10);

        // Check events include canary start
        let events = dash.get_events(&dep.id);
        let canary_events: Vec<_> = events.iter().filter(|e| e.event_type == DeployEventType::CanaryStarted).collect();
        assert_eq!(canary_events.len(), 1);

        // Health check should work on canary
        let result = dash.run_health_check(&dep.id);
        assert!(result.is_ok());

        // Switch should work
        dash.switch_traffic(&dep.id).unwrap();
        let updated = dash.get_deployment(&dep.id).unwrap();
        assert_eq!(updated.status, DeployStatus::Healthy);
    }
}
