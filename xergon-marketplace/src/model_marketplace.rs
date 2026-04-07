use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// ModelStatus
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum ModelStatus {
    Draft,
    Published,
    Deprecated,
    Archived,
    Removed,
}

impl std::fmt::Display for ModelStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Draft => write!(f, "Draft"),
            Self::Published => write!(f, "Published"),
            Self::Deprecated => write!(f, "Deprecated"),
            Self::Archived => write!(f, "Archived"),
            Self::Removed => write!(f, "Removed"),
        }
    }
}

// ---------------------------------------------------------------------------
// ModelVersion
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModelVersion {
    pub version: String,
    pub description: String,
    pub model_id: String,
    pub created_at: DateTime<Utc>,
    pub download_count: u64,
    pub size_bytes: u64,
    pub checksum: String,
    pub is_latest: bool,
}

impl ModelVersion {
    /// Create a new model version with default values.
    pub fn new(
        model_id: &str,
        version: &str,
        description: &str,
        size_bytes: u64,
        checksum: &str,
    ) -> Self {
        Self {
            version: version.to_string(),
            description: description.to_string(),
            model_id: model_id.to_string(),
            created_at: Utc::now(),
            download_count: 0,
            size_bytes,
            checksum: checksum.to_string(),
            is_latest: true,
        }
    }
}

// ---------------------------------------------------------------------------
// MarketplaceModelSnapshot
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MarketplaceModelSnapshot {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub tags: Vec<String>,
    pub status: ModelStatus,
    pub versions: Vec<ModelVersion>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub rating: f64,
    pub review_count: u64,
    pub total_downloads: u64,
    pub license: String,
}

// ---------------------------------------------------------------------------
// MarketplaceModel
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct MarketplaceModel {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    pub tags: Vec<String>,
    pub status: Arc<RwLock<ModelStatus>>,
    pub versions: DashMap<String, ModelVersion>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Arc<RwLock<DateTime<Utc>>>,
    pub rating: Arc<RwLock<f64>>,
    pub review_count: AtomicU64,
    pub total_downloads: AtomicU64,
    pub license: String,
}

impl MarketplaceModel {
    /// Create a new model with the given parameters and initial version.
    pub fn new(
        id: &str,
        name: &str,
        description: &str,
        author: &str,
        tags: Vec<String>,
        license: &str,
        initial_version: &str,
        version_description: &str,
        size_bytes: u64,
        checksum: &str,
    ) -> Self {
        let version = ModelVersion::new(id, initial_version, version_description, size_bytes, checksum);
        let now = Utc::now();

        let versions = DashMap::new();
        versions.insert(initial_version.to_string(), version);

        Self {
            id: id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            author: author.to_string(),
            tags,
            status: Arc::new(RwLock::new(ModelStatus::Draft)),
            versions,
            created_at: now,
            updated_at: Arc::new(RwLock::new(now)),
            rating: Arc::new(RwLock::new(0.0)),
            review_count: AtomicU64::new(0),
            total_downloads: AtomicU64::new(0),
            license: license.to_string(),
        }
    }

    /// Produce an immutable snapshot of the current model state.
    pub fn snapshot(&self) -> MarketplaceModelSnapshot {
        let mut version_list: Vec<ModelVersion> = self
            .versions
            .iter()
            .map(|r| r.value().clone())
            .collect();
        version_list.sort_by(|a, b| b.version.cmp(&a.version));

        MarketplaceModelSnapshot {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            author: self.author.clone(),
            tags: self.tags.clone(),
            status: self.status.read().unwrap().clone(),
            versions: version_list,
            created_at: self.created_at,
            updated_at: *self.updated_at.read().unwrap(),
            rating: *self.rating.read().unwrap(),
            review_count: self.review_count.load(Ordering::Relaxed),
            total_downloads: self.total_downloads.load(Ordering::Relaxed),
            license: self.license.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// SearchFilters
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchFilters {
    pub tags: Option<Vec<String>>,
    pub author: Option<String>,
    pub status: Option<ModelStatus>,
    pub min_rating: Option<f64>,
    pub sort_by: String,
    pub limit: u32,
    pub offset: u32,
}

impl Default for SearchFilters {
    fn default() -> Self {
        Self {
            tags: None,
            author: None,
            status: None,
            min_rating: None,
            sort_by: "downloads".to_string(),
            limit: 20,
            offset: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// MarketplaceMetrics
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MarketplaceMetrics {
    pub total_models: u64,
    pub total_published: u64,
    pub total_downloads: u64,
    pub total_versions: u64,
}

// ---------------------------------------------------------------------------
// ModelMarketplace
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct ModelMarketplace {
    models: DashMap<String, Arc<MarketplaceModel>>,
    total_published: AtomicU64,
    total_downloads: AtomicU64,
}

impl ModelMarketplace {
    /// Create a new empty marketplace.
    pub fn new() -> Self {
        Self {
            models: DashMap::new(),
            total_published: AtomicU64::new(0),
            total_downloads: AtomicU64::new(0),
        }
    }

    /// Publish a brand-new model. The model starts in Published status.
    pub fn publish(
        &self,
        name: &str,
        description: &str,
        author: &str,
        tags: Vec<String>,
        license: &str,
        initial_version: &str,
    ) -> Result<MarketplaceModelSnapshot, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let model = Arc::new(MarketplaceModel::new(
            &id,
            name,
            description,
            author,
            tags,
            license,
            initial_version,
            &format!("Initial version {}", initial_version),
            0,
            "",
        ));

        // Mark as published immediately
        {
            let mut status = model.status.write().unwrap();
            *status = ModelStatus::Published;
        }

        self.total_published.fetch_add(1, Ordering::Relaxed);
        let snapshot = model.snapshot();
        self.models.insert(id.clone(), model);
        Ok(snapshot)
    }

    /// Retrieve a model snapshot by id.
    pub fn get_model(&self, id: &str) -> Option<MarketplaceModelSnapshot> {
        self.models.get(id).map(|m| m.snapshot())
    }

    /// Update mutable fields of an existing model.
    ///
    /// Because `name`, `description`, and `tags` are plain `String`/`Vec` fields
    /// (no interior mutability), this method rebuilds the model by cloning the
    /// original, applying updates, and re-inserting it.
    pub fn update_model(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
        tags: Option<Vec<String>>,
    ) -> Result<MarketplaceModelSnapshot, String> {
        let entry = self
            .models
            .remove(id)
            .ok_or_else(|| format!("Model {} not found", id))?;

        let old = entry.1;

        // Build a new model preserving the same id, timestamps, counters, etc.
        let versions = DashMap::new();
        for kv in old.versions.iter() {
            versions.insert(kv.key().clone(), kv.value().clone());
        }

        let model = MarketplaceModel {
            id: old.id.clone(),
            name: name.unwrap_or(&old.name).to_string(),
            description: description.unwrap_or(&old.description).to_string(),
            author: old.author.clone(),
            tags: tags.unwrap_or_else(|| old.tags.clone()),
            status: old.status.clone(),
            versions,
            created_at: old.created_at,
            updated_at: Arc::new(RwLock::new(Utc::now())),
            rating: old.rating.clone(),
            review_count: AtomicU64::new(old.review_count.load(Ordering::Relaxed)),
            total_downloads: AtomicU64::new(old.total_downloads.load(Ordering::Relaxed)),
            license: old.license.clone(),
        };

        let snapshot = model.snapshot();
        self.models.insert(id.to_string(), Arc::new(model));
        Ok(snapshot)
    }

    /// Add a new version to an existing model.
    pub fn add_version(
        &self,
        model_id: &str,
        version: &str,
        description: &str,
        size_bytes: u64,
        checksum: &str,
    ) -> Result<ModelVersion, String> {
        let model = self
            .models
            .get(model_id)
            .ok_or_else(|| format!("Model {} not found", model_id))?;

        // Check for duplicate version
        if model.versions.contains_key(version) {
            return Err(format!("Version {} already exists for model {}", version, model_id));
        }

        // Check model is not removed
        {
            let status = model.status.read().unwrap();
            if *status == ModelStatus::Removed {
                return Err(format!("Cannot add version to removed model {}", model_id));
            }
        }

        // Mark all existing versions as non-latest
        for mut entry in model.versions.iter_mut() {
            entry.value_mut().is_latest = false;
        }

        let new_version = ModelVersion::new(model_id, version, description, size_bytes, checksum);
        let v = new_version.clone();
        model.versions.insert(version.to_string(), new_version);

        *model.updated_at.write().unwrap() = Utc::now();

        Ok(v)
    }

    /// Deprecate a model.
    pub fn deprecate_model(&self, id: &str) -> Result<MarketplaceModelSnapshot, String> {
        let model = self
            .models
            .get(id)
            .ok_or_else(|| format!("Model {} not found", id))?;

        let mut status = model.status.write().unwrap();
        match *status {
            ModelStatus::Published | ModelStatus::Draft => {
                *status = ModelStatus::Deprecated;
            }
            ModelStatus::Deprecated => {
                return Err(format!("Model {} is already deprecated", id));
            }
            ModelStatus::Archived | ModelStatus::Removed => {
                return Err(format!(
                    "Cannot deprecate model {} with status {}",
                    id, *status
                ));
            }
        }

        self.total_published.fetch_sub(1, Ordering::Relaxed);
        *model.updated_at.write().unwrap() = Utc::now();

        Ok(model.snapshot())
    }

    /// Archive a model.
    pub fn archive_model(&self, id: &str) -> Result<MarketplaceModelSnapshot, String> {
        let model = self
            .models
            .get(id)
            .ok_or_else(|| format!("Model {} not found", id))?;

        let mut status = model.status.write().unwrap();
        match *status {
            ModelStatus::Deprecated | ModelStatus::Draft => {
                *status = ModelStatus::Archived;
            }
            ModelStatus::Archived => {
                return Err(format!("Model {} is already archived", id));
            }
            ModelStatus::Published => {
                *status = ModelStatus::Archived;
                self.total_published.fetch_sub(1, Ordering::Relaxed);
            }
            ModelStatus::Removed => {
                return Err(format!("Cannot archive removed model {}", id));
            }
        }

        *model.updated_at.write().unwrap() = Utc::now();

        Ok(model.snapshot())
    }

    /// Remove a model.
    pub fn remove_model(&self, id: &str) -> Result<MarketplaceModelSnapshot, String> {
        let model = self
            .models
            .get(id)
            .ok_or_else(|| format!("Model {} not found", id))?;

        let mut status = model.status.write().unwrap();
        match *status {
            ModelStatus::Removed => {
                return Err(format!("Model {} is already removed", id));
            }
            _ => {
                if *status == ModelStatus::Published {
                    self.total_published.fetch_sub(1, Ordering::Relaxed);
                }
                *status = ModelStatus::Removed;
            }
        }

        *model.updated_at.write().unwrap() = Utc::now();

        Ok(model.snapshot())
    }

    /// Record a download for a specific model version.
    pub fn download_model(
        &self,
        model_id: &str,
        version: &str,
    ) -> Result<ModelVersion, String> {
        let model = self
            .models
            .get(model_id)
            .ok_or_else(|| format!("Model {} not found", model_id))?;

        {
            let status = model.status.read().unwrap();
            if *status != ModelStatus::Published {
                return Err(format!(
                    "Cannot download model {} with status {}",
                    model_id, *status
                ));
            }
        }

        let mut ver_entry = model
            .versions
            .get_mut(version)
            .ok_or_else(|| format!("Version {} not found for model {}", version, model_id))?;

        ver_entry.download_count += 1;
        let result = ver_entry.value().clone();

        model.total_downloads.fetch_add(1, Ordering::Relaxed);
        self.total_downloads.fetch_add(1, Ordering::Relaxed);

        Ok(result)
    }

    /// Rate a model with a score between 0.0 and 5.0.
    pub fn rate_model(&self, model_id: &str, score: f64) -> Result<(), String> {
        if !(0.0..=5.0).contains(&score) {
            return Err("Score must be between 0.0 and 5.0".to_string());
        }

        let model = self
            .models
            .get(model_id)
            .ok_or_else(|| format!("Model {} not found", model_id))?;

        let old_reviews = model.review_count.fetch_add(1, Ordering::Relaxed);
        let mut rating = model.rating.write().unwrap();

        // Compute running average
        let old_rating = *rating;
        let new_reviews = old_reviews + 1;
        *rating = (old_rating * old_reviews as f64 + score) / new_reviews as f64;

        Ok(())
    }

    /// Search models with filters.
    pub fn search(&self, filters: &SearchFilters) -> Vec<MarketplaceModelSnapshot> {
        let mut results: Vec<MarketplaceModelSnapshot> = self
            .models
            .iter()
            .map(|r| r.value().snapshot())
            .filter(|snap| {
                // Filter by tags
                if let Some(ref filter_tags) = filters.tags {
                    if !filter_tags.iter().any(|t| snap.tags.contains(t)) {
                        return false;
                    }
                }

                // Filter by author
                if let Some(ref author) = filters.author {
                    if &snap.author != author {
                        return false;
                    }
                }

                // Filter by status
                if let Some(ref status) = filters.status {
                    if &snap.status != status {
                        return false;
                    }
                }

                // Filter by minimum rating
                if let Some(min) = filters.min_rating {
                    if snap.rating < min {
                        return false;
                    }
                }

                true
            })
            .collect();

        // Sort
        match filters.sort_by.as_str() {
            "name" => results.sort_by(|a, b| a.name.cmp(&b.name)),
            "rating" => results.sort_by(|a, b| b.rating.partial_cmp(&a.rating).unwrap_or(std::cmp::Ordering::Equal)),
            "created_at" => results.sort_by(|a, b| b.created_at.cmp(&a.created_at)),
            "downloads" | _ => results.sort_by(|a, b| b.total_downloads.cmp(&a.total_downloads)),
        }

        // Paginate
        let offset = filters.offset as usize;
        let limit = filters.limit as usize;
        results
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect()
    }

    /// List all models.
    pub fn list_models(&self) -> Vec<MarketplaceModelSnapshot> {
        self.models.iter().map(|r| r.value().snapshot()).collect()
    }

    /// Get overall marketplace metrics.
    pub fn get_metrics(&self) -> MarketplaceMetrics {
        let total_models = self.models.len() as u64;
        let total_published = self.total_published.load(Ordering::Relaxed);
        let total_downloads = self.total_downloads.load(Ordering::Relaxed);
        let total_versions: u64 = self
            .models
            .iter()
            .map(|r| r.value().versions.len() as u64)
            .sum();

        MarketplaceMetrics {
            total_models,
            total_published,
            total_downloads,
            total_versions,
        }
    }
}

impl Default for ModelMarketplace {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_marketplace() -> ModelMarketplace {
        ModelMarketplace::new()
    }

    // -- publish / get --

    #[test]
    fn test_publish_model() {
        let mp = make_marketplace();
        let snap = mp
            .publish(
                "Test Model",
                "A test model",
                "alice",
                vec!["llm".to_string()],
                "MIT",
                "1.0.0",
            )
            .unwrap();

        assert_eq!(snap.name, "Test Model");
        assert_eq!(snap.author, "alice");
        assert_eq!(snap.status, ModelStatus::Published);
        assert_eq!(snap.tags, vec!["llm".to_string()]);
        assert_eq!(snap.versions.len(), 1);
        assert_eq!(snap.versions[0].version, "1.0.0");
    }

    #[test]
    fn test_get_model() {
        let mp = make_marketplace();
        let snap = mp
            .publish("M", "desc", "bob", vec![], "Apache-2.0", "0.1.0")
            .unwrap();
        let id = snap.id.clone();

        let fetched = mp.get_model(&id).unwrap();
        assert_eq!(fetched.id, id);
        assert_eq!(fetched.name, "M");
    }

    #[test]
    fn test_get_model_not_found() {
        let mp = make_marketplace();
        assert!(mp.get_model("nonexistent").is_none());
    }

    // -- update --

    #[test]
    fn test_update_model_name() {
        let mp = make_marketplace();
        let snap = mp
            .publish("Old", "desc", "carol", vec![], "MIT", "1.0.0")
            .unwrap();

        let updated = mp
            .update_model(&snap.id, Some("New Name"), None, None)
            .unwrap();
        assert_eq!(updated.name, "New Name");
    }

    #[test]
    fn test_update_model_tags() {
        let mp = make_marketplace();
        let snap = mp
            .publish("M", "desc", "carol", vec!["a".to_string()], "MIT", "1.0.0")
            .unwrap();

        let updated = mp
            .update_model(
                &snap.id,
                None,
                None,
                Some(vec!["b".to_string(), "c".to_string()]),
            )
            .unwrap();
        assert_eq!(updated.tags, vec!["b", "c"]);
    }

    #[test]
    fn test_update_model_not_found() {
        let mp = make_marketplace();
        let result = mp.update_model("nope", Some("X"), None, None);
        assert!(result.is_err());
    }

    // -- versions --

    #[test]
    fn test_add_version() {
        let mp = make_marketplace();
        let snap = mp
            .publish("M", "desc", "dave", vec![], "MIT", "1.0.0")
            .unwrap();

        let v = mp
            .add_version(&snap.id, "2.0.0", "Major update", 1024, "abc")
            .unwrap();
        assert_eq!(v.version, "2.0.0");
        assert!(v.is_latest);

        // Old version should no longer be latest
        let model = mp.models.get(&snap.id).unwrap();
        let old_v = model.versions.get("1.0.0").unwrap();
        assert!(!old_v.is_latest);
    }

    #[test]
    fn test_add_duplicate_version_fails() {
        let mp = make_marketplace();
        let snap = mp
            .publish("M", "desc", "eve", vec![], "MIT", "1.0.0")
            .unwrap();

        let result = mp.add_version(&snap.id, "1.0.0", "dup", 0, "");
        assert!(result.is_err());
    }

    #[test]
    fn test_add_version_removed_model_fails() {
        let mp = make_marketplace();
        let snap = mp
            .publish("M", "desc", "eve", vec![], "MIT", "1.0.0")
            .unwrap();
        mp.remove_model(&snap.id).unwrap();

        let result = mp.add_version(&snap.id, "2.0.0", "nope", 0, "");
        assert!(result.is_err());
    }

    // -- lifecycle: deprecate / archive / remove --

    #[test]
    fn test_deprecate_model() {
        let mp = make_marketplace();
        let snap = mp
            .publish("M", "desc", "frank", vec![], "MIT", "1.0.0")
            .unwrap();

        let deprecated = mp.deprecate_model(&snap.id).unwrap();
        assert_eq!(deprecated.status, ModelStatus::Deprecated);
    }

    #[test]
    fn test_deprecate_already_deprecated_fails() {
        let mp = make_marketplace();
        let snap = mp
            .publish("M", "desc", "frank", vec![], "MIT", "1.0.0")
            .unwrap();
        mp.deprecate_model(&snap.id).unwrap();

        let result = mp.deprecate_model(&snap.id);
        assert!(result.is_err());
    }

    #[test]
    fn test_archive_model() {
        let mp = make_marketplace();
        let snap = mp
            .publish("M", "desc", "grace", vec![], "MIT", "1.0.0")
            .unwrap();

        mp.deprecate_model(&snap.id).unwrap();
        let archived = mp.archive_model(&snap.id).unwrap();
        assert_eq!(archived.status, ModelStatus::Archived);
    }

    #[test]
    fn test_remove_model() {
        let mp = make_marketplace();
        let snap = mp
            .publish("M", "desc", "heidi", vec![], "MIT", "1.0.0")
            .unwrap();

        let removed = mp.remove_model(&snap.id).unwrap();
        assert_eq!(removed.status, ModelStatus::Removed);
    }

    #[test]
    fn test_remove_already_removed_fails() {
        let mp = make_marketplace();
        let snap = mp
            .publish("M", "desc", "ivan", vec![], "MIT", "1.0.0")
            .unwrap();
        mp.remove_model(&snap.id).unwrap();

        let result = mp.remove_model(&snap.id);
        assert!(result.is_err());
    }

    // -- downloads --

    #[test]
    fn test_download_model() {
        let mp = make_marketplace();
        let snap = mp
            .publish("M", "desc", "judy", vec![], "MIT", "1.0.0")
            .unwrap();

        let v = mp.download_model(&snap.id, "1.0.0").unwrap();
        assert_eq!(v.download_count, 1);

        // Check total downloads on model
        let model_snap = mp.get_model(&snap.id).unwrap();
        assert_eq!(model_snap.total_downloads, 1);
    }

    #[test]
    fn test_download_nonexistent_version_fails() {
        let mp = make_marketplace();
        let snap = mp
            .publish("M", "desc", "judy", vec![], "MIT", "1.0.0")
            .unwrap();

        let result = mp.download_model(&snap.id, "9.9.9");
        assert!(result.is_err());
    }

    #[test]
    fn test_download_deprecated_model_fails() {
        let mp = make_marketplace();
        let snap = mp
            .publish("M", "desc", "judy", vec![], "MIT", "1.0.0")
            .unwrap();
        mp.deprecate_model(&snap.id).unwrap();

        let result = mp.download_model(&snap.id, "1.0.0");
        assert!(result.is_err());
    }

    // -- rating --

    #[test]
    fn test_rate_model() {
        let mp = make_marketplace();
        let snap = mp
            .publish("M", "desc", "karl", vec![], "MIT", "1.0.0")
            .unwrap();

        mp.rate_model(&snap.id, 4.0).unwrap();
        mp.rate_model(&snap.id, 5.0).unwrap();

        let model_snap = mp.get_model(&snap.id).unwrap();
        assert_eq!(model_snap.rating, 4.5);
        assert_eq!(model_snap.review_count, 2);
    }

    #[test]
    fn test_rate_model_invalid_score() {
        let mp = make_marketplace();
        let snap = mp
            .publish("M", "desc", "karl", vec![], "MIT", "1.0.0")
            .unwrap();

        assert!(mp.rate_model(&snap.id, 6.0).is_err());
        assert!(mp.rate_model(&snap.id, -1.0).is_err());
    }

    #[test]
    fn test_rate_nonexistent_model_fails() {
        let mp = make_marketplace();
        assert!(mp.rate_model("nope", 3.0).is_err());
    }

    // -- search --

    #[test]
    fn test_search_by_author() {
        let mp = make_marketplace();
        mp.publish("M1", "desc", "alice", vec![], "MIT", "1.0.0")
            .unwrap();
        mp.publish("M2", "desc", "bob", vec![], "MIT", "1.0.0")
            .unwrap();

        let filters = SearchFilters {
            author: Some("alice".to_string()),
            ..Default::default()
        };
        let results = mp.search(&filters);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].author, "alice");
    }

    #[test]
    fn test_search_by_tags() {
        let mp = make_marketplace();
        mp.publish(
            "M1",
            "desc",
            "alice",
            vec!["llm".to_string(), "nlp".to_string()],
            "MIT",
            "1.0.0",
        )
        .unwrap();
        mp.publish("M2", "desc", "bob", vec!["cv".to_string()], "MIT", "1.0.0")
            .unwrap();

        let filters = SearchFilters {
            tags: Some(vec!["nlp".to_string()]),
            ..Default::default()
        };
        let results = mp.search(&filters);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "M1");
    }

    #[test]
    fn test_search_by_status() {
        let mp = make_marketplace();
        let snap = mp
            .publish("M1", "desc", "alice", vec![], "MIT", "1.0.0")
            .unwrap();
        mp.deprecate_model(&snap.id).unwrap();

        let filters = SearchFilters {
            status: Some(ModelStatus::Deprecated),
            ..Default::default()
        };
        let results = mp.search(&filters);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_by_min_rating() {
        let mp = make_marketplace();
        let snap1 = mp
            .publish("M1", "desc", "alice", vec![], "MIT", "1.0.0")
            .unwrap();
        let snap2 = mp
            .publish("M2", "desc", "bob", vec![], "MIT", "1.0.0")
            .unwrap();

        mp.rate_model(&snap1.id, 5.0).unwrap();
        mp.rate_model(&snap2.id, 1.0).unwrap();

        let filters = SearchFilters {
            min_rating: Some(4.0),
            ..Default::default()
        };
        let results = mp.search(&filters);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "M1");
    }

    #[test]
    fn test_search_pagination() {
        let mp = make_marketplace();
        for i in 0..5 {
            mp.publish(
                &format!("M{}", i),
                "desc",
                "alice",
                vec![],
                "MIT",
                "1.0.0",
            )
            .unwrap();
        }

        let filters = SearchFilters {
            limit: 2,
            offset: 1,
            ..Default::default()
        };
        let results = mp.search(&filters);
        assert_eq!(results.len(), 2);
    }

    // -- list / metrics --

    #[test]
    fn test_list_models() {
        let mp = make_marketplace();
        mp.publish("M1", "desc", "a", vec![], "MIT", "1.0.0")
            .unwrap();
        mp.publish("M2", "desc", "b", vec![], "MIT", "1.0.0")
            .unwrap();

        let list = mp.list_models();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_get_metrics() {
        let mp = make_marketplace();
        let snap = mp
            .publish("M", "desc", "a", vec![], "MIT", "1.0.0")
            .unwrap();
        mp.add_version(&snap.id, "2.0.0", "v2", 100, "chk")
            .unwrap();
        mp.download_model(&snap.id, "1.0.0").unwrap();

        let metrics = mp.get_metrics();
        assert_eq!(metrics.total_models, 1);
        assert_eq!(metrics.total_published, 1);
        assert_eq!(metrics.total_downloads, 1);
        assert_eq!(metrics.total_versions, 2);
    }

    // -- default --

    #[test]
    fn test_default_marketplace() {
        let mp = ModelMarketplace::default();
        assert_eq!(mp.models.len(), 0);
    }

    // -- status display --

    #[test]
    fn test_status_display() {
        assert_eq!(ModelStatus::Draft.to_string(), "Draft");
        assert_eq!(ModelStatus::Published.to_string(), "Published");
        assert_eq!(ModelStatus::Deprecated.to_string(), "Deprecated");
        assert_eq!(ModelStatus::Archived.to_string(), "Archived");
        assert_eq!(ModelStatus::Removed.to_string(), "Removed");
    }

    // -- snapshot --

    #[test]
    fn test_snapshot_includes_all_versions() {
        let mp = make_marketplace();
        let snap = mp
            .publish("M", "desc", "a", vec![], "MIT", "1.0.0")
            .unwrap();
        mp.add_version(&snap.id, "2.0.0", "v2", 100, "c")
            .unwrap();
        mp.add_version(&snap.id, "3.0.0", "v3", 200, "d")
            .unwrap();

        let fetched = mp.get_model(&snap.id).unwrap();
        assert_eq!(fetched.versions.len(), 3);
        // Versions should be sorted newest first
        assert_eq!(fetched.versions[0].version, "3.0.0");
        assert_eq!(fetched.versions[1].version, "2.0.0");
        assert_eq!(fetched.versions[2].version, "1.0.0");
    }
}
