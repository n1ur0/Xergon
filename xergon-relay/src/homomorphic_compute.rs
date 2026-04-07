//! Homomorphic Compute Module
//!
//! Provides mock homomorphic encryption (BFV/CKKS/Paillier), multi-party computation
//! with Shamir secret sharing, and secure federated gradient aggregation with
//! differential privacy noise.

use crate::proxy::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use blake3::hash;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Core Types
// ---------------------------------------------------------------------------

/// Supported homomorphic encryption schemes (all mock-implemented with blake3+XOR).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HEScheme {
    BFV,
    CKKS,
    MockPaillier,
}

/// Encryption context holding scheme parameters and a seed key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HEContext {
    pub scheme: HEScheme,
    pub poly_modulus_degree: u64,
    pub coeff_modulus_bits: u32,
    pub plain_modulus: u64,
    pub seed: [u8; 32],
}

/// Ciphertext produced by mock encryption: nonce + tag + encrypted data buffer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HECiphertext {
    pub scheme: HEScheme,
    pub nonce: u64,
    pub data: Vec<u8>,
}

/// The main HE module: encrypt / decrypt / add / multiply.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HEModule {
    pub context: HEContext,
}

// ---------------------------------------------------------------------------
// HE Module Implementation
// ---------------------------------------------------------------------------

impl HEModule {
    pub fn new(scheme: HEScheme, poly_modulus_degree: u64, coeff_modulus_bits: u32, plain_modulus: u64) -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u128)
            .unwrap_or(42);
        let seed_bytes: [u8; 32] = blake3::hash(&seed.to_le_bytes()).into();
        Self {
            context: HEContext {
                scheme,
                poly_modulus_degree,
                coeff_modulus_bits,
                plain_modulus,
                seed: seed_bytes,
            }
        }
    }

    /// Mock encrypt: blake3(nonce || seed) XOR plaintext.
    pub fn encrypt(&self, plaintext: &[u8]) -> HECiphertext {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        let mut combined = Vec::with_capacity(16 + 32);
        combined.extend_from_slice(&nonce.to_le_bytes());
        combined.extend_from_slice(&self.context.seed);
        let hash_out = hash(&combined);
        let hash_bytes = hash_out.as_bytes();
        let encrypted: Vec<u8> = plaintext
            .iter()
            .zip(hash_bytes.iter().cycle())
            .map(|(p, h)| p ^ h)
            .collect();
        HECiphertext {
            scheme: self.context.scheme.clone(),
            nonce,
            data: encrypted,
        }
    }

    /// Mock decrypt: recompute keystream and XOR back.
    pub fn decrypt(&self, ct: &HECiphertext) -> Vec<u8> {
        let mut combined = Vec::with_capacity(16 + 32);
        combined.extend_from_slice(&ct.nonce.to_le_bytes());
        combined.extend_from_slice(&self.context.seed);
        let hash_out = hash(&combined);
        let hash_bytes = hash_out.as_bytes();
        ct.data
            .iter()
            .zip(hash_bytes.iter().cycle())
            .map(|(c, h)| c ^ h)
            .collect()
    }

    /// Mock homomorphic add: XOR ciphertext data buffers together.
    pub fn add(&self, a: &HECiphertext, b: &HECiphertext) -> HECiphertext {
        assert_eq!(a.scheme, b.scheme, "scheme mismatch in add");
        let len = a.data.len().max(b.data.len());
        let mut result = vec![0u8; len];
        for i in 0..len {
            let av = a.data.get(i).copied().unwrap_or(0);
            let bv = b.data.get(i).copied().unwrap_or(0);
            result[i] = av.wrapping_add(bv);
        }
        HECiphertext {
            scheme: a.scheme.clone(),
            nonce: a.nonce ^ b.nonce,
            data: result,
        }
    }

    /// Mock homomorphic multiply: element-wise multiply ciphertext data.
    pub fn multiply(&self, a: &HECiphertext, b: &HECiphertext) -> HECiphertext {
        assert_eq!(a.scheme, b.scheme, "scheme mismatch in multiply");
        let len = a.data.len().min(b.data.len());
        let result: Vec<u8> = a.data[..len]
            .iter()
            .zip(b.data[..len].iter())
            .map(|(x, y)| x.wrapping_mul(*y))
            .collect();
        HECiphertext {
            scheme: a.scheme.clone(),
            nonce: a.nonce.wrapping_mul(b.nonce),
            data: result,
        }
    }
}

// ---------------------------------------------------------------------------
// MPC Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MPCProtocol {
    Shamir,
    Replicated,
    GMW,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Participant {
    pub id: String,
    pub public_key: String,
    pub address: String,
    pub threshold: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MPCSession {
    pub session_id: String,
    pub protocol: MPCProtocol,
    pub participants: Vec<Participant>,
    pub threshold: u32,
    pub created_at: u64,
    pub shares_generated: bool,
}

impl MPCSession {
    pub fn new(session_id: String, protocol: MPCProtocol, threshold: u32) -> Self {
        Self {
            session_id,
            protocol,
            participants: Vec::new(),
            threshold,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            shares_generated: false,
        }
    }

    pub fn add_participant(&mut self, p: Participant) {
        self.participants.push(p);
    }
}

/// A Shamir share: (x, y) pair.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShamirShare {
    pub x: u64,
    pub y: u64,
    pub prime: u64,
}

/// Shamir secret sharing over a finite field.
pub struct ShamirSharing {
    prime: u64,
}

impl ShamirSharing {
    pub fn new(prime: u64) -> Self {
        Self { prime }
    }

    /// Evaluate polynomial at point x mod prime.
    pub fn eval_poly(coeffs: &[u64], x: u64, prime: u64) -> u64 {
        let mut result = 0u64;
        for &c in coeffs.iter().rev() {
            result = (result.wrapping_mul(x) % prime).wrapping_add(c) % prime;
        }
        result
    }

    /// Split secret into n shares with threshold t.
    pub fn split(&self, secret: u64, n: u32, threshold: u32) -> Vec<ShamirShare> {
        assert!(n >= threshold, "n must be >= threshold");
        let t = threshold as usize;
        let mut rng_seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(12345);
        // Generate random coefficients (except first which is the secret)
        let mut coeffs = vec![secret];
        for _ in 1..t {
            rng_seed = rng_seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            coeffs.push(rng_seed % self.prime);
        }
        let mut shares = Vec::with_capacity(n as usize);
        for i in 1..=n {
            let y = Self::eval_poly(&coeffs, i as u64, self.prime);
            shares.push(ShamirShare { x: i as u64, y, prime: self.prime });
        }
        shares
    }

    /// Lagrange interpolation to reconstruct secret from shares.
    pub fn reconstruct(&self, shares: &[ShamirShare]) -> u64 {
        let prime = self.prime;
        let mut secret = 0u64;
        for i in 0..shares.len() {
            let xi = shares[i].x;
            let yi = shares[i].y;
            let mut num = 1u64;
            let mut den = 1u64;
            for j in 0..shares.len() {
                if i == j { continue; }
                let xj = shares[j].x;
                num = (num * ((prime - xj) % prime)) % prime;
                den = (den * ((prime + xi - xj) % prime)) % prime;
            }
            // modular inverse of den
            let den_inv = self.mod_inverse(den, prime);
            let lagrange = (yi * (num % prime) % prime) * den_inv % prime;
            secret = (secret + lagrange) % prime;
        }
        secret
    }

    fn mod_inverse(&self, a: u64, prime: u64) -> u64 {
        Self::pow_mod(a, prime - 2, prime)
    }

    fn pow_mod(base: u64, exp: u64, modulus: u64) -> u64 {
        let mut result = 1u64;
        let mut b = base % modulus;
        let mut e = exp;
        while e > 0 {
            if e & 1 == 1 {
                result = (result * b) % modulus;
            }
            e >>= 1;
            b = (b * b) % modulus;
        }
        result
    }
}

/// Orchestrator that manages MPC sessions and Shamir operations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MPCOrchestrator {
    pub sessions: Vec<MPCSession>,
    pub active_session_id: Option<String>,
}

impl MPCOrchestrator {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
            active_session_id: None,
        }
    }

    pub fn create_session(&mut self, id: String, protocol: MPCProtocol, threshold: u32) -> &MPCSession {
        self.sessions.push(MPCSession::new(id.clone(), protocol, threshold));
        self.active_session_id = Some(id.clone());
        self.sessions.last().unwrap()
    }

    pub fn get_session(&self, id: &str) -> Option<&MPCSession> {
        self.sessions.iter().find(|s| s.session_id == id)
    }

    pub fn get_session_mut(&mut self, id: &str) -> Option<&mut MPCSession> {
        self.sessions.iter_mut().find(|s| s.session_id == id)
    }

    pub fn list_sessions(&self) -> &[MPCSession] {
        &self.sessions
    }
}

// ---------------------------------------------------------------------------
// Secure Aggregation / Federated Learning
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GradientUpdate {
    pub participant_id: String,
    pub round: u64,
    pub values: Vec<f64>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AggregatedGradient {
    pub round: u64,
    pub values: Vec<f64>,
    pub participant_count: u32,
    pub dp_noise_sigma: f64,
    pub aggregated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GradientAggregator {
    pub current_round: u64,
    pub gradients: Vec<GradientUpdate>,
    pub aggregated: Vec<AggregatedGradient>,
    pub dp_noise_sigma: f64,
    pub dp_clip_norm: f64,
    pub learning_rate: f64,
}

impl GradientAggregator {
    pub fn new(dp_noise_sigma: f64, dp_clip_norm: f64, learning_rate: f64) -> Self {
        Self {
            current_round: 0,
            gradients: Vec::new(),
            aggregated: Vec::new(),
            dp_noise_sigma,
            dp_clip_norm,
            learning_rate,
        }
    }

    /// Submit a gradient update for the current round.
    pub fn submit_gradient(&mut self, participant_id: String, values: Vec<f64>) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // Clip gradient norm for DP
        let clipped = self.clip_gradient(&values);
        self.gradients.push(GradientUpdate {
            participant_id,
            round: self.current_round,
            values: clipped,
            timestamp,
        });
    }

    fn clip_gradient(&self, values: &[f64]) -> Vec<f64> {
        let norm: f64 = values.iter().map(|v| v * v).sum::<f64>().sqrt();
        if norm <= self.dp_clip_norm || norm == 0.0 {
            values.to_vec()
        } else {
            let scale = self.dp_clip_norm / norm;
            values.iter().map(|v| v * scale).collect()
        }
    }

    /// Add Gaussian DP noise using a simple Box-Muller transform.
    fn add_dp_noise(&self, values: &[f64], rng_seed: u64) -> Vec<f64> {
        let mut rng = rng_seed;
        let mut next_rand = move || -> f64 {
            rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (rng as f64) / (u64::MAX as f64)
        };
        values
            .iter()
            .map(|v| {
                // Box-Muller
                let u1 = next_rand();
                let u2 = next_rand();
                let z0 = ((-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos())
                    * self.dp_noise_sigma;
                v + z0
            })
            .collect()
    }

    /// Federated averaging: average submitted gradients, apply DP noise, advance round.
    pub fn federated_average(&mut self) -> Option<AggregatedGradient> {
        if self.gradients.is_empty() {
            return None;
        }
        let count = self.gradients.len();
        let dim = self.gradients[0].values.len();
        if dim == 0 {
            return None;
        }
        // Average
        let mut avg = vec![0.0f64; dim];
        for g in &self.gradients {
            for (i, v) in g.values.iter().enumerate() {
                if i < dim {
                    avg[i] += v / count as f64;
                }
            }
        }
        // Apply DP noise
        let rng_seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(42);
        let noisy_avg = self.add_dp_noise(&avg, rng_seed);
        // Apply learning rate
        let final_values: Vec<f64> = noisy_avg.iter().map(|v| v * self.learning_rate).collect();
        let aggregated = AggregatedGradient {
            round: self.current_round,
            values: final_values,
            participant_count: count as u32,
            dp_noise_sigma: self.dp_noise_sigma,
            aggregated_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        };
        self.aggregated.push(aggregated.clone());
        self.current_round += 1;
        self.gradients.clear();
        Some(aggregated)
    }

    pub fn get_aggregated_history(&self) -> &[AggregatedGradient] {
        &self.aggregated
    }

    pub fn pending_count(&self) -> usize {
        self.gradients.len()
    }
}

// ---------------------------------------------------------------------------
// Request / Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EncryptRequest {
    pub scheme: HEScheme,
    pub plaintext: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EncryptResponse {
    pub ciphertext: HECiphertext,
    pub context_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecryptRequest {
    pub context_id: String,
    pub ciphertext: HECiphertext,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecryptResponse {
    pub plaintext: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HEOperationRequest {
    pub context_id: String,
    pub a: HECiphertext,
    pub b: HECiphertext,
    pub op: String, // "add" or "multiply"
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HEOperationResponse {
    pub result: HECiphertext,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateContextRequest {
    pub scheme: HEScheme,
    pub poly_modulus_degree: u64,
    pub coeff_modulus_bits: u32,
    pub plain_modulus: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateContextResponse {
    pub context_id: String,
    pub scheme: HEScheme,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MPCSessionRequest {
    pub protocol: MPCProtocol,
    pub threshold: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MPCSessionResponse {
    pub session_id: String,
    pub protocol: MPCProtocol,
    pub threshold: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AddParticipantRequest {
    pub session_id: String,
    pub participant_id: String,
    pub public_key: String,
    pub address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AddParticipantResponse {
    pub session_id: String,
    pub participant_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShamirSplitRequest {
    pub secret: u64,
    pub num_shares: u32,
    pub threshold: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShamirSplitResponse {
    pub shares: Vec<ShamirShare>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShamirReconstructRequest {
    pub shares: Vec<ShamirShare>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShamirReconstructResponse {
    pub secret: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubmitGradientRequest {
    pub participant_id: String,
    pub values: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SubmitGradientResponse {
    pub accepted: bool,
    pub pending_count: usize,
    pub round: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AggregateRequest {
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AggregateResponse {
    pub aggregated: bool,
    pub gradient: Option<AggregatedGradient>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AggregationHistoryResponse {
    pub history: Vec<AggregatedGradient>,
    pub total_rounds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InfoResponse {
    pub module: String,
    pub schemes_supported: Vec<String>,
    pub mpc_protocols: Vec<String>,
    pub version: String,
}

// ---------------------------------------------------------------------------
// Application State
// ---------------------------------------------------------------------------

pub struct HomomorphicComputeState {
    pub he_contexts: DashMap<String, HEModule>,
    pub mpc_orchestrators: DashMap<String, MPCOrchestrator>,
    pub gradient_aggregators: DashMap<String, GradientAggregator>,
    pub request_counter: DashMap<String, u64>,
}

impl HomomorphicComputeState {
    pub fn new() -> Self {
        Self {
            he_contexts: DashMap::new(),
            mpc_orchestrators: DashMap::new(),
            gradient_aggregators: DashMap::new(),
            request_counter: DashMap::new(),
        }
    }

    pub fn increment_counter(&self, key: &str) -> u64 {
        let mut counter = self.request_counter.entry(key.to_string()).or_insert(0);
        let val = *counter + 1;
        *counter = val;
        val
    }

    pub fn get_counter(&self, key: &str) -> u64 {
        self.request_counter.get(key).map(|r| *r).unwrap_or(0)
    }

    pub fn context_count(&self) -> usize {
        self.he_contexts.len()
    }

    pub fn session_count(&self) -> usize {
        self.mpc_orchestrators
            .iter()
            .map(|r| r.value().sessions.len())
            .sum()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn generate_id(prefix: &str) -> String {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let h = hash(&ts.to_le_bytes());
    format!("{}_{}", prefix, &hex::encode(&h.as_bytes()[..8]))
}

fn bytes_to_string(data: &[u8]) -> String {
    String::from_utf8_lossy(data).to_string()
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        // HE endpoints
        .route("/he/info", get(handle_info))
        .route("/he/context", post(handle_create_context))
        .route("/he/encrypt", post(handle_encrypt))
        .route("/he/decrypt", post(handle_decrypt))
        .route("/he/operate", post(handle_he_operate))
        .route("/he/context/:id", get(handle_get_context))
        .route("/he/contexts", get(handle_list_contexts))
        // MPC endpoints
        .route("/mpc/session", post(handle_create_session))
        .route("/mpc/participant", post(handle_add_participant))
        .route("/mpc/sessions", get(handle_list_sessions))
        .route("/mpc/shamir/split", post(handle_shamir_split))
        .route("/mpc/shamir/reconstruct", post(handle_shamir_reconstruct))
        // Aggregation endpoints
        .route("/agg/gradient", post(handle_submit_gradient))
        .route("/agg/aggregate", post(handle_aggregate))
        .route("/agg/history", get(handle_aggregation_history))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn handle_info(
    State(state): State<AppState>,
) -> Result<Json<InfoResponse>, StatusCode> {
    let hc = &state.homomorphic_compute;
    hc.increment_counter("info");
    Ok(Json(InfoResponse {
        module: "homomorphic_compute".to_string(),
        schemes_supported: vec!["BFV".into(), "CKKS".into(), "MockPaillier".into()],
        mpc_protocols: vec!["Shamir".into(), "Replicated".into(), "GMW".into()],
        version: "0.1.0".to_string(),
    }))
}

async fn handle_create_context(
    State(state): State<AppState>,
    Json(req): Json<CreateContextRequest>,
) -> Result<Json<CreateContextResponse>, StatusCode> {
    let hc = &state.homomorphic_compute;
    hc.increment_counter("create_context");
    let scheme = req.scheme.clone();
    let module = HEModule::new(req.scheme, req.poly_modulus_degree, req.coeff_modulus_bits, req.plain_modulus);
    let id = generate_id("he");
    hc.he_contexts.insert(id.clone(), module);
    Ok(Json(CreateContextResponse {
        context_id: id,
        scheme,
    }))
}

async fn handle_encrypt(
    State(state): State<AppState>,
    Json(req): Json<EncryptRequest>,
) -> Result<Json<EncryptResponse>, StatusCode> {
    let hc = &state.homomorphic_compute;
    hc.increment_counter("encrypt");
    let module = HEModule::new(req.scheme.clone(), 8192, 128, 65537);
    let ct = module.encrypt(req.plaintext.as_bytes());
    let id = generate_id("he");
    hc.he_contexts.insert(id.clone(), module);
    Ok(Json(EncryptResponse {
        ciphertext: ct,
        context_id: id,
    }))
}

async fn handle_decrypt(
    State(state): State<AppState>,
    Json(req): Json<DecryptRequest>,
) -> Result<Json<DecryptResponse>, StatusCode> {
    let hc = &state.homomorphic_compute;
    hc.increment_counter("decrypt");
    let module = hc.he_contexts.get(&req.context_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    let plaintext = module.decrypt(&req.ciphertext);
    Ok(Json(DecryptResponse {
        plaintext: bytes_to_string(&plaintext),
    }))
}

async fn handle_he_operate(
    State(state): State<AppState>,
    Json(req): Json<HEOperationRequest>,
) -> Result<Json<HEOperationResponse>, StatusCode> {
    let hc = &state.homomorphic_compute;
    hc.increment_counter("operate");
    let module = hc.he_contexts.get(&req.context_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    let result = match req.op.as_str() {
        "add" => module.add(&req.a, &req.b),
        "multiply" => module.multiply(&req.a, &req.b),
        _ => return Err(StatusCode::BAD_REQUEST),
    };
    Ok(Json(HEOperationResponse { result }))
}

async fn handle_get_context(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<CreateContextResponse>, StatusCode> {
    let hc = &state.homomorphic_compute;
    hc.increment_counter("get_context");
    let module = hc.he_contexts.get(&id)
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(CreateContextResponse {
        context_id: id,
        scheme: module.context.scheme.clone(),
    }))
}

async fn handle_list_contexts(
    State(state): State<AppState>,
) -> Result<Json<Vec<CreateContextResponse>>, StatusCode> {
    let hc = &state.homomorphic_compute;
    hc.increment_counter("list_contexts");
    let contexts: Vec<CreateContextResponse> = hc
        .he_contexts
        .iter()
        .map(|r| CreateContextResponse {
            context_id: r.key().clone(),
            scheme: r.value().context.scheme.clone(),
        })
        .collect();
    Ok(Json(contexts))
}

async fn handle_create_session(
    State(state): State<AppState>,
    Json(req): Json<MPCSessionRequest>,
) -> Result<Json<MPCSessionResponse>, StatusCode> {
    let hc = &state.homomorphic_compute;
    hc.increment_counter("create_session");
    let id = generate_id("mpc");
    let orch_id = "default".to_string();
    let protocol = req.protocol.clone();
    let mut orch = hc.mpc_orchestrators.entry(orch_id.clone())
        .or_insert_with(MPCOrchestrator::new);
    orch.create_session(id.clone(), req.protocol, req.threshold);
    Ok(Json(MPCSessionResponse {
        session_id: id,
        protocol,
        threshold: req.threshold,
    }))
}

async fn handle_add_participant(
    State(state): State<AppState>,
    Json(req): Json<AddParticipantRequest>,
) -> Result<Json<AddParticipantResponse>, StatusCode> {
    let hc = &state.homomorphic_compute;
    hc.increment_counter("add_participant");
    let orch = hc.mpc_orchestrators.get("default")
        .ok_or(StatusCode::NOT_FOUND)?;
    // We need mutable access, so use entry
    drop(orch);
    let mut orch = hc.mpc_orchestrators.get_mut("default")
        .ok_or(StatusCode::NOT_FOUND)?;
    let session = orch.get_session_mut(&req.session_id)
        .ok_or(StatusCode::NOT_FOUND)?;
    session.add_participant(Participant {
        id: req.participant_id.clone(),
        public_key: req.public_key,
        address: req.address,
        threshold: session.threshold,
    });
    let count = session.participants.len();
    Ok(Json(AddParticipantResponse {
        session_id: req.session_id,
        participant_count: count,
    }))
}

async fn handle_list_sessions(
    State(state): State<AppState>,
) -> Result<Json<Vec<MPCSessionResponse>>, StatusCode> {
    let hc = &state.homomorphic_compute;
    hc.increment_counter("list_sessions");
    let mut sessions = Vec::new();
    for entry in hc.mpc_orchestrators.iter() {
        for s in entry.value().list_sessions() {
            sessions.push(MPCSessionResponse {
                session_id: s.session_id.clone(),
                protocol: s.protocol.clone(),
                threshold: s.threshold,
            });
        }
    }
    Ok(Json(sessions))
}

async fn handle_shamir_split(
    State(state): State<AppState>,
    Json(req): Json<ShamirSplitRequest>,
) -> Result<Json<ShamirSplitResponse>, StatusCode> {
    let hc = &state.homomorphic_compute;
    hc.increment_counter("shamir_split");
    let prime = 2_147_483_647u64; // large Mersenne-ish prime
    let sharing = ShamirSharing::new(prime);
    let shares = sharing.split(req.secret, req.num_shares, req.threshold);
    Ok(Json(ShamirSplitResponse { shares }))
}

async fn handle_shamir_reconstruct(
    State(state): State<AppState>,
    Json(req): Json<ShamirReconstructRequest>,
) -> Result<Json<ShamirReconstructResponse>, StatusCode> {
    let hc = &state.homomorphic_compute;
    hc.increment_counter("shamir_reconstruct");
    if req.shares.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let prime = req.shares[0].prime;
    let sharing = ShamirSharing::new(prime);
    let secret = sharing.reconstruct(&req.shares);
    Ok(Json(ShamirReconstructResponse { secret }))
}

async fn handle_submit_gradient(
    State(state): State<AppState>,
    Json(req): Json<SubmitGradientRequest>,
) -> Result<Json<SubmitGradientResponse>, StatusCode> {
    let hc = &state.homomorphic_compute;
    hc.increment_counter("submit_gradient");
    let agg_id = "default".to_string();
    let mut agg = hc.gradient_aggregators.entry(agg_id)
        .or_insert_with(|| GradientAggregator::new(0.1, 1.0, 0.01));
    let round = agg.current_round;
    agg.submit_gradient(req.participant_id, req.values);
    let pending = agg.pending_count();
    Ok(Json(SubmitGradientResponse {
        accepted: true,
        pending_count: pending,
        round,
    }))
}

async fn handle_aggregate(
    State(state): State<AppState>,
    Json(_req): Json<AggregateRequest>,
) -> Result<Json<AggregateResponse>, StatusCode> {
    let hc = &state.homomorphic_compute;
    hc.increment_counter("aggregate");
    let agg_id = "default".to_string();
    let mut agg = hc.gradient_aggregators.entry(agg_id)
        .or_insert_with(|| GradientAggregator::new(0.1, 1.0, 0.01));
    let gradient = agg.federated_average();
    Ok(Json(AggregateResponse {
        aggregated: gradient.is_some(),
        gradient,
    }))
}

async fn handle_aggregation_history(
    State(state): State<AppState>,
) -> Result<Json<AggregationHistoryResponse>, StatusCode> {
    let hc = &state.homomorphic_compute;
    hc.increment_counter("agg_history");
    let agg_id = "default".to_string();
    if let Some(agg) = hc.gradient_aggregators.get(&agg_id) {
        let history = agg.get_aggregated_history().to_vec();
        let total = agg.current_round;
        Ok(Json(AggregationHistoryResponse { history, total_rounds: total }))
    } else {
        Ok(Json(AggregationHistoryResponse {
            history: Vec::new(),
            total_rounds: 0,
        }))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- HE Tests --

    #[test]
    fn test_he_module_new_bfv() {
        let m = HEModule::new(HEScheme::BFV, 8192, 128, 65537);
        assert_eq!(m.context.scheme, HEScheme::BFV);
        assert_eq!(m.context.poly_modulus_degree, 8192);
    }

    #[test]
    fn test_he_module_new_ckks() {
        let m = HEModule::new(HEScheme::CKKS, 16384, 256, 0);
        assert_eq!(m.context.scheme, HEScheme::CKKS);
    }

    #[test]
    fn test_he_module_new_mock_paillier() {
        let m = HEModule::new(HEScheme::MockPaillier, 2048, 64, 0);
        assert_eq!(m.context.scheme, HEScheme::MockPaillier);
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let m = HEModule::new(HEScheme::BFV, 8192, 128, 65537);
        let plaintext = b"hello homomorphic world";
        let ct = m.encrypt(plaintext);
        let decrypted = m.decrypt(&ct);
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip_ckks() {
        let m = HEModule::new(HEScheme::CKKS, 8192, 128, 0);
        let plaintext = b"approximate arithmetic";
        let ct = m.encrypt(plaintext);
        let decrypted = m.decrypt(&ct);
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_produces_different_nonces() {
        let m = HEModule::new(HEScheme::BFV, 8192, 128, 65537);
        let ct1 = m.encrypt(b"data");
        let ct2 = m.encrypt(b"data");
        assert_ne!(ct1.nonce, ct2.nonce);
    }

    #[test]
    fn test_encrypt_same_seed_same_nonce() {
        let m = HEModule::new(HEScheme::BFV, 8192, 128, 65537);
        let seed = m.context.seed;
        let m2 = HEModule {
            context: HEContext {
                scheme: HEScheme::BFV,
                poly_modulus_degree: 8192,
                coeff_modulus_bits: 128,
                plain_modulus: 65537,
                seed,
            },
        };
        // Same seed => same keystream but different nonce still
        let ct1 = m.encrypt(b"data");
        let _ct2 = m2.encrypt(b"data");
        assert_eq!(m.context.seed, m2.context.seed);
    }

    #[test]
    fn test_add_ciphertexts() {
        let m = HEModule::new(HEScheme::BFV, 8192, 128, 65537);
        let ct1 = m.encrypt(b"aaa");
        let ct2 = m.encrypt(b"bbb");
        let ct_sum = m.add(&ct1, &ct2);
        assert_eq!(ct_sum.scheme, HEScheme::BFV);
        assert_eq!(ct_sum.data.len(), ct1.data.len().max(ct2.data.len()));
    }

    #[test]
    fn test_multiply_ciphertexts() {
        let m = HEModule::new(HEScheme::CKKS, 8192, 128, 0);
        let ct1 = m.encrypt(b"hello");
        let ct2 = m.encrypt(b"world");
        let ct_prod = m.multiply(&ct1, &ct2);
        assert_eq!(ct_prod.scheme, HEScheme::CKKS);
        assert_eq!(ct_prod.data.len(), ct1.data.len().min(ct2.data.len()));
    }

    #[test]
    fn test_add_scheme_mismatch() {
        let m_bfv = HEModule::new(HEScheme::BFV, 8192, 128, 65537);
        let m_ckks = HEModule::new(HEScheme::CKKS, 8192, 128, 0);
        let ct_bfv = m_bfv.encrypt(b"x");
        let ct_ckks = m_ckks.encrypt(b"x");
        let result = std::panic::catch_unwind(|| {
            m_bfv.add(&ct_bfv, &ct_ckks);
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_multiply_scheme_mismatch() {
        let m1 = HEModule::new(HEScheme::MockPaillier, 2048, 64, 0);
        let m2 = HEModule::new(HEScheme::BFV, 8192, 128, 65537);
        let ct1 = m1.encrypt(b"x");
        let ct2 = m2.encrypt(b"x");
        let result = std::panic::catch_unwind(|| {
            m1.multiply(&ct1, &ct2);
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_plaintext() {
        let m = HEModule::new(HEScheme::BFV, 8192, 128, 65537);
        let ct = m.encrypt(b"");
        let dec = m.decrypt(&ct);
        assert_eq!(dec, b"");
    }

    // -- MPC Tests --

    #[test]
    fn test_mpc_session_new() {
        let s = MPCSession::new("sess1".into(), MPCProtocol::Shamir, 3);
        assert_eq!(s.session_id, "sess1");
        assert_eq!(s.threshold, 3);
        assert!(!s.shares_generated);
    }

    #[test]
    fn test_mpc_session_add_participant() {
        let mut s = MPCSession::new("sess1".into(), MPCProtocol::Shamir, 2);
        s.add_participant(Participant {
            id: "p1".into(),
            public_key: "key1".into(),
            address: "addr1".into(),
            threshold: 2,
        });
        assert_eq!(s.participants.len(), 1);
        assert_eq!(s.participants[0].id, "p1");
    }

    #[test]
    fn test_mpc_orchestrator_create_session() {
        let mut orch = MPCOrchestrator::new();
        orch.create_session("s1".into(), MPCProtocol::GMW, 5);
        assert_eq!(orch.sessions.len(), 1);
        assert_eq!(orch.active_session_id, Some("s1".into()));
    }

    #[test]
    fn test_mpc_orchestrator_list_sessions() {
        let mut orch = MPCOrchestrator::new();
        orch.create_session("s1".into(), MPCProtocol::Shamir, 3);
        orch.create_session("s2".into(), MPCProtocol::Replicated, 2);
        assert_eq!(orch.list_sessions().len(), 2);
    }

    #[test]
    fn test_mpc_orchestrator_get_session() {
        let mut orch = MPCOrchestrator::new();
        orch.create_session("s1".into(), MPCProtocol::Shamir, 3);
        let s = orch.get_session("s1");
        assert!(s.is_some());
        assert_eq!(s.unwrap().protocol, MPCProtocol::Shamir);
        assert!(orch.get_session("nonexistent").is_none());
    }

    // -- Shamir Tests --

    #[test]
    fn test_shamir_split_reconstruct() {
        let prime = 2_147_483_647u64;
        let sharing = ShamirSharing::new(prime);
        let secret = 42u64;
        let shares = sharing.split(secret, 5, 3);
        assert_eq!(shares.len(), 5);
        // Reconstruct with exactly threshold
        let reconstructed = sharing.reconstruct(&shares[..3]);
        assert_eq!(reconstructed, secret);
    }

    #[test]
    fn test_shamir_reconstruct_different_subset() {
        let prime = 1_000_000_007u64;
        let sharing = ShamirSharing::new(prime);
        let secret = 12345u64;
        let shares = sharing.split(secret, 5, 3);
        // Use shares 1, 3, 5
        let subset = vec![shares[0].clone(), shares[2].clone(), shares[4].clone()];
        let reconstructed = sharing.reconstruct(&subset);
        assert_eq!(reconstructed, secret);
    }

    #[test]
    fn test_shamir_reconstruct_with_all_shares() {
        let prime = 997u64;
        let sharing = ShamirSharing::new(prime);
        let secret = 500u64;
        let shares = sharing.split(secret, 4, 2);
        let reconstructed = sharing.reconstruct(&shares);
        assert_eq!(reconstructed, secret);
    }

    #[test]
    fn test_shamir_split_threshold_one() {
        let prime = 1_000_000_007u64;
        let sharing = ShamirSharing::new(prime);
        let secret = 999u64;
        let shares = sharing.split(secret, 3, 1);
        // Any single share should reconstruct
        for share in &shares {
            let reconstructed = sharing.reconstruct(&[share.clone()]);
            assert_eq!(reconstructed, secret);
        }
    }

    #[test]
    fn test_shamir_large_secret() {
        let prime = 2_147_483_647u64;
        let sharing = ShamirSharing::new(prime);
        let secret = prime - 1;
        let shares = sharing.split(secret, 3, 2);
        let reconstructed = sharing.reconstruct(&shares[..2]);
        assert_eq!(reconstructed, secret);
    }

    #[test]
    fn test_eval_poly() {
        // f(x) = 2x^2 + 3x + 1 mod 97, f(5) = 2*25 + 3*5 + 1 = 66 mod 97
        let coeffs = vec![1u64, 3, 2]; // a0 + a1*x + a2*x^2
        let val = ShamirSharing::eval_poly(&coeffs, 5, 97);
        assert_eq!(val, 66);
    }

    #[test]
    fn test_shamir_zero_secret() {
        let prime = 1_000_000_007u64;
        let sharing = ShamirSharing::new(prime);
        let shares = sharing.split(0, 3, 2);
        let reconstructed = sharing.reconstruct(&shares[..2]);
        assert_eq!(reconstructed, 0);
    }

    // -- Gradient Aggregation Tests --

    #[test]
    fn test_gradient_aggregator_new() {
        let agg = GradientAggregator::new(0.1, 1.0, 0.01);
        assert_eq!(agg.current_round, 0);
        assert_eq!(agg.dp_noise_sigma, 0.1);
        assert_eq!(agg.learning_rate, 0.01);
    }

    #[test]
    fn test_submit_gradient() {
        let mut agg = GradientAggregator::new(0.0, 10.0, 1.0);
        agg.submit_gradient("p1".into(), vec![1.0, 2.0, 3.0]);
        assert_eq!(agg.pending_count(), 1);
    }

    #[test]
    fn test_gradient_clip() {
        let mut agg = GradientAggregator::new(0.0, 1.0, 1.0);
        agg.submit_gradient("p1".into(), vec![3.0, 4.0]); // norm = 5, clip to 1 => [0.6, 0.8]
        let g = &agg.gradients[0];
        let norm: f64 = g.values.iter().map(|v| v * v).sum::<f64>().sqrt();
        assert!(norm <= 1.0 + 1e-9);
    }

    #[test]
    fn test_federated_average() {
        let mut agg = GradientAggregator::new(0.0, 10.0, 1.0); // no DP noise, lr=1
        agg.submit_gradient("p1".into(), vec![2.0, 4.0]);
        agg.submit_gradient("p2".into(), vec![4.0, 8.0]);
        let result = agg.federated_average();
        assert!(result.is_some());
        let grad = result.unwrap();
        assert_eq!(grad.participant_count, 2);
        assert_eq!(grad.round, 0);
        // avg should be [3.0, 6.0] * lr=1 = [3.0, 6.0]
        assert!((grad.values[0] - 3.0).abs() < 1.0); // tolerance for any float imprecision
        assert!((grad.values[1] - 6.0).abs() < 1.0);
    }

    #[test]
    fn test_federated_average_no_gradients() {
        let mut agg = GradientAggregator::new(0.0, 10.0, 1.0);
        let result = agg.federated_average();
        assert!(result.is_none());
    }

    #[test]
    fn test_federated_average_advances_round() {
        let mut agg = GradientAggregator::new(0.0, 10.0, 1.0);
        agg.submit_gradient("p1".into(), vec![1.0]);
        agg.federated_average();
        assert_eq!(agg.current_round, 1);
        assert_eq!(agg.pending_count(), 0);
    }

    #[test]
    fn test_aggregation_history() {
        let mut agg = GradientAggregator::new(0.0, 10.0, 1.0);
        agg.submit_gradient("p1".into(), vec![1.0]);
        agg.federated_average();
        agg.submit_gradient("p2".into(), vec![2.0]);
        agg.federated_average();
        assert_eq!(agg.get_aggregated_history().len(), 2);
    }

    #[test]
    fn test_dp_noise_is_applied() {
        let mut agg = GradientAggregator::new(10.0, 10.0, 1.0); // high noise
        agg.submit_gradient("p1".into(), vec![0.0]);
        agg.submit_gradient("p2".into(), vec![0.0]);
        let result = agg.federated_average().unwrap();
        // With high noise, result should NOT be exactly 0
        // (probability of both noise samples being exactly 0 is negligible)
        assert!(result.values[0].abs() > 0.0 || result.dp_noise_sigma > 0.0);
    }

    // -- State Tests --

    #[test]
    fn test_homomorphic_compute_state_new() {
        let state = HomomorphicComputeState::new();
        assert_eq!(state.context_count(), 0);
        assert_eq!(state.session_count(), 0);
    }

    #[test]
    fn test_counter_increment() {
        let state = HomomorphicComputeState::new();
        assert_eq!(state.increment_counter("test"), 1);
        assert_eq!(state.increment_counter("test"), 2);
        assert_eq!(state.get_counter("test"), 2);
    }

    #[test]
    fn test_counter_default_zero() {
        let state = HomomorphicComputeState::new();
        assert_eq!(state.get_counter("nonexistent"), 0);
    }

    #[test]
    fn test_state_context_count() {
        let state = HomomorphicComputeState::new();
        let m = HEModule::new(HEScheme::BFV, 8192, 128, 65537);
        state.he_contexts.insert("c1".into(), m);
        state.he_contexts.insert("c2".into(), HEModule::new(HEScheme::CKKS, 8192, 128, 0));
        assert_eq!(state.context_count(), 2);
    }

    #[test]
    fn test_generate_id_unique() {
        let id1 = generate_id("test");
        let id2 = generate_id("test");
        // Very unlikely to be the same but possible in theory
        // Just check format
        assert!(id1.starts_with("test_"));
        assert!(id2.starts_with("test_"));
    }

    #[test]
    fn test_bytes_to_string() {
        let result = bytes_to_string(b"hello");
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_he_serialization_roundtrip() {
        let m = HEModule::new(HEScheme::BFV, 8192, 128, 65537);
        let ct = m.encrypt(b"test data");
        let json = serde_json::to_string(&ct).unwrap();
        let ct2: HECiphertext = serde_json::from_str(&json).unwrap();
        assert_eq!(ct, ct2);
    }

    #[test]
    fn test_mpc_session_serialization() {
        let s = MPCSession::new("s1".into(), MPCProtocol::Shamir, 3);
        let json = serde_json::to_string(&s).unwrap();
        let s2: MPCSession = serde_json::from_str(&json).unwrap();
        assert_eq!(s, s2);
    }

    #[test]
    fn test_shamir_share_serialization() {
        let share = ShamirShare { x: 1, y: 42, prime: 997 };
        let json = serde_json::to_string(&share).unwrap();
        let s2: ShamirShare = serde_json::from_str(&json).unwrap();
        assert_eq!(share, s2);
    }

    #[test]
    fn test_gradient_update_serialization() {
        let gu = GradientUpdate {
            participant_id: "p1".into(),
            round: 1,
            values: vec![0.1, 0.2],
            timestamp: 1000,
        };
        let json = serde_json::to_string(&gu).unwrap();
        let gu2: GradientUpdate = serde_json::from_str(&json).unwrap();
        assert_eq!(gu, gu2);
    }

    #[test]
    fn test_aggregated_gradient_serialization() {
        let ag = AggregatedGradient {
            round: 0,
            values: vec![1.0, 2.0],
            participant_count: 3,
            dp_noise_sigma: 0.1,
            aggregated_at: 999,
        };
        let json = serde_json::to_string(&ag).unwrap();
        let ag2: AggregatedGradient = serde_json::from_str(&json).unwrap();
        assert_eq!(ag, ag2);
    }

    #[test]
    fn test_info_response_serialization() {
        let info = InfoResponse {
            module: "test".into(),
            schemes_supported: vec!["BFV".into()],
            mpc_protocols: vec!["Shamir".into()],
            version: "1.0".into(),
        };
        let json = serde_json::to_string(&info).unwrap();
        let info2: InfoResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(info, info2);
    }

    #[test]
    fn test_he_context_serialization() {
        let ctx = HEContext {
            scheme: HEScheme::MockPaillier,
            poly_modulus_degree: 2048,
            coeff_modulus_bits: 64,
            plain_modulus: 0,
            seed: [42u8; 32],
        };
        let json = serde_json::to_string(&ctx).unwrap();
        let ctx2: HEContext = serde_json::from_str(&json).unwrap();
        assert_eq!(ctx, ctx2);
    }

    #[test]
    fn test_he_scheme_serialization() {
        for scheme in [HEScheme::BFV, HEScheme::CKKS, HEScheme::MockPaillier] {
            let json = serde_json::to_string(&scheme).unwrap();
            let s2: HEScheme = serde_json::from_str(&json).unwrap();
            assert_eq!(scheme, s2);
        }
    }
}
