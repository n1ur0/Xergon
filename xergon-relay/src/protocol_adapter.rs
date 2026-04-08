#![allow(dead_code)]
//! Protocol Adapter — Normalize requests across LLM providers
//!
//! Provides a unified request/response format that abstracts away differences
//! between provider APIs (OpenAI, Anthropic, Gemini, Ollama, XergonNative).
//! Supports bidirectional conversion and automatic schema detection.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info};

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Supported provider protocols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderProtocol {
    /// OpenAI Chat Completions API
    OpenAI,
    /// Anthropic Messages API
    Anthropic,
    /// Google Gemini generateContent API
    Gemini,
    /// Ollama local model API
    Ollama,
    /// Xergon native inference protocol
    XergonNative,
}

impl ProviderProtocol {
    /// Parse a protocol from a string (case-insensitive).
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "openai" => Some(ProviderProtocol::OpenAI),
            "anthropic" | "claude" => Some(ProviderProtocol::Anthropic),
            "gemini" | "google" => Some(ProviderProtocol::Gemini),
            "ollama" | "local" => Some(ProviderProtocol::Ollama),
            "xergon" | "xergonnative" | "native" => Some(ProviderProtocol::XergonNative),
            _ => None,
        }
    }

    /// Returns the default base path for this protocol's chat endpoint.
    pub fn chat_endpoint(&self) -> &str {
        match self {
            ProviderProtocol::OpenAI => "/v1/chat/completions",
            ProviderProtocol::Anthropic => "/v1/messages",
            ProviderProtocol::Gemini => "/v1beta/models/{model}:generateContent",
            ProviderProtocol::Ollama => "/api/chat",
            ProviderProtocol::XergonNative => "/v1/inference",
        }
    }
}

impl std::fmt::Display for ProviderProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderProtocol::OpenAI => write!(f, "openai"),
            ProviderProtocol::Anthropic => write!(f, "anthropic"),
            ProviderProtocol::Gemini => write!(f, "gemini"),
            ProviderProtocol::Ollama => write!(f, "ollama"),
            ProviderProtocol::XergonNative => write!(f, "xergon_native"),
        }
    }
}

/// Message role in a conversation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

impl MessageRole {
    /// Parse a role from a string.
    pub fn from_str_loose(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "system" => MessageRole::System,
            "user" => MessageRole::User,
            "assistant" | "model" => MessageRole::Assistant,
            "tool" | "function" => MessageRole::Tool,
            _ => MessageRole::User,
        }
    }

    /// Convert to a string representation.
    pub fn as_str(&self) -> &str {
        match self {
            MessageRole::System => "system",
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
        }
    }
}

// ---------------------------------------------------------------------------
// Normalized Types
// ---------------------------------------------------------------------------

/// A normalized message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedMessage {
    /// Message role
    pub role: MessageRole,
    /// Message content
    pub content: String,
    /// Optional name for tool/function messages
    pub name: Option<String>,
}

/// A normalized inference request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedRequest {
    /// Model identifier
    pub model_id: String,
    /// Conversation messages
    pub messages: Vec<NormalizedMessage>,
    /// Sampling temperature (0.0 - 2.0)
    pub temperature: Option<f64>,
    /// Maximum tokens to generate
    pub max_tokens: Option<u32>,
    /// Whether to stream the response
    pub stream: Option<bool>,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl NormalizedRequest {
    /// Extract the last user message as the prompt.
    pub fn last_user_message(&self) -> Option<&str> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::User)
            .map(|m| m.content.as_str())
    }

    /// Calculate approximate prompt size in bytes.
    pub fn prompt_size_bytes(&self) -> usize {
        self.messages
            .iter()
            .map(|m| m.content.len())
            .sum()
    }

    /// Get the system message if present.
    pub fn system_message(&self) -> Option<&str> {
        self.messages
            .iter()
            .find(|m| m.role == MessageRole::System)
            .map(|m| m.content.as_str())
    }
}

/// Token usage statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Number of tokens in the prompt
    pub prompt_tokens: u32,
    /// Number of tokens in the completion
    pub completion_tokens: u32,
    /// Total tokens (prompt + completion)
    pub total_tokens: u32,
}

impl TokenUsage {
    /// Create a new TokenUsage from prompt and completion counts.
    pub fn new(prompt_tokens: u32, completion_tokens: u32) -> Self {
        TokenUsage {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        }
    }
}

/// Finish reason for a completion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ContentFilter,
    ToolCalls,
    Unknown,
}

impl std::fmt::Display for FinishReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FinishReason::Stop => write!(f, "stop"),
            FinishReason::Length => write!(f, "length"),
            FinishReason::ContentFilter => write!(f, "content_filter"),
            FinishReason::ToolCalls => write!(f, "tool_calls"),
            FinishReason::Unknown => write!(f, "unknown"),
        }
    }
}

impl FinishReason {
    /// Parse from a string (case-insensitive).
    pub fn from_str_loose(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "stop" | "end_turn" => FinishReason::Stop,
            "length" | "max_tokens" => FinishReason::Length,
            "content_filter" => FinishReason::ContentFilter,
            "tool_calls" | "tool_use" => FinishReason::ToolCalls,
            _ => FinishReason::Unknown,
        }
    }
}

/// A normalized inference response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedResponse {
    /// Response content text
    pub content: String,
    /// Token usage statistics
    pub tokens_used: TokenUsage,
    /// Model that generated the response
    pub model: String,
    /// Finish reason
    pub finish_reason: FinishReason,
    /// Response latency in milliseconds
    pub latency_ms: u64,
    /// Provider protocol that generated this response
    pub provider: ProviderProtocol,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Provider Schema
// ---------------------------------------------------------------------------

/// Schema information for a registered provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSchema {
    /// Provider protocol type
    pub protocol: ProviderProtocol,
    /// Provider display name
    pub name: String,
    /// Provider endpoint base URL
    pub endpoint: String,
    /// Supported models
    pub supported_models: Vec<String>,
    /// Whether the provider supports streaming
    pub supports_streaming: bool,
    /// Maximum context window in tokens
    pub max_context_tokens: Option<u32>,
    /// Additional capabilities
    pub capabilities: HashMap<String, serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Provider Mapping
// ---------------------------------------------------------------------------

/// Internal mapping of a provider endpoint to its protocol configuration.
#[derive(Debug, Clone)]
struct ProviderMapping {
    /// Provider endpoint URL
    endpoint: String,
    /// Protocol type
    protocol: ProviderProtocol,
    /// Provider schema
    schema: ProviderSchema,
    /// Registration timestamp
    registered_at: chrono::DateTime<chrono::Utc>,
}

// ---------------------------------------------------------------------------
// Protocol Adapter
// ---------------------------------------------------------------------------

/// Normalizes requests and responses across different LLM provider protocols.
///
/// Maintains a registry of provider-to-protocol mappings and provides
/// bidirectional conversion between normalized and provider-specific formats.
pub struct ProtocolAdapter {
    /// Provider endpoint -> protocol mapping
    provider_mappings: DashMap<String, ProviderMapping>,
    /// Schema cache by protocol type
    schema_configs: DashMap<ProviderProtocol, ProviderSchema>,
}

impl ProtocolAdapter {
    /// Create a new protocol adapter with default schemas.
    pub fn new() -> Self {
        let adapter = ProtocolAdapter {
            provider_mappings: DashMap::new(),
            schema_configs: DashMap::new(),
        };

        // Register default schemas
        adapter.register_default_schemas();

        adapter
    }

    /// Register default schemas for all known protocols.
    fn register_default_schemas(&self) {
        let openai_schema = ProviderSchema {
            protocol: ProviderProtocol::OpenAI,
            name: "OpenAI".to_string(),
            endpoint: String::new(),
            supported_models: vec![
                "gpt-4o".to_string(),
                "gpt-4o-mini".to_string(),
                "gpt-4-turbo".to_string(),
                "gpt-3.5-turbo".to_string(),
            ],
            supports_streaming: true,
            max_context_tokens: Some(128_000),
            capabilities: HashMap::new(),
        };

        let anthropic_schema = ProviderSchema {
            protocol: ProviderProtocol::Anthropic,
            name: "Anthropic".to_string(),
            endpoint: String::new(),
            supported_models: vec![
                "claude-opus-4-20250514".to_string(),
                "claude-sonnet-4-20250514".to_string(),
                "claude-3-5-haiku-20241022".to_string(),
            ],
            supports_streaming: true,
            max_context_tokens: Some(200_000),
            capabilities: HashMap::new(),
        };

        let gemini_schema = ProviderSchema {
            protocol: ProviderProtocol::Gemini,
            name: "Gemini".to_string(),
            endpoint: String::new(),
            supported_models: vec![
                "gemini-2.5-pro".to_string(),
                "gemini-2.5-flash".to_string(),
                "gemini-2.0-flash".to_string(),
            ],
            supports_streaming: true,
            max_context_tokens: Some(1_000_000),
            capabilities: HashMap::new(),
        };

        let ollama_schema = ProviderSchema {
            protocol: ProviderProtocol::Ollama,
            name: "Ollama".to_string(),
            endpoint: String::new(),
            supported_models: vec![
                "llama3.1:70b".to_string(),
                "mistral:7b".to_string(),
                "codellama:13b".to_string(),
            ],
            supports_streaming: true,
            max_context_tokens: Some(32_000),
            capabilities: HashMap::new(),
        };

        let xergon_schema = ProviderSchema {
            protocol: ProviderProtocol::XergonNative,
            name: "Xergon Native".to_string(),
            endpoint: String::new(),
            supported_models: Vec::new(), // dynamically populated
            supports_streaming: true,
            max_context_tokens: None,
            capabilities: HashMap::new(),
        };

        self.schema_configs
            .insert(ProviderProtocol::OpenAI, openai_schema);
        self.schema_configs
            .insert(ProviderProtocol::Anthropic, anthropic_schema);
        self.schema_configs
            .insert(ProviderProtocol::Gemini, gemini_schema);
        self.schema_configs
            .insert(ProviderProtocol::Ollama, ollama_schema);
        self.schema_configs
            .insert(ProviderProtocol::XergonNative, xergon_schema);
    }

    // -----------------------------------------------------------------------
    // Provider Registration
    // -----------------------------------------------------------------------

    /// Register a provider with its protocol mapping.
    pub fn register_provider(
        &self,
        endpoint: &str,
        protocol: ProviderProtocol,
        name: &str,
        supported_models: Vec<String>,
    ) {
        let model_count = supported_models.len();
        let schema = ProviderSchema {
            protocol,
            name: name.to_string(),
            endpoint: endpoint.to_string(),
            supported_models,
            supports_streaming: true,
            max_context_tokens: None,
            capabilities: HashMap::new(),
        };

        let mapping = ProviderMapping {
            endpoint: endpoint.to_string(),
            protocol,
            schema: schema.clone(),
            registered_at: chrono::Utc::now(),
        };

        self.provider_mappings.insert(endpoint.to_string(), mapping);
        self.schema_configs.insert(protocol, schema);

        info!(
            endpoint = %endpoint,
            protocol = %protocol,
            models = model_count,
            "Provider registered with protocol adapter"
        );
    }

    /// Remove a provider mapping.
    pub fn unregister_provider(&self, endpoint: &str) -> bool {
        if self.provider_mappings.remove(endpoint).is_some() {
            info!(endpoint = %endpoint, "Provider unregistered from protocol adapter");
            true
        } else {
            false
        }
    }

    /// Get the protocol for a provider endpoint.
    pub fn get_provider_protocol(&self, endpoint: &str) -> Option<ProviderProtocol> {
        self.provider_mappings
            .get(endpoint)
            .map(|m| m.protocol)
    }

    /// Get the schema for a registered provider.
    pub fn get_provider_schema(&self, endpoint: &str) -> Option<ProviderSchema> {
        self.provider_mappings
            .get(endpoint)
            .map(|m| m.schema.clone())
    }

    /// Get the default schema for a protocol type.
    pub fn get_protocol_schema(&self, protocol: ProviderProtocol) -> Option<ProviderSchema> {
        self.schema_configs.get(&protocol).map(|s| s.clone())
    }

    /// List all registered providers.
    pub fn list_providers(&self) -> Vec<ProviderSchema> {
        self.provider_mappings
            .iter()
            .map(|e| e.value().schema.clone())
            .collect()
    }

    /// Detect the protocol from a raw JSON request body.
    pub fn detect_protocol(&self, body: &serde_json::Value) -> Option<ProviderProtocol> {
        // Check for Anthropic-specific fields
        if body.get("anthropic_version").is_some() {
            return Some(ProviderProtocol::Anthropic);
        }

        // Check for Gemini-specific fields
        if body.get("contents").is_some() && body.get("generationConfig").is_some() {
            return Some(ProviderProtocol::Gemini);
        }

        // Check for Ollama-specific fields
        if body.get("options").is_some() && body.get("template").is_some() {
            return Some(ProviderProtocol::Ollama);
        }

        // Check for Xergon-specific fields
        if body.get("xergon_version").is_some() || body.get("inference_id").is_some() {
            return Some(ProviderProtocol::XergonNative);
        }

        // Default to OpenAI format (most common)
        if body.get("model").is_some()
            && (body.get("messages").is_some() || body.get("prompt").is_some())
        {
            return Some(ProviderProtocol::OpenAI);
        }

        None
    }

    // -----------------------------------------------------------------------
    // Request Normalization
    // -----------------------------------------------------------------------

    /// Normalize a request from any provider format.
    pub fn normalize_request(
        &self,
        protocol: ProviderProtocol,
        body: &serde_json::Value,
    ) -> Result<NormalizedRequest, String> {
        match protocol {
            ProviderProtocol::OpenAI => self.convert_openai(body),
            ProviderProtocol::Anthropic => self.convert_anthropic(body),
            ProviderProtocol::Gemini => self.convert_gemini(body),
            ProviderProtocol::Ollama => self.convert_ollama(body),
            ProviderProtocol::XergonNative => self.convert_xergon(body),
        }
    }

    /// Normalize a response from any provider format.
    pub fn denormalize_response(
        &self,
        protocol: ProviderProtocol,
        body: &serde_json::Value,
        latency_ms: u64,
    ) -> Result<NormalizedResponse, String> {
        match protocol {
            ProviderProtocol::OpenAI => self.denormalize_openai_response(body, latency_ms),
            ProviderProtocol::Anthropic => self.denormalize_anthropic_response(body, latency_ms),
            ProviderProtocol::Gemini => self.denormalize_gemini_response(body, latency_ms),
            ProviderProtocol::Ollama => self.denormalize_ollama_response(body, latency_ms),
            ProviderProtocol::XergonNative => self.denormalize_xergon_response(body, latency_ms),
        }
    }

    // -----------------------------------------------------------------------
    // OpenAI Conversion
    // -----------------------------------------------------------------------

    /// Convert OpenAI chat/completions format to normalized.
    pub fn convert_openai(&self, body: &serde_json::Value) -> Result<NormalizedRequest, String> {
        let model_id = body
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let mut messages = Vec::new();

        if let Some(msgs) = body.get("messages").and_then(|v| v.as_array()) {
            for msg in msgs {
                let role = msg
                    .get("role")
                    .and_then(|v| v.as_str())
                    .unwrap_or("user");
                let content = msg
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let name = msg
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                messages.push(NormalizedMessage {
                    role: MessageRole::from_str_loose(role),
                    content: content.to_string(),
                    name,
                });
            }
        } else if let Some(prompt) = body.get("prompt").and_then(|v| v.as_str()) {
            // Completions API (legacy)
            messages.push(NormalizedMessage {
                role: MessageRole::User,
                content: prompt.to_string(),
                name: None,
            });
        }

        let temperature = body.get("temperature").and_then(|v| v.as_f64());
        let max_tokens = body.get("max_tokens").and_then(|v| v.as_u64()).map(|v| v as u32);
        let stream = body.get("stream").and_then(|v| v.as_bool());

        let metadata: HashMap<String, serde_json::Value> = body
            .as_object()
            .map(|obj| {
                obj.iter()
                    .filter(|(k, _)| {
                        !matches!(
                            k.as_str(),
                            "model" | "messages" | "prompt" | "temperature"
                                | "max_tokens" | "stream"
                        )
                    })
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            })
            .unwrap_or_default();

        Ok(NormalizedRequest {
            model_id,
            messages,
            temperature,
            max_tokens,
            stream,
            metadata,
        })
    }

    /// Convert a normalized response to OpenAI format.
    pub fn denormalize_openai_response(
        &self,
        body: &serde_json::Value,
        latency_ms: u64,
    ) -> Result<NormalizedResponse, String> {
        let content = body
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let model = body
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let usage = body.get("usage").map(|u| {
            TokenUsage::new(
                u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                u.get("completion_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32,
            )
        }).unwrap_or_default();

        let finish_reason = body
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("finish_reason"))
            .and_then(|v| v.as_str())
            .map(|s| FinishReason::from_str_loose(s))
            .unwrap_or(FinishReason::Unknown);

        Ok(NormalizedResponse {
            content,
            tokens_used: usage,
            model,
            finish_reason,
            latency_ms,
            provider: ProviderProtocol::OpenAI,
            metadata: HashMap::new(),
        })
    }

    // -----------------------------------------------------------------------
    // Anthropic Conversion
    // -----------------------------------------------------------------------

    /// Convert Anthropic messages format to normalized.
    pub fn convert_anthropic(&self, body: &serde_json::Value) -> Result<NormalizedRequest, String> {
        let model_id = body
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let mut messages = Vec::new();

        // Anthropic puts system prompt in a separate top-level field
        if let Some(system) = body.get("system") {
            let content = if system.is_string() {
                system.as_str().unwrap_or("").to_string()
            } else if let Some(arr) = system.as_array() {
                arr.iter()
                    .filter_map(|item| item.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                String::new()
            };
            if !content.is_empty() {
                messages.push(NormalizedMessage {
                    role: MessageRole::System,
                    content,
                    name: None,
                });
            }
        }

        if let Some(msgs) = body.get("messages").and_then(|v| v.as_array()) {
            for msg in msgs {
                let role = msg
                    .get("role")
                    .and_then(|v| v.as_str())
                    .unwrap_or("user");

                // Anthropic content can be a string or an array of content blocks
                let content = if let Some(s) = msg.get("content").and_then(|v| v.as_str()) {
                    s.to_string()
                } else if let Some(arr) = msg.get("content").and_then(|v| v.as_array()) {
                    arr.iter()
                        .filter_map(|block| {
                            block.get("text").and_then(|t| t.as_str())
                        })
                        .collect::<Vec<_>>()
                        .join("")
                } else {
                    String::new()
                };

                messages.push(NormalizedMessage {
                    role: MessageRole::from_str_loose(role),
                    content,
                    name: None,
                });
            }
        }

        let temperature = body.get("temperature").and_then(|v| v.as_f64());
        let max_tokens = body
            .get("max_tokens")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);
        let stream = body.get("stream").and_then(|v| v.as_bool());

        Ok(NormalizedRequest {
            model_id,
            messages,
            temperature,
            max_tokens,
            stream,
            metadata: HashMap::new(),
        })
    }

    /// Convert a normalized response to Anthropic format.
    fn denormalize_anthropic_response(
        &self,
        body: &serde_json::Value,
        latency_ms: u64,
    ) -> Result<NormalizedResponse, String> {
        let content = body
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.get(0))
            .and_then(|block| block.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let model = body
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let usage = body.get("usage").map(|u| {
            TokenUsage::new(
                u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                u.get("output_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32,
            )
        }).unwrap_or_default();

        let finish_reason = body
            .get("stop_reason")
            .and_then(|v| v.as_str())
            .map(|s| FinishReason::from_str_loose(s))
            .unwrap_or(FinishReason::Unknown);

        Ok(NormalizedResponse {
            content,
            tokens_used: usage,
            model,
            finish_reason,
            latency_ms,
            provider: ProviderProtocol::Anthropic,
            metadata: HashMap::new(),
        })
    }

    // -----------------------------------------------------------------------
    // Gemini Conversion
    // -----------------------------------------------------------------------

    /// Convert Gemini generateContent format to normalized.
    pub fn convert_gemini(&self, body: &serde_json::Value) -> Result<NormalizedRequest, String> {
        // Gemini uses "contents" array with "role" and "parts"
        let mut messages = Vec::new();

        if let Some(contents) = body.get("contents").and_then(|v| v.as_array()) {
            for item in contents {
                let role = item
                    .get("role")
                    .and_then(|v| v.as_str())
                    .unwrap_or("user");
                let content = item
                    .get("parts")
                    .and_then(|p| p.as_array())
                    .map(|parts| {
                        parts
                            .iter()
                            .filter_map(|part| part.get("text").and_then(|t| t.as_str()))
                            .collect::<Vec<_>>()
                            .join("")
                    })
                    .unwrap_or_default();

                if !content.is_empty() {
                    messages.push(NormalizedMessage {
                        role: MessageRole::from_str_loose(role),
                        content,
                        name: None,
                    });
                }
            }
        }

        // System instruction is a separate field in Gemini
        if let Some(sys) = body
            .get("systemInstruction")
            .or_else(|| body.get("system_instruction"))
        {
            let content = sys
                .get("parts")
                .and_then(|p| p.as_array())
                .map(|parts| {
                    parts
                        .iter()
                        .filter_map(|part| part.get("text").and_then(|t| t.as_str()))
                        .collect::<Vec<_>>()
                        .join("")
                })
                .unwrap_or_default();

            if !content.is_empty() {
                messages.insert(
                    0,
                    NormalizedMessage {
                        role: MessageRole::System,
                        content,
                        name: None,
                    },
                );
            }
        }

        // Generation config
        let gen_config = body.get("generationConfig").or(body.get("generation_config"));
        let temperature = gen_config
            .and_then(|c| c.get("temperature"))
            .and_then(|v| v.as_f64());
        let max_tokens = gen_config
            .and_then(|c| c.get("maxOutputTokens"))
            .or_else(|| gen_config.and_then(|c| c.get("max_output_tokens")))
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);

        // Model is in the URL path, not the body. Use "unknown" or try to extract.
        let model_id = body
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(NormalizedRequest {
            model_id,
            messages,
            temperature,
            max_tokens,
            stream: None,
            metadata: HashMap::new(),
        })
    }

    /// Convert a normalized response from Gemini format.
    fn denormalize_gemini_response(
        &self,
        body: &serde_json::Value,
        latency_ms: u64,
    ) -> Result<NormalizedResponse, String> {
        let content = body
            .get("candidates")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("content"))
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.as_array())
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();

        let model = body
            .get("modelVersion")
            .or(body.get("model"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let usage = body
            .get("usageMetadata")
            .or(body.get("usage_metadata"))
            .map(|u| {
                TokenUsage::new(
                    u.get("promptTokenCount")
                        .or(u.get("prompt_token_count"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as u32,
                    u.get("candidatesTokenCount")
                        .or(u.get("candidates_token_count"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as u32,
                )
            })
            .unwrap_or_default();

        let finish_reason = body
            .get("candidates")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("finishReason"))
            .or_else(|| {
                body.get("candidates")
                    .and_then(|c| c.get(0))
                    .and_then(|c| c.get("finish_reason"))
            })
            .and_then(|v| v.as_str())
            .map(|s| FinishReason::from_str_loose(s))
            .unwrap_or(FinishReason::Unknown);

        Ok(NormalizedResponse {
            content,
            tokens_used: usage,
            model,
            finish_reason,
            latency_ms,
            provider: ProviderProtocol::Gemini,
            metadata: HashMap::new(),
        })
    }

    // -----------------------------------------------------------------------
    // Ollama Conversion
    // -----------------------------------------------------------------------

    /// Convert Ollama chat API format to normalized.
    pub fn convert_ollama(&self, body: &serde_json::Value) -> Result<NormalizedRequest, String> {
        let model_id = body
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let mut messages = Vec::new();

        if let Some(msgs) = body.get("messages").and_then(|v| v.as_array()) {
            for msg in msgs {
                let role = msg
                    .get("role")
                    .and_then(|v| v.as_str())
                    .unwrap_or("user");
                let content = msg
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                messages.push(NormalizedMessage {
                    role: MessageRole::from_str_loose(role),
                    content: content.to_string(),
                    name: None,
                });
            }
        } else if let Some(prompt) = body.get("prompt").and_then(|v| v.as_str()) {
            // /api/generate format
            messages.push(NormalizedMessage {
                role: MessageRole::User,
                content: prompt.to_string(),
                name: None,
            });
        }

        let options = body.get("options");
        let temperature = options
            .and_then(|o| o.get("temperature"))
            .and_then(|v| v.as_f64());
        let num_predict = options
            .and_then(|o| o.get("num_predict"))
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);
        let stream = body.get("stream").and_then(|v| v.as_bool());

        Ok(NormalizedRequest {
            model_id,
            messages,
            temperature,
            max_tokens: num_predict,
            stream,
            metadata: HashMap::new(),
        })
    }

    /// Convert a normalized response from Ollama format.
    fn denormalize_ollama_response(
        &self,
        body: &serde_json::Value,
        latency_ms: u64,
    ) -> Result<NormalizedResponse, String> {
        let content = body
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|v| v.as_str())
            .or_else(|| body.get("response").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string();

        let model = body
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let usage = body
            .get("prompt_eval_count")
            .zip(body.get("eval_count"))
            .map(|(p, c)| {
                TokenUsage::new(
                    p.as_u64().unwrap_or(0) as u32,
                    c.as_u64().unwrap_or(0) as u32,
                )
            })
            .unwrap_or_default();

        let finish_reason = body
            .get("done")
            .and_then(|v| v.as_bool())
            .map(|done| {
                if done {
                    FinishReason::Stop
                } else {
                    FinishReason::Unknown
                }
            })
            .unwrap_or(FinishReason::Unknown);

        Ok(NormalizedResponse {
            content,
            tokens_used: usage,
            model,
            finish_reason,
            latency_ms,
            provider: ProviderProtocol::Ollama,
            metadata: HashMap::new(),
        })
    }

    // -----------------------------------------------------------------------
    // Xergon Native Conversion
    // -----------------------------------------------------------------------

    /// Convert Xergon native inference format to normalized.
    fn convert_xergon(&self, body: &serde_json::Value) -> Result<NormalizedRequest, String> {
        let model_id = body
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let mut messages = Vec::new();

        if let Some(msgs) = body.get("messages").and_then(|v| v.as_array()) {
            for msg in msgs {
                let role = msg
                    .get("role")
                    .and_then(|v| v.as_str())
                    .unwrap_or("user");
                let content = msg
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                messages.push(NormalizedMessage {
                    role: MessageRole::from_str_loose(role),
                    content: content.to_string(),
                    name: None,
                });
            }
        }

        let temperature = body.get("temperature").and_then(|v| v.as_f64());
        let max_tokens = body
            .get("max_tokens")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);
        let stream = body.get("stream").and_then(|v| v.as_bool());

        Ok(NormalizedRequest {
            model_id,
            messages,
            temperature,
            max_tokens,
            stream,
            metadata: HashMap::new(),
        })
    }

    /// Convert a normalized response from Xergon native format.
    fn denormalize_xergon_response(
        &self,
        body: &serde_json::Value,
        latency_ms: u64,
    ) -> Result<NormalizedResponse, String> {
        let content = body
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let model = body
            .get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let usage = body.get("usage").map(|u| {
            TokenUsage::new(
                u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                u.get("completion_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32,
            )
        }).unwrap_or_default();

        let finish_reason = body
            .get("finish_reason")
            .and_then(|v| v.as_str())
            .map(|s| FinishReason::from_str_loose(s))
            .unwrap_or(FinishReason::Unknown);

        Ok(NormalizedResponse {
            content,
            tokens_used: usage,
            model,
            finish_reason,
            latency_ms,
            provider: ProviderProtocol::XergonNative,
            metadata: HashMap::new(),
        })
    }

    // -----------------------------------------------------------------------
    // Utility Methods
    // -----------------------------------------------------------------------

    /// Count registered providers.
    pub fn provider_count(&self) -> usize {
        self.provider_mappings.len()
    }
}

impl Default for ProtocolAdapter {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// REST API Types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct NormalizeRequest {
    pub protocol: String,
    pub body: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct NormalizeResponse {
    pub protocol: String,
    pub normalized: NormalizedRequest,
}

#[derive(Debug, Deserialize)]
pub struct DenormalizeRequest {
    pub protocol: String,
    pub body: serde_json::Value,
    pub latency_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct DenormalizeResponse {
    pub protocol: String,
    pub normalized: NormalizedResponse,
}

#[derive(Debug, Deserialize)]
pub struct RegisterProviderRequest {
    pub endpoint: String,
    pub protocol: String,
    pub name: String,
    pub supported_models: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ProvidersResponse {
    pub providers: Vec<ProviderSchema>,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct ProviderDeletedResponse {
    pub endpoint: String,
    pub deleted: bool,
}

// ---------------------------------------------------------------------------
// REST Handlers
// ---------------------------------------------------------------------------

/// POST /v1/adapter/normalize — Normalize a request from any provider format
async fn normalize_handler(
    State(state): State<AppState>,
    Json(req): Json<NormalizeRequest>,
) -> impl IntoResponse {
    let protocol = match ProviderProtocol::from_str_loose(&req.protocol) {
        Some(p) => p,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Unknown protocol: {}", req.protocol)
                })),
            )
                .into_response();
        }
    };

    match state.protocol_adapter.normalize_request(protocol, &req.body) {
        Ok(normalized) => (StatusCode::OK, Json(NormalizeResponse {
            protocol: protocol.to_string(),
            normalized,
        })).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

/// POST /v1/adapter/denormalize — Normalize a response from any provider format
async fn denormalize_handler(
    State(state): State<AppState>,
    Json(req): Json<DenormalizeRequest>,
) -> impl IntoResponse {
    let protocol = match ProviderProtocol::from_str_loose(&req.protocol) {
        Some(p) => p,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Unknown protocol: {}", req.protocol)
                })),
            )
                .into_response();
        }
    };

    let latency_ms = req.latency_ms.unwrap_or(0);

    match state.protocol_adapter.denormalize_response(protocol, &req.body, latency_ms) {
        Ok(normalized) => (StatusCode::OK, Json(DenormalizeResponse {
            protocol: protocol.to_string(),
            normalized,
        })).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": e})),
        )
            .into_response(),
    }
}

/// GET /v1/adapter/providers — List all registered providers
async fn list_providers_handler(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let providers = state.protocol_adapter.list_providers();
    let count = providers.len();

    (StatusCode::OK, Json(ProvidersResponse { providers, count })).into_response()
}

/// POST /v1/adapter/providers — Register a new provider
async fn register_provider_handler(
    State(state): State<AppState>,
    Json(req): Json<RegisterProviderRequest>,
) -> impl IntoResponse {
    let protocol = match ProviderProtocol::from_str_loose(&req.protocol) {
        Some(p) => p,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Unknown protocol: {}", req.protocol)
                })),
            )
                .into_response();
        }
    };

    state.protocol_adapter.register_provider(
        &req.endpoint,
        protocol,
        &req.name,
        req.supported_models,
    );

    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "status": "registered",
            "endpoint": req.endpoint,
            "protocol": protocol.to_string(),
        })),
    )
        .into_response()
}

/// DELETE /v1/adapter/providers/{endpoint} — Unregister a provider
async fn delete_provider_handler(
    State(state): State<AppState>,
    Path(endpoint): Path<String>,
) -> impl IntoResponse {
    let deleted = state.protocol_adapter.unregister_provider(&endpoint);

    (
        StatusCode::OK,
        Json(ProviderDeletedResponse { endpoint, deleted }),
    )
        .into_response()
}

/// GET /v1/adapter/schema/{protocol} — Get schema for a protocol
async fn get_schema_handler(
    State(state): State<AppState>,
    Path(protocol): Path<String>,
) -> impl IntoResponse {
    let proto = match ProviderProtocol::from_str_loose(&protocol) {
        Some(p) => p,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Unknown protocol: {}", protocol)
                })),
            )
                .into_response();
        }
    };

    match state.protocol_adapter.get_protocol_schema(proto) {
        Some(schema) => (StatusCode::OK, Json(schema)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("No schema found for protocol: {}", protocol)
            })),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Build the protocol adapter router.
pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/v1/adapter/normalize", post(normalize_handler))
        .route("/v1/adapter/denormalize", post(denormalize_handler))
        .route("/v1/adapter/providers", get(list_providers_handler))
        .route("/v1/adapter/providers", post(register_provider_handler))
        .route(
            "/v1/adapter/providers/{endpoint}",
            delete(delete_provider_handler),
        )
        .route("/v1/adapter/schema/{protocol}", get(get_schema_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_adapter() -> ProtocolAdapter {
        ProtocolAdapter::new()
    }

    #[test]
    fn test_provider_protocol_from_str() {
        assert_eq!(ProviderProtocol::from_str_loose("openai"), Some(ProviderProtocol::OpenAI));
        assert_eq!(ProviderProtocol::from_str_loose("anthropic"), Some(ProviderProtocol::Anthropic));
        assert_eq!(ProviderProtocol::from_str_loose("claude"), Some(ProviderProtocol::Anthropic));
        assert_eq!(ProviderProtocol::from_str_loose("gemini"), Some(ProviderProtocol::Gemini));
        assert_eq!(ProviderProtocol::from_str_loose("google"), Some(ProviderProtocol::Gemini));
        assert_eq!(ProviderProtocol::from_str_loose("ollama"), Some(ProviderProtocol::Ollama));
        assert_eq!(ProviderProtocol::from_str_loose("xergon"), Some(ProviderProtocol::XergonNative));
        assert_eq!(ProviderProtocol::from_str_loose("unknown"), None);
    }

    #[test]
    fn test_provider_protocol_display() {
        assert_eq!(format!("{}", ProviderProtocol::OpenAI), "openai");
        assert_eq!(format!("{}", ProviderProtocol::Anthropic), "anthropic");
        assert_eq!(format!("{}", ProviderProtocol::Gemini), "gemini");
        assert_eq!(format!("{}", ProviderProtocol::Ollama), "ollama");
        assert_eq!(format!("{}", ProviderProtocol::XergonNative), "xergon_native");
    }

    #[test]
    fn test_message_role_from_str() {
        assert_eq!(MessageRole::from_str_loose("system"), MessageRole::System);
        assert_eq!(MessageRole::from_str_loose("user"), MessageRole::User);
        assert_eq!(MessageRole::from_str_loose("assistant"), MessageRole::Assistant);
        assert_eq!(MessageRole::from_str_loose("model"), MessageRole::Assistant);
        assert_eq!(MessageRole::from_str_loose("tool"), MessageRole::Tool);
        assert_eq!(MessageRole::from_str_loose("function"), MessageRole::Tool);
    }

    #[test]
    fn test_convert_openai_chat() {
        let adapter = make_adapter();
        let body = json!({
            "model": "gpt-4o",
            "messages": [
                {"role": "system", "content": "You are helpful."},
                {"role": "user", "content": "Hello!"}
            ],
            "temperature": 0.7,
            "max_tokens": 100,
            "stream": false
        });

        let req = adapter.convert_openai(&body).unwrap();
        assert_eq!(req.model_id, "gpt-4o");
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, MessageRole::System);
        assert_eq!(req.messages[0].content, "You are helpful.");
        assert_eq!(req.messages[1].role, MessageRole::User);
        assert_eq!(req.temperature, Some(0.7));
        assert_eq!(req.max_tokens, Some(100));
        assert_eq!(req.stream, Some(false));
    }

    #[test]
    fn test_convert_openai_completions() {
        let adapter = make_adapter();
        let body = json!({
            "model": "gpt-3.5-turbo",
            "prompt": "Say hello"
        });

        let req = adapter.convert_openai(&body).unwrap();
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.messages[0].role, MessageRole::User);
        assert_eq!(req.messages[0].content, "Say hello");
    }

    #[test]
    fn test_convert_anthropic() {
        let adapter = make_adapter();
        let body = json!({
            "model": "claude-sonnet-4-20250514",
            "system": "You are helpful.",
            "messages": [
                {"role": "user", "content": "Hello!"}
            ],
            "max_tokens": 1024,
            "anthropic_version": "2023-06-01"
        });

        let req = adapter.convert_anthropic(&body).unwrap();
        assert_eq!(req.model_id, "claude-sonnet-4-20250514");
        assert_eq!(req.messages.len(), 2); // system + user
        assert_eq!(req.messages[0].role, MessageRole::System);
        assert_eq!(req.max_tokens, Some(1024));
    }

    #[test]
    fn test_convert_gemini() {
        let adapter = make_adapter();
        let body = json!({
            "contents": [
                {"role": "user", "parts": [{"text": "Hello!"}]}
            ],
            "generationConfig": {
                "temperature": 0.5,
                "maxOutputTokens": 256
            }
        });

        let req = adapter.convert_gemini(&body).unwrap();
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.messages[0].content, "Hello!");
        assert_eq!(req.temperature, Some(0.5));
        assert_eq!(req.max_tokens, Some(256));
    }

    #[test]
    fn test_convert_ollama() {
        let adapter = make_adapter();
        let body = json!({
            "model": "llama3.1:70b",
            "messages": [
                {"role": "user", "content": "Hello!"}
            ],
            "options": {
                "temperature": 0.8,
                "num_predict": 200
            }
        });

        let req = adapter.convert_ollama(&body).unwrap();
        assert_eq!(req.model_id, "llama3.1:70b");
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.temperature, Some(0.8));
        assert_eq!(req.max_tokens, Some(200));
    }

    #[test]
    fn test_denormalize_openai_response() {
        let adapter = make_adapter();
        let body = json!({
            "model": "gpt-4o",
            "choices": [
                {
                    "message": {"role": "assistant", "content": "Hello! How can I help?"},
                    "finish_reason": "stop"
                }
            ],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 8,
                "total_tokens": 18
            }
        });

        let resp = adapter.denormalize_openai_response(&body, 150).unwrap();
        assert_eq!(resp.content, "Hello! How can I help?");
        assert_eq!(resp.tokens_used.prompt_tokens, 10);
        assert_eq!(resp.tokens_used.completion_tokens, 8);
        assert_eq!(resp.model, "gpt-4o");
        assert_eq!(resp.finish_reason, FinishReason::Stop);
        assert_eq!(resp.latency_ms, 150);
        assert_eq!(resp.provider, ProviderProtocol::OpenAI);
    }

    #[test]
    fn test_register_and_list_providers() {
        let adapter = make_adapter();
        assert_eq!(adapter.provider_count(), 0);

        adapter.register_provider(
            "http://api.example.com",
            ProviderProtocol::OpenAI,
            "Test Provider",
            vec!["gpt-4o".to_string()],
        );

        assert_eq!(adapter.provider_count(), 1);

        let providers = adapter.list_providers();
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].protocol, ProviderProtocol::OpenAI);
        assert_eq!(providers[0].name, "Test Provider");
    }

    #[test]
    fn test_unregister_provider() {
        let adapter = make_adapter();
        adapter.register_provider(
            "http://api.example.com",
            ProviderProtocol::OpenAI,
            "Test",
            vec![],
        );

        assert!(adapter.unregister_provider("http://api.example.com"));
        assert!(!adapter.unregister_provider("http://nonexistent.com"));
        assert_eq!(adapter.provider_count(), 0);
    }

    #[test]
    fn test_detect_protocol() {
        let adapter = make_adapter();

        let openai_body = json!({"model": "gpt-4o", "messages": []});
        assert_eq!(adapter.detect_protocol(&openai_body), Some(ProviderProtocol::OpenAI));

        let anthropic_body = json!({"anthropic_version": "2023-06-01", "model": "claude", "messages": []});
        assert_eq!(adapter.detect_protocol(&anthropic_body), Some(ProviderProtocol::Anthropic));

        let gemini_body = json!({"contents": [], "generationConfig": {}});
        assert_eq!(adapter.detect_protocol(&gemini_body), Some(ProviderProtocol::Gemini));

        let unknown_body = json!({"foo": "bar"});
        assert_eq!(adapter.detect_protocol(&unknown_body), None);
    }

    #[test]
    fn test_normalized_request_utilities() {
        let req = NormalizedRequest {
            model_id: "test".to_string(),
            messages: vec![
                NormalizedMessage {
                    role: MessageRole::System,
                    content: "System prompt".to_string(),
                    name: None,
                },
                NormalizedMessage {
                    role: MessageRole::User,
                    content: "User message".to_string(),
                    name: None,
                },
                NormalizedMessage {
                    role: MessageRole::Assistant,
                    content: "Assistant response".to_string(),
                    name: None,
                },
                NormalizedMessage {
                    role: MessageRole::User,
                    content: "Follow up".to_string(),
                    name: None,
                },
            ],
            temperature: Some(0.7),
            max_tokens: Some(100),
            stream: None,
            metadata: HashMap::new(),
        };

        assert_eq!(req.last_user_message(), Some("Follow up"));
        assert_eq!(req.system_message(), Some("System prompt"));
        assert!(req.prompt_size_bytes() > 0);
    }

    #[test]
    fn test_finish_reason_parsing() {
        assert_eq!(FinishReason::from_str_loose("stop"), FinishReason::Stop);
        assert_eq!(FinishReason::from_str_loose("end_turn"), FinishReason::Stop);
        assert_eq!(FinishReason::from_str_loose("length"), FinishReason::Length);
        assert_eq!(FinishReason::from_str_loose("max_tokens"), FinishReason::Length);
        assert_eq!(FinishReason::from_str_loose("content_filter"), FinishReason::ContentFilter);
        assert_eq!(FinishReason::from_str_loose("tool_calls"), FinishReason::ToolCalls);
        assert_eq!(FinishReason::from_str_loose("unknown"), FinishReason::Unknown);
    }
}
