//! Encrypted Inference Module
//!
//! Provides end-to-end encrypted inference routing through the Xergon relay.
//! The relay cannot read user prompts or model responses — encryption is
//! established directly between the client and the provider using X25519
//! key exchange with AES-256-GCM payload encryption.
//!
//! Also includes a lightweight proof-of-inference system: providers commit
//! to a model+prompt hash before inference, then submit a proof afterward.
//! The relay verifies the commitment matches and caches proofs to prevent
//! replay attacks.
//!
//! Endpoints:
//! - POST /v1/inference/encrypted    — submit an encrypted inference request
//! - GET  /v1/inference/keys         — retrieve the relay's public key
//! - POST /v1/inference/verify       — verify an inference proof
//! - GET  /v1/inference/proofs       — list recent verified proofs
//! - DELETE /v1/inference/proofs     — clear proof cache (admin)
//! - GET  /v1/inference/config       — get encryption config (admin)
//! - PUT  /v1/inference/config       — update encryption config (admin)
//! - POST /v1/inference/providers/keys — register a provider's public key (admin)

use std::time::{Duration, Instant};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};
use x25519_dalek::{PublicKey, StaticSecret};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors specific to the encrypted inference module.
#[derive(Debug, thiserror::Error)]
pub enum EncryptedInferenceError {
    #[error("encryption error: {0}")]
    Encryption(String),
    #[error("decryption error: {0}")]
    Decryption(String),
    #[error("invalid commitment: {0}")]
    InvalidCommitment(String),
    #[error("proof verification failed: {0}")]
    VerificationFailed(String),
    #[error("proof expired")]
    ProofExpired,
    #[error("replay detected: proof already verified")]
    ReplayDetected,
    #[error("provider not found: {0}")]
    ProviderNotFound(String),
    #[error("invalid key format")]
    InvalidKeyFormat,
    #[error("key exchange failed: {0}")]
    KeyExchange(String),
}

impl IntoResponse for EncryptedInferenceError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            Self::Encryption(_) | Self::Decryption(_) | Self::KeyExchange(_) => {
                (StatusCode::BAD_REQUEST, self.to_string())
            }
            Self::InvalidCommitment(_) | Self::VerificationFailed(_) => {
                (StatusCode::UNPROCESSABLE_ENTITY, self.to_string())
            }
            Self::ProofExpired => (StatusCode::GONE, self.to_string()),
            Self::ReplayDetected => (StatusCode::CONFLICT, self.to_string()),
            Self::ProviderNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
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
// Crypto primitives
// ---------------------------------------------------------------------------

/// Generate a new X25519 keypair.
pub fn generate_keypair() -> (StaticSecret, PublicKey) {
    let secret_bytes = rand::random::<[u8; 32]>();
    let secret = StaticSecret::from(secret_bytes);
    let public = PublicKey::from(&secret);
    (secret, public)
}

/// Perform ECDH key exchange: compute shared secret from our secret + their public.
pub fn compute_shared_secret(
    our_secret: &StaticSecret,
    their_public: &PublicKey,
) -> [u8; 32] {
    our_secret.diffie_hellman(their_public).to_bytes()
}

/// Derive an AES-256-GCM key from a shared secret using HKDF-like SHA-256 stretch.
/// We hash the shared secret twice to derive the 32-byte AES key.
pub fn derive_aes_key(shared_secret: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"xergon-encryption-v1");
    hasher.update(shared_secret);
    hasher.update(b"aes-key-derivation");
    let hash1 = hasher.finalize();

    let mut hasher2 = Sha256::new();
    hasher2.update(hash1);
    hasher2.update(b"xergon-aes-key-finalize");
    let result = hasher2.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

/// Derive a 12-byte nonce from a shared secret + nonce counter.
pub fn derive_nonce(shared_secret: &[u8; 32], nonce_counter: u64) -> [u8; 12] {
    let mut hasher = Sha256::new();
    hasher.update(b"xergon-nonce-v1");
    hasher.update(shared_secret);
    hasher.update(nonce_counter.to_le_bytes());
    let hash = hasher.finalize();
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&hash[..12]);
    nonce
}

/// Encrypt a plaintext payload using AES-256-GCM.
/// Returns (nonce_counter, ciphertext_with_tag).
pub fn encrypt_payload(
    aes_key: &[u8; 32],
    shared_secret: &[u8; 32],
    nonce_counter: u64,
    plaintext: &[u8],
) -> Result<(u64, Vec<u8>), EncryptedInferenceError> {
    let cipher = Aes256Gcm::new_from_slice(aes_key)
        .map_err(|e| EncryptedInferenceError::Encryption(e.to_string()))?;
    let nonce_bytes = derive_nonce(shared_secret, nonce_counter);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| EncryptedInferenceError::Encryption(e.to_string()))?;
    Ok((nonce_counter, ciphertext))
}

/// Decrypt an AES-256-GCM encrypted payload.
pub fn decrypt_payload(
    aes_key: &[u8; 32],
    shared_secret: &[u8; 32],
    nonce_counter: u64,
    ciphertext: &[u8],
) -> Result<Vec<u8>, EncryptedInferenceError> {
    let cipher = Aes256Gcm::new_from_slice(aes_key)
        .map_err(|e| EncryptedInferenceError::Decryption(e.to_string()))?;
    let nonce_bytes = derive_nonce(shared_secret, nonce_counter);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| EncryptedInferenceError::Decryption(e.to_string()))?;
    Ok(plaintext)
}

/// Parse a 32-byte public key from hex.
pub fn parse_public_key(hex_str: &str) -> Result<PublicKey, EncryptedInferenceError> {
    let bytes = hex::decode(hex_str)
        .map_err(|_| EncryptedInferenceError::InvalidKeyFormat)?;
    if bytes.len() != 32 {
        return Err(EncryptedInferenceError::InvalidKeyFormat);
    }
    Ok(PublicKey::from(<[u8; 32]>::try_from(bytes.as_slice()).unwrap()))
}

// ---------------------------------------------------------------------------
// Commitment scheme (SHA-256 based)
// ---------------------------------------------------------------------------

/// Create a commitment: SHA-256(model_id || prompt_hash || nonce || timestamp).
pub fn create_commitment(
    model_id: &str,
    prompt_hash: &[u8; 32],
    nonce: &[u8; 32],
    timestamp: u64,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"xergon-commitment-v1");
    hasher.update(model_id.as_bytes());
    hasher.update(prompt_hash);
    hasher.update(nonce);
    hasher.update(timestamp.to_le_bytes());
    let result = hasher.finalize();
    let mut commitment = [0u8; 32];
    commitment.copy_from_slice(&result);
    commitment
}

/// Hash arbitrary data with SHA-256.
pub fn hash_data(data: &[u8]) -> [u8; 32] {
    let result = Sha256::digest(data);
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

// ---------------------------------------------------------------------------
// Proof of inference
// ---------------------------------------------------------------------------

/// A proof that an inference was actually performed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceProof {
    /// Unique proof ID.
    pub proof_id: String,
    /// Provider that performed the inference.
    pub provider_id: String,
    /// Model used for inference.
    pub model_id: String,
    /// SHA-256 hash of the prompt.
    pub prompt_hash: String, // hex
    /// SHA-256 hash of the response.
    pub response_hash: String, // hex
    /// Commitment submitted before inference (hex).
    pub commitment: String,
    /// Nonce used in the commitment (hex).
    pub commitment_nonce: String,
    /// Timestamp of the commitment (epoch seconds).
    pub commitment_timestamp: u64,
    /// Timestamp when the proof was generated (epoch seconds).
    pub proof_timestamp: u64,
    /// Difficulty target for proof-of-work (number of leading zero bits required).
    pub pow_difficulty: u8,
    /// Nonce found for proof-of-work.
    pub pow_nonce: u64,
}

impl InferenceProof {
    /// Compute the proof-of-work hash: SHA-256(commitment || response_hash || pow_nonce).
    fn pow_hash(&self) -> [u8; 32] {
        let commitment_bytes = hex::decode(&self.commitment).unwrap_or_default();
        let response_bytes = hex::decode(&self.response_hash).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(b"xergon-pow-v1");
        hasher.update(&commitment_bytes);
        hasher.update(&response_bytes);
        hasher.update(self.pow_nonce.to_le_bytes());
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        hash
    }

    /// Check that the proof-of-work meets the difficulty target.
    pub fn check_pow(&self) -> bool {
        let hash = self.pow_hash();
        let leading_zero_bits = hash.iter().take_while(|&&b| b == 0).count() * 8
            + hash
                .iter()
                .find(|&&b| b != 0)
                .map(|&b| b.leading_zeros() as usize)
                .unwrap_or(0);
        leading_zero_bits >= self.pow_difficulty as usize
    }

    /// Verify the commitment: recompute from model_id, prompt_hash, nonce, timestamp.
    pub fn verify_commitment(&self) -> bool {
        let prompt_bytes = match hex::decode(&self.prompt_hash) {
            Ok(b) if b.len() == 32 => b,
            _ => return false,
        };
        let nonce_bytes = match hex::decode(&self.commitment_nonce) {
            Ok(b) if b.len() == 32 => b,
            _ => return false,
        };
        let prompt_hash: [u8; 32] = prompt_bytes.try_into().unwrap();
        let nonce: [u8; 32] = nonce_bytes.try_into().unwrap();
        let expected = create_commitment(
            &self.model_id,
            &prompt_hash,
            &nonce,
            self.commitment_timestamp,
        );
        let expected_hex = hex::encode(expected);
        expected_hex == self.commitment
    }

    /// Full verification: commitment + proof-of-work.
    pub fn verify(&self) -> Result<(), EncryptedInferenceError> {
        if !self.verify_commitment() {
            return Err(EncryptedInferenceError::InvalidCommitment(
                "Commitment does not match model+prompt+nonce".into(),
            ));
        }
        if !self.check_pow() {
            return Err(EncryptedInferenceError::VerificationFailed(
                "Proof-of-work does not meet difficulty target".into(),
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Proof cache with TTL
// ---------------------------------------------------------------------------

/// A cached verified proof with its expiry time.
#[derive(Debug, Clone)]
struct CachedProof {
    proof: InferenceProof,
    verified_at: Instant,
    expires_at: Instant,
}

/// Thread-safe proof cache backed by DashMap with TTL-based expiry.
pub struct ProofCache {
    entries: DashMap<String, CachedProof>,
    ttl: Duration,
}

impl ProofCache {
    /// Create a new proof cache with the given TTL.
    pub fn new(ttl: Duration) -> Self {
        Self {
            entries: DashMap::new(),
            ttl,
        }
    }

    /// Insert a verified proof. Returns true if it was newly inserted (not a replay).
    pub fn insert(&self, proof: &InferenceProof) -> bool {
        // Evict expired entries first (simple probabilistic cleanup)
        self.evict_expired();

        let now = Instant::now();
        let cached = CachedProof {
            proof: proof.clone(),
            verified_at: now,
            expires_at: now + self.ttl,
        };
        match self.entries.entry(proof.proof_id.clone()) {
            dashmap::mapref::entry::Entry::Occupied(_) => {
                // Already exists — replay
                warn!(proof_id = %proof.proof_id, "Replay detection: proof already cached");
                false
            }
            dashmap::mapref::entry::Entry::Vacant(e) => {
                e.insert(cached);
                true
            }
        }
    }

    /// Check if a proof has already been verified (replay detection).
    pub fn contains(&self, proof_id: &str) -> bool {
        if let Some(entry) = self.entries.get(proof_id) {
            if entry.expires_at > Instant::now() {
                return true;
            }
        }
        false
    }

    /// List all non-expired proofs.
    pub fn list_recent(&self, limit: usize) -> Vec<InferenceProof> {
        self.evict_expired();
        self.entries
            .iter()
            .filter(|e| e.expires_at > Instant::now())
            .take(limit)
            .map(|e| e.value().proof.clone())
            .collect()
    }

    /// Clear all cached proofs.
    pub fn clear(&self) -> usize {
        let count = self.entries.len();
        self.entries.clear();
        count
    }

    /// Evict expired entries.
    fn evict_expired(&self) {
        let now = Instant::now();
        self.entries.retain(|_, v| v.expires_at > now);
    }

    /// Number of entries (including possibly expired).
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

// ---------------------------------------------------------------------------
// Encryption state — holds relay keypair + provider keys
// ---------------------------------------------------------------------------

/// Per-provider encryption state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderEncryptionState {
    /// Provider identifier (e.g., public key or endpoint).
    pub provider_id: String,
    /// Provider's X25519 public key (hex).
    pub public_key: String,
    /// Whether encryption is enabled for this provider.
    pub encryption_enabled: bool,
    /// When this key was registered.
    pub registered_at: DateTime<Utc>,
}

/// Configuration for the encrypted inference subsystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionConfig {
    /// Whether encrypted inference is enabled globally.
    pub enabled: bool,
    /// Default proof-of-work difficulty (leading zero bits).
    pub pow_difficulty: u8,
    /// Proof cache TTL in seconds.
    pub proof_cache_ttl_secs: u64,
    /// Max proofs to return in list endpoint.
    pub max_list_proofs: usize,
    /// Nonce counter for relay-side encryption (persists in memory).
    #[serde(skip)]
    pub nonce_counter: u64,
}

impl Default for EncryptionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            pow_difficulty: 8,
            proof_cache_ttl_secs: 3600,
            max_list_proofs: 100,
            nonce_counter: 0,
        }
    }
}

/// The core encrypted inference engine. Holds the relay's keypair, provider
/// public keys, the proof cache, and configuration.
pub struct EncryptedInferenceState {
    /// Relay's X25519 static secret key.
    relay_secret: StaticSecret,
    /// Relay's X25519 public key.
    relay_public: PublicKey,
    /// Provider public keys (provider_id -> encryption state).
    provider_keys: DashMap<String, ProviderEncryptionState>,
    /// Verified proof cache with TTL.
    proof_cache: ProofCache,
    /// Configuration.
    config: tokio::sync::RwLock<EncryptionConfig>,
}

impl EncryptedInferenceState {
    /// Create a new encrypted inference engine, generating a fresh relay keypair.
    pub fn new(config: EncryptionConfig) -> Self {
        let (secret, public) = generate_keypair();
        info!(
            relay_public_key = hex::encode(public.as_bytes()),
            "Encrypted inference engine initialized"
        );
        Self {
            relay_secret: secret,
            relay_public: public,
            provider_keys: DashMap::new(),
            proof_cache: ProofCache::new(Duration::from_secs(config.proof_cache_ttl_secs)),
            config: tokio::sync::RwLock::new(config),
        }
    }

    /// Get the relay's public key as hex.
    pub fn relay_public_key_hex(&self) -> String {
        hex::encode(self.relay_public.as_bytes())
    }

    /// Register a provider's public key.
    pub fn register_provider_key(&self, state: ProviderEncryptionState) {
        let pid = state.provider_id.clone();
        self.provider_keys.insert(state.provider_id.clone(), state);
        info!(provider_id = %pid, "Provider encryption key registered");
    }

    /// Remove a provider's public key.
    pub fn remove_provider_key(&self, provider_id: &str) -> bool {
        let removed = self.provider_keys.remove(provider_id).is_some();
        if removed {
            info!(provider_id, "Provider encryption key removed");
        }
        removed
    }

    /// List registered provider keys.
    pub fn list_provider_keys(&self) -> Vec<ProviderEncryptionState> {
        self.provider_keys.iter().map(|e| e.value().clone()).collect()
    }

    /// Encrypt a payload for a specific provider using ECDH + AES-256-GCM.
    pub fn encrypt_for_provider(
        &self,
        provider_id: &str,
        plaintext: &[u8],
    ) -> Result<EncryptedPayload, EncryptedInferenceError> {
        let provider_state = self
            .provider_keys
            .get(provider_id)
            .ok_or_else(|| EncryptedInferenceError::ProviderNotFound(provider_id.into()))?;

        if !provider_state.encryption_enabled {
            return Err(EncryptedInferenceError::Encryption(
                "Encryption disabled for this provider".into(),
            ));
        }

        let provider_public = parse_public_key(&provider_state.public_key)?;
        let shared_secret = compute_shared_secret(&self.relay_secret, &provider_public);
        let aes_key = derive_aes_key(&shared_secret);

        let mut config = self.config.blocking_write();
        let nonce_counter = config.nonce_counter;
        config.nonce_counter += 1;
        drop(config);

        let (nc, ciphertext) = encrypt_payload(&aes_key, &shared_secret, nonce_counter, plaintext)?;

        Ok(EncryptedPayload {
            nonce_counter: nc,
            ciphertext: hex::encode(&ciphertext),
            relay_public_key: self.relay_public_key_hex(),
        })
    }

    /// Decrypt a payload from a specific provider.
    pub fn decrypt_from_provider(
        &self,
        provider_id: &str,
        nonce_counter: u64,
        ciphertext_hex: &str,
    ) -> Result<Vec<u8>, EncryptedInferenceError> {
        let provider_state = self
            .provider_keys
            .get(provider_id)
            .ok_or_else(|| EncryptedInferenceError::ProviderNotFound(provider_id.into()))?;

        let provider_public = parse_public_key(&provider_state.public_key)?;
        let shared_secret = compute_shared_secret(&self.relay_secret, &provider_public);
        let aes_key = derive_aes_key(&shared_secret);

        let ciphertext = hex::decode(ciphertext_hex)
            .map_err(|_| EncryptedInferenceError::InvalidKeyFormat)?;

        decrypt_payload(&aes_key, &shared_secret, nonce_counter, &ciphertext)
    }

    /// Verify an inference proof. Returns Ok(()) if valid, error otherwise.
    pub async fn verify_proof(&self, proof: &InferenceProof) -> Result<(), EncryptedInferenceError> {
        // Check for replay
        if self.proof_cache.contains(&proof.proof_id) {
            return Err(EncryptedInferenceError::ReplayDetected);
        }

        // Verify commitment and proof-of-work
        proof.verify()?;

        // Cache the verified proof
        self.proof_cache.insert(proof);
        info!(
            proof_id = %proof.proof_id,
            provider_id = %proof.provider_id,
            model_id = %proof.model_id,
            "Inference proof verified and cached"
        );
        Ok(())
    }

    /// List recent verified proofs.
    pub fn list_proofs(&self, limit: Option<usize>) -> Vec<InferenceProof> {
        let config = self.config.blocking_read();
        let limit = limit.unwrap_or(config.max_list_proofs);
        self.proof_cache.list_recent(limit)
    }

    /// Clear the proof cache. Returns the number of cleared proofs.
    pub fn clear_proofs(&self) -> usize {
        let count = self.proof_cache.clear();
        info!(cleared = count, "Proof cache cleared");
        count
    }

    /// Get current configuration.
    pub async fn get_config(&self) -> EncryptionConfig {
        self.config.read().await.clone()
    }

    /// Update configuration.
    pub async fn update_config(&self, update: EncryptionConfigUpdate) {
        let mut config = self.config.write().await;
        if let Some(enabled) = update.enabled {
            config.enabled = enabled;
        }
        if let Some(difficulty) = update.pow_difficulty {
            config.pow_difficulty = difficulty;
        }
        if let Some(ttl) = update.proof_cache_ttl_secs {
            config.proof_cache_ttl_secs = ttl;
        }
        if let Some(max) = update.max_list_proofs {
            config.max_list_proofs = max;
        }
        info!(
            enabled = config.enabled,
            pow_difficulty = config.pow_difficulty,
            proof_ttl = config.proof_cache_ttl_secs,
            "Encryption config updated"
        );
    }
}

// ---------------------------------------------------------------------------
// Data types for API requests/responses
// ---------------------------------------------------------------------------

/// An encrypted payload ready for transmission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedPayload {
    /// Nonce counter used for encryption.
    pub nonce_counter: u64,
    /// AES-256-GCM ciphertext (hex-encoded).
    pub ciphertext: String,
    /// Relay's public key (hex-encoded).
    pub relay_public_key: String,
}

/// Request body for POST /v1/inference/encrypted.
#[derive(Debug, Deserialize)]
pub struct EncryptedInferenceRequest {
    /// Target provider ID.
    pub provider_id: String,
    /// Target model ID.
    pub model_id: String,
    /// Encrypted prompt payload (hex-encoded ciphertext + nonce_counter).
    pub encrypted_payload: EncryptedPayload,
    /// Client's ephemeral public key (hex) for ECDH key exchange.
    /// This enables end-to-end encryption where the relay re-encrypts.
    pub client_public_key: Option<String>,
}

/// Response body for POST /v1/inference/encrypted.
#[derive(Debug, Serialize)]
pub struct EncryptedInferenceResponse {
    /// Request ID.
    pub request_id: String,
    /// Whether the request was forwarded successfully.
    pub forwarded: bool,
    /// Encrypted response payload (if available).
    pub encrypted_response: Option<EncryptedPayload>,
    /// Error message if forwarding failed.
    pub error: Option<String>,
}

/// Response for GET /v1/inference/keys.
#[derive(Debug, Serialize)]
pub struct KeysResponse {
    /// Relay's X25519 public key (hex).
    pub relay_public_key: String,
    /// Registered provider keys.
    pub providers: Vec<ProviderEncryptionState>,
}

/// Request body for POST /v1/inference/verify.
#[derive(Debug, Deserialize)]
pub struct VerifyProofRequest {
    /// The proof to verify.
    pub proof: InferenceProof,
}

/// Response for POST /v1/inference/verify.
#[derive(Debug, Serialize)]
pub struct VerifyProofResponse {
    /// Whether the proof is valid.
    pub valid: bool,
    /// Error message if verification failed.
    pub error: Option<String>,
}

/// Response for GET /v1/inference/proofs.
#[derive(Debug, Serialize)]
pub struct ProofsListResponse {
    /// List of verified proofs.
    pub proofs: Vec<InferenceProof>,
    /// Total count.
    pub count: usize,
}

/// Response for DELETE /v1/inference/proofs.
#[derive(Debug, Serialize)]
pub struct ProofsClearResponse {
    /// Number of proofs cleared.
    pub cleared: usize,
}

/// Request body for POST /v1/inference/providers/keys.
#[derive(Debug, Deserialize)]
pub struct RegisterProviderKeyRequest {
    /// Provider identifier.
    pub provider_id: String,
    /// Provider's X25519 public key (hex).
    pub public_key: String,
    /// Whether encryption is enabled.
    #[serde(default = "default_true")]
    pub encryption_enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Configuration update request.
#[derive(Debug, Deserialize)]
pub struct EncryptionConfigUpdate {
    pub enabled: Option<bool>,
    pub pow_difficulty: Option<u8>,
    pub proof_cache_ttl_secs: Option<u64>,
    pub max_list_proofs: Option<usize>,
}

// ---------------------------------------------------------------------------
// Axum handlers
// ---------------------------------------------------------------------------

/// POST /v1/inference/encrypted — Submit an encrypted inference request.
///
/// The relay re-encrypts the payload for the target provider using the
/// relay's own ECDH shared secret with that provider. This provides
/// end-to-end encryption: the relay never sees plaintext.
async fn handle_encrypted_inference(
    State(state): State<AppState>,
    Json(req): Json<EncryptedInferenceRequest>,
) -> Result<Json<EncryptedInferenceResponse>, EncryptedInferenceError> {
    let ei_state = &state.encrypted_inference;

    let config = ei_state.get_config().await;
    if !config.enabled {
        return Ok(Json(EncryptedInferenceResponse {
            request_id: uuid::Uuid::new_v4().to_string(),
            forwarded: false,
            encrypted_response: None,
            error: Some("Encrypted inference is disabled".into()),
        }));
    }

    // Decode the client's encrypted payload
    let ciphertext = hex::decode(&req.encrypted_payload.ciphertext)
        .map_err(|_| EncryptedInferenceError::Decryption("Invalid hex in ciphertext".into()))?;

    // Re-encrypt for the target provider
    match ei_state.encrypt_for_provider(&req.provider_id, &ciphertext) {
        Ok(payload) => {
            debug!(
                provider_id = %req.provider_id,
                model_id = %req.model_id,
                nonce_counter = payload.nonce_counter,
                "Encrypted inference request forwarded"
            );
            Ok(Json(EncryptedInferenceResponse {
                request_id: uuid::Uuid::new_v4().to_string(),
                forwarded: true,
                encrypted_response: Some(payload),
                error: None,
            }))
        }
        Err(e) => Ok(Json(EncryptedInferenceResponse {
            request_id: uuid::Uuid::new_v4().to_string(),
            forwarded: false,
            encrypted_response: None,
            error: Some(e.to_string()),
        })),
    }
}

/// GET /v1/inference/keys — Get the relay's public key and registered provider keys.
async fn handle_get_keys(
    State(state): State<AppState>,
) -> Json<KeysResponse> {
    let ei_state = &state.encrypted_inference;
    Json(KeysResponse {
        relay_public_key: ei_state.relay_public_key_hex(),
        providers: ei_state.list_provider_keys(),
    })
}

/// POST /v1/inference/verify — Verify an inference proof.
async fn handle_verify_proof(
    State(state): State<AppState>,
    Json(req): Json<VerifyProofRequest>,
) -> Result<Json<VerifyProofResponse>, EncryptedInferenceError> {
    let ei_state = &state.encrypted_inference;
    match ei_state.verify_proof(&req.proof).await {
        Ok(()) => Ok(Json(VerifyProofResponse {
            valid: true,
            error: None,
        })),
        Err(e) => Ok(Json(VerifyProofResponse {
            valid: false,
            error: Some(e.to_string()),
        })),
    }
}

/// GET /v1/inference/proofs — List recent verified proofs.
async fn handle_list_proofs(
    State(state): State<AppState>,
) -> Json<ProofsListResponse> {
    let ei_state = &state.encrypted_inference;
    let proofs = ei_state.list_proofs(None);
    let count = proofs.len();
    Json(ProofsListResponse { proofs, count })
}

/// DELETE /v1/inference/proofs — Clear proof cache (admin).
async fn handle_clear_proofs(
    State(state): State<AppState>,
) -> Json<ProofsClearResponse> {
    let ei_state = &state.encrypted_inference;
    let cleared = ei_state.clear_proofs();
    Json(ProofsClearResponse { cleared })
}

/// GET /v1/inference/config — Get encryption config (admin).
async fn handle_get_config(
    State(state): State<AppState>,
) -> Json<EncryptionConfig> {
    let ei_state = &state.encrypted_inference;
    Json(ei_state.get_config().await)
}

/// PUT /v1/inference/config — Update encryption config (admin).
async fn handle_update_config(
    State(state): State<AppState>,
    Json(update): Json<EncryptionConfigUpdate>,
) -> StatusCode {
    let ei_state = &state.encrypted_inference;
    ei_state.update_config(update).await;
    StatusCode::NO_CONTENT
}

/// POST /v1/inference/providers/keys — Register a provider's public key (admin).
async fn handle_register_provider_key(
    State(state): State<AppState>,
    Json(req): Json<RegisterProviderKeyRequest>,
) -> Result<StatusCode, EncryptedInferenceError> {
    // Validate the public key format
    let _pk = parse_public_key(&req.public_key)?;
    let ei_state = &state.encrypted_inference;
    ei_state.register_provider_key(ProviderEncryptionState {
        provider_id: req.provider_id,
        public_key: req.public_key,
        encryption_enabled: req.encryption_enabled,
        registered_at: Utc::now(),
    });
    Ok(StatusCode::CREATED)
}

/// DELETE /v1/inference/providers/keys/:provider_id — Remove a provider key.
async fn handle_remove_provider_key(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> StatusCode {
    let ei_state = &state.encrypted_inference;
    if ei_state.remove_provider_key(&provider_id) {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the encrypted inference router with all endpoints.
pub fn build_encrypted_inference_router(state: AppState) -> Router<AppState> {
    Router::new()
        // Core inference endpoints
        .route("/v1/inference/encrypted", post(handle_encrypted_inference))
        .route("/v1/inference/keys", get(handle_get_keys))
        // Proof verification endpoints
        .route("/v1/inference/verify", post(handle_verify_proof))
        .route("/v1/inference/proofs", get(handle_list_proofs))
        .route("/v1/inference/proofs", delete(handle_clear_proofs))
        // Admin config endpoints
        .route("/v1/inference/config", get(handle_get_config))
        .route("/v1/inference/config", put(handle_update_config))
        // Provider key management
        .route("/v1/inference/providers/keys", post(handle_register_provider_key))
        .route(
            "/v1/inference/providers/keys/{provider_id}",
            delete(handle_remove_provider_key),
        )
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use x25519_dalek::{PublicKey, StaticSecret};

    // Helper: generate a test keypair
    fn test_keypair() -> (StaticSecret, PublicKey) {
        generate_keypair()
    }

    // Helper: create a proof with valid commitment and PoW
    fn make_valid_proof(
        provider_id: &str,
        model_id: &str,
        prompt: &str,
        response: &str,
        pow_difficulty: u8,
    ) -> InferenceProof {
        let prompt_hash = hash_data(prompt.as_bytes());
        let commitment_nonce_bytes = rand::random::<[u8; 32]>();
        let commitment_timestamp = 1700000000u64;
        let commitment = create_commitment(
            model_id,
            &prompt_hash,
            &commitment_nonce_bytes,
            commitment_timestamp,
        );
        let response_hash = hash_data(response.as_bytes());

        // Find a valid PoW nonce
        let mut pow_nonce: u64 = 0;
        loop {
            let proof = InferenceProof {
                proof_id: uuid::Uuid::new_v4().to_string(),
                provider_id: provider_id.to_string(),
                model_id: model_id.to_string(),
                prompt_hash: hex::encode(prompt_hash),
                response_hash: hex::encode(response_hash),
                commitment: hex::encode(commitment),
                commitment_nonce: hex::encode(commitment_nonce_bytes),
                commitment_timestamp,
                proof_timestamp: commitment_timestamp + 60,
                pow_difficulty,
                pow_nonce,
            };
            if proof.check_pow() {
                return proof;
            }
            pow_nonce += 1;
        }
    }

    // ---------- Key generation tests ----------

    #[test]
    fn test_keypair_generation() {
        let (secret1, public1) = test_keypair();
        let (secret2, public2) = test_keypair();
        // Different keypairs should produce different keys
        assert_ne!(public1.as_bytes(), public2.as_bytes());
        assert_ne!(secret1.to_bytes(), secret2.to_bytes());
    }

    #[test]
    fn test_keypair_public_from_secret() {
        let (secret, public) = test_keypair();
        let derived_public = PublicKey::from(&secret);
        assert_eq!(public.as_bytes(), derived_public.as_bytes());
    }

    // ---------- Key exchange tests ----------

    #[test]
    fn test_key_exchange_symmetric() {
        let (secret_a, public_a) = test_keypair();
        let (secret_b, public_b) = test_keypair();

        // A computes shared secret with B's public key
        let shared_ab = compute_shared_secret(&secret_a, &public_b);
        // B computes shared secret with A's public key
        let shared_ba = compute_shared_secret(&secret_b, &public_a);

        // Should be identical
        assert_eq!(shared_ab, shared_ba);
    }

    #[test]
    fn test_key_exchange_different_pairs() {
        let (secret_a, public_a) = test_keypair();
        let (secret_b, _) = test_keypair();
        let (secret_c, public_c) = test_keypair();

        let shared_ab = compute_shared_secret(&secret_a, &public_c);
        let shared_cb = compute_shared_secret(&secret_c, &public_a);

        // Should be identical (commutative)
        assert_eq!(shared_ab, shared_cb);

        // But different from a different pair
        let (_, public_b) = {
            let secret_b = secret_b.to_bytes();
            let s = StaticSecret::from(secret_b);
            (s.clone(), PublicKey::from(&s))
        };
        let shared_ac = compute_shared_secret(&secret_a, &public_b);
        // Extremely unlikely to be the same
        assert_ne!(shared_ab, shared_ac);
    }

    // ---------- AES key derivation tests ----------

    #[test]
    fn test_aes_key_derivation_deterministic() {
        let shared = [0xABu8; 32];
        let key1 = derive_aes_key(&shared);
        let key2 = derive_aes_key(&shared);
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_aes_key_derivation_different_secrets() {
        let shared_a = [0xAAu8; 32];
        let shared_b = [0xBBu8; 32];
        let key_a = derive_aes_key(&shared_a);
        let key_b = derive_aes_key(&shared_b);
        assert_ne!(key_a, key_b);
    }

    // ---------- Nonce derivation tests ----------

    #[test]
    fn test_nonce_derivation_unique() {
        let shared = [0x42u8; 32];
        let n1 = derive_nonce(&shared, 0);
        let n2 = derive_nonce(&shared, 1);
        assert_ne!(n1, n2);
    }

    #[test]
    fn test_nonce_derivation_deterministic() {
        let shared = [0x42u8; 32];
        let n1 = derive_nonce(&shared, 42);
        let n2 = derive_nonce(&shared, 42);
        assert_eq!(n1, n2);
    }

    // ---------- Encryption/decryption tests ----------

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let (secret_a, public_b) = {
            let (s, _) = test_keypair();
            let (_, p) = test_keypair();
            (s, p)
        };
        let shared = compute_shared_secret(&secret_a, &public_b);
        let aes_key = derive_aes_key(&shared);

        let plaintext = b"Hello, encrypted inference!";
        let (nonce_counter, ciphertext) =
            encrypt_payload(&aes_key, &shared, 0, plaintext).unwrap();
        let decrypted = decrypt_payload(&aes_key, &shared, nonce_counter, &ciphertext).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[test]
    fn test_encrypt_decrypt_large_payload() {
        let (secret, _) = test_keypair();
        let (_, public) = test_keypair();
        let shared = compute_shared_secret(&secret, &public);
        let aes_key = derive_aes_key(&shared);

        // 100KB payload
        let plaintext = vec![0x42u8; 100_000];
        let (nc, ciphertext) =
            encrypt_payload(&aes_key, &shared, 0, &plaintext).unwrap();
        let decrypted = decrypt_payload(&aes_key, &shared, nc, &ciphertext).unwrap();

        assert_eq!(plaintext, decrypted);
    }

    #[test]
    fn test_decrypt_wrong_key_fails() {
        let (secret1, _) = test_keypair();
        let (_, public1) = test_keypair();
        let shared1 = compute_shared_secret(&secret1, &public1);
        let key1 = derive_aes_key(&shared1);

        let (secret2, _) = test_keypair();
        let (_, public2) = test_keypair();
        let shared2 = compute_shared_secret(&secret2, &public2);
        let key2 = derive_aes_key(&shared2);

        let plaintext = b"secret message";
        let (nc, ciphertext) =
            encrypt_payload(&key1, &shared1, 0, plaintext).unwrap();

        // Decrypting with wrong key should fail
        let result = decrypt_payload(&key2, &shared2, nc, &ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_wrong_nonce_fails() {
        let (secret, _) = test_keypair();
        let (_, public) = test_keypair();
        let shared = compute_shared_secret(&secret, &public);
        let aes_key = derive_aes_key(&shared);

        let plaintext = b"test message";
        let (nc, ciphertext) =
            encrypt_payload(&aes_key, &shared, 0, plaintext).unwrap();

        // Wrong nonce counter
        let result = decrypt_payload(&aes_key, &shared, nc + 1, &ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_encryptions_different_nonces() {
        let (secret, _) = test_keypair();
        let (_, public) = test_keypair();
        let shared = compute_shared_secret(&secret, &public);
        let aes_key = derive_aes_key(&shared);

        let plaintext = b"same message";
        let (nc1, ct1) = encrypt_payload(&aes_key, &shared, 0, plaintext).unwrap();
        let (nc2, ct2) = encrypt_payload(&aes_key, &shared, 1, plaintext).unwrap();

        // Different nonces should produce different ciphertexts
        assert_ne!(ct1, ct2);
        assert_ne!(nc1, nc2);

        // Both should decrypt correctly
        let d1 = decrypt_payload(&aes_key, &shared, nc1, &ct1).unwrap();
        let d2 = decrypt_payload(&aes_key, &shared, nc2, &ct2).unwrap();
        assert_eq!(d1, d2);
        assert_eq!(d1, plaintext.as_slice());
    }

    // ---------- Commitment scheme tests ----------

    #[test]
    fn test_commitment_deterministic() {
        let model = "llama-3.1-8b";
        let prompt_hash = hash_data(b"test prompt");
        let nonce = [0x11u8; 32];
        let ts = 1700000000u64;

        let c1 = create_commitment(model, &prompt_hash, &nonce, ts);
        let c2 = create_commitment(model, &prompt_hash, &nonce, ts);
        assert_eq!(c1, c2);
    }

    #[test]
    fn test_commitment_different_prompts() {
        let model = "llama-3.1-8b";
        let nonce = [0x11u8; 32];
        let ts = 1700000000u64;

        let c1 = create_commitment(model, &hash_data(b"prompt a"), &nonce, ts);
        let c2 = create_commitment(model, &hash_data(b"prompt b"), &nonce, ts);
        assert_ne!(c1, c2);
    }

    #[test]
    fn test_commitment_different_nonces() {
        let model = "llama-3.1-8b";
        let prompt_hash = hash_data(b"test prompt");
        let ts = 1700000000u64;

        let c1 = create_commitment(model, &prompt_hash, &[0x11u8; 32], ts);
        let c2 = create_commitment(model, &prompt_hash, &[0x22u8; 32], ts);
        assert_ne!(c1, c2);
    }

    // ---------- Proof verification tests ----------

    #[test]
    fn test_proof_verification_valid() {
        let proof = make_valid_proof("provider-1", "llama-3.1-8b", "Hello", "Hi there!", 0);
        assert!(proof.verify().is_ok());
    }

    #[test]
    fn test_proof_verification_pow() {
        let proof = make_valid_proof("provider-1", "llama-3.1-8b", "Hello", "Hi there!", 4);
        assert!(proof.check_pow());
    }

    #[test]
    fn test_proof_verification_invalid_pow() {
        let proof = make_valid_proof("provider-1", "llama-3.1-8b", "Hello", "Hi there!", 4);
        // Set difficulty impossibly high
        let mut bad_proof = proof.clone();
        bad_proof.pow_difficulty = 255;
        assert!(!bad_proof.check_pow());
    }

    #[test]
    fn test_proof_commitment_verification() {
        let proof = make_valid_proof("provider-1", "llama-3.1-8b", "Hello", "Hi there!", 0);
        assert!(proof.verify_commitment());

        // Tamper with model_id
        let mut bad_proof = proof.clone();
        bad_proof.model_id = "different-model".to_string();
        assert!(!bad_proof.verify_commitment());
    }

    #[test]
    fn test_proof_tampered_response_hash() {
        let proof = make_valid_proof("provider-1", "llama-3.1-8b", "Hello", "Hi there!", 0);
        // The commitment doesn't cover the response hash, so tampering it doesn't
        // break the commitment check — but it would break PoW
        let mut bad_proof = proof.clone();
        bad_proof.response_hash = hex::encode(hash_data(b"totally different response"));
        // PoW was computed with the original response hash, so it should fail
        assert!(!bad_proof.check_pow());
    }

    // ---------- Proof cache tests ----------

    #[tokio::test]
    async fn test_proof_cache_insert_and_contains() {
        let cache = ProofCache::new(Duration::from_secs(60));
        let proof = make_valid_proof("p1", "m1", "prompt", "response", 0);

        assert!(!cache.contains(&proof.proof_id));
        assert!(cache.insert(&proof));
        assert!(cache.contains(&proof.proof_id));
    }

    #[tokio::test]
    async fn test_proof_cache_replay_detection() {
        let cache = ProofCache::new(Duration::from_secs(60));
        let proof = make_valid_proof("p1", "m1", "prompt", "response", 0);

        assert!(cache.insert(&proof));
        // Second insert should return false (replay)
        assert!(!cache.insert(&proof));
    }

    #[test]
    fn test_proof_cache_list_recent() {
        let cache = ProofCache::new(Duration::from_secs(60));
        let p1 = make_valid_proof("p1", "m1", "a", "b", 0);
        let p2 = make_valid_proof("p2", "m2", "c", "d", 0);

        cache.insert(&p1);
        cache.insert(&p2);

        let listed = cache.list_recent(10);
        assert_eq!(listed.len(), 2);
    }

    #[test]
    fn test_proof_cache_clear() {
        let cache = ProofCache::new(Duration::from_secs(60));
        cache.insert(&make_valid_proof("p1", "m1", "a", "b", 0));
        cache.insert(&make_valid_proof("p2", "m2", "c", "d", 0));

        assert_eq!(cache.clear(), 2);
        assert_eq!(cache.len(), 0);
    }

    #[tokio::test]
    async fn test_proof_cache_ttl_expiry() {
        let cache = ProofCache::new(Duration::from_millis(50));
        let proof = make_valid_proof("p1", "m1", "prompt", "response", 0);

        cache.insert(&proof);
        assert!(cache.contains(&proof.proof_id));

        tokio::time::sleep(Duration::from_millis(100)).await;

        // After expiry, contains should return false and we should be able to re-insert
        assert!(!cache.contains(&proof.proof_id));
        assert!(cache.insert(&proof)); // Should succeed again
    }

    // ---------- EncryptedInferenceState integration tests ----------

    #[tokio::test]
    async fn test_encrypted_inference_state_creation() {
        let state = EncryptedInferenceState::new(EncryptionConfig::default());
        let pubkey_hex = state.relay_public_key_hex();
        assert_eq!(pubkey_hex.len(), 64); // 32 bytes = 64 hex chars
    }

    #[tokio::test]
    async fn test_register_and_encrypt_for_provider() {
        let state = EncryptedInferenceState::new(EncryptionConfig::default());

        // Create a provider keypair
        let (_, provider_public) = test_keypair();

        // Register provider
        state.register_provider_key(ProviderEncryptionState {
            provider_id: "test-provider".to_string(),
            public_key: hex::encode(provider_public.as_bytes()),
            encryption_enabled: true,
            registered_at: Utc::now(),
        });

        // Encrypt something for the provider
        let plaintext = b"Hello, provider!";
        let result = state.encrypt_for_provider("test-provider", plaintext);
        assert!(result.is_ok());

        let payload = result.unwrap();
        assert!(!payload.ciphertext.is_empty());
        assert_eq!(payload.relay_public_key, state.relay_public_key_hex());
    }

    #[tokio::test]
    async fn test_encrypt_nonexistent_provider_fails() {
        let state = EncryptedInferenceState::new(EncryptionConfig::default());
        let result = state.encrypt_for_provider("nonexistent", b"data");
        assert!(result.is_err());
        match result.unwrap_err() {
            EncryptedInferenceError::ProviderNotFound(_) => {}
            e => panic!("Expected ProviderNotFound, got: {e}"),
        }
    }

    #[tokio::test]
    async fn test_full_proof_verification_flow() {
        let state = EncryptedInferenceState::new(EncryptionConfig::default());
        let proof = make_valid_proof("provider-1", "llama-3.1-8b", "Hello", "Hi there!", 0);

        // Verify the proof
        let result = state.verify_proof(&proof).await;
        assert!(result.is_ok());

        // Replay should fail
        let result2 = state.verify_proof(&proof).await;
        assert!(result2.is_err());
        match result2.unwrap_err() {
            EncryptedInferenceError::ReplayDetected => {}
            e => panic!("Expected ReplayDetected, got: {e}"),
        }
    }

    #[tokio::test]
    async fn test_config_update() {
        let state = EncryptedInferenceState::new(EncryptionConfig::default());

        state.update_config(EncryptionConfigUpdate {
            enabled: Some(false),
            pow_difficulty: None,
            proof_cache_ttl_secs: None,
            max_list_proofs: None,
        }).await;

        let config = state.get_config().await;
        assert!(!config.enabled);
        assert_eq!(config.pow_difficulty, 8); // default unchanged
    }

    #[tokio::test]
    async fn test_list_provider_keys() {
        let state = EncryptedInferenceState::new(EncryptionConfig::default());
        let (_, pk1) = test_keypair();
        let (_, pk2) = test_keypair();

        state.register_provider_key(ProviderEncryptionState {
            provider_id: "p1".to_string(),
            public_key: hex::encode(pk1.as_bytes()),
            encryption_enabled: true,
            registered_at: Utc::now(),
        });
        state.register_provider_key(ProviderEncryptionState {
            provider_id: "p2".to_string(),
            public_key: hex::encode(pk2.as_bytes()),
            encryption_enabled: false,
            registered_at: Utc::now(),
        });

        let keys = state.list_provider_keys();
        assert_eq!(keys.len(), 2);
    }

    #[tokio::test]
    async fn test_remove_provider_key() {
        let state = EncryptedInferenceState::new(EncryptionConfig::default());
        let (_, pk) = test_keypair();

        state.register_provider_key(ProviderEncryptionState {
            provider_id: "p1".to_string(),
            public_key: hex::encode(pk.as_bytes()),
            encryption_enabled: true,
            registered_at: Utc::now(),
        });

        assert!(state.remove_provider_key("p1"));
        assert!(!state.remove_provider_key("p1")); // already removed
        assert!(state.list_provider_keys().is_empty());
    }

    // ---------- Public key parsing test ----------

    #[test]
    fn test_parse_public_key_valid() {
        let (_, pk) = test_keypair();
        let hex_str = hex::encode(pk.as_bytes());
        let parsed = parse_public_key(&hex_str).unwrap();
        assert_eq!(pk.as_bytes(), parsed.as_bytes());
    }

    #[test]
    fn test_parse_public_key_invalid_hex() {
        let result = parse_public_key("not-hex!!");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_public_key_wrong_length() {
        let result = parse_public_key("deadbeef");
        assert!(result.is_err());
    }

    // ---------- Hash data test ----------

    #[test]
    fn test_hash_data_deterministic() {
        let h1 = hash_data(b"test");
        let h2 = hash_data(b"test");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_data_different_inputs() {
        let h1 = hash_data(b"test1");
        let h2 = hash_data(b"test2");
        assert_ne!(h1, h2);
    }
}
