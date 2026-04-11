use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    pub provider_id: String,
    pub pown_score: Option<f32>,
    pub node_health: Option<NodeHealthStatus>,
    pub timestamp: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeHealthStatus {
    pub is_healthy: bool,
    pub current_height: Option<u32>,
    pub sync_progress: Option<f32>,
    pub last_block_time: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HeartbeatResponse {
    pub status: String,
    pub received_at: Option<u64>,
    pub provider_status: Option<ProviderStatus>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProviderStatus {
    pub provider_id: String,
    pub health_status: String,
    pub last_seen: u64,
    pub pown_score: Option<f32>,
}

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

pub fn get_current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

pub fn calculate_heartbeat_timeout() -> Duration {
    Duration::from_secs(300) // 5 minutes
}

pub fn is_heartbeat_stale(last_seen: u64, timeout_secs: u64) -> bool {
    let now = get_current_timestamp();
    now - last_seen > timeout_secs
}
