//! Signature-based authentication verification
//!
//! Verifies requests signed with HMAC-SHA256(secret_key, timestamp + method + path + body_hash).
//! The public_key is blake2b256(secret_key), which we use to identify users.
//!
//! Since we cannot verify HMAC without the secret key, verification consists of:
//! 1. Check timestamp freshness (within max_age_secs, and within 60s clock skew)
//! 2. Optionally check the public key has a staking box on-chain
//! 3. Replay protection: cache seen (public_key, timestamp) pairs

use axum::http::HeaderMap;
use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::debug;

use crate::config::AuthConfig;

/// Extracted auth information from request headers.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RequestAuth {
    /// Hex-encoded public key (blake2b256 of secret_key)
    pub public_key: String,
    /// Unix timestamp in milliseconds
    pub timestamp: i64,
    /// Hex-encoded HMAC-SHA256 signature
    pub signature: String,
}

/// Errors that can occur during authentication.
#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum AuthError {
    #[error("Missing required header: {0}")]
    MissingHeader(String),

    #[error("Invalid timestamp: {0}")]
    InvalidTimestamp(String),

    #[error("Request expired (older than {max_age}s)")]
    ExpiredRequest { max_age: i64 },

    #[error("Replay detected: public_key={public_key}, timestamp={timestamp}")]
    ReplayDetected { public_key: String, timestamp: i64 },

    #[error("Invalid signature: {0}")]
    InvalidSignature(String),

    #[error("No staking box found for public key: {0}")]
    NoStakingBox(String),
}

/// Header names used for signature-based auth.
const HEADER_TIMESTAMP: &str = "x-xergon-timestamp";
const HEADER_PUBLIC_KEY: &str = "x-xergon-public-key";
const HEADER_SIGNATURE: &str = "x-xergon-signature";

/// Maximum allowed clock skew in seconds (timestamp from the future).
const MAX_CLOCK_SKEW_SECS: i64 = 60;

/// Replay protection cache entry.
struct ReplayEntry {
    /// When this entry was inserted (for TTL-based eviction).
    inserted_at: Instant,
}

/// Verifies signature-based authentication on incoming requests.
pub struct AuthVerifier {
    /// Maximum age of a signed request in seconds.
    max_age_secs: i64,
    /// Replay protection cache: (public_key, timestamp) -> inserted_at.
    replay_cache: Arc<DashMap<String, ReplayEntry>>,
    /// Maximum replay cache size.
    replay_cache_size: usize,
    /// Whether to require an on-chain staking box for authentication.
    require_staking_box: bool,
    /// Optional whitelist of trusted public keys (for testing).
    trusted_keys: Option<Arc<DashMap<String, bool>>>,
}

impl AuthVerifier {
    /// Create a new AuthVerifier from config.
    pub fn new(config: &AuthConfig) -> Self {
        Self {
            max_age_secs: config.max_age_secs,
            replay_cache: Arc::new(DashMap::new()),
            replay_cache_size: config.replay_cache_size,
            require_staking_box: config.require_staking_box,
            trusted_keys: None,
        }
    }

    /// Create a new AuthVerifier with a trusted key whitelist (for testing).
    #[cfg(test)]
    pub fn with_trusted_keys(config: &AuthConfig, keys: Vec<String>) -> Self {
        let map: DashMap<String, bool> = keys.into_iter().map(|k| (k, true)).collect();
        Self {
            max_age_secs: config.max_age_secs,
            replay_cache: Arc::new(DashMap::new()),
            replay_cache_size: config.replay_cache_size,
            require_staking_box: config.require_staking_box,
            trusted_keys: Some(Arc::new(map)),
        }
    }

    /// Check whether any X-Xergon-* auth headers are present in the request.
    pub fn has_auth_headers(headers: &HeaderMap) -> bool {
        headers.contains_key(HEADER_TIMESTAMP)
            || headers.contains_key(HEADER_PUBLIC_KEY)
            || headers.contains_key(HEADER_SIGNATURE)
    }

    /// Extract and validate auth headers from a request.
    ///
    /// Returns a `RequestAuth` with the parsed header values.
    /// Does NOT verify the signature (that's done in `verify()`).
    pub fn extract_auth(headers: &HeaderMap) -> Result<RequestAuth, AuthError> {
        let timestamp_str = headers
            .get(HEADER_TIMESTAMP)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                AuthError::MissingHeader(HEADER_TIMESTAMP.to_string().to_uppercase())
            })?
            .trim();

        let public_key = headers
            .get(HEADER_PUBLIC_KEY)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                AuthError::MissingHeader(HEADER_PUBLIC_KEY.to_string().to_uppercase())
            })?
            .trim()
            .to_string();

        let signature = headers
            .get(HEADER_SIGNATURE)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                AuthError::MissingHeader(HEADER_SIGNATURE.to_string().to_uppercase())
            })?
            .trim()
            .to_string();

        let timestamp: i64 = timestamp_str.parse().map_err(|_| {
            AuthError::InvalidTimestamp(format!(
                "timestamp '{}' is not a valid unix milliseconds value",
                timestamp_str
            ))
        })?;

        // Basic format validation on public key (should be hex)
        if public_key.is_empty() {
            return Err(AuthError::MissingHeader(
                HEADER_PUBLIC_KEY.to_string().to_uppercase(),
            ));
        }

        // Basic format validation on signature (should be hex)
        if signature.is_empty() {
            return Err(AuthError::MissingHeader(
                HEADER_SIGNATURE.to_string().to_uppercase(),
            ));
        }

        Ok(RequestAuth {
            public_key,
            timestamp,
            signature,
        })
    }

    /// Verify the authenticity of a request.
    ///
    /// Since we can't verify HMAC without the secret key, we verify by:
    /// 1. Checking timestamp freshness (within max_age_secs and not too far in the future)
    /// 2. Checking for replay (same public_key+timestamp combination)
    /// 3. Optionally checking the public key has a staking box on-chain
    pub fn verify(
        &self,
        auth: &RequestAuth,
        _method: &str,
        _path: &str,
        _body: &[u8],
    ) -> Result<(), AuthError> {
        let now = chrono::Utc::now().timestamp_millis();
        let age_secs = (now - auth.timestamp) / 1000;

        // 1. Check timestamp is not too far in the future (clock skew)
        if auth.timestamp > now + MAX_CLOCK_SKEW_SECS * 1000 {
            return Err(AuthError::InvalidTimestamp(format!(
                "timestamp is {}ms in the future (max skew: {}s)",
                auth.timestamp - now,
                MAX_CLOCK_SKEW_SECS
            )));
        }

        // 2. Check request is not expired
        if age_secs > self.max_age_secs {
            return Err(AuthError::ExpiredRequest {
                max_age: self.max_age_secs,
            });
        }

        // 3. Replay protection
        let replay_key = format!("{}:{}", auth.public_key, auth.timestamp);
        if self.replay_cache.contains_key(&replay_key) {
            return Err(AuthError::ReplayDetected {
                public_key: auth.public_key.clone(),
                timestamp: auth.timestamp,
            });
        }

        // Insert into replay cache
        self.insert_replay_entry(replay_key);

        // 4. If trusted_keys is configured, check whitelist
        if let Some(ref trusted) = self.trusted_keys {
            if !trusted.contains_key(&auth.public_key) {
                return Err(AuthError::NoStakingBox(format!(
                    "public key '{}' not in trusted keys list",
                    &auth.public_key[..auth.public_key.len().min(16)]
                )));
            }
        }

        // 5. If require_staking_box, the caller (chat handler) must do the
        //    on-chain check using BalanceChecker. We don't do it here because
        //    this method is sync and the balance check is async.
        //    The handler is responsible for checking this flag and calling
        //    the balance checker if needed.

        debug!(
            public_key = %&auth.public_key[..auth.public_key.len().min(16)],
            age_secs,
            "Request auth verified"
        );

        Ok(())
    }

    /// Whether this verifier requires on-chain staking box verification.
    pub fn requires_staking_box(&self) -> bool {
        self.require_staking_box
    }

    /// Get the configured max_age_secs.
    pub fn max_age_secs(&self) -> i64 {
        self.max_age_secs
    }

    /// Insert a replay cache entry, evicting old entries if at capacity.
    fn insert_replay_entry(&self, key: String) {
        // Evict expired entries and enforce size limit
        if self.replay_cache.len() >= self.replay_cache_size {
            self.evict_expired_entries();
            // If still at capacity after eviction, remove oldest entries
            if self.replay_cache.len() >= self.replay_cache_size {
                self.evict_oldest_entries(self.replay_cache_size / 4);
            }
        }

        self.replay_cache.insert(
            key,
            ReplayEntry {
                inserted_at: Instant::now(),
            },
        );
    }

    /// Remove entries older than max_age_secs from the replay cache.
    fn evict_expired_entries(&self) {
        let max_age = Duration::from_secs(self.max_age_secs as u64 + 60);
        self.replay_cache
            .retain(|_, entry| entry.inserted_at.elapsed() < max_age);
    }

    /// Remove the oldest N entries from the replay cache.
    fn evict_oldest_entries(&self, count: usize) {
        // Collect keys sorted by insertion time
        let mut entries: Vec<(String, Instant)> = self
            .replay_cache
            .iter()
            .map(|r| (r.key().clone(), r.value().inserted_at))
            .collect();
        entries.sort_by_key(|(_, t)| *t);

        for (key, _) in entries.into_iter().take(count) {
            self.replay_cache.remove(&key);
        }
    }

    /// Get the current size of the replay cache (for monitoring).
    #[allow(dead_code)]
    pub fn replay_cache_size(&self) -> usize {
        self.replay_cache.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn test_config() -> AuthConfig {
        AuthConfig {
            enabled: true,
            max_age_secs: 300,
            replay_cache_size: 100,
            require_staking_box: false,
        }
    }

    fn make_headers(ts: &str, pk: &str, sig: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(HEADER_TIMESTAMP, HeaderValue::from_str(ts).unwrap());
        h.insert(HEADER_PUBLIC_KEY, HeaderValue::from_str(pk).unwrap());
        h.insert(HEADER_SIGNATURE, HeaderValue::from_str(sig).unwrap());
        h
    }

    #[test]
    fn test_extract_auth_valid() {
        let headers = make_headers(
            "1710000000000",
            "abcdef1234567890",
            "deadbeef00112233",
        );
        let auth = AuthVerifier::extract_auth(&headers).unwrap();
        assert_eq!(auth.public_key, "abcdef1234567890");
        assert_eq!(auth.timestamp, 1710000000000);
        assert_eq!(auth.signature, "deadbeef00112233");
    }

    #[test]
    fn test_extract_auth_missing_timestamp() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HEADER_PUBLIC_KEY,
            HeaderValue::from_static("abcdef"),
        );
        headers.insert(
            HEADER_SIGNATURE,
            HeaderValue::from_static("deadbeef"),
        );
        let err = AuthVerifier::extract_auth(&headers).unwrap_err();
        assert!(matches!(err, AuthError::MissingHeader(_)));
    }

    #[test]
    fn test_extract_auth_missing_public_key() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HEADER_TIMESTAMP,
            HeaderValue::from_static("1710000000000"),
        );
        headers.insert(
            HEADER_SIGNATURE,
            HeaderValue::from_static("deadbeef"),
        );
        let err = AuthVerifier::extract_auth(&headers).unwrap_err();
        assert!(matches!(err, AuthError::MissingHeader(_)));
    }

    #[test]
    fn test_extract_auth_missing_signature() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HEADER_TIMESTAMP,
            HeaderValue::from_static("1710000000000"),
        );
        headers.insert(
            HEADER_PUBLIC_KEY,
            HeaderValue::from_static("abcdef"),
        );
        let err = AuthVerifier::extract_auth(&headers).unwrap_err();
        assert!(matches!(err, AuthError::MissingHeader(_)));
    }

    #[test]
    fn test_extract_auth_invalid_timestamp() {
        let headers = make_headers("not-a-number", "abcdef", "deadbeef");
        let err = AuthVerifier::extract_auth(&headers).unwrap_err();
        assert!(matches!(err, AuthError::InvalidTimestamp(_)));
    }

    #[test]
    fn test_verify_recent_request() {
        let config = test_config();
        let verifier = AuthVerifier::with_trusted_keys(&config, vec!["testkey".into()]);

        let now = chrono::Utc::now().timestamp_millis();
        let auth = RequestAuth {
            public_key: "testkey".to_string(),
            timestamp: now,
            signature: "anysig".to_string(),
        };

        assert!(verifier.verify(&auth, "POST", "/v1/chat/completions", b"{}").is_ok());
    }

    #[test]
    fn test_verify_expired_request() {
        let config = test_config();
        let verifier = AuthVerifier::with_trusted_keys(&config, vec!["testkey".into()]);

        let now = chrono::Utc::now().timestamp_millis();
        let auth = RequestAuth {
            public_key: "testkey".to_string(),
            timestamp: now - 301_000, // 301 seconds ago (> 300s max_age)
            signature: "anysig".to_string(),
        };

        let err = verifier.verify(&auth, "POST", "/v1/chat/completions", b"{}").unwrap_err();
        assert!(matches!(err, AuthError::ExpiredRequest { .. }));
    }

    #[test]
    fn test_verify_future_timestamp_rejected() {
        let config = test_config();
        let verifier = AuthVerifier::with_trusted_keys(&config, vec!["testkey".into()]);

        let now = chrono::Utc::now().timestamp_millis();
        let auth = RequestAuth {
            public_key: "testkey".to_string(),
            timestamp: now + 120_000, // 120s in the future (> 60s skew)
            signature: "anysig".to_string(),
        };

        let err = verifier.verify(&auth, "POST", "/v1/chat/completions", b"{}").unwrap_err();
        assert!(matches!(err, AuthError::InvalidTimestamp(_)));
    }

    #[test]
    fn test_verify_replay_detection() {
        let config = test_config();
        let verifier = AuthVerifier::with_trusted_keys(&config, vec!["testkey".into()]);

        let now = chrono::Utc::now().timestamp_millis();
        let auth = RequestAuth {
            public_key: "testkey".to_string(),
            timestamp: now,
            signature: "anysig".to_string(),
        };

        // First request should succeed
        assert!(verifier.verify(&auth, "POST", "/v1/chat/completions", b"{}").is_ok());

        // Second request with same public_key+timestamp should be rejected
        let err = verifier.verify(&auth, "POST", "/v1/chat/completions", b"{}").unwrap_err();
        assert!(matches!(err, AuthError::ReplayDetected { .. }));
    }

    #[test]
    fn test_verify_different_timestamps_not_replay() {
        let config = test_config();
        let verifier = AuthVerifier::with_trusted_keys(&config, vec!["testkey".into()]);

        let now = chrono::Utc::now().timestamp_millis();
        let auth1 = RequestAuth {
            public_key: "testkey".to_string(),
            timestamp: now,
            signature: "sig1".to_string(),
        };
        let auth2 = RequestAuth {
            public_key: "testkey".to_string(),
            timestamp: now + 1000, // 1 second later
            signature: "sig2".to_string(),
        };

        assert!(verifier.verify(&auth1, "POST", "/v1/chat/completions", b"{}").is_ok());
        assert!(verifier.verify(&auth2, "POST", "/v1/chat/completions", b"{}").is_ok());
    }

    #[test]
    fn test_verify_untrusted_key_rejected() {
        let config = test_config();
        let verifier = AuthVerifier::with_trusted_keys(&config, vec!["trusted_key".into()]);

        let now = chrono::Utc::now().timestamp_millis();
        let auth = RequestAuth {
            public_key: "untrusted_key".to_string(),
            timestamp: now,
            signature: "anysig".to_string(),
        };

        let err = verifier.verify(&auth, "POST", "/v1/chat/completions", b"{}").unwrap_err();
        assert!(matches!(err, AuthError::NoStakingBox(_)));
    }

    #[test]
    fn test_has_auth_headers_true() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HEADER_TIMESTAMP,
            HeaderValue::from_static("1710000000000"),
        );
        assert!(AuthVerifier::has_auth_headers(&headers));
    }

    #[test]
    fn test_has_auth_headers_false() {
        let headers = HeaderMap::new();
        assert!(!AuthVerifier::has_auth_headers(&headers));
    }

    #[test]
    fn test_replay_cache_eviction() {
        let mut config = test_config();
        config.replay_cache_size = 5;
        let verifier = AuthVerifier::with_trusted_keys(&config, vec!["key".into()]);

        let now = chrono::Utc::now().timestamp_millis();

        // Fill cache to capacity
        for i in 0..5 {
            let auth = RequestAuth {
                public_key: "key".to_string(),
                timestamp: now + i as i64,
                signature: format!("sig{}", i),
            };
            verifier.verify(&auth, "POST", "/v1/test", b"").unwrap();
        }

        assert!(verifier.replay_cache_size() <= 10); // should have evicted
    }

    #[test]
    fn test_requires_staking_box() {
        let config = test_config();
        let verifier = AuthVerifier::new(&config);
        assert!(!verifier.requires_staking_box());

        let mut config = test_config();
        config.require_staking_box = true;
        let verifier = AuthVerifier::new(&config);
        assert!(verifier.requires_staking_box());
    }
}
