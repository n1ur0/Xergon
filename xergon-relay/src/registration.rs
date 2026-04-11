use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize, Clone)]
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
pub struct ProviderRegistrationResponse {
    pub success: bool,
    pub provider_id: Option<String>,
    pub message: String,
    pub registered_at: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProviderRegistry {
    providers: std::collections::HashMap<String, RegisteredProvider>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: std::collections::HashMap::new(),
        }
    }

    pub fn register(&mut self, req: ProviderRegistration) -> ProviderRegistrationResponse {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Validate provider_id
        if req.provider_id.is_empty() {
            return ProviderRegistrationResponse {
                success: false,
                provider_id: None,
                message: "provider_id cannot be empty".to_string(),
                registered_at: None,
            };
        }

        // Validate ergo_address (basic validation)
        if !req.ergo_address.starts_with("9") || req.ergo_address.len() < 30 {
            return ProviderRegistrationResponse {
                success: false,
                provider_id: None,
                message: "Invalid Ergo address format".to_string(),
                registered_at: None,
            };
        }

        // Check if already registered
        if self.providers.contains_key(&req.provider_id) {
            return ProviderRegistrationResponse {
                success: false,
                provider_id: Some(req.provider_id.clone()),
                message: "Provider already registered".to_string(),
                registered_at: Some(now),
            };
        }

        let registered_provider = RegisteredProvider {
            provider_id: req.provider_id.clone(),
            ergo_address: req.ergo_address,
            region: req.region,
            models: req.models,
            capacity_gpus: req.capacity_gpus,
            max_concurrent_requests: req.max_concurrent_requests,
            registered_at: now,
            last_heartbeat: None,
            health_status: HealthStatus::Unknown,
            pown_score: None,
        };

        self.providers.insert(req.provider_id.clone(), registered_provider);

        ProviderRegistrationResponse {
            success: true,
            provider_id: Some(req.provider_id),
            message: "Provider registered successfully".to_string(),
            registered_at: Some(now),
        }
    }

    pub fn get_provider(&self, provider_id: &str) -> Option<&RegisteredProvider> {
        self.providers.get(provider_id)
    }

    pub fn list_providers(&self) -> Vec<&RegisteredProvider> {
        self.providers.values().collect()
    }

    pub fn update_heartbeat(&mut self, provider_id: &str, pown_score: Option<f32>) -> bool {
        if let Some(provider) = self.providers.get_mut(provider_id) {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            provider.last_heartbeat = Some(now);
            provider.pown_score = pown_score;
            provider.health_status = HealthStatus::Healthy;
            true
        } else {
            false
        }
    }

    pub fn mark_unhealthy(&mut self, provider_id: &str) -> bool {
        if let Some(provider) = self.providers.get_mut(provider_id) {
            provider.health_status = HealthStatus::Unhealthy;
            true
        } else {
            false
        }
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}
