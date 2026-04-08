//! Automatic model discovery via HuggingFace registry scanning.
//!
//! Scans the HuggingFace Hub API for popular open-weight models matching
//! supported architectures (llama, qwen, mistral, deepseek, phi, gemma, codestral).
//! Filters by GGUF quantization availability, download size, and license.
//! Results are cached locally and refreshed on a configurable interval.
//!
//! API endpoints:
//! - GET  /api/discovery/models?architecture=llama&max_size=20gb&sort=downloads
//! - GET  /api/discovery/recommended
//! - POST /api/discovery/scan

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::config::ModelDiscoveryConfig;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A model discovered from HuggingFace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredModel {
    /// HuggingFace model ID (e.g. "meta-llama/Llama-3.1-8B-Instruct")
    pub model_id: String,
    /// Detected architecture family (llama, qwen, mistral, etc.)
    pub architecture: String,
    /// Model size in bytes on disk (GGUF)
    pub size_bytes: u64,
    /// Number of downloads on HuggingFace
    pub downloads: u64,
    /// Number of likes on HuggingFace
    pub likes: u64,
    /// License identifier (e.g. "apache-2.0", "llama3.1")
    pub license: String,
    /// Last modified timestamp (RFC3339)
    pub last_modified: String,
    /// Best GGUF quantization file found (e.g. "Q4_K_M")
    pub gguf_file: String,
    /// HuggingFace repo URL
    pub hf_url: String,
    /// Tags from HuggingFace (e.g. ["text-generation", "gguf"])
    pub tags: Vec<String>,
    /// Community rating (0.0 - 5.0, derived from likes)
    pub community_rating: f32,
}

/// Sort order for discovery results.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DiscoverySort {
    /// Sort by download count (default).
    #[default]
    Downloads,
    /// Sort by recent update time.
    RecentlyUpdated,
    /// Sort by community rating (likes).
    Rating,
    /// Sort by model size (smallest first).
    Size,
}

/// Query parameters for the discovery models endpoint.
#[derive(Debug, Deserialize, Default)]
pub struct DiscoveryQuery {
    pub architecture: Option<String>,
    pub max_size_gb: Option<f64>,
    pub sort: Option<DiscoverySort>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub license: Option<String>,
    pub search: Option<String>,
}

/// Response for the discovery models endpoint.
#[derive(Debug, Serialize)]
pub struct DiscoveryResponse {
    pub models: Vec<DiscoveredModel>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub cached_at: String,
    pub cache_age_secs: u64,
}

/// Response for the scan trigger endpoint.
#[derive(Debug, Serialize)]
pub struct ScanResponse {
    pub status: String,
    pub models_found: usize,
    pub duration_secs: f64,
    pub next_refresh_secs: u64,
}

// ---------------------------------------------------------------------------
// Supported architectures
// ---------------------------------------------------------------------------

/// Architecture keywords we support. The HuggingFace API `library_name` or
/// model card tags often contain these.
static SUPPORTED_ARCHITECTURES: &[&str] = &[
    "llama",
    "qwen",
    "mistral",
    "deepseek",
    "phi",
    "gemma",
    "codestral",
];

/// Default recommended models — curated list of popular, well-supported models.
static RECOMMENDED_MODELS: &[&str] = &[
    "meta-llama/Llama-3.1-8B-Instruct",
    "meta-llama/Llama-3.1-70B-Instruct",
    "Qwen/Qwen2.5-7B-Instruct",
    "Qwen/Qwen2.5-72B-Instruct",
    "mistralai/Mistral-7B-Instruct-v0.3",
    "mistralai/Mixtral-8x7B-Instruct-v0.1",
    "deepseek-ai/DeepSeek-V3",
    "deepseek-ai/DeepSeek-R1-Distill-Qwen-32B",
    "microsoft/phi-4",
    "google/gemma-2-9b-it",
    "mistralai/Codestral-22B-v0.1",
    "meta-llama/Llama-3.2-3B-Instruct",
];

/// Licenses that allow commercial use.
static COMMERCIAL_LICENSES: &[&str] = &[
    "apache-2.0",
    "mit",
    "bsd-3-clause",
    "bsd-2-clause",
    "cc-by-4.0",
    "cc-by-sa-4.0",
    "openrail",
    "llama3.1",
    "llama3.2",
    "llama3.3",
    "llama4",
    "gemma",
    "qwen",
    "deepseek",
    "mistral",
    "phi",
    "codestral",
    "agpl-3.0",
    "lgpl-3.0",
    "epl-2.0",
    "isc",
    "unlicense",
    "0bsd",
    "odc-by",
    "cdla-sharing-1.0",
    "cdla-permissive-1.0",
    "bigscience-openrail-m",
    "openrail++",
    "creativeml-openrail-m",
];

// ---------------------------------------------------------------------------
// HuggingFace API response types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct HfModelResponse {
    id: String,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    likes: u64,
    #[serde(default)]
    last_modified: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    siblings: Vec<HfSibling>,
    #[serde(default)]
    library_name: Option<String>,
    #[serde(default)]
    license: Option<serde_json::Value>,
    #[serde(default)]
    card_data: Option<serde_json::Value>,
    #[serde(default)]
    sha: Option<String>,
    author: Option<String>,
    #[serde(default)]
    pipeline_tag: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HfSibling {
    rfilename: String,
    #[serde(default)]
    size: Option<u64>,
}

// ---------------------------------------------------------------------------
// ModelDiscovery service
// ---------------------------------------------------------------------------

/// Model discovery service that scans HuggingFace for compatible models.
pub struct ModelDiscovery {
    config: ModelDiscoveryConfig,
    http_client: Client,
    /// Cached discovery results.
    cached_models: Arc<RwLock<Vec<DiscoveredModel>>>,
    /// When the cache was last refreshed.
    last_refresh: Arc<RwLock<Instant>>,
    /// Whether a scan is currently in progress.
    scanning: Arc<RwLock<bool>>,
    /// Cache file path for persistence.
    cache_file: PathBuf,
}

impl ModelDiscovery {
    /// Create a new model discovery service.
    pub fn new(config: ModelDiscoveryConfig, cache_dir: Option<PathBuf>) -> Result<Self> {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(15))
            .build()
            .context("Failed to build HTTP client for model discovery")?;

        let cache_dir = cache_dir.unwrap_or_else(|| {
            dirs::cache_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join("xergon-agent")
        });

        let cache_file = cache_dir.join("discovery_cache.json");

        Ok(Self {
            config,
            http_client,
            cached_models: Arc::new(RwLock::new(Vec::new())),
            last_refresh: Arc::new(RwLock::new(Instant::now() - Duration::from_secs(
                u64::MAX / 2,
            ))),
            scanning: Arc::new(RwLock::new(false)),
            cache_file,
        })
    }

    /// Load cached discovery results from disk.
    pub async fn load_cache(&self) -> Result<()> {
        if !self.cache_file.exists() {
            debug!("No discovery cache file found at {:?}", self.cache_file);
            return Ok(());
        }

        let content = tokio::fs::read_to_string(&self.cache_file)
            .await
            .context("Failed to read discovery cache file")?;

        let models: Vec<DiscoveredModel> = serde_json::from_str(&content)
            .context("Failed to parse discovery cache file")?;

        *self.cached_models.write().await = models;
        *self.last_refresh.write().await = {
            let metadata = tokio::fs::metadata(&self.cache_file)
                .await
                .ok();
            // Use file modification time as a proxy for last refresh
            let modified = metadata
                .and_then(|m| m.modified().ok())
                .unwrap_or(std::time::SystemTime::now());
            let duration = modified
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            Instant::now() - Duration::from_secs(duration.as_secs())
        };

        let models_count = self.cached_models.read().await.len();
        info!(
            models_count,
            "Loaded discovery cache from disk"
        );

        Ok(())
    }

    /// Save cached discovery results to disk.
    async fn save_cache(&self) -> Result<()> {
        if let Some(parent) = self.cache_file.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .context("Failed to create cache directory")?;
        }

        let models = self.cached_models.read().await;
        let content = serde_json::to_string_pretty(&*models)
            .context("Failed to serialize discovery cache")?;

        tokio::fs::write(&self.cache_file, content.as_bytes())
            .await
            .context("Failed to write discovery cache file")?;

        debug!("Saved discovery cache to {:?}", self.cache_file);
        Ok(())
    }

    /// Check if the cache is stale and needs refresh.
    pub async fn is_cache_stale(&self) -> bool {
        let last = *self.last_refresh.read().await;
        last.elapsed() > Duration::from_secs(self.config.refresh_interval_secs)
    }

    /// Perform a full scan of HuggingFace for compatible models.
    ///
    /// This is the main discovery logic: searches HF API for each supported
    /// architecture, filters by GGUF availability + size + license, and ranks.
    pub async fn scan(&self) -> Result<Vec<DiscoveredModel>> {
        let scan_start = Instant::now();

        // Check if already scanning
        {
            let mut scanning = self.scanning.write().await;
            if *scanning {
                warn!("Discovery scan already in progress, skipping");
                return Ok(self.cached_models.read().await.clone());
            }
            *scanning = true;
        }

        let result = self.do_scan().await;

        // Always reset scanning flag
        *self.scanning.write().await = false;

        match result {
            Ok(models) => {
                let duration = scan_start.elapsed();
                info!(
                    models_found = models.len(),
                    duration_secs = duration.as_secs_f64(),
                    "Discovery scan completed"
                );

                *self.cached_models.write().await = models.clone();
                *self.last_refresh.write().await = Instant::now();

                // Persist to disk
                if let Err(e) = self.save_cache().await {
                    warn!(error = %e, "Failed to persist discovery cache");
                }

                Ok(models)
            }
            Err(e) => {
                warn!(error = %e, "Discovery scan failed");
                Err(e)
            }
        }
    }

    async fn do_scan(&self) -> Result<Vec<DiscoveredModel>> {
        let mut all_models = Vec::new();
        let hf_token = &self.config.huggingface_token;

        for arch in SUPPORTED_ARCHITECTURES {
            // Skip explicitly excluded architectures
            if self.config.exclude_architectures.iter().any(|a| a.eq_ignore_ascii_case(arch)) {
                continue;
            }

            match self.scan_architecture(arch, hf_token).await {
                Ok(models) => {
                    debug!(architecture = arch, found = models.len(), "Architecture scan complete");
                    all_models.extend(models);
                }
                Err(e) => {
                    warn!(architecture = arch, error = %e, "Architecture scan failed, continuing");
                }
            }
        }

        // Deduplicate by model_id (keep the one with highest downloads)
        let mut best: HashMap<String, DiscoveredModel> = HashMap::new();
        for m in all_models {
            let entry = best.entry(m.model_id.clone()).or_insert_with(|| m.clone());
            if m.downloads > entry.downloads {
                *entry = m;
            }
        }

        let mut models: Vec<DiscoveredModel> = best.into_values().collect();

        // Sort by downloads descending
        models.sort_by(|a, b| b.downloads.cmp(&a.downloads));

        // Remove explicitly excluded models
        if !self.config.exclude_models.is_empty() {
            models.retain(|m| {
                !self
                    .config
                    .exclude_models
                    .iter()
                    .any(|ex| m.model_id.eq_ignore_ascii_case(ex))
            });
        }

        Ok(models)
    }

    /// Scan HuggingFace for models of a specific architecture.
    async fn scan_architecture(&self, arch: &str, hf_token: &str) -> Result<Vec<DiscoveredModel>> {
        let mut models = Vec::new();
        let max_size_bytes = (self.config.max_model_size_gb as u64) * 1_073_741_824; // GB to bytes

        // Search HuggingFace API for GGUF models with this architecture
        // We use the search API with tags gguf and the architecture name
        let search_queries = vec![
            // Search for models with the architecture in the model ID or tags
            format!("gguf+{}", arch),
            format!("{}-gguf", arch),
            format!("{}-GGUF", arch),
        ];

        let mut seen_ids = std::collections::HashSet::new();

        for query in &search_queries {
            let url = format!(
                "https://huggingface.co/api/models?search={}&limit=50&sort=downloads&filter=gguf",
                urlencoding::encode(query)
            );

            let mut req = self.http_client.get(&url);
            if !hf_token.is_empty() {
                req = req.bearer_auth(hf_token);
            }

            let resp = match req.send().await {
                Ok(r) => r,
                Err(e) => {
                    warn!(url = %url, error = %e, "HF API request failed");
                    continue;
                }
            };

            if !resp.status().is_success() {
                debug!(status = %resp.status(), "HF API returned non-success");
                continue;
            }

            let hf_models: Vec<HfModelResponse> = match resp.json().await {
                Ok(m) => m,
                Err(e) => {
                    warn!(error = %e, "Failed to parse HF API response");
                    continue;
                }
            };

            for hf_model in &hf_models {
                if seen_ids.contains(&hf_model.id) {
                    continue;
                }

                // Check if model is excluded
                if self
                    .config
                    .exclude_models
                    .iter()
                    .any(|ex| hf_model.id.eq_ignore_ascii_case(ex))
                {
                    continue;
                }

                // Find the best GGUF file (prefer Q4_K_M or Q5_K_M as good balance)
                let gguf_files: Vec<&HfSibling> = hf_model
                    .siblings
                    .iter()
                    .filter(|s| {
                        s.rfilename.to_lowercase().ends_with(".gguf")
                            && !s.rfilename.to_lowercase().contains("split")
                    })
                    .collect();

                if gguf_files.is_empty() {
                    continue;
                }

                // Pick the best quantization (prefer Q4_K_M, then Q5_K_M, then smallest)
                let best_file = pick_best_gguf(&gguf_files);
                let file_size = best_file.size.unwrap_or(0);

                // Filter by size
                if file_size > max_size_bytes && max_size_bytes > 0 {
                    continue;
                }

                // Check license
                let license_str = extract_license(&hf_model.license);
                let allowed = if self.config.allowed_licenses.is_empty() {
                    is_commercial_license(&license_str)
                } else {
                    self.config
                        .allowed_licenses
                        .iter()
                        .any(|l| l.eq_ignore_ascii_case(&license_str))
                };

                if !allowed {
                    debug!(
                        model = %hf_model.id,
                        license = %license_str,
                        "Model excluded due to license"
                    );
                    continue;
                }

                // Determine architecture from tags/model ID
                let detected_arch = detect_architecture(&hf_model.id, &hf_model.tags, arch);

                // Community rating (0.0 - 5.0) derived from likes
                // Simple heuristic: normalize likes to a 0-5 scale
                let community_rating = ((hf_model.likes as f32).log10().max(0.0) * 1.5).min(5.0);

                let discovered = DiscoveredModel {
                    model_id: hf_model.id.clone(),
                    architecture: detected_arch,
                    size_bytes: file_size,
                    downloads: hf_model.downloads,
                    likes: hf_model.likes,
                    license: license_str,
                    last_modified: hf_model.last_modified.clone(),
                    gguf_file: best_file.rfilename.clone(),
                    hf_url: format!("https://huggingface.co/{}", hf_model.id),
                    tags: hf_model.tags.clone(),
                    community_rating,
                };

                seen_ids.insert(hf_model.id.clone());
                models.push(discovered);
            }
        }

        Ok(models)
    }

    /// Get cached models, filtered and sorted according to the query.
    pub async fn get_models(&self, query: &DiscoveryQuery) -> DiscoveryResponse {
        let cached = self.cached_models.read().await;
        let last = *self.last_refresh.read().await;
        let cache_age_secs = last.elapsed().as_secs();

        let mut filtered: Vec<DiscoveredModel> = cached.clone();

        // Filter by architecture
        if let Some(ref arch) = query.architecture {
            filtered.retain(|m| m.architecture.eq_ignore_ascii_case(arch));
        }

        // Filter by max size
        if let Some(max_gb) = query.max_size_gb {
            let max_bytes = (max_gb as u64) * 1_073_741_824;
            filtered.retain(|m| m.size_bytes <= max_bytes);
        }

        // Filter by license
        if let Some(ref lic) = query.license {
            filtered.retain(|m| m.license.eq_ignore_ascii_case(lic));
        }

        // Filter by search term
        if let Some(ref search) = query.search {
            let search_lower = search.to_lowercase();
            filtered.retain(|m| {
                m.model_id.to_lowercase().contains(&search_lower)
                    || m.tags.iter().any(|t| t.to_lowercase().contains(&search_lower))
                    || m.architecture.to_lowercase().contains(&search_lower)
            });
        }

        let total = filtered.len();

        // Sort
        match query.sort.as_ref().unwrap_or(&DiscoverySort::Downloads) {
            DiscoverySort::Downloads => filtered.sort_by(|a, b| b.downloads.cmp(&a.downloads)),
            DiscoverySort::RecentlyUpdated => {
                filtered.sort_by(|a, b| b.last_modified.cmp(&a.last_modified))
            }
            DiscoverySort::Rating => filtered.sort_by(|a, b| {
                b.community_rating
                    .partial_cmp(&a.community_rating)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            DiscoverySort::Size => filtered.sort_by(|a, b| a.size_bytes.cmp(&b.size_bytes)),
        }

        // Paginate
        let offset = query.offset.unwrap_or(0);
        let limit = query.limit.unwrap_or(50).min(200);
        let models: Vec<DiscoveredModel> = filtered
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect();

        let cached_at = chrono::Utc::now().to_rfc3339();

        DiscoveryResponse {
            models,
            total,
            offset,
            limit,
            cached_at,
            cache_age_secs,
        }
    }

    /// Get the curated list of recommended models, enriched with discovery data.
    pub async fn get_recommended(&self) -> Vec<DiscoveredModel> {
        let cached = self.cached_models.read().await;

        RECOMMENDED_MODELS
            .iter()
            .filter_map(|model_id| cached.iter().find(|m| m.model_id == *model_id).cloned())
            .collect()
    }

    /// Check if a scan is currently in progress.
    pub async fn is_scanning(&self) -> bool {
        *self.scanning.read().await
    }

    /// Spawn a background refresh loop.
    pub fn spawn_refresh_loop(self: Arc<Self>) {
        let interval = self.config.refresh_interval_secs;

        tokio::spawn(async move {
            // Initial load from disk
            if let Err(e) = self.load_cache().await {
                debug!(error = %e, "Failed to load discovery cache from disk");
            }

            loop {
                if self.is_cache_stale().await {
                    info!("Discovery cache is stale, triggering refresh");
                    if let Err(e) = self.scan().await {
                        warn!(error = %e, "Background discovery refresh failed");
                    }
                }

                tokio::time::sleep(Duration::from_secs(interval)).await;
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Pick the best GGUF file from a list of candidates.
/// Prefers Q4_K_M as a good quality/size balance, falls back to Q5_K_M, Q8_0, then smallest.
fn pick_best_gguf<'a>(files: &[&'a HfSibling]) -> &'a HfSibling {
    const PREFERRED_QUANTS: &[&str] = &[
        "q4_k_m",
        "q5_k_m",
        "q4_0",
        "q5_0",
        "q8_0",
        "q4_k_s",
        "q5_k_s",
        "q2_k",
        "q3_k",
        "q6_k",
        "f16",
        "f32",
    ];

    // Try to find preferred quantization
    for pref in PREFERRED_QUANTS {
        if let Some(f) = files.iter().find(|s| {
            s.rfilename
                .to_lowercase()
                .contains(pref)
        }) {
            return f;
        }
    }

    // Fall back to smallest file with a known size
    files
        .iter()
        .filter(|f| f.size.is_some())
        .min_by_key(|f| f.size.unwrap_or(u64::MAX))
        .unwrap_or_else(|| files.first().unwrap())
}

/// Extract license string from HF API response.
fn extract_license(license: &Option<serde_json::Value>) -> String {
    match license {
        None => "unknown".to_string(),
        Some(serde_json::Value::String(s)) => s.to_lowercase(),
        Some(serde_json::Value::Array(arr)) => {
            // Some models have a list of licenses
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                .find(|l| is_commercial_license(l))
                .unwrap_or_else(|| "unknown".to_string())
        }
        _ => "unknown".to_string(),
    }
}

/// Check if a license allows commercial use.
fn is_commercial_license(license: &str) -> bool {
    let lic = license.to_lowercase();
    COMMERCIAL_LICENSES
        .iter()
        .any(|allowed| lic.contains(*allowed) || allowed.contains(&lic.as_str()))
        || lic.contains("permissive")
        || lic.contains("open")
        || lic.contains("free")
        || lic.is_empty()
}

/// Detect architecture from model ID, tags, and search hint.
fn detect_architecture(model_id: &str, tags: &[String], search_arch: &str) -> String {
    let model_lower = model_id.to_lowercase();

    for arch in SUPPORTED_ARCHITECTURES {
        if model_lower.contains(arch) {
            return arch.to_string();
        }
    }

    for tag in tags {
        let tag_lower = tag.to_lowercase();
        for arch in SUPPORTED_ARCHITECTURES {
            if tag_lower.contains(arch) {
                return arch.to_string();
            }
        }
    }

    // Fall back to the search architecture hint
    search_arch.to_string()
}

/// Simple URL encoding helper (avoids adding a dependency).
mod urlencoding {
    pub fn encode(s: &str) -> String {
        s.chars()
            .map(|c| match c {
                'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
                _ => format!("%{:02X}", c as u8),
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pick_best_gguf_prefers_q4_k_m() {
        let files = vec![
            HfSibling {
                rfilename: "model-Q2_K.gguf".into(),
                size: Some(2_000_000_000),
            },
            HfSibling {
                rfilename: "model-Q4_K_M.gguf".into(),
                size: Some(4_500_000_000),
            },
            HfSibling {
                rfilename: "model-Q8_0.gguf".into(),
                size: Some(8_000_000_000),
            },
        ];
        let refs: Vec<&HfSibling> = files.iter().collect();
        let best = pick_best_gguf(&refs);
        assert!(best.rfilename.contains("Q4_K_M"));
    }

    #[test]
    fn test_detect_architecture_from_model_id() {
        assert_eq!(
            detect_architecture("meta-llama/Llama-3.1-8B", &[], "unknown"),
            "llama"
        );
        assert_eq!(
            detect_architecture("Qwen/Qwen2.5-7B", &[], "unknown"),
            "qwen"
        );
        assert_eq!(
            detect_architecture("mistralai/Mistral-7B", &[], "unknown"),
            "mistral"
        );
        assert_eq!(
            detect_architecture("deepseek-ai/DeepSeek-V3", &[], "unknown"),
            "deepseek"
        );
        assert_eq!(
            detect_architecture("microsoft/phi-4", &[], "unknown"),
            "phi"
        );
        assert_eq!(
            detect_architecture("google/gemma-2-9b", &[], "unknown"),
            "gemma"
        );
    }

    #[test]
    fn test_extract_license_string() {
        let lic = extract_license(&Some(serde_json::Value::String("Apache-2.0".into())));
        assert_eq!(lic, "apache-2.0");

        let lic = extract_license(&Some(serde_json::Value::String("MIT".into())));
        assert_eq!(lic, "mit");

        let lic = extract_license(&None);
        assert_eq!(lic, "unknown");
    }

    #[test]
    fn test_is_commercial_license() {
        assert!(is_commercial_license("apache-2.0"));
        assert!(is_commercial_license("MIT"));
        assert!(is_commercial_license("llama3.1"));
        assert!(is_commercial_license("qwen-research"));
        assert!(is_commercial_license(""));
    }

    #[test]
    fn test_discovery_response_serialization() {
        let resp = DiscoveryResponse {
            models: vec![],
            total: 0,
            offset: 0,
            limit: 50,
            cached_at: "2025-01-01T00:00:00Z".into(),
            cache_age_secs: 0,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"total\":0"));
    }

    #[test]
    fn test_discovered_model_serialization() {
        let model = DiscoveredModel {
            model_id: "meta-llama/Llama-3.1-8B".into(),
            architecture: "llama".into(),
            size_bytes: 4_500_000_000,
            downloads: 1_000_000,
            likes: 5_000,
            license: "apache-2.0".into(),
            last_modified: "2025-01-01T00:00:00Z".into(),
            gguf_file: "Llama-3.1-8B-Instruct-Q4_K_M.gguf".into(),
            hf_url: "https://huggingface.co/meta-llama/Llama-3.1-8B".into(),
            tags: vec!["text-generation".into(), "gguf".into()],
            community_rating: 4.5,
        };
        let json = serde_json::to_string(&model).unwrap();
        assert!(json.contains("llama"));
        assert!(json.contains("4500000000"));
    }
}
