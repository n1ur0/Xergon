//! Auto-Healing System — monitors provider health and takes corrective action.
//!
//! Periodically checks:
//! - Inference server (Ollama / llama.cpp) liveness
//! - Disk space for model caches
//! - Ergo node sync status
//! - Relay connectivity
//!
//! Corrective actions:
//! - Restart crashed inference servers
//! - Clear disk space by evicting unpinned models
//! - Re-register with relay on connection loss

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Auto-heal configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AutoHealConfig {
    /// Enable the auto-heal system (default: false).
    #[serde(default)]
    pub enabled: bool,
    /// How often to run diagnostic checks (seconds, default: 300 = 5 min).
    #[serde(default = "default_check_interval")]
    pub check_interval_secs: u64,
    /// Attempt to restart crashed inference servers (default: true).
    #[serde(default = "default_true")]
    pub auto_restart_inference: bool,
    /// Evict unpinned models when disk is low (default: true).
    #[serde(default = "default_true")]
    pub auto_evict_models: bool,
    /// Minimum free disk space in GB before eviction triggers (default: 5).
    #[serde(default = "default_min_disk_gb")]
    pub min_disk_gb: f64,
    /// Model cache directory to monitor (default: auto-detect).
    #[serde(default)]
    pub model_cache_dir: String,
}

fn default_check_interval() -> u64 { 300 }
fn default_true() -> bool { true }
fn default_min_disk_gb() -> f64 { 5.0 }

impl Default for AutoHealConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            check_interval_secs: default_check_interval(),
            auto_restart_inference: default_true(),
            auto_evict_models: default_true(),
            min_disk_gb: default_min_disk_gb(),
            model_cache_dir: String::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Status of a single diagnostic check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticCheck {
    pub name: String,
    pub status: String, // "ok" | "warning" | "critical"
    pub message: String,
    pub duration_ms: u64,
}

/// A corrective action that was taken.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealAction {
    pub action: String,
    pub target: String,
    pub success: bool,
    pub message: String,
    pub timestamp: String,
}

/// Overall auto-heal status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoHealStatus {
    pub enabled: bool,
    pub last_check: Option<String>,
    pub last_check_duration_ms: Option<u64>,
    pub checks: Vec<DiagnosticCheck>,
    pub actions_taken: Vec<HealAction>,
    pub total_checks_run: u64,
    pub total_actions_taken: u64,
    pub running: bool,
}

// ---------------------------------------------------------------------------
// AutoHealer
// ---------------------------------------------------------------------------

/// Auto-healing system for the Xergon agent.
pub struct AutoHealer {
    config: AutoHealConfig,
    http_client: reqwest::Client,
    status: Arc<RwLock<AutoHealStatus>>,
    running: Arc<AtomicBool>,
    /// Ergo node REST URL for sync checks.
    ergo_node_url: String,
    /// Relay URL for connectivity checks.
    relay_url: String,
}

impl AutoHealer {
    pub fn new(
        config: AutoHealConfig,
        ergo_node_url: String,
        relay_url: String,
    ) -> Self {
        let enabled = config.enabled;
        Self {
            config,
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("Failed to create auto-heal HTTP client"),
            status: Arc::new(RwLock::new(AutoHealStatus {
                enabled,
                last_check: None,
                last_check_duration_ms: None,
                checks: Vec::new(),
                actions_taken: Vec::new(),
                total_checks_run: 0,
                total_actions_taken: 0,
                running: false,
            })),
            running: Arc::new(AtomicBool::new(false)),
            ergo_node_url,
            relay_url,
        }
    }

    /// Get current status.
    pub async fn get_status(&self) -> AutoHealStatus {
        self.status.read().await.clone()
    }

    /// Get configuration.
    pub fn get_config(&self) -> &AutoHealConfig {
        &self.config
    }

    /// Run a single diagnostic check cycle and take corrective actions.
    pub async fn check_and_heal(&self) -> AutoHealStatus {
        let start = Instant::now();
        let mut checks: Vec<DiagnosticCheck> = Vec::new();
        let mut actions: Vec<HealAction> = Vec::new();

        info!("Running auto-heal diagnostic check");

        // 1. Check Ollama process
        checks.push(self.check_ollama().await);
        if self.config.auto_restart_inference {
            if let Some(action) = self.heal_ollama(&checks).await {
                actions.push(action);
            }
        }

        // 2. Check llama.cpp process
        checks.push(self.check_llama_cpp().await);
        if self.config.auto_restart_inference {
            if let Some(action) = self.heal_llama_cpp(&checks).await {
                actions.push(action);
            }
        }

        // 3. Check disk space
        checks.push(self.check_disk_space().await);
        if self.config.auto_evict_models {
            if let Some(action) = self.heal_disk_space(&checks).await {
                actions.push(action);
            }
        }

        // 4. Check Ergo node sync
        checks.push(self.check_ergo_sync().await);

        // 5. Check relay connectivity
        checks.push(self.check_relay().await);
        if let Some(action) = self.heal_relay(&checks).await {
            actions.push(action);
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        // Update status
        let mut status = self.status.write().await;
        status.last_check = Some(chrono::Utc::now().to_rfc3339());
        status.last_check_duration_ms = Some(duration_ms);
        status.checks = checks;
        status.actions_taken = actions.clone();
        status.total_checks_run += 1;
        status.total_actions_taken += actions.len() as u64;

        let total_critical = status.checks.iter().filter(|c| c.status == "critical").count();
        let total_warning = status.checks.iter().filter(|c| c.status == "warning").count();
        info!(
            duration_ms,
            critical = total_critical,
            warning = total_warning,
            actions = actions.len(),
            "Auto-heal check complete"
        );

        status.clone()
    }

    /// Start the background auto-heal loop.
    pub fn spawn_loop(&self) {
        if !self.config.enabled {
            info!("Auto-heal system disabled (set [auto_heal].enabled = true to enable)");
            return;
        }

        self.running.store(true, Ordering::SeqCst);
        let config = self.config.clone();
        let running = self.running.clone();

        // We need to call check_and_heal in the background.
        // Use a weak-like pattern: re-create the caller each iteration.
        // Since AutoHealer is Clone-friendly via Arc, we'll use a spawn with captured state.

        let http_client = self.http_client.clone();
        let status = self.status.clone();
        let ergo_node_url = self.ergo_node_url.clone();
        let relay_url = self.relay_url.clone();
        let model_cache_dir = self.config.model_cache_dir.clone();
        let min_disk_gb = self.config.min_disk_gb;
        let auto_restart_inference = self.config.auto_restart_inference;
        let auto_evict_models = self.config.auto_evict_models;

        tokio::spawn(async move {
            let interval = std::time::Duration::from_secs(config.check_interval_secs);
            // Wait before first check
            tokio::time::sleep(interval).await;

            while running.load(Ordering::SeqCst) {
                let start = Instant::now();
                let mut checks: Vec<DiagnosticCheck> = Vec::new();
                let mut actions: Vec<HealAction> = Vec::new();

                info!("Running auto-heal diagnostic check (background)");

                // 1. Check Ollama
                checks.push(check_ollama_static(&http_client).await);
                if auto_restart_inference {
                    if let Some(action) = heal_ollama_static(&checks, &http_client).await {
                        actions.push(action);
                    }
                }

                // 2. Check llama.cpp
                checks.push(check_llama_cpp_static(&http_client).await);
                if auto_restart_inference {
                    if let Some(action) = heal_llama_cpp_static(&checks, &http_client).await {
                        actions.push(action);
                    }
                }

                // 3. Check disk space
                checks.push(check_disk_space_static(&model_cache_dir).await);
                if auto_evict_models {
                    if let Some(action) = heal_disk_space_static(&checks, min_disk_gb).await {
                        actions.push(action);
                    }
                }

                // 4. Check Ergo node sync
                checks.push(check_ergo_sync_static(&http_client, &ergo_node_url).await);

                // 5. Check relay connectivity
                checks.push(check_relay_static(&http_client, &relay_url).await);
                if let Some(action) = heal_relay_static(&checks, &http_client, &relay_url).await {
                    actions.push(action);
                }

                let duration_ms = start.elapsed().as_millis() as u64;

                let mut status = status.write().await;
                status.last_check = Some(chrono::Utc::now().to_rfc3339());
                status.last_check_duration_ms = Some(duration_ms);
                status.checks = checks;
                status.actions_taken = actions.clone();
                status.total_checks_run += 1;
                status.total_actions_taken += actions.len() as u64;

                let total_critical = status.checks.iter().filter(|c| c.status == "critical").count();
                let total_warning = status.checks.iter().filter(|c| c.status == "warning").count();
                info!(
                    duration_ms,
                    critical = total_critical,
                    warning = total_warning,
                    actions = actions.len(),
                    "Auto-heal background check complete"
                );
                drop(status);

                tokio::time::sleep(interval).await;
            }
        });

        info!(
            interval_secs = config.check_interval_secs,
            "Auto-heal background loop started"
        );
    }

    /// Stop the background loop.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    // -----------------------------------------------------------------------
    // Instance methods (for manual check_and_heal)
    // -----------------------------------------------------------------------

    async fn check_ollama(&self) -> DiagnosticCheck {
        check_ollama_static(&self.http_client).await
    }

    async fn check_llama_cpp(&self) -> DiagnosticCheck {
        check_llama_cpp_static(&self.http_client).await
    }

    async fn check_disk_space(&self) -> DiagnosticCheck {
        check_disk_space_static(&self.config.model_cache_dir).await
    }

    async fn check_ergo_sync(&self) -> DiagnosticCheck {
        check_ergo_sync_static(&self.http_client, &self.ergo_node_url).await
    }

    async fn check_relay(&self) -> DiagnosticCheck {
        check_relay_static(&self.http_client, &self.relay_url).await
    }

    async fn heal_ollama(&self, checks: &[DiagnosticCheck]) -> Option<HealAction> {
        heal_ollama_static(checks, &self.http_client).await
    }

    async fn heal_llama_cpp(&self, checks: &[DiagnosticCheck]) -> Option<HealAction> {
        heal_llama_cpp_static(checks, &self.http_client).await
    }

    async fn heal_disk_space(&self, checks: &[DiagnosticCheck]) -> Option<HealAction> {
        heal_disk_space_static(checks, self.config.min_disk_gb).await
    }

    async fn heal_relay(&self, checks: &[DiagnosticCheck]) -> Option<HealAction> {
        heal_relay_static(checks, &self.http_client, &self.relay_url).await
    }
}

// ---------------------------------------------------------------------------
// Static check functions (used by both instance methods and background loop)
// ---------------------------------------------------------------------------

async fn check_ollama_static(client: &reqwest::Client) -> DiagnosticCheck {
    let start = Instant::now();

    match client
        .get("http://localhost:11434/api/tags")
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            let model_count = resp.json::<serde_json::Value>().await
                .ok()
                .and_then(|v| v.get("models").and_then(|m| m.as_array()).map(|a| a.len()))
                .unwrap_or(0);
            DiagnosticCheck {
                name: "ollama".into(),
                status: "ok".into(),
                message: format!("Ollama is running ({} models)", model_count),
                duration_ms: start.elapsed().as_millis() as u64,
            }
        }
        Ok(resp) => DiagnosticCheck {
            name: "ollama".into(),
            status: "critical".into(),
            message: format!("Ollama returned HTTP {}", resp.status()),
            duration_ms: start.elapsed().as_millis() as u64,
        },
        Err(e) => DiagnosticCheck {
            name: "ollama".into(),
            status: "critical".into(),
            message: format!("Ollama not reachable: {}", e),
            duration_ms: start.elapsed().as_millis() as u64,
        },
    }
}

async fn heal_ollama_static(
    checks: &[DiagnosticCheck],
    _client: &reqwest::Client,
) -> Option<HealAction> {
    let ollama_check = checks.iter().find(|c| c.name == "ollama")?;
    if ollama_check.status != "critical" {
        return None;
    }

    info!("Attempting to restart Ollama...");

    // Try systemd restart first
    let restart_result = std::process::Command::new("systemctl")
        .args(["restart", "--user", "ollama"])
        .output();

    let success = match restart_result {
        Ok(output) => output.status.success(),
        Err(_) => {
            // Fallback: try direct launch
            let direct = std::process::Command::new("ollama")
                .arg("serve")
                .spawn();
            match direct {
                Ok(_) => true,
                Err(e) => {
                    warn!(error = %e, "Failed to restart Ollama");
                    false
                }
            }
        }
    };

    Some(HealAction {
        action: "restart_ollama".into(),
        target: "ollama".into(),
        success,
        message: if success { "Ollama restart initiated".into() } else { "Failed to restart Ollama".into() },
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

async fn check_llama_cpp_static(client: &reqwest::Client) -> DiagnosticCheck {
    let start = Instant::now();

    match client
        .get("http://localhost:8080/v1/models")
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            DiagnosticCheck {
                name: "llama_cpp".into(),
                status: "ok".into(),
                message: "llama.cpp server is running".into(),
                duration_ms: start.elapsed().as_millis() as u64,
            }
        }
        Ok(resp) => DiagnosticCheck {
            name: "llama_cpp".into(),
            status: "critical".into(),
            message: format!("llama.cpp returned HTTP {}", resp.status()),
            duration_ms: start.elapsed().as_millis() as u64,
        },
        Err(e) => DiagnosticCheck {
            name: "llama_cpp".into(),
            status: "warning".into(),
            message: format!("llama.cpp not reachable: {}", e),
            duration_ms: start.elapsed().as_millis() as u64,
        },
    }
}

async fn heal_llama_cpp_static(
    checks: &[DiagnosticCheck],
    _client: &reqwest::Client,
) -> Option<HealAction> {
    let check = checks.iter().find(|c| c.name == "llama_cpp")?;
    if check.status != "critical" && check.status != "warning" {
        return None;
    }

    // llama.cpp doesn't have a standard systemd service, so we log a warning
    // and suggest manual intervention.
    warn!("llama.cpp server is down — auto-restart not available (no standard service manager)");
    Some(HealAction {
        action: "restart_llama_cpp".into(),
        target: "llama_cpp".into(),
        success: false,
        message: "llama.cpp auto-restart not available — requires manual intervention".into(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

async fn check_disk_space_static(cache_dir: &str) -> DiagnosticCheck {
    let start = Instant::now();

    // Determine the directory to check
    let check_dir = if cache_dir.is_empty() {
        // Default: check the Ollama model directory or system temp
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
        format!("{}/.ollama/models", home)
    } else {
        cache_dir.to_string()
    };

    let free_gb = get_disk_free_gb(&check_dir);

    match free_gb {
        Some(gb) => {
            let status = if gb < 1.0 {
                "critical"
            } else if gb < 5.0 {
                "warning"
            } else {
                "ok"
            };
            DiagnosticCheck {
                name: "disk_space".into(),
                status: status.into(),
                message: format!("{:.1} GB free on {}", gb, check_dir),
                duration_ms: start.elapsed().as_millis() as u64,
            }
        }
        None => DiagnosticCheck {
            name: "disk_space".into(),
            status: "warning".into(),
            message: format!("Could not check disk space for {}", check_dir),
            duration_ms: start.elapsed().as_millis() as u64,
        },
    }
}

async fn heal_disk_space_static(
    checks: &[DiagnosticCheck],
    min_disk_gb: f64,
) -> Option<HealAction> {
    let check = checks.iter().find(|c| c.name == "disk_space")?;
    if check.status != "critical" && check.status != "warning" {
        return None;
    }

    info!(min_disk_gb, "Disk space low, attempting model eviction");

    // Try to evict unpinned models via Ollama CLI
    let prune_result = std::process::Command::new("ollama")
        .args(["prune"])
        .output();

    let success = match prune_result {
        Ok(output) => {
            if output.status.success() {
                let msg = String::from_utf8_lossy(&output.stdout).trim().to_string();
                info!(message = %msg, "Ollama prune completed");
                true
            } else {
                let msg = String::from_utf8_lossy(&output.stderr).trim().to_string();
                warn!(message = %msg, "Ollama prune failed");
                false
            }
        }
        Err(e) => {
            warn!(error = %e, "Could not run ollama prune");
            false
        }
    };

    Some(HealAction {
        action: "evict_models".into(),
        target: "disk_space".into(),
        success,
        message: if success { "Evicted unpinned models via ollama prune".into() } else { "Failed to evict models".into() },
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

async fn check_ergo_sync_static(
    client: &reqwest::Client,
    ergo_url: &str,
) -> DiagnosticCheck {
    let start = Instant::now();

    match client
        .get(format!("{}/info", ergo_url.trim_end_matches('/')))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<serde_json::Value>().await {
                Ok(info) => {
                    let is_synced = info.get("isSynced").and_then(|s| s.as_bool()).unwrap_or(false);
                    let height = info.get("fullHeight").and_then(|h| h.as_i64()).unwrap_or(-1);
                    let headers = info.get("headersHeight").and_then(|h| h.as_i64()).unwrap_or(-1);
                    let peers = info.get("peersCount").and_then(|p| p.as_i64()).unwrap_or(0);

                    let status = if !is_synced {
                        "critical"
                    } else if peers < 3 {
                        "warning"
                    } else {
                        "ok"
                    };

                    DiagnosticCheck {
                        name: "ergo_node".into(),
                        status: status.into(),
                        message: format!(
                            "Height: {}/{}, Synced: {}, Peers: {}",
                            height, headers, is_synced, peers
                        ),
                        duration_ms: start.elapsed().as_millis() as u64,
                    }
                }
                Err(e) => DiagnosticCheck {
                    name: "ergo_node".into(),
                    status: "warning".into(),
                    message: format!("Failed to parse Ergo node info: {}", e),
                    duration_ms: start.elapsed().as_millis() as u64,
                },
            }
        }
        Ok(resp) => DiagnosticCheck {
            name: "ergo_node".into(),
            status: "critical".into(),
            message: format!("Ergo node returned HTTP {}", resp.status()),
            duration_ms: start.elapsed().as_millis() as u64,
        },
        Err(e) => DiagnosticCheck {
            name: "ergo_node".into(),
            status: "critical".into(),
            message: format!("Ergo node not reachable: {}", e),
            duration_ms: start.elapsed().as_millis() as u64,
        },
    }
}

async fn check_relay_static(
    client: &reqwest::Client,
    relay_url: &str,
) -> DiagnosticCheck {
    if relay_url.is_empty() {
        return DiagnosticCheck {
            name: "relay".into(),
            status: "ok".into(),
            message: "No relay URL configured".into(),
            duration_ms: 0,
        };
    }

    let start = Instant::now();

    match client
        .get(format!("{}/health", relay_url.trim_end_matches('/')))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => DiagnosticCheck {
            name: "relay".into(),
            status: "ok".into(),
            message: format!("Relay reachable at {}", relay_url),
            duration_ms: start.elapsed().as_millis() as u64,
        },
        Ok(resp) => DiagnosticCheck {
            name: "relay".into(),
            status: "warning".into(),
            message: format!("Relay returned HTTP {}", resp.status()),
            duration_ms: start.elapsed().as_millis() as u64,
        },
        Err(e) => DiagnosticCheck {
            name: "relay".into(),
            status: "warning".into(),
            message: format!("Relay not reachable: {}", e),
            duration_ms: start.elapsed().as_millis() as u64,
        },
    }
}

async fn heal_relay_static(
    checks: &[DiagnosticCheck],
    _client: &reqwest::Client,
    _relay_url: &str,
) -> Option<HealAction> {
    let check = checks.iter().find(|c| c.name == "relay")?;
    if check.status != "warning" && check.status != "critical" {
        return None;
    }

    // Relay re-registration is handled by the relay_client heartbeat loop.
    // Here we just log the issue so the operator is aware.
    info!("Relay connectivity issue detected — the heartbeat loop will handle re-registration");

    Some(HealAction {
        action: "relay_reconnect".into(),
        target: "relay".into(),
        success: true,
        message: "Relay reconnect delegated to heartbeat loop".into(),
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

/// Get free disk space in GB for the filesystem containing the given path.
fn get_disk_free_gb(path: &str) -> Option<f64> {
    #[cfg(target_os = "linux")]
    {
        // Use statvfs via unsafe (or fall back to df)
        // Simple approach: use `df` command
        if let Ok(output) = std::process::Command::new("df")
            .args(["--output=avail", "-B1", path])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                // First line is header, second line is the value
                let lines: Vec<&str> = stdout.lines().collect();
                if lines.len() >= 2 {
                    let avail_str = lines[1].trim();
                    if let Ok(bytes) = avail_str.parse::<f64>() {
                        return Some(bytes / (1024.0 * 1024.0 * 1024.0));
                    }
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("df")
            .args(["-k", path])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let lines: Vec<&str> = stdout.lines().collect();
                if lines.len() >= 2 {
                    let parts: Vec<&str> = lines[1].split_whitespace().collect();
                    if parts.len() >= 4 {
                        if let Ok(kb) = parts[3].parse::<f64>() {
                            return Some(kb / (1024.0 * 1024.0));
                        }
                    }
                }
            }
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = path;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_heal_config_defaults() {
        let config = AutoHealConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.check_interval_secs, 300);
        assert!(config.auto_restart_inference);
        assert!(config.auto_evict_models);
        assert_eq!(config.min_disk_gb, 5.0);
    }

    #[test]
    fn test_diagnostic_check_serialization() {
        let check = DiagnosticCheck {
            name: "test".into(),
            status: "ok".into(),
            message: "all good".into(),
            duration_ms: 5,
        };
        let json = serde_json::to_string(&check).unwrap();
        assert!(json.contains("\"test\""));
        assert!(json.contains("\"ok\""));
    }

    #[test]
    fn test_heal_action_serialization() {
        let action = HealAction {
            action: "restart".into(),
            target: "ollama".into(),
            success: true,
            message: "restarted".into(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"restart\""));
        assert!(json.contains("true"));
    }

    #[tokio::test]
    async fn test_auto_heal_status() {
        let healer = AutoHealer::new(
            AutoHealConfig::default(),
            "http://127.0.0.1:9053".into(),
            String::new(),
        );
        let status = healer.get_status().await;
        assert!(!status.enabled);
        assert_eq!(status.total_checks_run, 0);
    }
}
