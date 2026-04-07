//! Prompt Versioning — Prompt template versioning and management
//!
//! Provides version control for prompt templates:
//! - PromptTemplate: named templates with version history
//! - PromptVersion: individual version snapshots with content, changelog, variables
//! - PromptDiff: diff between two versions (additions, removals, unchanged)
//! - Variable extraction: detects {{variable}} placeholders in prompt content
//!
//! Features:
//! - Create/update templates with version tracking
//! - Activate and rollback to specific versions
//! - Search templates by name, category, tags
//! - Diff any two versions of a template
//!
//! REST endpoints:
//! - POST /v1/prompts                        — Create a template
//! - GET  /v1/prompts                        — List/search templates
//! - GET  /v1/prompts/{id}                   — Get a template
//! - POST /v1/prompts/{id}/versions           — Create a new version
//! - GET  /v1/prompts/{id}/versions           — List versions
//! - GET  /v1/prompts/{id}/versions/{version} — Get specific version
//! - POST /v1/prompts/{id}/activate/{version} — Activate a version
//! - GET  /v1/prompts/{id}/diff/{from}/{to}   — Diff two versions

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::LazyLock;

// ---------------------------------------------------------------------------
// Variable extraction
// ---------------------------------------------------------------------------

/// Regex to match {{variable}} placeholders in prompt content.
static VARIABLE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{(\w+)\}\}").unwrap());

/// Extract variable names from prompt content ({{var}} patterns).
pub fn extract_variables(content: &str) -> Vec<String> {
    let mut vars: Vec<String> = VARIABLE_RE
        .captures_iter(content)
        .map(|cap| cap[1].to_string())
        .collect();
    vars.sort();
    vars.dedup();
    vars
}

// ---------------------------------------------------------------------------
// PromptVersion
// ---------------------------------------------------------------------------

/// A single version of a prompt template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptVersion {
    pub version_id: String,
    pub prompt_id: String,
    pub content: String,
    pub version_number: u32,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub changelog: String,
    pub variables: Vec<String>,
    pub is_active: bool,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl PromptVersion {
    /// Create a new prompt version.
    pub fn new(
        prompt_id: &str,
        content: &str,
        version_number: u32,
        created_by: &str,
        changelog: &str,
    ) -> Self {
        PromptVersion {
            version_id: uuid::Uuid::new_v4().to_string(),
            prompt_id: prompt_id.to_string(),
            content: content.to_string(),
            version_number,
            created_by: created_by.to_string(),
            created_at: Utc::now(),
            changelog: changelog.to_string(),
            variables: extract_variables(content),
            is_active: false,
            metadata: HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// PromptTemplate
// ---------------------------------------------------------------------------

/// A prompt template with version history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTemplate {
    pub prompt_id: String,
    pub name: String,
    pub description: String,
    pub current_version: u32,
    pub category: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub versions: Vec<u32>,
}

impl PromptTemplate {
    /// Create a new prompt template (no versions yet).
    pub fn new(prompt_id: &str, name: &str, description: &str, category: &str) -> Self {
        let now = Utc::now();
        PromptTemplate {
            prompt_id: prompt_id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            current_version: 0,
            category: category.to_string(),
            tags: Vec::new(),
            created_at: now,
            updated_at: now,
            versions: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// PromptDiff
// ---------------------------------------------------------------------------

/// A diff between two prompt versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptDiff {
    pub version_from: u32,
    pub version_to: u32,
    pub additions: Vec<String>,
    pub removals: Vec<String>,
    pub unchanged_lines: usize,
}

impl PromptDiff {
    /// Compute a line-level diff between two content strings.
    pub fn compute(from_content: &str, to_content: &str, version_from: u32, version_to: u32) -> Self {
        let from_lines: HashSet<&str> = from_content.lines().collect();
        let to_lines: HashSet<&str> = to_content.lines().collect();

        let additions: Vec<String> = to_content
            .lines()
            .filter(|line| !from_lines.contains(*line))
            .map(|s| s.to_string())
            .collect();

        let removals: Vec<String> = from_content
            .lines()
            .filter(|line| !to_lines.contains(*line))
            .map(|s| s.to_string())
            .collect();

        let unchanged_lines = from_content
            .lines()
            .filter(|line| to_lines.contains(*line))
            .count();

        PromptDiff {
            version_from,
            version_to,
            additions,
            removals,
            unchanged_lines,
        }
    }
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the prompt versioning system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptVersioningConfig {
    /// Maximum number of versions per template (0 = unlimited).
    pub max_versions_per_template: usize,
    /// Maximum prompt content length in characters.
    pub max_content_length: usize,
}

impl Default for PromptVersioningConfig {
    fn default() -> Self {
        PromptVersioningConfig {
            max_versions_per_template: 100,
            max_content_length: 100_000,
        }
    }
}

// ---------------------------------------------------------------------------
// PromptVersionManager — main manager
// ---------------------------------------------------------------------------

/// The prompt versioning manager.
///
/// Stores templates and version history in DashMaps for concurrent access.
pub struct PromptVersionManager {
    /// Templates indexed by prompt_id.
    templates: DashMap<String, PromptTemplate>,
    /// Version history indexed by (prompt_id, version_number).
    versions: DashMap<(String, u32), PromptVersion>,
    /// Index: version_id -> (prompt_id, version_number) for quick lookup.
    version_id_index: DashMap<String, (String, u32)>,
    /// Configuration.
    config: Arc<tokio::sync::RwLock<PromptVersioningConfig>>,
}

impl PromptVersionManager {
    /// Create a new prompt version manager with default config.
    pub fn new() -> Self {
        Self::with_config(PromptVersioningConfig::default())
    }

    /// Create a new prompt version manager with the given config.
    pub fn with_config(config: PromptVersioningConfig) -> Self {
        PromptVersionManager {
            templates: DashMap::new(),
            versions: DashMap::new(),
            version_id_index: DashMap::new(),
            config: Arc::new(tokio::sync::RwLock::new(config)),
        }
    }

    /// Create a new prompt template.
    pub fn create_template(
        &self,
        prompt_id: &str,
        name: &str,
        description: &str,
        category: &str,
        tags: Vec<String>,
        initial_content: &str,
        created_by: &str,
        changelog: &str,
    ) -> Result<PromptTemplate, String> {
        if self.templates.contains_key(prompt_id) {
            return Err(format!("Template '{}' already exists", prompt_id));
        }

        let mut template = PromptTemplate::new(prompt_id, name, description, category);
        template.tags = tags;

        let version = PromptVersion::new(prompt_id, initial_content, 1, created_by, changelog);
        let version_id = version.version_id.clone();
        let version_number = version.version_number;

        template.current_version = 1;
        template.versions.push(1);
        template.updated_at = Utc::now();

        self.templates.insert(prompt_id.to_string(), template.clone());
        self.versions
            .insert((prompt_id.to_string(), 1), version);
        self.version_id_index
            .insert(version_id, (prompt_id.to_string(), version_number));

        Ok(template)
    }

    /// Create a new version of an existing template.
    pub fn create_version(
        &self,
        prompt_id: &str,
        content: &str,
        created_by: &str,
        changelog: &str,
    ) -> Result<PromptVersion, String> {
        let mut template = self
            .templates
            .get_mut(prompt_id)
            .ok_or_else(|| format!("Template '{}' not found", prompt_id))?;

        let config = self.config.blocking_read();
        if config.max_versions_per_template > 0
            && template.versions.len() >= config.max_versions_per_template
        {
            return Err(format!(
                "Template '{}' has reached max versions ({})",
                prompt_id, config.max_versions_per_template
            ));
        }
        if content.len() > config.max_content_length {
            return Err(format!(
                "Content exceeds max length ({})",
                config.max_content_length
            ));
        }
        drop(config);

        let new_version_number = template.current_version + 1;
        let mut version =
            PromptVersion::new(prompt_id, content, new_version_number, created_by, changelog);

        // Deactivate previous version
        if let Some(mut old_version) = self
            .versions
            .get_mut(&(prompt_id.to_string(), template.current_version))
        {
            old_version.is_active = false;
        }

        // Activate new version
        version.is_active = true;
        let version_id = version.version_id.clone();

        template.current_version = new_version_number;
        template.versions.push(new_version_number);
        template.updated_at = Utc::now();

        self.versions.insert(
            (prompt_id.to_string(), new_version_number),
            version.clone(),
        );
        self.version_id_index
            .insert(version_id, (prompt_id.to_string(), new_version_number));

        Ok(version)
    }

    /// Get a template by ID.
    pub fn get_template(&self, prompt_id: &str) -> Option<PromptTemplate> {
        self.templates.get(prompt_id).map(|t| t.clone())
    }

    /// Get a specific version of a template.
    pub fn get_version(&self, prompt_id: &str, version_number: u32) -> Option<PromptVersion> {
        self.versions
            .get(&(prompt_id.to_string(), version_number))
            .map(|v| v.clone())
    }

    /// List all versions of a template.
    pub fn list_versions(&self, prompt_id: &str) -> Vec<PromptVersion> {
        let template = match self.templates.get(prompt_id) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut versions: Vec<PromptVersion> = template
            .versions
            .iter()
            .filter_map(|v| {
                self.versions
                    .get(&(prompt_id.to_string(), *v))
                    .map(|e| e.clone())
            })
            .collect();

        versions.sort_by_key(|v| v.version_number);
        versions
    }

    /// Compute a diff between two versions.
    pub fn diff_versions(
        &self,
        prompt_id: &str,
        version_from: u32,
        version_to: u32,
    ) -> Result<PromptDiff, String> {
        let v_from = self
            .get_version(prompt_id, version_from)
            .ok_or_else(|| format!("Version {} not found", version_from))?;
        let v_to = self
            .get_version(prompt_id, version_to)
            .ok_or_else(|| format!("Version {} not found", version_to))?;

        Ok(PromptDiff::compute(
            &v_from.content,
            &v_to.content,
            version_from,
            version_to,
        ))
    }

    /// Activate a specific version of a template.
    pub fn activate_version(
        &self,
        prompt_id: &str,
        version_number: u32,
    ) -> Result<(), String> {
        let mut template = self
            .templates
            .get_mut(prompt_id)
            .ok_or_else(|| format!("Template '{}' not found", prompt_id))?;

        if !template.versions.contains(&version_number) {
            return Err(format!("Version {} not found for template '{}'", version_number, prompt_id));
        }

        // Deactivate current version
        if let Some(mut old_version) = self
            .versions
            .get_mut(&(prompt_id.to_string(), template.current_version))
        {
            old_version.is_active = false;
        }

        // Activate new version
        if let Some(mut new_version) = self
            .versions
            .get_mut(&(prompt_id.to_string(), version_number))
        {
            new_version.is_active = true;
        }

        template.current_version = version_number;
        template.updated_at = Utc::now();

        Ok(())
    }

    /// Rollback to a previous version (creates a new version with old content).
    pub fn rollback(
        &self,
        prompt_id: &str,
        version_number: u32,
        rolled_back_by: &str,
    ) -> Result<PromptVersion, String> {
        let old_version = self
            .get_version(prompt_id, version_number)
            .ok_or_else(|| format!("Version {} not found", version_number))?;

        let changelog = format!(
            "Rollback to version {} by {}",
            version_number, rolled_back_by
        );

        self.create_version(prompt_id, &old_version.content, rolled_back_by, &changelog)
    }

    /// Search templates by name, category, or tags.
    pub fn search_templates(&self, query: &str, category: Option<&str>, tags: Option<&[String]>) -> Vec<PromptTemplate> {
        let query_lower = query.to_lowercase();
        let mut results: Vec<PromptTemplate> = self
            .templates
            .iter()
            .filter(|entry| {
                let t = entry.value();
                let name_match = t.name.to_lowercase().contains(&query_lower)
                    || t.description.to_lowercase().contains(&query_lower);
                let cat_match = match category {
                    Some(cat) => t.category == cat,
                    None => true,
                };
                let tag_match = match tags {
                    Some(tag_list) => tag_list.iter().all(|tag| t.tags.contains(tag)),
                    None => true,
                };
                name_match && cat_match && tag_match
            })
            .map(|e| e.value().clone())
            .collect();

        results.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        results
    }

    /// Delete a template and all its versions.
    pub fn delete_template(&self, prompt_id: &str) -> bool {
        let removed = self.templates.remove(prompt_id);
        if removed.is_none() {
            return false;
        }

        // Remove all versions
        let version_ids: Vec<(String, u32)> = self
            .versions
            .iter()
            .filter(|e| e.key().0 == prompt_id)
            .map(|e| e.key().clone())
            .collect();

        for (pid, vnum) in &version_ids {
            if let Some((_, version)) = self.versions.remove(&(pid.clone(), *vnum)) {
                self.version_id_index.remove(&version.version_id);
            }
        }

        true
    }

    /// Get all template IDs.
    pub fn list_template_ids(&self) -> Vec<String> {
        self.templates.iter().map(|e| e.key().clone()).collect()
    }

    /// Get the active version of a template.
    pub fn get_active_version(&self, prompt_id: &str) -> Option<PromptVersion> {
        let template = self.templates.get(prompt_id)?;
        self.get_version(prompt_id, template.current_version)
    }

    /// Update configuration.
    pub async fn update_config(&self, new_config: PromptVersioningConfig) {
        let mut config = self.config.write().await;
        *config = new_config;
    }

    /// Get current configuration.
    pub async fn get_config(&self) -> PromptVersioningConfig {
        self.config.read().await.clone()
    }

    /// Template count.
    pub fn template_count(&self) -> usize {
        self.templates.len()
    }
}

impl Default for PromptVersionManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// REST request types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateTemplateRequest {
    pub prompt_id: String,
    pub name: String,
    pub description: String,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub content: String,
    pub created_by: Option<String>,
    pub changelog: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateVersionRequest {
    pub content: String,
    pub created_by: Option<String>,
    pub changelog: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SearchTemplatesQuery {
    pub q: Option<String>,
    pub category: Option<String>,
    pub tags: Option<String>,
}

// ---------------------------------------------------------------------------
// REST handlers
// ---------------------------------------------------------------------------

/// POST /v1/prompts
async fn create_template_handler(
    State(manager): State<Arc<PromptVersionManager>>,
    Json(body): Json<CreateTemplateRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let created_by = body.created_by.as_deref().unwrap_or("anonymous");
    let changelog = body.changelog.as_deref().unwrap_or("Initial version");
    let category = body.category.as_deref().unwrap_or("general");
    let tags = body.tags.unwrap_or_default();

    manager
        .create_template(
            &body.prompt_id,
            &body.name,
            &body.description,
            category,
            tags,
            &body.content,
            created_by,
            changelog,
        )
        .map(|template| {
            (
                StatusCode::CREATED,
                Json(serde_json::json!({ "template": template })),
            )
        })
        .map_err(|e| {
            (
                StatusCode::CONFLICT,
                Json(serde_json::json!({ "error": e })),
            )
        })
}

/// GET /v1/prompts
async fn list_templates_handler(
    State(manager): State<Arc<PromptVersionManager>>,
    Query(query): Query<SearchTemplatesQuery>,
) -> Json<serde_json::Value> {
    let search_query = query.q.as_deref().unwrap_or("");
    let category = query.category.as_deref();
    let tags: Option<Vec<String>> = query
        .tags
        .as_deref()
        .map(|t| t.split(',').map(|s| s.trim().to_string()).collect());

    if search_query.is_empty() && category.is_none() && tags.is_none() {
        // List all
        let ids = manager.list_template_ids();
        let templates: Vec<PromptTemplate> = ids
            .iter()
            .filter_map(|id| manager.get_template(id))
            .collect();
        Json(serde_json::json!({
            "templates": templates,
            "count": templates.len(),
        }))
    } else {
        let templates =
            manager.search_templates(search_query, category, tags.as_deref());
        Json(serde_json::json!({
            "templates": templates,
            "count": templates.len(),
        }))
    }
}

/// GET /v1/prompts/{id}
async fn get_template_handler(
    State(manager): State<Arc<PromptVersionManager>>,
    Path(prompt_id): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let template = manager
        .get_template(&prompt_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(serde_json::json!({ "template": template })))
}

/// POST /v1/prompts/{id}/versions
async fn create_version_handler(
    State(manager): State<Arc<PromptVersionManager>>,
    Path(prompt_id): Path<String>,
    Json(body): Json<CreateVersionRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let created_by = body.created_by.as_deref().unwrap_or("anonymous");
    let changelog = body.changelog.as_deref().unwrap_or("Updated");

    manager
        .create_version(&prompt_id, &body.content, created_by, changelog)
        .map(|version| {
            (
                StatusCode::CREATED,
                Json(serde_json::json!({ "version": version })),
            )
        })
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": e })),
            )
        })
}

/// GET /v1/prompts/{id}/versions
async fn list_versions_handler(
    State(manager): State<Arc<PromptVersionManager>>,
    Path(prompt_id): Path<String>,
) -> Json<serde_json::Value> {
    let versions = manager.list_versions(&prompt_id);
    Json(serde_json::json!({
        "prompt_id": prompt_id,
        "versions": versions,
        "count": versions.len(),
    }))
}

/// GET /v1/prompts/{id}/versions/{version}
async fn get_version_handler(
    State(manager): State<Arc<PromptVersionManager>>,
    Path((prompt_id, version)): Path<(String, u32)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let version = manager
        .get_version(&prompt_id, version)
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(serde_json::json!({ "version": version })))
}

/// POST /v1/prompts/{id}/activate/{version}
async fn activate_version_handler(
    State(manager): State<Arc<PromptVersionManager>>,
    Path((prompt_id, version)): Path<(String, u32)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    manager
        .activate_version(&prompt_id, version)
        .map(|_| Json(serde_json::json!({ "activated": true, "version": version })))
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": e })),
            )
        })
}

/// GET /v1/prompts/{id}/diff/{from}/{to}
async fn diff_versions_handler(
    State(manager): State<Arc<PromptVersionManager>>,
    Path((prompt_id, from_ver, to_ver)): Path<(String, u32, u32)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    manager
        .diff_versions(&prompt_id, from_ver, to_ver)
        .map(|diff| Json(serde_json::json!({ "diff": diff })))
        .map_err(|e| {
            (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": e })),
            )
        })
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the prompt versioning router.
pub fn build_prompt_versioning_router(state: crate::api::AppState) -> axum::Router {
    use axum::routing::{get, post};

    axum::Router::new()
        .route("/v1/prompts", post(create_template_handler).get(list_templates_handler))
        .route("/v1/prompts/{id}", get(get_template_handler))
        .route(
            "/v1/prompts/{id}/versions",
            post(create_version_handler).get(list_versions_handler),
        )
        .route("/v1/prompts/{id}/versions/{version}", get(get_version_handler))
        .route(
            "/v1/prompts/{id}/activate/{version}",
            post(activate_version_handler),
        )
        .route(
            "/v1/prompts/{id}/diff/{from}/{to}",
            get(diff_versions_handler),
        )
        .with_state(state.prompt_versioning.clone())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manager() -> Arc<PromptVersionManager> {
        Arc::new(PromptVersionManager::new())
    }

    #[test]
    fn test_extract_variables() {
        let content = "Hello {{name}}, you have {{count}} messages from {{sender}}.";
        let vars = extract_variables(content);
        assert_eq!(vars, vec!["count", "name", "sender"]);
    }

    #[test]
    fn test_extract_variables_empty() {
        let content = "No variables here.";
        let vars = extract_variables(content);
        assert!(vars.is_empty());
    }

    #[test]
    fn test_extract_variables_dedup() {
        let content = "{{x}} and {{x}} and {{y}}";
        let vars = extract_variables(content);
        assert_eq!(vars, vec!["x", "y"]);
    }

    #[tokio::test]
    async fn test_create_template() {
        let mgr = make_manager();
        let template = mgr
            .create_template(
                "t1",
                "Chat Prompt",
                "A chat prompt",
                "chat",
                vec!["gpt".to_string()],
                "Hello {{name}}",
                "user1",
                "Initial",
            )
            .unwrap();
        assert_eq!(template.name, "Chat Prompt");
        assert_eq!(template.current_version, 1);
        assert_eq!(template.versions.len(), 1);
    }

    #[tokio::test]
    async fn test_create_duplicate_template_fails() {
        let mgr = make_manager();
        mgr.create_template("t1", "Name", "Desc", "cat", vec![], "Content", "u", "c")
            .unwrap();
        assert!(mgr
            .create_template("t1", "Name2", "Desc2", "cat", vec![], "Content2", "u", "c")
            .is_err());
    }

    #[tokio::test]
    async fn test_create_version() {
        let mgr = make_manager();
        mgr.create_template("t1", "Name", "Desc", "cat", vec![], "v1", "u", "c")
            .unwrap();
        let version = mgr
            .create_version("t1", "v2 content {{var}}", "user2", "Updated")
            .unwrap();
        assert_eq!(version.version_number, 2);
        assert_eq!(version.variables, vec!["var"]);
        assert!(version.is_active);
    }

    #[tokio::test]
    async fn test_get_version() {
        let mgr = make_manager();
        mgr.create_template("t1", "Name", "Desc", "cat", vec![], "v1", "u", "c")
            .unwrap();
        mgr.create_version("t1", "v2", "u", "c").unwrap();
        let v1 = mgr.get_version("t1", 1).unwrap();
        assert_eq!(v1.version_number, 1);
        assert!(!v1.is_active);
        let v2 = mgr.get_version("t1", 2).unwrap();
        assert!(v2.is_active);
    }

    #[tokio::test]
    async fn test_list_versions() {
        let mgr = make_manager();
        mgr.create_template("t1", "Name", "Desc", "cat", vec![], "v1", "u", "c")
            .unwrap();
        mgr.create_version("t1", "v2", "u", "c").unwrap();
        mgr.create_version("t1", "v3", "u", "c").unwrap();
        let versions = mgr.list_versions("t1");
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0].version_number, 1);
        assert_eq!(versions[2].version_number, 3);
    }

    #[tokio::test]
    async fn test_activate_version() {
        let mgr = make_manager();
        mgr.create_template("t1", "Name", "Desc", "cat", vec![], "v1", "u", "c")
            .unwrap();
        mgr.create_version("t1", "v2", "u", "c").unwrap();
        mgr.create_version("t1", "v3", "u", "c").unwrap();

        mgr.activate_version("t1", 1).unwrap();
        let template = mgr.get_template("t1").unwrap();
        assert_eq!(template.current_version, 1);

        let v1 = mgr.get_version("t1", 1).unwrap();
        assert!(v1.is_active);
        let v3 = mgr.get_version("t1", 3).unwrap();
        assert!(!v3.is_active);
    }

    #[tokio::test]
    async fn test_diff_versions() {
        let mgr = make_manager();
        mgr.create_template("t1", "Name", "Desc", "cat", vec![], "line1\nline2\nline3", "u", "c")
            .unwrap();
        mgr.create_version("t1", "line1\nline4\nline3", "u", "c")
            .unwrap();

        let diff = mgr.diff_versions("t1", 1, 2).unwrap();
        assert_eq!(diff.version_from, 1);
        assert_eq!(diff.version_to, 2);
        assert!(diff.removals.contains(&"line2".to_string()));
        assert!(diff.additions.contains(&"line4".to_string()));
        assert_eq!(diff.unchanged_lines, 2);
    }

    #[tokio::test]
    async fn test_rollback() {
        let mgr = make_manager();
        mgr.create_template("t1", "Name", "Desc", "cat", vec![], "v1", "u", "c")
            .unwrap();
        mgr.create_version("t1", "v2", "u", "c").unwrap();

        let rolled = mgr.rollback("t1", 1, "admin").unwrap();
        assert_eq!(rolled.version_number, 3);
        assert_eq!(rolled.content, "v1");
        assert!(rolled.changelog.contains("Rollback"));
    }

    #[tokio::test]
    async fn test_search_templates() {
        let mgr = make_manager();
        mgr.create_template("t1", "Chat Prompt", "For chatting", "chat", vec!["gpt".to_string()], "c", "u", "c")
            .unwrap();
        mgr.create_template("t2", "Code Prompt", "For code gen", "code", vec!["gpt".to_string(), "code".to_string()], "c", "u", "c")
            .unwrap();

        let results = mgr.search_templates("chat", None, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].prompt_id, "t1");

        let results = mgr.search_templates("", Some("code"), None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].prompt_id, "t2");
    }

    #[tokio::test]
    async fn test_delete_template() {
        let mgr = make_manager();
        mgr.create_template("t1", "Name", "Desc", "cat", vec![], "v1", "u", "c")
            .unwrap();
        mgr.create_version("t1", "v2", "u", "c").unwrap();
        assert!(mgr.delete_template("t1"));
        assert!(mgr.get_template("t1").is_none());
        assert!(mgr.get_version("t1", 1).is_none());
    }

    #[tokio::test]
    async fn test_get_active_version() {
        let mgr = make_manager();
        mgr.create_template("t1", "Name", "Desc", "cat", vec![], "v1", "u", "c")
            .unwrap();
        mgr.create_version("t1", "v2", "u", "c").unwrap();
        let active = mgr.get_active_version("t1").unwrap();
        assert_eq!(active.version_number, 2);
    }

    #[tokio::test]
    async fn test_config_max_versions() {
        let config = PromptVersioningConfig {
            max_versions_per_template: 2,
            max_content_length: 100_000,
        };
        let mgr = Arc::new(PromptVersionManager::with_config(config));
        mgr.create_template("t1", "Name", "Desc", "cat", vec![], "v1", "u", "c")
            .unwrap();
        mgr.create_version("t1", "v2", "u", "c").unwrap();
        let result = mgr.create_version("t1", "v3", "u", "c");
        assert!(result.is_err());
    }
}
