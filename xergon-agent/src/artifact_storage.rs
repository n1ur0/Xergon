use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageType {
    Local,
    S3,
    GCS,
    Azure,
}

impl Default for StorageType {
    fn default() -> Self {
        Self::Local
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompressionAlgorithm {
    None,
    Gzip,
    Zstd,
    Lz4,
}

impl Default for CompressionAlgorithm {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactStorageConfig {
    pub storage_type: StorageType,
    pub base_path: PathBuf,
    pub max_storage_mb: u64,
    pub compression: CompressionAlgorithm,
    pub integrity_check: bool,
    pub replication_factor: u32,
    pub cleanup_interval_secs: u64,
}

impl Default for ArtifactStorageConfig {
    fn default() -> Self {
        Self {
            storage_type: StorageType::Local,
            base_path: PathBuf::from("./artifacts"),
            max_storage_mb: 102_400, // 100 GB
            compression: CompressionAlgorithm::None,
            integrity_check: true,
            replication_factor: 1,
            cleanup_interval_secs: 3600,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateArtifactStorageConfigRequest {
    pub storage_type: Option<StorageType>,
    pub base_path: Option<PathBuf>,
    pub max_storage_mb: Option<u64>,
    pub compression: Option<CompressionAlgorithm>,
    pub integrity_check: Option<bool>,
    pub replication_factor: Option<u32>,
    pub cleanup_interval_secs: Option<u64>,
}

// ---------------------------------------------------------------------------
// Artifact type
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
    Weights,
    Config,
    Tokenizer,
    FineTuneAdapter,
    LoRAAdapter,
    QuantizedWeights,
    ONNXExport,
    GGUFExport,
    Logs,
    Benchmark,
    Checkpoint,
    Custom(String),
}

impl std::fmt::Display for ArtifactType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArtifactType::Custom(s) => write!(f, "custom:{}", s),
            other => write!(f, "{:?}", other),
        }
    }
}

// ---------------------------------------------------------------------------
// Model artifact
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelArtifact {
    pub id: String,
    pub model_id: String,
    pub name: String,
    pub version: String,
    pub artifact_type: ArtifactType,
    pub size_bytes: u64,
    pub checksum: String,
    pub storage_path: String,
    pub created_at: String,
    pub metadata: HashMap<String, String>,
    pub tags: Vec<String>,
}

// ---------------------------------------------------------------------------
// Query / Request types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct ListArtifactsQuery {
    pub model_id: Option<String>,
    pub artifact_type: Option<String>,
    pub tag: Option<String>,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateArtifactRequest {
    pub model_id: String,
    pub name: String,
    pub version: String,
    pub artifact_type: String,
    pub metadata: Option<HashMap<String, String>>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateArtifactResponse {
    pub id: String,
    pub storage_path: String,
    pub checksum: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifyResult {
    pub id: String,
    pub valid: bool,
    pub expected_checksum: String,
    pub actual_checksum: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageStatsResponse {
    pub total_artifacts: u64,
    pub total_size_bytes: u64,
    pub storage_used_mb: f64,
    pub downloads: u64,
    pub uploads: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CleanupResult {
    pub artifacts_removed: usize,
    pub bytes_freed: u64,
}

// ---------------------------------------------------------------------------
// ArtifactStorage
// ---------------------------------------------------------------------------

pub struct ArtifactStorage {
    config: tokio::sync::RwLock<ArtifactStorageConfig>,
    artifacts: DashMap<String, ModelArtifact>,
    // In-memory data store (keyed by artifact id)
    data: DashMap<String, Vec<u8>>,
    total_size_bytes: AtomicU64,
    downloads: AtomicU64,
    uploads: AtomicU64,
}

impl ArtifactStorage {
    pub fn new(config: ArtifactStorageConfig) -> Self {
        // Ensure base path exists
        let _ = std::fs::create_dir_all(&config.base_path);
        Self {
            config: tokio::sync::RwLock::new(config),
            artifacts: DashMap::new(),
            data: DashMap::new(),
            total_size_bytes: AtomicU64::new(0),
            downloads: AtomicU64::new(0),
            uploads: AtomicU64::new(0),
        }
    }

    pub fn default() -> Self {
        Self::new(ArtifactStorageConfig::default())
    }

    /// Create a new artifact.
    pub async fn create_artifact(&self, req: CreateArtifactRequest) -> Result<CreateArtifactResponse, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let cfg = self.config.read().await;

        let artifact_type = match req.artifact_type.to_lowercase().as_str() {
            "weights" => ArtifactType::Weights,
            "config" => ArtifactType::Config,
            "tokenizer" => ArtifactType::Tokenizer,
            "finetune_adapter" | "fine_tune_adapter" => ArtifactType::FineTuneAdapter,
            "lora_adapter" => ArtifactType::LoRAAdapter,
            "quantized_weights" => ArtifactType::QuantizedWeights,
            "onnx_export" => ArtifactType::ONNXExport,
            "gguf_export" => ArtifactType::GGUFExport,
            "logs" => ArtifactType::Logs,
            "benchmark" => ArtifactType::Benchmark,
            "checkpoint" => ArtifactType::Checkpoint,
            other => ArtifactType::Custom(other.to_string()),
        };

        let storage_path = cfg.base_path.join(&id).to_string_lossy().to_string();
        let checksum = Self::empty_checksum(); // Will be updated when data is written
        let now = chrono::Utc::now().to_rfc3339();

        let artifact = ModelArtifact {
            id: id.clone(),
            model_id: req.model_id,
            name: req.name,
            version: req.version,
            artifact_type,
            size_bytes: 0,
            checksum: checksum.clone(),
            storage_path: storage_path.clone(),
            created_at: now,
            metadata: req.metadata.unwrap_or_default(),
            tags: req.tags.unwrap_or_default(),
        };

        // Write metadata file
        let meta_path = cfg.base_path.join(format!("{}.meta.json", id));
        if let Ok(json) = serde_json::to_string_pretty(&artifact) {
            let _ = std::fs::write(meta_path, json);
        }

        self.artifacts.insert(id.clone(), artifact);
        self.uploads.fetch_add(1, Ordering::Relaxed);

        Ok(CreateArtifactResponse {
            id,
            storage_path,
            checksum,
            size_bytes: 0,
        })
    }

    /// Write data for an artifact.
    pub async fn write_data(&self, id: &str, data: Vec<u8>) -> Result<u64, String> {
        let cfg = self.config.read().await;
        let checksum = Self::compute_checksum(&data);
        let size = data.len() as u64;

        // Write to disk
        let path = cfg.base_path.join(id);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&path, &data).map_err(|e| format!("Failed to write artifact: {}", e))?;

        // Update in-memory data
        self.data.insert(id.to_string(), data);
        self.total_size_bytes.fetch_add(size, Ordering::Relaxed);

        // Update metadata
        if let Some(mut artifact) = self.artifacts.get_mut(id) {
            artifact.size_bytes = size;
            artifact.checksum = checksum.clone();
        }

        // Update meta file
        let meta_path = cfg.base_path.join(format!("{}.meta.json", id));
        if let Some(artifact) = self.artifacts.get(id) {
            if let Ok(json) = serde_json::to_string_pretty(&*artifact) {
                let _ = std::fs::write(meta_path, json);
            }
        }

        Ok(size)
    }

    /// Read artifact data.
    pub async fn read_data(&self, id: &str) -> Result<Vec<u8>, String> {
        // Try in-memory first
        if let Some(data) = self.data.get(id) {
            self.downloads.fetch_add(1, Ordering::Relaxed);
            return Ok(data.clone());
        }

        // Fall back to disk
        let cfg = self.config.read().await;
        let path = cfg.base_path.join(id);
        let data = std::fs::read(&path).map_err(|e| format!("Artifact not found: {}", e))?;
        self.downloads.fetch_add(1, Ordering::Relaxed);
        Ok(data)
    }

    /// Get artifact metadata.
    pub fn get_artifact(&self, id: &str) -> Option<ModelArtifact> {
        self.artifacts.get(id).map(|r| r.value().clone())
    }

    /// List artifacts with optional filters.
    pub fn list_artifacts(&self, query: &ListArtifactsQuery) -> Vec<ModelArtifact> {
        let offset = query.offset.unwrap_or(0);
        let limit = query.limit.unwrap_or(50);
        let tag = query.tag.as_deref();

        self.artifacts
            .iter()
            .filter(|r| {
                let a = r.value();
                if let Some(ref model_id) = query.model_id {
                    if &a.model_id != model_id { return false; }
                }
                if let Some(ref t) = query.artifact_type {
                    if a.artifact_type.to_string() != *t { return false; }
                }
                if let Some(tag_filter) = tag {
                    if !a.tags.iter().any(|t| t == tag_filter) { return false; }
                }
                true
            })
            .skip(offset)
            .take(limit)
            .map(|r| r.value().clone())
            .collect()
    }

    /// Delete an artifact.
    pub async fn delete_artifact(&self, id: &str) -> Result<ModelArtifact, String> {
        let artifact = self.artifacts.remove(id)
            .ok_or_else(|| "Artifact not found".to_string())?
            .1;

        // Remove data
        let size = self.data.remove(id).map(|(_, d)| d.len() as u64).unwrap_or(0);
        self.total_size_bytes.fetch_sub(size, Ordering::Relaxed);

        // Remove files
        let cfg = self.config.read().await;
        let _ = std::fs::remove_file(cfg.base_path.join(id));
        let _ = std::fs::remove_file(cfg.base_path.join(format!("{}.meta.json", id)));

        Ok(artifact)
    }

    /// Verify artifact integrity.
    pub async fn verify_artifact(&self, id: &str) -> Result<VerifyResult, String> {
        let artifact = self.artifacts.get(id)
            .map(|r| r.value().clone())
            .ok_or_else(|| "Artifact not found".to_string())?;

        let expected = artifact.checksum.clone();
        let actual = match self.read_data(id).await {
            Ok(data) => Self::compute_checksum(&data),
            Err(_) => Self::empty_checksum(),
        };

        Ok(VerifyResult {
            id: id.to_string(),
            valid: expected == actual,
            expected_checksum: expected,
            actual_checksum: actual,
        })
    }

    /// Get storage statistics.
    pub async fn get_stats(&self) -> StorageStatsResponse {
        StorageStatsResponse {
            total_artifacts: self.artifacts.len() as u64,
            total_size_bytes: self.total_size_bytes.load(Ordering::Relaxed),
            storage_used_mb: self.total_size_bytes.load(Ordering::Relaxed) as f64 / (1024.0 * 1024.0),
            downloads: self.downloads.load(Ordering::Relaxed),
            uploads: self.uploads.load(Ordering::Relaxed),
        }
    }

    /// Cleanup oldest artifacts when exceeding quota. If target_mb is None, uses config max.
    pub async fn cleanup(&self, target_mb: Option<u64>) -> CleanupResult {
        let cfg = self.config.read().await;
        let max_bytes = target_mb.unwrap_or(cfg.max_storage_mb) * 1024 * 1024;
        let current = self.total_size_bytes.load(Ordering::Relaxed);

        if current <= max_bytes {
            return CleanupResult {
                artifacts_removed: 0,
                bytes_freed: 0,
            };
        }

        // Sort artifacts by creation time, oldest first
        let mut sorted: Vec<ModelArtifact> = self.artifacts.iter()
            .map(|r| r.value().clone())
            .collect();
        sorted.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        let mut freed = 0u64;
        let mut removed = 0usize;
        let mut remaining = current;

        for artifact in sorted {
            if remaining <= max_bytes {
                break;
            }
            let _ = self.delete_artifact(&artifact.id).await;
            remaining -= artifact.size_bytes;
            freed += artifact.size_bytes;
            removed += 1;
        }

        CleanupResult {
            artifacts_removed: removed,
            bytes_freed: freed,
        }
    }

    /// Update storage config.
    pub async fn update_config(&self, update: UpdateArtifactStorageConfigRequest) -> ArtifactStorageConfig {
        let mut cfg = self.config.write().await;
        if let Some(v) = update.storage_type { cfg.storage_type = v; }
        if let Some(v) = update.base_path { cfg.base_path = v; }
        if let Some(v) = update.max_storage_mb { cfg.max_storage_mb = v; }
        if let Some(v) = update.compression { cfg.compression = v; }
        if let Some(v) = update.integrity_check { cfg.integrity_check = v; }
        if let Some(v) = update.replication_factor { cfg.replication_factor = v; }
        if let Some(v) = update.cleanup_interval_secs { cfg.cleanup_interval_secs = v; }
        cfg.clone()
    }

    fn compute_checksum(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        hex::encode(hasher.finalize())
    }

    fn empty_checksum() -> String {
        let mut hasher = Sha256::new();
        hex::encode(hasher.finalize())
    }
}
