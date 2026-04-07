use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Serialize)]
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
    pub pricing: PricingConfig,
    #[serde(default)]
    pub relay: crate::relay_client::RelayClientConfig,
    #[serde(default)]
    pub chain: ChainTxConfig,
    #[serde(default)]
    pub airdrop: crate::airdrop::AirdropConfig,
    #[serde(default)]
    pub gpu_rental: GpuRentalConfig,
    /// P2P provider-to-provider communication config
    #[serde(default)]
    pub p2p: crate::p2p::P2PConfig,
    /// Multi-relay discovery config
    #[serde(default)]
    pub relay_discovery: crate::relay_discovery::RelayDiscoveryConfig,
    /// Automatic model pulling config
    #[serde(default)]
    pub auto_model_pull: AutoModelPullConfig,
    /// HuggingFace model discovery config
    #[serde(default)]
    pub model_discovery: ModelDiscoveryConfig,
    /// Model caching config with LRU eviction
    #[serde(default)]
    pub model_cache: ModelCacheConfig,
    /// Usage proof rollup config (Merkle tree epoch batching)
    #[serde(default)]
    pub rollup: RollupConfig,
    /// Cross-chain payment bridge config
    #[serde(default)]
    pub payment_bridge: PaymentBridgeConfig,
    /// Self-update configuration
    #[serde(default)]
    pub update: UpdateConfig,
    /// Contract hex overrides (embedded contracts can be overridden per-deployment)
    #[serde(default)]
    pub contracts: ContractsConfig,
    /// Ergo oracle configuration (ERG/USD price feed from oracle pool box)
    #[serde(default)]
    pub oracle: OracleConfig,
    /// Provider on-chain registration configuration
    #[serde(default)]
    pub provider_registry: ProviderRegistryConfig,
    /// Storage rent monitoring configuration
    #[serde(default)]
    pub storage_rent: crate::storage_rent::StorageRentConfig,
    /// Rate limiting configuration
    #[serde(default)]
    pub rate_limit: crate::rate_limit::RateLimitConfig,
    /// Audit/logging configuration
    #[serde(default)]
    pub audit: crate::audit::AuditConfig,
    /// Peer reputation scoring configuration
    #[serde(default)]
    pub reputation: crate::reputation::ReputationConfig,
    /// Gossip protocol configuration
    #[serde(default)]
    pub gossip: crate::gossip::GossipConfig,
    /// Marketplace sync configuration (periodic provider info push to relay)
    #[serde(default)]
    pub marketplace_sync: crate::marketplace_sync::MarketplaceSyncConfig,
}

/// On-chain transaction configuration (Phase 2).
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct ChainTxConfig {
    /// Submit heartbeat as an on-chain Ergo tx (default: false)
    #[serde(default)]
    pub heartbeat_tx_enabled: bool,
    /// Submit usage proofs as on-chain Ergo txs (default: false)
    #[serde(default)]
    pub usage_proof_tx_enabled: bool,
    /// Batch usage proof submissions every N seconds (default: 30)
    #[serde(default = "default_usage_proof_batch_interval")]
    pub usage_proof_batch_interval_secs: u64,
    /// Minimum ERG value for a usage proof box (nanoERG, default: 0.001 ERG)
    #[serde(default = "default_usage_proof_min_value")]
    pub usage_proof_min_value_nanoerg: u64,
    /// Provider NFT token ID (required for heartbeat tx and usage proof tx)
    #[serde(default)]
    pub provider_nft_token_id: String,
    /// Usage proof box ErgoTree hex (compiled from contracts/usage_proof.es)
    #[serde(default)]
    pub usage_proof_tree_hex: String,
}

fn default_usage_proof_batch_interval() -> u64 {
    30
}
fn default_usage_proof_min_value() -> u64 {
    1_000_000 // 0.001 ERG
}

impl Default for ChainTxConfig {
    fn default() -> Self {
        Self {
            heartbeat_tx_enabled: false,
            usage_proof_tx_enabled: false,
            usage_proof_batch_interval_secs: default_usage_proof_batch_interval(),
            usage_proof_min_value_nanoerg: default_usage_proof_min_value(),
            provider_nft_token_id: String::new(),
            usage_proof_tree_hex: String::new(),
        }
    }
}

/// llama-server (llama.cpp) configuration for AI inference backend detection.
#[derive(Debug, Clone, Deserialize, Serialize)]
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

fn default_llama_health_interval() -> u64 {
    60
}

impl Default for LlamaServerConfig {
    fn default() -> Self {
        Self {
            url: default_llama_server_url(),
            health_check_interval_secs: default_llama_health_interval(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ErgoNodeConfig {
    /// REST API URL of the local Ergo node (default: http://127.0.0.1:9053)
    #[serde(default = "default_ergo_url")]
    pub rest_url: String,
}

fn default_ergo_url() -> String {
    "http://127.0.0.1:9053".into()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Deserialize, Serialize)]
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

fn default_discovery_interval() -> u64 {
    120
}
fn default_probe_timeout() -> u64 {
    5
}
fn default_xergon_port() -> u16 {
    9099
}
fn default_max_concurrent() -> usize {
    10
}
fn default_max_peers_per_cycle() -> usize {
    50
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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
///
/// All amounts are in nanoERG (1 ERG = 10^9 nanoERG).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SettlementConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_settlement_interval")]
    pub interval_secs: u64,
    #[serde(default)]
    pub ledger_file: Option<PathBuf>,
    #[serde(default = "default_settlement_dry_run")]
    pub dry_run: bool,
    /// Cost per 1K tokens in nanoERG (default: 1_000_000 = 0.001 ERG per 1K tokens)
    #[serde(default = "default_cost_per_1k_nanoerg")]
    pub cost_per_1k_tokens_nanoerg: u64,
    /// Minimum nanoERG before a provider payment is included in a settlement batch
    /// (default: 1_000_000_000 = 1 ERG minimum)
    #[serde(default = "default_min_settlement_nanoerg")]
    pub min_settlement_nanoerg: u64,
    /// When true, settlement uses real eUTXO transactions via the Ergo node
    /// wallet to spend user staking boxes and settle fees on-chain.
    /// When false (default), uses in-memory batch accumulation only.
    #[serde(default)]
    pub chain_enabled: bool,
    /// Minimum number of confirmations (blocks) a staking box must have before
    /// it can be included in a settlement. Default: 30.
    #[serde(default = "default_min_confirmations")]
    pub min_confirmations: u32,
}

impl Default for SettlementConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_secs: default_settlement_interval(),
            ledger_file: None,
            dry_run: default_settlement_dry_run(),
            cost_per_1k_tokens_nanoerg: default_cost_per_1k_nanoerg(),
            min_settlement_nanoerg: default_min_settlement_nanoerg(),
            chain_enabled: false,
            min_confirmations: default_min_confirmations(),
        }
    }
}

fn default_settlement_interval() -> u64 {
    86400
}
fn default_settlement_dry_run() -> bool {
    true
}
fn default_cost_per_1k_nanoerg() -> u64 {
    1_000_000 // 0.001 ERG per 1K tokens
}
fn default_min_settlement_nanoerg() -> u64 {
    1_000_000_000 // 1 ERG minimum settlement
}
fn default_min_confirmations() -> u32 {
    30
}

/// Inference proxy configuration.
///
/// Controls the OpenAI-compatible inference endpoint that xergon-agent
/// exposes to xergon-relay. Proxies requests to a local LLM backend
/// (Ollama, llama.cpp server, etc.).
#[derive(Debug, Clone, Deserialize, Serialize)]
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
    /// If set, requests must include `Authorization: Bearer ***` header.
    #[serde(default)]
    pub api_key: String,
    /// List of model names this agent advertises as available for inference.
    /// Populated via `xergon-agent setup` when using Ollama or similar backends.
    #[serde(default)]
    pub served_models: Vec<String>,
}

fn default_inference_enabled() -> bool {
    true
}
fn default_inference_url() -> String {
    "http://127.0.0.1:11434".into()
}
fn default_inference_timeout() -> u64 {
    120
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            enabled: default_inference_enabled(),
            url: default_inference_url(),
            timeout_secs: default_inference_timeout(),
            api_key: String::new(),
            served_models: Vec::new(),
        }
    }
}

/// Per-model pricing configuration for the provider.
///
/// Controls the price advertised on-chain in the Provider Box R6 register.
/// Prices are in nanoERG per 1M tokens.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PricingConfig {
    /// Default price per 1M tokens in nanoERG (used if no per-model override).
    /// Default: 50000 nanoERG = 0.00005 ERG per 1M tokens.
    #[serde(default = "default_price_per_1m_tokens")]
    pub default_price_per_1m_tokens: u64,

    /// Per-model price overrides (optional). Model IDs that don't appear here
    /// use `default_price_per_1m_tokens`.
    #[serde(default)]
    pub models: std::collections::HashMap<String, u64>,
}

fn default_price_per_1m_tokens() -> u64 {
    50_000 // 0.00005 ERG per 1M tokens
}

impl Default for PricingConfig {
    fn default() -> Self {
        Self {
            default_price_per_1m_tokens: default_price_per_1m_tokens(),
            models: std::collections::HashMap::new(),
        }
    }
}

impl PricingConfig {
    /// Build the R6 register JSON string for a given list of served model IDs.
    ///
    /// Produces the structured format:
    ///   {"models":[{"id":"model-name","price_per_1m_tokens":N},...]}
    ///
    /// Each model gets its price from `self.models` (per-model override), or
    /// falls back to `self.default_price_per_1m_tokens`.
    pub fn build_r6_json(&self, served_models: &[String]) -> String {
        #[derive(serde::Serialize)]
        struct ModelEntry {
            id: String,
            price_per_1m_tokens: u64,
        }
        #[derive(serde::Serialize)]
        struct ModelsPayload {
            models: Vec<ModelEntry>,
        }

        let models: Vec<ModelEntry> = served_models
            .iter()
            .map(|id| ModelEntry {
                id: id.clone(),
                price_per_1m_tokens: self.models.get(id).copied().unwrap_or(self.default_price_per_1m_tokens),
            })
            .collect();

        serde_json::to_string(&ModelsPayload { models }).unwrap_or_else(|_| "[]".to_string())
    }
}

/// GPU Bazar rental configuration (Phase 4).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GpuRentalConfig {
    /// Enable GPU rental endpoints (default: false)
    #[serde(default)]
    pub enabled: bool,
    /// Ergo node REST API URL for GPU rental operations
    #[serde(default = "default_ergo_url")]
    pub ergo_node_url: String,
    /// Compiled GPU rental listing contract ErgoTree hex
    #[serde(default)]
    pub listing_tree_hex: String,
    /// Compiled GPU rental contract ErgoTree hex
    #[serde(default)]
    pub rental_tree_hex: String,
    /// Port range for SSH tunnels (e.g. "22000-22100")
    #[serde(default = "default_ssh_tunnel_port_range")]
    pub ssh_tunnel_port_range: String,
    /// Maximum rental duration in hours (default: 720 = 30 days)
    #[serde(default = "default_max_rental_hours")]
    pub max_rental_hours: i32,
    /// Enable SSH tunnel management (default: true when gpu_rental is enabled)
    #[serde(default = "default_ssh_enabled")]
    pub ssh_enabled: bool,
    /// How often to check sessions for expiration (seconds, default: 60)
    #[serde(default = "default_metering_check_interval")]
    pub metering_check_interval_secs: u64,
    /// SSH username for connecting to provider nodes (default: "xergon")
    #[serde(default = "default_ssh_username")]
    pub ssh_username: String,
    /// Compiled GPU rating contract ErgoTree hex
    #[serde(default)]
    pub rating_tree_hex: String,
}

fn default_ssh_tunnel_port_range() -> String {
    "22000-22100".into()
}

fn default_max_rental_hours() -> i32 {
    720
}

fn default_ssh_enabled() -> bool {
    true
}

fn default_metering_check_interval() -> u64 {
    60
}

fn default_ssh_username() -> String {
    "xergon".into()
}

impl Default for GpuRentalConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ergo_node_url: default_ergo_url(),
            listing_tree_hex: String::new(),
            rental_tree_hex: String::new(),
            ssh_tunnel_port_range: default_ssh_tunnel_port_range(),
            max_rental_hours: default_max_rental_hours(),
            ssh_enabled: default_ssh_enabled(),
            metering_check_interval_secs: default_metering_check_interval(),
            ssh_username: default_ssh_username(),
            rating_tree_hex: String::new(),
        }
    }
}

/// Automatic model pulling configuration.
///
/// Controls auto-pulling of models from Ollama registry, HuggingFace,
/// or P2P peers when a requested model is not locally available.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AutoModelPullConfig {
    /// Enable automatic model pulling (default: false)
    #[serde(default)]
    pub enabled: bool,
    /// Maximum time to wait for a model pull (seconds, default: 600)
    #[serde(default = "default_pull_timeout")]
    pub pull_timeout_secs: u64,
    /// Maximum concurrent pulls (default: 2)
    #[serde(default = "default_max_concurrent_pulls")]
    pub max_concurrent_pulls: u32,
    /// Models to pre-pull on startup (empty = none)
    #[serde(default)]
    pub pre_pull_models: Vec<String>,
    /// Optional HuggingFace API token for gated models
    #[serde(default)]
    pub huggingface_token: String,
    /// Backend URL to pull models from (defaults to inference.url)
    #[serde(default)]
    pub backend_url: String,
}

fn default_pull_timeout() -> u64 {
    600
}
fn default_max_concurrent_pulls() -> u32 {
    2
}

impl Default for AutoModelPullConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            pull_timeout_secs: default_pull_timeout(),
            max_concurrent_pulls: default_max_concurrent_pulls(),
            pre_pull_models: Vec::new(),
            huggingface_token: String::new(),
            backend_url: String::new(),
        }
    }
}

/// HuggingFace model discovery configuration.
///
/// Controls automatic scanning of the HuggingFace Hub for compatible models
/// with GGUF quantization, license filtering, and size limits.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelDiscoveryConfig {
    /// Enable model discovery scanning (default: false)
    #[serde(default)]
    pub enabled: bool,
    /// Allowed license identifiers (empty = allow all commercial licenses)
    #[serde(default)]
    pub allowed_licenses: Vec<String>,
    /// Maximum model download size in GB (default: 40)
    #[serde(default = "default_max_model_size_gb")]
    pub max_model_size_gb: u32,
    /// Cache refresh interval in seconds (default: 86400 = 24h)
    #[serde(default = "default_discovery_refresh_interval")]
    pub refresh_interval_secs: u64,
    /// Models to exclude from discovery results
    #[serde(default)]
    pub exclude_models: Vec<String>,
    /// Architectures to exclude from discovery (e.g. ["phi", "gemma"])
    #[serde(default)]
    pub exclude_architectures: Vec<String>,
    /// Optional HuggingFace API token for gated models
    #[serde(default)]
    pub huggingface_token: String,
}

fn default_max_model_size_gb() -> u32 {
    40
}

fn default_discovery_refresh_interval() -> u64 {
    86400 // 24 hours
}

impl Default for ModelDiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allowed_licenses: Vec::new(),
            max_model_size_gb: default_max_model_size_gb(),
            refresh_interval_secs: default_discovery_refresh_interval(),
            exclude_models: Vec::new(),
            exclude_architectures: Vec::new(),
            huggingface_token: String::new(),
        }
    }
}

/// Model caching configuration with LRU eviction.
///
/// Controls disk-based caching of downloaded models with automatic
/// eviction when usage exceeds the configured limit.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelCacheConfig {
    /// Enable model caching with LRU eviction (default: false)
    #[serde(default)]
    pub enabled: bool,
    /// Maximum cache size in GB (default: 100)
    #[serde(default = "default_cache_max_size_gb")]
    pub max_size_gb: u32,
    /// Eviction threshold as percentage of max_size_gb (default: 80)
    #[serde(default = "default_eviction_threshold")]
    pub eviction_threshold_percent: f64,
    /// Cache directory path (default: OS cache dir / xergon-agent / models)
    #[serde(default)]
    pub cache_dir: String,
    /// How often to check for eviction (seconds, default: 300 = 5min)
    #[serde(default = "default_eviction_check_interval")]
    pub eviction_check_interval_secs: u64,
}

fn default_cache_max_size_gb() -> u32 {
    100
}

fn default_eviction_threshold() -> f64 {
    80.0
}

fn default_eviction_check_interval() -> u64 {
    300
}

impl Default for ModelCacheConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_size_gb: default_cache_max_size_gb(),
            eviction_threshold_percent: default_eviction_threshold(),
            cache_dir: String::new(),
            eviction_check_interval_secs: default_eviction_check_interval(),
        }
    }
}

/// Usage proof rollup configuration (Merkle tree epoch batching).
///
/// Instead of creating individual usage proof boxes, batches proofs
/// into a single commitment box with a Merkle root.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RollupConfig {
    /// Enable usage proof rollups (default: false)
    #[serde(default)]
    pub enabled: bool,
    /// Epoch duration in seconds (default: 300 = 5 minutes)
    #[serde(default = "default_epoch_duration_secs")]
    pub epoch_duration_secs: u32,
    /// Minimum proofs before committing an epoch (default: 10)
    #[serde(default = "default_min_proofs_per_commitment")]
    pub min_proofs_per_commitment: u32,
    /// Maximum proofs per commitment (default: 1000)
    #[serde(default = "default_max_proofs_per_commitment")]
    pub max_proofs_per_commitment: u32,
    /// Compiled usage_commitment.es ErgoTree hex
    #[serde(default)]
    pub commitment_tree_hex: String,
    /// Commitment NFT token ID for the commitment box
    #[serde(default)]
    pub commitment_nft_token_id: String,
    /// Minimum ERG value for a commitment box (nanoERG, default: 0.001 ERG)
    #[serde(default = "default_commitment_min_value")]
    pub commitment_min_value_nanoerg: u64,
}

fn default_epoch_duration_secs() -> u32 {
    300
}
fn default_min_proofs_per_commitment() -> u32 {
    10
}
fn default_max_proofs_per_commitment() -> u32 {
    1000
}
fn default_commitment_min_value() -> u64 {
    1_000_000 // 0.001 ERG
}

impl Default for RollupConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            epoch_duration_secs: default_epoch_duration_secs(),
            min_proofs_per_commitment: default_min_proofs_per_commitment(),
            max_proofs_per_commitment: default_max_proofs_per_commitment(),
            commitment_tree_hex: String::new(),
            commitment_nft_token_id: String::new(),
            commitment_min_value_nanoerg: default_commitment_min_value(),
        }
    }
}

/// Cross-chain payment bridge configuration.
///
/// Controls the invoice-based Lock-and-Mint bridge for accepting
/// payments from foreign chains (BTC, ETH, ADA) to pay for
/// Xergon inference and GPU rental.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PaymentBridgeConfig {
    /// Enable cross-chain payment bridge (default: false)
    #[serde(default)]
    pub enabled: bool,
    /// Bridge operator public key (hex, for confirmations)
    #[serde(default)]
    pub bridge_public_key: String,
    /// Supported foreign chains (default: btc, eth, ada)
    #[serde(default = "default_bridge_supported_chains")]
    pub supported_chains: Vec<crate::payment_bridge::ForeignChain>,
    /// Invoice timeout in blocks before buyer can refund (default: 720 = ~24 hours)
    #[serde(default = "default_bridge_timeout_blocks")]
    pub invoice_timeout_blocks: u32,
    /// Compiled payment_bridge.es ErgoTree hex
    #[serde(default)]
    pub invoice_tree_hex: String,
}

fn default_bridge_supported_chains() -> Vec<crate::payment_bridge::ForeignChain> {
    vec![
        crate::payment_bridge::ForeignChain::Btc,
        crate::payment_bridge::ForeignChain::Eth,
        crate::payment_bridge::ForeignChain::Ada,
    ]
}

fn default_bridge_timeout_blocks() -> u32 {
    720 // ~24 hours at 2-minute block time
}

impl Default for PaymentBridgeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bridge_public_key: String::new(),
            supported_chains: default_bridge_supported_chains(),
            invoice_timeout_blocks: default_bridge_timeout_blocks(),
            invoice_tree_hex: String::new(),
        }
    }
}

/// Self-update configuration.
///
/// Controls automatic update checking and the GitHub Releases URL
/// used by `xergon update`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateConfig {
    /// GitHub Releases API URL for checking latest version
    #[serde(default = "default_update_release_url")]
    pub release_url: String,
    /// Check for updates automatically on agent startup (default: false)
    #[serde(default)]
    pub auto_check: bool,
    /// How often to check for updates in hours (default: 24)
    #[serde(default = "default_update_check_interval")]
    pub check_interval_hours: u64,
}

fn default_update_release_url() -> String {
    "https://api.github.com/repos/n1ur0/Xergon-Network/releases/latest".into()
}

fn default_update_check_interval() -> u64 {
    24
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            release_url: default_update_release_url(),
            auto_check: false,
            check_interval_hours: default_update_check_interval(),
        }
    }
}

/// Contract hex override configuration.
///
/// Allows per-deployment overrides of embedded ErgoTree hex values.
/// If a field is empty, the embedded compiled hex is used instead.
/// This is useful for testing with different contract versions or
/// deploying to different Ergo networks (testnet vs mainnet).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ContractsConfig {
    /// Override for provider_box.es compiled hex
    #[serde(default)]
    pub provider_box_hex: String,
    /// Override for provider_registration.es compiled hex
    #[serde(default)]
    pub provider_registration_hex: String,
    /// Override for treasury_box.es compiled hex
    #[serde(default)]
    pub treasury_box_hex: String,
    /// Override for usage_proof.es compiled hex
    #[serde(default)]
    pub usage_proof_hex: String,
    /// Override for user_staking.es compiled hex
    #[serde(default)]
    pub user_staking_hex: String,
    /// Override for gpu_rental.es compiled hex
    #[serde(default)]
    pub gpu_rental_hex: String,
    /// Override for usage_commitment.es compiled hex
    #[serde(default)]
    pub usage_commitment_hex: String,
    /// Override for relay_registry.es compiled hex
    #[serde(default)]
    pub relay_registry_hex: String,
    /// Override for gpu_rating.es compiled hex
    #[serde(default)]
    pub gpu_rating_hex: String,
    /// Override for gpu_rental_listing.es compiled hex
    #[serde(default)]
    pub gpu_rental_listing_hex: String,
    /// Override for payment_bridge.es compiled hex
    #[serde(default)]
    pub payment_bridge_hex: String,
}

impl Default for ContractsConfig {
    fn default() -> Self {
        Self {
            provider_box_hex: String::new(),
            provider_registration_hex: String::new(),
            treasury_box_hex: String::new(),
            usage_proof_hex: String::new(),
            user_staking_hex: String::new(),
            gpu_rental_hex: String::new(),
            usage_commitment_hex: String::new(),
            relay_registry_hex: String::new(),
            gpu_rating_hex: String::new(),
            gpu_rental_listing_hex: String::new(),
            payment_bridge_hex: String::new(),
        }
    }
}

/// Ergo oracle configuration (ERG/USD price feed from oracle pool box).
///
/// Reads the current ERG/USD rate from an oracle-core pool box on the Ergo
/// blockchain. The pool box contains R4 (Long) with the aggregated rate in
/// nanoERG per USD cent, which is converted to ERG/USD.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OracleConfig {
    /// Enable the oracle service (default: false)
    #[serde(default)]
    pub enabled: bool,
    /// Oracle pool NFT token ID for ERG/USD price feed.
    /// When set, the agent fetches the current rate from the oracle pool box.
    /// Example: the mainnet oracle-core pool NFT ID.
    #[serde(default)]
    pub pool_nft_id: String,
    /// How often to refresh the oracle rate (seconds, default: 600 = 10 min)
    #[serde(default = "default_oracle_refresh_interval")]
    pub refresh_interval_secs: u64,
}

fn default_oracle_refresh_interval() -> u64 {
    600
}

impl Default for OracleConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            pool_nft_id: String::new(),
            refresh_interval_secs: default_oracle_refresh_interval(),
        }
    }
}

/// Provider on-chain registration configuration.
///
/// Controls whether the agent auto-registers as a provider on the Ergo blockchain.
/// The registration creates a Provider Box with the provider_registration ErgoTree,
/// containing R4-R7 registers and a singleton NFT token.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderRegistryConfig {
    /// Enable the provider registration subsystem (default: false)
    #[serde(default)]
    pub enabled: bool,
    /// Automatically register on startup if not already registered (default: false)
    #[serde(default)]
    pub auto_register: bool,
    /// Amount of ERG to stake in the provider box (nanoERG, default: 1_000_000_000 = 1 ERG)
    #[serde(default = "default_registration_stake_nanoerg")]
    pub registration_stake_nanoerg: u64,
    /// Provider's compressed secp256k1 public key (hex, 33 bytes).
    /// Required for auto-registration. If empty, auto_register will be skipped.
    #[serde(default)]
    pub provider_pk_hex: String,
    /// Provider endpoint URL to advertise on-chain.
    /// If empty, defaults to the agent's own listen address.
    #[serde(default)]
    pub endpoint_url: String,
    /// Price per token in nanoERG to advertise on-chain (default: 50000)
    #[serde(default = "default_provider_price_per_token")]
    pub price_per_token: u64,
}

fn default_registration_stake_nanoerg() -> u64 {
    1_000_000_000 // 1 ERG
}

fn default_provider_price_per_token() -> u64 {
    50_000 // 0.00005 ERG per 1M tokens
}

impl Default for ProviderRegistryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            auto_register: false,
            registration_stake_nanoerg: default_registration_stake_nanoerg(),
            provider_pk_hex: String::new(),
            endpoint_url: String::new(),
            price_per_token: default_provider_price_per_token(),
        }
    }
}

impl AgentConfig {
    /// Validate configuration values and return a descriptive error for any problem.
    pub fn validate(&self) -> Result<(), String> {
        // 1. ergo_node.rest_url must be a valid URL starting with http:// or https://
        {
            let url = &self.ergo_node.rest_url;
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return Err(format!(
                    "ergo_node.rest_url \"{}\" must start with http:// or https://",
                    url
                ));
            }
            if url::Url::parse(url).is_err() {
                return Err(format!(
                    "ergo_node.rest_url \"{}\" is not a valid URL",
                    url
                ));
            }
        }

        // 2. inference.url must be a valid URL
        {
            let url = &self.inference.url;
            if url::Url::parse(url).is_err() {
                return Err(format!(
                    "inference.url \"{}\" is not a valid URL",
                    url
                ));
            }
        }

        // 3. settlement.cost_per_1k_tokens_nanoerg must be > 0 when settlement is enabled
        if self.settlement.enabled && self.settlement.cost_per_1k_tokens_nanoerg.eq(&0) {
            return Err(
                "settlement.cost_per_1k_tokens_nanoerg must be > 0 when settlement is enabled"
                    .into(),
            );
        }

        // 4. pricing.models entries must have valid model IDs and prices >= 0
        for (model_id, price) in &self.pricing.models {
            if model_id.trim().is_empty() {
                return Err("pricing.models contains an entry with an empty model ID".into());
            }
            if *price == 0 {
                return Err(format!(
                    "pricing.models[\"{}\"] price must be > 0, got 0",
                    model_id
                ));
            }
        }

        // 5. api.listen_addr must be parseable as a SocketAddr
        if self.api.listen_addr.parse::<std::net::SocketAddr>().is_err() {
            return Err(format!(
                "api.listen_addr \"{}\" is not a valid SocketAddr (expected host:port)",
                self.api.listen_addr
            ));
        }

        Ok(())
    }

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
            served_models: vec!["qwen3.5-4b".into()],
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

    #[test]
    fn test_inference_config_served_models_default_empty() {
        let cfg = InferenceConfig::default();
        assert!(cfg.served_models.is_empty());
    }

    #[test]
    fn test_inference_config_served_models_deserialize() {
        let cfg: InferenceConfig = serde_json::from_value(serde_json::json!({
            "enabled": true,
            "url": "http://localhost:11434",
            "served_models": ["qwen3.5-4b", "llama3.1-8b"]
        }))
        .expect("deserialization should succeed");
        assert_eq!(cfg.served_models, vec!["qwen3.5-4b", "llama3.1-8b"]);
    }

    // ---- Config validation tests ----

    fn make_test_agent_config() -> AgentConfig {
        let toml_str = r#"
[xergon]
provider_id = "test_provider"
provider_name = "Test Provider"
region = "us-east"
ergo_address = "9f5jKpQ3fTjMGdBGJG7oPdz3pDX9dJhGmNcD1w3L8jSuP2bKkQj"

[ergo_node]
rest_url = "http://127.0.0.1:9053"

[api]
listen_addr = "0.0.0.0:9099"
api_key = ""

[settlement]
enabled = false
cost_per_1k_tokens_nanoerg = 1000000

[inference]
url = "http://127.0.0.1:11434"

[pricing]
default_price_per_1m_tokens = 50000

[peer_discovery]
"#;
        let settings = config::Config::builder()
            .add_source(config::File::from_str(toml_str, config::FileFormat::Toml))
            .build()
            .unwrap();
        settings.try_deserialize().unwrap()
    }

    #[test]
    fn test_validate_ok_with_defaults() {
        let config = make_test_agent_config();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_bad_ergo_node_url() {
        let mut config = make_test_agent_config();
        config.ergo_node.rest_url = "ftp://bad-scheme".into();
        assert!(config.validate().is_err());
        let err = config.validate().unwrap_err();
        assert!(err.contains("ergo_node.rest_url"), "{err}");
    }

    #[test]
    fn test_validate_bad_inference_url() {
        let mut config = make_test_agent_config();
        config.inference.url = "not a url://@@@".into();
        assert!(config.validate().is_err());
        let err = config.validate().unwrap_err();
        assert!(err.contains("inference.url"), "{err}");
    }

    #[test]
    fn test_validate_settlement_zero_cost() {
        let mut config = make_test_agent_config();
        config.settlement.enabled = true;
        config.settlement.cost_per_1k_tokens_nanoerg = 0;
        assert!(config.validate().is_err());
        let err = config.validate().unwrap_err();
        assert!(err.contains("cost_per_1k_tokens_nanoerg"), "{err}");
    }

    #[test]
    fn test_validate_pricing_empty_model_id() {
        let mut config = make_test_agent_config();
        config.pricing.models.insert("".into(), 100);
        assert!(config.validate().is_err());
        let err = config.validate().unwrap_err();
        assert!(err.contains("empty model ID"), "{err}");
    }

    #[test]
    fn test_validate_pricing_zero_price() {
        let mut config = make_test_agent_config();
        config.pricing.models.insert("test-model".into(), 0);
        assert!(config.validate().is_err());
        let err = config.validate().unwrap_err();
        assert!(err.contains("test-model"), "{err}");
    }

    #[test]
    fn test_validate_bad_listen_addr() {
        let mut config = make_test_agent_config();
        config.api.listen_addr = "not-an-addr".into();
        assert!(config.validate().is_err());
        let err = config.validate().unwrap_err();
        assert!(err.contains("listen_addr"), "{err}");
    }

    #[test]
    fn test_validate_settlement_disabled_allows_zero_cost() {
        let mut config = make_test_agent_config();
        config.settlement.enabled = false;
        config.settlement.cost_per_1k_tokens_nanoerg = 0;
        assert!(config.validate().is_ok());
    }
}
