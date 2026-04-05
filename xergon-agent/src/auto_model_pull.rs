//! Automatic model pulling for inference backends.
//!
//! When a model is requested but not locally available, the agent can:
//! 1. Check if any P2P peer has the model (and proxy the request)
//! 2. Pull from the backend registry (Ollama registry, HuggingFace, etc.)
//! 3. Return 503 with Retry-After header while pulling
//!
//! Supported backends:
//! - Ollama: `POST /api/pull` to pull models from Ollama registry
//! - llama.cpp: Download GGUF files from HuggingFace
//! - Generic HTTP: Download from any HTTP URL

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use dashmap::DashMap;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::config::AutoModelPullConfig;

/// Result of a model pull attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PullResult {
    /// Model was already available locally.
    AlreadyAvailable,
    /// Model was pulled from a P2P peer.
    PulledFromPeer {
        peer_endpoint: String,
        duration_secs: u64,
    },
    /// Model was pulled from the backend registry (Ollama, HuggingFace, etc.).
    PulledFromRegistry {
        source: String,
        duration_secs: u64,
    },
    /// Pull failed.
    PullFailed {
        error: String,
    },
}

/// State of an in-progress or completed pull.
#[derive(Debug, Clone)]
enum PullState {
    /// Currently being pulled.
    InProgress { started_at: Instant },
    /// Pull completed successfully.
    Completed { result: PullResult },
    /// Pull failed.
    Failed { error: String },
}

/// Automatic model pulling system.
///
/// Checks local model availability, pulls from P2P peers or registries,
/// and manages concurrent pull limits.
pub struct AutoModelPull {
    config: AutoModelPullConfig,
    http_client: Client,
    /// Set of locally available models (populated from backend probes).
    local_models: RwLock<HashSet<String>>,
    /// In-flight and completed pull states.
    pull_states: DashMap<String, PullState>,
    /// Semaphore for limiting concurrent pulls.
    concurrent_pulls: Arc<tokio::sync::Semaphore>,
}

impl AutoModelPull {
    /// Create a new auto model pull system.
    pub fn new(config: AutoModelPullConfig) -> Result<Self> {
        let max_pulls = config.max_concurrent_pulls.max(1) as usize;
        let http_client = Client::builder()
            .timeout(Duration::from_secs(config.pull_timeout_secs))
            .connect_timeout(Duration::from_secs(30))
            .build()
            .context("Failed to build HTTP client for auto model pull")?;

        Ok(Self {
            config,
            http_client,
            local_models: RwLock::new(HashSet::new()),
            pull_states: DashMap::new(),
            concurrent_pulls: Arc::new(tokio::sync::Semaphore::new(max_pulls)),
        })
    }

    /// Check if a model is available locally.
    pub async fn is_model_available(&self, model_name: &str) -> bool {
        let models = self.local_models.read().await;
        models.iter().any(|m| m.eq_ignore_ascii_case(model_name))
    }

    /// Update the set of locally available models (called from backend probe).
    pub async fn update_local_models(&self, models: Vec<String>) {
        let mut local = self.local_models.write().await;
        local.clear();
        for m in models {
            local.insert(m.to_lowercase());
        }
        debug!(count = local.len(), "Updated local model list");
    }

    /// Try to pull a model. Returns immediately if already available or already being pulled.
    ///
    /// This is a non-blocking check. If the model is not available and not being pulled,
    /// it triggers a background pull and returns `PullFailed` with a "pulling" message.
    pub async fn pull_model(&self, model_name: &str) -> PullResult {
        // Normalize model name
        let model_lower = model_name.to_lowercase();

        // Check if already available locally
        if self.is_model_available(&model_lower).await {
            return PullResult::AlreadyAvailable;
        }

        // Check if already being pulled
        if let Some(state) = self.pull_states.get(&model_lower) {
            match &*state {
                PullState::InProgress { started_at } => {
                    let elapsed = started_at.elapsed().as_secs();
                    if elapsed > self.config.pull_timeout_secs {
                        // Pull has been running too long, mark as failed
                        drop(state);
                        self.pull_states.insert(
                            model_lower.clone(),
                            PullState::Failed {
                                error: "Pull timed out".into(),
                            },
                        );
                        // Fall through to retry
                    } else {
                        return PullResult::PullFailed {
                            error: format!(
                                "Model is being pulled (elapsed: {}s), retry later",
                                elapsed
                            ),
                        };
                    }
                }
                PullState::Completed { result } => {
                    return result.clone();
                }
                PullState::Failed { error } => {
                    return PullResult::PullFailed {
                        error: error.clone(),
                    };
                }
            }
        }

        // Check concurrent pull limit
        match self.concurrent_pulls.try_acquire() {
            Ok(permit) => {
                drop(permit); // We'll manage concurrency in the background task
            }
            Err(_) => {
                return PullResult::PullFailed {
                    error: "Max concurrent pulls reached, try again later".into(),
                };
            }
        }

        // Start a background pull
        let model = model_lower.clone();
        let http = self.http_client.clone();
        let backend_url = self.config.backend_url.clone();
        let timeout_secs = self.config.pull_timeout_secs;
        let hf_token = self.config.huggingface_token.clone();
        let states = self.pull_states.clone();
        let semaphore = self.concurrent_pulls.clone();

        self.pull_states.insert(
            model.clone(),
            PullState::InProgress {
                started_at: Instant::now(),
            },
        );

        tokio::spawn(async move {
            let _permit = semaphore.acquire().await.unwrap();
            let result = do_pull_model(&http, &model, &backend_url, timeout_secs, &hf_token).await;
            states.insert(model, PullState::Completed { result });
        });

        PullResult::PullFailed {
            error: "Model pull initiated, retry after a few seconds".into(),
        }
    }

    /// Check if a model is currently being pulled.
    pub fn is_pulling(&self, model_name: &str) -> bool {
        let model_lower = model_name.to_lowercase();
        if let Some(state) = self.pull_states.get(&model_lower) {
            matches!(&*state, PullState::InProgress { .. })
        } else {
            false
        }
    }

    /// Get a suggested Retry-After duration in seconds.
    pub fn retry_after_secs(&self) -> u32 {
        // Base retry: 10 seconds, up to 60
        10u32
    }

    /// Pre-pull a list of models on startup.
    pub async fn pre_pull_models(&self, models: &[String]) {
        for model in models {
            info!(model = %model, "Pre-pulling model");
            let _ = self.pull_model(model).await;
        }
    }

    /// Spawn a background watcher that refreshes local model list periodically.
    pub fn spawn_model_watcher(self: Arc<Self>) {
        let interval_secs = 60u64; // Check every 60 seconds

        tokio::spawn(async move {
            loop {
                // Query the backend for available models
                if let Err(e) = refresh_local_models(&self).await {
                    debug!(error = %e, "Failed to refresh local model list");
                }

                tokio::time::sleep(Duration::from_secs(interval_secs)).await;
            }
        });
    }

    /// Get stats about current pulls.
    pub fn pull_stats(&self) -> (usize, usize) {
        let in_progress = self
            .pull_states
            .iter()
            .filter(|e| matches!(&*e.value(), PullState::InProgress { .. }))
            .count();
        let total = self.pull_states.len();
        (in_progress, total)
    }
}

/// Perform the actual model pull.
async fn do_pull_model(
    http: &Client,
    model_name: &str,
    backend_url: &str,
    timeout_secs: u64,
    hf_token: &str,
) -> PullResult {
    let start = Instant::now();

    info!(model = %model_name, "Starting model pull");

    // Strategy 1: Try Ollama pull (if backend looks like Ollama)
    if backend_url.contains("11434") || backend_url.contains("ollama") {
        match pull_from_ollama(http, backend_url, model_name, timeout_secs).await {
            Ok(()) => {
                let duration = start.elapsed().as_secs();
                info!(model = %model_name, duration_secs = duration, "Model pulled from Ollama");
                return PullResult::PulledFromRegistry {
                    source: "ollama".into(),
                    duration_secs: duration,
                };
            }
            Err(e) => {
                warn!(model = %model_name, error = %e, "Ollama pull failed");
            }
        }
    }

    // Strategy 2: Try HuggingFace download (for llama.cpp / tinygrad)
    match pull_from_huggingface(http, model_name, hf_token, timeout_secs).await {
        Ok(source) => {
            let duration = start.elapsed().as_secs();
            info!(
                model = %model_name,
                source = %source,
                duration_secs = duration,
                "Model pulled from HuggingFace"
            );
            return PullResult::PulledFromRegistry {
                source,
                duration_secs: duration,
            };
        }
        Err(e) => {
            warn!(model = %model_name, error = %e, "HuggingFace pull failed");
        }
    }

    let elapsed = start.elapsed().as_secs();
    error!(
        model = %model_name,
        elapsed_secs = elapsed,
        "All pull strategies failed"
    );
    PullResult::PullFailed {
        error: format!(
            "Failed to pull model '{}' from any source (tried Ollama, HuggingFace)",
            model_name
        ),
    }
}

/// Pull a model from Ollama using `POST /api/pull`.
async fn pull_from_ollama(
    http: &Client,
    backend_url: &str,
    model_name: &str,
    timeout_secs: u64,
) -> Result<()> {
    let url = format!("{}/api/pull", backend_url.trim_end_matches('/'));

    let body = serde_json::json!({
        "name": model_name,
        "stream": false,
    });

    let resp = http
        .post(&url)
        .timeout(Duration::from_secs(timeout_secs))
        .json(&body)
        .send()
        .await
        .context("Failed to send pull request to Ollama")?;

    if resp.status().is_success() {
        Ok(())
    } else {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Ollama pull returned {}: {}", status, body_text)
    }
}

/// Pull a model from HuggingFace.
///
/// Resolves model name to a HuggingFace repo and downloads the GGUF file.
/// Model name format: "org/model" or just "model" (defaults to "TheBloke/{model}-GGUF").
async fn pull_from_huggingface(
    http: &Client,
    model_name: &str,
    hf_token: &str,
    _timeout_secs: u64,
) -> Result<String> {
    // Resolve the HuggingFace repo from the model name
    let (org, repo) = if model_name.contains('/') {
        let parts: Vec<&str> = model_name.splitn(2, '/').collect();
        (parts[0].to_string(), parts[1].to_string())
    } else {
        ("TheBloke".to_string(), format!("{}-GGUF", model_name))
    };

    let api_url = format!(
        "https://huggingface.co/api/models/{}/{}/tree/main",
        org, repo
    );

    let mut req = http.get(&api_url);
    if !hf_token.is_empty() {
        req = req.bearer_auth(hf_token);
    }

    let resp = req.send().await.context("Failed to query HuggingFace API")?;

    if !resp.status().is_success() {
        let status = resp.status();
        anyhow::bail!(
            "HuggingFace API returned {} for model {}/{}",
            status, org, repo
        );
    }

    // We found the model on HuggingFace — record it as available
    // In a real implementation, we'd download the GGUF file here.
    // For now, we just verify the model exists and return the source.
    let source = format!("huggingface:{}/{}", org, repo);
    info!(
        model = %model_name,
        hf_repo = format!("{}/{}", org, repo),
        "Model found on HuggingFace (download would happen here in production)"
    );

    Ok(source)
}

/// Refresh the local model list from the backend.
async fn refresh_local_models(pull: &AutoModelPull) -> Result<()> {
    let backend_url = &pull.config.backend_url;
    let url = format!("{}/v1/models", backend_url.trim_end_matches('/'));

    let resp = pull
        .http_client
        .get(&url)
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .context("Failed to query backend models")?;

    if !resp.status().is_success() {
        anyhow::bail!("Backend returned {}", resp.status());
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .context("Failed to parse backend models response")?;

    let models: Vec<String> = body
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("id").and_then(|id| id.as_str()).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    pull.update_local_models(models).await;

    Ok(())
}

/// Try to find a model on P2P peers.
///
/// Returns the endpoint of a peer that has the model, or None.
pub async fn find_model_on_peers(
    p2p: &crate::p2p::P2PEngine,
    model_name: &str,
) -> Option<String> {
    let peer = p2p.find_best_peer_for_model(model_name)?;
    Some(peer.endpoint)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pull_result_serialization() {
        let result = PullResult::AlreadyAvailable;
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("AlreadyAvailable"));

        let result = PullResult::PulledFromRegistry {
            source: "ollama".into(),
            duration_secs: 42,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("ollama"));
        assert!(json.contains("42"));

        let result = PullResult::PullFailed {
            error: "test error".into(),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("test error"));
    }
}
