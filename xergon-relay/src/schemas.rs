//! Request/Response Schema Validation
//!
//! Defines JSON schemas for all request/response types and provides
//! manual validation middleware that checks incoming request bodies
//! against their expected schemas without external dependencies.
//!
//! Validation is performed using serde_json::Value checks.

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use tracing::warn;

// ---------------------------------------------------------------------------
// Validation error types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ValidationErrorResponse {
    pub error: crate::schemas::SchemaError,
}

#[derive(Debug, Serialize)]
pub struct SchemaError {
    pub code: String,
    pub message: String,
    pub details: Vec<ValidationError>,
}

// ---------------------------------------------------------------------------
// Schema validation functions
// ---------------------------------------------------------------------------

/// Validate a ChatCompletionRequest body.
pub fn validate_chat_completion_request(body: &Value) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();

    // Must be an object
    if !body.is_object() {
        return Err(vec![ValidationError {
            field: "$root".into(),
            message: "Request body must be a JSON object".into(),
        }]);
    }

    let obj = body.as_object().unwrap();

    // Required: model (string)
    match obj.get("model") {
        Some(v) if v.is_string() && !v.as_str().unwrap().is_empty() => {}
        Some(v) if v.is_string() => errors.push(ValidationError {
            field: "model".into(),
            message: "model must be a non-empty string".into(),
        }),
        Some(_) => errors.push(ValidationError {
            field: "model".into(),
            message: "model must be a string".into(),
        }),
        None => errors.push(ValidationError {
            field: "model".into(),
            message: "model is required".into(),
        }),
    }

    // Required: messages (array of objects with role + content)
    match obj.get("messages") {
        Some(v) if v.is_array() => {
            let arr = v.as_array().unwrap();
            if arr.is_empty() {
                errors.push(ValidationError {
                    field: "messages".into(),
                    message: "messages must be a non-empty array".into(),
                });
            } else {
                for (i, msg) in arr.iter().enumerate() {
                    let idx = format!("messages[{i}]");
                    if !msg.is_object() {
                        errors.push(ValidationError {
                            field: idx.clone(),
                            message: "each message must be an object".into(),
                        });
                        continue;
                    }
                    let msg_obj = msg.as_object().unwrap();

                    // role is required and must be string
                    match msg_obj.get("role") {
                        Some(r) if r.is_string() => {
                            let role = r.as_str().unwrap();
                            if !["system", "user", "assistant"].contains(&role) {
                                errors.push(ValidationError {
                                    field: format!("{idx}.role"),
                                    message: format!("role must be one of: system, user, assistant (got '{role}')"),
                                });
                            }
                        }
                        Some(_) => errors.push(ValidationError {
                            field: format!("{idx}.role"),
                            message: "role must be a string".into(),
                        }),
                        None => errors.push(ValidationError {
                            field: format!("{idx}.role"),
                            message: "role is required".into(),
                        }),
                    }

                    // content is required and must be string
                    match msg_obj.get("content") {
                        Some(c) if c.is_string() => {}
                        Some(_) => errors.push(ValidationError {
                            field: format!("{idx}.content"),
                            message: "content must be a string".into(),
                        }),
                        None => errors.push(ValidationError {
                            field: format!("{idx}.content"),
                            message: "content is required".into(),
                        }),
                    }
                }
            }
        }
        Some(_) => errors.push(ValidationError {
            field: "messages".into(),
            message: "messages must be an array".into(),
        }),
        None => errors.push(ValidationError {
            field: "messages".into(),
            message: "messages is required".into(),
        }),
    }

    // Optional: stream (boolean)
    if let Some(v) = obj.get("stream") {
        if !v.is_boolean() {
            errors.push(ValidationError {
                field: "stream".into(),
                message: "stream must be a boolean".into(),
            });
        }
    }

    // Optional: temperature (number, 0.0-2.0)
    if let Some(v) = obj.get("temperature") {
        if !v.is_number() {
            errors.push(ValidationError {
                field: "temperature".into(),
                message: "temperature must be a number".into(),
            });
        } else if let Some(n) = v.as_f64() {
            if n < 0.0 || n > 2.0 {
                errors.push(ValidationError {
                    field: "temperature".into(),
                    message: "temperature must be between 0.0 and 2.0".into(),
                });
            }
        }
    }

    // Optional: max_tokens (positive integer)
    if let Some(v) = obj.get("max_tokens") {
        if !v.is_number() {
            errors.push(ValidationError {
                field: "max_tokens".into(),
                message: "max_tokens must be a number".into(),
            });
        } else if let Some(n) = v.as_i64() {
            if n < 1 {
                errors.push(ValidationError {
                    field: "max_tokens".into(),
                    message: "max_tokens must be a positive integer".into(),
                });
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Validate a ProviderOnboardRequest body.
pub fn validate_provider_onboard_request(body: &Value) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();

    if !body.is_object() {
        return Err(vec![ValidationError {
            field: "$root".into(),
            message: "Request body must be a JSON object".into(),
        }]);
    }

    let obj = body.as_object().unwrap();

    // Required: endpoint (string, valid URL)
    match obj.get("endpoint") {
        Some(v) if v.is_string() => {
            let ep = v.as_str().unwrap();
            if !ep.starts_with("http://") && !ep.starts_with("https://") {
                errors.push(ValidationError {
                    field: "endpoint".into(),
                    message: "endpoint must be a valid HTTP(S) URL".into(),
                });
            }
        }
        Some(_) => errors.push(ValidationError {
            field: "endpoint".into(),
            message: "endpoint must be a string".into(),
        }),
        None => errors.push(ValidationError {
            field: "endpoint".into(),
            message: "endpoint is required".into(),
        }),
    }

    // Required: region (string)
    match obj.get("region") {
        Some(v) if v.is_string() && !v.as_str().unwrap().is_empty() => {}
        Some(v) if v.is_string() => errors.push(ValidationError {
            field: "region".into(),
            message: "region must be a non-empty string".into(),
        }),
        Some(_) => errors.push(ValidationError {
            field: "region".into(),
            message: "region must be a string".into(),
        }),
        None => errors.push(ValidationError {
            field: "region".into(),
            message: "region is required".into(),
        }),
    }

    // Optional: auth_token (string)
    if let Some(v) = obj.get("auth_token") {
        if !v.is_string() {
            errors.push(ValidationError {
                field: "auth_token".into(),
                message: "auth_token must be a string".into(),
            });
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

// ---------------------------------------------------------------------------
// Validation middleware
// ---------------------------------------------------------------------------

type ValidatorFn = Box<dyn Fn(&Value) -> Result<(), Vec<ValidationError>> + Send + Sync>;

/// A registry that maps (method, path_pattern) to a validation function.
pub struct SchemaValidatorRegistry {
    validators: HashMap<(String, String), ValidatorFn>,
}

impl SchemaValidatorRegistry {
    pub fn new() -> Self {
        Self {
            validators: HashMap::new(),
        }
    }

    /// Register a validator for a given method and path pattern.
    /// The path_pattern is a prefix match (e.g. "/v1/chat/completions").
    pub fn register<F>(&mut self, method: &str, path_pattern: &str, validator: F)
    where
        F: Fn(&Value) -> Result<(), Vec<ValidationError>> + Send + Sync + 'static,
    {
        self.validators.insert(
            (method.to_uppercase(), path_pattern.to_string()),
            Box::new(validator),
        );
    }

    /// Find a validator for the given method and path.
    fn find_validator(&self, method: &str, path: &str) -> Option<&ValidatorFn> {
        // Exact match first
        let key = (method.to_uppercase(), path.to_string());
        if let Some(v) = self.validators.get(&key) {
            return Some(v);
        }

        // Prefix match for paths with path params (strip trailing segments)
        let segments: Vec<&str> = path.split('/').collect();
        for len in (2..=segments.len()).rev() {
            let prefix = segments[..len].join("/");
            let key = (method.to_uppercase(), prefix);
            if let Some(v) = self.validators.get(&key) {
                return Some(v);
            }
        }

        None
    }

    /// Validate a request body against registered schemas.
    pub fn validate(&self, method: &str, path: &str, body: &Value) -> Result<(), Vec<ValidationError>> {
        if let Some(validator) = self.find_validator(method, path) {
            validator(body)
        } else {
            Ok(()) // No validator registered — pass through
        }
    }
}

impl Default for SchemaValidatorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Build the default schema validator registry with all known endpoints.
pub fn build_validator_registry() -> SchemaValidatorRegistry {
    let mut registry = SchemaValidatorRegistry::new();

    registry.register("POST", "/v1/chat/completions", validate_chat_completion_request);
    registry.register("POST", "/v1/providers/onboard", validate_provider_onboard_request);

    registry
}

/// Schema validation middleware.
///
/// Checks POST request bodies against their registered JSON schemas.
/// Returns 400 with detailed validation errors if the body doesn't match.
/// Non-POST requests and unregistered paths pass through unchanged.
pub async fn schema_validation_middleware(req: Request, next: Next) -> Response {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();

    // Only validate POST requests with registered schemas
    if method != "POST" {
        return next.run(req).await;
    }

    // Check if we have a validator for this path
    let needs_validation = {
        let registry = build_validator_registry();
        registry.find_validator(&method, &path).is_some()
    };

    if !needs_validation {
        return next.run(req).await;
    }

    // Read the body bytes
    let (parts, body) = req.into_parts();
    let bytes = match axum::body::to_bytes(body, 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            warn!(error = %e, path = %path, "Failed to read request body for validation");
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": {
                        "code": "invalid_request",
                        "message": "Failed to read request body"
                    }
                })),
            )
                .into_response();
        }
    };

    // Parse as JSON
    let parsed: Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": {
                        "code": "invalid_json",
                        "message": format!("Request body is not valid JSON: {e}")
                    }
                })),
            )
                .into_response();
        }
    };

    // Validate against schema
    let validation_result = {
        let reg = build_validator_registry();
        reg.validate(&method, &path, &parsed)
    };

    if let Err(errors) = validation_result {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": {
                    "code": "validation_error",
                    "message": "Request body does not match expected schema",
                    "details": errors
                }
            })),
        )
            .into_response();
    }

    // Reconstruct the request with the consumed body
    let req = Request::from_parts(parts, axum::body::Body::from(bytes));
    next.run(req).await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ---- Chat completion validation tests ----

    #[test]
    fn test_valid_chat_completion_request() {
        let body = json!({
            "model": "llama-3.1-8b",
            "messages": [
                {"role": "user", "content": "Hello!"}
            ]
        });
        assert!(validate_chat_completion_request(&body).is_ok());
    }

    #[test]
    fn test_chat_completion_with_all_options() {
        let body = json!({
            "model": "llama-3.1-8b",
            "messages": [
                {"role": "system", "content": "You are helpful."},
                {"role": "user", "content": "Hello!"}
            ],
            "stream": true,
            "temperature": 0.7,
            "max_tokens": 100
        });
        assert!(validate_chat_completion_request(&body).is_ok());
    }

    #[test]
    fn test_chat_completion_missing_model() {
        let body = json!({
            "messages": [{"role": "user", "content": "Hello!"}]
        });
        let errors = validate_chat_completion_request(&body).unwrap_err();
        assert!(errors.iter().any(|e| e.field == "model"));
    }

    #[test]
    fn test_chat_completion_missing_messages() {
        let body = json!({
            "model": "llama-3.1-8b"
        });
        let errors = validate_chat_completion_request(&body).unwrap_err();
        assert!(errors.iter().any(|e| e.field == "messages"));
    }

    #[test]
    fn test_chat_completion_empty_messages() {
        let body = json!({
            "model": "llama-3.1-8b",
            "messages": []
        });
        let errors = validate_chat_completion_request(&body).unwrap_err();
        assert!(errors.iter().any(|e| e.field == "messages"));
    }

    #[test]
    fn test_chat_completion_invalid_message_role() {
        let body = json!({
            "model": "llama-3.1-8b",
            "messages": [{"role": "invalid", "content": "Hello!"}]
        });
        let errors = validate_chat_completion_request(&body).unwrap_err();
        assert!(errors.iter().any(|e| e.field.contains("role")));
    }

    #[test]
    fn test_chat_completion_message_missing_role() {
        let body = json!({
            "model": "llama-3.1-8b",
            "messages": [{"content": "Hello!"}]
        });
        let errors = validate_chat_completion_request(&body).unwrap_err();
        assert!(errors.iter().any(|e| e.field.contains("role")));
    }

    #[test]
    fn test_chat_completion_message_missing_content() {
        let body = json!({
            "model": "llama-3.1-8b",
            "messages": [{"role": "user"}]
        });
        let errors = validate_chat_completion_request(&body).unwrap_err();
        assert!(errors.iter().any(|e| e.field.contains("content")));
    }

    #[test]
    fn test_chat_completion_invalid_stream() {
        let body = json!({
            "model": "llama-3.1-8b",
            "messages": [{"role": "user", "content": "Hello!"}],
            "stream": "yes"
        });
        let errors = validate_chat_completion_request(&body).unwrap_err();
        assert!(errors.iter().any(|e| e.field == "stream"));
    }

    #[test]
    fn test_chat_completion_temperature_out_of_range() {
        let body = json!({
            "model": "llama-3.1-8b",
            "messages": [{"role": "user", "content": "Hello!"}],
            "temperature": 5.0
        });
        let errors = validate_chat_completion_request(&body).unwrap_err();
        assert!(errors.iter().any(|e| e.field == "temperature" && e.message.contains("0.0")));
    }

    #[test]
    fn test_chat_completion_negative_max_tokens() {
        let body = json!({
            "model": "llama-3.1-8b",
            "messages": [{"role": "user", "content": "Hello!"}],
            "max_tokens": -1
        });
        let errors = validate_chat_completion_request(&body).unwrap_err();
        assert!(errors.iter().any(|e| e.field == "max_tokens"));
    }

    #[test]
    fn test_chat_completion_non_object_body() {
        let body = json!("not an object");
        let errors = validate_chat_completion_request(&body).unwrap_err();
        assert!(errors.iter().any(|e| e.field == "$root"));
    }

    // ---- Provider onboard validation tests ----

    #[test]
    fn test_valid_provider_onboard_request() {
        let body = json!({
            "endpoint": "https://provider.example.com/v1",
            "region": "us-east"
        });
        assert!(validate_provider_onboard_request(&body).is_ok());
    }

    #[test]
    fn test_provider_onboard_with_auth_token() {
        let body = json!({
            "endpoint": "https://provider.example.com/v1",
            "region": "eu-west",
            "auth_token": "secret-token"
        });
        assert!(validate_provider_onboard_request(&body).is_ok());
    }

    #[test]
    fn test_provider_onboard_missing_endpoint() {
        let body = json!({
            "region": "us-east"
        });
        let errors = validate_provider_onboard_request(&body).unwrap_err();
        assert!(errors.iter().any(|e| e.field == "endpoint"));
    }

    #[test]
    fn test_provider_onboard_invalid_endpoint() {
        let body = json!({
            "endpoint": "not-a-url",
            "region": "us-east"
        });
        let errors = validate_provider_onboard_request(&body).unwrap_err();
        assert!(errors.iter().any(|e| e.field == "endpoint"));
    }

    #[test]
    fn test_provider_onboard_missing_region() {
        let body = json!({
            "endpoint": "https://provider.example.com/v1"
        });
        let errors = validate_provider_onboard_request(&body).unwrap_err();
        assert!(errors.iter().any(|e| e.field == "region"));
    }

    #[test]
    fn test_provider_onboard_invalid_auth_token() {
        let body = json!({
            "endpoint": "https://provider.example.com/v1",
            "region": "us-east",
            "auth_token": 12345
        });
        let errors = validate_provider_onboard_request(&body).unwrap_err();
        assert!(errors.iter().any(|e| e.field == "auth_token"));
    }

    // ---- Validator registry tests ----

    #[test]
    fn test_registry_validates_known_path() {
        let registry = build_validator_registry();
        let body = json!({
            "model": "llama-3.1-8b",
            "messages": [{"role": "user", "content": "Hello!"}]
        });
        assert!(registry.validate("POST", "/v1/chat/completions", &body).is_ok());
    }

    #[test]
    fn test_registry_rejects_invalid_body() {
        let registry = build_validator_registry();
        let body = json!({
            "model": "llama-3.1-8b"
        });
        assert!(registry.validate("POST", "/v1/chat/completions", &body).is_err());
    }

    #[test]
    fn test_registry_passes_unknown_path() {
        let registry = build_validator_registry();
        let body = json!({"anything": "goes"});
        assert!(registry.validate("POST", "/v1/unknown", &body).is_ok());
    }

    #[test]
    fn test_registry_passes_get_request() {
        let registry = build_validator_registry();
        let body = json!({"anything": "goes"});
        assert!(registry.validate("GET", "/v1/models", &body).is_ok());
    }
}
