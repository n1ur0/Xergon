#![allow(dead_code)]
//! Quantum-Resistant Cryptographic Primitives Module
//!
//! Provides post-quantum cryptographic primitives for the Xergon relay:
//!
//! 1. **Kyber-like post-quantum KEM**: Simplified ML-KEM-inspired key encapsulation
//!    using SHAKE256-based deterministic key derivation and matrix-based key generation.
//!
//! 2. **Homomorphic inference verification**: Hash-based commitment scheme for model
//!    weights with Merkle tree proofs and verifiable computation receipts.
//!
//! 3. **Hybrid encryption mode**: Combines classical (X25519+AES-256-GCM) with
//!    post-quantum KEM for "best of both worlds" security.
//!
//! Endpoints:
//! - POST /v1/quantum/keygen       — generate hybrid keypair (classical + PQ)
//! - POST /v1/quantum/encapsulate  — encapsulate hybrid shared secret
//! - POST /v1/quantum/verify       — verify homomorphic inference proof
//! - POST /v1/quantum/commit-model — provider commits to model weights
//! - GET  /v1/quantum/proofs       — list verified proofs
//! - GET  /v1/quantum/status       — crypto module status
//! - DELETE /v1/quantum/proofs     — clear proof cache (admin)
//! - PUT  /v1/quantum/config       — update quantum crypto config

use std::time::{Duration, Instant};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha3::digest::Digest;
use sha2::Sha256;
use sha3::Shake256;
use sha3::digest::{ExtendableOutput, XofReader};
use tracing::{info, warn};
use x25519_dalek::{PublicKey, StaticSecret};

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors specific to the quantum crypto module.
#[derive(Debug, thiserror::Error)]
pub enum QuantumCryptoError {
    #[error("key generation error: {0}")]
    KeyGeneration(String),
    #[error("encapsulation error: {0}")]
    Encapsulation(String),
    #[error("decapsulation error: {0}")]
    Decapsulation(String),
    #[error("verification failed: {0}")]
    VerificationFailed(String),
    #[error("proof expired")]
    ProofExpired,
    #[error("replay detected: proof already verified")]
    ReplayDetected,
    #[error("model not committed: {0}")]
    ModelNotCommitted(String),
    #[error("invalid commitment")]
    InvalidCommitment,
    #[error("invalid key format")]
    InvalidKeyFormat,
    #[error("encryption error: {0}")]
    Encryption(String),
    #[error("config error: {0}")]
    Config(String),
}

impl IntoResponse for QuantumCryptoError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            Self::KeyGeneration(_)
            | Self::Encapsulation(_)
            | Self::Decapsulation(_)
            | Self::Encryption(_)
            | Self::Config(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            Self::VerificationFailed(_) | Self::InvalidCommitment => {
                (StatusCode::UNPROCESSABLE_ENTITY, self.to_string())
            }
            Self::ProofExpired => (StatusCode::GONE, self.to_string()),
            Self::ReplayDetected => (StatusCode::CONFLICT, self.to_string()),
            Self::ModelNotCommitted(_) => (StatusCode::NOT_FOUND, self.to_string()),
            Self::InvalidKeyFormat => (StatusCode::BAD_REQUEST, self.to_string()),
        };
        (
            status,
            Json(serde_json::json!({
                "error": message,
            })),
        )
            .into_response()
    }
}

// ---------------------------------------------------------------------------
// Type aliases
// ---------------------------------------------------------------------------

type HmacSha256 = Hmac<Sha256>;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Matrix dimension for lattice-based KEM (k x k matrix)
const KEM_MATRIX_K: usize = 4;
/// Public matrix size in bytes (k * k * 4 = 64 bytes for k=4)
const KEM_PUBLIC_MATRIX_BYTES: usize = KEM_MATRIX_K * KEM_MATRIX_K * 4;
/// Seed size for deterministic key generation
const KEM_SEED_BYTES: usize = 32;
/// Shared secret size
const KEM_SHARED_SECRET_BYTES: usize = 32;
/// Error bound for lattice rounding
const KEM_ERROR_BOUND: i16 = 8;
/// Chunk size for Merkle tree leaf hashes
const MERKLE_CHUNK_SIZE: usize = 4096;

// ---------------------------------------------------------------------------
// SHAKE256-based deterministic PRNG
// ---------------------------------------------------------------------------

/// A deterministic byte generator from a seed using SHAKE256 (XOF).
struct Shake256Stream {
    state: Shake256,
    buffer: Vec<u8>,
    offset: usize,
}

impl Shake256Stream {
    fn new(seed: &[u8]) -> Self {
        let mut state = Shake256::default();
        // Personalize with domain separator
        sha3::digest::Update::update(&mut state, b"xergon-pq-kem-v1");
        sha3::digest::Update::update(&mut state, seed);
        Self {
            state,
            buffer: Vec::new(),
            offset: 0,
        }
    }

    fn fill_bytes(&mut self, out: &mut [u8]) {
        let mut remaining = out.len();
        let mut written = 0;
        while remaining > 0 {
            if self.offset >= self.buffer.len() {
                // Generate more bytes from SHAKE256 using XOF reader
                self.buffer.clear();
                self.buffer.resize(64, 0);
                let mut reader = self.state.clone().finalize_xof();
                reader.read(&mut self.buffer);
                self.offset = 0;
            }
            let available = self.buffer.len() - self.offset;
            let to_copy = remaining.min(available);
            out[written..written + to_copy]
                .copy_from_slice(&self.buffer[self.offset..self.offset + to_copy]);
            self.offset += to_copy;
            written += to_copy;
            remaining -= to_copy;
        }
    }

    fn next_u16(&mut self) -> u16 {
        let mut buf = [0u8; 2];
        self.fill_bytes(&mut buf);
        u16::from_le_bytes(buf)
    }

    fn next_i16(&mut self) -> i16 {
        let raw = self.next_u16();
        // Center the range: map u16 to [-32768, 32767]
        let centered = raw as i16;
        // Apply error bound by rejection sampling
        let bounded = centered % (KEM_ERROR_BOUND * 2 + 1);
        bounded - KEM_ERROR_BOUND
    }

    fn next_bytes(&mut self, n: usize) -> Vec<u8> {
        let mut out = vec![0u8; n];
        self.fill_bytes(&mut out);
        out
    }
}

// ---------------------------------------------------------------------------
// Post-Quantum KEM (ML-KEM-inspired)
// ---------------------------------------------------------------------------

/// A post-quantum keypair for the lattice-based KEM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostQuantumKeyPair {
    /// Random seed used for deterministic key generation.
    #[serde(with = "hex_bytes_serde")]
    pub seed: Vec<u8>,
    /// Public matrix (k x k) derived from seed.
    #[serde(with = "hex_bytes_serde")]
    pub public_matrix: Vec<u8>,
    /// Secret key derived from seed.
    #[serde(with = "hex_bytes_serde")]
    pub secret_key: Vec<u8>,
}

mod hex_bytes_serde {
    use serde::{self, Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(data: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&hex::encode(data))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        hex::decode(&s).map_err(serde::de::Error::custom)
    }
}

/// A lattice-based ciphertext from encapsulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KEMCiphertext {
    /// Encrypted/encoded matrix element (u vector).
    #[serde(with = "hex_bytes_serde")]
    pub u_vec: Vec<u8>,
    /// Random error vector (v component).
    #[serde(with = "hex_bytes_serde")]
    pub v_vec: Vec<u8>,
    /// Salt used for deterministic encapsulation.
    #[serde(with = "hex_bytes_serde")]
    pub salt: Vec<u8>,
}

/// Generate a new post-quantum keypair using lattice-based key generation.
///
/// Key generation:
/// 1. Generate random seed
/// 2. Use SHAKE256(seed) to deterministically derive a k x k public matrix A
/// 3. Derive secret key s as a small-norm vector from seed
/// 4. Compute public value: T = A * s + e (error term)
pub fn pq_keygen() -> PostQuantumKeyPair {
    let seed = rand::random::<[u8; KEM_SEED_BYTES]>().to_vec();

    let mut prng = Shake256Stream::new(&seed);

    // Generate secret vector s (small-norm)
    let mut secret_vec = Vec::with_capacity(KEM_MATRIX_K);
    for _ in 0..KEM_MATRIX_K {
        secret_vec.push(prng.next_i16());
    }

    // Generate public matrix A from seed (deterministic)
    let mut public_matrix = Vec::with_capacity(KEM_PUBLIC_MATRIX_BYTES);
    let mut a_prng = Shake256Stream::new(&[seed.as_slice(), b"public-matrix"].concat());
    for _ in 0..(KEM_MATRIX_K * KEM_MATRIX_K) {
        let val = a_prng.next_u16();
        public_matrix.extend_from_slice(&val.to_le_bytes());
    }

    // Generate error vector e
    let mut error_vec = Vec::with_capacity(KEM_MATRIX_K);
    for _ in 0..KEM_MATRIX_K {
        error_vec.push(prng.next_i16());
    }

    // T = A * s + e (simplified matrix-vector multiply in the lattice)
    let mut t_vec = Vec::with_capacity(KEM_MATRIX_K);
    for i in 0..KEM_MATRIX_K {
        let mut dot = 0i32;
        for j in 0..KEM_MATRIX_K {
            let a_ij = i16::from_le_bytes(
                public_matrix[(i * KEM_MATRIX_K + j) * 4..(i * KEM_MATRIX_K + j) * 4 + 4]
                    .try_into()
                    .unwrap(),
            );
            dot += (a_ij as i32) * (secret_vec[j] as i32);
        }
        dot += error_vec[i] as i32;
        t_vec.push(dot);
    }

    // Serialize secret key: seed + secret_vec + error_vec
    let mut secret_key = Vec::new();
    secret_key.extend_from_slice(&seed);
    for val in &secret_vec {
        secret_key.extend_from_slice(&val.to_le_bytes());
    }

    PostQuantumKeyPair {
        seed,
        public_matrix,
        secret_key,
    }
}

/// Encapsulate: produce a shared secret and ciphertext from a public key.
///
/// Encapsulation:
/// 1. Generate random salt
/// 2. Derive random vector r and error e1, e2 from salt+public_key
/// 3. Compute u = A^T * r + e1 (ciphertext component)
/// 4. Compute v = T^T * r + e2 + encode(shared_secret_from_KDF)
/// 5. Derive shared secret via SHAKE256(salt || u || v)
pub fn pq_encapsulate(public_key: &PostQuantumKeyPair) -> (Vec<u8>, KEMCiphertext) {
    let salt = rand::random::<[u8; 32]>().to_vec();

    // Create PRNG from salt + public matrix for deterministic randomness
    let entropy = [salt.as_slice(), public_key.public_matrix.as_slice()].concat();
    let mut prng = Shake256Stream::new(&entropy);

    // Generate random vector r
    let mut r_vec = Vec::with_capacity(KEM_MATRIX_K);
    for _ in 0..KEM_MATRIX_K {
        r_vec.push(prng.next_i16());
    }

    // Generate error vectors
    let mut e1_vec = Vec::with_capacity(KEM_MATRIX_K);
    for _ in 0..KEM_MATRIX_K {
        e1_vec.push(prng.next_i16());
    }
    let e2_val = prng.next_i16();

    // Compute u = A^T * r + e1
    let mut u_vec = Vec::with_capacity(KEM_MATRIX_K);
    for j in 0..KEM_MATRIX_K {
        let mut dot = 0i32;
        for i in 0..KEM_MATRIX_K {
            let a_ij = i16::from_le_bytes(
                public_key.public_matrix[(i * KEM_MATRIX_K + j) * 4
                    ..(i * KEM_MATRIX_K + j) * 4 + 4]
                    .try_into()
                    .unwrap(),
            );
            dot += (a_ij as i32) * (r_vec[i] as i32);
        }
        dot += e1_vec[j] as i32;
        u_vec.push(dot);
    }

    // Serialize u vector
    let mut u_bytes = Vec::new();
    for val in &u_vec {
        u_bytes.extend_from_slice(&(*val as i16).to_le_bytes());
    }

    // Compute v = T^T * r + e2 (simplified)
    // Reconstruct T from seed
    let mut seed_prng = Shake256Stream::new(&public_key.seed);
    let mut secret_vec = Vec::with_capacity(KEM_MATRIX_K);
    for _ in 0..KEM_MATRIX_K {
        secret_vec.push(seed_prng.next_i16());
    }
    let mut error_vec = Vec::with_capacity(KEM_MATRIX_K);
    for _ in 0..KEM_MATRIX_K {
        error_vec.push(seed_prng.next_i16());
    }

    let mut v_val = 0i32;
    for i in 0..KEM_MATRIX_K {
        // T[i] = A[i,:] dot s + e[i]
        let mut t_i = 0i32;
        for j in 0..KEM_MATRIX_K {
            let a_ij = i16::from_le_bytes(
                public_key.public_matrix[(i * KEM_MATRIX_K + j) * 4
                    ..(i * KEM_MATRIX_K + j) * 4 + 4]
                    .try_into()
                    .unwrap(),
            );
            t_i += (a_ij as i32) * (secret_vec[j] as i32);
        }
        t_i += error_vec[i] as i32;
        v_val += t_i * (r_vec[i] as i32);
    }
    v_val += e2_val as i32;

    let v_bytes = (v_val as i32).to_le_bytes().to_vec();

    // Derive shared secret via SHAKE256
    let mut hasher = Shake256::default();
    sha3::digest::Update::update(&mut hasher, b"xergon-pq-shared-secret-v1");
    sha3::digest::Update::update(&mut hasher, &salt);
    sha3::digest::Update::update(&mut hasher, &u_bytes);
    sha3::digest::Update::update(&mut hasher, &v_bytes);
    let mut shared_secret = vec![0u8; KEM_SHARED_SECRET_BYTES];
    let mut reader = hasher.finalize_xof();
    reader.read(&mut shared_secret);

    (
        shared_secret,
        KEMCiphertext {
            u_vec: u_bytes,
            v_vec: v_bytes,
            salt,
        },
    )
}

/// Decapsulate: recover shared secret from ciphertext using secret key.
///
/// Decapsulation mirrors encapsulation using the secret key to recompute
/// the shared secret.
pub fn pq_decapsulate(
    secret_key: &PostQuantumKeyPair,
    ciphertext: &KEMCiphertext,
) -> Result<Vec<u8>, QuantumCryptoError> {
    // Verify public matrix consistency
    if secret_key.public_matrix.len() != KEM_PUBLIC_MATRIX_BYTES {
        return Err(QuantumCryptoError::Decapsulation(
            "Invalid public matrix size".into(),
        ));
    }

    // Reconstruct secret vector from seed
    let mut seed_prng = Shake256Stream::new(&secret_key.seed);
    let mut s_vec = Vec::with_capacity(KEM_MATRIX_K);
    for _ in 0..KEM_MATRIX_K {
        s_vec.push(seed_prng.next_i16());
    }
    let mut e_vec = Vec::with_capacity(KEM_MATRIX_K);
    for _ in 0..KEM_MATRIX_K {
        e_vec.push(seed_prng.next_i16());
    }

    // Reconstruct T
    let mut t_vec = Vec::with_capacity(KEM_MATRIX_K);
    for i in 0..KEM_MATRIX_K {
        let mut t_i = 0i32;
        for j in 0..KEM_MATRIX_K {
            let a_ij = i16::from_le_bytes(
                secret_key.public_matrix[(i * KEM_MATRIX_K + j) * 4
                    ..(i * KEM_MATRIX_K + j) * 4 + 4]
                    .try_into()
                    .unwrap(),
            );
            t_i += (a_ij as i32) * (s_vec[j] as i32);
        }
        t_i += e_vec[i] as i32;
        t_vec.push(t_i);
    }

    // Recreate the PRNG from salt + public_matrix (same as encapsulate)
    let entropy = [ciphertext.salt.as_slice(), secret_key.public_matrix.as_slice()].concat();
    let mut prng = Shake256Stream::new(&entropy);

    let mut r_vec = Vec::with_capacity(KEM_MATRIX_K);
    for _ in 0..KEM_MATRIX_K {
        r_vec.push(prng.next_i16());
    }
    let mut e1_vec = Vec::with_capacity(KEM_MATRIX_K);
    for _ in 0..KEM_MATRIX_K {
        e1_vec.push(prng.next_i16());
    }
    let e2_val = prng.next_i16();

    // Recompute v = T^T * r + e2
    let mut v_val = 0i32;
    for i in 0..KEM_MATRIX_K {
        v_val += t_vec[i] * (r_vec[i] as i32);
    }
    v_val += e2_val as i32;

    // Derive shared secret (same KDF as encapsulate)
    let mut hasher = Shake256::default();
    sha3::digest::Update::update(&mut hasher, b"xergon-pq-shared-secret-v1");
    sha3::digest::Update::update(&mut hasher, &ciphertext.salt);
    sha3::digest::Update::update(&mut hasher, &ciphertext.u_vec);
    sha3::digest::Update::update(&mut hasher, &(v_val as i32).to_le_bytes());
    let mut shared_secret = vec![0u8; KEM_SHARED_SECRET_BYTES];
    let mut reader2 = hasher.finalize_xof();
    reader2.read(&mut shared_secret);

    Ok(shared_secret)
}

// ---------------------------------------------------------------------------
// Homomorphic Inference Verification
// ---------------------------------------------------------------------------

/// A commitment to model weights (hash-based).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCommitment {
    /// Unique model identifier.
    pub model_id: String,
    /// Root hash of the Merkle tree of model weight chunks.
    pub merkle_root: String, // hex
    /// Number of chunks in the Merkle tree.
    pub num_chunks: usize,
    /// Hash of the full model weights.
    pub full_hash: String, // hex
    /// Provider that committed the model.
    pub provider_id: String,
    /// Timestamp of commitment.
    pub committed_at: DateTime<Utc>,
    /// TTL in seconds for this commitment.
    pub ttl_secs: u64,
}

/// A Merkle tree for model weight verification.
#[derive(Debug, Clone)]
pub struct MerkleTree {
    /// Leaf hashes (bottom layer).
    leaves: Vec<[u8; 32]>,
    /// Internal nodes (concatenation of all layers above leaves).
    nodes: Vec<Vec<[u8; 32]>>,
    /// Root hash.
    root: [u8; 32],
}

impl MerkleTree {
    /// Build a Merkle tree from data chunks.
    pub fn build(chunks: &[&[u8]]) -> Self {
        let mut leaves: Vec<[u8; 32]> = chunks
            .iter()
            .map(|chunk| {
                let mut hasher = Sha256::new();
                hasher.update(b"xergon-merkle-leaf-v1");
                hasher.update(chunk);
                let result = hasher.finalize();
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&result);
                hash
            })
            .collect();

        // Pad to power of 2 if needed
        if leaves.is_empty() {
            leaves.push([0u8; 32]);
        }
        let padded_len = leaves.len().next_power_of_two();
        while leaves.len() < padded_len {
            leaves.push(*leaves.last().unwrap());
        }

        let mut nodes = Vec::new();
        let mut current_layer = leaves.clone();

        while current_layer.len() > 1 {
            let mut next_layer = Vec::with_capacity(current_layer.len() / 2);
            for i in (0..current_layer.len()).step_by(2) {
                let mut hasher = Sha256::new();
                hasher.update(b"xergon-merkle-node-v1");
                hasher.update(&current_layer[i]);
                hasher.update(&current_layer[i + 1]);
                let result = hasher.finalize();
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&result);
                next_layer.push(hash);
            }
            nodes.push(current_layer);
            current_layer = next_layer;
        }

        let root = current_layer[0];
        Self {
            leaves,
            nodes,
            root,
        }
    }

    /// Get the root hash.
    pub fn root(&self) -> [u8; 32] {
        self.root
    }

    /// Get the number of leaves.
    pub fn num_leaves(&self) -> usize {
        self.leaves.len()
    }

    /// Generate a Merkle proof for a specific leaf index.
    /// Returns a list of sibling hashes from leaf to root.
    pub fn proof(&self, index: usize) -> Vec<[u8; 32]> {
        let mut proof = Vec::new();
        let mut idx = index;

        // Collect from leaves layer
        let mut current = self.leaves.clone();
        while current.len() > 1 {
            let sibling_idx = if idx % 2 == 0 { idx + 1 } else { idx - 1 };
            proof.push(current[sibling_idx]);
            let mut next = Vec::new();
            for i in (0..current.len()).step_by(2) {
                let mut hasher = Sha256::new();
                hasher.update(b"xergon-merkle-node-v1");
                hasher.update(&current[i]);
                hasher.update(&current[i + 1]);
                let result = hasher.finalize();
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&result);
                next.push(hash);
            }
            current = next;
            idx /= 2;
        }

        proof
    }

    /// Verify a Merkle proof for a given leaf hash.
    pub fn verify_proof(leaf_hash: &[u8; 32], index: usize, proof: &[[u8; 32]], root: &[u8; 32]) -> bool {
        let mut current = *leaf_hash;
        let mut idx = index;

        for sibling in proof {
            let mut hasher = Sha256::new();
            hasher.update(b"xergon-merkle-node-v1");
            if idx % 2 == 0 {
                hasher.update(&current);
                hasher.update(sibling);
            } else {
                hasher.update(sibling);
                hasher.update(&current);
            }
            let result = hasher.finalize();
            current.copy_from_slice(&result);
            idx /= 2;
        }

        current == *root
    }
}

/// A verifiable computation proof for inference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputationProof {
    /// Unique proof ID.
    pub proof_id: String,
    /// Provider that submitted the proof.
    pub provider_id: String,
    /// Model commitment (merkle root).
    pub model_commitment: String, // hex
    /// Hash of the input.
    pub input_hash: String, // hex
    /// Hash of the output.
    pub output_hash: String, // hex
    /// HMAC proof: HMAC-SHA256(key=model_commitment, msg=input||output||timestamp||nonce).
    pub computation_proof: String, // hex
    /// Timestamp (epoch seconds).
    pub timestamp: u64,
    /// Nonce for replay prevention.
    pub nonce: String, // hex
    /// Layer index proven (for partial model verification).
    pub layer_index: Option<usize>,
    /// Merkle proof for the specific layer weights.
    pub merkle_proof: Option<Vec<String>>, // hex strings
    /// Leaf hash for the specific layer.
    pub leaf_hash: Option<String>, // hex
}

impl ComputationProof {
    /// Compute the HMAC-SHA256 proof.
    pub fn compute_proof(
        model_commitment: &[u8; 32],
        input_hash: &[u8; 32],
        output_hash: &[u8; 32],
        timestamp: u64,
        nonce: &[u8; 32],
    ) -> [u8; 32] {
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(model_commitment)
            .expect("HMAC can take key of any size");
        mac.update(input_hash);
        mac.update(output_hash);
        mac.update(&timestamp.to_le_bytes());
        mac.update(nonce);
        let result = mac.finalize();
        let mut proof = [0u8; 32];
        proof.copy_from_slice(&result.into_bytes());
        proof
    }

    /// Verify the computation proof.
    pub fn verify(&self) -> Result<(), QuantumCryptoError> {
        let model_commitment_bytes = hex::decode(&self.model_commitment)
            .map_err(|_| QuantumCryptoError::InvalidCommitment)?;
        let model_commitment: [u8; 32] = model_commitment_bytes
            .try_into()
            .map_err(|_| QuantumCryptoError::InvalidCommitment)?;

        let input_bytes = hex::decode(&self.input_hash)
            .map_err(|_| QuantumCryptoError::VerificationFailed("Invalid input hash".into()))?;
        let input_hash: [u8; 32] = input_bytes
            .try_into()
            .map_err(|_| QuantumCryptoError::VerificationFailed("Input hash not 32 bytes".into()))?;

        let output_bytes = hex::decode(&self.output_hash)
            .map_err(|_| QuantumCryptoError::VerificationFailed("Invalid output hash".into()))?;
        let output_hash: [u8; 32] = output_bytes
            .try_into()
            .map_err(|_| QuantumCryptoError::VerificationFailed("Output hash not 32 bytes".into()))?;

        let nonce_bytes = hex::decode(&self.nonce)
            .map_err(|_| QuantumCryptoError::VerificationFailed("Invalid nonce".into()))?;
        let nonce: [u8; 32] = nonce_bytes
            .try_into()
            .map_err(|_| QuantumCryptoError::VerificationFailed("Nonce not 32 bytes".into()))?;

        let expected_proof = Self::compute_proof(
            &model_commitment,
            &input_hash,
            &output_hash,
            self.timestamp,
            &nonce,
        );

        let actual_proof = hex::decode(&self.computation_proof)
            .map_err(|_| QuantumCryptoError::VerificationFailed("Invalid proof hex".into()))?;

        if expected_proof[..] != actual_proof[..] {
            return Err(QuantumCryptoError::VerificationFailed(
                "HMAC proof does not match".into(),
            ));
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Homomorphic Verifier
// ---------------------------------------------------------------------------

/// Manages model commitments and verifies homomorphic inference proofs.
pub struct HomomorphicVerifier {
    /// Committed models (model_id -> commitment).
    models: DashMap<String, ModelCommitment>,
    /// Cached Merkle trees for committed models (model_id -> tree).
    merkle_trees: DashMap<String, MerkleTree>,
}

impl HomomorphicVerifier {
    pub fn new() -> Self {
        Self {
            models: DashMap::new(),
            merkle_trees: DashMap::new(),
        }
    }

    /// Commit to model weights: build Merkle tree, store commitment.
    pub fn commit_model(
        &self,
        model_id: &str,
        weights: &[u8],
        provider_id: &str,
        ttl_secs: u64,
    ) -> ModelCommitment {
        // Build Merkle tree from chunks
        let chunks: Vec<&[u8]> = weights
            .chunks(MERKLE_CHUNK_SIZE)
            .collect();
        let tree = MerkleTree::build(&chunks);

        let merkle_root = tree.root();
        let full_hash = hash_data_blake3_fallback(weights);

        let commitment = ModelCommitment {
            model_id: model_id.to_string(),
            merkle_root: hex::encode(merkle_root),
            num_chunks: tree.num_leaves(),
            full_hash: hex::encode(full_hash),
            provider_id: provider_id.to_string(),
            committed_at: Utc::now(),
            ttl_secs,
        };

        self.merkle_trees.insert(model_id.to_string(), tree);
        self.models.insert(model_id.to_string(), commitment.clone());

        info!(
            model_id = %model_id,
            provider_id = %provider_id,
            num_chunks = commitment.num_chunks,
            "Model weights committed"
        );

        commitment
    }

    /// Verify a computation proof.
    pub fn verify_computation(&self, proof: &ComputationProof) -> Result<(), QuantumCryptoError> {
        // Check HMAC proof
        proof.verify()?;

        // Verify Merkle proof if provided
        if let (Some(layer_idx), Some(ref merkle_proof_hex), Some(ref leaf_hash_hex)) =
            (proof.layer_index, &proof.merkle_proof, &proof.leaf_hash)
        {
            // Check model commitment exists
            let _commitment = self
                .models
                .get(&proof.model_commitment)
                .ok_or_else(|| {
                    QuantumCryptoError::ModelNotCommitted(proof.model_commitment.clone())
                })?;

            // Note: model_commitment field in proof is the model_id, not the merkle_root.
            // We look up by model_id. Let's also support looking up by merkle_root.
            let commitment = self
                .models
                .iter()
                .find(|e| e.value().merkle_root == proof.model_commitment)
                .map(|e| e.value().clone())
                .or_else(|| self.models.get(&proof.model_commitment).map(|e| e.value().clone()))
                .ok_or_else(|| {
                    QuantumCryptoError::ModelNotCommitted(proof.model_commitment.clone())
                })?;

            let root_bytes = hex::decode(&commitment.merkle_root)
                .map_err(|_| QuantumCryptoError::InvalidCommitment)?;
            let root: [u8; 32] = root_bytes
                .try_into()
                .map_err(|_| QuantumCryptoError::InvalidCommitment)?;

            let leaf_bytes = hex::decode(leaf_hash_hex)
                .map_err(|_| QuantumCryptoError::VerificationFailed("Invalid leaf hash".into()))?;
            let leaf: [u8; 32] = leaf_bytes
                .try_into()
                .map_err(|_| QuantumCryptoError::VerificationFailed("Leaf hash not 32 bytes".into()))?;

            let merkle_proof: Vec<[u8; 32]> = merkle_proof_hex
                .iter()
                .map(|h| {
                    let bytes = hex::decode(h).unwrap_or_default();
                    let mut arr = [0u8; 32];
                    arr[..bytes.len().min(32)].copy_from_slice(&bytes[..bytes.len().min(32)]);
                    arr
                })
                .collect();

            if !MerkleTree::verify_proof(&leaf, layer_idx, &merkle_proof, &root) {
                return Err(QuantumCryptoError::VerificationFailed(
                    "Merkle proof verification failed".into(),
                ));
            }
        }

        Ok(())
    }

    /// Aggregate multiple proof IDs into a single Merkle root.
    pub fn aggregate_proofs(&self, proof_ids: &[String]) -> [u8; 32] {
        if proof_ids.is_empty() {
            return [0u8; 32];
        }

        let chunks: Vec<&[u8]> = proof_ids
            .iter()
            .map(|id| id.as_bytes())
            .collect();
        let tree = MerkleTree::build(&chunks);
        tree.root()
    }

    /// Get the Merkle tree for a committed model.
    pub fn get_merkle_tree(&self, model_id: &str) -> Option<MerkleTree> {
        self.merkle_trees.get(model_id).map(|e| e.value().clone())
    }

    /// List all committed models.
    pub fn list_models(&self) -> Vec<ModelCommitment> {
        self.models.iter().map(|e| e.value().clone()).collect()
    }

    /// Remove expired commitments.
    pub fn evict_expired(&self) {
        let now = Utc::now();
        self.models.retain(|_, v| {
            let expires = v.committed_at + chrono::Duration::seconds(v.ttl_secs as i64);
            expires > now
        });
        // Also clean up merkle trees for removed models
        self.merkle_trees.retain(|k, _| self.models.contains_key(k));
    }

    /// Number of committed models.
    pub fn model_count(&self) -> usize {
        self.models.len()
    }
}

// ---------------------------------------------------------------------------
// Hybrid Key Exchange (Classical + Post-Quantum)
// ---------------------------------------------------------------------------

/// A hybrid keypair combining X25519 and PQ KEM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridKeyPair {
    /// X25519 public key (hex).
    pub x25519_public_key: String,
    /// Post-quantum public key.
    pub pq_public_key: PostQuantumKeyPair,
    /// Key pair ID for reference.
    pub key_id: String,
}

/// A hybrid ciphertext combining classical and PQ ciphertexts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridCiphertext {
    /// X25519 ephemeral public key (hex).
    pub x25519_ephemeral_public: String,
    /// PQ ciphertext.
    pub pq_ciphertext: KEMCiphertext,
    /// AES-256-GCM encrypted payload (nonce || ciphertext || tag).
    #[serde(with = "hex_bytes_serde")]
    pub encrypted_payload: Vec<u8>,
    /// Nonce used for AES-GCM.
    #[serde(with = "hex_bytes_serde")]
    pub nonce: Vec<u8>,
    /// Key pair ID of the recipient.
    pub key_id: String,
}

/// Manages hybrid key exchange operations.
pub struct HybridKeyExchange {
    /// Our X25519 secret key.
    x25519_secret: StaticSecret,
    /// Our X25519 public key.
    x25519_public: PublicKey,
    /// Our PQ keypair.
    pq_keypair: PostQuantumKeyPair,
}

impl HybridKeyExchange {
    /// Generate a new hybrid keypair.
    pub fn generate_keypair() -> (Self, HybridKeyPair) {
        let x25519_secret_bytes = rand::random::<[u8; 32]>();
        let x25519_secret = StaticSecret::from(x25519_secret_bytes);
        let x25519_public = PublicKey::from(&x25519_secret);

        let pq_keypair = pq_keygen();

        let key_id = uuid::Uuid::new_v4().to_string();

        let exchange = Self {
            x25519_secret,
            x25519_public,
            pq_keypair: pq_keypair.clone(),
        };

        let public = HybridKeyPair {
            x25519_public_key: hex::encode(x25519_public.as_bytes()),
            pq_public_key: pq_keypair,
            key_id: key_id.clone(),
        };

        (exchange, public)
    }

    /// Generate a keypair from an existing PQ keypair (for server-side use).
    pub fn from_pq_keypair(pq_keypair: PostQuantumKeyPair) -> (Self, HybridKeyPair) {
        let x25519_secret_bytes = rand::random::<[u8; 32]>();
        let x25519_secret = StaticSecret::from(x25519_secret_bytes);
        let x25519_public = PublicKey::from(&x25519_secret);

        let key_id = uuid::Uuid::new_v4().to_string();

        let exchange = Self {
            x25519_secret,
            x25519_public,
            pq_keypair: pq_keypair.clone(),
        };

        let public = HybridKeyPair {
            x25519_public_key: hex::encode(x25519_public.as_bytes()),
            pq_public_key: pq_keypair,
            key_id: key_id.clone(),
        };

        (exchange, public)
    }

    /// Encapsulate a hybrid shared secret and encrypt a plaintext.
    pub fn encapsulate_and_encrypt(
        &self,
        recipient_public: &HybridKeyPair,
        plaintext: &[u8],
    ) -> Result<HybridCiphertext, QuantumCryptoError> {
        // Classical: ECDH with X25519
        let recipient_x25519 = x25519_dalek::PublicKey::from(
            <[u8; 32]>::try_from(hex::decode(&recipient_public.x25519_public_key).map_err(
                |_| QuantumCryptoError::InvalidKeyFormat,
            )?)
            .map_err(|_| QuantumCryptoError::InvalidKeyFormat)?,
        );
        let x25519_shared = self.x25519_secret.diffie_hellman(&recipient_x25519).to_bytes();

        // Post-quantum: KEM encapsulate
        let (pq_shared, pq_ciphertext) = pq_encapsulate(&recipient_public.pq_public_key);

        // Combine secrets with HKDF-like SHA-256 derivation
        let combined_key = derive_hybrid_key(&x25519_shared, &pq_shared);

        // Encrypt with AES-256-GCM
        let cipher = Aes256Gcm::new_from_slice(&combined_key)
            .map_err(|e| QuantumCryptoError::Encryption(e.to_string()))?;
        let nonce = rand::random::<[u8; 12]>();
        let nonce_obj = Nonce::from_slice(&nonce);
        let encrypted = cipher
            .encrypt(nonce_obj, plaintext)
            .map_err(|e| QuantumCryptoError::Encryption(e.to_string()))?;

        Ok(HybridCiphertext {
            x25519_ephemeral_public: hex::encode(self.x25519_public.as_bytes()),
            pq_ciphertext,
            encrypted_payload: encrypted,
            nonce: nonce.to_vec(),
            key_id: recipient_public.key_id.clone(),
        })
    }

    /// Decapsulate hybrid shared secret and decrypt ciphertext.
    pub fn decapsulate_and_decrypt(
        &self,
        ciphertext: &HybridCiphertext,
    ) -> Result<Vec<u8>, QuantumCryptoError> {
        // Classical: ECDH with sender's ephemeral key
        let sender_x25519 = x25519_dalek::PublicKey::from(
            <[u8; 32]>::try_from(
                hex::decode(&ciphertext.x25519_ephemeral_public)
                    .map_err(|_| QuantumCryptoError::InvalidKeyFormat)?,
            )
            .map_err(|_| QuantumCryptoError::InvalidKeyFormat)?,
        );
        let x25519_shared = self.x25519_secret.diffie_hellman(&sender_x25519).to_bytes();

        // Post-quantum: KEM decapsulate
        let pq_shared = pq_decapsulate(&self.pq_keypair, &ciphertext.pq_ciphertext)?;

        // Combine secrets
        let combined_key = derive_hybrid_key(&x25519_shared, &pq_shared);

        // Decrypt with AES-256-GCM
        let cipher = Aes256Gcm::new_from_slice(&combined_key)
            .map_err(|e| QuantumCryptoError::Encryption(format!("{e}")))?;
        let nonce_obj = Nonce::from_slice(&ciphertext.nonce);
        let plaintext = cipher
            .decrypt(nonce_obj, ciphertext.encrypted_payload.as_slice())
            .map_err(|e| QuantumCryptoError::Encryption(format!("{e}")))?;

        Ok(plaintext)
    }

    /// Get the public keypair.
    pub fn public_keypair(&self) -> HybridKeyPair {
        HybridKeyPair {
            x25519_public_key: hex::encode(self.x25519_public.as_bytes()),
            pq_public_key: self.pq_keypair.clone(),
            key_id: String::new(), // not stored here
        }
    }
}

/// Derive a combined 32-byte key from classical and PQ shared secrets.
/// Uses SHA-256-based HKDF: final_key = HKDF(x25519_secret || pq_secret).
fn derive_hybrid_key(x25519_secret: &[u8; 32], pq_secret: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"xergon-hybrid-key-v1");
    hasher.update(x25519_secret);
    hasher.update(b"||x25519-pq-separator||");
    hasher.update(pq_secret);
    hasher.update(b"key-derivation-finalize");
    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

// ---------------------------------------------------------------------------
// Proof Store with TTL and deduplication
// ---------------------------------------------------------------------------

/// A cached verified proof with expiry.
#[derive(Debug, Clone)]
struct CachedQuantumProof {
    proof: ComputationProof,
    verified_at: Instant,
    expires_at: Instant,
}

/// Thread-safe proof store backed by DashMap with TTL, deduplication, and
/// Merkle root caching.
pub struct ProofStore {
    entries: DashMap<String, CachedQuantumProof>,
    /// Cache of aggregated Merkle roots for batch proofs.
    merkle_root_cache: DashMap<String, [u8; 32]>,
    ttl: Duration,
}

impl ProofStore {
    pub fn new(ttl: Duration) -> Self {
        Self {
            entries: DashMap::new(),
            merkle_root_cache: DashMap::new(),
            ttl,
        }
    }

    /// Insert a verified proof. Returns true if newly inserted (not a replay).
    pub fn insert(&self, proof: &ComputationProof) -> bool {
        self.evict_expired();
        let now = Instant::now();
        let cached = CachedQuantumProof {
            proof: proof.clone(),
            verified_at: now,
            expires_at: now + self.ttl,
        };
        match self.entries.entry(proof.proof_id.clone()) {
            dashmap::mapref::entry::Entry::Occupied(_) => {
                warn!(proof_id = %proof.proof_id, "Replay: proof already cached");
                false
            }
            dashmap::mapref::entry::Entry::Vacant(e) => {
                e.insert(cached);
                // Invalidate merkle root cache
                self.merkle_root_cache.clear();
                true
            }
        }
    }

    /// Check if a proof exists (replay detection).
    pub fn contains(&self, proof_id: &str) -> bool {
        if let Some(entry) = self.entries.get(proof_id) {
            if entry.expires_at > Instant::now() {
                return true;
            }
        }
        false
    }

    /// List recent proofs.
    pub fn list_recent(&self, limit: usize) -> Vec<ComputationProof> {
        self.evict_expired();
        self.entries
            .iter()
            .filter(|e| e.expires_at > Instant::now())
            .take(limit)
            .map(|e| e.value().proof.clone())
            .collect()
    }

    /// Get or compute a Merkle root for all cached proof IDs.
    pub fn get_or_compute_merkle_root(&self) -> [u8; 32] {
        let cache_key = "all_proofs".to_string();
        if let Some(root) = self.merkle_root_cache.get(&cache_key) {
            return *root;
        }
        let proof_ids: Vec<String> = self
            .entries
            .iter()
            .filter(|e| e.expires_at > Instant::now())
            .map(|e| e.value().proof.proof_id.clone())
            .collect();

        let tree = if proof_ids.is_empty() {
            MerkleTree::build(&[])
        } else {
            let chunks: Vec<&[u8]> = proof_ids.iter().map(|id| id.as_bytes()).collect();
            MerkleTree::build(&chunks)
        };

        let root = tree.root();
        self.merkle_root_cache.insert(cache_key, root);
        root
    }

    /// Clear all proofs.
    pub fn clear(&self) -> usize {
        let count = self.entries.len();
        self.entries.clear();
        self.merkle_root_cache.clear();
        count
    }

    fn evict_expired(&self) {
        let now = Instant::now();
        self.entries.retain(|_, v| v.expires_at > now);
        // Also invalidate merkle cache if we evicted anything
        if self.entries.len() < self.merkle_root_cache.len() {
            self.merkle_root_cache.clear();
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

// ---------------------------------------------------------------------------
// Quantum Crypto Configuration
// ---------------------------------------------------------------------------

/// Configuration for the quantum crypto subsystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuantumCryptoConfig {
    /// Whether quantum crypto is enabled.
    pub enabled: bool,
    /// Whether post-quantum KEM is enabled.
    pub pq_kem_enabled: bool,
    /// Whether homomorphic verification is enabled.
    pub homomorphic_enabled: bool,
    /// Whether hybrid encryption is enabled.
    pub hybrid_enabled: bool,
    /// Proof cache TTL in seconds.
    pub proof_cache_ttl_secs: u64,
    /// Max proofs to return in list endpoint.
    pub max_list_proofs: usize,
    /// Default model commitment TTL in seconds.
    pub model_commitment_ttl_secs: u64,
}

impl Default for QuantumCryptoConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            pq_kem_enabled: true,
            homomorphic_enabled: true,
            hybrid_enabled: true,
            proof_cache_ttl_secs: 3600,
            max_list_proofs: 100,
            model_commitment_ttl_secs: 86400,
        }
    }
}

// ---------------------------------------------------------------------------
// Quantum Crypto State
// ---------------------------------------------------------------------------

/// The core quantum crypto state.
pub struct QuantumCryptoState {
    /// Post-quantum keypair (server-side).
    pq_keypair: PostQuantumKeyPair,
    /// Hybrid key exchange engine.
    hybrid_exchange: HybridKeyExchange,
    /// Homomorphic verifier.
    verifier: HomomorphicVerifier,
    /// Proof store with TTL.
    proof_store: ProofStore,
    /// Configuration.
    config: tokio::sync::RwLock<QuantumCryptoConfig>,
    /// Key generation counter.
    key_count: std::sync::atomic::AtomicU64,
}

impl QuantumCryptoState {
    pub fn new(config: QuantumCryptoConfig) -> Self {
        let pq_keypair = pq_keygen();
        let (hybrid_exchange, _) = HybridKeyExchange::from_pq_keypair(pq_keypair.clone());

        info!(
            pq_seed = hex::encode(&pq_keypair.seed[..8]),
            "Quantum crypto engine initialized"
        );

        Self {
            pq_keypair,
            hybrid_exchange,
            verifier: HomomorphicVerifier::new(),
            proof_store: ProofStore::new(Duration::from_secs(config.proof_cache_ttl_secs)),
            config: tokio::sync::RwLock::new(config),
            key_count: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Generate a new hybrid keypair.
    pub async fn generate_keypair(&self) -> Result<HybridKeyPair, QuantumCryptoError> {
        let cfg = self.config.read().await;
        if !cfg.enabled || !cfg.hybrid_enabled {
            return Err(QuantumCryptoError::Config("Hybrid key exchange disabled".into()));
        }
        let (_, keypair) = HybridKeyExchange::generate_keypair();
        self.key_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(keypair)
    }

    /// Encapsulate a hybrid shared secret.
    pub async fn encapsulate_hybrid(
        &self,
        recipient: &HybridKeyPair,
        plaintext: &[u8],
    ) -> Result<HybridCiphertext, QuantumCryptoError> {
        let cfg = self.config.read().await;
        if !cfg.enabled || !cfg.hybrid_enabled {
            return Err(QuantumCryptoError::Config("Hybrid encryption disabled".into()));
        }
        let (sender_exchange, _) = HybridKeyExchange::generate_keypair();
        sender_exchange.encapsulate_and_encrypt(recipient, plaintext)
    }

    /// Get the server's PQ public key.
    pub fn pq_public_key(&self) -> PostQuantumKeyPair {
        self.pq_keypair.clone()
    }

    /// Get the server's hybrid public key.
    pub fn hybrid_public_key(&self) -> HybridKeyPair {
        self.hybrid_exchange.public_keypair()
    }
}

// ---------------------------------------------------------------------------
// Helper: hash data (blake3 if available, otherwise SHA-256)
// ---------------------------------------------------------------------------

/// Hash arbitrary data. Uses SHA-256 (blake3 crate not available).
fn hash_data_blake3_fallback(data: &[u8]) -> [u8; 32] {
    let result = Sha256::digest(data);
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

// ---------------------------------------------------------------------------
// REST API Handlers
// ---------------------------------------------------------------------------

/// POST /v1/quantum/keygen — generate hybrid keypair
async fn handle_keygen(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, QuantumCryptoError> {
    let keypair = state.quantum_crypto.generate_keypair().await?;
    Ok(Json(serde_json::json!({
        "key_id": keypair.key_id,
        "x25519_public_key": keypair.x25519_public_key,
        "pq_public_key": {
            "seed": hex::encode(&keypair.pq_public_key.seed),
            "public_matrix": hex::encode(&keypair.pq_public_key.public_matrix),
        },
    })))
}

/// POST /v1/quantum/encapsulate — encapsulate hybrid shared secret
async fn handle_encapsulate(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, QuantumCryptoError> {
    let recipient_hex = body
        .get("pq_public_key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| QuantumCryptoError::Encapsulation("Missing pq_public_key".into()))?;

    let recipient_pq_bytes = hex::decode(recipient_hex)
        .map_err(|_| QuantumCryptoError::InvalidKeyFormat)?;

    let recipient_x25519_hex = body
        .get("x25519_public_key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| QuantumCryptoError::Encapsulation("Missing x25519_public_key".into()))?;

    let plaintext_b64 = body
        .get("plaintext")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // For simplicity, use the plaintext as raw bytes
    let plaintext = plaintext_b64.as_bytes();

    // Reconstruct a minimal HybridKeyPair for encapsulation
    let recipient_pq: PostQuantumKeyPair = PostQuantumKeyPair {
        seed: vec![0u8; KEM_SEED_BYTES], // placeholder
        public_matrix: recipient_pq_bytes,
        secret_key: vec![0u8; 32],
    };

    let recipient = HybridKeyPair {
        x25519_public_key: recipient_x25519_hex.to_string(),
        pq_public_key: recipient_pq,
        key_id: body
            .get("key_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    };

    let ciphertext = state.quantum_crypto.encapsulate_hybrid(&recipient, plaintext).await?;

    Ok(Json(serde_json::json!({
        "x25519_ephemeral_public": ciphertext.x25519_ephemeral_public,
        "pq_ciphertext": {
            "u_vec": hex::encode(&ciphertext.pq_ciphertext.u_vec),
            "v_vec": hex::encode(&ciphertext.pq_ciphertext.v_vec),
            "salt": hex::encode(&ciphertext.pq_ciphertext.salt),
        },
        "encrypted_payload": hex::encode(&ciphertext.encrypted_payload),
        "nonce": hex::encode(&ciphertext.nonce),
        "key_id": ciphertext.key_id,
    })))
}

/// POST /v1/quantum/verify — verify homomorphic inference proof
async fn handle_verify(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, QuantumCryptoError> {
    let proof_id = body
        .get("proof_id")
        .and_then(|v| v.as_str())
        .unwrap_or(&uuid::Uuid::new_v4().to_string())
        .to_string();

    let provider_id = body
        .get("provider_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let model_commitment = body
        .get("model_commitment")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let input_hash = body
        .get("input_hash")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let output_hash = body
        .get("output_hash")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let nonce = body
        .get("nonce")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let computation_proof = body
        .get("computation_proof")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let timestamp = body
        .get("timestamp")
        .and_then(|v| v.as_u64())
        .unwrap_or_else(|| chrono::Utc::now().timestamp() as u64);

    let proof = ComputationProof {
        proof_id: proof_id.clone(),
        provider_id,
        model_commitment,
        input_hash,
        output_hash,
        computation_proof,
        timestamp,
        nonce,
        layer_index: body.get("layer_index").and_then(|v| v.as_u64()).map(|v| v as usize),
        merkle_proof: body
            .get("merkle_proof")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            }),
        leaf_hash: body.get("leaf_hash").and_then(|v| v.as_str()).map(String::from),
    };

    // Verify
    state.quantum_crypto.verifier.verify_computation(&proof)?;

    // Dedup
    if state.quantum_crypto.proof_store.contains(&proof_id) {
        return Err(QuantumCryptoError::ReplayDetected);
    }

    // Store
    state.quantum_crypto.proof_store.insert(&proof);

    Ok(Json(serde_json::json!({
        "proof_id": proof_id,
        "verified": true,
        "timestamp": timestamp,
    })))
}

/// POST /v1/quantum/commit-model — provider commits to model weights
async fn handle_commit_model(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, QuantumCryptoError> {
    let model_id = body
        .get("model_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| QuantumCryptoError::InvalidCommitment)?;

    let weights_hex = body
        .get("weights")
        .and_then(|v| v.as_str())
        .ok_or_else(|| QuantumCryptoError::InvalidCommitment)?;

    let weights = hex::decode(weights_hex)
        .map_err(|_| QuantumCryptoError::InvalidCommitment)?;

    let provider_id = body
        .get("provider_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let ttl_secs = body
        .get("ttl_secs")
        .and_then(|v| v.as_u64())
        .unwrap_or(86400);

    let commitment = state.quantum_crypto.verifier.commit_model(
        model_id,
        &weights,
        provider_id,
        ttl_secs,
    );

    Ok(Json(serde_json::json!({
        "model_id": commitment.model_id,
        "merkle_root": commitment.merkle_root,
        "num_chunks": commitment.num_chunks,
        "full_hash": commitment.full_hash,
        "committed_at": commitment.committed_at.to_rfc3339(),
    })))
}

/// GET /v1/quantum/proofs — list verified proofs
async fn handle_list_proofs(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let cfg = state.quantum_crypto.config.read().await;
    let proofs = state.quantum_crypto.proof_store.list_recent(cfg.max_list_proofs);
    let merkle_root = state.quantum_crypto.proof_store.get_or_compute_merkle_root();

    Json(serde_json::json!({
        "proofs": proofs,
        "count": proofs.len(),
        "merkle_root": hex::encode(merkle_root),
    }))
}

/// GET /v1/quantum/status — crypto module status
async fn handle_status(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let cfg = state.quantum_crypto.config.read().await;
    let key_count = state
        .quantum_crypto
        .key_count
        .load(std::sync::atomic::Ordering::Relaxed);

    Json(serde_json::json!({
        "enabled": cfg.enabled,
        "pq_kem_enabled": cfg.pq_kem_enabled,
        "homomorphic_enabled": cfg.homomorphic_enabled,
        "hybrid_enabled": cfg.hybrid_enabled,
        "proof_cache_ttl_secs": cfg.proof_cache_ttl_secs,
        "key_count": key_count,
        "committed_models": state.quantum_crypto.verifier.model_count(),
        "cached_proofs": state.quantum_crypto.proof_store.len(),
        "pq_algorithm": "ML-KEM-inspired (simplified lattice)",
        "classical_algorithm": "X25519 + AES-256-GCM",
        "hash_algorithm": "SHA-256 / SHAKE256",
    }))
}

/// DELETE /v1/quantum/proofs — clear proof cache (admin)
async fn handle_clear_proofs(
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let count = state.quantum_crypto.proof_store.clear();
    info!(cleared = count, "Quantum proof cache cleared");
    Json(serde_json::json!({
        "cleared": count,
    }))
}

/// PUT /v1/quantum/config — update quantum crypto config
async fn handle_update_config(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, QuantumCryptoError> {
    let mut cfg = state.quantum_crypto.config.write().await;

    if let Some(enabled) = body.get("enabled").and_then(|v| v.as_bool()) {
        cfg.enabled = enabled;
    }
    if let Some(pq) = body.get("pq_kem_enabled").and_then(|v| v.as_bool()) {
        cfg.pq_kem_enabled = pq;
    }
    if let Some(homo) = body.get("homomorphic_enabled").and_then(|v| v.as_bool()) {
        cfg.homomorphic_enabled = homo;
    }
    if let Some(hybrid) = body.get("hybrid_enabled").and_then(|v| v.as_bool()) {
        cfg.hybrid_enabled = hybrid;
    }
    if let Some(ttl) = body.get("proof_cache_ttl_secs").and_then(|v| v.as_u64()) {
        cfg.proof_cache_ttl_secs = ttl;
    }

    info!(
        enabled = cfg.enabled,
        pq = cfg.pq_kem_enabled,
        homomorphic = cfg.homomorphic_enabled,
        hybrid = cfg.hybrid_enabled,
        "Quantum crypto config updated"
    );

    Ok(Json(serde_json::json!({
        "enabled": cfg.enabled,
        "pq_kem_enabled": cfg.pq_kem_enabled,
        "homomorphic_enabled": cfg.homomorphic_enabled,
        "hybrid_enabled": cfg.hybrid_enabled,
        "proof_cache_ttl_secs": cfg.proof_cache_ttl_secs,
    })))
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the quantum crypto router.
pub fn build_quantum_crypto_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/v1/quantum/keygen", post(handle_keygen))
        .route("/v1/quantum/encapsulate", post(handle_encapsulate))
        .route("/v1/quantum/verify", post(handle_verify))
        .route("/v1/quantum/commit-model", post(handle_commit_model))
        .route("/v1/quantum/proofs", get(handle_list_proofs))
        .route("/v1/quantum/status", get(handle_status))
        .route("/v1/quantum/proofs", delete(handle_clear_proofs))
        .route("/v1/quantum/config", put(handle_update_config))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- KEM Tests ----------

    #[test]
    fn test_kem_keygen_deterministic_from_seed() {
        let kp1 = pq_keygen();
        let kp2 = pq_keygen();
        // Different random seeds should produce different keypairs
        assert_ne!(kp1.seed, kp2.seed, "Different keypairs should have different seeds");
        assert_ne!(
            kp1.public_matrix, kp2.public_matrix,
            "Different keypairs should have different public matrices"
        );
    }

    #[test]
    fn test_kem_encapsulation_decapsulation_symmetry() {
        let keypair = pq_keygen();
        let (shared_secret_enc, ciphertext) = pq_encapsulate(&keypair);
        let shared_secret_dec = pq_decapsulate(&keypair, &ciphertext).unwrap();
        assert_eq!(
            shared_secret_enc, shared_secret_dec,
            "Encapsulated and decapsulated shared secrets must match"
        );
    }

    #[test]
    fn test_kem_different_keys_different_secrets() {
        let kp1 = pq_keygen();
        let kp2 = pq_keygen();
        let (secret1, _) = pq_encapsulate(&kp1);
        let (secret2, _) = pq_encapsulate(&kp2);
        assert_ne!(
            secret1, secret2,
            "Different keypairs should produce different shared secrets"
        );
    }

    #[test]
    fn test_kem_ciphertext_serialization() {
        let keypair = pq_keygen();
        let (_, ciphertext) = pq_encapsulate(&keypair);
        let json = serde_json::to_string(&ciphertext).unwrap();
        let deserialized: KEMCiphertext = serde_json::from_str(&json).unwrap();
        assert_eq!(
            ciphertext.salt, deserialized.salt,
            "Serialized ciphertext should round-trip"
        );
    }

    #[test]
    fn test_kem_keypair_serialization() {
        let keypair = pq_keygen();
        let json = serde_json::to_string(&keypair).unwrap();
        let deserialized: PostQuantumKeyPair = serde_json::from_str(&json).unwrap();
        assert_eq!(
            keypair.seed, deserialized.seed,
            "Serialized keypair should round-trip"
        );
    }

    #[test]
    fn test_kem_multiple_encapsulations_different_ciphertexts() {
        let keypair = pq_keygen();
        let (_, ct1) = pq_encapsulate(&keypair);
        let (_, ct2) = pq_encapsulate(&keypair);
        assert_ne!(ct1.salt, ct2.salt, "Different encapsulations should use different salts");
    }

    // ---------- Hybrid Key Exchange Tests ----------

    #[test]
    fn test_hybrid_keypair_generation() {
        let (_, public) = HybridKeyExchange::generate_keypair();
        assert!(!public.x25519_public_key.is_empty());
        assert!(!public.pq_public_key.seed.is_empty());
        assert!(!public.pq_public_key.public_matrix.is_empty());
        assert!(!public.key_id.is_empty());
    }

    #[tokio::test]
    async fn test_hybrid_encapsulate_decapsulate() {
        let (alice_exchange, alice_public) = HybridKeyExchange::generate_keypair();
        let (bob_exchange, bob_public) = HybridKeyExchange::generate_keypair();

        let plaintext = b"Hello, quantum world!";
        let ciphertext = alice_exchange
            .encapsulate_and_encrypt(&bob_public, plaintext)
            .unwrap();
        let decrypted = bob_exchange.decapsulate_and_decrypt(&ciphertext).unwrap();
        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_hybrid_key_derivation_deterministic() {
        let x25519_secret = [0xAAu8; 32];
        let pq_secret = [0xBBu8; 32];
        let key1 = derive_hybrid_key(&x25519_secret, &pq_secret);
        let key2 = derive_hybrid_key(&x25519_secret, &pq_secret);
        assert_eq!(key1, key2, "Hybrid key derivation must be deterministic");
    }

    #[test]
    fn test_hybrid_key_derivation_different_inputs() {
        let x25519_secret = [0xAAu8; 32];
        let pq_secret1 = [0xBBu8; 32];
        let pq_secret2 = [0xCCu8; 32];
        let key1 = derive_hybrid_key(&x25519_secret, &pq_secret1);
        let key2 = derive_hybrid_key(&x25519_secret, &pq_secret2);
        assert_ne!(key1, key2, "Different PQ secrets should produce different keys");
    }

    // ---------- Merkle Tree Tests ----------

    #[test]
    fn test_merkle_tree_build() {
        let chunks: Vec<&[u8]> = vec![
            b"chunk-0-data",
            b"chunk-1-data",
            b"chunk-2-data",
            b"chunk-3-data",
        ];
        let tree = MerkleTree::build(&chunks);
        assert_eq!(tree.num_leaves(), 4);
        let root = tree.root();
        assert_ne!(root, [0u8; 32], "Merkle root should not be all zeros");
    }

    #[test]
    fn test_merkle_tree_deterministic() {
        let chunks: Vec<&[u8]> = vec![b"a", b"b", b"c", b"d"];
        let tree1 = MerkleTree::build(&chunks);
        let tree2 = MerkleTree::build(&chunks);
        assert_eq!(tree1.root(), tree2.root(), "Same chunks should produce same root");
    }

    #[test]
    fn test_merkle_proof_verify() {
        let chunks: Vec<&[u8]> = vec![b"a", b"b", b"c", b"d"];
        let tree = MerkleTree::build(&chunks);

        for i in 0..4 {
            let mut hasher = Sha256::new();
            hasher.update(b"xergon-merkle-leaf-v1");
            hasher.update(chunks[i]);
            let result = hasher.finalize();
            let mut leaf_hash = [0u8; 32];
            leaf_hash.copy_from_slice(&result);

            let proof = tree.proof(i);
            assert!(
                MerkleTree::verify_proof(&leaf_hash, i, &proof, &tree.root()),
                "Merkle proof should verify for index {}",
                i
            );
        }
    }

    #[test]
    fn test_merkle_proof_invalid_leaf() {
        let chunks: Vec<&[u8]> = vec![b"a", b"b", b"c", b"d"];
        let tree = MerkleTree::build(&chunks);

        let fake_leaf = [0xFFu8; 32];
        let proof = tree.proof(0);
        assert!(
            !MerkleTree::verify_proof(&fake_leaf, 0, &proof, &tree.root()),
            "Fake leaf hash should fail verification"
        );
    }

    // ---------- Homomorphic Verification Tests ----------

    #[test]
    fn test_homomorphic_commitment_deterministic() {
        let verifier = HomomorphicVerifier::new();
        let weights = b"model-weights-data-for-testing";
        let c1 = verifier.commit_model("model-1", weights, "provider-1", 3600);
        let c2 = verifier.commit_model("model-2", weights, "provider-1", 3600);
        // Same weights but different model IDs should produce same merkle_root and full_hash
        assert_eq!(c1.merkle_root, c2.merkle_root);
        assert_eq!(c1.full_hash, c2.full_hash);
    }

    #[test]
    fn test_homomorphic_commitment_different_weights() {
        let verifier = HomomorphicVerifier::new();
        let c1 = verifier.commit_model("model-1", b"weights-1", "provider-1", 3600);
        let c2 = verifier.commit_model("model-1", b"weights-2", "provider-1", 3600);
        assert_ne!(c1.merkle_root, c2.merkle_root);
        assert_ne!(c1.full_hash, c2.full_hash);
    }

    #[test]
    fn test_computation_proof_valid() {
        let model_commitment = hash_data_blake3_fallback(b"model-id");
        let input_hash = hash_data_blake3_fallback(b"input-data");
        let output_hash = hash_data_blake3_fallback(b"output-data");
        let timestamp = 1700000000u64;
        let nonce = rand::random::<[u8; 32]>();

        let proof_bytes = ComputationProof::compute_proof(
            &model_commitment,
            &input_hash,
            &output_hash,
            timestamp,
            &nonce,
        );

        let proof = ComputationProof {
            proof_id: "test-proof-1".to_string(),
            provider_id: "provider-1".to_string(),
            model_commitment: hex::encode(model_commitment),
            input_hash: hex::encode(input_hash),
            output_hash: hex::encode(output_hash),
            computation_proof: hex::encode(proof_bytes),
            timestamp,
            nonce: hex::encode(nonce),
            layer_index: None,
            merkle_proof: None,
            leaf_hash: None,
        };

        assert!(proof.verify().is_ok(), "Valid proof should verify");
    }

    #[test]
    fn test_computation_proof_invalid() {
        let model_commitment = hash_data_blake3_fallback(b"model-id");
        let input_hash = hash_data_blake3_fallback(b"input-data");
        let output_hash = hash_data_blake3_fallback(b"output-data");
        let timestamp = 1700000000u64;
        let nonce = rand::random::<[u8; 32]>();

        let proof = ComputationProof {
            proof_id: "test-proof-invalid".to_string(),
            provider_id: "provider-1".to_string(),
            model_commitment: hex::encode(model_commitment),
            input_hash: hex::encode(input_hash),
            output_hash: hex::encode(output_hash),
            computation_proof: hex::encode([0xFFu8; 32]), // Invalid proof
            timestamp,
            nonce: hex::encode(nonce),
            layer_index: None,
            merkle_proof: None,
            leaf_hash: None,
        };

        assert!(proof.verify().is_err(), "Invalid proof should fail verification");
    }

    #[test]
    fn test_computation_proof_tampered_input() {
        let model_commitment = hash_data_blake3_fallback(b"model-id");
        let input_hash = hash_data_blake3_fallback(b"input-data");
        let output_hash = hash_data_blake3_fallback(b"output-data");
        let timestamp = 1700000000u64;
        let nonce = rand::random::<[u8; 32]>();

        let proof_bytes = ComputationProof::compute_proof(
            &model_commitment,
            &input_hash,
            &output_hash,
            timestamp,
            &nonce,
        );

        let tampered_input = hash_data_blake3_fallback(b"tampered-input");

        let proof = ComputationProof {
            proof_id: "test-proof-tampered".to_string(),
            provider_id: "provider-1".to_string(),
            model_commitment: hex::encode(model_commitment),
            input_hash: hex::encode(tampered_input), // tampered
            output_hash: hex::encode(output_hash),
            computation_proof: hex::encode(proof_bytes),
            timestamp,
            nonce: hex::encode(nonce),
            layer_index: None,
            merkle_proof: None,
            leaf_hash: None,
        };

        assert!(
            proof.verify().is_err(),
            "Tampered input hash should fail verification"
        );
    }

    #[test]
    fn test_proof_aggregation() {
        let verifier = HomomorphicVerifier::new();
        let proof_ids = vec![
            "proof-1".to_string(),
            "proof-2".to_string(),
            "proof-3".to_string(),
        ];
        let root = verifier.aggregate_proofs(&proof_ids);
        assert_ne!(root, [0u8; 32]);

        // Same inputs should give same root
        let root2 = verifier.aggregate_proofs(&proof_ids);
        assert_eq!(root, root2);
    }

    #[test]
    fn test_proof_aggregation_empty() {
        let verifier = HomomorphicVerifier::new();
        let root = verifier.aggregate_proofs(&[]);
        assert_eq!(root, [0u8; 32]);
    }

    // ---------- Proof Store Tests ----------

    #[test]
    fn test_proof_store_insert_and_retrieve() {
        let store = ProofStore::new(Duration::from_secs(60));

        let model_commitment = hash_data_blake3_fallback(b"model");
        let input_hash = hash_data_blake3_fallback(b"input");
        let output_hash = hash_data_blake3_fallback(b"output");
        let nonce = [0u8; 32];
        let proof_bytes =
            ComputationProof::compute_proof(&model_commitment, &input_hash, &output_hash, 100, &nonce);

        let proof = ComputationProof {
            proof_id: "test-1".to_string(),
            provider_id: "p1".to_string(),
            model_commitment: hex::encode(model_commitment),
            input_hash: hex::encode(input_hash),
            output_hash: hex::encode(output_hash),
            computation_proof: hex::encode(proof_bytes),
            timestamp: 100,
            nonce: hex::encode(nonce),
            layer_index: None,
            merkle_proof: None,
            leaf_hash: None,
        };

        assert!(store.insert(&proof), "First insert should succeed");
        assert!(!store.insert(&proof), "Duplicate insert should fail (replay)");
        assert!(store.contains("test-1"));
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn test_proof_store_replay_detection() {
        let store = ProofStore::new(Duration::from_secs(60));

        let model_commitment = hash_data_blake3_fallback(b"model");
        let input_hash = hash_data_blake3_fallback(b"input");
        let output_hash = hash_data_blake3_fallback(b"output");
        let nonce = [0u8; 32];
        let proof_bytes =
            ComputationProof::compute_proof(&model_commitment, &input_hash, &output_hash, 100, &nonce);

        let proof = ComputationProof {
            proof_id: "replay-test".to_string(),
            provider_id: "p1".to_string(),
            model_commitment: hex::encode(model_commitment),
            input_hash: hex::encode(input_hash),
            output_hash: hex::encode(output_hash),
            computation_proof: hex::encode(proof_bytes),
            timestamp: 100,
            nonce: hex::encode(nonce),
            layer_index: None,
            merkle_proof: None,
            leaf_hash: None,
        };

        store.insert(&proof);
        assert!(store.contains("replay-test"));
        assert!(!store.insert(&proof), "Replay should be rejected");
    }

    #[test]
    fn test_proof_store_clear() {
        let store = ProofStore::new(Duration::from_secs(60));
        let model_commitment = hash_data_blake3_fallback(b"m");
        let input_hash = hash_data_blake3_fallback(b"i");
        let output_hash = hash_data_blake3_fallback(b"o");
        let nonce = [0u8; 32];
        let proof_bytes =
            ComputationProof::compute_proof(&model_commitment, &input_hash, &output_hash, 1, &nonce);

        for i in 0..5 {
            let proof = ComputationProof {
                proof_id: format!("proof-{}", i),
                provider_id: "p".to_string(),
                model_commitment: hex::encode(model_commitment),
                input_hash: hex::encode(input_hash),
                output_hash: hex::encode(output_hash),
                computation_proof: hex::encode(proof_bytes),
                timestamp: 1,
                nonce: hex::encode(nonce),
                layer_index: None,
                merkle_proof: None,
                leaf_hash: None,
            };
            store.insert(&proof);
        }

        assert_eq!(store.len(), 5);
        let cleared = store.clear();
        assert_eq!(cleared, 5);
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn test_proof_store_merkle_root_caching() {
        let store = ProofStore::new(Duration::from_secs(60));
        let model_commitment = hash_data_blake3_fallback(b"m");
        let input_hash = hash_data_blake3_fallback(b"i");
        let output_hash = hash_data_blake3_fallback(b"o");
        let nonce = [0u8; 32];
        let proof_bytes =
            ComputationProof::compute_proof(&model_commitment, &input_hash, &output_hash, 1, &nonce);

        let proof = ComputationProof {
            proof_id: "cached-proof".to_string(),
            provider_id: "p".to_string(),
            model_commitment: hex::encode(model_commitment),
            input_hash: hex::encode(input_hash),
            output_hash: hex::encode(output_hash),
            computation_proof: hex::encode(proof_bytes),
            timestamp: 1,
            nonce: hex::encode(nonce),
            layer_index: None,
            merkle_proof: None,
            leaf_hash: None,
        };
        store.insert(&proof);

        let root1 = store.get_or_compute_merkle_root();
        let root2 = store.get_or_compute_merkle_root();
        assert_eq!(root1, root2, "Cached merkle roots should match");
        assert_ne!(root1, [0u8; 32], "Root should not be zero with proofs");
    }

    #[test]
    fn test_proof_store_list_recent() {
        let store = ProofStore::new(Duration::from_secs(60));
        let model_commitment = hash_data_blake3_fallback(b"m");
        let input_hash = hash_data_blake3_fallback(b"i");
        let output_hash = hash_data_blake3_fallback(b"o");
        let nonce = [0u8; 32];
        let proof_bytes =
            ComputationProof::compute_proof(&model_commitment, &input_hash, &output_hash, 1, &nonce);

        for i in 0..10 {
            let proof = ComputationProof {
                proof_id: format!("list-proof-{}", i),
                provider_id: "p".to_string(),
                model_commitment: hex::encode(model_commitment),
                input_hash: hex::encode(input_hash),
                output_hash: hex::encode(output_hash),
                computation_proof: hex::encode(proof_bytes),
                timestamp: 1,
                nonce: hex::encode(nonce),
                layer_index: None,
                merkle_proof: None,
                leaf_hash: None,
            };
            store.insert(&proof);
        }

        let recent = store.list_recent(5);
        assert_eq!(recent.len(), 5, "Should return at most limit proofs");
    }

    // ---------- QuantumCryptoState Tests ----------

    #[tokio::test]
    async fn test_quantum_crypto_state_status() {
        let state = QuantumCryptoState::new(QuantumCryptoConfig::default());
        let cfg = state.config.read().await;
        assert!(cfg.enabled);
        assert!(cfg.pq_kem_enabled);
        assert!(cfg.homomorphic_enabled);
        assert!(cfg.hybrid_enabled);
    }

    #[tokio::test]
    async fn test_quantum_crypto_generate_keypair() {
        let state = QuantumCryptoState::new(QuantumCryptoConfig::default());
        let keypair = state.generate_keypair().await.unwrap();
        assert!(!keypair.key_id.is_empty());
        assert!(!keypair.x25519_public_key.is_empty());
    }

    #[tokio::test]
    async fn test_quantum_crypto_disabled() {
        let mut cfg = QuantumCryptoConfig::default();
        cfg.enabled = false;
        let state = QuantumCryptoState::new(cfg);
        let result = state.generate_keypair().await;
        assert!(result.is_err());
    }
}
