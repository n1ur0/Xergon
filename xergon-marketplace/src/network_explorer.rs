//! Network Explorer Dashboard
//!
//! Provides a blockchain explorer for the Xergon network:
//! Block browser, transaction viewer, provider box inspector,
//! chain statistics overview.
//!
//! REST endpoints:
//! - GET /v1/explorer/blocks/:height — Get block at height
//! - GET /v1/explorer/blocks          — List recent blocks with pagination
//! - GET /v1/explorer/tx/:id          — Get transaction details
//! - GET /v1/explorer/box/:id         — Get box details with decoded registers
//! - GET /v1/explorer/provider/:nft   — Get provider box by NFT ID
//! - GET /v1/explorer/stats           — Network statistics overview

use axum::{
    extract::{Path, Query, State},
    Json,
    Router,
    routing::{get, post},
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// ================================================================
// Types
// ================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockInfo {
    pub height: u64,
    pub timestamp: String,
    pub tx_count: u32,
    pub miner_address: String,
    pub block_size_bytes: u64,
    pub difficulty: f64,
    pub main_chain: bool,
    pub header_id: String,
    pub parent_id: String,
    pub ad_proofs_root: String,
    pub transactions_root: String,
    pub extension_hash: String,
    pub votes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionInfo {
    pub tx_id: String,
    pub block_height: u64,
    pub timestamp: String,
    pub index_in_block: u32,
    pub inputs: Vec<TxInput>,
    pub outputs: Vec<TxOutput>,
    pub data_inputs: Vec<TxDataInput>,
    pub size_bytes: u64,
    pub fee_nanoerg: u64,
    pub fee_erg: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxInput {
    pub box_id: String,
    pub spending_proof: Option<String>,
    pub proof_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxDataInput {
    pub box_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxOutput {
    pub box_id: String,
    pub value_nanoerg: u64,
    pub value_erg: f64,
    pub ergo_tree_hex: String,
    pub registers: HashMap<String, RegisterValue>,
    pub tokens: Vec<TokenInfo>,
    pub creation_height: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterValue {
    pub raw_hex: String,
    pub decoded_type: String,
    pub decoded_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    pub token_id: String,
    pub amount: u64,
    pub decimals: u32,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoxInfo {
    pub box_id: String,
    pub transaction_id: String,
    pub value_nanoerg: u64,
    pub value_erg: f64,
    pub ergo_tree_hex: String,
    pub ergo_tree_template: String,
    pub registers: HashMap<String, RegisterValue>,
    pub tokens: Vec<TokenInfo>,
    pub creation_height: u64,
    pub spent: bool,
    pub spending_height: Option<u64>,
    pub spending_tx_id: Option<String>,
    pub box_size_bytes: u64,
    pub min_value_nanoerg: u64,
    pub address: String,
    pub protocol_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderBoxInfo {
    pub base: BoxInfo,
    pub provider_pubkey: Option<String>,
    pub endpoint_url: Option<String>,
    pub models_served: Vec<String>,
    pub ponw_score: Option<i32>,
    pub last_heartbeat_height: Option<u64>,
    pub region: Option<String>,
    pub is_active: bool,
    pub rent_status: RentStatus,
    pub chain_age_blocks: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RentStatus {
    pub min_value_required: u64,
    pub current_value: u64,
    pub cycles_remaining: f64,
    pub status: String, // "healthy", "warning", "critical"
    pub blocks_until_critical: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    pub current_height: u64,
    pub total_blocks: u64,
    pub total_transactions: u64,
    pub total_boxes: u64,
    pub total_xergon_providers: u64,
    pub active_providers: u64,
    pub total_erg_in_boxes: u64,
    pub total_erg_in_boxes_display: f64,
    pub avg_block_size: u64,
    pub avg_tx_count_per_block: f64,
    pub total_xrg_tokens: u64,
    pub protocol_boxes: ProtocolBoxCounts,
    pub network_uptime_pct: f64,
    pub last_updated: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolBoxCounts {
    pub provider_boxes: u64,
    pub user_staking_boxes: u64,
    pub usage_proof_boxes: u64,
    pub treasury_boxes: u64,
    pub governance_boxes: u64,
    pub slashing_boxes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationParams {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
}

// ================================================================
// Explorer State
// ================================================================

pub struct ExplorerState {
    current_height: AtomicU64,
    blocks: DashMap<u64, BlockInfo>,
    transactions: DashMap<String, TransactionInfo>,
    boxes: DashMap<String, BoxInfo>,
    stats: tokio::sync::RwLock<NetworkStats>,
}

impl ExplorerState {
    pub fn new() -> Self {
        let stats = NetworkStats {
            current_height: 0,
            total_blocks: 0,
            total_transactions: 0,
            total_boxes: 0,
            total_xergon_providers: 0,
            active_providers: 0,
            total_erg_in_boxes: 0,
            total_erg_in_boxes_display: 0.0,
            avg_block_size: 0,
            avg_tx_count_per_block: 0.0,
            total_xrg_tokens: 0,
            protocol_boxes: ProtocolBoxCounts {
                provider_boxes: 0,
                user_staking_boxes: 0,
                usage_proof_boxes: 0,
                treasury_boxes: 0,
                governance_boxes: 0,
                slashing_boxes: 0,
            },
            network_uptime_pct: 99.95,
            last_updated: chrono::Utc::now().to_rfc3339(),
        };
        let state = Self {
            current_height: AtomicU64::new(100_000),
            blocks: DashMap::new(),
            transactions: DashMap::new(),
            boxes: DashMap::new(),
            stats: tokio::sync::RwLock::new(stats),
        };
        state.seed_demo_data();
        state
    }

    fn seed_demo_data(&self) {
        // Seed some demo blocks
        for h in 99_990..=100_000u64 {
            let block = BlockInfo {
                height: h,
                timestamp: format!("2026-04-0{}T12:00:00Z", (h % 10) + 1),
                tx_count: 3 + (h % 7) as u32,
                miner_address: format!("3W{}...", h % 1000),
                block_size_bytes: 2048 + ((h * 17) % 4096),
                difficulty: 1_000_000.0 + (h as f64 * 0.01),
                main_chain: true,
                header_id: format!("header-{}", h),
                parent_id: if h > 0 { format!("header-{}", h - 1) } else { "genesis".to_string() },
                ad_proofs_root: format!("aproof-{}", h),
                transactions_root: format!("txroot-{}", h),
                extension_hash: format!("ext-{}", h),
                votes: vec!["vote_yes".to_string()],
            };
            self.blocks.insert(h, block);
        }

        // Seed demo transactions
        for i in 0..5u64 {
            let tx = TransactionInfo {
                tx_id: format!("tx-demo-{}", i),
                block_height: 100_000 - i,
                timestamp: format!("2026-04-08T12:{:02}:00Z", i * 10),
                index_in_block: i as u32,
                inputs: vec![TxInput {
                    box_id: format!("input-{}-0", i),
                    spending_proof: Some("proof_hex_...".to_string()),
                    proof_type: "proveDlog".to_string(),
                }],
                outputs: vec![TxOutput {
                    box_id: format!("output-{}-0", i),
                    value_nanoerg: 1_000_000_000,
                    value_erg: 1.0,
                    ergo_tree_hex: "0001cd02e8ec".to_string(),
                    registers: HashMap::new(),
                    tokens: vec![],
                    creation_height: 100_000 - i,
                }],
                data_inputs: vec![],
                size_bytes: 512 + (i * 64),
                fee_nanoerg: 1_000_000,
                fee_erg: 0.001,
            };
            self.transactions.insert(tx.tx_id.clone(), tx);
        }

        // Seed a demo provider box
        let mut regs = HashMap::new();
        regs.insert("R4".to_string(), RegisterValue {
            raw_hex: "0e0b02e8ec6e8a4b7".to_string(),
            decoded_type: "GroupElement".to_string(),
            decoded_value: "0e0b02e8ec6e8a4b7...".to_string(),
        });
        regs.insert("R5".to_string(), RegisterValue {
            raw_hex: "0e0568747470733a2f2f70726f762e786572676f6e2e6e6574".to_string(),
            decoded_type: "SString".to_string(),
            decoded_value: "https://prov.xergon.net".to_string(),
        });
        regs.insert("R6".to_string(), RegisterValue {
            raw_hex: "0e055b226c6c616d612d332d38622d32225d".to_string(),
            decoded_type: "SString".to_string(),
            decoded_value: "[\"llama-3-8b-2\"]".to_string(),
        });
        regs.insert("R7".to_string(), RegisterValue {
            raw_hex: "0e29c2d101".to_string(),
            decoded_type: "SInt".to_string(),
            decoded_value: "850".to_string(),
        });
        regs.insert("R8".to_string(), RegisterValue {
            raw_hex: "0e29c2d101".to_string(),
            decoded_type: "SInt".to_string(),
            decoded_value: "99998".to_string(),
        });
        regs.insert("R9".to_string(), RegisterValue {
            raw_hex: "0e0575732d7765737432".to_string(),
            decoded_type: "SString".to_string(),
            decoded_value: "us-west2".to_string(),
        });

        let provider_box = BoxInfo {
            box_id: "provider-box-demo-001".to_string(),
            transaction_id: "tx-demo-reg".to_string(),
            value_nanoerg: 2_000_000_000,
            value_erg: 2.0,
            ergo_tree_hex: "0001cd02e8ec6e8a4b7abcdef...".to_string(),
            ergo_tree_template: "sigmaProp(proveDlog(R4))".to_string(),
            registers: regs,
            tokens: vec![TokenInfo {
                token_id: "provider-nft-001".to_string(),
                amount: 1,
                decimals: 0,
                name: Some("ProviderNFT".to_string()),
            }],
            creation_height: 99_500,
            spent: false,
            spending_height: None,
            spending_tx_id: None,
            box_size_bytes: 512,
            min_value_nanoerg: 184_320,
            address: "3WxergonProvider1...".to_string(),
            protocol_type: Some("provider_box".to_string()),
        };
        self.boxes.insert(provider_box.box_id.clone(), provider_box);
    }

    /// Decode raw register hex to human-readable value
    #[allow(dead_code)]
    fn decode_register(hex: &str) -> RegisterValue {
        let clean = hex.trim_start_matches("0x");
        let decoded_type = if clean.starts_with("0e08cd02") { "SigmaProp".to_string() }
                          else if clean.starts_with("0e0b") { "GroupElement".to_string() }
                          else if clean.starts_with("0e21") { "SLong".to_string() }
                          else if clean.starts_with("0e29") { "SInt".to_string() }
                          else if clean.starts_with("0e0c") { "SBoolean".to_string() }
                          else if clean.starts_with("0e08") || clean.starts_with("0e05") { "Coll[Byte]/SString".to_string() }
                          else { "Unknown".to_string() };

        // Try UTF-8 decode for strings
        let decoded_value = if decoded_type == "Coll[Byte]/SString" && clean.len() > 4 {
            let bytes = (4..clean.len()).step_by(2)
                .filter_map(|i| u8::from_str_radix(&clean[i..i+2], 16).ok())
                .collect::<Vec<u8>>();
            String::from_utf8(bytes).unwrap_or_else(|_| format!("0x{}", &clean[..16]))
        } else if decoded_type == "SInt" && clean.len() >= 6 {
            let val = i32::from_str_radix(&clean[4..8], 16).unwrap_or(0);
            val.to_string()
        } else if decoded_type == "SLong" && clean.len() >= 6 {
            let val = i64::from_str_radix(&clean[4..12], 16).unwrap_or(0);
            val.to_string()
        } else if clean.len() > 16 {
            format!("0x{}...", &clean[..16])
        } else {
            format!("0x{}", clean)
        };

        RegisterValue {
            raw_hex: hex.to_string(),
            decoded_type,
            decoded_value,
        }
    }

    /// Get provider-specific box info
    fn enrich_provider_box(&self, box_info: &BoxInfo) -> ProviderBoxInfo {
        let current_height = self.current_height.load(Ordering::SeqCst);
        let mut models = Vec::new();
        let mut ponw = None;
        let mut last_hb = None;
        let mut region = None;
        let mut endpoint = None;
        let mut pubkey = None;

        if let Some(r6) = box_info.registers.get("R6") {
            // Parse JSON array of models
            let val = r6.decoded_value.trim_matches('"');
            if val.starts_with('[') && val.ends_with(']') {
                let inner = val[1..val.len()-1].replace("\"", "");
                for part in inner.split(',') {
                    let model = part.trim().trim_matches('"').trim();
                    if !model.is_empty() {
                        models.push(model.to_string());
                    }
                }
            }
        }
        if let Some(r7) = box_info.registers.get("R7") {
            ponw = r7.decoded_value.parse::<i32>().ok();
        }
        if let Some(r8) = box_info.registers.get("R8") {
            last_hb = r8.decoded_value.parse::<u64>().ok();
        }
        if let Some(r9) = box_info.registers.get("R9") {
            region = Some(r9.decoded_value.clone());
        }
        if let Some(r5) = box_info.registers.get("R5") {
            endpoint = Some(r5.decoded_value.clone());
        }
        if let Some(r4) = box_info.registers.get("R4") {
            pubkey = Some(r4.raw_hex.clone());
        }

        let is_active = match last_hb {
            Some(h) => current_height - h < 100, // Active if heartbeat within 100 blocks
            None => false,
        };

        let chain_age = current_height - box_info.creation_height;
        let min_required = box_info.min_value_nanoerg;
        let cycles = if min_required > 0 {
            box_info.value_nanoerg as f64 / min_required as f64
        } else { f64::MAX };

        let rent_status = RentStatus {
            min_value_required: min_required,
            current_value: box_info.value_nanoerg,
            cycles_remaining: cycles,
            status: if cycles > 2.0 { "healthy".to_string() }
                     else if cycles > 1.0 { "warning".to_string() }
                     else { "critical".to_string() },
            blocks_until_critical: if cycles > 1.0 {
                ((cycles - 1.0) * 525_600.0) as u64 // ~4yr cycle in blocks
            } else { 0 },
        };

        ProviderBoxInfo {
            base: box_info.clone(),
            provider_pubkey: pubkey,
            endpoint_url: endpoint,
            models_served: models,
            ponw_score: ponw,
            last_heartbeat_height: last_hb,
            region,
            is_active,
            rent_status,
            chain_age_blocks: chain_age,
        }
    }
}

// ================================================================
// REST Handlers
// ================================================================

pub fn build_router() -> Router {
    let state = Arc::new(ExplorerState::new());
    Router::new()
        .route("/v1/explorer/blocks", post(list_blocks))
        .route("/v1/explorer/blocks/:height", get(get_block))
        .route("/v1/explorer/tx/:id", get(get_transaction))
        .route("/v1/explorer/box/:id", get(get_box))
        .route("/v1/explorer/provider/:nft", get(get_provider_box))
        .route("/v1/explorer/stats", get(get_network_stats))
        .with_state(state)
}

async fn get_block(
    State(state): State<Arc<ExplorerState>>,
    Path(height): Path<u64>,
) -> (axum::http::StatusCode, Json<serde_json::Value>) {
    match state.blocks.get(&height) {
        Some(block) => (axum::http::StatusCode::OK, Json(serde_json::to_value(block.value().clone()).unwrap())),
        None => (axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({"error": format!("Block {} not found", height)}))),
    }
}

#[derive(Deserialize)]
struct BlockListParams {
    offset: Option<u64>,
    limit: Option<u64>,
}

async fn list_blocks(
    State(state): State<Arc<ExplorerState>>,
    Query(params): Query<BlockListParams>,
) -> Json<Vec<BlockInfo>> {
    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(20).min(100);
    let current = state.current_height.load(std::sync::atomic::Ordering::SeqCst);

    let mut blocks = Vec::new();
    let start = current.saturating_sub(offset);
    for h in (start.saturating_sub(limit)..=start).rev() {
        if let Some(block) = state.blocks.get(&h) {
            blocks.push(block.value().clone());
        }
    }
    Json(blocks)
}

async fn get_transaction(
    State(state): State<Arc<ExplorerState>>,
    Path(id): Path<String>,
) -> (axum::http::StatusCode, Json<serde_json::Value>) {
    match state.transactions.get(&id) {
        Some(tx) => (axum::http::StatusCode::OK, Json(serde_json::to_value(tx.value().clone()).unwrap())),
        None => (axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({"error": format!("Transaction {} not found", id)}))),
    }
}

async fn get_box(
    State(state): State<Arc<ExplorerState>>,
    Path(id): Path<String>,
) -> (axum::http::StatusCode, Json<serde_json::Value>) {
    match state.boxes.get(&id) {
        Some(box_info) => (axum::http::StatusCode::OK, Json(serde_json::to_value(box_info.value().clone()).unwrap())),
        None => (axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({"error": format!("Box {} not found", id)}))),
    }
}

async fn get_provider_box(
    State(state): State<Arc<ExplorerState>>,
    Path(nft_id): Path<String>,
) -> (axum::http::StatusCode, Json<serde_json::Value>) {
    // Search boxes for one containing the NFT
    for entry in state.boxes.iter() {
        let box_info = entry.value();
        let has_nft = box_info.tokens.iter().any(|t| t.token_id == nft_id);
        let is_provider = box_info.protocol_type.as_deref() == Some("provider_box");
        if has_nft || is_provider {
            let enriched = state.enrich_provider_box(box_info);
            return (axum::http::StatusCode::OK, Json(serde_json::to_value(enriched).unwrap()));
        }
    }
    // Return demo provider for any query (seeded data)
    if let Some(entry) = state.boxes.get("provider-box-demo-001") {
        let enriched = state.enrich_provider_box(entry.value());
        return (axum::http::StatusCode::OK, Json(serde_json::to_value(enriched).unwrap()));
    }
    (axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({"error": format!("Provider with NFT {} not found", nft_id)})))
}

async fn get_network_stats(
    State(state): State<Arc<ExplorerState>>,
) -> Json<NetworkStats> {
    let stats = state.stats.read().await;
    Json(stats.clone())
}

// ================================================================
// Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_explorer_state_creation() {
        let state = ExplorerState::new();
        assert_eq!(state.current_height.load(Ordering::SeqCst), 100_000);
        assert!(state.blocks.len() > 0);
        assert!(state.transactions.len() > 0);
    }

    #[test]
    fn test_decode_register_group_element() {
        let val = ExplorerState::decode_register("0e0b02e8ec6e8a4b7");
        assert_eq!(val.decoded_type, "GroupElement");
    }

    #[test]
    fn test_decode_register_long() {
        let val = ExplorerState::decode_register("0e210000000123456789");
        assert_eq!(val.decoded_type, "SLong");
    }

    #[test]
    fn test_decode_register_int() {
        let val = ExplorerState::decode_register("0e2900000352");
        assert_eq!(val.decoded_type, "SInt");
    }

    #[test]
    fn test_decode_register_sigma_prop() {
        let val = ExplorerState::decode_register("0e08cd02e8ec");
        assert_eq!(val.decoded_type, "SigmaProp");
    }

    #[test]
    fn test_decode_register_string() {
        let // "test" = 74657374
            val = ExplorerState::decode_register("0e0874657374");
        assert_eq!(val.decoded_type, "Coll[Byte]/SString");
        assert_eq!(val.decoded_value, "test");
    }

    #[test]
    fn test_decode_register_unknown() {
        let val = ExplorerState::decode_register("ffabcd");
        assert_eq!(val.decoded_type, "Unknown");
    }

    #[test]
    fn test_block_info_serialization() {
        let block = BlockInfo {
            height: 100_000,
            timestamp: "2026-04-08T12:00:00Z".to_string(),
            tx_count: 5,
            miner_address: "3Wtest...".to_string(),
            block_size_bytes: 2048,
            difficulty: 1_000_000.0,
            main_chain: true,
            header_id: "header-100000".to_string(),
            parent_id: "header-99999".to_string(),
            ad_proofs_root: "aproof-100000".to_string(),
            transactions_root: "txroot-100000".to_string(),
            extension_hash: "ext-100000".to_string(),
            votes: vec![],
        };
        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains("100000"));
        assert!(json.contains("main_chain"));
    }

    #[test]
    fn test_tx_info_serialization() {
        let tx = TransactionInfo {
            tx_id: "tx-test".to_string(),
            block_height: 100_000,
            timestamp: "2026-04-08T12:00:00Z".to_string(),
            index_in_block: 0,
            inputs: vec![],
            outputs: vec![],
            data_inputs: vec![],
            size_bytes: 512,
            fee_nanoerg: 1_000_000,
            fee_erg: 0.001,
        };
        let json = serde_json::to_string(&tx).unwrap();
        assert!(json.contains("tx-test"));
        assert!(json.contains("0.001"));
    }

    #[test]
    fn test_box_info_serialization() {
        let box_info = BoxInfo {
            box_id: "box-001".to_string(),
            transaction_id: "tx-001".to_string(),
            value_nanoerg: 1_000_000_000,
            value_erg: 1.0,
            ergo_tree_hex: "0001cd02".to_string(),
            ergo_tree_template: "sigmaProp(proveDlog(R4))".to_string(),
            registers: HashMap::new(),
            tokens: vec![],
            creation_height: 100_000,
            spent: false,
            spending_height: None,
            spending_tx_id: None,
            box_size_bytes: 256,
            min_value_nanoerg: 92_160,
            address: "3Wtest...".to_string(),
            protocol_type: Some("provider_box".to_string()),
        };
        let json = serde_json::to_string(&box_info).unwrap();
        assert!(json.contains("provider_box"));
    }

    #[test]
    fn test_enrich_provider_box() {
        let state = ExplorerState::new();
        let box_info = state.boxes.get("provider-box-demo-001").map(|e| e.value().clone());
        if let Some(info) = box_info {
            let enriched = state.enrich_provider_box(&info);
            assert!(!enriched.models_served.is_empty());
            assert!(enriched.ponw_score.is_some());
            assert!(enriched.endpoint_url.is_some());
            assert!(!enriched.rent_status.status.is_empty());
        }
    }

    #[test]
    fn test_rent_status_calculation() {
        let state = ExplorerState::new();
        let box_info = state.boxes.get("provider-box-demo-001").map(|e| e.value().clone());
        if let Some(info) = box_info {
            let enriched = state.enrich_provider_box(&info);
            // 2 ERG / 184320 nanoerg = many cycles
            assert!(enriched.rent_status.cycles_remaining > 1.0);
            assert_eq!(enriched.rent_status.status, "healthy");
        }
    }

    #[test]
    fn test_protocol_box_counts() {
        let counts = ProtocolBoxCounts {
            provider_boxes: 5,
            user_staking_boxes: 100,
            usage_proof_boxes: 1000,
            treasury_boxes: 1,
            governance_boxes: 10,
            slashing_boxes: 2,
        };
        let json = serde_json::to_string(&counts).unwrap();
        assert!(json.contains("provider_boxes"));
    }

    #[test]
    fn test_network_stats() {
        let state = ExplorerState::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let stats = rt.block_on(async {
            let s = state.stats.read().await;
            s.clone()
        });
        assert!(stats.current_height > 0);
        assert!(!stats.last_updated.is_empty());
    }
}
