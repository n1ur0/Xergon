//! Multi-Agent Orchestration REST API.
//!
//! Provides endpoints for task scheduling, agent registration, load balancing,
//! and fan-out/fan-in orchestration:
//!
//! - `POST   /v1/orchestration/tasks`               -- submit a task
//! - `GET    /v1/orchestration/tasks`               -- list tasks (filtered)
//! - `GET    /v1/orchestration/tasks/{id}`          -- get task by id
//! - `POST   /v1/orchestration/tasks/{id}/assign`   -- assign to best agent
//! - `POST   /v1/orchestration/tasks/{id}/complete` -- mark complete
//! - `POST   /v1/orchestration/tasks/{id}/fail`     -- mark failed
//! - `POST   /v1/orchestration/tasks/{id}/cancel`   -- cancel task + subtasks
//! - `POST   /v1/orchestration/fanout`              -- submit fanout
//! - `GET    /v1/orchestration/stats`               -- orchestrator stats
//! - `POST   /v1/orchestration/agents/register`     -- register agent
//! - `DELETE /v1/orchestration/agents/{pk}`         -- unregister agent
//! - `GET    /v1/orchestration/agents`              -- list agents

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{delete, get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::orchestration::{
    AgentCapability, OrchestrationError, Orchestrator, RegisteredAgent,
    Task, TaskFilter, TaskPriority, TaskResult, TaskType,
};

// ---------------------------------------------------------------------------
// Shared state
// ---------------------------------------------------------------------------

/// State shared across orchestration API handlers.
#[derive(Clone)]
pub struct OrchestrationState {
    pub orchestrator: Arc<Orchestrator>,
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the orchestration API router.
pub fn build_orchestration_router(orchestrator: Arc<Orchestrator>) -> Router {
    let state = OrchestrationState { orchestrator };
    Router::new()
        .route("/v1/orchestration/tasks", post(submit_task).get(list_tasks))
        .route(
            "/v1/orchestration/tasks/{task_id}",
            get(get_task),
        )
        .route(
            "/v1/orchestration/tasks/{task_id}/assign",
            post(assign_task),
        )
        .route(
            "/v1/orchestration/tasks/{task_id}/complete",
            post(complete_task),
        )
        .route(
            "/v1/orchestration/tasks/{task_id}/fail",
            post(fail_task),
        )
        .route(
            "/v1/orchestration/tasks/{task_id}/cancel",
            post(cancel_task),
        )
        .route("/v1/orchestration/fanout", post(submit_fanout))
        .route("/v1/orchestration/stats", get(get_stats))
        .route(
            "/v1/orchestration/agents/register",
            post(register_agent),
        )
        .route(
            "/v1/orchestration/agents/{agent_pk}",
            delete(unregister_agent),
        )
        .route("/v1/orchestration/agents", get(list_agents))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

fn json_error(status: StatusCode, error_type: &str, message: &str) -> impl IntoResponse {
    (
        status,
        Json(serde_json::json!({
            "error": {
                "type": error_type,
                "message": message,
                "code": status.as_u16(),
            }
        })),
    )
}

fn orchestration_error_to_response(err: OrchestrationError) -> impl IntoResponse {
    let (status, error_type) = match &err {
        OrchestrationError::TaskNotFound(_) => (StatusCode::NOT_FOUND, "task_not_found"),
        OrchestrationError::AgentNotFound(_) => (StatusCode::NOT_FOUND, "agent_not_found"),
        OrchestrationError::MaxParallelTasks(_) => (StatusCode::CONFLICT, "max_parallel_tasks"),
        OrchestrationError::NoAvailableAgents => (StatusCode::SERVICE_UNAVAILABLE, "no_available_agents"),
        OrchestrationError::InvalidTransition(_) => (StatusCode::CONFLICT, "invalid_transition"),
        OrchestrationError::MaxRetriesExceeded(_) => (StatusCode::CONFLICT, "max_retries_exceeded"),
        OrchestrationError::SubtaskDepthExceeded(_) => (StatusCode::BAD_REQUEST, "max_subtask_depth"),
    };
    json_error(status, error_type, &err.to_string())
}

// ---------------------------------------------------------------------------
// Request / Response types
// ---------------------------------------------------------------------------

// -- Submit task --

#[derive(Debug, Deserialize)]
pub struct SubmitTaskRequest {
    pub task_type: TaskType,
    pub payload: serde_json::Value,
    #[serde(default)]
    pub priority: Option<TaskPriority>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub max_retries: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct SubmitTaskResponse {
    pub task_id: String,
}

// -- List tasks query params --

#[derive(Debug, Deserialize, Default)]
pub struct ListTasksQuery {
    pub task_type: Option<TaskType>,
    pub status: Option<String>,
    pub priority: Option<TaskPriority>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

// -- Complete task --

#[derive(Debug, Deserialize)]
pub struct CompleteTaskRequest {
    pub result: serde_json::Value,
    #[serde(default = "default_quality")]
    pub quality_score: f64,
    #[serde(default)]
    pub tokens_used: Option<u64>,
    #[serde(default)]
    pub latency_ms: Option<u64>,
}

fn default_quality() -> f64 {
    1.0
}

// -- Fail task --

#[derive(Debug, Deserialize)]
pub struct FailTaskRequest {
    pub error: String,
}

// -- Fanout --

#[derive(Debug, Deserialize)]
pub struct FanoutRequest {
    pub parent_task_type: TaskType,
    pub parent_payload: serde_json::Value,
    #[serde(default)]
    pub parent_priority: Option<TaskPriority>,
    pub subtasks: Vec<SubtaskDef>,
}

#[derive(Debug, Deserialize)]
pub struct SubtaskDef {
    pub task_type: TaskType,
    pub payload: serde_json::Value,
    #[serde(default)]
    pub priority: Option<TaskPriority>,
}

#[derive(Debug, Serialize)]
pub struct FanoutResponse {
    pub parent_task_id: String,
}

// -- Register agent --

#[derive(Debug, Deserialize)]
pub struct RegisterAgentRequest {
    pub pk: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub endpoint: Option<String>,
    pub capabilities: Vec<AgentCapability>,
    #[serde(default)]
    pub max_concurrent_tasks: Option<usize>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /v1/orchestration/tasks -- Submit a new task
async fn submit_task(
    State(state): State<OrchestrationState>,
    Json(body): Json<SubmitTaskRequest>,
) -> impl IntoResponse {
    let mut task = Task::new(body.task_type, body.payload);
    if let Some(p) = body.priority {
        task = task.with_priority(p);
    }
    if let Some(t) = body.timeout_secs {
        task = task.with_timeout(t);
    }
    if let Some(r) = body.max_retries {
        task = task.with_max_retries(r);
    }
    match state.orchestrator.submit_task(task) {
        Ok(id) => (StatusCode::CREATED, Json(SubmitTaskResponse { task_id: id })).into_response(),
        Err(err) => orchestration_error_to_response(err).into_response(),
    }
}

/// GET /v1/orchestration/tasks -- List tasks with optional filters
async fn list_tasks(
    State(state): State<OrchestrationState>,
    Query(q): Query<ListTasksQuery>,
) -> Json<Vec<Task>> {
    let status = q.status.as_deref().and_then(|s| {
        // Parse status string to TaskStatus enum
        serde_json::from_value(serde_json::json!(s)).ok()
    });
    let filter = TaskFilter {
        task_type: q.task_type,
        status,
        assigned_to: None,
        priority: q.priority,
        limit: q.limit,
    };
    let tasks = state.orchestrator.list_tasks(filter);
    Json(tasks)
}

/// GET /v1/orchestration/tasks/:task_id -- Get a task by ID
async fn get_task(
    State(state): State<OrchestrationState>,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    match state.orchestrator.get_task(&task_id) {
        Some(task) => (StatusCode::OK, Json(task)).into_response(),
        None => json_error(
            StatusCode::NOT_FOUND,
            "task_not_found",
            &format!("Task {} not found", task_id),
        )
        .into_response(),
    }
}

/// POST /v1/orchestration/tasks/:task_id/assign -- Assign task to best agent
async fn assign_task(
    State(state): State<OrchestrationState>,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    match state.orchestrator.assign_task(&task_id) {
        Ok(agent_pk) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "task_id": task_id,
                "assigned_to": agent_pk,
            })),
        )
            .into_response(),
        Err(err) => orchestration_error_to_response(err).into_response(),
    }
}

/// POST /v1/orchestration/tasks/:task_id/complete -- Mark a task as completed
async fn complete_task(
    State(state): State<OrchestrationState>,
    Path(task_id): Path<String>,
    Json(body): Json<CompleteTaskRequest>,
) -> impl IntoResponse {
    let result = TaskResult {
        output: body.result,
        tokens_used: body.tokens_used.unwrap_or(0),
        duration_ms: body.latency_ms.unwrap_or(0),
        agent_pk: String::new(), // filled by orchestrator from assignment
        quality_score: Some(body.quality_score),
    };
    match state.orchestrator.complete_task(&task_id, result) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "task_id": task_id,
                "status": "completed",
            })),
        )
            .into_response(),
        Err(err) => orchestration_error_to_response(err).into_response(),
    }
}

/// POST /v1/orchestration/tasks/:task_id/fail -- Fail a task (may retry)
async fn fail_task(
    State(state): State<OrchestrationState>,
    Path(task_id): Path<String>,
    Json(body): Json<FailTaskRequest>,
) -> impl IntoResponse {
    match state.orchestrator.fail_task(&task_id, body.error) {
        Ok(()) => {
            // Check if task was retried or permanently failed
            let task = state.orchestrator.get_task(&task_id);
            let status = task
                .map(|t| format!("{:?}", t.status))
                .unwrap_or_else(|| "unknown".to_string());
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "task_id": task_id,
                    "status": status,
                })),
            )
                .into_response()
        }
        Err(err) => orchestration_error_to_response(err).into_response(),
    }
}

/// POST /v1/orchestration/tasks/:task_id/cancel -- Cancel a task and subtasks
async fn cancel_task(
    State(state): State<OrchestrationState>,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    match state.orchestrator.cancel_task(&task_id) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "task_id": task_id,
                "status": "cancelled",
            })),
        )
            .into_response(),
        Err(err) => orchestration_error_to_response(err).into_response(),
    }
}

/// POST /v1/orchestration/fanout -- Submit a fanout (parent + subtasks)
async fn submit_fanout(
    State(state): State<OrchestrationState>,
    Json(body): Json<FanoutRequest>,
) -> impl IntoResponse {
    let mut parent = Task::new(body.parent_task_type, body.parent_payload);
    if let Some(p) = body.parent_priority {
        parent = parent.with_priority(p);
    }
    let subtasks: Vec<Task> = body
        .subtasks
        .into_iter()
        .map(|s| {
            let mut t = Task::new(s.task_type, s.payload).with_parent(parent.id.clone());
            if let Some(p) = s.priority {
                t = t.with_priority(p);
            }
            t
        })
        .collect();
    match state.orchestrator.submit_fanout(parent, subtasks) {
        Ok(parent_id) => (StatusCode::CREATED, Json(FanoutResponse { parent_task_id: parent_id })).into_response(),
        Err(err) => orchestration_error_to_response(err).into_response(),
    }
}

/// GET /v1/orchestration/stats -- Get orchestrator statistics
async fn get_stats(State(state): State<OrchestrationState>) -> Json<serde_json::Value> {
    let stats = state.orchestrator.get_stats();
    Json(serde_json::to_value(stats).unwrap_or_default())
}

/// POST /v1/orchestration/agents/register -- Register an agent
async fn register_agent(
    State(state): State<OrchestrationState>,
    Json(body): Json<RegisterAgentRequest>,
) -> impl IntoResponse {
    let agent = RegisteredAgent::new(
        body.pk,
        body.capabilities,
        body.max_concurrent_tasks.unwrap_or(4),
        body.region.unwrap_or_else(|| "unknown".to_string()),
        1.0, // default reputation
    );
    state.orchestrator.register_agent(agent);
    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "status": "registered",
        })),
    )
        .into_response()
}

/// DELETE /v1/orchestration/agents/:agent_pk -- Unregister an agent
async fn unregister_agent(
    State(state): State<OrchestrationState>,
    Path(agent_pk): Path<String>,
) -> impl IntoResponse {
    match state.orchestrator.unregister_agent(&agent_pk) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "agent_pk": agent_pk,
                "status": "unregistered",
            })),
        )
            .into_response(),
        Err(err) => orchestration_error_to_response(err).into_response(),
    }
}

/// GET /v1/orchestration/agents -- List registered agents
async fn list_agents(State(state): State<OrchestrationState>) -> Json<Vec<serde_json::Value>> {
    let agents = state
        .orchestrator
        .get_agents()
        .into_iter()
        .map(|a| serde_json::to_value(a).unwrap_or_default())
        .collect();
    Json(agents)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt; // for oneshot()

    /// Helper to create an orchestrator with a test agent registered.
    fn test_orchestrator() -> Arc<Orchestrator> {
        let orch = Arc::new(Orchestrator::new(OrchestrationConfig::default()));
        let agent = RegisteredAgent::new(
            "pk-test".to_string(),
            vec![AgentCapability {
                role: AgentRole::Worker,
                models: vec!["llama-3.1-8b".to_string()],
                max_concurrent_tasks: 5,
                region: "us-east".to_string(),
                gpu_spec: None,
                reputation_score: 0.9,
            }],
            5,
            "us-east".to_string(),
            0.9,
        );
        orch.register_agent(agent);
        orch
    }

    /// Helper to build the router for testing.
    fn test_router() -> Router {
        build_orchestration_router(test_orchestrator())
    }

    #[tokio::test]
    async fn test_submit_task() {
        let app = test_router();
        let req = Request::builder()
            .method("POST")
            .uri("/v1/orchestration/tasks")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::json!({
                "task_type": "ChatCompletion",
                "payload": {"prompt": "hello"}
            }).to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_get_task_not_found() {
        let app = test_router();
        let req = Request::builder()
            .method("GET")
            .uri("/v1/orchestration/tasks/nonexistent")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_submit_and_get_task() {
        let orch = Arc::new(Orchestrator::new(OrchestrationConfig::default()));
        let app = build_orchestration_router(orch.clone());

        // Submit
        let req = Request::builder()
            .method("POST")
            .uri("/v1/orchestration/tasks")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::json!({
                "task_type": "ChatCompletion",
                "payload": {"prompt": "test"}
            }).to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        // Get the task id from stats or list
        let tasks = orch.list_tasks(TaskFilter::default());
        assert_eq!(tasks.len(), 1);
        let task_id = tasks[0].id.clone();

        // Get by id
        let app2 = build_orchestration_router(orch);
        let req2 = Request::builder()
            .method("GET")
            .uri(format!("/v1/orchestration/tasks/{}", task_id))
            .body(Body::empty())
            .unwrap();
        let resp2 = app2.oneshot(req2).await.unwrap();
        assert_eq!(resp2.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_assign_task() {
        let orch = Arc::new(Orchestrator::new(OrchestrationConfig::default()));
        let agent = RegisteredAgent::new(
            "pk1".to_string(),
            vec![AgentCapability {
                role: AgentRole::Worker,
                models: vec!["m1".to_string()],
                max_concurrent_tasks: 5,
                region: "us-east".to_string(),
                gpu_spec: None,
                reputation_score: 0.9,
            }],
            5,
            "us-east".to_string(),
            0.9,
        );
        orch.register_agent(agent);
        let id = orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({}))).unwrap();

        let app = build_orchestration_router(orch);
        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/orchestration/tasks/{}/assign", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_complete_task() {
        let orch = Arc::new(Orchestrator::new(OrchestrationConfig::default()));
        let agent = RegisteredAgent::new(
            "pk1".to_string(),
            vec![AgentCapability {
                role: AgentRole::Worker,
                models: vec!["m1".to_string()],
                max_concurrent_tasks: 5,
                region: "us-east".to_string(),
                gpu_spec: None,
                reputation_score: 0.9,
            }],
            5,
            "us-east".to_string(),
            0.9,
        );
        orch.register_agent(agent);
        let id = orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({}))).unwrap();
        orch.assign_task(&id).unwrap();

        let app = build_orchestration_router(orch);
        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/orchestration/tasks/{}/complete", id))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::json!({
                "result": {"output": "42"},
                "quality_score": 0.95,
                "tokens_used": 100,
                "latency_ms": 500
            }).to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_fail_task() {
        let orch = Arc::new(Orchestrator::new(OrchestrationConfig::default()));
        let agent = RegisteredAgent::new(
            "pk1".to_string(),
            vec![AgentCapability {
                role: AgentRole::Worker,
                models: vec!["m1".to_string()],
                max_concurrent_tasks: 5,
                region: "us-east".to_string(),
                gpu_spec: None,
                reputation_score: 0.9,
            }],
            5,
            "us-east".to_string(),
            0.9,
        );
        orch.register_agent(agent);
        let id = orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({})).with_max_retries(3)).unwrap();
        orch.assign_task(&id).unwrap();

        let app = build_orchestration_router(orch);
        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/orchestration/tasks/{}/fail", id))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::json!({"error": "timeout"}).to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_cancel_task() {
        let orch = Arc::new(Orchestrator::new(OrchestrationConfig::default()));
        let id = orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({}))).unwrap();

        let app = build_orchestration_router(orch);
        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/orchestration/tasks/{}/cancel", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_fanout() {
        let app = test_router();
        let req = Request::builder()
            .method("POST")
            .uri("/v1/orchestration/fanout")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::json!({
                "parent_task_type": "CodeGeneration",
                "parent_payload": {"prompt": "build a server"},
                "subtasks": [
                    {"task_type": "ChatCompletion", "payload": {"prompt": "step 1"}},
                    {"task_type": "ChatCompletion", "payload": {"prompt": "step 2"}}
                ]
            }).to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_stats() {
        let app = test_router();
        let req = Request::builder()
            .method("GET")
            .uri("/v1/orchestration/stats")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_register_and_list_agents() {
        let app = test_router();

        // List (should have test agent)
        let req = Request::builder()
            .method("GET")
            .uri("/v1/orchestration/agents")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_unregister_agent() {
        let orch = Arc::new(Orchestrator::new(OrchestrationConfig::default()));
        let agent = RegisteredAgent::new(
            "pk-del".to_string(),
            vec![AgentCapability {
                role: AgentRole::Worker,
                models: vec![],
                max_concurrent_tasks: 1,
                region: "us-east".to_string(),
                gpu_spec: None,
                reputation_score: 0.5,
            }],
            1,
            "us-east".to_string(),
            0.5,
        );
        orch.register_agent(agent);
        assert_eq!(orch.get_agents().len(), 1);

        let app = build_orchestration_router(orch);
        let req = Request::builder()
            .method("DELETE")
            .uri("/v1/orchestration/agents/pk-del")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_list_tasks_empty() {
        let orch = Arc::new(Orchestrator::new(OrchestrationConfig::default()));
        let app = build_orchestration_router(orch);
        let req = Request::builder()
            .method("GET")
            .uri("/v1/orchestration/tasks")
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_assign_no_agents() {
        let orch = Arc::new(Orchestrator::new(OrchestrationConfig::default()));
        let id = orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({}))).unwrap();

        let app = build_orchestration_router(orch);
        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/orchestration/tasks/{}/assign", id))
            .body(Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn test_complete_unassigned_task() {
        let orch = Arc::new(Orchestrator::new(OrchestrationConfig::default()));
        let id = orch.submit_task(Task::new(TaskType::ChatCompletion, serde_json::json!({}))).unwrap();

        let app = build_orchestration_router(orch);
        let req = Request::builder()
            .method("POST")
            .uri(format!("/v1/orchestration/tasks/{}/complete", id))
            .header("content-type", "application/json")
            .body(Body::from(serde_json::json!({"result": {}}).to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        // Pending task cannot be completed -- invalid transition
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }
}
