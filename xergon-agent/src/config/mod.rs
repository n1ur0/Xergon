use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct AgentConfig {
    pub ergo_node: ErgoNodeConfig,
    pub xergon: XergonConfig,
    pub peer_discovery: PeerDiscoveryConfig,
    pub api: ApiConfig,
    #[serde(default)]
    pub settlement: SettlementConfig,
    #[serde(default)]
    pub llama_server: LlamaServerConfig,
    #[serde(default)]
    pub inference: InferenceConfig,
    #[serde(default)]
    pub relay: crate::relay_client::RelayClientConfig,
}

/// llama-server (llama.cpp) configuration for AI inference backend detection.
#[derive(Debug, Clone, Deserialize)]
pub struct LlamaServerConfig {
    /// Base URL of the llama-server instance (default: http://127.0.0.1:8080)
    #[serde(default = "default_llama_server_url")]
    pub url: String,
    /// How often to probe llama-server health (seconds, default: 60)
    #[serde(default = "default_llama_health_interval")]
    pub health_check_interval_secs: u64,
}

fn default_llama_server_url() -> String {
    "http://127.0.0.1:8080".into()
}

fn default_llama_health_interval() -> u64 { 60 }

impl Default for LlamaServerConfig {
    fn default() -> Self {
        Self {
            url: default_llama_server_url(),
            health_check_interval_secs: default_llama_health_interval(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ErgoNodeConfig {
    /// REST API URL of the local Ergo node (default: http://127.0.0.1:9053)
    #[serde(default = "default_ergo_url")]
    pub rest_url: String,
}

fn default_ergo_url() -> String {
    "http://127.0.0.1:9053".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct XergonConfig {
    /// Provider ID (e.g., "Xergon_LT")
    pub provider_id: String,
    /// Provider display name
    pub provider_name: String,
    /// Provider region (e.g., "us-east")
    pub region: String,
    /// Ergo address for PoNW identity
    pub ergo_address: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PeerDiscoveryConfig {
    /// How often to run a full peer discovery cycle (seconds)
    #[serde(default = "default_discovery_interval")]
    pub discovery_interval_secs: u64,
    /// Timeout for probing a single peer (seconds)
    #[serde(default = "default_probe_timeout")]
    pub probe_timeout_secs: u64,
    /// Port where Xergon agents expose their status endpoint
    #[serde(default = "default_xergon_port")]
    pub xergon_agent_port: u16,
    /// Max concurrent peer probes
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_probes: usize,
    /// Maximum number of Ergo peers to probe per cycle
    #[serde(default = "default_max_peers_per_cycle")]
    pub max_peers_per_cycle: usize,
    /// Path to persist known Xergon peers between restarts
    #[serde(default)]
    pub peers_file: Option<PathBuf>,
}

fn default_discovery_interval() -> u64 { 120 }
fn default_probe_timeout() -> u64 { 5 }
fn default_xergon_port() -> u16 { 9099 }
fn default_max_concurrent() -> usize { 10 }
fn default_max_peers_per_cycle() -> usize { 50 }

#[derive(Debug, Clone, Deserialize)]
pub struct ApiConfig {
    /// Address to bind the Xergon agent REST API
    #[serde(default = "default_api_addr")]
    pub listen_addr: String,
    /// Optional API key for management endpoints (/xergon/*).
    /// If empty, management endpoints are open (dev mode).
    /// If set, requests must include `Authorization: Bearer <api_key>`.
    #[serde(default)]
    pub api_key: String,
}

fn default_api_addr() -> String {
    "0.0.0.0:9099".into()
}

/// ERG settlement configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct SettlementConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_settlement_interval")]
    pub interval_secs: u64,
    #[serde(default)]
    pub ledger_file: Option<PathBuf>,
    #[serde(default = "default_settlement_dry_run")]
    pub dry_run: bool,
    #[serde(default = "default_min_settlement_usd")]
    #[allow(dead_code)] // TODO: will be used for settlement threshold enforcement
    pub min_settlement_usd: f64,
}

impl Default for SettlementConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_secs: default_settlement_interval(),
            ledger_file: None,
            dry_run: default_settlement_dry_run(),
            min_settlement_usd: default_min_settlement_usd(),
        }
    }
}

fn default_settlement_interval() -> u64 { 86400 }
fn default_settlement_dry_run() -> bool { true }
fn default_min_settlement_usd() -> f64 { 0.10 }

/// Inference proxy configuration.
///
/// Controls the OpenAI-compatible inference endpoint that xergon-agent
/// exposes to xergon-relay. Proxies requests to a local LLM backend
/// (Ollama, llama.cpp server, etc.).
#[derive(Debug, Clone, Deserialize)]
pub struct InferenceConfig {
    /// Enable the inference proxy endpoint (default: true)
    #[serde(default = "default_inference_enabled")]
    pub enabled: bool,
    /// Base URL of the LLM backend (default: http://127.0.0.1:11434 for Ollama)
    #[serde(default = "default_inference_url")]
    pub url: String,
    /// Request timeout in seconds (default: 120)
    #[serde(default = "default_inference_timeout")]
    pub timeout_secs: u64,
    /// Optional API key required to access inference endpoints.
    /// If empty, inference endpoints are open (suitable for local/trusted networks).
    /// If set, requests must include `Authorization: Bearer <key>` header.
    #[serde(default)]
    pub api_key: String,
}

fn default_inference_enabled() -> bool { true }
fn default_inference_url() -> String { "http://127.0.0.1:11434".into() }
fn default_inference_timeout() -> u64 { 120 }

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            enabled: default_inference_enabled(),
            url: default_inference_url(),
            timeout_secs: default_inference_timeout(),
            api_key: String::new(),
        }
    }
}

impl AgentConfig {
    /// Load configuration from the given path (or `XERGON_CONFIG` env var, or `config.toml`).
    /// Environment variables with `XERGON__` prefix override file values.
    pub fn load_from(path: Option<PathBuf>) -> anyhow::Result<Self> {
        let config_path = path
            .or_else(|| std::env::var("XERGON_CONFIG").ok().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("config.toml"));

        if config_path.exists() {
            let settings = config::Config::builder()
                .add_source(config::File::from(config_path))
                .add_source(
                    config::Environment::with_prefix("XERGON")
                        .separator("__")
                        .try_parsing(true),
                )
                .build()?;
            Ok(settings.try_deserialize()?)
        } else {
            let settings = config::Config::builder()
                .add_source(
                    config::Environment::with_prefix("XERGON")
                        .separator("__")
                        .try_parsing(true),
                )
                .build()?;
            Ok(settings.try_deserialize()?)
        }
    }

    /// Load configuration using default path resolution (env var or `config.toml`).
    pub fn load() -> anyhow::Result<Self> {
        Self::load_from(None)
    }
}

// ---------------------------------------------------------------------------
// Tests — W7 fixes: ApiConfig / InferenceConfig api_key field
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_config_has_api_key_field() {
        let cfg = ApiConfig {
            listen_addr: "0.0.0.0:9099".into(),
            api_key: "secret-key".into(),
        };
        assert_eq!(cfg.api_key, "secret-key");
    }

    #[test]
    fn test_api_config_api_key_default_is_empty() {
        let cfg: ApiConfig = serde_json::from_value(serde_json::json!({
            "listen_addr": "0.0.0.0:9099"
        }))
        .expect("deserialization should succeed");
        assert!(cfg.api_key.is_empty());
    }

    #[test]
    fn test_inference_config_has_api_key_field() {
        let cfg = InferenceConfig {
            enabled: true,
            url: "http://localhost:11434".into(),
            timeout_secs: 120,
            api_key: "inference-secret".into(),
        };
        assert_eq!(cfg.api_key, "inference-secret");
    }

    #[test]
    fn test_inference_config_default_api_key_is_empty() {
        let cfg = InferenceConfig::default();
        assert!(cfg.api_key.is_empty());
    }

    #[test]
    fn test_inference_config_default_values() {
        let cfg = InferenceConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.url, "http://127.0.0.1:11434");
        assert_eq!(cfg.timeout_secs, 120);
    }
}
