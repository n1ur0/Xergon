//! Inference response caching with TTL-based expiration and LRU eviction.
//!
//! Caches inference responses keyed by SHA-256(model + normalized_prompt) to avoid
//! redundant compute for identical requests. Features:
//! - TTL-based expiration (configurable, default 5 min)
//! - LRU eviction when max_entries reached
//! - Size limit per entry (reject caching of huge responses)
//! - Cache bypass via X-Cache-Bypass header
//! - Cache stats: hits, misses, hit rate, evictions
//! - Selective caching: only successful responses (no errors)
//!
//! API endpoints:
//! - GET    /api/inference-cache/stats       -- cache statistics
//! - DELETE /api/inference-cache/clear       -- clear all cached entries
//! - GET    /api/inference-cache/entries     -- list cached entries (paginated)
//! - DELETE /api/inference-cache/entries/{id} -- evict specific entry
//! - PATCH  /api/inference-cache/config      -- update cache config

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{debug, info};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the inference cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceCacheConfig {
    /// Whether caching is enabled.
    pub enabled: bool,
    /// Maximum number of cached entries.
    pub max_entries: usize,
    /// Time-to-live for cached entries.
    pub ttl_secs: u64,
    /// Maximum size in bytes for a single cached response.
    pub max_entry_size: usize,
    /// Semantic similarity threshold for cache hits (0.0-1.0).
    /// Currently only exact match is supported; this is reserved for future
    /// embeddings-based similarity.
    pub similarity_threshold: f64,
}

impl Default for InferenceCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_entries: 10_000,
            ttl_secs: 300, // 5 minutes
            max_entry_size: 1_048_576, // 1 MB
            similarity_threshold: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A cached inference response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedInference {
    /// Unique entry identifier.
    pub id: String,
    /// Model name used for the inference.
    pub model: String,
    /// SHA-256 hash of the normalized prompt + model.
    pub prompt_hash: String,
    /// The inference response text.
    pub response: String,
    /// Number of tokens consumed.
    pub tokens_used: u32,
    /// When this entry was created.
    pub created_at: std::time::SystemTime,
    /// How many times this cache entry has been hit.
    pub hit_count: u64,
    /// Size of the cached response in bytes.
    pub size_bytes: usize,
}

/// Cache statistics (lock-free counters).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CacheStats {
    pub hits: AtomicU64,
    pub misses: AtomicU64,
    pub evictions: AtomicU64,
    pub size_bytes: AtomicU64,
}

impl CacheStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn hit_rate(&self) -> f64 {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total = hits + misses;
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }

    pub fn snapshot(&self) -> CacheStatsSnapshot {
        CacheStatsSnapshot {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            evictions: self.evictions.load(Ordering::Relaxed),
            size_bytes: self.size_bytes.load(Ordering::Relaxed),
            hit_rate: self.hit_rate(),
        }
    }
}

/// A point-in-time snapshot of cache statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStatsSnapshot {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub size_bytes: u64,
    pub hit_rate: f64,
}

/// Serialized view of a cached entry for API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedEntryView {
    pub id: String,
    pub model: String,
    pub prompt_hash: String,
    pub tokens_used: u32,
    pub hit_count: u64,
    pub size_bytes: usize,
    pub created_at: String,
}

/// Paginated list of cached entries.
#[derive(Debug, Serialize, Deserialize)]
pub struct CachedEntriesResponse {
    pub entries: Vec<CachedEntryView>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
}

/// Request body for PATCH /api/inference-cache/config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCacheConfigRequest {
    pub enabled: Option<bool>,
    pub max_entries: Option<usize>,
    pub ttl_secs: Option<u64>,
    pub max_entry_size: Option<usize>,
    pub similarity_threshold: Option<f64>,
}

// ---------------------------------------------------------------------------
// Inference Cache
// ---------------------------------------------------------------------------

/// Thread-safe inference response cache with TTL and LRU eviction.
pub struct InferenceCache {
    /// Cache configuration (wrapped in RwLock for dynamic updates).
    config: tokio::sync::RwLock<InferenceCacheConfig>,
    /// Cache entries keyed by prompt_hash.
    entries: DashMap<String, CachedInference>,
    /// Cache statistics (lock-free).
    stats: CacheStats,
}

impl InferenceCache {
    /// Create a new inference cache with the given configuration.
    pub fn new(config: InferenceCacheConfig) -> Self {
        info!(
            enabled = config.enabled,
            max_entries = config.max_entries,
            ttl_secs = config.ttl_secs,
            max_entry_size = config.max_entry_size,
            "Inference cache initialized"
        );
        Self {
            config: tokio::sync::RwLock::new(config),
            entries: DashMap::new(),
            stats: CacheStats::new(),
        }
    }

    /// Create a new inference cache with default configuration.
    pub fn new_default() -> Self {
        Self::new(InferenceCacheConfig::default())
    }

    /// Compute SHA-256 hash of model + normalized prompt.
    pub fn compute_hash(model: &str, prompt: &str) -> String {
        let normalized = prompt.trim();
        let mut hasher = Sha256::new();
        hasher.update(model.as_bytes());
        hasher.update(b":");
        hasher.update(normalized.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Look up a cached inference response.
    /// Returns None on cache miss, or if the entry has expired, or if caching is disabled.
    pub fn get(&self, model: &str, prompt: &str) -> Option<CachedInference> {
        let config = self.config.blocking_read();
        if !config.enabled {
            return None;
        }
        let key = Self::compute_hash(model, prompt);
        drop(config);

        if let Some(mut entry) = self.entries.get_mut(&key) {
            let ttl = Duration::from_secs(self.config.blocking_read().ttl_secs);
            let age = entry.created_at.elapsed().unwrap_or(Duration::ZERO);
            if age > ttl {
                // Expired -- evict
                self.stats.evictions.fetch_add(1, Ordering::Relaxed);
                self.stats
                    .size_bytes
                    .fetch_sub(entry.size_bytes as u64, Ordering::Relaxed);
                drop(entry);
                self.entries.remove(&key);
                self.stats.misses.fetch_add(1, Ordering::Relaxed);
                return None;
            }
            entry.hit_count += 1;
            let cached = entry.clone();
            self.stats.hits.fetch_add(1, Ordering::Relaxed);
            debug!(key = %key, hit_count = cached.hit_count, "Cache hit");
            Some(cached)
        } else {
            self.stats.misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// Insert an inference response into the cache.
    /// Returns false if the entry was rejected (too large or caching disabled).
    /// Only successful responses should be cached.
    pub fn insert(
        &self,
        model: &str,
        prompt: &str,
        response: &str,
        tokens_used: u32,
    ) -> bool {
        let config = self.config.blocking_read();
        if !config.enabled {
            return false;
        }

        let size_bytes = response.len();
        if size_bytes > config.max_entry_size {
            debug!(
                size_bytes,
                max_entry_size = config.max_entry_size,
                "Skipping cache insert: response too large"
            );
            return false;
        }

        let key = Self::compute_hash(model, prompt);
        let id = uuid::Uuid::new_v4().to_string();
        let now = std::time::SystemTime::now();

        // Check if we need to evict (LRU: remove oldest entries until under limit)
        let current_count = self.entries.len();
        if current_count >= config.max_entries && !self.entries.contains_key(&key) {
            let to_remove = current_count - config.max_entries + 1;
            self.evict_oldest(to_remove);
        }

        let entry = CachedInference {
            id: id.clone(),
            model: model.to_string(),
            prompt_hash: key.clone(),
            response: response.to_string(),
            tokens_used,
            created_at: now,
            hit_count: 0,
            size_bytes,
        };

        // Update size tracking
        if let Some(old) = self.entries.insert(key.clone(), entry) {
            self.stats
                .size_bytes
                .fetch_sub(old.size_bytes as u64, Ordering::Relaxed);
        }
        self.stats
            .size_bytes
            .fetch_add(size_bytes as u64, Ordering::Relaxed);

        debug!(key = %key, size_bytes, "Cache entry inserted");
        true
    }

    /// Remove a specific entry by ID.
    /// Returns true if the entry was found and removed.
    pub fn evict(&self, id: &str) -> bool {
        let mut removed = None;
        self.entries.retain(|_, v| {
            if v.id == id && removed.is_none() {
                removed = Some(v.size_bytes as u64);
                false
            } else {
                true
            }
        });
        if let Some(size) = removed {
            self.stats.size_bytes.fetch_sub(size, Ordering::Relaxed);
            self.stats.evictions.fetch_add(1, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// Clear all cached entries.
    pub fn clear(&self) -> usize {
        let count = self.entries.len();
        self.entries.clear();
        self.stats.size_bytes.store(0, Ordering::Relaxed);
        info!(evicted = count, "Cache cleared");
        count
    }

    /// Get current cache statistics.
    pub fn stats(&self) -> CacheStatsSnapshot {
        self.stats.snapshot()
    }

    /// Get current configuration.
    pub async fn get_config(&self) -> InferenceCacheConfig {
        self.config.read().await.clone()
    }

    /// Update cache configuration.
    pub async fn update_config(&self, update: UpdateCacheConfigRequest) -> InferenceCacheConfig {
        let mut config = self.config.write().await;
        if let Some(enabled) = update.enabled {
            config.enabled = enabled;
        }
        if let Some(max_entries) = update.max_entries {
            config.max_entries = max_entries;
        }
        if let Some(ttl_secs) = update.ttl_secs {
            config.ttl_secs = ttl_secs;
        }
        if let Some(max_entry_size) = update.max_entry_size {
            config.max_entry_size = max_entry_size;
        }
        if let Some(similarity_threshold) = update.similarity_threshold {
            config.similarity_threshold = similarity_threshold.clamp(0.0, 1.0);
        }
        info!(
            enabled = config.enabled,
            max_entries = config.max_entries,
            ttl_secs = config.ttl_secs,
            "Cache config updated"
        );
        config.clone()
    }

    /// List cached entries with pagination.
    pub fn list_entries(&self, offset: usize, limit: usize) -> CachedEntriesResponse {
        let mut entries: Vec<CachedEntryView> = self
            .entries
            .iter()
            .map(|e| {
                let v = e.value();
                CachedEntryView {
                    id: v.id.clone(),
                    model: v.model.clone(),
                    prompt_hash: v.prompt_hash.clone(),
                    tokens_used: v.tokens_used,
                    hit_count: v.hit_count,
                    size_bytes: v.size_bytes,
                    created_at: v
                        .created_at
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                        .to_string(),
                }
            })
            .collect();

        // Sort by created_at descending (most recent first)
        entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        let total = entries.len();
        let entries = entries.into_iter().skip(offset).take(limit).collect();

        CachedEntriesResponse {
            entries,
            total,
            offset,
            limit,
        }
    }

    /// Get the number of currently cached entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Run periodic TTL cleanup. Should be called from a background task.
    pub fn cleanup_expired(&self) -> usize {
        let ttl = Duration::from_secs(self.config.blocking_read().ttl_secs);
        let now = std::time::SystemTime::now();
        let mut evicted = 0usize;

        self.entries.retain(|key, entry| {
            let age = now.duration_since(entry.created_at).unwrap_or(Duration::ZERO);
            if age > ttl {
                self.stats.evictions.fetch_add(1, Ordering::Relaxed);
                self.stats
                    .size_bytes
                    .fetch_sub(entry.size_bytes as u64, Ordering::Relaxed);
                evicted += 1;
                debug!(key = %key, "Evicted expired cache entry");
                false
            } else {
                true
            }
        });

        if evicted > 0 {
            debug!(evicted, "TTL cleanup: evicted expired entries");
        }
        evicted
    }

    /// Evict the oldest N entries (LRU eviction).
    fn evict_oldest(&self, count: usize) {
        let mut entries: Vec<(String, std::time::SystemTime)> = self
            .entries
            .iter()
            .map(|e| (e.key().clone(), e.value().created_at))
            .collect();

        // Sort by creation time ascending (oldest first)
        entries.sort_by_key(|e| e.1);

        for (key, _) in entries.into_iter().take(count) {
            if let Some((_, removed)) = self.entries.remove(&key) {
                self.stats.evictions.fetch_add(1, Ordering::Relaxed);
                self.stats
                    .size_bytes
                    .fetch_sub(removed.size_bytes as u64, Ordering::Relaxed);
                debug!(key = %key, "LRU evicted cache entry");
            }
        }
    }

    /// Spawn a background task that periodically cleans up expired entries.
    pub fn spawn_cleanup_task(cache: Arc<Self>, interval_secs: u64) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
            loop {
                interval.tick().await;
                cache.cleanup_expired();
            }
        });
    }
}

use std::sync::Arc;
