use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// Chat completion types
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub temperature: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<Choice>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Choice {
    pub index: usize,
    pub message: Message,
    pub finish_reason: Option<String>,
}

// Provider registration types
#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(dead_code)]
pub struct ProviderRegistration {
    pub provider_id: String,
    pub ergo_address: String,
    pub region: String,
    pub models: Vec<String>,
    #[serde(default)]
    pub capacity_gpus: Option<u32>,
    #[serde(default)]
    pub max_concurrent_requests: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ProviderRegistrationResponse {
    pub success: bool,
    pub provider_id: Option<String>,
    pub message: String,
    pub registered_at: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(dead_code)]
pub struct RegisteredProvider {
    pub provider_id: String,
    pub ergo_address: String,
    pub region: String,
    pub models: Vec<String>,
    pub capacity_gpus: Option<u32>,
    pub max_concurrent_requests: Option<u32>,
    pub registered_at: u64,
    pub last_heartbeat: Option<u64>,
    pub health_status: HealthStatus,
    pub pown_score: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[allow(dead_code)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

// Heartbeat types
#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct HeartbeatRequest {
    pub provider_id: String,
    pub pown_score: Option<f32>,
    pub node_health: Option<NodeHealthStatus>,
    pub timestamp: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(dead_code)]
pub struct NodeHealthStatus {
    pub is_healthy: bool,
    pub current_height: Option<u32>,
    pub sync_progress: Option<f32>,
    pub last_block_time: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct HeartbeatResponse {
    pub status: String,
    pub received_at: Option<u64>,
    pub provider_status: Option<ProviderStatus>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(dead_code)]
pub struct ProviderStatus {
    pub provider_id: String,
    pub health_status: String,
    pub last_seen: u64,
    pub pown_score: Option<f32>,
}

// Settlement types
#[derive(Debug, Serialize, Deserialize)]
pub struct UsageProof {
    pub provider_id: String,
    pub tokens_input: u32,
    pub tokens_output: u32,
    pub timestamp: u64,
    pub inference_id: Option<String>,
    pub model_used: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SettlementRequest {
    pub proofs: Vec<UsageProof>,
    pub provider_signature: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SettlementResponse {
    pub success: bool,
    pub transaction_id: Option<String>,
    pub message: String,
    pub batch_size: usize,
}

// Utility functions
pub fn get_current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[allow(dead_code)]
pub fn calculate_heartbeat_timeout() -> Duration {
    Duration::from_secs(300) // 5 minutes
}

#[allow(dead_code)]
pub fn is_heartbeat_stale(last_seen: u64, timeout_secs: u64) -> bool {
    let now = get_current_timestamp();
    now - last_seen > timeout_secs
}
