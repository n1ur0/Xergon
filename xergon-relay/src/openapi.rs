#![allow(dead_code)]
//! OpenAPI 3.0.3 Specification Generator
//!
//! Builds an OpenAPI spec programmatically as serde_json::Value (no utoipa dependency).
//! Documents all xergon-relay endpoints and serves them via:
//! - GET /v1/openapi.json — raw spec
//! - GET /v1/docs — Swagger UI HTML page

use axum::{
    response::{
        Html,
        Json
    },
};
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// OpenApiSpec builder
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct OpenApiSpec {
    spec: Value,
}

impl OpenApiSpec {
    pub fn new() -> Self {
        Self {
            spec: json!({
                "openapi": "3.0.3",
                "info": {
                    "title": "Xergon Relay API",
                    "description": "Thin stateless router that proxies inference requests to Xergon providers. OpenAI-compatible endpoints with smart routing, fallback chains, and SSE streaming.",
                    "version": env!("CARGO_PKG_VERSION"),
                    "contact": {
                        "name": "Xergon Network",
                        "url": "https://xergon.network"
                    }
                },
                "paths": {},
                "components": {
                    "schemas": {},
                    "securitySchemes": {}
                },
                "tags": []
            }),
        }
    }

    /// Add an HTTP endpoint to the spec.
    pub fn add_path(
        &mut self,
        method: &str,
        path: &str,
        summary: &str,
        description: &str,
        tags: &[&str],
        request_body_schema: Option<&str>,
        response_schemas: Option<&[(u16, &str, &str)]>, // (status, description, schema_ref)
    ) -> &mut Self {
        let method_lower = method.to_lowercase();
        let mut operation = json!({
            "summary": summary,
            "description": description,
            "tags": tags,
            "responses": {}
        });

        // Add request body if provided
        if let Some(schema_ref) = request_body_schema {
            operation["requestBody"] = json!({
                "required": true,
                "content": {
                    "application/json": {
                        "schema": { "$ref": format!("#/components/schemas/{schema_ref}") }
                    }
                }
            });
        }

        // Add responses
        if let Some(responses) = response_schemas {
            for (status, desc, schema_ref) in responses {
                let mut resp_obj = json!({
                    "description": desc
                });
                if !schema_ref.is_empty() {
                    resp_obj["content"] = json!({
                        "application/json": {
                            "schema": { "$ref": format!("#/components/schemas/{schema_ref}") }
                        }
                    });
                }
                operation["responses"][status.to_string()] = resp_obj;
            }
        }

        // Ensure the path entry exists
        if self.spec["paths"][path].is_null() {
            self.spec["paths"][path] = json!({});
        }
        self.spec["paths"][path][method_lower] = operation;

        self
    }

    /// Add a WebSocket endpoint (uses x-websocket special handling).
    pub fn add_websocket_path(
        &mut self,
        path: &str,
        summary: &str,
        description: &str,
        tags: &[&str],
    ) -> &mut Self {
        let mut operation = json!({
            "summary": summary,
            "description": description,
            "tags": tags,
            "responses": {
                "101": {
                    "description": "Switching Protocols — WebSocket connection established"
                }
            }
        });

        // Document the websocket upgrade
        operation["requestBody"] = json!({
            "description": "WebSocket upgrade request",
            "required": false
        });

        if self.spec["paths"][path].is_null() {
            self.spec["paths"][path] = json!({});
        }
        self.spec["paths"][path]["get"] = operation;

        self
    }

    /// Add a reusable component schema.
    pub fn add_component(&mut self, name: &str, schema: Value) -> &mut Self {
        self.spec["components"]["schemas"][name] = schema;
        self
    }

    /// Add a security scheme.
    pub fn add_security_scheme(&mut self, name: &str, scheme_type: &str, description: &str) -> &mut Self {
        let scheme = match scheme_type {
            "bearer" => json!({
                "type": "http",
                "scheme": "bearer",
                "bearerFormat": "JWT",
                "description": description
            }),
            "apiKey" => json!({
                "type": "apiKey",
                "in": "header",
                "name": "X-Auth-Token",
                "description": description
            }),
            "signature" => json!({
                "type": "http",
                "scheme": "signature",
                "description": description
            }),
            _ => json!({
                "type": scheme_type,
                "description": description
            }),
        };
        self.spec["components"]["securitySchemes"][name] = scheme;
        self
    }

    /// Add a tag.
    pub fn add_tag(&mut self, name: &str, description: &str) -> &mut Self {
        let tags = self.spec["tags"].as_array_mut().unwrap();
        tags.push(json!({
            "name": name,
            "description": description
        }));
        self
    }

    /// Serialize the spec to a pretty-printed JSON string.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(&self.spec).unwrap_or_default()
    }

    /// Get the spec as serde_json::Value.
    pub fn to_value(&self) -> Value {
        self.spec.clone()
    }
}

impl Default for OpenApiSpec {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Schema definitions
// ---------------------------------------------------------------------------

fn chat_completion_request_schema() -> Value {
    json!({
        "type": "object",
        "required": ["model", "messages"],
        "properties": {
            "model": {
                "type": "string",
                "description": "The model ID to use for completion"
            },
            "messages": {
                "type": "array",
                "description": "Array of chat messages",
                "items": {
                    "type": "object",
                    "required": ["role", "content"],
                    "properties": {
                        "role": {
                            "type": "string",
                            "enum": ["system", "user", "assistant"],
                            "description": "The role of the message author"
                        },
                        "content": {
                            "type": "string",
                            "description": "The content of the message"
                        }
                    }
                }
            },
            "stream": {
                "type": "boolean",
                "default": false,
                "description": "Whether to stream the response as SSE"
            },
            "temperature": {
                "type": "number",
                "minimum": 0.0,
                "maximum": 2.0,
                "default": 1.0,
                "description": "Sampling temperature"
            },
            "max_tokens": {
                "type": "integer",
                "minimum": 1,
                "description": "Maximum number of tokens to generate"
            }
        }
    })
}

fn chat_completion_response_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": {
                "type": "string",
                "description": "Unique completion ID"
            },
            "object": {
                "type": "string",
                "description": "Object type, e.g. 'chat.completion'"
            },
            "model": {
                "type": "string",
                "description": "The model used for completion"
            },
            "choices": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "index": { "type": "integer" },
                        "message": {
                            "type": "object",
                            "properties": {
                                "role": { "type": "string" },
                                "content": { "type": "string" }
                            }
                        },
                        "finish_reason": {
                            "type": "string",
                            "description": "Reason for completion: 'stop', 'length', 'content_filter'"
                        }
                    }
                }
            },
            "usage": {
                "type": "object",
                "properties": {
                    "prompt_tokens": { "type": "integer" },
                    "completion_tokens": { "type": "integer" },
                    "total_tokens": { "type": "integer" }
                }
            }
        }
    })
}

fn provider_onboard_request_schema() -> Value {
    json!({
        "type": "object",
        "required": ["endpoint", "region"],
        "properties": {
            "endpoint": {
                "type": "string",
                "format": "uri",
                "description": "The provider's inference endpoint URL"
            },
            "region": {
                "type": "string",
                "description": "Provider region code, e.g. 'us-east', 'eu-west'"
            },
            "auth_token": {
                "type": "string",
                "description": "Optional authentication token for the provider"
            }
        }
    })
}

fn error_response_schema() -> Value {
    json!({
        "type": "object",
        "required": ["error"],
        "properties": {
            "error": {
                "type": "object",
                "required": ["code", "message"],
                "properties": {
                    "code": {
                        "type": "string",
                        "description": "Machine-readable error code"
                    },
                    "message": {
                        "type": "string",
                        "description": "Human-readable error message"
                    }
                }
            }
        }
    })
}

fn model_list_response_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "object": { "type": "string", "description": "Object type, always 'list'" },
            "data": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Model ID" },
                        "object": { "type": "string" },
                        "created": { "type": "integer", "description": "Unix timestamp" },
                        "owned_by": { "type": "string" },
                        "context_length": { "type": "integer", "description": "Maximum context length in tokens" },
                        "pricing_nanoerg_per_million_tokens": { "type": "integer", "description": "Price in nanoERG per million tokens" }
                    }
                }
            }
        }
    })
}

fn provider_list_response_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "providers": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" },
                        "endpoint": { "type": "string" },
                        "region": { "type": "string" },
                        "healthy": { "type": "boolean" },
                        "models": {
                            "type": "array",
                            "items": { "type": "string" }
                        },
                        "latency_ms": { "type": "integer" }
                    }
                }
            }
        }
    })
}

fn health_response_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "status": { "type": "string" },
            "version": { "type": "string" },
            "uptime_secs": { "type": "integer" },
            "ergo_node_connected": { "type": "boolean" },
            "active_providers": { "type": "integer" },
            "degraded_providers": { "type": "integer" },
            "total_providers": { "type": "integer" }
        }
    })
}

fn balance_response_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "address": { "type": "string" },
            "nanoerg": { "type": "integer", "description": "Balance in nanoERG" },
            "erg": { "type": "number", "description": "Balance in ERG" }
        }
    })
}

fn onboarding_status_response_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "provider_pk": { "type": "string" },
            "status": { "type": "string", "enum": ["pending", "testing", "active", "failed"] },
            "endpoint": { "type": "string" },
            "tests_passed": { "type": "boolean" },
            "error": { "type": "string" }
        }
    })
}

// ---------------------------------------------------------------------------
// Build the full spec
// ---------------------------------------------------------------------------

/// Build the complete OpenAPI spec for all xergon-relay endpoints.
pub fn build_openapi_spec() -> OpenApiSpec {
    let mut spec = OpenApiSpec::new();

    // Tags
    spec.add_tag("Chat", "AI chat completion endpoints (OpenAI-compatible)")
        .add_tag("Models", "Model discovery and details")
        .add_tag("Providers", "Provider management, onboarding, and status")
        .add_tag("System", "Health, metrics, and relay statistics")
        .add_tag("Balance", "On-chain balance checking")
        .add_tag("GPU", "GPU marketplace endpoints")
        .add_tag("Auth", "Authentication status and verification")
        .add_tag("Incentive", "Rarity incentive system")
        .add_tag("Bridge", "Cross-chain payment bridge")
        .add_tag("API", "API versioning and documentation");

    // Security schemes
    spec.add_security_scheme(
        "BearerAuth",
        "bearer",
        "Ergo signature-based authentication token",
    )
    .add_security_scheme(
        "SignatureAuth",
        "signature",
        "Ergo wallet signature authentication (X-Signature, X-Timestamp, X-Public-Key headers)",
    );

    // ---- Chat endpoints ----
    spec.add_path(
        "post",
        "/v1/chat/completions",
        "Create chat completion",
        "Sends messages to an AI model and returns a completion response. Supports SSE streaming when `stream: true`.",
        &["Chat"],
        Some("ChatCompletionRequest"),
        Some(&[
            (200, "Successful completion", "ChatCompletionResponse"),
            (400, "Invalid request body", "ErrorResponse"),
            (401, "Authentication required", "ErrorResponse"),
            (429, "Rate limit exceeded", "ErrorResponse"),
            (500, "Internal server error", "ErrorResponse"),
        ]),
    );

    spec.add_websocket_path(
        "/v1/chat/ws",
        "WebSocket chat transport",
        "Real-time bidirectional chat via WebSocket. Send messages as JSON frames and receive streaming responses.",
        &["Chat"],
    );

    // ---- Model endpoints ----
    spec.add_path(
        "get",
        "/v1/models",
        "List available models",
        "Returns a list of all models available across healthy providers.",
        &["Models"],
        None,
        Some(&[
            (200, "List of models", "ModelListResponse"),
            (500, "Internal server error", "ErrorResponse"),
        ]),
    );

    spec.add_path(
        "get",
        "/v1/models/{model_id}",
        "Get model details",
        "Returns detailed information about a specific model including pricing and provider availability.",
        &["Models"],
        None,
        Some(&[
            (200, "Model details", "ModelListResponse"),
            (404, "Model not found", "ErrorResponse"),
            (500, "Internal server error", "ErrorResponse"),
        ]),
    );

    // ---- Provider endpoints ----
    spec.add_path(
        "get",
        "/v1/providers",
        "List providers",
        "Returns all registered providers with their health status, supported models, and latency.",
        &["Providers"],
        None,
        Some(&[
            (200, "List of providers", "ProviderListResponse"),
            (500, "Internal server error", "ErrorResponse"),
        ]),
    );

    spec.add_path(
        "post",
        "/v1/providers/onboard",
        "Onboard new provider",
        "Register a new provider endpoint. The relay will health-check the provider before activation.",
        &["Providers"],
        Some("ProviderOnboardRequest"),
        Some(&[
            (200, "Provider onboarded successfully", "OnboardingStatusResponse"),
            (400, "Invalid request", "ErrorResponse"),
            (409, "Provider already registered", "ErrorResponse"),
            (500, "Internal server error", "ErrorResponse"),
        ]),
    );

    spec.add_path(
        "get",
        "/v1/providers/onboard/{provider_pk}",
        "Get onboarding status",
        "Returns the current onboarding status for a provider by their public key.",
        &["Providers"],
        None,
        Some(&[
            (200, "Onboarding status", "OnboardingStatusResponse"),
            (404, "Provider not found", "ErrorResponse"),
            (500, "Internal server error", "ErrorResponse"),
        ]),
    );

    spec.add_path(
        "post",
        "/v1/providers/onboard/{provider_pk}/test",
        "Test provider",
        "Runs a health check and inference test against the specified provider.",
        &["Providers"],
        None,
        Some(&[
            (200, "Test completed", "OnboardingStatusResponse"),
            (404, "Provider not found", "ErrorResponse"),
            (500, "Test failed", "ErrorResponse"),
        ]),
    );

    spec.add_path(
        "delete",
        "/v1/providers/{provider_pk}",
        "Deregister provider",
        "Removes a provider from the relay registry.",
        &["Providers"],
        None,
        Some(&[
            (200, "Provider deregistered", "OnboardingStatusResponse"),
            (404, "Provider not found", "ErrorResponse"),
            (500, "Internal server error", "ErrorResponse"),
        ]),
    );

    // ---- System endpoints ----
    spec.add_path(
        "get",
        "/v1/health",
        "Health check",
        "Returns relay health status including uptime, provider counts, and chain connectivity.",
        &["System"],
        None,
        Some(&[
            (200, "Health status", "HealthResponse"),
            (500, "Internal server error", "ErrorResponse"),
        ]),
    );

    spec.add_path(
        "get",
        "/v1/balance/{user_pk}",
        "Check user balance",
        "Returns the ERG balance for a given public key address on the Ergo blockchain.",
        &["Balance"],
        None,
        Some(&[
            (200, "Balance info", "BalanceResponse"),
            (400, "Invalid address", "ErrorResponse"),
            (500, "Internal server error", "ErrorResponse"),
        ]),
    );

    // ---- API documentation endpoints ----
    spec.add_path(
        "get",
        "/v1/openapi.json",
        "OpenAPI specification",
        "Returns the OpenAPI 3.0.3 specification for the relay API.",
        &["API"],
        None,
        Some(&[
            (200, "OpenAPI spec", ""),
        ]),
    );

    spec.add_path(
        "get",
        "/v1/docs",
        "API documentation",
        "Serves the Swagger UI HTML page for interactive API documentation.",
        &["API"],
        None,
        Some(&[
            (200, "HTML documentation page", ""),
        ]),
    );

    spec.add_path(
        "get",
        "/api/versions",
        "List API versions",
        "Returns a list of all supported API versions with their status and metadata.",
        &["API"],
        None,
        Some(&[
            (200, "Version list", ""),
        ]),
    );

    // ---- Component schemas ----
    spec.add_component("ChatCompletionRequest", chat_completion_request_schema())
        .add_component("ChatCompletionResponse", chat_completion_response_schema())
        .add_component("ProviderOnboardRequest", provider_onboard_request_schema())
        .add_component("ErrorResponse", error_response_schema())
        .add_component("ModelListResponse", model_list_response_schema())
        .add_component("ProviderListResponse", provider_list_response_schema())
        .add_component("HealthResponse", health_response_schema())
        .add_component("BalanceResponse", balance_response_schema())
        .add_component("OnboardingStatusResponse", onboarding_status_response_schema());

    spec
}

// ---------------------------------------------------------------------------
// Axum handlers
// ---------------------------------------------------------------------------

/// Handler for GET /v1/openapi.json
pub async fn openapi_spec_handler() -> Json<Value> {
    let spec = build_openapi_spec();
    Json(spec.to_value())
}

/// Handler for GET /v1/docs — serves Swagger UI
pub async fn docs_handler() -> Html<String> {
    let html = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Xergon Relay API Documentation</title>
    <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css">
    <style>
        body { margin: 0; background: #fafafa; }
        .swagger-ui .topbar { display: none; }
        .swagger-ui .info .title { color: #1a1a2e; }
    </style>
</head>
<body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
    <script>
        window.onload = function() {
            SwaggerUIBundle({
                url: "/v1/openapi.json",
                dom_id: '#swagger-ui',
                deepLinking: true,
                displayRequestDuration: true,
                filter: true,
                tryItOutEnabled: true,
                presets: [
                    SwaggerUIBundle.presets.apis,
                    SwaggerUIBundle.SwaggerUIStandalonePreset
                ],
                layout: "StandaloneLayout"
            });
        }
    </script>
</body>
</html>"#;

    Html(html.to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spec_is_valid_openapi() {
        let spec = build_openapi_spec();
        let value = spec.to_value();
        assert_eq!(value["openapi"], "3.0.3");
        assert_eq!(value["info"]["title"], "Xergon Relay API");
    }

    #[test]
    fn test_spec_contains_all_paths() {
        let spec = build_openapi_spec();
        let value = spec.to_value();
        let paths = value["paths"].as_object().unwrap();
        let expected_paths = [
            "/v1/chat/completions",
            "/v1/chat/ws",
            "/v1/models",
            "/v1/models/{model_id}",
            "/v1/providers",
            "/v1/providers/onboard",
            "/v1/providers/onboard/{provider_pk}",
            "/v1/providers/onboard/{provider_pk}/test",
            "/v1/providers/{provider_pk}",
            "/v1/health",
            "/v1/balance/{user_pk}",
            "/v1/openapi.json",
            "/v1/docs",
            "/api/versions",
        ];
        for path in &expected_paths {
            assert!(
                paths.contains_key(*path),
                "Missing expected path: {path}"
            );
        }
    }

    #[test]
    fn test_spec_contains_all_tags() {
        let spec = build_openapi_spec();
        let value = spec.to_value();
        let tags = value["tags"].as_array().unwrap();
        let tag_names: Vec<&str> = tags.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(tag_names.contains(&"Chat"));
        assert!(tag_names.contains(&"Models"));
        assert!(tag_names.contains(&"Providers"));
        assert!(tag_names.contains(&"System"));
        assert!(tag_names.contains(&"Balance"));
        assert!(tag_names.contains(&"GPU"));
        assert!(tag_names.contains(&"Auth"));
        assert!(tag_names.contains(&"Incentive"));
        assert!(tag_names.contains(&"Bridge"));
        assert!(tag_names.contains(&"API"));
    }

    #[test]
    fn test_spec_contains_all_component_schemas() {
        let spec = build_openapi_spec();
        let value = spec.to_value();
        let schemas = value["components"]["schemas"].as_object().unwrap();
        let expected_schemas = [
            "ChatCompletionRequest",
            "ChatCompletionResponse",
            "ProviderOnboardRequest",
            "ErrorResponse",
            "ModelListResponse",
            "ProviderListResponse",
            "HealthResponse",
            "BalanceResponse",
            "OnboardingStatusResponse",
        ];
        for schema in &expected_schemas {
            assert!(
                schemas.contains_key(*schema),
                "Missing expected schema: {schema}"
            );
        }
    }

    #[test]
    fn test_chat_completion_has_required_fields() {
        let spec = build_openapi_spec();
        let value = spec.to_value();
        let request_schema = &value["components"]["schemas"]["ChatCompletionRequest"];
        let required = request_schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("model")));
        assert!(required.contains(&json!("messages")));
    }

    #[test]
    fn test_spec_to_json_is_valid() {
        let spec = build_openapi_spec();
        let json_str = spec.to_json();
        let parsed: Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["openapi"], "3.0.3");
    }

    #[test]
    fn test_spec_has_security_schemes() {
        let spec = build_openapi_spec();
        let schemes = &spec.to_value()["components"]["securitySchemes"];
        assert!(schemes.get("BearerAuth").is_some());
        assert!(schemes.get("SignatureAuth").is_some());
    }

    #[test]
    fn test_error_response_schema() {
        let schema = error_response_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("error")));
        let error_props = &schema["properties"]["error"]["required"];
        let error_required = error_props.as_array().unwrap();
        assert!(error_required.contains(&json!("code")));
        assert!(error_required.contains(&json!("message")));
    }
}
