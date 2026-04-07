//! Model Access Control — Role-based access control for model operations
//!
//! Provides RBAC for model management:
//! - Permission: Read, Write, Deploy, Admin, Inference, Train, Evaluate, Delete
//! - Role: named collections of permissions with hierarchy support
//! - AccessPolicy: policy-based access control with conditions and priority
//! - Condition: field-level condition matching (eq, ne, lt, gt, in, contains)
//! - ModelAccessControl: DashMap-backed RBAC manager
//!
//! Features:
//! - Role hierarchy with inheritance
//! - Policy evaluation with priority ordering
//! - Condition matching (equality, comparison, set membership, substring)
//! - Deny overrides (deny always takes precedence over allow)
//! - User-role assignment management
//!
//! REST endpoints:
//! - POST /v1/rbac/roles                  — Create a role
//! - GET  /v1/rbac/roles                  — List all roles
//! - POST /v1/rbac/roles/{id}/assign      — Assign role to user
//! - DELETE /v1/rbac/roles/{id}/assign    — Revoke role from user
//! - POST /v1/rbac/policies               — Add an access policy
//! - GET  /v1/rbac/policies               — List all policies
//! - POST /v1/rbac/check                  — Check permission for user
//! - GET  /v1/rbac/user/{id}/roles        — Get roles for a user

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Permission
// ---------------------------------------------------------------------------

/// Permissions for model operations.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// Read model metadata and configuration.
    Read,
    /// Modify model configuration.
    Write,
    /// Deploy a model for serving.
    Deploy,
    /// Full administrative access.
    Admin,
    /// Run inference against a model.
    Inference,
    /// Train or fine-tune a model.
    Train,
    /// Evaluate model performance.
    Evaluate,
    /// Delete a model.
    Delete,
}

impl Permission {
    /// Get all permission variants.
    pub fn all() -> Vec<Permission> {
        vec![
            Permission::Read,
            Permission::Write,
            Permission::Deploy,
            Permission::Admin,
            Permission::Inference,
            Permission::Train,
            Permission::Evaluate,
            Permission::Delete,
        ]
    }

    /// Parse a permission from a string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "read" => Some(Permission::Read),
            "write" => Some(Permission::Write),
            "deploy" => Some(Permission::Deploy),
            "admin" => Some(Permission::Admin),
            "inference" => Some(Permission::Inference),
            "train" => Some(Permission::Train),
            "evaluate" => Some(Permission::Evaluate),
            "delete" => Some(Permission::Delete),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Condition
// ---------------------------------------------------------------------------

/// Condition operator for policy matching.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConditionOperator {
    /// Field equals value.
    Eq,
    /// Field does not equal value.
    Ne,
    /// Field less than value (numeric).
    Lt,
    /// Field greater than value (numeric).
    Gt,
    /// Field is in a list of values.
    In,
    /// Field contains value as substring.
    Contains,
}

/// A condition for policy evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    /// The field name to check.
    pub field: String,
    /// The comparison operator.
    pub operator: ConditionOperator,
    /// The value to compare against (JSON).
    pub value: serde_json::Value,
}

impl Condition {
    /// Create a new condition.
    pub fn new(field: &str, operator: ConditionOperator, value: serde_json::Value) -> Self {
        Self {
            field: field.to_string(),
            operator,
            value,
        }
    }

    /// Evaluate this condition against a context map.
    pub fn evaluate(&self, context: &HashMap<String, serde_json::Value>) -> bool {
        let field_value = match context.get(&self.field) {
            Some(v) => v,
            None => return false,
        };

        match self.operator {
            ConditionOperator::Eq => field_value == &self.value,
            ConditionOperator::Ne => field_value != &self.value,
            ConditionOperator::Lt => {
                // Numeric comparison
                let a = field_value.as_f64().unwrap_or(0.0);
                let b = self.value.as_f64().unwrap_or(0.0);
                a < b
            }
            ConditionOperator::Gt => {
                let a = field_value.as_f64().unwrap_or(0.0);
                let b = self.value.as_f64().unwrap_or(0.0);
                a > b
            }
            ConditionOperator::In => {
                if let Some(arr) = self.value.as_array() {
                    arr.contains(field_value)
                } else {
                    false
                }
            }
            ConditionOperator::Contains => {
                let haystack = field_value.as_str().unwrap_or("");
                let needle = self.value.as_str().unwrap_or("");
                haystack.contains(needle)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Effect
// ---------------------------------------------------------------------------

/// Policy effect: allow or deny.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effect {
    Allow,
    Deny,
}

// ---------------------------------------------------------------------------
// Role
// ---------------------------------------------------------------------------

/// A named role with a set of permissions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Role {
    /// Unique role identifier.
    pub role_id: String,
    /// Human-readable role name.
    pub name: String,
    /// Role description.
    pub description: String,
    /// Set of permissions granted by this role.
    pub permissions: HashSet<Permission>,
    /// Parent role ID for inheritance (optional).
    pub parent_role_id: Option<String>,
    /// When the role was created.
    pub created_at: DateTime<Utc>,
}

impl Role {
    /// Create a new role.
    pub fn new(name: &str, description: &str, permissions: HashSet<Permission>) -> Self {
        Self {
            role_id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            description: description.to_string(),
            permissions,
            parent_role_id: None,
            created_at: Utc::now(),
        }
    }

    /// Create a role with a parent for inheritance.
    pub fn with_parent(
        name: &str,
        description: &str,
        permissions: HashSet<Permission>,
        parent_role_id: &str,
    ) -> Self {
        Self {
            role_id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            description: description.to_string(),
            permissions,
            parent_role_id: Some(parent_role_id.to_string()),
            created_at: Utc::now(),
        }
    }

    /// Check if this role has a specific permission.
    pub fn has_permission(&self, perm: &Permission) -> bool {
        self.permissions.contains(perm)
    }

    /// Add a permission to this role.
    pub fn add_permission(&mut self, perm: Permission) {
        self.permissions.insert(perm);
    }

    /// Remove a permission from this role.
    pub fn remove_permission(&mut self, perm: &Permission) -> bool {
        self.permissions.remove(perm)
    }
}

// ---------------------------------------------------------------------------
// AccessPolicy
// ---------------------------------------------------------------------------

/// An access policy that grants or denies access based on conditions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessPolicy {
    /// Unique policy identifier.
    pub policy_id: String,
    /// The model this policy applies to (empty string for all models).
    pub model_id: String,
    /// The role this policy targets (empty string for all roles).
    pub role_id: String,
    /// Conditions that must be met for this policy to apply.
    pub conditions: Vec<Condition>,
    /// The effect of this policy (allow or deny).
    pub effect: Effect,
    /// Priority (lower number = higher priority). Evaluated in priority order.
    pub priority: i32,
    /// Policy description.
    pub description: String,
    /// Whether this policy is enabled.
    pub enabled: bool,
    /// When the policy was created.
    pub created_at: DateTime<Utc>,
}

impl AccessPolicy {
    /// Create a new access policy.
    pub fn new(
        model_id: &str,
        role_id: &str,
        conditions: Vec<Condition>,
        effect: Effect,
        priority: i32,
    ) -> Self {
        Self {
            policy_id: Uuid::new_v4().to_string(),
            model_id: model_id.to_string(),
            role_id: role_id.to_string(),
            conditions,
            effect,
            priority,
            description: String::new(),
            enabled: true,
            created_at: Utc::now(),
        }
    }

    /// Create a policy with description.
    pub fn with_description(
        model_id: &str,
        role_id: &str,
        conditions: Vec<Condition>,
        effect: Effect,
        priority: i32,
        description: &str,
    ) -> Self {
        Self {
            policy_id: Uuid::new_v4().to_string(),
            model_id: model_id.to_string(),
            role_id: role_id.to_string(),
            conditions,
            effect,
            priority,
            description: description.to_string(),
            enabled: true,
            created_at: Utc::now(),
        }
    }

    /// Check if this policy matches the given context.
    pub fn matches(&self, context: &EvaluationContext) -> bool {
        // Check model_id match
        if !self.model_id.is_empty() && self.model_id != context.model_id {
            return false;
        }

        // Check role_id match
        if !self.role_id.is_empty() && !context.role_ids.contains(&self.role_id) {
            return false;
        }

        // Check all conditions
        self.conditions
            .iter()
            .all(|cond| cond.evaluate(&context.attributes))
    }
}

// ---------------------------------------------------------------------------
// EvaluationContext
// ---------------------------------------------------------------------------

/// Context for evaluating access policies.
#[derive(Debug, Clone)]
pub struct EvaluationContext {
    /// The user performing the action.
    pub user_id: String,
    /// The model being accessed.
    pub model_id: String,
    /// The permission being checked.
    pub permission: Permission,
    /// The user's role IDs.
    pub role_ids: Vec<String>,
    /// Additional attributes for condition evaluation.
    pub attributes: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// EvaluationResult
// ---------------------------------------------------------------------------

/// Result of an access evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationResult {
    /// Whether access is allowed.
    pub allowed: bool,
    /// The effect that determined the result.
    pub effect: Effect,
    /// The policy that determined the result (if any).
    pub policy_id: Option<String>,
    /// Human-readable reason.
    pub reason: String,
}

// ---------------------------------------------------------------------------
// ModelAccessControl
// ---------------------------------------------------------------------------

/// Role-based access control manager with DashMap-backed storage.
#[derive(Debug, Clone)]
pub struct ModelAccessControl {
    /// Roles keyed by role_id.
    roles: Arc<DashMap<String, Role>>,
    /// Access policies keyed by policy_id.
    policies: Arc<DashMap<String, AccessPolicy>>,
    /// User-to-role assignments: user_id -> Set<role_id>.
    user_roles: Arc<DashMap<String, HashSet<String>>>,
}

impl ModelAccessControl {
    /// Create a new ModelAccessControl manager.
    pub fn new() -> Self {
        Self {
            roles: Arc::new(DashMap::new()),
            policies: Arc::new(DashMap::new()),
            user_roles: Arc::new(DashMap::new()),
        }
    }

    // --- Role management ---

    /// Create a new role.
    pub fn create_role(&self, role: Role) -> Result<Role, String> {
        let role_id = role.role_id.clone();
        if self.roles.contains_key(&role_id) {
            return Err(format!("Role {} already exists", role_id));
        }
        let mut stored = role;
        self.roles.insert(role_id.clone(), stored.clone());

        // Inherit permissions from parent
        if let Some(ref parent_id) = stored.parent_role_id {
            if let Some(parent) = self.roles.get(parent_id) {
                for perm in parent.permissions.iter() {
                    stored.permissions.insert(perm.clone());
                }
            }
            self.roles.insert(role_id, stored.clone());
        }

        Ok(stored)
    }

    /// Get a role by ID.
    pub fn get_role(&self, role_id: &str) -> Option<Role> {
        self.roles.get(role_id).map(|r| r.clone())
    }

    /// List all roles.
    pub fn list_roles(&self) -> Vec<Role> {
        self.roles.iter().map(|r| r.clone()).collect()
    }

    /// Delete a role.
    pub fn delete_role(&self, role_id: &str) -> Result<(), String> {
        if self.roles.remove(role_id).is_none() {
            return Err(format!("Role {} not found", role_id));
        }
        // Clean up user assignments
        for mut entry in self.user_roles.iter_mut() {
            entry.value_mut().remove(role_id);
        }
        Ok(())
    }

    // --- User-role assignment ---

    /// Assign a role to a user.
    pub fn assign_role(&self, user_id: &str, role_id: &str) -> Result<(), String> {
        if !self.roles.contains_key(role_id) {
            return Err(format!("Role {} not found", role_id));
        }
        self.user_roles
            .entry(user_id.to_string())
            .or_default()
            .value_mut()
            .insert(role_id.to_string());
        Ok(())
    }

    /// Revoke a role from a user.
    pub fn revoke_role(&self, user_id: &str, role_id: &str) -> Result<(), String> {
        let mut entry = self
            .user_roles
            .get_mut(user_id)
            .ok_or_else(|| format!("No roles found for user {}", user_id))?;
        if !entry.remove(role_id) {
            return Err(format!("User {} does not have role {}", user_id, role_id));
        }
        Ok(())
    }

    /// Get all roles assigned to a user.
    pub fn get_user_roles(&self, user_id: &str) -> Vec<String> {
        self.user_roles
            .get(user_id)
            .map(|r| r.iter().cloned().collect())
            .unwrap_or_default()
    }

    // --- Permission checking ---

    /// Check if a user has a specific permission (via role assignments).
    pub fn check_permission(&self, user_id: &str, permission: &Permission) -> bool {
        let role_ids = self.get_user_roles(user_id);
        for role_id in &role_ids {
            if let Some(role) = self.roles.get(role_id) {
                if role.has_permission(permission) {
                    return true;
                }
            }
        }
        false
    }

    // --- Policy management ---

    /// Add an access policy.
    pub fn add_policy(&self, policy: AccessPolicy) -> Result<AccessPolicy, String> {
        let policy_id = policy.policy_id.clone();
        let stored = policy.clone();
        self.policies.insert(policy_id, stored);
        Ok(policy)
    }

    /// Remove an access policy.
    pub fn remove_policy(&self, policy_id: &str) -> Result<AccessPolicy, String> {
        self.policies
            .remove(policy_id)
            .map(|(_, p)| p)
            .ok_or_else(|| format!("Policy {} not found", policy_id))
    }

    /// List all policies.
    pub fn list_policies(&self) -> Vec<AccessPolicy> {
        self.policies.iter().map(|p| p.clone()).collect()
    }

    /// List policies for a specific model.
    pub fn list_policies_for_model(&self, model_id: &str) -> Vec<AccessPolicy> {
        self.policies
            .iter()
            .filter(|p| p.model_id == model_id || p.model_id.is_empty())
            .map(|p| p.clone())
            .collect()
    }

    // --- Evaluation ---

    /// Evaluate access for a given context.
    /// Policies are evaluated in priority order (lower number = higher priority).
    /// Deny overrides: if any matching deny policy is found, access is denied
    /// regardless of allow policies.
    pub fn evaluate(&self, context: &EvaluationContext) -> EvaluationResult {
        // First, collect all matching policies sorted by priority
        let mut matching_policies: Vec<AccessPolicy> = self
            .policies
            .iter()
            .filter(|p| p.value().enabled && p.value().matches(context))
            .map(|p| p.value().clone())
            .collect();

        // Sort by priority (lower number = higher priority)
        matching_policies.sort_by_key(|p| p.priority);

        // Check for deny policies first (deny overrides)
        for policy in &matching_policies {
            if policy.effect == Effect::Deny {
                return EvaluationResult {
                    allowed: false,
                    effect: Effect::Deny,
                    policy_id: Some(policy.policy_id.clone()),
                    reason: format!(
                        "Access denied by policy '{}' (priority {})",
                        policy.policy_id, policy.priority
                    ),
                };
            }
        }

        // Check for allow policies
        for policy in &matching_policies {
            if policy.effect == Effect::Allow {
                // Also verify the user's roles have the required permission
                let has_perm = self.check_permission(&context.user_id, &context.permission);
                if has_perm {
                    return EvaluationResult {
                        allowed: true,
                        effect: Effect::Allow,
                        policy_id: Some(policy.policy_id.clone()),
                        reason: format!(
                            "Access allowed by policy '{}' (priority {})",
                            policy.policy_id, policy.priority
                        ),
                    };
                } else {
                    return EvaluationResult {
                        allowed: false,
                        effect: Effect::Deny,
                        policy_id: Some(policy.policy_id.clone()),
                        reason: "User lacks the required permission in assigned roles".to_string(),
                    };
                }
            }
        }

        // Default: deny if no policies match
        EvaluationResult {
            allowed: false,
            effect: Effect::Deny,
            policy_id: None,
            reason: "No matching policy found — default deny".to_string(),
        }
    }
}

impl Default for ModelAccessControl {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// REST API types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateRoleRequest {
    pub name: String,
    pub description: String,
    pub permissions: Vec<String>,
    #[serde(default)]
    pub parent_role_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RoleResponse {
    pub role_id: String,
    pub name: String,
    pub description: String,
    pub permissions: HashSet<String>,
    pub parent_role_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct AssignRoleRequest {
    pub user_id: String,
}

#[derive(Debug, Serialize)]
pub struct AssignRoleResponse {
    pub user_id: String,
    pub role_id: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct CreatePolicyRequest {
    pub model_id: String,
    pub role_id: String,
    #[serde(default)]
    pub conditions: Vec<Condition>,
    pub effect: Effect,
    pub priority: i32,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct CheckPermissionRequest {
    pub user_id: String,
    pub model_id: String,
    pub permission: String,
    #[serde(default)]
    pub attributes: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// REST handlers
// ---------------------------------------------------------------------------

/// POST /v1/rbac/roles — Create a new role.
pub async fn create_role_handler(
    State(state): State<crate::api::AppState>,
    Json(req): Json<CreateRoleRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let permissions: HashSet<Permission> = req
        .permissions
        .iter()
        .filter_map(|p| Permission::from_str(p))
        .collect();

    let role = if let Some(parent_id) = req.parent_role_id {
        Role::with_parent(&req.name, &req.description, permissions, &parent_id)
    } else {
        Role::new(&req.name, &req.description, permissions)
    };

    match state.model_access_control.create_role(role) {
        Ok(created) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "role_id": created.role_id,
                "name": created.name,
                "description": created.description,
                "permissions": created.permissions.iter().map(|p| format!("{:?}", p).to_lowercase()).collect::<Vec<_>>(),
                "parent_role_id": created.parent_role_id,
                "created_at": created.created_at.to_rfc3339(),
            })),
        ),
        Err(e) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

/// GET /v1/rbac/roles — List all roles.
pub async fn list_roles_handler(
    State(state): State<crate::api::AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let roles = state.model_access_control.list_roles();
    let response: Vec<serde_json::Value> = roles
        .iter()
        .map(|r| {
            serde_json::json!({
                "role_id": r.role_id,
                "name": r.name,
                "description": r.description,
                "permissions": r.permissions.iter().map(|p| format!("{:?}", p).to_lowercase()).collect::<Vec<_>>(),
                "parent_role_id": r.parent_role_id,
                "created_at": r.created_at.to_rfc3339(),
            })
        })
        .collect();

    (StatusCode::OK, Json(serde_json::json!({ "roles": response })))
}

/// POST /v1/rbac/roles/{id}/assign — Assign role to user.
pub async fn assign_role_handler(
    State(state): State<crate::api::AppState>,
    Path(role_id): Path<String>,
    Json(req): Json<AssignRoleRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state
        .model_access_control
        .assign_role(&req.user_id, &role_id)
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "user_id": req.user_id,
                "role_id": role_id,
                "message": "Role assigned successfully",
            })),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

/// DELETE /v1/rbac/roles/{id}/assign — Revoke role from user.
pub async fn revoke_role_handler(
    State(state): State<crate::api::AppState>,
    Path(role_id): Path<String>,
    Json(req): Json<AssignRoleRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    match state
        .model_access_control
        .revoke_role(&req.user_id, &role_id)
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "user_id": req.user_id,
                "role_id": role_id,
                "message": "Role revoked successfully",
            })),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

/// POST /v1/rbac/policies — Add an access policy.
pub async fn create_policy_handler(
    State(state): State<crate::api::AppState>,
    Json(req): Json<CreatePolicyRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let policy = AccessPolicy::with_description(
        &req.model_id,
        &req.role_id,
        req.conditions,
        req.effect,
        req.priority,
        &req.description,
    );

    match state.model_access_control.add_policy(policy) {
        Ok(created) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "policy_id": created.policy_id,
                "model_id": created.model_id,
                "role_id": created.role_id,
                "effect": format!("{:?}", created.effect).to_lowercase(),
                "priority": created.priority,
                "description": created.description,
                "created_at": created.created_at.to_rfc3339(),
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": e})),
        ),
    }
}

/// GET /v1/rbac/policies — List all policies.
pub async fn list_policies_handler(
    State(state): State<crate::api::AppState>,
) -> (StatusCode, Json<serde_json::Value>) {
    let policies = state.model_access_control.list_policies();
    let response: Vec<serde_json::Value> = policies
        .iter()
        .map(|p| {
            serde_json::json!({
                "policy_id": p.policy_id,
                "model_id": p.model_id,
                "role_id": p.role_id,
                "effect": format!("{:?}", p.effect).to_lowercase(),
                "priority": p.priority,
                "description": p.description,
                "enabled": p.enabled,
                "created_at": p.created_at.to_rfc3339(),
            })
        })
        .collect();

    (
        StatusCode::OK,
        Json(serde_json::json!({ "policies": response })),
    )
}

/// POST /v1/rbac/check — Check permission for a user.
pub async fn check_permission_handler(
    State(state): State<crate::api::AppState>,
    Json(req): Json<CheckPermissionRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let permission = match Permission::from_str(&req.permission) {
        Some(p) => p,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Invalid permission: {}", req.permission)})),
            );
        }
    };

    let role_ids = state.model_access_control.get_user_roles(&req.user_id);
    let context = EvaluationContext {
        user_id: req.user_id.clone(),
        model_id: req.model_id.clone(),
        permission,
        role_ids,
        attributes: req.attributes,
    };

    let result = state.model_access_control.evaluate(&context);
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "allowed": result.allowed,
            "effect": format!("{:?}", result.effect).to_lowercase(),
            "policy_id": result.policy_id,
            "reason": result.reason,
        })),
    )
}

/// GET /v1/rbac/user/{id}/roles — Get roles for a user.
pub async fn get_user_roles_handler(
    State(state): State<crate::api::AppState>,
    Path(user_id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let role_ids = state.model_access_control.get_user_roles(&user_id);
    let roles: Vec<serde_json::Value> = role_ids
        .iter()
        .filter_map(|rid| {
            state
                .model_access_control
                .get_role(rid)
                .map(|r| {
                    serde_json::json!({
                        "role_id": r.role_id,
                        "name": r.name,
                        "description": r.description,
                    })
                })
        })
        .collect();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "user_id": user_id,
            "roles": roles,
        })),
    )
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the model access control router.
pub fn build_rbac_router(state: crate::api::AppState) -> axum::Router {
    use axum::routing::{delete, get, post};

    axum::Router::new()
        .route("/v1/rbac/roles", post(create_role_handler).get(list_roles_handler))
        .route(
            "/v1/rbac/roles/{id}/assign",
            post(assign_role_handler).delete(revoke_role_handler),
        )
        .route(
            "/v1/rbac/policies",
            post(create_policy_handler).get(list_policies_handler),
        )
        .route("/v1/rbac/check", post(check_permission_handler))
        .route("/v1/rbac/user/{id}/roles", get(get_user_roles_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_rbac() -> ModelAccessControl {
        ModelAccessControl::new()
    }

    fn make_permissions(perms: &[&str]) -> HashSet<Permission> {
        perms.iter().filter_map(|p| Permission::from_str(p)).collect()
    }

    #[test]
    fn test_permission_from_str() {
        assert_eq!(Permission::from_str("read"), Some(Permission::Read));
        assert_eq!(Permission::from_str("write"), Some(Permission::Write));
        assert_eq!(Permission::from_str("deploy"), Some(Permission::Deploy));
        assert_eq!(Permission::from_str("admin"), Some(Permission::Admin));
        assert_eq!(Permission::from_str("inference"), Some(Permission::Inference));
        assert_eq!(Permission::from_str("train"), Some(Permission::Train));
        assert_eq!(Permission::from_str("evaluate"), Some(Permission::Evaluate));
        assert_eq!(Permission::from_str("delete"), Some(Permission::Delete));
        assert_eq!(Permission::from_str("invalid"), None);
    }

    #[test]
    fn test_create_role() {
        let rbac = create_test_rbac();
        let role = Role::new("viewer", "Can view models", make_permissions(&["read"]));
        let created = rbac.create_role(role).unwrap();
        assert_eq!(created.name, "viewer");
        assert!(created.has_permission(&Permission::Read));
        assert!(!created.has_permission(&Permission::Write));
    }

    #[test]
    fn test_create_duplicate_role_fails() {
        let rbac = create_test_rbac();
        let role = Role::new("viewer", "Can view", make_permissions(&["read"]));
        rbac.create_role(role.clone()).unwrap();
        let result = rbac.create_role(role);
        assert!(result.is_err());
    }

    #[test]
    fn test_assign_and_revoke_role() {
        let rbac = create_test_rbac();
        let role = Role::new("editor", "Can edit", make_permissions(&["read", "write"]));
        let created = rbac.create_role(role).unwrap();

        rbac.assign_role("user-1", &created.role_id).unwrap();
        let roles = rbac.get_user_roles("user-1");
        assert_eq!(roles.len(), 1);
        assert_eq!(roles[0], created.role_id);

        rbac.revoke_role("user-1", &created.role_id).unwrap();
        let roles = rbac.get_user_roles("user-1");
        assert!(roles.is_empty());
    }

    #[test]
    fn test_assign_nonexistent_role_fails() {
        let rbac = create_test_rbac();
        let result = rbac.assign_role("user-1", "nonexistent-role");
        assert!(result.is_err());
    }

    #[test]
    fn test_check_permission_via_role() {
        let rbac = create_test_rbac();
        let role = Role::new("inference-user", "Can run inference", make_permissions(&["inference", "read"]));
        let created = rbac.create_role(role).unwrap();
        rbac.assign_role("user-1", &created.role_id).unwrap();

        assert!(rbac.check_permission("user-1", &Permission::Inference));
        assert!(rbac.check_permission("user-1", &Permission::Read));
        assert!(!rbac.check_permission("user-1", &Permission::Admin));
        assert!(!rbac.check_permission("user-2", &Permission::Inference));
    }

    #[test]
    fn test_condition_evaluate_eq() {
        let cond = Condition::new("region", ConditionOperator::Eq, serde_json::json!("us-east"));
        let mut ctx = HashMap::new();
        ctx.insert("region".to_string(), serde_json::json!("us-east"));
        assert!(cond.evaluate(&ctx));

        ctx.insert("region".to_string(), serde_json::json!("eu-west"));
        assert!(!cond.evaluate(&ctx));
    }

    #[test]
    fn test_condition_evaluate_in() {
        let cond = Condition::new(
            "role",
            ConditionOperator::In,
            serde_json::json!(["admin", "super-admin"]),
        );
        let mut ctx = HashMap::new();
        ctx.insert("role".to_string(), serde_json::json!("admin"));
        assert!(cond.evaluate(&ctx));

        ctx.insert("role".to_string(), serde_json::json!("viewer"));
        assert!(!cond.evaluate(&ctx));
    }

    #[test]
    fn test_condition_evaluate_numeric() {
        let lt = Condition::new("usage", ConditionOperator::Lt, serde_json::json!(100));
        let gt = Condition::new("usage", ConditionOperator::Gt, serde_json::json!(50));
        let mut ctx = HashMap::new();
        ctx.insert("usage".to_string(), serde_json::json!(75));

        assert!(lt.evaluate(&ctx));
        assert!(gt.evaluate(&ctx));
    }

    #[test]
    fn test_add_and_remove_policy() {
        let rbac = create_test_rbac();
        let policy = AccessPolicy::new("", "", vec![], Effect::Allow, 1);
        let created = rbac.add_policy(policy).unwrap();
        assert!(rbac.remove_policy(&created.policy_id).is_ok());
        assert!(rbac.remove_policy(&created.policy_id).is_err());
    }

    #[test]
    fn test_evaluate_allow_policy() {
        let rbac = create_test_rbac();
        let role = Role::new("reader", "Can read", make_permissions(&["read"]));
        let created_role = rbac.create_role(role).unwrap();
        rbac.assign_role("user-1", &created_role.role_id).unwrap();

        let policy = AccessPolicy::new("model-1", &created_role.role_id, vec![], Effect::Allow, 1);
        rbac.add_policy(policy).unwrap();

        let context = EvaluationContext {
            user_id: "user-1".to_string(),
            model_id: "model-1".to_string(),
            permission: Permission::Read,
            role_ids: vec![created_role.role_id.clone()],
            attributes: HashMap::new(),
        };

        let result = rbac.evaluate(&context);
        assert!(result.allowed);
    }

    #[test]
    fn test_evaluate_deny_overrides_allow() {
        let rbac = create_test_rbac();
        let role = Role::new("reader", "Can read", make_permissions(&["read"]));
        let created_role = rbac.create_role(role).unwrap();
        rbac.assign_role("user-1", &created_role.role_id).unwrap();

        // Allow policy at priority 10
        let allow = AccessPolicy::new("model-1", &created_role.role_id, vec![], Effect::Allow, 10);
        rbac.add_policy(allow).unwrap();

        // Deny policy at priority 1 (higher priority)
        let deny = AccessPolicy::new("model-1", &created_role.role_id, vec![], Effect::Deny, 1);
        rbac.add_policy(deny).unwrap();

        let context = EvaluationContext {
            user_id: "user-1".to_string(),
            model_id: "model-1".to_string(),
            permission: Permission::Read,
            role_ids: vec![created_role.role_id.clone()],
            attributes: HashMap::new(),
        };

        let result = rbac.evaluate(&context);
        assert!(!result.allowed);
        assert_eq!(result.effect, Effect::Deny);
    }

    #[test]
    fn test_list_policies_for_model() {
        let rbac = create_test_rbac();
        let p1 = AccessPolicy::new("model-a", "", vec![], Effect::Allow, 1);
        let p2 = AccessPolicy::new("model-b", "", vec![], Effect::Allow, 2);
        let p3 = AccessPolicy::new("", "", vec![], Effect::Allow, 3); // global
        rbac.add_policy(p1).unwrap();
        rbac.add_policy(p2).unwrap();
        rbac.add_policy(p3).unwrap();

        let policies = rbac.list_policies_for_model("model-a");
        assert_eq!(policies.len(), 2); // model-a + global
    }

    #[test]
    fn test_delete_role_cleans_assignments() {
        let rbac = create_test_rbac();
        let role = Role::new("temp", "Temporary", make_permissions(&["read"]));
        let created = rbac.create_role(role).unwrap();
        rbac.assign_role("user-1", &created.role_id).unwrap();

        rbac.delete_role(&created.role_id).unwrap();
        let roles = rbac.get_user_roles("user-1");
        assert!(roles.is_empty());
    }
}
