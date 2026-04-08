#![allow(dead_code)]
//! Provider capability negotiation protocol.
//!
//! Allows providers to register their capabilities (supported models, batch sizes,
//! streaming, encryption, quantization, etc.) and for clients to negotiate the
//! best provider match for a given set of required features.

use std::collections::{HashSet};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// Describes the full set of capabilities a single provider advertises.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitySet {
    /// Unique provider identifier (e.g. Ergo PK or registered name).
    pub provider_id: String,
    /// List of model identifiers this provider can serve (e.g. "llama-3-70b").
    pub supported_models: Vec<String>,
    /// Maximum number of requests the provider will accept in a single batch.
    pub max_batch_size: u32,
    /// Whether the provider supports SSE streaming responses.
    pub streaming_supported: bool,
    /// Encryption schemes the provider supports (e.g. "aes-256-gcm", "x25519").
    pub encryption_schemes: Vec<String>,
    /// Quantization formats supported (e.g. "fp16", "int8", "int4", "none").
    pub quantization_formats: Vec<String>,
    /// Maximum context window length in tokens.
    pub max_context_length: u32,
    /// Arbitrary feature flags the provider advertises.
    pub features: Vec<String>,
    /// Protocol version string the provider speaks (semver).
    pub protocol_version: String,
    /// Human-readable endpoint URL for the provider.
    pub endpoint: Option<String>,
    /// Geographic region hint (e.g. "us-east", "eu-west").
    pub region: Option<String>,
    /// Timestamp when these capabilities were last updated.
    #[serde(with = "chrono::serde::ts_seconds")]
    pub registered_at: DateTime<Utc>,
    /// Optional provider display name.
    pub display_name: Option<String>,
    /// Whether the provider is currently accepting requests.
    pub accepting_requests: bool,
}

impl CapabilitySet {
    /// Check whether this provider supports a specific model.
    pub fn supports_model(&self, model: &str) -> bool {
        self.supported_models.iter().any(|m| m.eq_ignore_ascii_case(model))
    }

    /// Check whether this provider supports a specific feature flag.
    pub fn supports_feature(&self, feature: &str) -> bool {
        self.features.iter().any(|f| f.eq_ignore_ascii_case(feature))
    }

    /// Check whether this provider supports a given encryption scheme.
    pub fn supports_encryption(&self, scheme: &str) -> bool {
        self.encryption_schemes.iter().any(|s| s.eq_ignore_ascii_case(scheme))
    }

    /// Check whether this provider supports a given quantization format.
    pub fn supports_quantization(&self, fmt: &str) -> bool {
        self.quantization_formats.iter().any(|q| q.eq_ignore_ascii_case(fmt))
    }
}

/// Request body for registering / updating capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub provider_id: String,
    #[serde(default)]
    pub supported_models: Vec<String>,
    #[serde(default = "default_max_batch_size")]
    pub max_batch_size: u32,
    #[serde(default)]
    pub streaming_supported: bool,
    #[serde(default)]
    pub encryption_schemes: Vec<String>,
    #[serde(default)]
    pub quantization_formats: Vec<String>,
    #[serde(default = "default_max_context")]
    pub max_context_length: u32,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default = "default_protocol_version")]
    pub protocol_version: String,
    pub endpoint: Option<String>,
    pub region: Option<String>,
    pub display_name: Option<String>,
    #[serde(default = "default_true")]
    pub accepting_requests: bool,
}

fn default_max_batch_size() -> u32 { 1 }
fn default_max_context() -> u32 { 4096 }
fn default_protocol_version() -> String { "1.0.0".to_string() }
fn default_true() -> bool { true }

impl From<RegisterRequest> for CapabilitySet {
    fn from(r: RegisterRequest) -> Self {
        Self {
            provider_id: r.provider_id,
            supported_models: r.supported_models,
            max_batch_size: r.max_batch_size,
            streaming_supported: r.streaming_supported,
            encryption_schemes: r.encryption_schemes,
            quantization_formats: r.quantization_formats,
            max_context_length: r.max_context_length,
            features: r.features,
            protocol_version: r.protocol_version,
            endpoint: r.endpoint,
            region: r.region,
            registered_at: Utc::now(),
            display_name: r.display_name,
            accepting_requests: r.accepting_requests,
        }
    }
}

/// Request body for capability negotiation — describes what the client needs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegotiateRequest {
    /// Required model(s) — at least one must be supported.
    pub required_models: Vec<String>,
    /// Minimum acceptable batch size.
    #[serde(default = "default_max_batch_size")]
    pub min_batch_size: u32,
    /// Whether streaming is required.
    #[serde(default)]
    pub require_streaming: bool,
    /// Required encryption scheme (empty = any).
    #[serde(default)]
    pub required_encryption: String,
    /// Preferred quantization format (empty = any).
    #[serde(default)]
    pub preferred_quantization: String,
    /// Minimum acceptable context length.
    #[serde(default = "default_max_context")]
    pub min_context_length: u32,
    /// Required feature flags — all must be present.
    #[serde(default)]
    pub required_features: Vec<String>,
    /// Minimum acceptable protocol version (semver).
    #[serde(default = "default_protocol_version")]
    pub min_protocol_version: String,
    /// Preferred region hint for latency optimisation.
    #[serde(default)]
    pub preferred_region: Option<String>,
}

/// Negotiation result for a single provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegotiationResult {
    pub provider_id: String,
    pub endpoint: Option<String>,
    pub display_name: Option<String>,
    pub region: Option<String>,
    /// Score 0..1 indicating how well the provider matches.
    pub match_score: f64,
    /// Human-readable list of matched / missing items.
    pub matched: Vec<String>,
    pub missing: Vec<String>,
}

/// Response for the register endpoint.
#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterResponse {
    pub ok: bool,
    pub provider_id: String,
    pub message: String,
}

/// Simple feature check response.
#[derive(Debug, Serialize, Deserialize)]
pub struct FeatureCheckResponse {
    pub feature: String,
    pub provider_count: usize,
    pub providers: Vec<String>,
}

// ---------------------------------------------------------------------------
// Core negotiator
// ---------------------------------------------------------------------------

/// Thread-safe registry of provider capabilities with negotiation logic.
pub struct CapabilityNegotiator {
    /// provider_id -> CapabilitySet
    capabilities: DashMap<String, CapabilitySet>,
}

impl std::fmt::Debug for CapabilityNegotiator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapabilityNegotiator")
            .field("provider_count", &self.capabilities.len())
            .finish()
    }
}

impl CapabilityNegotiator {
    /// Create a new empty negotiator.
    pub fn new() -> Self {
        Self {
            capabilities: DashMap::new(),
        }
    }

    /// Register or update a provider's capabilities. Returns the previous set if
    /// the provider was already registered.
    pub fn register_capabilities(
        &self,
        caps: CapabilitySet,
    ) -> Option<CapabilitySet> {
        info!(
            provider_id = %caps.provider_id,
            models = ?caps.supported_models,
            features = ?caps.features,
            "Registering provider capabilities"
        );
        self.capabilities.insert(caps.provider_id.clone(), caps)
    }

    /// Negotiate: given a client requirement set, return all matching providers
    /// sorted by match score descending.
    pub fn negotiate(&self, req: &NegotiateRequest) -> Vec<NegotiationResult> {
        let mut results: Vec<NegotiationResult> = self
            .capabilities
            .iter()
            .filter_map(|entry| {
                let caps = entry.value();
                if !caps.accepting_requests {
                    return None;
                }
                let mut matched = Vec::new();
                let mut missing = Vec::new();
                let mut score = 0.0;
                let max_score = 8.0; // total possible points

                // Model match (required)
                let model_match = req.required_models.iter().any(|m| caps.supports_model(m));
                if model_match {
                    matched.push("models".into());
                    score += 1.0;
                } else {
                    missing.push("models".into());
                }

                // Batch size
                if caps.max_batch_size >= req.min_batch_size {
                    matched.push("batch_size".into());
                    score += 1.0;
                } else {
                    missing.push("batch_size".into());
                }

                // Streaming
                if req.require_streaming {
                    if caps.streaming_supported {
                        matched.push("streaming".into());
                        score += 1.0;
                    } else {
                        missing.push("streaming".into());
                    }
                } else {
                    score += 1.0; // not required, free point
                }

                // Encryption
                if req.required_encryption.is_empty() || caps.supports_encryption(&req.required_encryption) {
                    matched.push("encryption".into());
                    score += 1.0;
                } else {
                    missing.push("encryption".into());
                }

                // Context length
                if caps.max_context_length >= req.min_context_length {
                    matched.push("context_length".into());
                    score += 1.0;
                } else {
                    missing.push("context_length".into());
                }

                // Features (all required)
                let all_features = req.required_features.iter().all(|f| caps.supports_feature(f));
                if all_features {
                    matched.push("features".into());
                    score += 1.0;
                } else {
                    let missing_features: Vec<String> = req
                        .required_features
                        .iter()
                        .filter(|f| !caps.supports_feature(f))
                        .cloned()
                        .collect();
                    missing.push(format!("features: {:?}", missing_features));
                }

                // Protocol version (semver gte check)
                let version_ok = semver_gte(&caps.protocol_version, &req.min_protocol_version);
                if version_ok {
                    matched.push("protocol_version".into());
                    score += 1.0;
                } else {
                    missing.push("protocol_version".into());
                }

                // Region preference bonus
                if let Some(ref pref) = req.preferred_region {
                    if caps.region.as_deref() == Some(pref.as_str()) {
                        score += 1.0;
                        matched.push("region".into());
                    }
                }

                // Reject if any required field is missing
                if !model_match || !all_features || !version_ok {
                    return None;
                }

                Some(NegotiationResult {
                    provider_id: caps.provider_id.clone(),
                    endpoint: caps.endpoint.clone(),
                    display_name: caps.display_name.clone(),
                    region: caps.region.clone(),
                    match_score: score / max_score,
                    matched,
                    missing,
                })
            })
            .collect();

        results.sort_by(|a, b| b.match_score.partial_cmp(&a.match_score).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    /// Get a specific provider's capabilities.
    pub fn get_capabilities(&self, provider_id: &str) -> Option<CapabilitySet> {
        self.capabilities.get(provider_id).map(|e| e.value().clone())
    }

    /// Find all providers that support a given feature flag.
    pub fn get_compatible_providers(&self, feature: &str) -> Vec<String> {
        self.capabilities
            .iter()
            .filter(|e| e.value().supports_feature(feature))
            .map(|e| e.value().provider_id.clone())
            .collect()
    }

    /// Check whether a specific provider supports a specific feature.
    pub fn check_feature(&self, provider_id: &str, feature: &str) -> bool {
        self.capabilities
            .get(provider_id)
            .map(|e| e.value().supports_feature(feature))
            .unwrap_or(false)
    }

    /// Update (merge) capabilities for an existing provider.
    /// Fields that are `Some` in the request replace existing values;
    /// `None` / empty fields are left unchanged.
    pub fn update_capabilities(
        &self,
        provider_id: &str,
        update: &RegisterRequest,
    ) -> bool {
        let mut entry = match self.capabilities.get_mut(provider_id) {
            Some(e) => e,
            None => return false,
        };

        let caps = entry.value_mut();

        if !update.supported_models.is_empty() {
            caps.supported_models = update.supported_models.clone();
        }
        if update.max_batch_size > 0 {
            caps.max_batch_size = update.max_batch_size;
        }
        if update.streaming_supported {
            caps.streaming_supported = true;
        }
        if !update.encryption_schemes.is_empty() {
            caps.encryption_schemes = update.encryption_schemes.clone();
        }
        if !update.quantization_formats.is_empty() {
            caps.quantization_formats = update.quantization_formats.clone();
        }
        if update.max_context_length > 0 {
            caps.max_context_length = update.max_context_length;
        }
        if !update.features.is_empty() {
            caps.features = update.features.clone();
        }
        if !update.protocol_version.is_empty() {
            caps.protocol_version = update.protocol_version.clone();
        }
        if update.endpoint.is_some() {
            caps.endpoint = update.endpoint.clone();
        }
        if update.region.is_some() {
            caps.region = update.region.clone();
        }
        if update.display_name.is_some() {
            caps.display_name = update.display_name.clone();
        }
        caps.accepting_requests = update.accepting_requests;
        caps.registered_at = Utc::now();

        info!(provider_id = %provider_id, "Updated provider capabilities");
        true
    }

    /// List all registered providers.
    pub fn list_all(&self) -> Vec<CapabilitySet> {
        self.capabilities.iter().map(|e| e.value().clone()).collect()
    }

    /// Remove a provider from the registry.
    pub fn deregister(&self, provider_id: &str) -> bool {
        self.capabilities.remove(provider_id).is_some()
    }

    /// Collect the union of all feature flags across all providers.
    pub fn all_features(&self) -> HashSet<String> {
        let mut set = HashSet::new();
        for entry in self.capabilities.iter() {
            for f in &entry.value().features {
                set.insert(f.to_lowercase());
            }
        }
        set
    }

    /// Number of registered providers.
    pub fn provider_count(&self) -> usize {
        self.capabilities.len()
    }
}

// ---------------------------------------------------------------------------
// Semver helpers
// ---------------------------------------------------------------------------

/// Simple semver "greater-than-or-equal" comparison.
/// Handles major.minor.patch; ignores pre-release/build for simplicity.
fn semver_gte(version: &str, min_version: &str) -> bool {
    let v = parse_semver(version);
    let m = parse_semver(min_version);
    v >= m
}

fn parse_semver(s: &str) -> (u32, u32, u32) {
    let mut parts = s.split('.');
    let major = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let patch = parts
        .next()
        .and_then(|p| p.split('-').next().and_then(|pp| pp.parse().ok()))
        .unwrap_or(0);
    (major, minor, patch)
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

/// POST /v1/capabilities/register
async fn register_handler(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> impl IntoResponse {
    let caps: CapabilitySet = req.clone().into();
    let pid = caps.provider_id.clone();
    state.capability_negotiation.register_capabilities(caps);
    debug!(provider_id = %pid, "Capabilities registered via HTTP");
    (StatusCode::OK, Json(RegisterResponse {
        ok: true,
        provider_id: pid,
        message: "Capabilities registered successfully".into(),
    }))
}

/// POST /v1/capabilities/negotiate
async fn negotiate_handler(
    State(state): State<AppState>,
    Json(req): Json<NegotiateRequest>,
) -> impl IntoResponse {
    let results = state.capability_negotiation.negotiate(&req);
    debug!(results = results.len(), "Negotiation completed");
    (StatusCode::OK, Json(results))
}

/// GET /v1/capabilities/providers/{id}
async fn get_provider_handler(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> axum::response::Response {
    match state.capability_negotiation.get_capabilities(&provider_id) {
        Some(caps) => (StatusCode::OK, Json(caps)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Provider not found"})),
        ).into_response(),
    }
}

/// GET /v1/capabilities/features
async fn list_features_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let mut features: Vec<String> = state
        .capability_negotiation
        .all_features()
        .into_iter()
        .collect();
    features.sort();
    (StatusCode::OK, Json(features))
}

/// GET /v1/capabilities/compatible/{feature}
async fn compatible_providers_handler(
    State(state): State<AppState>,
    Path(feature): Path<String>,
) -> impl IntoResponse {
    let providers = state.capability_negotiation.get_compatible_providers(&feature);
    (StatusCode::OK, Json(providers))
}

/// DELETE /v1/capabilities/{id}
async fn deregister_handler(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
) -> axum::response::Response {
    let removed = state.capability_negotiation.deregister(&provider_id);
    if removed {
        debug!(provider_id = %provider_id, "Provider deregistered");
        (StatusCode::OK, Json(RegisterResponse {
            ok: true,
            provider_id,
            message: "Provider deregistered".into(),
        })).into_response()
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Provider not found"})),
        ).into_response()
    }
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/v1/capabilities/register", post(register_handler))
        .route("/v1/capabilities/negotiate", post(negotiate_handler))
        .route("/v1/capabilities/providers/{id}", get(get_provider_handler))
        .route("/v1/capabilities/features", get(list_features_handler))
        .route("/v1/capabilities/compatible/{feature}", get(compatible_providers_handler))
        .route("/v1/capabilities/{id}", delete(deregister_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_caps(provider_id: &str, models: Vec<&str>, features: Vec<&str>) -> CapabilitySet {
        CapabilitySet {
            provider_id: provider_id.to_string(),
            supported_models: models.iter().map(|s| s.to_string()).collect(),
            max_batch_size: 8,
            streaming_supported: true,
            encryption_schemes: vec!["aes-256-gcm".to_string()],
            quantization_formats: vec!["fp16".to_string(), "int8".to_string()],
            max_context_length: 8192,
            features: features.iter().map(|s| s.to_string()).collect(),
            protocol_version: "2.1.0".to_string(),
            endpoint: Some(format!("https://{}.xergon.test", provider_id)),
            region: Some("us-east".to_string()),
            registered_at: Utc::now(),
            display_name: Some(provider_id.to_uppercase()),
            accepting_requests: true,
        }
    }

    fn make_negotiator_with_data() -> CapabilityNegotiator {
        let n = CapabilityNegotiator::new();
        n.register_capabilities(make_caps("prov-a", vec!["llama-3-70b", "mistral-7b"], vec!["tool-use", "json-mode"]));
        n.register_capabilities(make_caps("prov-b", vec!["llama-3-70b"], vec!["tool-use", "vision"]));
        n.register_capabilities(make_caps("prov-c", vec!["gpt-4o"], vec!["tool-use", "json-mode", "vision"]));
        n
    }

    #[test]
    fn test_register_and_retrieve() {
        let n = make_negotiator_with_data();
        assert_eq!(n.provider_count(), 3);

        let caps = n.get_capabilities("prov-a").unwrap();
        assert_eq!(caps.supported_models.len(), 2);
        assert!(caps.streaming_supported);
    }

    #[test]
    fn test_negotiate_matching() {
        let n = make_negotiator_with_data();
        let req = NegotiateRequest {
            required_models: vec!["llama-3-70b".into()],
            min_batch_size: 1,
            require_streaming: false,
            required_encryption: String::new(),
            preferred_quantization: String::new(),
            min_context_length: 4096,
            required_features: vec!["tool-use".into()],
            min_protocol_version: "1.0.0".into(),
            preferred_region: Some("us-east".into()),
        };
        let results = n.negotiate(&req);
        assert_eq!(results.len(), 2); // prov-a and prov-b
        assert!(results[0].match_score >= results[1].match_score);
        assert!(results.iter().all(|r| r.matched.contains(&"region".into())));
    }

    #[test]
    fn test_negotiate_mismatch() {
        let n = make_negotiator_with_data();
        let req = NegotiateRequest {
            required_models: vec!["nonexistent-model".into()],
            min_batch_size: 1,
            require_streaming: false,
            required_encryption: String::new(),
            preferred_quantization: String::new(),
            min_context_length: 4096,
            required_features: vec![],
            min_protocol_version: "1.0.0".into(),
            preferred_region: None,
        };
        let results = n.negotiate(&req);
        assert!(results.is_empty());
    }

    #[test]
    fn test_feature_check() {
        let n = make_negotiator_with_data();
        assert!(n.check_feature("prov-a", "tool-use"));
        assert!(n.check_feature("prov-a", "json-mode"));
        assert!(!n.check_feature("prov-a", "vision"));
        assert!(!n.check_feature("prov-b", "json-mode"));
        assert!(n.check_feature("prov-c", "vision"));
    }

    #[test]
    fn test_version_ordering() {
        let n = CapabilityNegotiator::new();
        let caps_old = CapabilitySet {
            provider_id: "old-prov".into(),
            supported_models: vec!["m".into()],
            max_batch_size: 1,
            streaming_supported: false,
            encryption_schemes: vec![],
            quantization_formats: vec![],
            max_context_length: 1024,
            features: vec![],
            protocol_version: "1.0.0".into(),
            endpoint: None,
            region: None,
            registered_at: Utc::now(),
            display_name: None,
            accepting_requests: true,
        };
        n.register_capabilities(caps_old);

        let req = NegotiateRequest {
            required_models: vec!["m".into()],
            min_batch_size: 1,
            require_streaming: false,
            required_encryption: String::new(),
            preferred_quantization: String::new(),
            min_context_length: 1024,
            required_features: vec![],
            min_protocol_version: "2.0.0".into(),
            preferred_region: None,
        };
        assert!(n.negotiate(&req).is_empty());

        let req2 = NegotiateRequest {
            required_models: vec!["m".into()],
            min_batch_size: 1,
            require_streaming: false,
            required_encryption: String::new(),
            preferred_quantization: String::new(),
            min_context_length: 1024,
            required_features: vec![],
            min_protocol_version: "0.9.0".into(),
            preferred_region: None,
        };
        assert_eq!(n.negotiate(&req2).len(), 1);
    }

    #[test]
    fn test_deregister() {
        let n = make_negotiator_with_data();
        assert!(n.deregister("prov-a"));
        assert_eq!(n.provider_count(), 2);
        assert!(n.get_capabilities("prov-a").is_none());
        assert!(!n.deregister("prov-a")); // already removed
    }

    #[test]
    fn test_update_capabilities() {
        let n = make_negotiator_with_data();
        let update = RegisterRequest {
            provider_id: "prov-a".into(),
            supported_models: vec!["llama-3-70b".into(), "phi-3".into()],
            max_batch_size: 16,
            streaming_supported: true,
            encryption_schemes: vec![],
            quantization_formats: vec![],
            max_context_length: 16384,
            features: vec!["tool-use".into(), "json-mode".into(), "code-exec".into()],
            protocol_version: "2.1.0".into(),
            endpoint: None,
            region: None,
            display_name: None,
            accepting_requests: true,
        };
        assert!(n.update_capabilities("prov-a", &update));
        let caps = n.get_capabilities("prov-a").unwrap();
        assert_eq!(caps.supported_models.len(), 2);
        assert_eq!(caps.max_batch_size, 16);
        assert_eq!(caps.max_context_length, 16384);
        assert!(caps.features.contains(&"code-exec".into()));
    }

    #[test]
    fn test_get_compatible_providers() {
        let n = make_negotiator_with_data();
        let providers = n.get_compatible_providers("vision");
        assert_eq!(providers.len(), 2);
        assert!(providers.contains(&"prov-b".into()));
        assert!(providers.contains(&"prov-c".into()));
    }

    #[test]
    fn test_all_features() {
        let n = make_negotiator_with_data();
        let features = n.all_features();
        assert!(features.contains("tool-use"));
        assert!(features.contains("json-mode"));
        assert!(features.contains("vision"));
    }

    #[test]
    fn test_list_all() {
        let n = make_negotiator_with_data();
        let all = n.list_all();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_not_accepting_requests_excluded() {
        let n = CapabilityNegotiator::new();
        let mut caps = make_caps("offline-prov", vec!["m"], vec!["feat"]);
        caps.accepting_requests = false;
        n.register_capabilities(caps);

        let req = NegotiateRequest {
            required_models: vec!["m".into()],
            min_batch_size: 1,
            require_streaming: false,
            required_encryption: String::new(),
            preferred_quantization: String::new(),
            min_context_length: 1024,
            required_features: vec!["feat".into()],
            min_protocol_version: "1.0.0".into(),
            preferred_region: None,
        };
        assert!(n.negotiate(&req).is_empty());
    }
}
