//! Configuration for the Xergon relay server

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct RelayConfig {
    pub relay: RelaySettings,
    pub providers: ProviderSettings,
    #[serde(default)]
    pub chain: ChainConfig,
    #[serde(default)]
    pub balance: BalanceConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub incentive: IncentiveConfig,
    #[serde(default)]
    pub bridge: BridgeConfig,
    #[serde(default)]
    pub rate_limit: RateLimitConfig,
    #[serde(default)]
    pub oracle: OracleConfig,
    #[serde(default)]
    pub free_tier: FreeTierConfig,
    #[serde(default)]
    pub events: EventsConfig,
    #[serde(default)]
    pub gossip: GossipConfig,
    #[serde(default)]
    pub health_v2: HealthV2Config,
    #[serde(default)]
    pub ws_chat: WsChatConfig,
    #[serde(default)]
    pub ws_pool: WsPoolConfig,
    #[serde(default)]
    pub dedup: DedupConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub circuit_breaker: CircuitBreakerConfig,
    #[serde(default)]
    pub load_shed: LoadShedConfig,
    #[serde(default)]
    pub degradation: DegradationConfig,
    #[serde(default)]
    pub coalesce: CoalesceConfig,
    #[serde(default)]
    pub stream_buffer: StreamBufferConfig,
    #[serde(default)]
    pub adaptive_routing: AdaptiveRoutingConfig,
    #[serde(default)]
    pub telemetry: TelemetryConfig,
    #[serde(default)]
    pub admin: AdminConfig,
    #[serde(default)]
    pub auto_register: crate::auto_register::AutoRegistrationConfig,
    #[serde(default)]
    pub cache_sync: crate::cache_sync::CacheSyncConfig,
    #[serde(default)]
// DELETED:     pub multi_region: crate::multi_region::RegionConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RelaySettings {
    /// Address to bind (e.g. "0.0.0.0:8080")
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,

    /// CORS origins (comma-separated, or "*" for all)
    #[serde(default = "default_cors_origins")]
    pub cors_origins: String,

    /// Health poll interval in seconds
    #[serde(default = "default_health_poll")]
    pub health_poll_interval_secs: u64,

    /// Provider request timeout in seconds
    #[serde(default = "default_provider_timeout")]
    pub provider_timeout_secs: u64,

    /// Max fallback attempts
    #[serde(default = "default_max_fallback")]
    pub max_fallback_attempts: usize,

    /// Circuit breaker: consecutive failures before opening circuit (default: 5)
    #[serde(default = "default_circuit_failure_threshold")]
    pub circuit_failure_threshold: u32,

    /// Circuit breaker: seconds before transitioning Open -> HalfOpen (default: 30)
    #[serde(default = "default_circuit_recovery_timeout_secs")]
    pub circuit_recovery_timeout_secs: u64,

    /// Circuit breaker: max probe requests allowed in HalfOpen state (default: 2)
    #[serde(default = "default_circuit_half_open_max_probes")]
    pub circuit_half_open_max_probes: u32,

    /// Sticky sessions: TTL in seconds for session affinity (default: 1800 = 30 min)
    #[serde(default = "default_sticky_session_ttl_secs")]
    pub sticky_session_ttl_secs: u64,

    /// Auth token required for provider onboarding API (POST /v1/providers/onboard).
    /// When None, onboarding is open (no auth required).
    #[serde(default)]
    pub onboarding_auth_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderSettings {
    /// Static list of known xergon-agent endpoints
    #[serde(default = "default_known_endpoints")]
    pub known_endpoints: Vec<String>,
}

/// Configuration for Ergo chain-state provider discovery.
#[derive(Debug, Clone, Deserialize)]
pub struct ChainConfig {
    /// Enable/disable chain scanning
    #[serde(default = "default_chain_enabled")]
    pub enabled: bool,

    /// Ergo node REST API URL
    #[serde(default = "default_ergo_node_url")]
    pub ergo_node_url: String,

    /// How often to scan the chain for providers (seconds)
    #[serde(default = "default_scan_interval_secs")]
    pub scan_interval_secs: u64,

    /// How long cached chain data is considered fresh (seconds).
    /// Listing endpoints use cached data within this window instead
    /// of triggering a fresh scan on every request.
    #[serde(default = "default_cache_ttl_secs")]
    pub cache_ttl_secs: u64,

    /// Hex-encoded ErgoTree bytes for the Provider Box contract.
    /// Used as a CONTAINS predicate in EIP-1 registered scans.
    /// Leave empty to disable chain scanning (placeholder mode).
    #[serde(default)]
    pub provider_tree_bytes: String,

    /// Hex-encoded ErgoTree bytes for the GPU Rental Listing Box contract.
    /// Used as a CONTAINS predicate in EIP-1 registered scans.
    /// Leave empty to disable GPU listing scanning (placeholder mode).
    #[serde(default)]
    pub gpu_listing_tree_bytes: String,

    /// Hex-encoded ErgoTree bytes for the GPU Rental Box contract.
    /// Used to scan for active rental boxes on-chain.
    /// Leave empty to disable GPU rental scanning (placeholder mode).
    #[serde(default)]
    pub gpu_rental_tree_bytes: String,

    /// Hex-encoded ErgoTree bytes for the GPU Rating Box contract.
    /// Used to scan for rating boxes on-chain for reputation.
    /// Leave empty to disable GPU rating scanning.
    #[serde(default)]
    #[allow(dead_code)]
    pub gpu_rating_tree_bytes: String,

    /// Base URL for the xergon-agent GPU API (used to proxy rental requests).
    /// e.g. "http://127.0.0.1:9099"
    #[serde(default)]
    pub agent_gpu_endpoint: String,
}

impl Default for ChainConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ergo_node_url: default_ergo_node_url(),
            scan_interval_secs: default_scan_interval_secs(),
            cache_ttl_secs: default_cache_ttl_secs(),
            provider_tree_bytes: String::new(),
            gpu_listing_tree_bytes: String::new(),
            gpu_rental_tree_bytes: String::new(),
            gpu_rating_tree_bytes: String::new(),
            agent_gpu_endpoint: String::new(),
        }
    }
}

fn default_listen_addr() -> String {
    "0.0.0.0:8080".into()
}
fn default_cors_origins() -> String {
    "*".into()
}
fn default_health_poll() -> u64 {
    30
}
fn default_provider_timeout() -> u64 {
    30
}
fn default_max_fallback() -> usize {
    3
}
fn default_circuit_failure_threshold() -> u32 {
    5
}
fn default_circuit_recovery_timeout_secs() -> u64 {
    30
}
fn default_circuit_half_open_max_probes() -> u32 {
    2
}
fn default_sticky_session_ttl_secs() -> u64 {
    1800 // 30 minutes
}
fn default_known_endpoints() -> Vec<String> {
    vec!["http://127.0.0.1:9099".into()]
}
fn default_chain_enabled() -> bool {
    true
}
fn default_ergo_node_url() -> String {
    "http://127.0.0.1:9053".into()
}
fn default_scan_interval_secs() -> u64 {
    30
}
fn default_cache_ttl_secs() -> u64 {
    10
}

/// Configuration for on-chain user balance verification.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct BalanceConfig {
    /// Enable/disable balance checking before allowing inference requests.
    #[serde(default = "default_balance_enabled")]
    pub enabled: bool,

    /// Ergo node REST API URL (for staking box queries).
    #[serde(default = "default_ergo_node_url")]
    pub ergo_node_url: String,

    /// Minimum balance (in nanoERG) required to make a request.
    /// 1 ERG = 1,000,000,000 nanoERG. Default: 0.001 ERG.
    #[serde(default = "default_min_balance_nanoerg")]
    pub min_balance_nanoerg: u64,

    /// How long to cache balance results (seconds).
    #[serde(default = "default_balance_cache_ttl")]
    pub cache_ttl_secs: u64,

    /// Allow free tier requests (limited number of requests without balance check).
    #[serde(default = "default_free_tier_enabled")]
    pub free_tier_enabled: bool,

    /// Number of free requests a user can make without any balance.
    #[serde(default = "default_free_tier_requests")]
    pub free_tier_requests: u32,

    /// Hex-encoded ErgoTree bytes for the Staking Box contract.
    /// Used as a CONTAINS predicate in EIP-1 registered scans.
    /// Leave empty to disable balance checking (placeholder mode).
    #[serde(default)]
    pub staking_tree_bytes: String,
}

impl Default for BalanceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ergo_node_url: default_ergo_node_url(),
            min_balance_nanoerg: default_min_balance_nanoerg(),
            cache_ttl_secs: default_balance_cache_ttl(),
            free_tier_enabled: default_free_tier_enabled(),
            free_tier_requests: default_free_tier_requests(),
            staking_tree_bytes: String::new(),
        }
    }
}

fn default_balance_enabled() -> bool {
    true
}
fn default_min_balance_nanoerg() -> u64 {
    1_000_000 // 0.001 ERG
}
fn default_balance_cache_ttl() -> u64 {
    30
}
fn default_free_tier_enabled() -> bool {
    true
}
fn default_free_tier_requests() -> u32 {
    10
}

/// Configuration for signature-based authentication.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct AuthConfig {
    /// Enable/disable signature-based authentication.
    #[serde(default = "default_auth_enabled")]
    pub enabled: bool,

    /// Maximum age of a signed request in seconds (default: 5 minutes).
    #[serde(default = "default_auth_max_age_secs")]
    pub max_age_secs: i64,

    /// Maximum number of entries in the replay protection cache.
    #[serde(default = "default_auth_replay_cache_size")]
    pub replay_cache_size: usize,

    /// Require an on-chain staking box for authenticated requests.
    /// When false, a valid signature is sufficient for authentication.
    /// When true, the public key must also have a staking box on-chain.
    #[serde(default = "default_auth_require_staking_box")]
    pub require_staking_box: bool,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: default_auth_enabled(),
            max_age_secs: default_auth_max_age_secs(),
            replay_cache_size: default_auth_replay_cache_size(),
            require_staking_box: default_auth_require_staking_box(),
        }
    }
}

fn default_auth_enabled() -> bool {
    true
}
fn default_auth_max_age_secs() -> i64 {
    300 // 5 minutes
}
fn default_auth_replay_cache_size() -> usize {
    10_000
}
fn default_auth_require_staking_box() -> bool {
    false
}

/// Configuration for the rare model incentive system.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct IncentiveConfig {
    /// Enable/disable rarity bonus for PoNW scoring
    #[serde(default = "default_incentive_enabled")]
    pub rarity_bonus_enabled: bool,

    /// Maximum rarity multiplier (capped to prevent gaming, default: 10.0)
    #[serde(default = "default_rarity_max_multiplier")]
    pub rarity_max_multiplier: f64,

    /// Number of providers below which max multiplier applies (default: 1)
    #[serde(default = "default_rarity_min_providers")]
    pub rarity_min_providers: usize,
}

impl Default for IncentiveConfig {
    fn default() -> Self {
        Self {
            rarity_bonus_enabled: default_incentive_enabled(),
            rarity_max_multiplier: default_rarity_max_multiplier(),
            rarity_min_providers: default_rarity_min_providers(),
        }
    }
}

fn default_incentive_enabled() -> bool {
    true
}
fn default_rarity_max_multiplier() -> f64 {
    10.0
}
fn default_rarity_min_providers() -> usize {
    1
}

/// Configuration for rate limiting.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct RateLimitConfig {
    /// Enable/disable rate limiting (default: true)
    #[serde(default = "default_rate_limit_enabled")]
    pub enabled: bool,
    /// Requests per minute per IP (default: 30)
    #[serde(default = "default_rate_limit_ip_rpm")]
    pub ip_rpm: u32,
    /// Burst capacity per IP (default: 10)
    #[serde(default = "default_rate_limit_ip_burst")]
    pub ip_burst: u32,
    /// Requests per minute per API key (default: 120)
    #[serde(default = "default_rate_limit_key_rpm")]
    pub key_rpm: u32,
    /// Burst capacity per API key (default: 30)
    #[serde(default = "default_rate_limit_key_burst")]
    pub key_burst: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: default_rate_limit_enabled(),
            ip_rpm: default_rate_limit_ip_rpm(),
            ip_burst: default_rate_limit_ip_burst(),
            key_rpm: default_rate_limit_key_rpm(),
            key_burst: default_rate_limit_key_burst(),
        }
    }
}

fn default_rate_limit_enabled() -> bool {
    true
}
fn default_rate_limit_ip_rpm() -> u32 {
    30
}
fn default_rate_limit_ip_burst() -> u32 {
    10
}
fn default_rate_limit_key_rpm() -> u32 {
    120
}
fn default_rate_limit_key_burst() -> u32 {
    30
}

/// Configuration for the Ergo oracle integration (ERG/USD price feed).
#[derive(Debug, Clone, Deserialize)]
pub struct OracleConfig {
    /// Optional oracle pool NFT token ID for ERG/USD price feed.
    /// When set, the relay fetches the current rate from the oracle pool box.
    pub pool_nft_token_id: Option<String>,
    /// How often to refresh the oracle rate (seconds, default: 300 = 5 min).
    #[serde(default = "default_oracle_refresh_secs")]
    pub refresh_secs: u64,
}

impl Default for OracleConfig {
    fn default() -> Self {
        Self {
            pool_nft_token_id: None,
            refresh_secs: default_oracle_refresh_secs(),
        }
    }
}

fn default_oracle_refresh_secs() -> u64 {
    300
}

/// Configuration for the free tier request tracker.
///
/// New users who received an airdrop get a limited number of free
/// inference requests before needing to deposit ERG.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct FreeTierConfig {
    /// Enable/disable free tier tracking (default: true)
    #[serde(default = "default_free_tier_tracker_enabled")]
    pub enabled: bool,

    /// Maximum free requests per window (default: 100)
    #[serde(default = "default_free_tier_max_requests")]
    pub max_requests: u64,

    /// Hours after which the request counter resets (default: 24)
    #[serde(default = "default_free_tier_decay_hours")]
    pub decay_hours: u64,
}

impl Default for FreeTierConfig {
    fn default() -> Self {
        Self {
            enabled: default_free_tier_tracker_enabled(),
            max_requests: default_free_tier_max_requests(),
            decay_hours: default_free_tier_decay_hours(),
        }
    }
}

fn default_free_tier_tracker_enabled() -> bool {
    true
}
fn default_free_tier_max_requests() -> u64 {
    100
}
fn default_free_tier_decay_hours() -> u64 {
    24
}

/// Foreign chain type for cross-chain payment bridge.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BridgeForeignChain {
    Btc,
    Eth,
    Ada,
}

/// Configuration for the cross-chain payment bridge.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct BridgeConfig {
    /// Enable cross-chain payment bridge (default: false)
    #[serde(default)]
    pub enabled: bool,
    /// Bridge operator public key (hex, for confirmations)
    #[serde(default)]
    pub bridge_public_key: String,
    /// Supported foreign chains (default: btc, eth, ada)
    #[serde(default = "default_bridge_supported_chains")]
    pub supported_chains: Vec<BridgeForeignChain>,
    /// Invoice timeout in blocks before buyer can refund (default: 720)
    #[serde(default = "default_bridge_timeout_blocks")]
    pub invoice_timeout_blocks: u32,
    /// Compiled payment_bridge.es ErgoTree hex
    #[serde(default)]
    pub invoice_tree_hex: String,
}

fn default_bridge_supported_chains() -> Vec<BridgeForeignChain> {
    vec![BridgeForeignChain::Btc, BridgeForeignChain::Eth, BridgeForeignChain::Ada]
}

fn default_bridge_timeout_blocks() -> u32 {
    720 // ~24 hours at 2-minute block time
}

impl Default for BridgeConfig {
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

/// Configuration for the SSE events endpoint (GET /v1/events).
#[derive(Debug, Clone, Deserialize)]
pub struct EventsConfig {
    /// Enable/disable the SSE events endpoint (default: true).
    #[serde(default = "default_events_enabled")]
    pub enabled: bool,
    /// Maximum number of concurrent SSE subscribers (default: 1000).
    #[serde(default = "default_events_max_subscribers")]
    pub max_subscribers: usize,
}

impl Default for EventsConfig {
    fn default() -> Self {
        Self {
            enabled: default_events_enabled(),
            max_subscribers: default_events_max_subscribers(),
        }
    }
}

fn default_events_enabled() -> bool {
    true
}
fn default_events_max_subscribers() -> usize {
    1000
}

/// Configuration for the latency-aware health scoring v2 model.
///
/// When enabled, routing uses a multi-dimensional health score with
/// exponential decay for staleness instead of the simple linear recency
/// penalty from v1.
#[derive(Debug, Clone, Deserialize)]
pub struct HealthV2Config {
    /// Enable/disable health scoring v2 (default: true).
    /// When false, falls back to the legacy v1 scoring model.
    #[serde(default = "default_health_v2_enabled")]
    pub enabled: bool,

    /// Weight for latency score in overall health (default: 0.4).
    /// Sigmoid maps: 50ms -> ~1.0, 500ms -> ~0.5, 2000ms -> ~0.1
    #[serde(default = "default_health_v2_latency_weight")]
    pub latency_weight: f64,

    /// Weight for success rate score (default: 0.3).
    /// 100% success -> 1.0, 90% -> 0.8, 50% -> 0.0
    #[serde(default = "default_health_v2_success_weight")]
    pub success_weight: f64,

    /// Weight for staleness/exponential decay score (default: 0.2).
    /// Decays over `staleness_decay_minutes` since last heartbeat.
    #[serde(default = "default_health_v2_staleness_weight")]
    pub staleness_weight: f64,

    /// Weight for PoNW score (default: 0.1).
    #[serde(default = "default_health_v2_ponw_weight")]
    pub ponw_weight: f64,

    /// Minutes over which staleness score decays from 1.0 -> ~0.37 (default: 5).
    /// Uses exponential decay: score = e^(-t / decay_minutes).
    #[serde(default = "default_health_v2_staleness_decay_minutes")]
    pub staleness_decay_minutes: f64,

    /// Maximum number of latency samples retained per provider (default: 100).
    #[serde(default = "default_health_v2_max_latency_samples")]
    pub max_latency_samples: usize,
}

impl Default for HealthV2Config {
    fn default() -> Self {
        Self {
            enabled: default_health_v2_enabled(),
            latency_weight: default_health_v2_latency_weight(),
            success_weight: default_health_v2_success_weight(),
            staleness_weight: default_health_v2_staleness_weight(),
            ponw_weight: default_health_v2_ponw_weight(),
            staleness_decay_minutes: default_health_v2_staleness_decay_minutes(),
            max_latency_samples: default_health_v2_max_latency_samples(),
        }
    }
}

fn default_health_v2_enabled() -> bool {
    true
}
fn default_health_v2_latency_weight() -> f64 {
    0.4
}
fn default_health_v2_success_weight() -> f64 {
    0.3
}
fn default_health_v2_staleness_weight() -> f64 {
    0.2
}
fn default_health_v2_ponw_weight() -> f64 {
    0.1
}
fn default_health_v2_staleness_decay_minutes() -> f64 {
    5.0
}
fn default_health_v2_max_latency_samples() -> usize {
    100
}

/// Configuration for multi-relay gossip consensus.
///
/// When GOSSIP_PEERS is non-empty, this relay participates in a gossip
/// protocol with peer relays to reach consensus on provider health status.
/// When empty (default), the relay operates in solo mode.
#[derive(Debug, Clone, Deserialize)]
pub struct GossipConfig {
    /// Enable/disable gossip protocol (default: false).
    /// Enabled automatically when peers are configured.
    #[serde(default)]
    pub enabled: bool,

    /// Comma-separated list of peer relay URLs for gossip (env: GOSSIP_PEERS).
    /// Example: "http://relay2:8080,http://relay3:8080"
    #[serde(default)]
    pub peers: Vec<String>,

    /// Seconds between gossip rounds (default: 30).
    #[serde(default = "default_gossip_interval")]
    pub interval_secs: u64,

    /// Unique ID for this relay instance (default: auto-generated UUID).
    #[serde(default)]
    pub relay_id: String,
}

impl Default for GossipConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            peers: Vec::new(),
            interval_secs: default_gossip_interval(),
            relay_id: uuid::Uuid::new_v4().to_string(),
        }
    }
}

fn default_gossip_interval() -> u64 {
    30
}

/// Configuration for WebSocket chat transport (GET /v1/chat/ws).
#[derive(Debug, Clone, Deserialize)]
pub struct WsChatConfig {
    /// Enable/disable WebSocket chat endpoint (default: true).
    #[serde(default = "default_ws_chat_enabled")]
    pub enabled: bool,
    /// Maximum concurrent WebSocket chat connections (default: 1000).
    #[serde(default = "default_ws_chat_max_connections")]
    pub max_connections: usize,
}

impl Default for WsChatConfig {
    fn default() -> Self {
        Self {
            enabled: default_ws_chat_enabled(),
            max_connections: default_ws_chat_max_connections(),
        }
    }
}

fn default_ws_chat_enabled() -> bool {
    true
}
fn default_ws_chat_max_connections() -> usize {
    1000
}

/// Configuration for WebSocket connection pooling to backend providers.
/// See [`crate::ws_pool::WsPoolConfig`] for field documentation.
pub use crate::ws_pool::WsPoolConfig;

/// Configuration for request deduplication.
#[derive(Debug, Clone, Deserialize)]
pub struct DedupConfig {
    /// Enable/disable request deduplication (default: true).
    #[serde(default = "default_dedup_enabled")]
    pub enabled: bool,
    /// Time window in seconds for considering requests as duplicates (default: 30).
    #[serde(default = "default_dedup_window_secs")]
    pub window_secs: u64,
    /// TTL-based dedup window in seconds (default: 30).
    /// Don't dedup requests that were registered longer ago than this.
    #[serde(default = "default_dedup_ttl_secs")]
    pub dedup_ttl_secs: u64,
}

impl Default for DedupConfig {
    fn default() -> Self {
        Self {
            enabled: default_dedup_enabled(),
            window_secs: default_dedup_window_secs(),
            dedup_ttl_secs: default_dedup_ttl_secs(),
        }
    }
}

fn default_dedup_enabled() -> bool {
    true
}
fn default_dedup_window_secs() -> u64 {
    30
}
fn default_dedup_ttl_secs() -> u64 {
    30
}

/// Configuration for the response cache.
#[derive(Debug, Clone, Deserialize)]
pub struct CacheConfig {
    /// Enable/disable response caching (default: true).
    #[serde(default = "default_cache_enabled")]
    pub enabled: bool,
    /// Maximum number of cache entries (default: 10000).
    #[serde(default = "default_cache_max_entries")]
    pub max_entries: usize,
    /// Default TTL for cached responses in seconds (default: 60).
    #[serde(default = "default_cache_default_ttl")]
    pub default_ttl_secs: u64,
    /// TTL for /v1/models in seconds (default: 30).
    #[serde(default = "default_cache_model_ttl")]
    pub model_list_ttl_secs: u64,
    /// TTL for /v1/providers in seconds (default: 15).
    #[serde(default = "default_cache_provider_ttl")]
    pub provider_list_ttl_secs: u64,
    /// TTL for /v1/health in seconds (default: 5).
    #[serde(default = "default_cache_health_ttl")]
    pub health_ttl_secs: u64,
    /// Maximum size of a single cached entry in bytes (default: 102400 = 100KB).
    #[serde(default = "default_cache_max_entry_size")]
    pub max_entry_size_bytes: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: default_cache_enabled(),
            max_entries: default_cache_max_entries(),
            default_ttl_secs: default_cache_default_ttl(),
            model_list_ttl_secs: default_cache_model_ttl(),
            provider_list_ttl_secs: default_cache_provider_ttl(),
            health_ttl_secs: default_cache_health_ttl(),
            max_entry_size_bytes: default_cache_max_entry_size(),
        }
    }
}

fn default_cache_enabled() -> bool {
    true
}
fn default_cache_max_entries() -> usize {
    10_000
}
fn default_cache_default_ttl() -> u64 {
    60
}
fn default_cache_model_ttl() -> u64 {
    30
}
fn default_cache_provider_ttl() -> u64 {
    15
}
fn default_cache_health_ttl() -> u64 {
    5
}
fn default_cache_max_entry_size() -> usize {
    102_400 // 100KB
}

/// Configuration for the standalone circuit breaker (per-provider fault tolerance).
/// This supplements the provider-embedded circuit breaker with metrics and
/// a centralized management API.
pub use crate::circuit_breaker::CircuitBreakerConfig;

/// Configuration for load shedding and request prioritization.
pub use crate::load_shed::LoadShedConfig;

/// Configuration for graceful degradation.
pub use crate::degradation::DegradationConfig;

/// Configuration for request coalescing.
pub use crate::coalesce::CoalesceConfig;

/// Configuration for stream buffering.
pub use crate::stream_buffer::StreamBufferConfig;

/// Configuration for the adaptive routing system.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AdaptiveRoutingConfig {
    /// Whether the AdaptiveRouter is used for provider selection in the proxy path.
    /// When disabled, the legacy ProviderRegistry scoring is used. (default: true)
    #[serde(default = "default_adaptive_enabled")]
    pub enabled: bool,

    /// Routing strategy (default: health_score)
    #[serde(default = "default_adaptive_strategy")]
    pub strategy: String,

    /// Enable geo-based routing (default: true)
    #[serde(default = "default_adaptive_geo_enabled")]
    pub geo_routing_enabled: bool,

    /// Number of fallback providers to try (default: 3)
    #[serde(default = "default_adaptive_fallback_count")]
    pub fallback_count: u32,

    /// Sticky session TTL in seconds (default: 300)
    #[serde(default = "default_adaptive_sticky_ttl")]
    pub sticky_session_ttl_secs: u64,

    /// Circuit breaker threshold: health score below this triggers circuit break (default: 0.1)
    #[serde(default = "default_adaptive_circuit_threshold")]
    pub circuit_breaker_threshold: f64,
}

impl Default for AdaptiveRoutingConfig {
    fn default() -> Self {
        Self {
            enabled: default_adaptive_enabled(),
            strategy: default_adaptive_strategy(),
            geo_routing_enabled: default_adaptive_geo_enabled(),
            fallback_count: default_adaptive_fallback_count(),
            sticky_session_ttl_secs: default_adaptive_sticky_ttl(),
            circuit_breaker_threshold: default_adaptive_circuit_threshold(),
        }
    }
}

fn default_adaptive_enabled() -> bool {
    true
}

fn default_adaptive_strategy() -> String {
    "health_score".into()
}
fn default_adaptive_geo_enabled() -> bool {
    true
}
fn default_adaptive_fallback_count() -> u32 {
    3
}
fn default_adaptive_sticky_ttl() -> u64 {
    300
}
fn default_adaptive_circuit_threshold() -> f64 {
    0.1
}

/// Configuration for the admin API.
#[derive(Debug, Clone, Deserialize)]
pub struct AdminConfig {
    /// Enable/disable the admin API (default: false).
    #[serde(default)]
    pub enabled: bool,
    /// API key required for admin endpoints (sent as `X-Admin-Key` header).
    /// When empty, admin API is disabled regardless of `enabled`.
    #[serde(default)]
    pub api_key: String,
}

impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_key: String::new(),
        }
    }
}

/// Configuration for OpenTelemetry distributed tracing.
#[derive(Debug, Clone, Deserialize)]
pub struct TelemetryConfig {
    /// Enable/disable OpenTelemetry tracing (default: false).
    /// Also enabled when OTEL_EXPORTER_OTLP_ENDPOINT env var is set.
    #[serde(default)]
    pub enabled: bool,

    /// OTLP gRPC exporter endpoint (default: "http://localhost:4317").
    /// Overridden by OTEL_EXPORTER_OTLP_ENDPOINT env var if set.
    #[serde(default = "default_telemetry_endpoint")]
    pub otlp_endpoint: String,

    /// OpenTelemetry service name (default: "xergon-relay").
    /// Overridden by OTEL_SERVICE_NAME env var if set.
    #[serde(default = "default_telemetry_service_name")]
    pub service_name: String,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            otlp_endpoint: default_telemetry_endpoint(),
            service_name: default_telemetry_service_name(),
        }
    }
}

fn default_telemetry_endpoint() -> String {
    "http://localhost:4317".into()
}

fn default_telemetry_service_name() -> String {
    "xergon-relay".into()
}

/// Check whether a CORS wildcard warning should be emitted.
/// Extracted from `load()` for testability.
#[cfg(test)]
pub(crate) fn should_warn_cors_wildcard(cors_origins: &str, is_dev: bool) -> bool {
    cors_origins == "*" && !is_dev
}

impl RelayConfig {
    /// Validate configuration values and return a descriptive error for any problem.
    pub fn validate(&self) -> Result<(), String> {
        // 1. relay.listen_addr must be parseable as a SocketAddr
        if self.relay.listen_addr.parse::<std::net::SocketAddr>().is_err() {
            return Err(format!(
                "relay.listen_addr \"{}\" is not a valid SocketAddr (expected host:port)",
                self.relay.listen_addr
            ));
        }

        // 2. relay.health_poll_interval_secs must be >= 5
        if self.relay.health_poll_interval_secs < 5 {
            return Err(format!(
                "relay.health_poll_interval_secs must be >= 5, got {}",
                self.relay.health_poll_interval_secs
            ));
        }

        // 3. relay.provider_timeout_secs must be >= 1
        if self.relay.provider_timeout_secs < 1 {
            return Err(format!(
                "relay.provider_timeout_secs must be >= 1, got {}",
                self.relay.provider_timeout_secs
            ));
        }

        // 4. relay.max_fallback_attempts must be >= 1
        if self.relay.max_fallback_attempts < 1 {
            return Err(format!(
                "relay.max_fallback_attempts must be >= 1, got {}",
                self.relay.max_fallback_attempts
            ));
        }

        // 5. If chain.enabled, validate chain config
        if self.chain.enabled {
            let url = &self.chain.ergo_node_url;
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return Err(format!(
                    "chain.ergo_node_url \"{}\" must start with http:// or https://",
                    url
                ));
            }
            if url::Url::parse(url).is_err() {
                return Err(format!(
                    "chain.ergo_node_url \"{}\" is not a valid URL",
                    url
                ));
            }
        }

        // 6. oracle.pool_nft_token_id if set, must be a valid hex string (64 chars)
        if let Some(ref token_id) = self.oracle.pool_nft_token_id {
            let trimmed = token_id.trim();
            if trimmed.len() != 64 || !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(format!(
                    "oracle.pool_nft_token_id \"{}\" must be a 64-character hex string",
                    token_id
                ));
            }
        }

        // 7. oracle.refresh_secs must be >= 60
        if self.oracle.refresh_secs < 60 {
            return Err(format!(
                "oracle.refresh_secs must be >= 60, got {}",
                self.oracle.refresh_secs
            ));
        }

        Ok(())
    }

    pub fn load() -> anyhow::Result<Self> {
        let config_path = std::env::var("XERGON_RELAY_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("config.toml"));

        let settings = if config_path.exists() {
            config::Config::builder()
                .add_source(config::File::from(config_path))
                .add_source(
                    config::Environment::with_prefix("XERGON_RELAY")
                        .separator("__")
                        .try_parsing(true),
                )
                .build()?
        } else {
            config::Config::builder()
                .add_source(
                    config::Environment::with_prefix("XERGON_RELAY")
                        .separator("__")
                        .try_parsing(true),
                )
                .build()?
        };

        let config: Self = settings.try_deserialize()?;

        // -- Security: validate secrets are not defaults in production --
        let is_dev = std::env::var("XERGON_ENV")
            .map(|v| v == "development")
            .unwrap_or(false);

        if config.relay.cors_origins == "*" && !is_dev {
            tracing::warn!("SECURITY: CORS allows all origins - restrict in production");
        }

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cors_wildcard_warns_in_production() {
        assert!(should_warn_cors_wildcard("*", false));
    }

    #[test]
    fn test_cors_wildcard_no_warn_in_development() {
        assert!(!should_warn_cors_wildcard("*", true));
    }

    #[test]
    fn test_cors_specific_origin_no_warn() {
        assert!(!should_warn_cors_wildcard("https://xergon.ai", false));
        assert!(!should_warn_cors_wildcard("https://xergon.ai", true));
    }

    #[test]
    fn test_cors_comma_separated_no_warn() {
        assert!(!should_warn_cors_wildcard(
            "https://a.com,https://b.com",
            false
        ));
    }

    #[test]
    fn test_cors_empty_no_warn() {
        assert!(!should_warn_cors_wildcard("", false));
    }

    #[test]
    fn test_chain_config_defaults() {
        let config = ChainConfig::default();
        assert!(config.enabled);
        assert_eq!(config.ergo_node_url, "http://127.0.0.1:9053");
        assert_eq!(config.scan_interval_secs, 30);
        assert_eq!(config.cache_ttl_secs, 10);
        assert!(config.provider_tree_bytes.is_empty());
        assert!(config.gpu_listing_tree_bytes.is_empty());
        assert!(config.gpu_rental_tree_bytes.is_empty());
        assert!(config.agent_gpu_endpoint.is_empty());
    }

    #[test]
    fn test_balance_config_defaults() {
        let config = BalanceConfig::default();
        assert!(config.enabled);
        assert_eq!(config.ergo_node_url, "http://127.0.0.1:9053");
        assert_eq!(config.min_balance_nanoerg, 1_000_000);
        assert_eq!(config.cache_ttl_secs, 30);
        assert!(config.free_tier_enabled);
        assert_eq!(config.free_tier_requests, 10);
        assert!(config.staking_tree_bytes.is_empty());
    }

    #[test]
    fn test_auth_config_defaults() {
        let config = AuthConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_age_secs, 300);
        assert_eq!(config.replay_cache_size, 10_000);
        assert!(!config.require_staking_box);
    }

    #[test]
    fn test_rate_limit_config_defaults() {
        let config = RateLimitConfig::default();
        assert!(config.enabled);
        assert_eq!(config.ip_rpm, 30);
        assert_eq!(config.ip_burst, 10);
        assert_eq!(config.key_rpm, 120);
        assert_eq!(config.key_burst, 30);
    }

    #[test]
    fn test_oracle_config_defaults() {
        let config = OracleConfig::default();
        assert!(config.pool_nft_token_id.is_none());
        assert_eq!(config.refresh_secs, 300);
    }

    // ---- Config validation tests ----

    fn make_test_relay_config() -> RelayConfig {
        use config::Config;
        // Build a minimal valid config via TOML string
        let toml_str = r#"
[relay]
listen_addr = "0.0.0.0:8080"
health_poll_interval_secs = 30
provider_timeout_secs = 30
max_fallback_attempts = 3

[providers]
known_endpoints = ["http://127.0.0.1:9099"]

[chain]
enabled = false

[oracle]
refresh_secs = 300
"#;
        let settings = Config::builder()
            .add_source(config::File::from_str(toml_str, config::FileFormat::Toml))
            .build()
            .unwrap();
        settings.try_deserialize().unwrap()
    }

    #[test]
    fn test_validate_ok_with_defaults() {
        let config = make_test_relay_config();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_bad_listen_addr() {
        let mut config = make_test_relay_config();
        config.relay.listen_addr = "not-an-addr".into();
        assert!(config.validate().is_err());
        let err = config.validate().unwrap_err();
        assert!(err.contains("listen_addr"), "error should mention listen_addr: {err}");
    }

    #[test]
    fn test_validate_health_poll_too_low() {
        let mut config = make_test_relay_config();
        config.relay.health_poll_interval_secs = 2;
        assert!(config.validate().is_err());
        let err = config.validate().unwrap_err();
        assert!(err.contains("health_poll_interval_secs"), "{err}");
    }

    #[test]
    fn test_validate_provider_timeout_zero() {
        let mut config = make_test_relay_config();
        config.relay.provider_timeout_secs = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_max_fallback_zero() {
        let mut config = make_test_relay_config();
        config.relay.max_fallback_attempts = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_chain_url_bad_when_enabled() {
        let mut config = make_test_relay_config();
        config.chain.enabled = true;
        config.chain.ergo_node_url = "ftp://bad".into();
        assert!(config.validate().is_err());
        let err = config.validate().unwrap_err();
        assert!(err.contains("ergo_node_url"), "{err}");
    }

    #[test]
    fn test_validate_chain_disabled_skips_url_check() {
        let mut config = make_test_relay_config();
        config.chain.enabled = false;
        config.chain.ergo_node_url = "not-a-url".into();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_oracle_bad_token_id() {
        let mut config = make_test_relay_config();
        config.oracle.pool_nft_token_id = Some("short".into());
        assert!(config.validate().is_err());
        let err = config.validate().unwrap_err();
        assert!(err.contains("pool_nft_token_id"), "{err}");
    }

    #[test]
    fn test_validate_oracle_refresh_too_low() {
        let mut config = make_test_relay_config();
        config.oracle.refresh_secs = 30;
        assert!(config.validate().is_err());
        let err = config.validate().unwrap_err();
        assert!(err.contains("refresh_secs"), "{err}");
    }

    #[test]
    fn test_validate_oracle_good_token_id() {
        let mut config = make_test_relay_config();
        let hex_id = "0".repeat(64);
        config.oracle.pool_nft_token_id = Some(hex_id);
        assert!(config.validate().is_ok());
    }
}
