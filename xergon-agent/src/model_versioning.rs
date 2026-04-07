//! Model versioning: semver, tags, multi-version management.
//!
//! Tracks per-model version information for deployed models.  Each model can
//! have multiple versions (identified by semver), tags (e.g. "latest",
//! "stable", "beta"), and one active version that is currently being served.

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info, warn};

/// A single version of a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelVersion {
    /// Logical model name (e.g. "llama-3.1-8b").
    pub model_name: String,
    /// Semver string (e.g. "1.2.0").
    pub version: String,
    /// Tag assigned to this version (e.g. "latest", "stable", "beta").
    /// A version can carry multiple tags but we store the primary one here;
    /// additional tags live in `extra_tags`.
    pub tag: String,
    /// Additional tags for this version.
    #[serde(default)]
    pub extra_tags: Vec<String>,
    /// SHA-256 digest of the model file(s).
    pub digest: String,
    /// Filesystem path to the model files.
    pub file_path: String,
    /// Total size on disk in bytes.
    pub size_bytes: u64,
    /// When this version was pulled / registered.
    pub pulled_at: DateTime<Utc>,
    /// Where the model came from.
    pub source: String, // "ollama", "huggingface", "local"
    /// Arbitrary extra metadata.
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Registry of model versions.
///
/// Keyed by model name -> ordered list of versions (newest last).
pub struct ModelVersionRegistry {
    /// model_name -> Vec<ModelVersion>
    versions: DashMap<String, Vec<ModelVersion>>,
    /// model_name -> active version string
    active: DashMap<String, String>,
}

impl Default for ModelVersionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelVersionRegistry {
    pub fn new() -> Self {
        Self {
            versions: DashMap::new(),
            active: DashMap::new(),
        }
    }

    // ------------------------------------------------------------------
    // Write operations
    // ------------------------------------------------------------------

    /// Register a new version for a model.
    ///
    /// If a version with the same `(model_name, version)` already exists it
    /// is replaced (upsert).
    pub fn register_version(&self, version: ModelVersion) {
        let model_name = version.model_name.clone();
        let ver_str = version.version.clone();

        // Upsert into the versions list
        let mut entry = self.versions.entry(model_name.clone()).or_default();
        if let Some(existing) = entry.iter_mut().find(|v| v.version == ver_str) {
            *existing = version.clone();
        } else {
            entry.push(version.clone());
            // Keep sorted newest-first by pulled_at
            entry.sort_by(|a, b| b.pulled_at.cmp(&a.pulled_at));
        }
        drop(entry);

        // Auto-set as active if this is the first version or carries "latest"
        if !self.active.contains_key(&model_name) {
            self.active.insert(model_name.clone(), ver_str.clone());
        }
        if version.tag == "latest" || version.extra_tags.contains(&"latest".to_string()) {
            self.active.insert(model_name.clone(), ver_str.clone());
        }

        info!(
            model = %model_name,
            version = %ver_str,
            tag = %version.tag,
            digest = %version.digest,
            "Registered model version"
        );
    }

    /// Set a tag on a specific version.  Common tags: "latest", "stable",
    /// "beta".  Setting tag "latest" also makes the version active.
    pub fn set_tag(&self, model: &str, version: &str, tag: &str) -> Result<(), String> {
        let mut entry = self
            .versions
            .get_mut(model)
            .ok_or_else(|| format!("model '{}' not found", model))?;

        // Remove this tag from all OTHER versions of this model first
        for v in entry.iter_mut() {
            if v.version != version {
                v.extra_tags.retain(|t| t != tag);
                if v.tag == tag {
                    v.tag = String::new();
                }
            }
        }

        // Now apply the tag to the target version
        let ver_str = version.to_string();
        let found = entry
            .iter_mut()
            .find(|v| v.version == ver_str)
            .ok_or_else(|| format!("version '{}' not found for model '{}'", version, model))?;

        found.tag = tag.to_string();
        if !found.extra_tags.contains(&tag.to_string()) {
            found.extra_tags.push(tag.to_string());
        }

        drop(entry);

        // "latest" tag implies active
        if tag == "latest" {
            self.active.insert(model.to_string(), version.to_string());
        }

        info!(model = %model, version = %version, tag = %tag, "Set model version tag");
        Ok(())
    }

    /// Set the active (currently serving) version for a model.
    pub fn set_active_version(&self, model: &str, version: &str) -> Result<(), String> {
        // Verify the version exists
        let entry = self
            .versions
            .get(model)
            .ok_or_else(|| format!("model '{}' not found", model))?;

        if !entry.iter().any(|v| v.version == version) {
            drop(entry);
            return Err(format!("version '{}' not found for model '{}'", version, model));
        }
        drop(entry);

        self.active.insert(model.to_string(), version.to_string());
        info!(model = %model, version = %version, "Set active model version");
        Ok(())
    }

    /// Remove a specific version of a model.
    ///
    /// Fails if the version is currently active.
    pub fn remove_version(&self, model: &str, version: &str) -> Result<(), String> {
        // Check active
        let active_version = self.get_active_version(model).map(|v| v.version);
        if active_version.as_deref() == Some(version) {
            return Err(format!(
                "cannot remove active version '{}' for model '{}' — switch active version first",
                version, model
            ));
        }

        let mut entry = self
            .versions
            .get_mut(model)
            .ok_or_else(|| format!("model '{}' not found", model))?;

        let before = entry.len();
        entry.retain(|v| v.version != version);
        if entry.len() == before {
            drop(entry);
            return Err(format!("version '{}' not found for model '{}'", version, model));
        }

        if entry.is_empty() {
            drop(entry);
            self.versions.remove(model);
            self.active.remove(model);
        }

        info!(model = %model, version = %version, "Removed model version");
        Ok(())
    }

    /// Prune old versions, keeping only the `keep` most recent ones.
    /// Never removes the active version.
    pub fn prune_old_versions(&self, model: &str, keep: usize) -> Result<usize, String> {
        let active_ver = self
            .get_active_version(model)
            .map(|v| v.version.clone());

        let mut entry = self
            .versions
            .get_mut(model)
            .ok_or_else(|| format!("model '{}' not found", model))?;

        if entry.len() <= keep {
            return Ok(0);
        }

        let before = entry.len();
        // Sort newest-first, keep first `keep` + any active version not in that set
        entry.sort_by(|a, b| b.pulled_at.cmp(&a.pulled_at));

        let keep_set: std::collections::HashSet<String> = entry
            .iter()
            .take(keep)
            .map(|v| v.version.clone())
            .collect();

        entry.retain(|v| {
            keep_set.contains(&v.version)
                || active_ver.as_ref().map(|av| av == &v.version).unwrap_or(false)
        });

        let removed = before.saturating_sub(entry.len());
        if entry.is_empty() {
            drop(entry);
            self.versions.remove(model);
            self.active.remove(model);
        }

        if removed > 0 {
            info!(model = %model, removed = removed, remaining = before - removed, "Pruned old model versions");
        }

        Ok(removed)
    }

    // ------------------------------------------------------------------
    // Read operations
    // ------------------------------------------------------------------

    /// Get all versions for a model (newest first).
    pub fn list_versions(&self, model: &str) -> Vec<ModelVersion> {
        self.versions
            .get(model)
            .map(|entry| entry.clone())
            .unwrap_or_default()
    }

    /// Get a specific version of a model.
    pub fn get_version(&self, model: &str, version: &str) -> Option<ModelVersion> {
        self.versions
            .get(model)
            .and_then(|entry| entry.iter().find(|v| v.version == version).cloned())
    }

    /// Get the latest (most recently pulled) version of a model.
    pub fn get_latest(&self, model: &str) -> Option<ModelVersion> {
        self.versions.get(model).and_then(|entry| entry.first().cloned())
    }

    /// Get the currently active (serving) version of a model.
    pub fn get_active_version(&self, model: &str) -> Option<ModelVersion> {
        let active_ver = self.active.get(model)?;
        let version_str = active_ver.value().clone();
        drop(active_ver);
        self.get_version(model, &version_str)
    }

    /// List all known model names.
    pub fn list_models(&self) -> Vec<String> {
        self.versions.iter().map(|r| r.key().clone()).collect()
    }

    /// Summary of all models with their versions.
    pub fn list_all(&self) -> Vec<ModelVersionsSummary> {
        self.versions
            .iter()
            .map(|entry| {
                let model_name = entry.key().clone();
                let versions = entry.value().clone();
                let active_version = self
                    .active
                    .get(&model_name)
                    .map(|v| v.value().clone())
                    .unwrap_or_default();
                ModelVersionsSummary {
                    model_name,
                    versions,
                    active_version,
                }
            })
            .collect()
    }
}

/// Summary used by the API to list all models + their versions.
#[derive(Debug, Clone, Serialize)]
pub struct ModelVersionsSummary {
    pub model_name: String,
    pub versions: Vec<ModelVersion>,
    pub active_version: String,
}

// ------------------------------------------------------------------
// Tests
// ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_version(model: &str, version: &str, tag: &str) -> ModelVersion {
        ModelVersion {
            model_name: model.into(),
            version: version.into(),
            tag: tag.into(),
            extra_tags: if tag.is_empty() { vec![] } else { vec![tag.into()] },
            digest: format!("sha256:fake{}", version),
            file_path: format!("/tmp/models/{}", version),
            size_bytes: 1024,
            pulled_at: Utc::now(),
            source: "local".into(),
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_register_and_list() {
        let reg = ModelVersionRegistry::new();
        reg.register_version(make_version("llama-3.1-8b", "1.0.0", "stable"));
        reg.register_version(make_version("llama-3.1-8b", "1.1.0", "latest"));

        let versions = reg.list_versions("llama-3.1-8b");
        assert_eq!(versions.len(), 2);
        // Newest first
        assert_eq!(versions[0].version, "1.1.0");
    }

    #[test]
    fn test_get_latest() {
        let reg = ModelVersionRegistry::new();
        reg.register_version(make_version("mistral-7b", "0.1.0", ""));
        reg.register_version(make_version("mistral-7b", "0.2.0", "latest"));

        let latest = reg.get_latest("mistral-7b").unwrap();
        assert_eq!(latest.version, "0.2.0");
    }

    #[test]
    fn test_active_version() {
        let reg = ModelVersionRegistry::new();
        reg.register_version(make_version("phi-3", "1.0.0", "latest"));
        reg.register_version(make_version("phi-3", "1.1.0", ""));

        // "latest" tag auto-sets active
        let active = reg.get_active_version("phi-3").unwrap();
        assert_eq!(active.version, "1.0.0");

        // Explicitly set active
        reg.set_active_version("phi-3", "1.1.0").unwrap();
        let active = reg.get_active_version("phi-3").unwrap();
        assert_eq!(active.version, "1.1.0");
    }

    #[test]
    fn test_set_tag() {
        let reg = ModelVersionRegistry::new();
        reg.register_version(make_version("gemma-2", "1.0.0", "latest"));
        reg.register_version(make_version("gemma-2", "1.1.0", ""));

        reg.set_tag("gemma-2", "1.1.0", "stable").unwrap();

        let v = reg.get_version("gemma-2", "1.1.0").unwrap();
        assert_eq!(v.tag, "stable");

        // "latest" tag removed from old version
        let old = reg.get_version("gemma-2", "1.0.0").unwrap();
        assert_ne!(old.tag, "latest");
    }

    #[test]
    fn test_remove_version_rejects_active() {
        let reg = ModelVersionRegistry::new();
        reg.register_version(make_version("test-model", "1.0.0", "latest"));

        let err = reg.remove_version("test-model", "1.0.0");
        assert!(err.is_err());
    }

    #[test]
    fn test_prune_old_versions() {
        let reg = ModelVersionRegistry::new();
        reg.register_version(make_version("deepseek", "1.0.0", ""));
        reg.register_version(make_version("deepseek", "2.0.0", ""));
        reg.register_version(make_version("deepseek", "3.0.0", "latest"));

        let removed = reg.prune_old_versions("deepseek", 1).unwrap();
        // Active version (3.0.0) + 1 kept = 2 remain, so 1 removed
        assert_eq!(removed, 1);
        assert_eq!(reg.list_versions("deepseek").len(), 2);
    }
}
