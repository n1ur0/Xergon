use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::error::Error;

type HmacSha256 = Hmac<Sha256>;

/// Constant-time string comparison to prevent timing attacks
fn const_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let mut result = 0u8;
    for (x, y) in a_bytes.iter().zip(b_bytes.iter()) {
        result |= x.wrapping_sub(*y);
    }
    result == 0
}

#[derive(Debug, Clone)]
pub struct ApiKey {
    pub key: String,
    pub secret: String,
    pub tier: ApiTier,
    pub rate_limit: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ApiTier {
    Free,
    Premium,
    Enterprise,
}

impl ApiKey {
    pub fn new(key: String, secret: String, tier: ApiTier) -> Self {
        let rate_limit = match tier {
            ApiTier::Free => 100,      // 100 requests per minute
            ApiTier::Premium => 1000,  // 1000 requests per minute
            ApiTier::Enterprise => 10000, // 10000 requests per minute
        };

        Self {
            key,
            secret,
            tier,
            rate_limit,
        }
    }
}

pub struct AuthManager {
    api_keys: std::collections::HashMap<String, ApiKey>,
}

impl AuthManager {
    pub fn new() -> Self {
        Self::new_with_keys(false)
    }

    pub fn new_with_keys(include_test_keys: bool) -> Self {
        let mut api_keys = std::collections::HashMap::new();
        
        // Add test API keys only when explicitly requested (e.g., for testing)
        if include_test_keys {
            api_keys.insert(
                "xergon-test-key-1".to_string(),
                ApiKey::new(
                    "xergon-test-key-1".to_string(),
                    "test-secret-1".to_string(),
                    ApiTier::Premium,
                ),
            );
            
            api_keys.insert(
                "xergon-test-key-2".to_string(),
                ApiKey::new(
                    "xergon-test-key-2".to_string(),
                    "test-secret-2".to_string(),
                    ApiTier::Free,
                ),
            );
        }

        Self { api_keys }
    }

    pub fn verify_signature(
        &self,
        api_key: &str,
        payload: &str,
        signature: &str,
    ) -> Result<bool, Box<dyn Error>> {
        // Get the API key
        let key = self.api_keys.get(api_key).ok_or("Invalid API key")?;

        // Compute HMAC-SHA256
        let mut mac = HmacSha256::new_from_slice(key.secret.as_bytes())
            .map_err(|e| format!("HMAC initialization failed: {}", e))?;
        mac.update(payload.as_bytes());
        let result = mac.finalize();
        let computed_signature = hex::encode(result.into_bytes());

        // Constant-time comparison to prevent timing attacks
        Ok(const_time_eq(&computed_signature, signature))
    }

    pub fn get_api_key(&self, key: &str) -> Option<&ApiKey> {
        self.api_keys.get(key)
    }

    pub fn add_api_key(&mut self, api_key: ApiKey) {
        self.api_keys.insert(api_key.key.clone(), api_key);
    }
}

impl Default for AuthManager {
    fn default() -> Self {
        Self::new()
    }
}

// Rate limiter
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub struct RateLimiter {
    requests: HashMap<String, Vec<Instant>>,
    window: Duration,
}

impl RateLimiter {
    pub fn new(window_secs: u64) -> Self {
        Self {
            requests: HashMap::new(),
            window: Duration::from_secs(window_secs),
        }
    }

    pub fn check_limit(&mut self, api_key: &str, limit: usize) -> bool {
        let now = Instant::now();
        let requests = self.requests.entry(api_key.to_string()).or_insert_with(Vec::new);

        // Remove old requests outside the window
        requests.retain(|&timestamp| now.duration_since(timestamp) < self.window);

        // Check if within limit
        if requests.len() < limit {
            requests.push(now);
            true
        } else {
            false
        }
    }

    pub fn get_remaining(&self, api_key: &str, limit: usize) -> usize {
        let now = Instant::now();
        if let Some(requests) = self.requests.get(api_key) {
            let valid_requests: usize = requests
                .iter()
                .filter(|&&timestamp| now.duration_since(timestamp) < self.window)
                .count();
            limit.saturating_sub(valid_requests)
        } else {
            limit
        }
    }
}
