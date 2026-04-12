//! Model caching with LRU (Least Recently Used) eviction.
//!
//! Manages downloaded model files on disk with:
//! - LRU eviction based on disk usage (not just count)
//! - Pin mechanism to protect important models from eviction
//! - Automatic eviction when disk usage exceeds configurable limit
//! - Cache statistics tracking
//!
//! API endpoints:
//! - GET    /api/cache/stats          -- cache statistics
//! - GET    /api/cache/models         -- list cached models with sizes and timestamps
//! - DELETE /api/cache/models/{id}    -- manually evict a model
//! - POST   /api/cache/models/{id}/pin -- pin/unpin a model

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::config::ModelCacheConfig;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Information about a single cached model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedModelInfo {
    /// Model identifier (e.g. "meta-llama/Llama-3.1-8B-Instruct")
    pub model_id: String,
    /// Path to the model directory on disk.
    pub path: String,
    /// Total size in bytes of all files in the model directory.
    pub size_bytes: u64,
    /// Whether the model is pinned (protected from eviction).
    pub pinned: bool,
    /// Last access timestamp (Unix epoch seconds).
    pub last_accessed_secs: u64,
    /// Last access timestamp (RFC3339 for API responses).
    pub last_accessed: String,
    /// When the model was first cached (Unix epoch seconds).
    pub cached_at_secs: u64,
    /// When the model was first cached (RFC3339 for API responses).
    pub cached_at: String,
    /// Number of GGUF files in the model directory.
    pub file_count: usize,
}

/// Cache statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    /// Total cache size in bytes.
    pub total_size_bytes: u64,
    /// Total cache size in GB (human-readable).
    pub total_size_gb: f64,
    /// Number of cached models.
    pub model_count: usize,
    /// Number of pinned models.
    pub pinned_count: usize,
    /// Available disk space in bytes.
    pub available_bytes: u64,
    /// Available disk space in GB.
    pub available_gb: f64,
    /// Maximum cache size in bytes.
    pub max_size_bytes: u64,
    /// Maximum cache size in GB.
    pub max_size_gb: f64,
    /// Disk usage percentage (0.0 - 100.0).
    pub usage_percent: f64,
    /// Cache directory path.
    pub cache_dir: String,
    /// High water mark eviction threshold (percentage).
    pub eviction_threshold_percent: f64,
}

/// Response for evicting a model.
#[derive(Debug, Serialize)]
pub struct EvictResponse {
    pub model_id: String,
    pub evicted: bool,
    pub freed_bytes: u64,
    pub error: Option<String>,
}

/// Response for pinning/unpinning a model.
#[derive(Debug, Serialize)]
pub struct PinResponse {
    pub model_id: String,
    pub pinned: bool,
    pub error: Option<String>,
}

// ---------------------------------------------------------------------------
// ModelCache service
// ---------------------------------------------------------------------------

/// Model cache with LRU eviction based on disk usage.
///
/// Tracks models by their directory on disk, measures size, and evicts
/// least-recently-used models when disk usage exceeds the configured limit.
pub struct ModelCache {
    config: ModelCacheConfig,
    cache_dir: PathBuf,
    /// In-memory model registry (model_id -> info).
    models: Arc<RwLock<HashMap<String, CachedModelInfo>>>,
    /// LRU ordering: most recently used at the back.
    lru_order: Arc<RwLock<Vec<String>>>,
    /// State file for persistence.
    state_file: PathBuf,
}

impl ModelCache {
    /// Create a new model cache.
    pub fn new(config: ModelCacheConfig) -> Result<Self> {
        let cache_dir = if config.cache_dir.is_empty() {
            dirs::cache_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join("xergon-agent")
                .join("models")
        } else {
            PathBuf::from(&config.cache_dir)
        };

        let state_file = cache_dir.join(".cache_state.json");

        Ok(Self {
            config,
            cache_dir: cache_dir.clone(),
            models: Arc::new(RwLock::new(HashMap::new())),
            lru_order: Arc::new(RwLock::new(Vec::new())),
            state_file,
        })
    }

    /// Initialize the cache: create directory, scan existing models, load state.
    pub async fn initialize(&self) -> Result<()> {
        // Create cache directory if it doesn't exist
        tokio::fs::create_dir_all(&self.cache_dir)
            .await
            .context("Failed to create cache directory")?;

        info!(dir = %self.cache_dir.display(), "Cache directory ready");

        // Scan existing model directories
        self.scan_disk().await?;

        // Load persisted pin state
        self.load_state().await?;

        // Run initial eviction check
        self.check_and_evict().await?;

        let stats = self.stats().await;
        info!(
            model_count = stats.model_count,
            total_gb = stats.total_size_gb,
            available_gb = stats.available_gb,
            "Model cache initialized"
        );

        Ok(())
    }

    /// Scan the cache directory for existing model directories.
    async fn scan_disk(&self) -> Result<()> {
        let mut entries = tokio::fs::read_dir(&self.cache_dir)
            .await
            .context("Failed to read cache directory")?;

        let mut models = self.models.write().await;
        let mut lru = self.lru_order.write().await;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();

            // Skip the state file and hidden files
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') || !path.is_dir() {
                    continue;
                }
            }

            // Try to compute model size
            let size_bytes = dir_size(&path).await.unwrap_or(0);
            let file_count = count_gguf_files(&path).await.unwrap_or(0);

            // Skip empty directories
            if size_bytes == 0 {
                continue;
            }

            let model_id = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            let _now_secs = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            // Get directory modification time as a proxy for last access
            let modified_secs = match tokio::fs::metadata(&path).await {
                Ok(meta) => meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                Err(_) => 0,
            };

            let info = CachedModelInfo {
                model_id: model_id.clone(),
                path: path.to_string_lossy().to_string(),
                size_bytes,
                pinned: false, // Will be updated from state file
                last_accessed_secs: modified_secs,
                last_accessed: secs_to_rfc3339(modified_secs),
                cached_at_secs: modified_secs,
                cached_at: secs_to_rfc3339(modified_secs),
                file_count,
            };

            models.insert(model_id.clone(), info);
            lru.push(model_id);
        }

        Ok(())
    }

    /// Load persisted state (pins) from disk.
    async fn load_state(&self) -> Result<()> {
        if !self.state_file.exists() {
            return Ok(());
        }

        let content = tokio::fs::read_to_string(&self.state_file)
            .await
            .context("Failed to read cache state file")?;

        #[derive(Deserialize)]
        struct PersistedState {
            pinned: Vec<String>,
        }

        let state: PersistedState = match serde_json::from_str(&content) {
            Ok(s) => s,
            Err(_) => return Ok(()), // Ignore corrupt state file
        };

        let pinned_count = state.pinned.len();
        let mut models = self.models.write().await;
        for model_id in state.pinned {
            if let Some(model) = models.get_mut(&model_id) {
                model.pinned = true;
            }
        }

        debug!(pinned_count, "Loaded cache pin state");
        Ok(())
    }

    /// Save state (pins) to disk.
    async fn save_state(&self) -> Result<()> {
        let models = self.models.read().await;
        let pinned: Vec<String> = models
            .iter()
            .filter(|(_, m)| m.pinned)
            .map(|(id, _)| id.clone())
            .collect();

        let state = serde_json::json!({ "pinned": pinned });

        tokio::fs::write(&self.state_file, serde_json::to_string_pretty(&state)?.as_bytes())
            .await
            .context("Failed to write cache state file")?;

        Ok(())
    }

    /// Register a model as cached (or update its last-accessed time).
    pub async fn touch(&self, model_id: &str) {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut models = self.models.write().await;
        let mut lru = self.lru_order.write().await;

        if let Some(model) = models.get_mut(model_id) {
            model.last_accessed_secs = now_secs;
            model.last_accessed = secs_to_rfc3339(now_secs);
        } else {
            // New model — compute its size from the cache directory
            let model_path = self.cache_dir.join(sanitize_model_id(model_id));
            let size_bytes = dir_size(&model_path).await.unwrap_or(0);
            let file_count = count_gguf_files(&model_path).await.unwrap_or(0);

            let info = CachedModelInfo {
                model_id: model_id.to_string(),
                path: model_path.to_string_lossy().to_string(),
                size_bytes,
                pinned: false,
                last_accessed_secs: now_secs,
                last_accessed: secs_to_rfc3339(now_secs),
                cached_at_secs: now_secs,
                cached_at: secs_to_rfc3339(now_secs),
                file_count,
            };

            models.insert(model_id.to_string(), info);
        }

        // Move to back of LRU (most recently used)
        lru.retain(|id| id != model_id);
        lru.push(model_id.to_string());
    }

    /// Pin a model so it won't be evicted.
    pub async fn pin_model(&self, model_id: &str) -> Result<bool> {
        let mut models = self.models.write().await;

        if let Some(model) = models.get_mut(model_id) {
            model.pinned = true;
            drop(models);
            self.save_state().await?;
            info!(model_id = %model_id, "Model pinned");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Unpin a model so it can be evicted.
    pub async fn unpin_model(&self, model_id: &str) -> Result<bool> {
        let mut models = self.models.write().await;

        if let Some(model) = models.get_mut(model_id) {
            model.pinned = false;
            drop(models);
            self.save_state().await?;
            info!(model_id = %model_id, "Model unpinned");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Manually evict a model from the cache.
    ///
    /// Returns the number of bytes freed.
    pub async fn evict_model(&self, model_id: &str) -> Result<(bool, u64)> {
        let mut models = self.models.write().await;

        if let Some(model) = models.remove(model_id) {
            if model.pinned {
                warn!(
                    model_id = %model_id,
                    "Attempted to evict pinned model, rejecting"
                );
                models.insert(model_id.to_string(), model);
                return Ok((false, 0));
            }

            let freed = model.size_bytes;
            let path = PathBuf::from(&model.path);

            drop(models);

            // Remove from LRU order
            let mut lru = self.lru_order.write().await;
            lru.retain(|id| id != model_id);
            drop(lru);

            // Delete from disk
            if path.exists() {
                match tokio::fs::remove_dir_all(&path).await {
                    Ok(()) => {
                        info!(
                            model_id = %model_id,
                            freed_gb = freed as f64 / 1_073_741_824.0,
                            "Model evicted from cache"
                        );
                    }
                    Err(e) => {
                        warn!(
                            model_id = %model_id,
                            error = %e,
                            path = %path.display(),
                            "Failed to delete model directory from disk"
                        );
                    }
                }
            }

            Ok((true, freed))
        } else {
            Ok((false, 0))
        }
    }

    /// Check disk usage and evict LRU models if over the limit.
    pub async fn check_and_evict(&self) -> Result<u64> {
        let max_bytes = (self.config.max_size_gb as u64) * 1_073_741_824;
        let threshold_bytes = (max_bytes as f64 * self.config.eviction_threshold_percent / 100.0) as u64;

        let total_bytes = {
            let models = self.models.read().await;
            models.values().map(|m| m.size_bytes).sum::<u64>()
        };

        if total_bytes <= threshold_bytes {
            return Ok(0);
        }

        info!(
            total_gb = total_bytes as f64 / 1_073_741_824.0,
            threshold_gb = threshold_bytes as f64 / 1_073_741_824.0,
            "Cache usage exceeds threshold, starting LRU eviction"
        );

        let mut freed_total: u64 = 0;
        let target = total_bytes - threshold_bytes;

        // Evict from the front of the LRU list (oldest first)
        loop {
            if freed_total >= target {
                break;
            }

            // Get the oldest non-pinned model
            let to_evict = {
                let lru = self.lru_order.read().await;
                let models = self.models.read().await;
                lru.iter()
                    .find(|id| {
                        models
                            .get(*id)
                            .map(|m| !m.pinned)
                            .unwrap_or(false)
                    })
                    .cloned()
            };

            match to_evict {
                Some(model_id) => {
                    let (evicted, freed) = self.evict_model(&model_id).await?;
                    if evicted {
                        freed_total += freed;
                        debug!(
                            model_id = %model_id,
                            freed_gb = freed as f64 / 1_073_741_824.0,
                            total_freed_gb = freed_total as f64 / 1_073_741_824.0,
                            "Evicted LRU model"
                        );
                    } else {
                        // Model was pinned or not found, skip
                        let mut lru = self.lru_order.write().await;
                        lru.retain(|id| id != &model_id);
                    }
                }
                None => {
                    debug!("No more evictable models");
                    break;
                }
            }
        }

        if freed_total > 0 {
            info!(
                freed_gb = freed_total as f64 / 1_073_741_824.0,
                "LRU eviction complete"
            );
        }

        Ok(freed_total)
    }

    /// Get cache statistics.
    pub async fn stats(&self) -> CacheStats {
        let models = self.models.read().await;

        let total_size_bytes: u64 = models.values().map(|m| m.size_bytes).sum();
        let model_count = models.len();
        let pinned_count = models.values().filter(|m| m.pinned).count();

        let max_bytes = (self.config.max_size_gb as u64) * 1_073_741_824;

        // Get available disk space
        let available_bytes = available_disk_space(&self.cache_dir).await.unwrap_or(0);

        let usage_percent = if max_bytes > 0 {
            (total_size_bytes as f64 / max_bytes as f64) * 100.0
        } else {
            0.0
        };

        CacheStats {
            total_size_bytes,
            total_size_gb: total_size_bytes as f64 / 1_073_741_824.0,
            model_count,
            pinned_count,
            available_bytes,
            available_gb: available_bytes as f64 / 1_073_741_824.0,
            max_size_bytes: max_bytes,
            max_size_gb: self.config.max_size_gb as f64,
            usage_percent,
            cache_dir: self.cache_dir.to_string_lossy().to_string(),
            eviction_threshold_percent: self.config.eviction_threshold_percent,
        }
    }

    /// List all cached models.
    pub async fn list_models(&self) -> Vec<CachedModelInfo> {
        let models = self.models.read().await;
        let mut list: Vec<CachedModelInfo> = models.values().cloned().collect();

        // Sort by last accessed (most recent first)
        list.sort_by(|a, b| b.last_accessed_secs.cmp(&a.last_accessed_secs));

        list
    }

    /// Get the cache directory path.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Get the max cache size in bytes.
    pub fn max_size_bytes(&self) -> u64 {
        (self.config.max_size_gb as u64) * 1_073_741_824
    }

    /// Spawn a background eviction checker.
    pub fn spawn_eviction_checker(self: Arc<Self>, interval_secs: u64) {
        tokio::spawn(async move {
            loop {
                if let Err(e) = self.check_and_evict().await {
                    warn!(error = %e, "Background eviction check failed");
                }
                tokio::time::sleep(Duration::from_secs(interval_secs)).await;
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Recursively compute the total size of a directory in bytes.
fn dir_size(path: &Path) -> BoxFuture<'static, Result<u64>> {
    let path = path.to_owned();
    Box::pin(async move {
        let mut total = 0u64;
        let mut entries = tokio::fs::read_dir(&path)
            .await
            .context("Failed to read directory")?;

        while let Some(entry) = entries.next_entry().await? {
            let metadata = entry.metadata().await?;
            if metadata.is_file() {
                total += metadata.len();
            } else if metadata.is_dir() {
                total += dir_size(&entry.path()).await.unwrap_or(0);
            }
        }
        Ok(total)
    })
}

/// Count GGUF files in a directory (non-recursive).
async fn count_gguf_files(path: &Path) -> Result<usize> {
    let mut count = 0;
    let mut entries = tokio::fs::read_dir(path)
        .await
        .context("Failed to read directory")?;

    while let Some(entry) = entries.next_entry().await? {
        if let Some(name) = entry.file_name().to_str() {
            if name.to_lowercase().ends_with(".gguf") {
                count += 1;
            }
        }
    }

    Ok(count)
}

/// Get available disk space for the filesystem containing the given path.
#[cfg(target_os = "linux")]
async fn available_disk_space(path: &Path) -> Result<u64> {
    use std::os::unix::ffi::OsStrExt;
    let path_cstr = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap_or_default();
    let mut statvfs: libc::statvfs = unsafe { std::mem::zeroed() };

    let ret = unsafe { libc::statvfs(path_cstr.as_ptr(), &mut statvfs) };
    if ret != 0 {
        return Ok(0);
    }

    let available = statvfs.f_bavail as u64 * statvfs.f_frsize as u64;
    Ok(available)
}

#[cfg(target_os = "macos")]
async fn available_disk_space(path: &Path) -> Result<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let path_cstr = CString::new(path.as_os_str().as_bytes()).unwrap_or_default();
    let mut statvfs: libc::statvfs = unsafe { std::mem::zeroed() };

    let ret = unsafe { libc::statvfs(path_cstr.as_ptr(), &mut statvfs) };
    if ret != 0 {
        return Ok(0);
    }

    let available = statvfs.f_bavail as u64 * statvfs.f_frsize as u64;
    Ok(available)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
async fn available_disk_space(_path: &Path) -> Result<u64> {
    // Fallback: no disk space info available
    Ok(0)
}

/// Convert Unix epoch seconds to RFC3339 string.
fn secs_to_rfc3339(secs: u64) -> String {
    chrono::DateTime::from_timestamp(secs as i64, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string())
}

/// Sanitize a model ID for use as a directory name.
/// Replaces '/' with '_' and other unsafe characters.
fn sanitize_model_id(model_id: &str) -> String {
    model_id
        .chars()
        .map(|c| match c {
            '/' => '_',
            '\\' => '_',
            ':' => '_',
            '*' => '_',
            '?' => '_',
            '"' => '_',
            '<' => '_',
            '>' => '_',
            '|' => '_',
            c if c.is_ascii_control() => '_',
            c => c,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_model_id() {
        assert_eq!(
            sanitize_model_id("meta-llama/Llama-3.1-8B"),
            "meta-llama_Llama-3.1-8B"
        );
        assert_eq!(
            sanitize_model_id("Qwen/Qwen2.5-7B-Instruct"),
            "Qwen_Qwen2.5-7B-Instruct"
        );
    }

    #[test]
    fn test_secs_to_rfc3339() {
        let result = secs_to_rfc3339(1704067200); // 2024-01-01
        assert!(result.starts_with("2024-01-01"));
    }

    #[test]
    fn test_cached_model_info_serialization() {
        let info = CachedModelInfo {
            model_id: "test-model".into(),
            path: "/tmp/cache/test-model".into(),
            size_bytes: 4_500_000_000,
            pinned: true,
            last_accessed_secs: 1704067200,
            last_accessed: "2024-01-01T00:00:00Z".into(),
            cached_at_secs: 1704067200,
            cached_at: "2024-01-01T00:00:00Z".into(),
            file_count: 1,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("test-model"));
        assert!(json.contains("pinned"));
    }

    #[test]
    fn test_cache_stats_serialization() {
        let stats = CacheStats {
            total_size_bytes: 10_737_418_240,
            total_size_gb: 10.0,
            model_count: 3,
            pinned_count: 1,
            available_bytes: 100_000_000_000,
            available_gb: 93.13,
            max_size_bytes: 107_374_182_400,
            max_size_gb: 100.0,
            usage_percent: 10.0,
            cache_dir: "/tmp/xergon-cache".into(),
            eviction_threshold_percent: 80.0,
        };
        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("\"model_count\":3"));
        assert!(json.contains("\"pinned_count\":1"));
    }

    #[tokio::test]
    async fn test_model_cache_lru_order() {
        let config = ModelCacheConfig::default();
        let cache = ModelCache::new(config).unwrap();

        // Touch models in order
        cache.touch("model-a").await;
        cache.touch("model-b").await;
        cache.touch("model-c").await;

        let lru = cache.lru_order.read().await;
        assert_eq!(lru[0], "model-a"); // oldest
        assert_eq!(lru[2], "model-c"); // newest
    }

    #[tokio::test]
    async fn test_model_cache_touch_updates_lru() {
        let config = ModelCacheConfig::default();
        let cache = ModelCache::new(config).unwrap();

        cache.touch("model-a").await;
        cache.touch("model-b").await;
        cache.touch("model-a").await; // Touch again

        let lru = cache.lru_order.read().await;
        assert_eq!(lru[0], "model-b"); // oldest
        assert_eq!(lru[1], "model-a"); // newest
    }
}
