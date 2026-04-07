//! ErgoAuth JWT verification and provider auto-registration.
//!
//! Implements the ErgoAuth protocol (EIP-12 based):
//!   1. dApp generates a challenge (random message + sigmaBoolean proving address ownership)
//!   2. Wallet signs the challenge (Schnorr signature proving key ownership)
//!   3. dApp verifies the signature, issues a JWT
//!
//! This module provides:
//!   - Challenge generation endpoint (`POST /v1/auth/ergoauth/challenge`)
//!   - Signature verification + JWT issuance (`POST /v1/auth/ergoauth/verify`)
//!   - JWT middleware for protecting endpoints
//!   - Provider auto-registration on first authenticated heartbeat

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use rand::RngCore;
use axum::extract::State;
use axum::response::IntoResponse;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use k256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
use k256::schnorr::{Signature as SchnorrSignature, VerifyingKey as SchnorrVerifyingKey};
use serde::{Deserialize, Serialize};

use crate::chain::client::ErgoNodeClient;
use crate::config::ProviderRegistryConfig;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// JWT token expiry in seconds (24 hours)
const JWT_EXPIRY_SECS: i64 = 86400;

/// Challenge expiry in seconds (5 minutes)
const CHALLENGE_EXPIRY_SECS: u64 = 300;

/// JWT issuer claim
const JWT_ISSUER: &str = "xergon-agent";

/// JWT audience claim
const JWT_AUDIENCE: &str = "xergon-api";

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// JWT claims issued by the agent after successful ErgoAuth verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErgoAuthClaims {
    /// Issued at (unix timestamp)
    pub iat: i64,
    /// Expiration (unix timestamp)
    pub exp: i64,
    /// Issuer
    pub iss: String,
    /// Audience
    pub aud: String,
    /// Subject — the Ergo address of the authenticated user
    pub sub: String,
    /// Provider public key (hex, 33 bytes compressed)
    pub provider_pk: String,
}

/// Challenge request from the client.
#[derive(Debug, Deserialize)]
pub struct ChallengeRequest {
    /// The Ergo address of the wallet requesting authentication.
    pub address: String,
}

/// Challenge response sent to the client.
#[derive(Debug, Serialize, Deserialize)]
pub struct ChallengeResponse {
    /// Random challenge message (hex-encoded)
    pub challenge: String,
    /// SigmaBoolean proving address ownership (hex-encoded)
    pub sigma_boolean: String,
    /// Challenge expiry (unix timestamp)
    pub expires_at: u64,
}

/// Verify request from the client (wallet-signed challenge).
#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    /// The Ergo address of the authenticating wallet
    pub address: String,
    /// The original challenge message (hex-encoded, from ChallengeResponse)
    pub challenge: String,
    /// The sigmaBoolean from the challenge (hex-encoded)
    pub sigma_boolean: String,
    /// Schnorr signature proving key ownership (hex-encoded, 64 bytes)
    pub proof: String,
    /// The signing message (the full message that was signed)
    pub signing_message: String,
    /// Provider's compressed secp256k1 public key (hex, 33 bytes).
    /// Used for auto-registration if provided.
    #[serde(default)]
    pub provider_pk_hex: String,
}

/// Verify response — JWT token and provider registration info.
#[derive(Debug, Serialize)]
pub struct VerifyResponse {
    /// JWT access token
    pub token: String,
    /// Token type
    pub token_type: String,
    /// Expiry in seconds
    pub expires_in: i64,
    /// Provider registration result (if auto-registered)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registration: Option<ProviderRegistrationInfo>,
}

/// Provider auto-registration info returned on first auth.
#[derive(Debug, Serialize)]
pub struct ProviderRegistrationInfo {
    pub tx_id: String,
    pub provider_nft_id: String,
    pub provider_box_id: String,
}

/// In-memory pending challenges store.
/// Key: challenge hex, Value: (address, expires_at)
pub struct ChallengeStore {
    /// Inner store: challenge_hex -> (address, expires_at_unix)
    entries: dashmap::DashMap<String, (String, u64)>,
    /// Periodic cleanup interval (in seconds)
    cleanup_interval_secs: u64,
}

impl ChallengeStore {
    /// Create a new challenge store.
    pub fn new() -> Self {
        Self {
            entries: dashmap::DashMap::new(),
            cleanup_interval_secs: 60,
        }
    }

    /// Store a new challenge.
    pub fn insert(&self, challenge_hex: &str, address: &str, expires_at: u64) {
        self.entries
            .insert(challenge_hex.to_string(), (address.to_string(), expires_at));
    }

    /// Retrieve and consume a challenge. Returns None if not found or expired.
    pub fn take(&self, challenge_hex: &str) -> Option<(String, u64)> {
        let entry = self.entries.remove(challenge_hex)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if entry.1 .1 < now {
            return None; // expired
        }
        Some(entry.1)
    }

    /// Clean up expired challenges.
    pub fn cleanup(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.entries.retain(|_, (_, expires_at)| *expires_at >= now);
    }

    /// Start a background task that periodically cleans up expired challenges.
    pub fn start_cleanup_task(store: Arc<Self>) {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(store.cleanup_interval_secs))
                    .await;
                store.cleanup();
            }
        });
    }
}

impl Default for ChallengeStore {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Shared auth state
// ---------------------------------------------------------------------------

/// Shared auth state for the axum router.
#[derive(Clone)]
pub struct AuthState {
    /// JWT secret key for signing tokens.
    /// In production this should be loaded from config/env.
    pub jwt_secret: Arc<String>,
    /// Pending challenges store.
    pub challenges: Arc<ChallengeStore>,
    /// Ergo node URL (for provider auto-registration).
    pub ergo_node_url: String,
    /// Provider registry config (Some if auto-registration is enabled).
    pub provider_registry_config: Option<Arc<ProviderRegistryConfig>>,
}

// ---------------------------------------------------------------------------
// Challenge generation
// ---------------------------------------------------------------------------

/// Generate a random ErgoAuth challenge.
///
/// The challenge consists of:
/// - A random 32-byte nonce (hex-encoded)
/// - A sigmaBoolean placeholder (for ErgoAuth, this proves address ownership)
///
/// The challenge is stored in the ChallengeStore with an expiry time.
pub fn generate_challenge(
    address: &str,
    store: &ChallengeStore,
) -> Result<ChallengeResponse> {
    let mut nonce = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut nonce);

    let challenge_hex = hex::encode(nonce);

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let expires_at = now + CHALLENGE_EXPIRY_SECS;

    // SigmaBoolean: for ErgoAuth, we use the address's PK as a placeholder.
    // In a full implementation, this would be the serialized sigmaBoolean from
    // the Ergo address (P2PK or P2S). For MVP, we use a placeholder.
    let sigma_boolean = address_to_sigma_boolean(address);

    store.insert(&challenge_hex, address, expires_at);

    Ok(ChallengeResponse {
        challenge: challenge_hex,
        sigma_boolean,
        expires_at,
    })
}

/// Convert an Ergo address to a sigmaBoolean hex placeholder.
///
/// For P2PK addresses (3...), extracts the PK bytes.
/// For P2S addresses (...), uses the script hash.
/// Falls back to blake2b256 of the address string.
fn address_to_sigma_boolean(address: &str) -> String {
    // Ergo P2PK addresses start with '3' and encode a compressed PK
    if address.starts_with('3') && address.len() >= 3 {
        // Decode base58 and extract the PK
        if let Ok(decoded) = bs58::decode(address).into_vec() {
            // Ergo P2PK format: 1 byte version + 32 byte PK + 4 byte checksum
            if decoded.len() >= 33 {
                // Skip first byte (network type/version), take 32 bytes of PK
                return hex::encode(&decoded[1..33]);
            }
        }
    }

    // Fallback: hash the address
    let hash = crate::wallet::blake2b256(address.as_bytes());
    hex::encode(hash)
}

// ---------------------------------------------------------------------------
// Signature verification
// ---------------------------------------------------------------------------

/// Verify an ErgoAuth Schnorr signature.
///
/// The ErgoAuth protocol uses Schnorr signatures on secp256k1.
/// The signature is 64 bytes (r || s) and the public key is 33 bytes (compressed).
///
/// The verification process:
/// 1. Recover the public key from the address (or use provided provider_pk_hex)
/// 2. Construct the message to verify (signing_message + challenge)
/// 3. Verify the Schnorr signature against the public key
pub fn verify_ergoauth_signature(
    proof_hex: &str,
    signing_message: &str,
    challenge_hex: &str,
    public_key_bytes: &[u8],
) -> Result<()> {
    // Decode the Schnorr signature (64 bytes: r || s)
    let proof_bytes = hex::decode(proof_hex)
        .context("Invalid proof hex encoding")?;
    if proof_bytes.len() != 64 {
        bail!(
            "Schnorr proof must be 64 bytes, got {} bytes",
            proof_bytes.len()
        );
    }

    // Parse the Schnorr signature (use TryFrom for k256 0.13)
    let signature = SchnorrSignature::try_from(proof_bytes.as_slice())
        .map_err(|e| anyhow::anyhow!("Invalid Schnorr signature: {}", e))?;

    // Parse the verifying key (compressed secp256k1 public key)
    let verifying_key = SchnorrVerifyingKey::from_bytes(public_key_bytes)
        .map_err(|e| anyhow::anyhow!("Invalid public key: {}", e))?;

    // Construct the message that was signed
    // In ErgoAuth, the signing message includes the challenge
    let message = format!("{}{}", signing_message, challenge_hex);

    // Hash the message before verification (Ergo uses blake2b256)
    let message_hash = crate::wallet::blake2b256(message.as_bytes());

    // Verify the signature
    verifying_key
        .verify(&message_hash, &signature)
        .map_err(|e| anyhow::anyhow!("Signature verification failed: {}", e))?;

    Ok(())
}

/// Verify an ECDSA signature (alternative verification mode).
///
/// Some ErgoAuth implementations use ECDSA instead of Schnorr.
pub fn verify_ecdsa_signature(
    proof_hex: &str,
    message: &[u8],
    public_key_bytes: &[u8],
) -> Result<()> {
    let proof_bytes = hex::decode(proof_hex)
        .context("Invalid proof hex encoding")?;

    let signature = Signature::from_slice(&proof_bytes)
        .map_err(|e| anyhow::anyhow!("Invalid ECDSA signature: {}", e))?;

    let verifying_key = VerifyingKey::from_sec1_bytes(public_key_bytes)
        .map_err(|e| anyhow::anyhow!("Invalid public key: {}", e))?;

    verifying_key
        .verify(message, &signature)
        .map_err(|e| anyhow::anyhow!("ECDSA signature verification failed: {}", e))?;

    Ok(())
}

/// Extract the public key from an Ergo P2PK address.
///
/// Returns the 33-byte compressed secp256k1 public key.
pub fn public_key_from_address(address: &str) -> Result<[u8; 33]> {
    if !address.starts_with('3') {
        bail!("Not a P2PK address (must start with '3')");
    }

    let decoded = bs58::decode(address)
        .into_vec()
        .context("Invalid base58 encoding in address")?;

    if decoded.len() < 34 {
        bail!(
            "Address too short: {} bytes (need at least 34)",
            decoded.len()
        );
    }

    let mut pk = [0u8; 33];
    pk.copy_from_slice(&decoded[1..34]);
    Ok(pk)
}

// ---------------------------------------------------------------------------
// JWT generation and validation
// ---------------------------------------------------------------------------

/// Generate a JWT token for an authenticated user.
pub fn generate_jwt(
    jwt_secret: &str,
    address: &str,
    provider_pk: &str,
) -> Result<(String, i64)> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let claims = ErgoAuthClaims {
        iat: now,
        exp: now + JWT_EXPIRY_SECS,
        iss: JWT_ISSUER.to_string(),
        aud: JWT_AUDIENCE.to_string(),
        sub: address.to_string(),
        provider_pk: provider_pk.to_string(),
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret.as_bytes()),
    )
    .context("Failed to generate JWT")?;

    Ok((token, JWT_EXPIRY_SECS))
}

/// Validate a JWT token and return the claims.
pub fn validate_jwt(jwt_secret: &str, token: &str) -> Result<ErgoAuthClaims> {
    let mut validation = Validation::default();
    validation.set_issuer(&[JWT_ISSUER]);
    validation.set_audience(&[JWT_AUDIENCE]);

    let token_data = decode::<ErgoAuthClaims>(
        token,
        &DecodingKey::from_secret(jwt_secret.as_bytes()),
        &validation,
    )
    .context("Invalid JWT token")?;

    Ok(token_data.claims)
}

/// Extract the Bearer token from the Authorization header.
pub fn extract_bearer_token(auth_header: Option<&str>) -> Option<String> {
    let header = auth_header?.strip_prefix("Bearer ").or_else(|| {
        auth_header?.strip_prefix("bearer ")
    })?;
    if header.is_empty() {
        None
    } else {
        Some(header.to_string())
    }
}

// ---------------------------------------------------------------------------
// Provider auto-registration
// ---------------------------------------------------------------------------

/// Attempt to auto-register a provider on first authentication.
///
/// Checks if a provider box already exists for the given PK, and if not,
/// registers using the config defaults and the provided endpoint info.
pub async fn auto_register_provider(
    ergo_node_url: &str,
    config: &ProviderRegistryConfig,
    provider_pk_hex: &str,
    provider_name: &str,
    endpoint_url: &str,
) -> Result<Option<crate::provider_registry::ProviderRegistrationResult>> {
    let client = ErgoNodeClient::new(ergo_node_url.to_string());

    let result = crate::provider_registry::auto_register_if_needed(
        &client,
        config,
        provider_name,
        endpoint_url,
        provider_pk_hex,
        config.price_per_token,
    )
    .await;

    match result {
        Ok(Some(reg)) => Ok(Some(reg)),
        Ok(None) => Ok(None), // already registered
        Err(e) => {
            tracing::warn!(
                error = %e,
                provider_pk = %provider_pk_hex,
                "Auto-registration failed (non-fatal)"
            );
            Ok(None)
        }
    }
}

// ---------------------------------------------------------------------------
// Full verification flow
// ---------------------------------------------------------------------------

/// Process a full ErgoAuth verification and return a JWT + optional registration.
///
/// This is the main entry point for the verify endpoint.
pub async fn process_ergoauth_verify(
    auth_state: &AuthState,
    req: VerifyRequest,
) -> Result<VerifyResponse> {
    // 1. Validate the challenge
    let (stored_address, _expires_at) = auth_state
        .challenges
        .take(&req.challenge)
        .context("Challenge not found or expired")?;

    if stored_address != req.address {
        bail!("Challenge address mismatch");
    }

    // 2. Determine the public key
    let pk_bytes = if !req.provider_pk_hex.is_empty() {
        let bytes = hex::decode(&req.provider_pk_hex)
            .context("Invalid provider_pk_hex")?;
        if bytes.len() != 33 {
            bail!("provider_pk_hex must be 33 bytes (compressed secp256k1)");
        }
        let mut arr = [0u8; 33];
        arr.copy_from_slice(&bytes);
        arr
    } else {
        // Try to extract PK from the Ergo address
        public_key_from_address(&req.address)?
    };

    let pk_hex = hex::encode(pk_bytes);

    // 3. Verify the signature
    // Try Schnorr first (standard ErgoAuth), fall back to ECDSA
    let verify_result = verify_ergoauth_signature(
        &req.proof,
        &req.signing_message,
        &req.challenge,
        &pk_bytes,
    );

    if let Err(e) = verify_result {
        // Try ECDSA as fallback
        let message = format!("{}{}", req.signing_message, req.challenge);
        if let Err(ecdsa_err) =
            verify_ecdsa_signature(&req.proof, message.as_bytes(), &pk_bytes)
        {
            bail!(
                "Both Schnorr and ECDSA verification failed. Schnorr: {}, ECDSA: {}",
                e,
                ecdsa_err
            );
        }
    }

    tracing::info!(
        address = %req.address,
        provider_pk = %pk_hex,
        "ErgoAuth verification successful"
    );

    // 4. Generate JWT
    let (token, expires_in) = generate_jwt(
        &auth_state.jwt_secret,
        &req.address,
        &pk_hex,
    )?;

    // 5. Attempt auto-registration if provider_pk_hex was provided
    let registration = if !req.provider_pk_hex.is_empty() {
        if let Some(config) = &auth_state.provider_registry_config {
            if config.auto_register {
                match auto_register_provider(
                    &auth_state.ergo_node_url,
                    config,
                    &pk_hex,
                    &req.address, // use address as name placeholder
                    "",           // endpoint not known yet
                )
                .await
                {
                    Ok(Some(reg)) => Some(ProviderRegistrationInfo {
                        tx_id: reg.tx_id,
                        provider_nft_id: reg.provider_nft_id,
                        provider_box_id: reg.provider_box_id,
                    }),
                    Ok(None) => None,
                    Err(e) => {
                        tracing::warn!(error = %e, "Auto-registration failed");
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    Ok(VerifyResponse {
        token,
        token_type: "Bearer".to_string(),
        expires_in,
        registration,
    })
}

// ---------------------------------------------------------------------------
// Axum JWT middleware
// ---------------------------------------------------------------------------

/// Axum middleware that validates JWT tokens on protected routes.
///
/// Extracts the `Authorization: Bearer <token>` header, validates the JWT,
/// and injects the claims into request extensions.
pub async fn jwt_auth_middleware(
    State(auth_state): State<AuthState>,
    req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    let token = match extract_bearer_token(auth_header) {
        Some(t) => t,
        None => {
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                axum::Json(serde_json::json!({
                    "error": {
                        "type": "unauthorized",
                        "message": "Missing or invalid Authorization header",
                        "code": 401,
                    }
                })),
            )
                .into_response();
        }
    };

    match validate_jwt(&auth_state.jwt_secret, &token) {
        Ok(claims) => {
            // Inject claims into request extensions
            let mut req = req;
            req.extensions_mut().insert(claims);
            next.run(req).await
        }
        Err(e) => (
            axum::http::StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({
                "error": {
                    "type": "unauthorized",
                    "message": format!("Invalid token: {}", e),
                    "code": 401,
                }
            })),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use k256::ecdsa::{signature::Signer, SigningKey};
    use k256::schnorr::{SigningKey as SchnorrSigningKey};

    /// Helper: create a deterministic JWT secret for testing.
    fn test_jwt_secret() -> String {
        "test-secret-key-for-xergon-auth-jwt-tokens-32bytes!".to_string()
    }

    // ---- Challenge generation tests ----

    #[test]
    fn test_generate_challenge_returns_valid_hex() {
        let store = ChallengeStore::new();
        let resp = generate_challenge("3Wwx...abc", &store).unwrap();

        // Challenge should be 64 hex chars (32 bytes)
        assert_eq!(resp.challenge.len(), 64);

        // SigmaBoolean should be non-empty hex
        assert!(!resp.sigma_boolean.is_empty());

        // Should be stored in the store
        let retrieved = store.take(&resp.challenge).unwrap();
        assert_eq!(retrieved.0, "3Wwx...abc");
    }

    #[test]
    fn test_challenge_store_take_consumes_entry() {
        let store = ChallengeStore::new();
        store.insert("abcdef", "address1", u64::MAX);

        let first = store.take("abcdef");
        assert!(first.is_some());

        let second = store.take("abcdef");
        assert!(second.is_none()); // consumed
    }

    #[test]
    fn test_challenge_store_expired() {
        let store = ChallengeStore::new();
        // Use a past timestamp
        store.insert("old_challenge", "address1", 1000); // way in the past

        let result = store.take("old_challenge");
        assert!(result.is_none()); // expired
    }

    #[test]
    fn test_address_to_sigma_boolean_p2pk() {
        // Use a realistic-looking P2PK address (starts with '3')
        let address = "3WwxSzKgA3UByVpi7VhycMMYd2uGGBaCYV";
        let sb = address_to_sigma_boolean(address);

        // Should return hex of the decoded PK
        assert!(!sb.is_empty());
        // Hex should be 64 chars (32 bytes)
        assert_eq!(sb.len(), 64);
    }

    #[test]
    fn test_address_to_sigma_boolean_fallback() {
        let address = "not-a-real-address";
        let sb = address_to_sigma_boolean(address);

        // Should return blake2b256 hash (64 hex chars)
        assert!(!sb.is_empty());
        assert_eq!(sb.len(), 64);
    }

    // ---- JWT generation and validation tests ----

    #[test]
    fn test_jwt_generate_and_validate() {
        let secret = test_jwt_secret();
        let address = "3WwxSzKgA3UByVpi7VhycMMYd2uGGBaCYV";
        let pk_hex = "02aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

        let (token, expires_in) = generate_jwt(&secret, address, pk_hex).unwrap();

        assert!(!token.is_empty());
        assert_eq!(expires_in, JWT_EXPIRY_SECS);

        // Validate the token
        let claims = validate_jwt(&secret, &token).unwrap();
        assert_eq!(claims.sub, address);
        assert_eq!(claims.provider_pk, pk_hex);
        assert_eq!(claims.iss, JWT_ISSUER);
        assert_eq!(claims.aud, JWT_AUDIENCE);
        assert!(claims.exp > claims.iat);
    }

    #[test]
    fn test_jwt_invalid_secret_fails() {
        let secret = test_jwt_secret();
        let (token, _) = generate_jwt(&secret, "test_addr", "test_pk").unwrap();

        // Try to validate with wrong secret
        let result = validate_jwt("wrong-secret", &token);
        assert!(result.is_err());
    }

    #[test]
    fn test_jwt_expired_fails() {
        let secret = test_jwt_secret();
        let address = "test_addr";
        let pk_hex = "test_pk";

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let claims = ErgoAuthClaims {
            iat: now - JWT_EXPIRY_SECS - 100, // issued in the past
            exp: now - 120,                    // expired well beyond default leeway
            iss: JWT_ISSUER.to_string(),
            aud: JWT_AUDIENCE.to_string(),
            sub: address.to_string(),
            provider_pk: pk_hex.to_string(),
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap();

        let result = validate_jwt(&secret, &token);
        assert!(result.is_err());
    }

    // ---- Bearer token extraction ----

    #[test]
    fn test_extract_bearer_token_valid() {
        let token = extract_bearer_token(Some("Bearer my-jwt-token"));
        assert_eq!(token, Some("my-jwt-token".to_string()));
    }

    #[test]
    fn test_extract_bearer_token_lowercase() {
        let token = extract_bearer_token(Some("bearer my-jwt-token"));
        assert_eq!(token, Some("my-jwt-token".to_string()));
    }

    #[test]
    fn test_extract_bearer_token_no_header() {
        let token = extract_bearer_token(None);
        assert!(token.is_none());
    }

    #[test]
    fn test_extract_bearer_token_empty() {
        let token = extract_bearer_token(Some("Bearer "));
        assert!(token.is_none());
    }

    // ---- ECDSA signature verification tests ----

    #[test]
    fn test_ecdsa_sign_and_verify_roundtrip() {
        // Generate a random keypair
        let signing_key = SigningKey::random(&mut rand::rngs::OsRng);
        let verifying_key_bytes = signing_key.verifying_key().to_sec1_bytes();

        // Sign a message
        let message = b"Hello, ErgoAuth!";
        let signature: Signature = signing_key.sign(message);
        let proof_hex = hex::encode(signature.to_bytes());

        // Verify
        let result = verify_ecdsa_signature(&proof_hex, message, &verifying_key_bytes);
        assert!(result.is_ok());
    }

    #[test]
    fn test_ecdsa_wrong_message_fails() {
        let signing_key = SigningKey::random(&mut rand::rngs::OsRng);
        let verifying_key_bytes = signing_key.verifying_key().to_sec1_bytes();

        let signature: Signature = signing_key.sign(b"correct message");
        let proof_hex = hex::encode(signature.to_bytes());

        // Try to verify with wrong message
        let result = verify_ecdsa_signature(&proof_hex, b"wrong message", &verifying_key_bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_ecdsa_wrong_key_fails() {
        let signing_key = SigningKey::random(&mut rand::rngs::OsRng);
        let other_key = SigningKey::random(&mut rand::rngs::OsRng);
        let other_vk_bytes = other_key.verifying_key().to_sec1_bytes();

        let signature: Signature = signing_key.sign(b"message");
        let proof_hex = hex::encode(signature.to_bytes());

        // Verify with wrong key
        let result = verify_ecdsa_signature(&proof_hex, b"message", &other_vk_bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_ecdsa_invalid_proof_hex() {
        let vk_bytes = [0x02u8; 33]; // dummy key
        let result = verify_ecdsa_signature("zzzz", b"msg", &vk_bytes);
        assert!(result.is_err());
    }

    // ---- Schnorr signature verification tests ----

    #[test]
    fn test_schnorr_sign_and_verify_roundtrip() {
        // Generate a random Schnorr keypair
        use k256::schnorr::signature::Signer;
        let signing_key = SchnorrSigningKey::random(&mut rand::rngs::OsRng);
        let verifying_key_bytes = signing_key.verifying_key().to_bytes();

        // Sign a message hash
        // verify_ergoauth_signature computes: blake2b256((signing_message + challenge).as_bytes())
        // so we must sign that same hash
        let message = b"Hello, ErgoAuth Schnorr!";
        let signing_msg = hex::encode(message);
        let challenge_hex = hex::encode(message);
        let verify_input = format!("{}{}", signing_msg, challenge_hex);
        let message_hash = crate::wallet::blake2b256(verify_input.as_bytes());
        let signature: SchnorrSignature = signing_key.sign(&message_hash);
        let proof_hex = hex::encode(signature.to_bytes());

        // Verify
        let result = verify_ergoauth_signature(
            &proof_hex,
            &signing_msg,
            &challenge_hex,
            &verifying_key_bytes,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_schnorr_wrong_message_fails() {
        use k256::schnorr::signature::Signer;
        let signing_key = SchnorrSigningKey::random(&mut rand::rngs::OsRng);
        let verifying_key_bytes = signing_key.verifying_key().to_bytes();

        let message_hash = crate::wallet::blake2b256(b"correct message");
        let signature: SchnorrSignature = signing_key.sign(&message_hash);
        let proof_hex = hex::encode(signature.to_bytes());

        // Verify with wrong message hash
        let wrong_hash = crate::wallet::blake2b256(b"wrong message");
        let vk = SchnorrVerifyingKey::from_bytes(&verifying_key_bytes).unwrap();
        let result = vk.verify(&wrong_hash, &signature);
        assert!(result.is_err());
    }

    #[test]
    fn test_schnorr_invalid_proof_length() {
        let vk_bytes = [0x02u8; 33]; // dummy
        let result = verify_ergoauth_signature(
            "abcdef", // too short
            "signing_msg",
            "challenge",
            &vk_bytes,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("64 bytes"));
    }

    // ---- Claims serialization ----

    #[test]
    fn test_claims_serialization_roundtrip() {
        let claims = ErgoAuthClaims {
            iat: 1700000000,
            exp: 1700086400,
            iss: JWT_ISSUER.to_string(),
            aud: JWT_AUDIENCE.to_string(),
            sub: "3WwxSzKgA3UByVpi7VhycMMYd2uGGBaCYV".to_string(),
            provider_pk: "02abcdef".to_string(),
        };

        let json = serde_json::to_string(&claims).unwrap();
        let parsed: ErgoAuthClaims = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.sub, claims.sub);
        assert_eq!(parsed.provider_pk, claims.provider_pk);
        assert_eq!(parsed.iss, claims.iss);
    }

    // ---- Challenge store cleanup ----

    #[test]
    fn test_challenge_store_cleanup() {
        let store = ChallengeStore::new();

        // Insert one valid and one expired
        let far_future = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            + 3600;
        let far_past = 1000u64;

        store.insert("valid_challenge", "addr1", far_future);
        store.insert("expired_challenge", "addr2", far_past);

        store.cleanup();

        // Valid should still exist
        assert!(store.take("valid_challenge").is_some());
        // Expired should be gone
        assert!(store.take("expired_challenge").is_none());
    }

    // ---- Verify request/response serialization ----

    #[test]
    fn test_verify_request_deserialization() {
        let json = r#"{
            "address": "3WwxSzKgA3UByVpi7VhycMMYd2uGGBaCYV",
            "challenge": "abcdef1234567890",
            "sigma_boolean": "02abcdef",
            "proof": "aabbccddaabbccddaabbccddaabbccddaabbccddaabbccddaabbccdd",
            "signing_message": "ErgoAuth login",
            "provider_pk_hex": "02aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        }"#;

        let req: VerifyRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.address, "3WwxSzKgA3UByVpi7VhycMMYd2uGGBaCYV");
        assert!(!req.provider_pk_hex.is_empty());
    }

    #[test]
    fn test_verify_response_serialization() {
        let resp = VerifyResponse {
            token: "eyJhbGciOiJIUzI1NiJ9.test".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: 86400,
            registration: None,
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("Bearer"));
        assert!(!json.contains("registration"));
    }

    #[test]
    fn test_verify_response_with_registration() {
        let resp = VerifyResponse {
            token: "eyJhbGciOiJIUzI1NiJ9.test".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: 86400,
            registration: Some(ProviderRegistrationInfo {
                tx_id: "abc123".to_string(),
                provider_nft_id: "nft456".to_string(),
                provider_box_id: "box789".to_string(),
            }),
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("registration"));
        assert!(json.contains("abc123"));
        assert!(json.contains("nft456"));
    }

    #[test]
    fn test_challenge_request_deserialization() {
        let json = r#"{"address": "3WwxSzKgA3UByVpi7VhycMMYd2uGGBaCYV"}"#;
        let req: ChallengeRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.address, "3WwxSzKgA3UByVpi7VhycMMYd2uGGBaCYV");
    }

    #[test]
    fn test_challenge_response_serialization() {
        let resp = ChallengeResponse {
            challenge: "aabbccdd".repeat(8),
            sigma_boolean: "02abcdef".repeat(4),
            expires_at: 1700086400,
        };

        let json = serde_json::to_string(&resp).unwrap();
        let parsed: ChallengeResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.challenge, resp.challenge);
        assert_eq!(parsed.sigma_boolean, resp.sigma_boolean);
        assert_eq!(parsed.expires_at, resp.expires_at);
    }
}
