use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// CacheEntry
// ---------------------------------------------------------------------------

/// A single entry in the warm cache representing a pre-loaded model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub model_id: String,
    pub loaded_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
    pub access_count: u64,
    pub size_bytes: u64,
    pub gpu_id: Option<u32>,
    pub temperature: f64,
}

impl CacheEntry {
    /// Create a brand-new cache entry.
    fn new(model_id: String, size_bytes: u64, gpu_id: Option<u32>) -> Self {
        let now = Utc::now();
        Self {
            model_id,
            loaded_at: now,
            last_accessed: now,
            access_count: 0,
            size_bytes,
            gpu_id,
            temperature: 1.0,
        }
    }

    /// Record an access: bump counters and apply temperature dynamics.
    ///
    /// Temperature increases by 1.0 on each access.  A separate decay step
    /// (called on eviction ranking) reduces it by 0.1 per *prior* access so
    /// that frequently-accessed entries cool down slightly between hits,
    /// keeping the total bounded but still reflective of recency.
    fn record_access(&mut self) {
        self.access_count += 1;
        self.last_accessed = Utc::now();
        self.temperature += 1.0;
    }

    /// Apply one decay step (0.1 per previous access) for ranking purposes.
    fn decay_temperature(&self) -> f64 {
        self.temperature - (self.access_count as f64 * 0.1)
    }
}

// ---------------------------------------------------------------------------
// EvictionPolicy
// ---------------------------------------------------------------------------

/// Determines which entry to evict when the cache is full.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum EvictionPolicy {
    /// Evict the least-recently-used entry.
    LRU,
    /// Evict the least-frequently-used entry.
    LFU,
    /// Evict the entry with the lowest temperature score.
    Temperature,
}

impl Default for EvictionPolicy {
    fn default() -> Self {
        Self::LRU
    }
}

// ---------------------------------------------------------------------------
// WarmCacheConfig
// ---------------------------------------------------------------------------

/// Configuration for the model warm cache.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmCacheConfig {
    /// Maximum total byte size of cached models (default 10 GiB).
    pub max_size_bytes: u64,
    /// Default eviction policy.
    pub default_policy: EvictionPolicy,
    /// Maximum number of entries regardless of size (default 100).
    pub max_entries: u32,
    /// Whether to pre-warm configured models on startup (default true).
    pub prewarm_on_startup: bool,
    /// Time-to-live in seconds for entries (default 3600).
    pub ttl_secs: u64,
}

impl Default for WarmCacheConfig {
    fn default() -> Self {
        Self {
            max_size_bytes: 10 * 1024 * 1024 * 1024, // 10 GiB
            default_policy: EvictionPolicy::default(),
            max_entries: 100,
            prewarm_on_startup: true,
            ttl_secs: 3600,
        }
    }
}

// ---------------------------------------------------------------------------
// WarmCacheMetricsSnapshot
// ---------------------------------------------------------------------------

/// Point-in-time snapshot of cache metrics.
#[derive(Debug, Clone, Serialize)]
pub struct WarmCacheMetricsSnapshot {
    pub total_entries: u32,
    pub total_size_bytes: u64,
    pub hit_rate: f64,
    pub total_hits: u64,
    pub total_misses: u64,
    pub evictions: u64,
    pub gpu_distribution: HashMap<u32, u32>,
}

// ---------------------------------------------------------------------------
// ModelWarmCache
// ---------------------------------------------------------------------------

/// A concurrent, policy-driven cache for pre-loaded model weights.
///
/// Uses a `DashMap` so that readers never block each other and writers only
/// lock at the entry level.  Global configuration is protected by a
/// `std::sync::RwLock` because it is read far more often than written.
pub struct ModelWarmCache {
    entries: DashMap<String, CacheEntry>,
    config: std::sync::RwLock<WarmCacheConfig>,
    total_hits: AtomicU64,
    total_misses: AtomicU64,
    total_evictions: AtomicU64,
}

// ---- Construction ---------------------------------------------------------

impl ModelWarmCache {
    /// Create a new cache with the given configuration.
    pub fn new(config: WarmCacheConfig) -> Self {
        Self {
            entries: DashMap::new(),
            config: std::sync::RwLock::new(config),
            total_hits: AtomicU64::new(0),
            total_misses: AtomicU64::new(0),
            total_evictions: AtomicU64::new(0),
        }
    }

    /// Convenience constructor using the default configuration.
    pub fn with_defaults() -> Self {
        Self::new(WarmCacheConfig::default())
    }
}

// ---- Core operations ------------------------------------------------------

impl ModelWarmCache {
    /// Load a model into the warm cache.
    ///
    /// Returns an error if the model is already cached or if adding it would
    /// exceed either the entry-count or byte-size capacity.
    pub fn warm_model(
        &self,
        model_id: &str,
        size_bytes: u64,
        gpu_id: Option<u32>,
    ) -> Result<CacheEntry, String> {
        // Reject duplicates.
        if self.entries.contains_key(model_id) {
            return Err(format!("model '{}' is already in the cache", model_id));
        }

        let cfg = self.config.read().unwrap();

        // Check entry count.
        if self.entries.len() as u32 >= cfg.max_entries {
            return Err(format!(
                "cache full: {} entries (max {})",
                self.entries.len(),
                cfg.max_entries
            ));
        }

        // Check byte capacity.
        let current_size = self.get_total_size();
        if current_size + size_bytes > cfg.max_size_bytes {
            return Err(format!(
                "not enough capacity: need {} bytes, {} already used, limit {}",
                size_bytes, current_size, cfg.max_size_bytes
            ));
        }

        let entry = CacheEntry::new(model_id.to_string(), size_bytes, gpu_id);
        self.entries.insert(model_id.to_string(), entry.clone());

        Ok(entry)
    }

    /// Access a cached model, recording the hit and updating bookkeeping.
    ///
    /// Returns `None` if the model is not in the cache.
    pub fn access_model(&self, model_id: &str) -> Option<CacheEntry> {
        let mut entry_ref = self.entries.get_mut(model_id)?;
        entry_ref.record_access();
        self.total_hits.fetch_add(1, Ordering::Relaxed);
        Some(entry_ref.value().clone())
    }

    /// Remove a specific model from the cache.
    ///
    /// Returns `true` if the model was present and evicted.
    pub fn evict_model(&self, model_id: &str) -> bool {
        let removed = self.entries.remove(model_id).is_some();
        if removed {
            self.total_evictions.fetch_add(1, Ordering::Relaxed);
        }
        removed
    }

    /// Evict entries using the default policy until the total size is at or
    /// below `target_size_bytes`.  Returns the number of entries evicted.
    pub fn evict_to_target(&self, target_size_bytes: u64) -> u32 {
        let policy = {
            let cfg = self.config.read().unwrap();
            cfg.default_policy.clone()
        };

        let mut evicted: u32 = 0;
        while self.get_total_size() > target_size_bytes {
            match self.evict_one(policy.clone()) {
                Some(_) => evicted += 1,
                None => break, // nothing left to evict
            }
        }
        evicted
    }

    /// Evict a single entry according to the given policy.
    ///
    /// Returns the `model_id` of the evicted entry, or `None` if the cache is
    /// empty.
    pub fn evict_one(&self, policy: EvictionPolicy) -> Option<String> {
        if self.entries.is_empty() {
            return None;
        }

        let victim_id = match policy {
            EvictionPolicy::LRU => self.find_victim_lru(),
            EvictionPolicy::LFU => self.find_victim_lfu(),
            EvictionPolicy::Temperature => self.find_victim_temperature(),
        }?;

        self.evict_model(&victim_id);
        Some(victim_id)
    }

    // -- Victim selection helpers (scan-based; good enough for ≤100 entries) -

    fn find_victim_lru(&self) -> Option<String> {
        let mut oldest: Option<(DateTime<Utc>, String)> = None;
        for entry in self.entries.iter() {
            match &oldest {
                None => oldest = Some((entry.last_accessed, entry.model_id.clone())),
                Some((t, _)) if entry.last_accessed < *t => {
                    oldest = Some((entry.last_accessed, entry.model_id.clone()))
                }
                _ => {}
            }
        }
        oldest.map(|(_, id)| id)
    }

    fn find_victim_lfu(&self) -> Option<String> {
        let mut least: Option<(u64, String)> = None;
        for entry in self.entries.iter() {
            match &least {
                None => least = Some((entry.access_count, entry.model_id.clone())),
                Some((c, _)) if entry.access_count < *c => {
                    least = Some((entry.access_count, entry.model_id.clone()))
                }
                _ => {}
            }
        }
        least.map(|(_, id)| id)
    }

    fn find_victim_temperature(&self) -> Option<String> {
        let mut coldest: Option<(f64, String)> = None;
        for entry in self.entries.iter() {
            let temp = entry.decay_temperature();
            match &coldest {
                None => coldest = Some((temp, entry.model_id.clone())),
                Some((t, _)) if temp < *t => coldest = Some((temp, entry.model_id.clone())),
                _ => {}
            }
        }
        coldest.map(|(_, id)| id)
    }

    // -- Listing / lookup ----------------------------------------------------

    /// Return a snapshot of every entry currently in the cache.
    pub fn list_entries(&self) -> Vec<CacheEntry> {
        self.entries.iter().map(|r| r.value().clone()).collect()
    }

    /// Retrieve a single entry without recording an access.
    pub fn get_entry(&self, model_id: &str) -> Option<CacheEntry> {
        self.entries.get(model_id).map(|r| r.value().clone())
    }

    // -- Prewarming ----------------------------------------------------------

    /// Attempt to warm a batch of models.
    ///
    /// Returns `(success_count, fail_count)`.
    pub fn prewarm_models(&self, models: Vec<(&str, u64)>) -> (u32, u32) {
        let mut ok: u32 = 0;
        let mut fail: u32 = 0;
        for (id, size) in models {
            match self.warm_model(id, size, None) {
                Ok(_) => ok += 1,
                Err(_) => fail += 1,
            }
        }
        (ok, fail)
    }

    // -- Metrics -------------------------------------------------------------

    /// Compute a point-in-time metrics snapshot.
    pub fn get_metrics(&self) -> WarmCacheMetricsSnapshot {
        let total_entries = self.entries.len() as u32;
        let total_size_bytes = self.get_total_size();
        let total_hits = self.total_hits.load(Ordering::Relaxed);
        let total_misses = self.total_misses.load(Ordering::Relaxed);
        let evictions = self.total_evictions.load(Ordering::Relaxed);

        let total_requests = total_hits + total_misses;
        let hit_rate = if total_requests > 0 {
            total_hits as f64 / total_requests as f64
        } else {
            0.0
        };

        let mut gpu_distribution: HashMap<u32, u32> = HashMap::new();
        for entry in self.entries.iter() {
            if let Some(gpu) = entry.gpu_id {
                *gpu_distribution.entry(gpu).or_insert(0) += 1;
            }
        }

        WarmCacheMetricsSnapshot {
            total_entries,
            total_size_bytes,
            hit_rate,
            total_hits,
            total_misses,
            evictions,
            gpu_distribution,
        }
    }

    /// Return the current total size (bytes) of all cached models.
    pub fn get_total_size(&self) -> u64 {
        self.entries.iter().map(|r| r.size_bytes).sum()
    }

    // -- Configuration -------------------------------------------------------

    /// Replace the entire configuration atomically and return the old config.
    pub fn update_config(&self, config: WarmCacheConfig) -> WarmCacheConfig {
        let mut guard = self.config.write().unwrap();
        std::mem::replace(&mut *guard, config)
    }

    /// Return a clone of the current configuration.
    pub fn get_config(&self) -> WarmCacheConfig {
        self.config.read().unwrap().clone()
    }

    // -- TTL / expiration helpers (public for tests, useful internally) ------

    /// Check whether a given entry has expired based on the current TTL.
    ///
    /// Returns `true` when the entry is **expired**.
    pub fn is_entry_expired(&self, entry: &CacheEntry) -> bool {
        let cfg = self.config.read().unwrap();
        let ttl = chrono::Duration::seconds(cfg.ttl_secs as i64);
        Utc::now() - entry.loaded_at > ttl
    }

    /// Evict all entries whose TTL has elapsed.  Returns the count evicted.
    pub fn evict_expired(&self) -> u32 {
        let expired_ids: Vec<String> = self
            .entries
            .iter()
            .filter(|r| self.is_entry_expired(r.value()))
            .map(|r| r.key().clone())
            .collect();

        let count = expired_ids.len() as u32;
        for id in expired_ids {
            self.evict_model(&id);
        }
        count
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    // -- Helper --------------------------------------------------------------

    /// Create a tiny config suitable for unit tests.
    fn test_config(max_entries: u32, max_size_bytes: u64) -> WarmCacheConfig {
        WarmCacheConfig {
            max_size_bytes,
            default_policy: EvictionPolicy::LRU,
            max_entries,
            prewarm_on_startup: false,
            ttl_secs: 3600,
        }
    }

    // =========================================================================
    // 1. Warm / access / evict lifecycle
    // =========================================================================

    #[test]
    fn test_warm_access_evict_lifecycle() {
        let cache = ModelWarmCache::new(test_config(10, 1024 * 1024));

        // Warm a model.
        let entry = cache.warm_model("llama-7b", 1024, Some(0)).unwrap();
        assert_eq!(entry.model_id, "llama-7b");
        assert_eq!(entry.access_count, 0);
        assert_eq!(entry.temperature, 1.0);

        // Access it.
        let accessed = cache.access_model("llama-7b").unwrap();
        assert_eq!(accessed.access_count, 1);
        assert!((accessed.temperature - 2.0).abs() < f64::EPSILON);

        // Evict it.
        assert!(cache.evict_model("llama-7b"));
        assert!(cache.get_entry("llama-7b").is_none());
    }

    // =========================================================================
    // 2. LRU eviction
    // =========================================================================

    #[test]
    fn test_lru_eviction() {
        let cache = ModelWarmCache::new(test_config(10, 1024 * 1024));
        cache.warm_model("a", 100, None).unwrap();
        cache.warm_model("b", 100, None).unwrap();

        // Access "b" so "a" is LRU.
        cache.access_model("b").unwrap();

        let victim = cache.evict_one(EvictionPolicy::LRU).unwrap();
        assert_eq!(victim, "a");
        assert!(cache.get_entry("b").is_some());
    }

    // =========================================================================
    // 3. LFU eviction
    // =========================================================================

    #[test]
    fn test_lfu_eviction() {
        let cache = ModelWarmCache::new(test_config(10, 1024 * 1024));
        cache.warm_model("x", 100, None).unwrap();
        cache.warm_model("y", 100, None).unwrap();

        // Access "y" three times; "x" stays at 0.
        cache.access_model("y").unwrap();
        cache.access_model("y").unwrap();
        cache.access_model("y").unwrap();

        let victim = cache.evict_one(EvictionPolicy::LFU).unwrap();
        assert_eq!(victim, "x");
    }

    // =========================================================================
    // 4. Temperature eviction
    // =========================================================================

    #[test]
    fn test_temperature_eviction() {
        let cache = ModelWarmCache::new(test_config(10, 1024 * 1024));
        cache.warm_model("hot", 100, None).unwrap();
        cache.warm_model("cold", 100, None).unwrap();

        // Make "hot" hotter.
        for _ in 0..5 {
            cache.access_model("hot").unwrap();
        }

        let victim = cache.evict_one(EvictionPolicy::Temperature).unwrap();
        assert_eq!(victim, "cold");
    }

    // =========================================================================
    // 5. Capacity limits – entry count
    // =========================================================================

    #[test]
    fn test_capacity_entry_count() {
        let cache = ModelWarmCache::new(test_config(2, 1024 * 1024));
        cache.warm_model("m1", 100, None).unwrap();
        cache.warm_model("m2", 100, None).unwrap();
        assert!(cache.warm_model("m3", 100, None).is_err());
    }

    // =========================================================================
    // 6. Capacity limits – byte size
    // =========================================================================

    #[test]
    fn test_capacity_byte_size() {
        let cache = ModelWarmCache::new(test_config(100, 200));
        cache.warm_model("m1", 100, None).unwrap();
        cache.warm_model("m2", 101, None).unwrap_err();
    }

    // =========================================================================
    // 7. Duplicate warm
    // =========================================================================

    #[test]
    fn test_duplicate_warm() {
        let cache = ModelWarmCache::with_defaults();
        cache.warm_model("dup", 100, None).unwrap();
        let err = cache.warm_model("dup", 200, None).unwrap_err();
        assert!(err.contains("already in the cache"));
    }

    // =========================================================================
    // 8. Access non-existent model
    // =========================================================================

    #[test]
    fn test_access_nonexistent() {
        let cache = ModelWarmCache::with_defaults();
        assert!(cache.access_model("ghost").is_none());
    }

    // =========================================================================
    // 9. Evict non-existent model
    // =========================================================================

    #[test]
    fn test_evict_nonexistent() {
        let cache = ModelWarmCache::with_defaults();
        assert!(!cache.evict_model("ghost"));
    }

    // =========================================================================
    // 10. evict_one on empty cache
    // =========================================================================

    #[test]
    fn test_evict_one_empty() {
        let cache = ModelWarmCache::with_defaults();
        assert!(cache.evict_one(EvictionPolicy::LRU).is_none());
    }

    // =========================================================================
    // 11. Prewarming
    // =========================================================================

    #[test]
    fn test_prewarm_models() {
        let cache = ModelWarmCache::new(test_config(10, 1024 * 1024));
        let models = vec![("alpha", 100), ("beta", 200), ("gamma", 300)];
        let (ok, fail) = cache.prewarm_models(models);
        assert_eq!(ok, 3);
        assert_eq!(fail, 0);
        assert_eq!(cache.entries.len(), 3);
    }

    #[test]
    fn test_prewarm_partial_failure() {
        let cache = ModelWarmCache::new(test_config(2, 1024 * 1024));
        let models = vec![("a", 100), ("b", 100), ("c", 100)];
        let (ok, fail) = cache.prewarm_models(models);
        assert_eq!(ok, 2);
        assert_eq!(fail, 1);
    }

    // =========================================================================
    // 12. Metrics
    // =========================================================================

    #[test]
    fn test_metrics_empty() {
        let cache = ModelWarmCache::with_defaults();
        let m = cache.get_metrics();
        assert_eq!(m.total_entries, 0);
        assert_eq!(m.total_hits, 0);
        assert_eq!(m.total_misses, 0);
        assert!((m.hit_rate - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_metrics_hit_miss_rate() {
        let cache = ModelWarmCache::with_defaults();
        cache.warm_model("m", 100, None).unwrap();

        // 3 hits
        for _ in 0..3 {
            cache.access_model("m").unwrap();
        }
        // 1 miss
        let _ = cache.access_model("nope");

        let m = cache.get_metrics();
        assert_eq!(m.total_hits, 3);
        assert_eq!(m.total_misses, 1);
        assert!((m.hit_rate - 0.75).abs() < 1e-9);
    }

    #[test]
    fn test_metrics_gpu_distribution() {
        let cache = ModelWarmCache::with_defaults();
        cache.warm_model("a", 100, Some(0)).unwrap();
        cache.warm_model("b", 100, Some(0)).unwrap();
        cache.warm_model("c", 100, Some(1)).unwrap();

        let m = cache.get_metrics();
        assert_eq!(m.gpu_distribution.get(&0), Some(&2));
        assert_eq!(m.gpu_distribution.get(&1), Some(&1));
    }

    // =========================================================================
    // 13. Config updates
    // =========================================================================

    #[test]
    fn test_config_update() {
        let cache = ModelWarmCache::with_defaults();
        let original = cache.get_config();
        assert_eq!(original.max_entries, 100);

        let new_cfg = WarmCacheConfig {
            max_entries: 50,
            max_size_bytes: 1024,
            default_policy: EvictionPolicy::LFU,
            prewarm_on_startup: false,
            ttl_secs: 600,
        };
        let old = cache.update_config(new_cfg.clone());
        assert_eq!(old.max_entries, 100);
        assert_eq!(cache.get_config().max_entries, 50);
        assert_eq!(cache.get_config().default_policy, EvictionPolicy::LFU);
    }

    // =========================================================================
    // 14. TTL expiration check
    // =========================================================================

    #[test]
    fn test_ttl_not_expired() {
        let cache = ModelWarmCache::new(WarmCacheConfig {
            ttl_secs: 3600,
            ..WarmCacheConfig::default()
        });
        let entry = cache.warm_model("fresh", 100, None).unwrap();
        assert!(!cache.is_entry_expired(&entry));
    }

    #[test]
    fn test_ttl_expired() {
        let cache = ModelWarmCache::new(WarmCacheConfig {
            ttl_secs: 0, // immediately expired
            ..WarmCacheConfig::default()
        });
        let entry = cache.warm_model("stale", 100, None).unwrap();
        assert!(cache.is_entry_expired(&entry));
    }

    #[test]
    fn test_evict_expired() {
        let cache = ModelWarmCache::new(WarmCacheConfig {
            ttl_secs: 0,
            max_entries: 10,
            max_size_bytes: 1024 * 1024,
            default_policy: EvictionPolicy::LRU,
            prewarm_on_startup: false,
        });
        cache.warm_model("a", 100, None).unwrap();
        cache.warm_model("b", 100, None).unwrap();

        let evicted = cache.evict_expired();
        assert_eq!(evicted, 2);
        assert_eq!(cache.entries.len(), 0);
    }

    // =========================================================================
    // 15. evict_to_target
    // =========================================================================

    #[test]
    fn test_evict_to_target() {
        let cache = ModelWarmCache::new(WarmCacheConfig {
            max_entries: 10,
            max_size_bytes: 1000,
            default_policy: EvictionPolicy::LRU,
            prewarm_on_startup: false,
            ttl_secs: 3600,
        });
        cache.warm_model("a", 300, None).unwrap();
        cache.warm_model("b", 300, None).unwrap();
        cache.warm_model("c", 300, None).unwrap();
        assert_eq!(cache.get_total_size(), 900);

        // Evict until ≤ 400 bytes.
        let count = cache.evict_to_target(400);
        assert!(count >= 2); // need to evict at least 2 entries
        assert!(cache.get_total_size() <= 400);
    }

    // =========================================================================
    // 16. list_entries returns clones
    // =========================================================================

    #[test]
    fn test_list_entries() {
        let cache = ModelWarmCache::with_defaults();
        cache.warm_model("x", 10, None).unwrap();
        cache.warm_model("y", 20, Some(1)).unwrap();

        let list = cache.list_entries();
        assert_eq!(list.len(), 2);
        let ids: Vec<&str> = list.iter().map(|e| e.model_id.as_str()).collect();
        assert!(ids.contains(&"x"));
        assert!(ids.contains(&"y"));
    }

    // =========================================================================
    // 17. get_entry without access recording
    // =========================================================================

    #[test]
    fn test_get_entry_no_side_effects() {
        let cache = ModelWarmCache::with_defaults();
        cache.warm_model("sneaky", 50, None).unwrap();

        let _ = cache.get_entry("sneaky");
        let _ = cache.get_entry("sneaky");

        let m = cache.get_metrics();
        assert_eq!(m.total_hits, 0); // no hits recorded
        assert_eq!(m.total_misses, 0);

        let entry = cache.get_entry("sneaky").unwrap();
        assert_eq!(entry.access_count, 0); // untouched
    }

    // =========================================================================
    // 18. Concurrent access stress test
    // =========================================================================

    #[test]
    fn test_concurrent_access() {
        let cache = Arc::new(ModelWarmCache::new(test_config(20, 1024 * 1024)));
        cache.warm_model("shared", 100, None).unwrap();

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let c = Arc::clone(&cache);
                thread::spawn(move || {
                    for _ in 0..100 {
                        let _ = c.access_model("shared");
                        let _ = c.access_model("nonexistent");
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        let m = cache.get_metrics();
        assert_eq!(m.total_hits, 800);
        assert_eq!(m.total_misses, 800);
    }

    // =========================================================================
    // 19. EvictionPolicy Default
    // =========================================================================

    #[test]
    fn test_eviction_policy_default() {
        assert_eq!(EvictionPolicy::default(), EvictionPolicy::LRU);
    }

    // =========================================================================
    // 20. WarmCacheConfig Default
    // =========================================================================

    #[test]
    fn test_config_default_values() {
        let cfg = WarmCacheConfig::default();
        assert_eq!(cfg.max_entries, 100);
        assert_eq!(cfg.max_size_bytes, 10 * 1024 * 1024 * 1024);
        assert_eq!(cfg.default_policy, EvictionPolicy::LRU);
        assert!(cfg.prewarm_on_startup);
        assert_eq!(cfg.ttl_secs, 3600);
    }

    // =========================================================================
    // 21. Temperature decay computation
    // =========================================================================

    #[test]
    fn test_temperature_decay() {
        let cache = ModelWarmCache::with_defaults();
        let entry = cache.warm_model("decay-test", 100, None).unwrap();
        // Initial temperature = 1.0, access_count = 0 → decayed = 1.0
        assert!((entry.decay_temperature() - 1.0).abs() < f64::EPSILON);

        // After one access: temp = 2.0, count = 1 → decayed = 2.0 - 0.1 = 1.9
        cache.access_model("decay-test").unwrap();
        let entry = cache.get_entry("decay-test").unwrap();
        assert!((entry.decay_temperature() - 1.9).abs() < 1e-9);
    }

    // =========================================================================
    // 22. get_total_size
    // =========================================================================

    #[test]
    fn test_get_total_size() {
        let cache = ModelWarmCache::with_defaults();
        assert_eq!(cache.get_total_size(), 0);
        cache.warm_model("a", 111, None).unwrap();
        cache.warm_model("b", 222, None).unwrap();
        assert_eq!(cache.get_total_size(), 333);
        cache.evict_model("a");
        assert_eq!(cache.get_total_size(), 222);
    }

    // =========================================================================
    // 23. GPU id stored correctly
    // =========================================================================

    #[test]
    fn test_gpu_id_stored() {
        let cache = ModelWarmCache::with_defaults();
        let entry = cache.warm_model("gpu-model", 500, Some(7)).unwrap();
        assert_eq!(entry.gpu_id, Some(7));

        let fetched = cache.get_entry("gpu-model").unwrap();
        assert_eq!(fetched.gpu_id, Some(7));
    }

    // =========================================================================
    // 24. Eviction counter
    // =========================================================================

    #[test]
    fn test_eviction_counter() {
        let cache = ModelWarmCache::with_defaults();
        cache.warm_model("e1", 100, None).unwrap();
        cache.warm_model("e2", 100, None).unwrap();
        cache.warm_model("e3", 100, None).unwrap();

        cache.evict_model("e1");
        cache.evict_model("e2");

        let m = cache.get_metrics();
        assert_eq!(m.evictions, 2);

        // Evicting a non-existent model must NOT increment the counter.
        cache.evict_model("ghost");
        let m = cache.get_metrics();
        assert_eq!(m.evictions, 2);
    }

    // =========================================================================
    // 25. with_defaults constructor
    // =========================================================================

    #[test]
    fn test_with_defaults_constructor() {
        let cache = ModelWarmCache::with_defaults();
        let cfg = cache.get_config();
        assert_eq!(cfg.max_entries, 100);
        assert_eq!(cfg.default_policy, EvictionPolicy::LRU);
    }
}
