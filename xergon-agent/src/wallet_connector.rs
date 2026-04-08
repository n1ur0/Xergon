//! EIP-12 Wallet Connector for Xergon Agent
//!
//! Implements the Ergo dApp Connector standard (EIP-12) for browser wallet
//! integration. Manages wallet discovery, connection lifecycle, and exposes
//! the full Context API for reading wallet state and signing transactions.
//!
//! All wallet operations are simulated (no real wallet connection) for
//! development and testing purposes.

// ================================================================
// Imports
// ================================================================

use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use dashmap::DashMap;
use rand::{Rng, RngCore};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

// ================================================================
// Types - Wallet Enumeration
// ================================================================

/// Supported wallet types implementing EIP-12
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum WalletType {
    Nautilus,
    SAFEW,
    Minotaur,
    #[serde(untagged)]
    Unknown(String),
}

impl std::fmt::Display for WalletType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WalletType::Nautilus => write!(f, "nautilus"),
            WalletType::SAFEW => write!(f, "safew"),
            WalletType::Minotaur => write!(f, "minotaur"),
            WalletType::Unknown(name) => write!(f, "{}", name),
        }
    }
}

// ================================================================
// Types - Wallet Info
// ================================================================

/// Public information about a connected wallet
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletInfo {
    pub name: WalletType,
    pub version: Option<String>,
    pub connected: bool,
    pub session_id: String,
    pub addresses: Vec<String>,
    pub connected_at: i64,
}

// ================================================================
// Types - Session & Context
// ================================================================

/// An active wallet session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletSession {
    pub id: String,
    pub wallet_type: WalletType,
    pub address: String,
    pub access_granted: bool,
    pub created_at: i64,
    pub last_used_at: i64,
    pub context: WalletContext,
}

/// Wallet context: the full EIP-12 Context API surface
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletContext {
    pub balance: u64,
    pub utxos: Vec<UtxoEntry>,
    pub used_addresses: Vec<String>,
}

// ================================================================
// Types - UTXO & Token
// ================================================================

/// A single unspent box entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UtxoEntry {
    pub box_id: String,
    pub value: u64,
    pub ergo_tree: String,
    pub assets: Vec<TokenRef>,
    pub registers: HashMap<String, String>,
    pub creation_height: u32,
}

/// A token reference within a box
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRef {
    pub token_id: String,
    pub amount: u64,
}

// ================================================================
// Types - Transaction Signing
// ================================================================

/// Request to sign a transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignTxRequest {
    pub unsigned_tx: UnsignedTxInput,
    #[serde(default = "default_fee")]
    pub fee: u64,
    #[serde(default)]
    pub inputs: Vec<TxInputBox>,
    #[serde(default)]
    pub data_inputs: Vec<TxInputBox>,
}

fn default_fee() -> u64 {
    1_000_000 // 0.001 ERG default fee
}

/// Unsigned transaction input (EIP-12 format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsignedTxInput {
    pub inputs: Vec<TxInputRef>,
    pub data_inputs: Vec<TxInputRef>,
    pub outputs: Vec<TxOutputRef>,
}

/// Transaction input reference (box_id + extension)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxInputRef {
    pub box_id: String,
    #[serde(default)]
    pub extension: HashMap<String, String>,
}

/// Transaction output reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxOutputRef {
    pub ergo_tree: String,
    pub value: u64,
    #[serde(default)]
    pub assets: Vec<TokenRef>,
    #[serde(default)]
    pub additional_registers: HashMap<String, String>,
    pub creation_height: u32,
}

/// A fully-resolved input box (for signing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxInputBox {
    pub box_id: String,
    pub ergo_tree: String,
    pub value: u64,
    #[serde(default)]
    pub assets: Vec<TokenRef>,
    #[serde(default)]
    pub additional_registers: HashMap<String, String>,
    pub creation_height: u32,
}

// ================================================================
// Types - Signing & Submission Results
// ================================================================

/// Result of signing a transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignTxResult {
    pub tx_id: String,
    pub signed_inputs: Vec<SignedInputRef>,
}

/// A signed input with proof bytes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedInputRef {
    pub box_id: String,
    pub proof_bytes: String,
    pub extension: HashMap<String, String>,
}

/// Result of submitting a transaction to the network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitTxResult {
    pub tx_id: String,
    pub submitted: bool,
    pub error: Option<String>,
}

// ================================================================
// Types - Configuration & Statistics
// ================================================================

/// Connector configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorConfig {
    pub supported_wallets: Vec<WalletType>,
    pub auto_connect: bool,
    pub timeout_ms: u64,
    pub session_ttl_secs: u64,
}

impl Default for ConnectorConfig {
    fn default() -> Self {
        Self {
            supported_wallets: vec![WalletType::Nautilus, WalletType::SAFEW],
            auto_connect: false,
            timeout_ms: 30_000,
            session_ttl_secs: 3600,
        }
    }
}

/// Connector statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorStats {
    pub total_connections: u64,
    pub total_signings: u64,
    pub total_submissions: u64,
    pub active_sessions: u64,
    pub rejected_connections: u64,
}

// ================================================================
// Engine - Wallet Connector Engine
// ================================================================

/// Core wallet connector engine implementing EIP-12
pub struct WalletConnectorEngine {
    /// Active wallet sessions keyed by session ID
    sessions: DashMap<String, WalletSession>,
    /// Connector configuration
    config: tokio::sync::RwLock<ConnectorConfig>,
    /// Statistics counters
    total_connections: AtomicU64,
    total_signings: AtomicU64,
    total_submissions: AtomicU64,
    rejected_connections: AtomicU64,
}

impl WalletConnectorEngine {
    /// Create a new engine with default configuration
    pub fn new() -> Arc<Self> {
        Self::with_config(ConnectorConfig::default())
    }

    /// Create a new engine with custom configuration
    pub fn with_config(config: ConnectorConfig) -> Arc<Self> {
        Arc::new(Self {
            sessions: DashMap::new(),
            config: tokio::sync::RwLock::new(config),
            total_connections: AtomicU64::new(0),
            total_signings: AtomicU64::new(0),
            total_submissions: AtomicU64::new(0),
            rejected_connections: AtomicU64::new(0),
        })
    }

    // ============================================================
    // Wallet Discovery
    // ============================================================

    /// Discover available wallets (simulated)
    pub fn discover_wallets(&self) -> Vec<WalletInfo> {
        info!("Discovering available wallets");
        let _now = Utc::now().timestamp_millis();
        let nautilus_addr = "3WvsTBCG5sTi1QWb1TCdPkwFbKvQvS8QJTCe5MvaiRjqKqD1oYj";
        let safew_addr = "9fR7gVhEjL3qkC8q7XPrqZsBdMfKxNpYvQwWfGc6sVhEjL3qkC";

        vec![
            WalletInfo {
                name: WalletType::Nautilus,
                version: Some("0.7.1".to_string()),
                connected: false,
                session_id: String::new(),
                addresses: vec![nautilus_addr.to_string()],
                connected_at: 0,
            },
            WalletInfo {
                name: WalletType::SAFEW,
                version: Some("1.3.0".to_string()),
                connected: false,
                session_id: String::new(),
                addresses: vec![safew_addr.to_string()],
                connected_at: 0,
            },
        ]
    }

    // ============================================================
    // Connection Management
    // ============================================================

    /// Connect to a wallet, creating a new session
    pub async fn connect(
        &self,
        wallet_type: WalletType,
        address: String,
    ) -> Result<WalletSession, String> {
        info!(wallet = %wallet_type, "Connecting to wallet");

        // Check if wallet type is supported
        {
            let cfg = self.config.read().await;
            if !cfg.supported_wallets.contains(&wallet_type)
                && wallet_type != WalletType::Unknown("".to_string())
            {
                warn!(wallet = %wallet_type, "Unsupported wallet type");
                self.rejected_connections.fetch_add(1, Ordering::Relaxed);
                return Err(format!("Unsupported wallet type: {}", wallet_type));
            }
        }

        let session_id = Uuid::new_v4().to_string();
        let now = Utc::now().timestamp_millis();

        // Generate simulated wallet data based on wallet type
        let (balance, utxos, used_addresses) = match wallet_type {
            WalletType::Nautilus => self.simulate_nautilus_data(&address),
            WalletType::SAFEW => self.simulate_safew_data(&address),
            _ => self.simulate_generic_data(&address),
        };

        let session = WalletSession {
            id: session_id.clone(),
            wallet_type: wallet_type.clone(),
            address: address.clone(),
            access_granted: true,
            created_at: now,
            last_used_at: now,
            context: WalletContext {
                balance,
                utxos,
                used_addresses,
            },
        };

        self.sessions.insert(session_id.clone(), session.clone());
        self.total_connections.fetch_add(1, Ordering::Relaxed);
        info!(
            session = %session_id,
            wallet = %wallet_type,
            "Wallet connected successfully"
        );

        Ok(session)
    }

    /// Disconnect from a wallet by session ID
    pub fn disconnect(&self, session_id: &str) -> bool {
        info!(session = %session_id, "Disconnecting wallet");
        let removed = self.sessions.remove(session_id).is_some();
        if removed {
            debug!(session = %session_id, "Session removed");
        } else {
            warn!(session = %session_id, "Session not found for disconnect");
        }
        removed
    }

    /// Check if a session is connected
    pub fn is_connected(&self, session_id: &str) -> bool {
        self.sessions.contains_key(session_id)
    }

    // ============================================================
    // Context API - Balance, UTXOs, Addresses
    // ============================================================

    /// Get ERG balance for a session (simulated)
    pub fn get_balance(&self, session_id: &str) -> Result<u64, String> {
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| format!("Session not found: {}", session_id))?;
        Ok(session.context.balance)
    }

    /// Get unspent boxes for a session (simulated)
    pub fn get_utxos(
        &self,
        session_id: &str,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> Result<Vec<UtxoEntry>, String> {
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| format!("Session not found: {}", session_id))?;

        let utxos = &session.context.utxos;
        let start = offset.unwrap_or(0);
        let end = (start + limit.unwrap_or(usize::MAX)).min(utxos.len());
        Ok(utxos[start..end].to_vec())
    }

    /// Get used addresses for a session (simulated)
    pub fn get_used_addresses(&self, session_id: &str) -> Result<Vec<String>, String> {
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| format!("Session not found: {}", session_id))?;
        Ok(session.context.used_addresses.clone())
    }

    // ============================================================
    // Transaction Operations
    // ============================================================

    /// Sign a transaction (simulated: generates mock proof bytes)
    pub fn sign_tx(
        &self,
        session_id: &str,
        request: &SignTxRequest,
    ) -> Result<SignTxResult, String> {
        info!(session = %session_id, "Signing transaction");
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| format!("Session not found: {}", session_id))?;

        if !session.access_granted {
            return Err("Access not granted for this session".to_string());
        }

        // Generate mock signed inputs with random proof bytes
        let mut signed_inputs = Vec::new();
        for input_ref in &request.unsigned_tx.inputs {
            let mut proof_bytes = vec![0u8; 64];
            rand::thread_rng().fill_bytes(&mut proof_bytes);
            let proof_hex = hex::encode(&proof_bytes);

            signed_inputs.push(SignedInputRef {
                box_id: input_ref.box_id.clone(),
                proof_bytes: proof_hex,
                extension: input_ref.extension.clone(),
            });
        }

        // Generate mock tx_id
        let mut tx_id_bytes = vec![0u8; 32];
        rand::thread_rng().fill_bytes(&mut tx_id_bytes);
        let tx_id = hex::encode(&tx_id_bytes);

        self.total_signings.fetch_add(1, Ordering::Relaxed);
        info!(session = %session_id, tx_id = %tx_id, "Transaction signed");

        Ok(SignTxResult {
            tx_id,
            signed_inputs,
        })
    }

    /// Submit a signed transaction (simulated)
    pub fn submit_tx(&self, session_id: &str, tx_id: &str) -> Result<SubmitTxResult, String> {
        info!(session = %session_id, tx_id = %tx_id, "Submitting transaction");
        let _session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| format!("Session not found: {}", session_id))?;

        // Simulate successful submission (90% success rate in simulation)
        let success = rand::thread_rng().gen_bool(0.9);
        self.total_submissions.fetch_add(1, Ordering::Relaxed);

        if success {
            info!(session = %session_id, tx_id = %tx_id, "Transaction submitted successfully");
            Ok(SubmitTxResult {
                tx_id: tx_id.to_string(),
                submitted: true,
                error: None,
            })
        } else {
            warn!(session = %session_id, tx_id = %tx_id, "Transaction submission failed (simulated)");
            Ok(SubmitTxResult {
                tx_id: tx_id.to_string(),
                submitted: false,
                error: Some("Network timeout (simulated)".to_string()),
            })
        }
    }

    // ============================================================
    // Session Management
    // ============================================================

    /// Get session details
    pub fn get_session(&self, session_id: &str) -> Result<WalletSession, String> {
        let session = self
            .sessions
            .get(session_id)
            .ok_or_else(|| format!("Session not found: {}", session_id))?;
        Ok(session.clone())
    }

    /// List active sessions, optionally filtered by wallet type
    pub fn list_sessions(&self, wallet_type: Option<&WalletType>) -> Vec<WalletSession> {
        let mut sessions: Vec<WalletSession> = self
            .sessions
            .iter()
            .map(|entry| entry.value().clone())
            .collect();

        if let Some(wt) = wallet_type {
            sessions.retain(|s| &s.wallet_type == wt);
        }

        sessions
    }

    // ============================================================
    // Configuration & Statistics
    // ============================================================

    /// Get the current connector configuration
    pub async fn get_config(&self) -> ConnectorConfig {
        self.config.read().await.clone()
    }

    /// Update the connector configuration
    pub async fn update_config(&self, config: ConnectorConfig) {
        info!("Updating connector configuration");
        *self.config.write().await = config;
    }

    /// Get connector statistics
    pub fn get_stats(&self) -> ConnectorStats {
        ConnectorStats {
            total_connections: self.total_connections.load(Ordering::Relaxed),
            total_signings: self.total_signings.load(Ordering::Relaxed),
            total_submissions: self.total_submissions.load(Ordering::Relaxed),
            active_sessions: self.sessions.len() as u64,
            rejected_connections: self.rejected_connections.load(Ordering::Relaxed),
        }
    }

    /// Remove expired sessions based on TTL
    pub async fn cleanup_expired_sessions(&self) -> usize {
        let ttl_secs = self.config.read().await.session_ttl_secs;
        let now = Utc::now().timestamp_millis();
        let ttl_ms = (ttl_secs as i64) * 1000;

        let expired: Vec<String> = self
            .sessions
            .iter()
            .filter(|entry| (now - entry.value().created_at) > ttl_ms)
            .map(|entry| entry.key().clone())
            .collect();

        let count = expired.len();
        for session_id in &expired {
            self.sessions.remove(session_id);
            debug!(session = %session_id, "Expired session cleaned up");
        }

        if count > 0 {
            info!(count = count, "Cleaned up expired sessions");
        }
        count
    }

    // ============================================================
    // Simulation Helpers
    // ============================================================

    /// Simulate Nautilus wallet data
    fn simulate_nautilus_data(
        &self,
        address: &str,
    ) -> (u64, Vec<UtxoEntry>, Vec<String>) {
        let balance = 10_000_000_000u64; // 10 ERG
        let utxos = vec![
            UtxoEntry {
                box_id: "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2".to_string(),
                value: 5_000_000_000,
                ergo_tree: "100204a02b240".to_string(),
                assets: vec![
                    TokenRef {
                        token_id: "d53fdd23c6ae6b999c2d13c7c7d0a3a0a3a0a3a0a3a0a3a0a3a0a3a0a3a0a3a0a3a".to_string(),
                        amount: 1_000_000,
                    },
                ],
                registers: HashMap::new(),
                creation_height: 500_000,
            },
            UtxoEntry {
                box_id: "b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3".to_string(),
                value: 2_000_000_000,
                ergo_tree: "100204a02b240".to_string(),
                assets: vec![],
                registers: HashMap::new(),
                creation_height: 502_000,
            },
            UtxoEntry {
                box_id: "c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4".to_string(),
                value: 1_000_000_000,
                ergo_tree: "100204a02b240".to_string(),
                assets: vec![],
                registers: HashMap::new(),
                creation_height: 505_000,
            },
            UtxoEntry {
                box_id: "d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5".to_string(),
                value: 1_000_000_000,
                ergo_tree: "100204a02b240".to_string(),
                assets: vec![
                    TokenRef {
                        token_id: "e6f7a8b9c0d1e6f7a8b9c0d1e6f7a8b9c0d1e6f7a8b9c0d1e6f7a8b9c0d1e6f7".to_string(),
                        amount: 500,
                    },
                ],
                registers: HashMap::new(),
                creation_height: 510_000,
            },
            UtxoEntry {
                box_id: "e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6".to_string(),
                value: 1_000_000_000,
                ergo_tree: "100204a02b240".to_string(),
                assets: vec![],
                registers: HashMap::new(),
                creation_height: 512_000,
            },
        ];
        let used_addresses = vec![
            address.to_string(),
            "3WvsTBCG5sTi1QWb1TCdPkwFbKvQvS8QJTCe5MvaiRjqKqD1oYj".to_string(),
        ];

        (balance, utxos, used_addresses)
    }

    /// Simulate SAFEW wallet data
    fn simulate_safew_data(&self, address: &str) -> (u64, Vec<UtxoEntry>, Vec<String>) {
        let balance = 5_000_000_000u64; // 5 ERG
        let utxos = vec![
            UtxoEntry {
                box_id: "f1a2b3c4d5e6f1a2b3c4d5e6f1a2b3c4d5e6f1a2b3c4d5e6f1a2b3c4d5e6f1a2".to_string(),
                value: 2_500_000_000,
                ergo_tree: "100204a02b240".to_string(),
                assets: vec![],
                registers: HashMap::new(),
                creation_height: 490_000,
            },
            UtxoEntry {
                box_id: "a2b3c4d5e6f1a2b3c4d5e6f1a2b3c4d5e6f1a2b3c4d5e6f1a2b3c4d5e6f1a2b3".to_string(),
                value: 1_500_000_000,
                ergo_tree: "100204a02b240".to_string(),
                assets: vec![
                    TokenRef {
                        token_id: "00c4d5e6f1a2b3c4d5e6f1a2b3c4d5e6f1a2b3c4d5e6f1a2b3c4d5e6f1a2b3c4".to_string(),
                        amount: 100_000,
                    },
                ],
                registers: HashMap::new(),
                creation_height: 495_000,
            },
            UtxoEntry {
                box_id: "b3c4d5e6f1a2b3c4d5e6f1a2b3c4d5e6f1a2b3c4d5e6f1a2b3c4d5e6f1a2b3c4".to_string(),
                value: 1_000_000_000,
                ergo_tree: "100204a02b240".to_string(),
                assets: vec![],
                registers: HashMap::new(),
                creation_height: 498_000,
            },
        ];
        let used_addresses = vec![
            address.to_string(),
            "9fR7gVhEjL3qkC8q7XPrqZsBdMfKxNpYvQwWfGc6sVhEjL3qkC".to_string(),
        ];

        (balance, utxos, used_addresses)
    }

    /// Simulate generic wallet data
    fn simulate_generic_data(&self, address: &str) -> (u64, Vec<UtxoEntry>, Vec<String>) {
        let balance = 1_000_000_000u64; // 1 ERG
        let utxos = vec![UtxoEntry {
            box_id: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            value: 1_000_000_000,
            ergo_tree: "100204a02b240".to_string(),
            assets: vec![],
            registers: HashMap::new(),
            creation_height: 400_000,
        }];
        let used_addresses = vec![address.to_string()];

        (balance, utxos, used_addresses)
    }
}

// ================================================================
// REST API - Request/Response Types
// ================================================================

#[derive(Debug, Deserialize)]
struct ConnectRequest {
    wallet_type: WalletType,
    address: String,
}

#[derive(Debug, Deserialize)]
struct ListSessionsQuery {
    wallet_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UtxosQuery {
    offset: Option<usize>,
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
struct ApiResponse<T: Serialize> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    fn ok(data: T) -> Json<Self> {
        Json(Self {
            success: true,
            data: Some(data),
            error: None,
        })
    }

    fn err(msg: impl Into<String>) -> Json<Self> {
        Json(Self {
            success: false,
            data: None,
            error: Some(msg.into()),
        })
    }
}

#[derive(Debug, Deserialize)]
struct SubmitTxRequest {
    tx_id: String,
}

// ================================================================
// REST API - Handler Functions
// ================================================================

/// GET /discover - Discover available wallets
async fn handle_discover(
    State(engine): State<Arc<WalletConnectorEngine>>,
) -> Json<ApiResponse<Vec<WalletInfo>>> {
    let wallets = engine.discover_wallets();
    ApiResponse::ok(wallets)
}

/// POST /connect - Connect to a wallet
async fn handle_connect(
    State(engine): State<Arc<WalletConnectorEngine>>,
    Json(body): Json<ConnectRequest>,
) -> Json<ApiResponse<WalletSession>> {
    match engine.connect(body.wallet_type, body.address).await {
        Ok(session) => ApiResponse::ok(session),
        Err(e) => ApiResponse::err(e),
    }
}

/// POST /disconnect/:session_id - Disconnect from a wallet
async fn handle_disconnect(
    State(engine): State<Arc<WalletConnectorEngine>>,
    Path(session_id): Path<String>,
) -> Json<ApiResponse<String>> {
    let removed = engine.disconnect(&session_id);
    if removed {
        ApiResponse::ok(format!("Disconnected session {}", session_id))
    } else {
        ApiResponse::err(format!("Session not found: {}", session_id))
    }
}

/// GET /session/:session_id - Get session info
async fn handle_get_session(
    State(engine): State<Arc<WalletConnectorEngine>>,
    Path(session_id): Path<String>,
) -> Json<ApiResponse<WalletSession>> {
    match engine.get_session(&session_id) {
        Ok(session) => ApiResponse::ok(session),
        Err(e) => ApiResponse::err(e),
    }
}

/// GET /sessions - List active sessions
async fn handle_list_sessions(
    State(engine): State<Arc<WalletConnectorEngine>>,
    Query(query): Query<ListSessionsQuery>,
) -> Json<ApiResponse<Vec<WalletSession>>> {
    let wallet_type = query.wallet_type.as_ref().map(|s| WalletType::Unknown(s.clone()));
    let sessions = engine.list_sessions(wallet_type.as_ref());
    ApiResponse::ok(sessions)
}

/// GET /:session_id/balance - Get balance
async fn handle_get_balance(
    State(engine): State<Arc<WalletConnectorEngine>>,
    Path(session_id): Path<String>,
) -> Json<ApiResponse<u64>> {
    match engine.get_balance(&session_id) {
        Ok(balance) => ApiResponse::ok(balance),
        Err(e) => ApiResponse::err(e),
    }
}

/// GET /:session_id/utxos - Get UTXOs
async fn handle_get_utxos(
    State(engine): State<Arc<WalletConnectorEngine>>,
    Path(session_id): Path<String>,
    Query(query): Query<UtxosQuery>,
) -> Json<ApiResponse<Vec<UtxoEntry>>> {
    match engine.get_utxos(&session_id, query.offset, query.limit) {
        Ok(utxos) => ApiResponse::ok(utxos),
        Err(e) => ApiResponse::err(e),
    }
}

/// POST /:session_id/sign-tx - Sign a transaction
async fn handle_sign_tx(
    State(engine): State<Arc<WalletConnectorEngine>>,
    Path(session_id): Path<String>,
    Json(body): Json<SignTxRequest>,
) -> Json<ApiResponse<SignTxResult>> {
    match engine.sign_tx(&session_id, &body) {
        Ok(result) => ApiResponse::ok(result),
        Err(e) => ApiResponse::err(e),
    }
}

/// POST /:session_id/submit-tx - Submit a transaction
async fn handle_submit_tx(
    State(engine): State<Arc<WalletConnectorEngine>>,
    Path(session_id): Path<String>,
    Json(body): Json<SubmitTxRequest>,
) -> Json<ApiResponse<SubmitTxResult>> {
    match engine.submit_tx(&session_id, &body.tx_id) {
        Ok(result) => ApiResponse::ok(result),
        Err(e) => ApiResponse::err(e),
    }
}

/// GET /stats - Get connector statistics
async fn handle_get_stats(
    State(engine): State<Arc<WalletConnectorEngine>>,
) -> Json<ApiResponse<ConnectorStats>> {
    ApiResponse::ok(engine.get_stats())
}

// ================================================================
// Router
// ================================================================

/// Build the EIP-12 wallet connector router
pub fn wallet_connector_router(engine: Arc<WalletConnectorEngine>) -> Router {
    Router::new()
        .route("/discover", get(handle_discover))
        .route("/connect", post(handle_connect))
        .route("/disconnect/{session_id}", post(handle_disconnect))
        .route("/session/{session_id}", get(handle_get_session))
        .route("/sessions", get(handle_list_sessions))
        .route("/{session_id}/balance", get(handle_get_balance))
        .route("/{session_id}/utxos", get(handle_get_utxos))
        .route("/{session_id}/sign-tx", post(handle_sign_tx))
        .route("/{session_id}/submit-tx", post(handle_submit_tx))
        .route("/stats", get(handle_get_stats))
        .with_state(engine)
}

// ================================================================
// Unit Tests
// ================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_engine() -> Arc<WalletConnectorEngine> {
        WalletConnectorEngine::new()
    }

    // ---------------------------------------------------------------
    // test_discover_wallets
    // ---------------------------------------------------------------
    #[tokio::test]
    async fn test_discover_wallets() {
        let engine = make_engine();
        let wallets = engine.discover_wallets();
        assert_eq!(wallets.len(), 2);
        assert_eq!(wallets[0].name, WalletType::Nautilus);
        assert_eq!(wallets[1].name, WalletType::SAFEW);
        assert!(!wallets[0].connected);
    }

    // ---------------------------------------------------------------
    // test_connect_nautilus
    // ---------------------------------------------------------------
    #[tokio::test]
    async fn test_connect_nautilus() {
        let engine = make_engine();
        let session = engine
            .connect(WalletType::Nautilus, "3WtestNautilusAddress".to_string())
            .await
            .unwrap();
        assert_eq!(session.wallet_type, WalletType::Nautilus);
        assert!(session.access_granted);
        assert_eq!(session.context.balance, 10_000_000_000);
        assert_eq!(session.context.utxos.len(), 5);
        assert!(!session.id.is_empty());
    }

    // ---------------------------------------------------------------
    // test_connect_safew
    // ---------------------------------------------------------------
    #[tokio::test]
    async fn test_connect_safew() {
        let engine = make_engine();
        let session = engine
            .connect(WalletType::SAFEW, "9fR7testSAFEWAddress".to_string())
            .await
            .unwrap();
        assert_eq!(session.wallet_type, WalletType::SAFEW);
        assert!(session.access_granted);
        assert_eq!(session.context.balance, 5_000_000_000);
        assert_eq!(session.context.utxos.len(), 3);
    }

    // ---------------------------------------------------------------
    // test_disconnect
    // ---------------------------------------------------------------
    #[tokio::test]
    async fn test_disconnect() {
        let engine = make_engine();
        let session = engine
            .connect(WalletType::Nautilus, "3WtestDisconnect".to_string())
            .await
            .unwrap();
        assert!(engine.is_connected(&session.id));
        let removed = engine.disconnect(&session.id);
        assert!(removed);
        assert!(!engine.is_connected(&session.id));
    }

    // ---------------------------------------------------------------
    // test_is_connected
    // ---------------------------------------------------------------
    #[tokio::test]
    async fn test_is_connected() {
        let engine = make_engine();
        assert!(!engine.is_connected("nonexistent-session"));
        let session = engine
            .connect(WalletType::Nautilus, "3WtestConnected".to_string())
            .await
            .unwrap();
        assert!(engine.is_connected(&session.id));
    }

    // ---------------------------------------------------------------
    // test_get_balance
    // ---------------------------------------------------------------
    #[tokio::test]
    async fn test_get_balance() {
        let engine = make_engine();
        let session = engine
            .connect(WalletType::Nautilus, "3WtestBalance".to_string())
            .await
            .unwrap();
        let balance = engine.get_balance(&session.id).unwrap();
        assert_eq!(balance, 10_000_000_000);

        // Non-existent session
        assert!(engine.get_balance("bad-session").is_err());
    }

    // ---------------------------------------------------------------
    // test_get_utxos
    // ---------------------------------------------------------------
    #[tokio::test]
    async fn test_get_utxos() {
        let engine = make_engine();
        let session = engine
            .connect(WalletType::Nautilus, "3WtestUtxos".to_string())
            .await
            .unwrap();
        let utxos = engine.get_utxos(&session.id, None, None).unwrap();
        assert_eq!(utxos.len(), 5);

        // Paginated: offset=1, limit=2
        let page = engine.get_utxos(&session.id, Some(1), Some(2)).unwrap();
        assert_eq!(page.len(), 2);
        assert_eq!(page[0].box_id, utxos[1].box_id);

        // Non-existent session
        assert!(engine.get_utxos("bad-session", None, None).is_err());
    }

    // ---------------------------------------------------------------
    // test_get_used_addresses
    // ---------------------------------------------------------------
    #[tokio::test]
    async fn test_get_used_addresses() {
        let engine = make_engine();
        let addr = "3WtestUsedAddr";
        let session = engine
            .connect(WalletType::Nautilus, addr.to_string())
            .await
            .unwrap();
        let addresses = engine.get_used_addresses(&session.id).unwrap();
        assert!(addresses.contains(&addr.to_string()));
        assert!(addresses.len() >= 1);

        // Non-existent session
        assert!(engine.get_used_addresses("bad-session").is_err());
    }

    // ---------------------------------------------------------------
    // test_sign_transaction
    // ---------------------------------------------------------------
    #[tokio::test]
    async fn test_sign_transaction() {
        let engine = make_engine();
        let session = engine
            .connect(WalletType::Nautilus, "3WtestSign".to_string())
            .await
            .unwrap();

        let request = SignTxRequest {
            unsigned_tx: UnsignedTxInput {
                inputs: vec![TxInputRef {
                    box_id: "abc123".to_string(),
                    extension: HashMap::new(),
                }],
                data_inputs: vec![],
                outputs: vec![TxOutputRef {
                    ergo_tree: "100204a02b240".to_string(),
                    value: 1_000_000_000,
                    assets: vec![],
                    additional_registers: HashMap::new(),
                    creation_height: 500_000,
                }],
            },
            fee: 1_000_000,
            inputs: vec![],
            data_inputs: vec![],
        };

        let result = engine.sign_tx(&session.id, &request).unwrap();
        assert!(!result.tx_id.is_empty());
        assert_eq!(result.signed_inputs.len(), 1);
        assert_eq!(result.signed_inputs[0].box_id, "abc123");
        assert!(!result.signed_inputs[0].proof_bytes.is_empty());

        // Non-existent session
        assert!(engine.sign_tx("bad-session", &request).is_err());
    }

    // ---------------------------------------------------------------
    // test_submit_transaction
    // ---------------------------------------------------------------
    #[tokio::test]
    async fn test_submit_transaction() {
        let engine = make_engine();
        let session = engine
            .connect(WalletType::Nautilus, "3WtestSubmit".to_string())
            .await
            .unwrap();

        let result = engine.submit_tx(&session.id, "test-tx-id-123").unwrap();
        assert_eq!(result.tx_id, "test-tx-id-123");

        // Non-existent session
        assert!(engine.submit_tx("bad-session", "tx").is_err());
    }

    // ---------------------------------------------------------------
    // test_session_lifecycle
    // ---------------------------------------------------------------
    #[tokio::test]
    async fn test_session_lifecycle() {
        let engine = make_engine();

        // 1. Connect
        let session = engine
            .connect(WalletType::Nautilus, "3Wlifecycle".to_string())
            .await
            .unwrap();
        assert!(engine.is_connected(&session.id));

        // 2. Get session
        let fetched = engine.get_session(&session.id).unwrap();
        assert_eq!(fetched.id, session.id);

        // 3. Balance & UTXOs
        let _balance = engine.get_balance(&session.id).unwrap();
        let _utxos = engine.get_utxos(&session.id, None, None).unwrap();

        // 4. Sign
        let sign_req = SignTxRequest {
            unsigned_tx: UnsignedTxInput {
                inputs: vec![TxInputRef {
                    box_id: "box1".to_string(),
                    extension: HashMap::new(),
                }],
                data_inputs: vec![],
                outputs: vec![TxOutputRef {
                    ergo_tree: "tree".to_string(),
                    value: 100,
                    assets: vec![],
                    additional_registers: HashMap::new(),
                    creation_height: 100,
                }],
            },
            fee: 1_000_000,
            inputs: vec![],
            data_inputs: vec![],
        };
        let sign_result = engine.sign_tx(&session.id, &sign_req).unwrap();
        assert!(!sign_result.tx_id.is_empty());

        // 5. Submit
        let submit_result = engine
            .submit_tx(&session.id, &sign_result.tx_id)
            .unwrap();
        assert!(submit_result.submitted || submit_result.error.is_some());

        // 6. Disconnect
        engine.disconnect(&session.id);
        assert!(!engine.is_connected(&session.id));
    }

    // ---------------------------------------------------------------
    // test_list_sessions
    // ---------------------------------------------------------------
    #[tokio::test]
    async fn test_list_sessions() {
        let engine = make_engine();

        let s1 = engine
            .connect(WalletType::Nautilus, "3Wlist1".to_string())
            .await
            .unwrap();
        let s2 = engine
            .connect(WalletType::SAFEW, "9fRlist2".to_string())
            .await
            .unwrap();

        let all = engine.list_sessions(None);
        assert_eq!(all.len(), 2);

        let nautilus_only = engine.list_sessions(Some(&WalletType::Nautilus));
        assert_eq!(nautilus_only.len(), 1);
        assert_eq!(nautilus_only[0].id, s1.id);

        let safew_only = engine.list_sessions(Some(&WalletType::SAFEW));
        assert_eq!(safew_only.len(), 1);
        assert_eq!(safew_only[0].id, s2.id);
    }

    // ---------------------------------------------------------------
    // test_cleanup_expired
    // ---------------------------------------------------------------
    #[tokio::test]
    async fn test_cleanup_expired() {
        let mut cfg = ConnectorConfig::default();
        cfg.session_ttl_secs = 0; // Instant expiry
        let engine = WalletConnectorEngine::with_config(cfg);

        let session = engine
            .connect(WalletType::Nautilus, "3Wexpired".to_string())
            .await
            .unwrap();
        assert!(engine.is_connected(&session.id));

        // Give a tiny bit of time to ensure timestamp advances
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;

        let cleaned = engine.cleanup_expired_sessions().await;
        assert!(cleaned >= 1);
        assert!(!engine.is_connected(&session.id));
    }

    // ---------------------------------------------------------------
    // test_concurrent_connections
    // ---------------------------------------------------------------
    #[tokio::test]
    async fn test_concurrent_connections() {
        let engine = make_engine();
        let mut handles = Vec::new();

        for i in 0..10 {
            let eng = engine.clone();
            let handle = tokio::spawn(async move {
                let addr = format!("3Wconcurrent{}", i);
                eng.connect(WalletType::Nautilus, addr).await.unwrap()
            });
            handles.push(handle);
        }

        let mut sessions = Vec::new();
        for handle in handles {
            sessions.push(handle.await.unwrap());
        }

        assert_eq!(sessions.len(), 10);
        // All session IDs must be unique
        let ids: std::collections::HashSet<_> = sessions.iter().map(|s| s.id.clone()).collect();
        assert_eq!(ids.len(), 10);

        // Stats should reflect 10 connections
        let stats = engine.get_stats();
        assert_eq!(stats.total_connections, 10);
        assert_eq!(stats.active_sessions, 10);
    }

    // ---------------------------------------------------------------
    // test_stats_tracking
    // ---------------------------------------------------------------
    #[tokio::test]
    async fn test_stats_tracking() {
        let engine = make_engine();

        // Initial stats
        let stats = engine.get_stats();
        assert_eq!(stats.total_connections, 0);
        assert_eq!(stats.total_signings, 0);
        assert_eq!(stats.total_submissions, 0);
        assert_eq!(stats.active_sessions, 0);
        assert_eq!(stats.rejected_connections, 0);

        // Connect
        let session = engine
            .connect(WalletType::Nautilus, "3Wstats".to_string())
            .await
            .unwrap();
        let stats = engine.get_stats();
        assert_eq!(stats.total_connections, 1);
        assert_eq!(stats.active_sessions, 1);

        // Sign
        let sign_req = SignTxRequest {
            unsigned_tx: UnsignedTxInput {
                inputs: vec![TxInputRef {
                    box_id: "statbox".to_string(),
                    extension: HashMap::new(),
                }],
                data_inputs: vec![],
                outputs: vec![],
            },
            fee: 1_000_000,
            inputs: vec![],
            data_inputs: vec![],
        };
        engine.sign_tx(&session.id, &sign_req).unwrap();
        let stats = engine.get_stats();
        assert_eq!(stats.total_signings, 1);

        // Submit
        engine.submit_tx(&session.id, "stat-tx").unwrap();
        let stats = engine.get_stats();
        assert_eq!(stats.total_submissions, 1);

        // Disconnect
        engine.disconnect(&session.id);
        let stats = engine.get_stats();
        assert_eq!(stats.active_sessions, 0);

        // Rejected connection (unsupported wallet)
        let _ = engine
            .connect(WalletType::Minotaur, "rejectme".to_string())
            .await;
        let stats = engine.get_stats();
        assert_eq!(stats.rejected_connections, 1);
    }
}
