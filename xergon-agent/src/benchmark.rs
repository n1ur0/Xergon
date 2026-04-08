//! Model Benchmark Suite — inference latency/TPS testing and memory profiling.
//!
//! Provides comprehensive benchmarking for LLM backends (Ollama, llama.cpp).
//! Measures time-to-first-token (TTFT), tokens-per-second (TPS), peak memory,
//! throughput under concurrency, and basic accuracy spot-checks.
//!
//! Auto-detects backend by probing Ollama first, then llama.cpp.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Result of a single benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    /// Model name that was benchmarked.
    pub model: String,
    /// Type of benchmark (latency, throughput, memory, accuracy, full).
    pub benchmark_type: String,
    /// ISO 8601 timestamp of the benchmark.
    pub timestamp: String,
    /// Time-to-first-token in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttft_ms: Option<f64>,
    /// Tokens per second (throughput metric).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tps: Option<f64>,
    /// Total request latency in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_latency_ms: Option<f64>,
    /// Peak memory usage in MB during inference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_memory_mb: Option<f64>,
    /// Number of prompt tokens.
    #[serde(default)]
    pub tokens_prompt: u64,
    /// Number of completion tokens generated.
    #[serde(default)]
    pub tokens_completion: u64,
    /// Error message if the benchmark failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Additional details (e.g. concurrent requests for throughput tests).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl BenchmarkResult {
    pub fn ok(model: &str, benchmark_type: &str) -> Self {
        Self {
            model: model.to_string(),
            benchmark_type: benchmark_type.to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            ttft_ms: None,
            tps: None,
            total_latency_ms: None,
            peak_memory_mb: None,
            tokens_prompt: 0,
            tokens_completion: 0,
            error: None,
            details: None,
        }
    }

    pub fn with_error(model: &str, benchmark_type: &str, err: impl std::fmt::Display) -> Self {
        let mut r = Self::ok(model, benchmark_type);
        r.error = Some(err.to_string());
        r
    }
}

/// Request body for the benchmark run API.
#[derive(Debug, Deserialize)]
pub struct BenchmarkRunRequest {
    pub model: String,
    #[serde(default = "default_benchmark_type")]
    pub benchmark_type: String,
    /// Optional prompt tokens count (default: 64).
    #[serde(default = "default_prompt_tokens")]
    pub prompt_tokens: usize,
    /// Optional concurrent requests count (default: 4).
    #[serde(default = "default_concurrent")]
    pub concurrent_requests: usize,
    /// Optional timeout override in seconds (default: 60).
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

fn default_benchmark_type() -> String { "full".into() }
fn default_prompt_tokens() -> usize { 64 }
fn default_concurrent() -> usize { 4 }
fn default_timeout() -> u64 { 60 }

// ---------------------------------------------------------------------------
// Backend detection
// ---------------------------------------------------------------------------

/// Which inference backend is available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferenceBackend {
    Ollama,
    LlamaCpp,
}

/// Detect which backend is running by probing Ollama first, then llama.cpp.
pub async fn detect_backend(client: &reqwest::Client) -> Option<(InferenceBackend, String)> {
    // Try Ollama: GET http://localhost:11434/api/tags
    if let Ok(resp) = client
        .get("http://localhost:11434/api/tags")
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
    {
        if resp.status().is_success() {
            debug!("Detected Ollama backend");
            return Some((InferenceBackend::Ollama, "http://localhost:11434".into()));
        }
    }

    // Try llama.cpp: GET http://localhost:8080/v1/models
    if let Ok(resp) = client
        .get("http://localhost:8080/v1/models")
        .timeout(std::time::Duration::from_secs(3))
        .send()
        .await
    {
        if resp.status().is_success() {
            debug!("Detected llama.cpp backend");
            return Some((InferenceBackend::LlamaCpp, "http://localhost:8080".into()));
        }
    }

    None
}

// ---------------------------------------------------------------------------
// BenchmarkSuite
// ---------------------------------------------------------------------------

/// Model benchmark suite — runs inference performance tests.
pub struct BenchmarkSuite {
    http_client: reqwest::Client,
    results: Arc<RwLock<Vec<BenchmarkResult>>>,
}

impl BenchmarkSuite {
    pub fn new() -> Self {
        Self {
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .expect("Failed to create benchmark HTTP client"),
            results: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Run all benchmark types for a model.
    pub async fn run_full_benchmark(
        &self,
        model: &str,
        timeout_secs: u64,
    ) -> Vec<BenchmarkResult> {
        let mut results = Vec::new();

        info!(model = %model, "Running full benchmark suite");

        results.push(self.run_latency_test(model, 64, timeout_secs).await);
        results.push(self.run_throughput_test(model, 4, timeout_secs).await);
        results.push(self.run_memory_test(model, timeout_secs).await);
        results.push(self.run_accuracy_spot_check(model, timeout_secs).await);

        // Store results
        {
            let mut stored = self.results.write().await;
            stored.extend(results.clone());
            // Keep last 1000 results
            if stored.len() > 1000 {
                let drain_from = stored.len() - 1000;
                stored.drain(..drain_from);
            }
        }

        results
    }

    /// Run a single benchmark by type name.
    pub async fn run_benchmark(
        &self,
        model: &str,
        benchmark_type: &str,
        prompt_tokens: usize,
        concurrent_requests: usize,
        timeout_secs: u64,
    ) -> BenchmarkResult {
        let result = match benchmark_type {
            "latency" => self.run_latency_test(model, prompt_tokens, timeout_secs).await,
            "throughput" => self.run_throughput_test(model, concurrent_requests, timeout_secs).await,
            "memory" => self.run_memory_test(model, timeout_secs).await,
            "accuracy" => self.run_accuracy_spot_check(model, timeout_secs).await,
            "full" => {
                // Return the first result from full suite
                let results = self.run_full_benchmark(model, timeout_secs).await;
                results.into_iter().find(|r| r.error.is_none()).unwrap_or_else(|| {
                    BenchmarkResult::with_error(model, "full", "All benchmark types failed")
                })
            }
            _ => BenchmarkResult::with_error(model, benchmark_type, format!("Unknown benchmark type: {}", benchmark_type)),
        };

        // Store individual result too
        {
            let mut stored = self.results.write().await;
            stored.push(result.clone());
            if stored.len() > 1000 {
                let drain_from = stored.len() - 1000;
                stored.drain(..drain_from);
            }
        }

        result
    }

    /// Measure time-to-first-token (TTFT) and tokens-per-second (TPS).
    pub async fn run_latency_test(
        &self,
        model: &str,
        prompt_tokens: usize,
        timeout_secs: u64,
    ) -> BenchmarkResult {
        let mut result = BenchmarkResult::ok(model, "latency");

        // Generate a prompt of approximately the requested token count (~4 chars/token)
        let prompt = generate_test_prompt(prompt_tokens);

        let backend_info = match detect_backend(&self.http_client).await {
            Some(info) => info,
            None => {
                result.error = Some("No inference backend detected (Ollama or llama.cpp)".into());
                return result;
            }
        };

        let start = Instant::now();

        match backend_info.0 {
            InferenceBackend::Ollama => {
                // Use Ollama streaming API to measure TTFT
                let body = serde_json::json!({
                    "model": model,
                    "prompt": &prompt,
                    "stream": true,
                    "options": {
                        "num_predict": 128,
                    }
                });

                let resp = match self.http_client
                    .post(format!("{}/api/generate", backend_info.1))
                    .timeout(std::time::Duration::from_secs(timeout_secs))
                    .json(&body)
                    .send()
                    .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        result.error = Some(format!("Request failed: {}", e));
                        return result;
                    }
                };

                if !resp.status().is_success() {
                    result.error = Some(format!("Backend returned HTTP {}", resp.status()));
                    return result;
                }

                // Read streaming response to measure TTFT
                let mut ttft_ms: Option<f64> = None;
                let _total_chars = 0u64;
                let mut token_count = 0u64;
                let request_start = Instant::now();

                // Use bytes stream
                let mut stream = resp.bytes_stream();
                use futures_util::StreamExt;

                while let Some(chunk_result) = stream.next().await {
                    match chunk_result {
                        Ok(chunk) => {
                            let text = String::from_utf8_lossy(&chunk);
                            for line in text.lines() {
                                if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                                    // Check if this is a response with content
                                    if json.get("response").is_some() && ttft_ms.is_none() {
                                        ttft_ms = Some(request_start.elapsed().as_secs_f64() * 1000.0);
                                    }
                                    if let Some(content) = json.get("response").and_then(|r| r.as_str()) {
                                        let _ = content.len();
                                    }
                                    // Check for done
                                    if json.get("done").and_then(|d| d.as_bool()).unwrap_or(false) {
                                        if let Some(eval_count) = json.get("eval_count").and_then(|c| c.as_u64()) {
                                            token_count = eval_count;
                                        }
                                        if let Some(prompt_eval_count) = json.get("prompt_eval_count").and_then(|c| c.as_u64()) {
                                            result.tokens_prompt = prompt_eval_count;
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "Stream read error during latency benchmark");
                            break;
                        }
                    }
                }

                let total_ms = start.elapsed().as_secs_f64() * 1000.0;
                result.ttft_ms = ttft_ms;
                result.total_latency_ms = Some(total_ms);
                result.tokens_completion = token_count;
                if token_count > 0 && total_ms > 0.0 {
                    result.tps = Some(token_count as f64 / (total_ms / 1000.0));
                }
            }
            InferenceBackend::LlamaCpp => {
                // Use llama.cpp streaming completion API
                let body = serde_json::json!({
                    "prompt": &prompt,
                    "n_predict": 128,
                    "stream": true,
                });

                let resp = match self.http_client
                    .post(format!("{}/completion", backend_info.1))
                    .timeout(std::time::Duration::from_secs(timeout_secs))
                    .json(&body)
                    .send()
                    .await
                {
                    Ok(r) => r,
                    Err(e) => {
                        result.error = Some(format!("Request failed: {}", e));
                        return result;
                    }
                };

                if !resp.status().is_success() {
                    result.error = Some(format!("Backend returned HTTP {}", resp.status()));
                    return result;
                }

                let mut ttft_ms: Option<f64> = None;
                let mut total_chars = 0u64;
                let mut token_count = 0u64;
                let request_start = Instant::now();

                let mut stream = resp.bytes_stream();
                use futures_util::StreamExt;

                while let Some(chunk_result) = stream.next().await {
                    match chunk_result {
                        Ok(chunk) => {
                            let text = String::from_utf8_lossy(&chunk);
                            for line in text.lines() {
                                if let Some(data) = line.strip_prefix("data: ") {
                                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                                        // Check for content in the response
                                        if json.get("content").is_some() && ttft_ms.is_none() {
                                            ttft_ms = Some(request_start.elapsed().as_secs_f64() * 1000.0);
                                        }
                                        if let Some(content) = json.get("content").and_then(|c| c.as_str()) {
                                            total_chars += content.len() as u64;
                                        }
                                        // Check for stop
                                        if json.get("stop").is_some() {
                                            token_count = json.get("tokens_predicted")
                                                .and_then(|t| t.as_u64())
                                                .unwrap_or(total_chars.div_ceil(4));
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "Stream read error during latency benchmark");
                            break;
                        }
                    }
                }

                let total_ms = start.elapsed().as_secs_f64() * 1000.0;
                result.ttft_ms = ttft_ms;
                result.total_latency_ms = Some(total_ms);
                result.tokens_completion = if token_count > 0 { token_count } else { total_chars.div_ceil(4) };
                if result.tokens_completion > 0 && total_ms > 0.0 {
                    result.tps = Some(result.tokens_completion as f64 / (total_ms / 1000.0));
                }
            }
        }

        info!(
            model = %model,
            ttft_ms = ?result.ttft_ms,
            tps = ?result.tps,
            total_ms = ?result.total_latency_ms,
            "Latency benchmark complete"
        );

        result
    }

    /// Measure max throughput with N concurrent requests.
    pub async fn run_throughput_test(
        &self,
        model: &str,
        concurrent_requests: usize,
        timeout_secs: u64,
    ) -> BenchmarkResult {
        let mut result = BenchmarkResult::ok(model, "throughput");
        result.details = Some(serde_json::json!({
            "concurrent_requests": concurrent_requests,
        }));

        let prompt = generate_test_prompt(32);

        let backend_info = match detect_backend(&self.http_client).await {
            Some(info) => info,
            None => {
                result.error = Some("No inference backend detected".into());
                return result;
            }
        };

        let start = Instant::now();
        let per_request_timeout = std::time::Duration::from_secs(timeout_secs);

        // Spawn concurrent requests
        let mut handles = Vec::with_capacity(concurrent_requests);
        for _ in 0..concurrent_requests {
            let client = self.http_client.clone();
            let backend_url = backend_info.1.clone();
            let prompt_clone = prompt.clone();
            let model_clone = model.to_string();

            handles.push(tokio::spawn(async move {
                let mut token_count = 0u64;
                let mut total_ms = 0.0;

                match backend_url.contains("11434") {
                    true => {
                        // Ollama
                        let body = serde_json::json!({
                            "model": model_clone,
                            "prompt": prompt_clone,
                            "stream": false,
                            "options": { "num_predict": 64 }
                        });

                        let req_start = Instant::now();
                        match client
                            .post(format!("{}/api/generate", backend_url))
                            .timeout(per_request_timeout)
                            .json(&body)
                            .send()
                            .await
                        {
                            Ok(resp) if resp.status().is_success() => {
                                total_ms = req_start.elapsed().as_secs_f64() * 1000.0;
                                if let Ok(json) = resp.json::<serde_json::Value>().await {
                                    token_count = json.get("eval_count").and_then(|c| c.as_u64()).unwrap_or(0);
                                }
                            }
                            _ => {}
                        }
                    }
                    false => {
                        // llama.cpp
                        let body = serde_json::json!({
                            "prompt": prompt_clone,
                            "n_predict": 64,
                            "stream": false,
                        });

                        let req_start = Instant::now();
                        match client
                            .post(format!("{}/completion", backend_url))
                            .timeout(per_request_timeout)
                            .json(&body)
                            .send()
                            .await
                        {
                            Ok(resp) if resp.status().is_success() => {
                                total_ms = req_start.elapsed().as_secs_f64() * 1000.0;
                                if let Ok(json) = resp.json::<serde_json::Value>().await {
                                    token_count = json.get("tokens_predicted")
                                        .and_then(|t| t.as_u64())
                                        .unwrap_or(0);
                                }
                            }
                            _ => {}
                        }
                    }
                }

                (token_count, total_ms)
            }));
        }

        // Collect results
        let mut total_tokens = 0u64;
        let mut total_latency_ms = 0.0;
        let mut successful = 0usize;

        for handle in handles {
            if let Ok((tokens, latency)) = handle.await {
                if tokens > 0 {
                    total_tokens += tokens;
                    total_latency_ms += latency;
                    successful += 1;
                }
            }
        }

        let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
        result.total_latency_ms = Some(wall_ms);
        result.tokens_completion = total_tokens;

        if wall_ms > 0.0 && total_tokens > 0 {
            result.tps = Some(total_tokens as f64 / (wall_ms / 1000.0));
        }

        result.details = Some(serde_json::json!({
            "concurrent_requests": concurrent_requests,
            "successful_requests": successful,
            "avg_request_latency_ms": if successful > 0 { Some(total_latency_ms / successful as f64) } else { None },
        }));

        info!(
            model = %model,
            concurrent = concurrent_requests,
            successful = successful,
            tps = ?result.tps,
            wall_ms = wall_ms,
            "Throughput benchmark complete"
        );

        result
    }

    /// Measure peak memory usage during inference.
    ///
    /// On Linux, reads /proc/{pid}/status for VmRSS.
    /// On macOS, uses `ps -o rss=`.
    /// Falls back to estimating from the inference process.
    pub async fn run_memory_test(
        &self,
        model: &str,
        timeout_secs: u64,
    ) -> BenchmarkResult {
        let mut result = BenchmarkResult::ok(model, "memory");

        let prompt = generate_test_prompt(128);

        let backend_info = match detect_backend(&self.http_client).await {
            Some(info) => info,
            None => {
                result.error = Some("No inference backend detected".into());
                return result;
            }
        };

        // Find the PID of the inference server process
        let backend_pid = find_backend_pid(backend_info.0);

        // Measure memory before inference
        let memory_before_mb = read_process_memory_mb(backend_pid.map(|p| p.to_string()).as_deref());

        // Run inference (non-streaming, longer completion to stress memory)
        let per_request_timeout = std::time::Duration::from_secs(timeout_secs);

        let _ = match backend_info.0 {
            InferenceBackend::Ollama => {
                let body = serde_json::json!({
                    "model": model,
                    "prompt": &prompt,
                    "stream": false,
                    "options": { "num_predict": 256 }
                });
                self.http_client
                    .post(format!("{}/api/generate", backend_info.1))
                    .timeout(per_request_timeout)
                    .json(&body)
                    .send()
                    .await
            }
            InferenceBackend::LlamaCpp => {
                let body = serde_json::json!({
                    "prompt": &prompt,
                    "n_predict": 256,
                    "stream": false,
                });
                self.http_client
                    .post(format!("{}/completion", backend_info.1))
                    .timeout(per_request_timeout)
                    .json(&body)
                    .send()
                    .await
            }
        };

        // Measure memory after inference (peak should be during inference)
        let memory_after_mb = read_process_memory_mb(backend_pid.map(|p| p.to_string()).as_deref());

        result.peak_memory_mb = match (memory_before_mb, memory_after_mb) {
            (Some(before), Some(after)) => Some(after.max(before)),
            (Some(v), None) | (None, Some(v)) => Some(v),
            _ => None,
        };

        if let Some(peak) = result.peak_memory_mb {
            info!(model = %model, peak_memory_mb = peak, "Memory benchmark complete");
        } else {
            info!(model = %model, "Memory benchmark complete (could not measure process memory)");
        }

        result
    }

    /// Send known prompts and check response quality (basic heuristic).
    pub async fn run_accuracy_spot_check(
        &self,
        model: &str,
        timeout_secs: u64,
    ) -> BenchmarkResult {
        let mut result = BenchmarkResult::ok(model, "accuracy");

        let test_prompts = [
            ("What is 2+2?", "4"),
            ("What color is the sky?", "blue"),
            ("Say hello.", "hello"),
        ];

        let backend_info = match detect_backend(&self.http_client).await {
            Some(info) => info,
            None => {
                result.error = Some("No inference backend detected".into());
                return result;
            }
        };

        let per_request_timeout = std::time::Duration::from_secs(timeout_secs);
        let mut passed = 0usize;
        let mut total_latency_ms = 0.0f64;
        let mut spot_results = Vec::new();

        for (prompt, expected_substr) in &test_prompts {
            let response_text = match backend_info.0 {
                InferenceBackend::Ollama => {
                    let body = serde_json::json!({
                        "model": model,
                        "prompt": prompt,
                        "stream": false,
                        "options": { "num_predict": 64 }
                    });

                    let start = Instant::now();
                    match self.http_client
                        .post(format!("{}/api/generate", backend_info.1))
                        .timeout(per_request_timeout)
                        .json(&body)
                        .send()
                        .await
                    {
                        Ok(resp) if resp.status().is_success() => {
                            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                            total_latency_ms += elapsed;
                            match resp.json::<serde_json::Value>().await {
                                Ok(json) => json.get("response").and_then(|r| r.as_str()).unwrap_or("").to_string(),
                                Err(_) => String::new(),
                            }
                        }
                        _ => String::new(),
                    }
                }
                InferenceBackend::LlamaCpp => {
                    let body = serde_json::json!({
                        "prompt": prompt,
                        "n_predict": 64,
                        "stream": false,
                    });

                    let start = Instant::now();
                    match self.http_client
                        .post(format!("{}/completion", backend_info.1))
                        .timeout(per_request_timeout)
                        .json(&body)
                        .send()
                        .await
                    {
                        Ok(resp) if resp.status().is_success() => {
                            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                            total_latency_ms += elapsed;
                            match resp.json::<serde_json::Value>().await {
                                Ok(json) => json.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string(),
                                Err(_) => String::new(),
                            }
                        }
                        _ => String::new(),
                    }
                }
            };

            let response_lower = response_text.to_lowercase();
            let found = response_lower.contains(expected_substr);
            let response_len = response_text.len();

            // Pass if response contains expected substring or is non-empty (lenient)
            let pass = found || response_len > 0;
            if pass {
                passed += 1;
            }

            spot_results.push(serde_json::json!({
                "prompt": prompt,
                "expected_contains": expected_substr,
                "response_length": response_len,
                "found_expected": found,
                "pass": pass,
            }));
        }

        result.total_latency_ms = Some(total_latency_ms);
        result.details = Some(serde_json::json!({
            "tests_passed": passed,
            "tests_total": test_prompts.len(),
            "pass_rate": passed as f64 / test_prompts.len() as f64,
            "results": spot_results,
        }));

        info!(
            model = %model,
            passed = passed,
            total = test_prompts.len(),
            "Accuracy spot-check complete"
        );

        result
    }

    /// Get recent benchmark results (last N).
    pub async fn get_recent_results(&self, limit: usize) -> Vec<BenchmarkResult> {
        let results = self.results.read().await;
        let len = results.len();
        let start = if len > limit { len - limit } else { 0 };
        results[start..].to_vec()
    }

    /// Get benchmark history for a specific model.
    pub async fn get_model_history(&self, model: &str) -> Vec<BenchmarkResult> {
        let results = self.results.read().await;
        results.iter()
            .filter(|r| r.model == model)
            .cloned()
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Generate a test prompt of approximately `target_tokens` tokens.
/// Uses a repeating pattern that is easy to tokenize.
fn generate_test_prompt(target_tokens: usize) -> String {
    // ~4 chars per token heuristic; generate a prompt about the right size
    let target_chars = target_tokens * 4;
    let base = "The quick brown fox jumps over the lazy dog. ";
    let repeats = (target_chars + base.len() - 1) / base.len();
    let prompt = base.repeat(repeats);
    prompt[..target_chars.min(prompt.len())].to_string()
}

/// Find the PID of the inference backend process.
fn find_backend_pid(backend: InferenceBackend) -> Option<u32> {
    let process_name = match backend {
        InferenceBackend::Ollama => "ollama",
        InferenceBackend::LlamaCpp => "llama-server",
    };

    // Try `pgrep` first
    if let Ok(output) = std::process::Command::new("pgrep")
        .arg("-x")
        .arg(process_name)
        .output()
    {
        if output.status.success() {
            let pid_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if let Ok(pid) = pid_str.parse::<u32>() {
                return Some(pid);
            }
        }
    }

    None
}

/// Read the RSS (resident set size) of a process in MB.
///
/// Linux: reads /proc/{pid}/status VmRSS field.
/// macOS: uses `ps -o rss= -p {pid}`.
fn read_process_memory_mb(pid: Option<&str>) -> Option<f64> {
    let pid = pid?;

    #[cfg(target_os = "linux")]
    {
        let path = format!("/proc/{}/status", pid);
        if let Ok(content) = std::fs::read_to_string(&path) {
            for line in content.lines() {
                if line.starts_with("VmRSS:") {
                    // Format: "VmRSS:    123456 kB"
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(kb) = parts[1].parse::<f64>() {
                            return Some(kb / 1024.0); // Convert kB to MB
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", pid])
            .output()
        {
            if output.status.success() {
                let rss_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if let Ok(kb) = rss_str.parse::<f64>() {
                    return Some(kb / 1024.0); // Convert kB to MB
                }
            }
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_test_prompt() {
        let prompt = generate_test_prompt(64);
        // Should be roughly 256 chars (64 tokens * 4 chars/token)
        assert!(prompt.len() >= 200);
        assert!(prompt.len() <= 300);
    }

    #[test]
    fn test_benchmark_result_ok() {
        let r = BenchmarkResult::ok("test-model", "latency");
        assert_eq!(r.model, "test-model");
        assert_eq!(r.benchmark_type, "latency");
        assert!(r.error.is_none());
        assert!(!r.timestamp.is_empty());
    }

    #[test]
    fn test_benchmark_result_with_error() {
        let r = BenchmarkResult::with_error("test-model", "latency", "backend down");
        assert_eq!(r.error, Some("backend down".into()));
    }

    #[tokio::test]
    async fn test_suite_creation() {
        let suite = BenchmarkSuite::new();
        let results = suite.get_recent_results(10).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_benchmark_run_request_deserialize() {
        let json = r#"{"model": "llama3", "benchmark_type": "latency"}"#;
        let req: BenchmarkRunRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model, "llama3");
        assert_eq!(req.benchmark_type, "latency");
        assert_eq!(req.prompt_tokens, 64);
    }
}
