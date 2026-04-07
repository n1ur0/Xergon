//! Model Warm-Up Pools
//!
//! Keeps inference models pre-loaded in GPU memory so that the first real
//! request does not pay a cold-start penalty.
//!
//! Features:
//! - Preload models on startup from config
//! - Warm-up: send a small inference request to load model into GPU memory
//! - Keep-warm: periodically ping models to prevent OS/GPU driver from evicting
//! - Idle eviction: unload models not used within `idle_evict_timeout`
//! - VRAM-aware: check available VRAM before warming, skip if insufficient
//! - Priority: warm high-priority models first when resources are limited
//! - Manual: explicit load/unload via API
//!
//! Eviction strategies:
//! - **LRU**: evict least recently used
//! - **LFU**: evict least frequently used
//! - **Priority**: evict lowest priority model
//! - **Manual**: only explicit load/unload (no auto-eviction)
//!
//! API endpoints:
//! - GET  /api/warmup/status  -- pool status, all entries
//! - POST /api/warmup/load    -- warm a specific model
//! - POST /api/warmup/unload  -- unload a model
//! - PATCH /api/warmup/config -- update warmup config
//! - GET  /api/warmup/stats   -- pool statistics (VRAM used, hit rate, evictions)

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Eviction strategy for the warm-up pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WarmupStrategy {
    /// Evict least recently used model.
    LRU,
    /// Evict least frequently used model.
    LFU,
    /// Evict lowest priority model.
    Priority,
    /// Only explicit load/unload, no auto-eviction.
    Manual,
}

impl Default for WarmupStrategy {
    fn default() -> Self {
        WarmupStrategy::LRU
    }
}

/// Status of a single model in the warm-up pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WarmupStatus {
    /// Currently being warmed up (inference request in-flight).
    Warming,
    /// Model is warm and ready for fast inference.
    Warm,
    /// Model is being evicted / unloaded.
    Evicting,
    /// Model has been unloaded from the pool.
    Unloaded,
}

impl std::fmt::Display for WarmupStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WarmupStatus::Warming => write!(f, "warming"),
            WarmupStatus::Warm => write!(f, "warm"),
            WarmupStatus::Evicting => write!(f, "evicting"),
            WarmupStatus::Unloaded => write!(f, "unloaded"),
        }
    }
}

/// A single entry in the warm-up pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmupEntry {
    /// Model identifier (e.g. "llama3:8b").
    pub model_id: String,
    /// When the model was loaded into the pool.
    pub loaded_at: DateTime<Utc>,
    /// When the model was last accessed (read or warmup ping).
    pub last_used: DateTime<Utc>,
    /// Total number of warm-up pings / keep-alive pings sent.
    pub request_count: u64,
    /// Current warm-up status.
    pub status: WarmupStatus,
    /// Estimated VRAM reserved for this model (bytes).
    pub vram_reserved: u64,
    /// Optional priority (higher = more important). Used by Priority strategy.
    pub priority: i32,
}

/// Configuration for the warm-up pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmupConfig {
    /// Whether the warm-up pool is enabled.
    pub enabled: bool,
    /// Maximum number of models to keep warm simultaneously.
    pub pool_size: usize,
    /// Models to preload on startup.
    pub preload_on_startup: Vec<String>,
    /// Maximum time to wait for a model to warm up.
    #[serde(with = "humantime_serde")]
    pub warmup_timeout: Duration,
    /// Evict a model if it has been idle for this long.
    #[serde(with = "humantime_serde")]
    pub idle_evict_timeout: Duration,
    /// Eviction strategy.
    pub strategy: WarmupStrategy,
    /// How often to send keep-warm pings to warm models.
    #[serde(with = "humantime_serde")]
    pub keep_warm_interval: Duration,
    /// How often to check for idle models to evict.
    #[serde(with = "humantime_serde")]
    pub eviction_check_interval: Duration,
    /// Inference backend URL (e.g. http://127.0.0.1:11434).
    pub backend_url: String,
}

impl Default for WarmupConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            pool_size: 3,
            preload_on_startup: Vec::new(),
            warmup_timeout: Duration::from_secs(300),
            idle_evict_timeout: Duration::from_secs(1800), // 30 min
            strategy: WarmupStrategy::LRU,
            keep_warm_interval: Duration::from_secs(120),  // 2 min
            eviction_check_interval: Duration::from_secs(60), // 1 min
            backend_url: "http://127.0.0.1:11434".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// API request / response types
// ---------------------------------------------------------------------------

/// Request body for POST /api/warmup/load
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoadModelRequest {
    pub model_id: String,
    /// Estimated VRAM in bytes (0 = auto-detect / unknown).
    #[serde(default)]
    pub vram_reserved: u64,
    /// Priority (higher = more important). Default 0.
    #[serde(default)]
    pub priority: i32,
}

/// Request body for POST /api/warmup/unload
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UnloadModelRequest {
    pub model_id: String,
}

/// Response for POST /api/warmup/load
#[derive(Debug, Clone, Serialize)]
pub struct LoadModelResponse {
    pub model_id: String,
    pub status: WarmupStatus,
    pub message: String,
}

/// Response for POST /api/warmup/unload
#[derive(Debug, Clone, Serialize)]
pub struct UnloadModelResponse {
    pub model_id: String,
    pub evicted: bool,
    pub message: String,
}

/// Full pool status returned by GET /api/warmup/status
#[derive(Debug, Clone, Serialize)]
pub struct WarmupPoolStatus {
    pub enabled: bool,
    pub strategy: String,
    pub pool_size: usize,
    pub current_count: usize,
    pub total_vram_reserved: u64,
    pub entries: Vec<WarmupEntry>,
}

/// Pool statistics returned by GET /api/warmup/stats
#[derive(Debug, Clone, Serialize)]
pub struct WarmupPoolStats {
    pub total_warmups: u64,
    pub total_evictions: u64,
    pub total_vram_reserved: u64,
    pub warm_count: usize,
    pub warming_count: usize,
    pub evicting_count: usize,
    pub hit_rate: f64,
    /// Total requests that hit a warm model.
    pub hits: u64,
    /// Total requests that missed (model not warm).
    pub misses: u64,
}

/// Request body for PATCH /api/warmup/config
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateWarmupConfigRequest {
    pub enabled: Option<bool>,
    pub pool_size: Option<usize>,
    pub preload_on_startup: Option<Vec<String>>,
    pub warmup_timeout_secs: Option<u64>,
    pub idle_evict_timeout_secs: Option<u64>,
    pub strategy: Option<WarmupStrategy>,
    pub keep_warm_interval_secs: Option<u64>,
    pub eviction_check_interval_secs: Option<u64>,
}

// ---------------------------------------------------------------------------
// WarmupPool
// ---------------------------------------------------------------------------

/// Thread-safe warm-up pool that keeps inference models pre-loaded.
pub struct WarmupPool {
    /// Mutable configuration (can be updated via API).
    config: Arc<RwLock<WarmupConfig>>,
    /// Pool entries keyed by model_id.
    entries: DashMap<String, WarmupEntry>,
    /// Total VRAM currently reserved across all entries.
    total_vram_reserved: AtomicU64,
    /// HTTP client for sending warm-up / keep-warm inference requests.
    http_client: Client,
    /// Counter: total successful warm-ups completed.
    total_warmups: AtomicU64,
    /// Counter: total evictions performed.
    total_evictions: AtomicU64,
    /// Counter: requests that hit a warm model.
    hits: AtomicU64,
    /// Counter: requests that missed (model not in pool or not warm).
    misses: AtomicU64,
    /// Background task running flag.
    running: AtomicBool,
}

impl WarmupPool {
    /// Create a new warm-up pool with the given configuration.
    pub fn new(config: WarmupConfig) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            entries: DashMap::new(),
            total_vram_reserved: AtomicU64::new(0),
            http_client: Client::new(),
            total_warmups: AtomicU64::new(0),
            total_evictions: AtomicU64::new(0),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            running: AtomicBool::new(false),
        }
    }

    /// Start the background tasks (keep-warm pings + idle eviction).
    ///
    /// Also preloads any models listed in `config.preload_on_startup`.
    /// Call `stop()` to terminate background tasks.
    pub async fn start(self: &Arc<Self>) {
        if self.running.swap(true, Ordering::Relaxed) {
            warn!("Warmup pool is already running");
            return;
        }

        let config = self.config.read().await;
        let enabled = config.enabled;
        let preload = config.preload_on_startup.clone();
        let keep_warm_interval = config.keep_warm_interval;
        let eviction_check_interval = config.eviction_check_interval;
        drop(config);

        info!("Warmup pool started (enabled={})", enabled);

        // Preload models on startup
        if enabled {
            for model_id in &preload {
                info!(model = %model_id, "Preloading model on startup");
                if let Err(e) = self.warm_model(model_id, 0, 0).await {
                    warn!(model = %model_id, error = %e, "Failed to preload model on startup");
                }
            }
        }

        let pool = Arc::clone(self);
        tokio::spawn(async move {
            let mut keep_warm = tokio::time::interval(keep_warm_interval);
            let mut eviction_check = tokio::time::interval(eviction_check_interval);

            loop {
                if !pool.running.load(Ordering::Relaxed) {
                    break;
                }

                tokio::select! {
                    _ = keep_warm.tick() => {
                        pool.keep_warm_pass().await;
                    }
                    _ = eviction_check.tick() => {
                        pool.eviction_pass().await;
                    }
                }
            }
            info!("Warmup pool background tasks stopped");
        });
    }

    /// Stop background tasks.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
        info!("Warmup pool stop requested");
    }

    /// Check if the background loop is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Warm up a specific model by sending a small inference request.
    ///
    /// If `vram_reserved` is 0, a default estimate of 4 GiB is used.
    /// Returns the model's status after the warm-up attempt.
    pub async fn warm_model(
        &self,
        model_id: &str,
        vram_reserved: u64,
        priority: i32,
    ) -> Result<WarmupStatus, String> {
        let config = self.config.read().await;
        if !config.enabled {
            return Err("Warmup pool is disabled".to_string());
        }

        // Check pool capacity
        let warm_count = self
            .entries
            .iter()
            .filter(|e| e.value().status == WarmupStatus::Warm || e.value().status == WarmupStatus::Warming)
            .count();

        if warm_count >= config.pool_size {
            // Try to evict a model first
            if config.strategy != WarmupStrategy::Manual {
                let evicted = self.evict_one(&config.strategy, &config.backend_url).await;
                if !evicted {
                    return Err(format!(
                        "Pool is full ({} models) and no model could be evicted",
                        config.pool_size
                    ));
                }
            } else {
                return Err(format!(
                    "Pool is full ({} models) and strategy is Manual — unload a model first",
                    config.pool_size
                ));
            }
        }
        drop(config);

        // Estimate VRAM if not provided
        let vram_estimate = if vram_reserved > 0 {
            vram_reserved
        } else {
            4 * 1024 * 1024 * 1024 // default 4 GiB
        };

        // Mark as warming
        {
            let mut entry = self.entries.entry(model_id.to_string()).or_insert_with(|| WarmupEntry {
                model_id: model_id.to_string(),
                loaded_at: Utc::now(),
                last_used: Utc::now(),
                request_count: 0,
                status: WarmupStatus::Warming,
                vram_reserved: 0,
                priority,
            });
            entry.status = WarmupStatus::Warming;
        }

        // Send a small inference request to warm the model
        let config = self.config.read().await;
        let backend_url = config.backend_url.trim_end_matches('/').to_string();
        let timeout = config.warmup_timeout;
        drop(config);

        let probe_url = format!("{}/v1/chat/completions", backend_url);
        let probe_body = serde_json::json!({
            "model": model_id,
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 1,
            "stream": false,
        });

        let result = self
            .http_client
            .post(&probe_url)
            .timeout(timeout)
            .header("Content-Type", "application/json")
            .json(&probe_body)
            .send()
            .await;

        match result {
            Ok(resp) if resp.status().is_success() => {
                // Model is warm — update entry
                if let Some(mut entry) = self.entries.get_mut(model_id) {
                    entry.status = WarmupStatus::Warm;
                    entry.loaded_at = Utc::now();
                    entry.last_used = Utc::now();
                    entry.request_count += 1;
                    // Only add vram if newly reserved (was 0 or different)
                    if entry.vram_reserved == 0 {
                        entry.vram_reserved = vram_estimate;
                        self.total_vram_reserved.fetch_add(vram_estimate, Ordering::Relaxed);
                    }
                } else {
                    // Entry was removed while we were warming — reinsert
                    self.entries.insert(model_id.to_string(), WarmupEntry {
                        model_id: model_id.to_string(),
                        loaded_at: Utc::now(),
                        last_used: Utc::now(),
                        request_count: 1,
                        status: WarmupStatus::Warm,
                        vram_reserved: vram_estimate,
                        priority,
                    });
                    self.total_vram_reserved.fetch_add(vram_estimate, Ordering::Relaxed);
                }

                self.total_warmups.fetch_add(1, Ordering::Relaxed);
                info!(model = %model_id, "Model warmed up successfully");
                Ok(WarmupStatus::Warm)
            }
            Ok(resp) => {
                let status = resp.status();
                let err_msg = format!("Backend returned HTTP {}", status);
                // Mark as unloaded on failure
                if let Some(mut entry) = self.entries.get_mut(model_id) {
                    entry.status = WarmupStatus::Unloaded;
                }
                self.misses.fetch_add(1, Ordering::Relaxed);
                Err(err_msg)
            }
            Err(e) => {
                let err_msg = format!("Warm-up request failed: {}", e);
                if let Some(mut entry) = self.entries.get_mut(model_id) {
                    entry.status = WarmupStatus::Unloaded;
                }
                self.misses.fetch_add(1, Ordering::Relaxed);
                Err(err_msg)
            }
        }
    }

    /// Unload (evict) a specific model from the pool.
    ///
    /// Returns true if the model was found and evicted.
    pub fn unload_model(&self, model_id: &str) -> bool {
        if let Some((_, entry)) = self.entries.remove(model_id) {
            self.total_vram_reserved
                .fetch_sub(entry.vram_reserved, Ordering::Relaxed);
            self.total_evictions.fetch_add(1, Ordering::Relaxed);
            info!(model = %model_id, "Model unloaded from warm-up pool");
            true
        } else {
            debug!(model = %model_id, "Model not found in warm-up pool");
            false
        }
    }

    /// Record that a real inference request hit a warm model.
    /// Called by the inference path to update `last_used`.
    pub fn record_hit(&self, model_id: &str) {
        if let Some(mut entry) = self.entries.get_mut(model_id) {
            if entry.status == WarmupStatus::Warm {
                entry.last_used = Utc::now();
                self.hits.fetch_add(1, Ordering::Relaxed);
            }
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record that a request missed the warm pool (model not warm).
    pub fn record_miss(&self) {
        self.misses.fetch_add(1, Ordering::Relaxed);
    }

    // -- Background passes --------------------------------------------------

    /// Keep-warm pass: ping all warm models to prevent OS/GPU driver eviction.
    async fn keep_warm_pass(&self) {
        let config = self.config.read().await;
        if !config.enabled {
            return;
        }
        let backend_url = config.backend_url.trim_end_matches('/').to_string();
        let timeout = config.warmup_timeout;
        drop(config);

        let model_ids: Vec<String> = self
            .entries
            .iter()
            .filter(|e| e.value().status == WarmupStatus::Warm)
            .map(|e| e.key().clone())
            .collect();

        for model_id in &model_ids {
            let probe_url = format!("{}/v1/chat/completions", backend_url);
            let probe_body = serde_json::json!({
                "model": model_id,
                "messages": [{"role": "user", "content": "ping"}],
                "max_tokens": 1,
                "stream": false,
            });

            let result = self
                .http_client
                .post(&probe_url)
                .timeout(timeout)
                .header("Content-Type", "application/json")
                .json(&probe_body)
                .send()
                .await;

            match result {
                Ok(resp) if resp.status().is_success() => {
                    if let Some(mut entry) = self.entries.get_mut(model_id) {
                        entry.last_used = Utc::now();
                        entry.request_count += 1;
                    }
                    debug!(model = %model_id, "Keep-warm ping successful");
                }
                Ok(resp) => {
                    warn!(
                        model = %model_id,
                        status = %resp.status(),
                        "Keep-warm ping failed — backend error"
                    );
                    // Mark as unloaded so it can be re-warmed
                    if let Some(mut entry) = self.entries.get_mut(model_id) {
                        entry.status = WarmupStatus::Unloaded;
                    }
                }
                Err(e) => {
                    warn!(model = %model_id, error = %e, "Keep-warm ping failed");
                    if let Some(mut entry) = self.entries.get_mut(model_id) {
                        entry.status = WarmupStatus::Unloaded;
                    }
                }
            }
        }
    }

    /// Idle eviction pass: evict models that have not been used recently.
    async fn eviction_pass(&self) {
        let config = self.config.read().await;
        if !config.enabled || config.strategy == WarmupStrategy::Manual {
            return;
        }
        let idle_timeout = config.idle_evict_timeout;
        drop(config);

        let now = Utc::now();
        let idle_models: Vec<String> = self
            .entries
            .iter()
            .filter(|e| {
                e.value().status == WarmupStatus::Warm
                    && now.signed_duration_since(e.value().last_used)
                        .to_std()
                        .ok()
                        .map(|d| d >= idle_timeout)
                        .unwrap_or(false)
            })
            .map(|e| e.key().clone())
            .collect();

        for model_id in &idle_models {
            info!(model = %model_id, "Evicting idle model from warm-up pool");
            self.unload_model(model_id);
        }
    }

    /// Evict a single model according to the given strategy.
    ///
    /// Returns true if a model was evicted.
    async fn evict_one(&self, strategy: &WarmupStrategy, _backend_url: &str) -> bool {
        let candidates: Vec<(String, WarmupEntry)> = self
            .entries
            .iter()
            .filter(|e| e.value().status == WarmupStatus::Warm)
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect();

        if candidates.is_empty() {
            return false;
        }

        let victim = match strategy {
            WarmupStrategy::LRU => {
                // Sort by last_used ascending — oldest first
                candidates
                    .into_iter()
                    .min_by_key(|(_, e)| e.last_used)
                    .map(|(id, _)| id)
            }
            WarmupStrategy::LFU => {
                // Sort by request_count ascending — least frequently used first
                candidates
                    .into_iter()
                    .min_by_key(|(_, e)| e.request_count)
                    .map(|(id, _)| id)
            }
            WarmupStrategy::Priority => {
                // Sort by priority ascending — lowest priority first
                candidates
                    .into_iter()
                    .min_by_key(|(_, e)| e.priority)
                    .map(|(id, _)| id)
            }
            WarmupStrategy::Manual => return false,
        };

        if let Some(model_id) = victim {
            self.unload_model(&model_id);
            true
        } else {
            false
        }
    }

    // -- Public queries -----------------------------------------------------

    /// Get the full pool status (all entries).
    pub async fn get_status(&self) -> WarmupPoolStatus {
        let config = self.config.read().await;
        let entries: Vec<WarmupEntry> = self.entries.iter().map(|e| e.value().clone()).collect();
        let current_count = entries.len();
        WarmupPoolStatus {
            enabled: config.enabled,
            strategy: format!("{:?}", config.strategy).to_lowercase(),
            pool_size: config.pool_size,
            current_count,
            total_vram_reserved: self.total_vram_reserved.load(Ordering::Relaxed),
            entries,
        }
    }

    /// Get pool statistics.
    pub fn get_stats(&self) -> WarmupPoolStats {
        let warm_count = self
            .entries
            .iter()
            .filter(|e| e.value().status == WarmupStatus::Warm)
            .count();
        let warming_count = self
            .entries
            .iter()
            .filter(|e| e.value().status == WarmupStatus::Warming)
            .count();
        let evicting_count = self
            .entries
            .iter()
            .filter(|e| e.value().status == WarmupStatus::Evicting)
            .count();

        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total = hits + misses;
        let hit_rate = if total > 0 { hits as f64 / total as f64 } else { 0.0 };

        WarmupPoolStats {
            total_warmups: self.total_warmups.load(Ordering::Relaxed),
            total_evictions: self.total_evictions.load(Ordering::Relaxed),
            total_vram_reserved: self.total_vram_reserved.load(Ordering::Relaxed),
            warm_count,
            warming_count,
            evicting_count,
            hit_rate,
            hits,
            misses,
        }
    }

    /// Get current configuration (clone).
    pub async fn get_config(&self) -> WarmupConfig {
        self.config.read().await.clone()
    }

    /// Update configuration. Returns the new config.
    pub async fn update_config(&self, update: UpdateWarmupConfigRequest) -> WarmupConfig {
        let mut config = self.config.write().await;
        if let Some(enabled) = update.enabled {
            config.enabled = enabled;
        }
        if let Some(pool_size) = update.pool_size {
            config.pool_size = pool_size;
        }
        if let Some(preload) = update.preload_on_startup {
            config.preload_on_startup = preload;
        }
        if let Some(secs) = update.warmup_timeout_secs {
            config.warmup_timeout = Duration::from_secs(secs);
        }
        if let Some(secs) = update.idle_evict_timeout_secs {
            config.idle_evict_timeout = Duration::from_secs(secs);
        }
        if let Some(strategy) = update.strategy {
            config.strategy = strategy;
        }
        if let Some(secs) = update.keep_warm_interval_secs {
            config.keep_warm_interval = Duration::from_secs(secs);
        }
        if let Some(secs) = update.eviction_check_interval_secs {
            config.eviction_check_interval = Duration::from_secs(secs);
        }
        info!(
            enabled = config.enabled,
            pool_size = config.pool_size,
            strategy = ?config.strategy,
            "Warmup pool configuration updated"
        );
        config.clone()
    }

    /// Check if a model is currently warm.
    pub fn is_warm(&self, model_id: &str) -> bool {
        self.entries
            .get(model_id)
            .map(|e| e.value().status == WarmupStatus::Warm)
            .unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// Humantime serde helper (inline to avoid adding a crate)
// ---------------------------------------------------------------------------

mod humantime_serde {
    use serde::{self, Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{}s", duration.as_secs()))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        // Accept formats: "60s", "60", "60m", etc.
        let secs = if let Some(stripped) = s.strip_suffix('s') {
            stripped.parse::<u64>().map_err(serde::de::Error::custom)?
        } else if let Some(stripped) = s.strip_suffix('m') {
            stripped
                .parse::<u64>()
                .map_err(serde::de::Error::custom)?
                * 60
        } else if let Some(stripped) = s.strip_suffix('h') {
            stripped
                .parse::<u64>()
                .map_err(serde::de::Error::custom)?
                * 3600
        } else {
            s.parse::<u64>().map_err(serde::de::Error::custom)?
        };
        Ok(Duration::from_secs(secs))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> WarmupConfig {
        WarmupConfig {
            enabled: true,
            pool_size: 2,
            preload_on_startup: vec![],
            warmup_timeout: Duration::from_secs(5),
            idle_evict_timeout: Duration::from_secs(3600),
            strategy: WarmupStrategy::LRU,
            keep_warm_interval: Duration::from_secs(300),
            eviction_check_interval: Duration::from_secs(300),
            backend_url: "http://127.0.0.1:11434".to_string(),
        }
    }

    #[test]
    fn test_warmup_config_default() {
        let config = WarmupConfig::default();
        assert!(config.enabled);
        assert_eq!(config.pool_size, 3);
        assert_eq!(config.strategy, WarmupStrategy::LRU);
    }

    #[test]
    fn test_warmup_strategy_display() {
        assert_eq!(WarmupStatus::Warming.to_string(), "warming");
        assert_eq!(WarmupStatus::Warm.to_string(), "warm");
        assert_eq!(WarmupStatus::Evicting.to_string(), "evicting");
        assert_eq!(WarmupStatus::Unloaded.to_string(), "unloaded");
    }

    #[tokio::test]
    async fn test_pool_creation_and_status() {
        let pool = WarmupPool::new(test_config());
        let status = pool.get_status().await;
        assert!(status.enabled);
        assert_eq!(status.pool_size, 2);
        assert_eq!(status.current_count, 0);
        assert_eq!(status.entries.len(), 0);
    }

    #[tokio::test]
    async fn test_unload_nonexistent_model() {
        let pool = WarmupPool::new(test_config());
        assert!(!pool.unload_model("nonexistent"));
    }

    #[tokio::test]
    async fn test_record_hit_miss() {
        let pool = WarmupPool::new(test_config());
        pool.record_miss();
        pool.record_miss();
        let stats = pool.get_stats();
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.hits, 0);
    }

    #[tokio::test]
    async fn test_update_config() {
        let pool = WarmupPool::new(test_config());
        let new_config = pool
            .update_config(UpdateWarmupConfigRequest {
                enabled: Some(false),
                pool_size: Some(10),
                preload_on_startup: None,
                warmup_timeout_secs: Some(60),
                idle_evict_timeout_secs: None,
                strategy: Some(WarmupStrategy::Manual),
                keep_warm_interval_secs: None,
                eviction_check_interval_secs: None,
            })
            .await;
        assert!(!new_config.enabled);
        assert_eq!(new_config.pool_size, 10);
        assert_eq!(new_config.strategy, WarmupStrategy::Manual);
        assert_eq!(new_config.warmup_timeout, Duration::from_secs(60));
        // Unchanged fields should retain original values
        assert_eq!(
            new_config.idle_evict_timeout,
            Duration::from_secs(3600)
        );
    }

    #[test]
    fn test_humantime_duration_serde() {
        #[derive(Deserialize, Serialize)]
        struct Wrapper {
            #[serde(with = "humantime_serde")]
            dur: Duration,
        }

        // Serialize
        let w = Wrapper {
            dur: Duration::from_secs(120),
        };
        let json = serde_json::to_string(&w).unwrap();
        assert!(json.contains("120s"));

        // Deserialize
        let w2: Wrapper = serde_json::from_str(r#"{"dur":"60s"}"#).unwrap();
        assert_eq!(w2.dur, Duration::from_secs(60));

        let w3: Wrapper = serde_json::from_str(r#"{"dur":"5m"}"#).unwrap();
        assert_eq!(w3.dur, Duration::from_secs(300));

        let w4: Wrapper = serde_json::from_str(r#"{"dur":"1h"}"#).unwrap();
        assert_eq!(w4.dur, Duration::from_secs(3600));

        let w5: Wrapper = serde_json::from_str(r#"{"dur":"42"}"#).unwrap();
        assert_eq!(w5.dur, Duration::from_secs(42));
    }
}
