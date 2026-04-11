//! Content negotiation for inference responses.
//!
//! Parses `Accept`, `Accept-Encoding`, and `Accept-Charset` headers with quality
//! values and selects the best matching representation from a supported types registry.

use std::collections::HashMap;
use std::sync::RwLock;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Supported content types for inference responses.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContentType {
    Json,
    Protobuf,
    MsgPack,
    Csv,
    PlainText,
    SseStream,
    Binary,
}

impl ContentType {
    /// Parse a MIME type string into a ContentType.
    pub fn from_mime(mime: &str) -> Option<Self> {
        match mime.trim().to_lowercase().as_str() {
            "application/json" | "text/json" => Some(ContentType::Json),
            "application/protobuf" | "application/x-protobuf" => Some(ContentType::Protobuf),
            "application/x-msgpack" | "application/msgpack" => Some(ContentType::MsgPack),
            "text/csv" | "application/csv" => Some(ContentType::Csv),
            "text/plain" => Some(ContentType::PlainText),
            "text/event-stream" => Some(ContentType::SseStream),
            "application/octet-stream" | "application/binary" => Some(ContentType::Binary),
            // Handle wildcards
            "*/*" | "application/*" => Some(ContentType::Json), // default to JSON
            _ => None,
        }
    }

    /// Convert to MIME string.
    pub fn to_mime(&self) -> &'static str {
        match self {
            ContentType::Json => "application/json",
            ContentType::Protobuf => "application/protobuf",
            ContentType::MsgPack => "application/x-msgpack",
            ContentType::Csv => "text/csv",
            ContentType::PlainText => "text/plain",
            ContentType::SseStream => "text/event-stream",
            ContentType::Binary => "application/octet-stream",
        }
    }

    /// File extension for this content type.
    pub fn extension(&self) -> &'static str {
        match self {
            ContentType::Json => "json",
            ContentType::Protobuf => "pb",
            ContentType::MsgPack => "msgpack",
            ContentType::Csv => "csv",
            ContentType::PlainText => "txt",
            ContentType::SseStream => "sse",
            ContentType::Binary => "bin",
        }
    }
}

impl std::fmt::Display for ContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_mime())
    }
}

/// A parsed Accept header with quality values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptHeader {
    /// Ordered list of (content_type, quality) pairs.
    pub types: Vec<(ContentType, f32)>,
    /// Additional parameters from the Accept header.
    pub params: HashMap<String, String>,
}

/// Result of content negotiation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegotiationResult {
    /// The selected content type.
    pub content_type: ContentType,
    /// Selected charset.
    pub charset: String,
    /// Selected encoding.
    pub encoding: String,
    /// Selected language (if applicable).
    pub language: String,
    /// Quality value of the selected type.
    pub quality: f32,
}

// ---------------------------------------------------------------------------
// Negotiator
// ---------------------------------------------------------------------------

/// Content negotiator engine.
#[derive(Debug)]
pub struct ContentNegotiator {
    /// Supported content types with their default quality values.
    supported: RwLock<HashMap<ContentType, f32>>,
    /// Default content type when negotiation fails.
    default_type: ContentType,
    /// Minimum quality threshold for accepting a type.
    min_quality: f32,
}

impl ContentNegotiator {
    /// Create a new negotiator with JSON as default.
    pub fn new() -> Self {
        let mut supported = HashMap::new();
        supported.insert(ContentType::Json, 1.0);
        supported.insert(ContentType::SseStream, 1.0);
        supported.insert(ContentType::Protobuf, 0.9);
        supported.insert(ContentType::MsgPack, 0.8);
        supported.insert(ContentType::Csv, 0.7);
        supported.insert(ContentType::PlainText, 0.5);
        supported.insert(ContentType::Binary, 0.3);

        Self {
            supported: RwLock::new(supported),
            default_type: ContentType::Json,
            min_quality: 0.0,
        }
    }

    /// Create with a custom default type.
    pub fn with_default(default: ContentType) -> Self {
        let mut neg = Self::new();
        neg.default_type = default;
        neg
    }

    /// Register a content type with its default quality value.
    pub fn register_type(&self, content_type: ContentType, quality: f32) {
        let mut supported = self.supported.write().unwrap();
        supported.insert(content_type, quality);
    }

    /// Get the list of supported types.
    pub fn get_supported(&self) -> Vec<(ContentType, f32)> {
        let supported = self.supported.read().unwrap();
        let mut types: Vec<_> = supported.iter().map(|(k, &v)| (k.clone(), v)).collect();
        types.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        types
    }

    /// Parse an Accept header string.
    pub fn parse_accept_header(header: &str) -> AcceptHeader {
        let mut types = Vec::new();
        let mut params = HashMap::new();

        if header.trim().is_empty() {
            return AcceptHeader { types, params };
        }

        for part in header.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }

            // Split off parameters
            let mut segments = part.split(';');
            let mime_type = segments.next().unwrap_or("").trim();

            let mut quality = 1.0_f32;
            for param in segments {
                let param = param.trim();
                if let Some(q_val) = param.strip_prefix("q=") {
                    if let Ok(q) = q_val.trim().parse::<f32>() {
                        quality = q.clamp(0.0, 1.0);
                    }
                } else if let Some((k, v)) = param.split_once('=') {
                    params.insert(k.trim().to_string(), v.trim().to_string());
                }
            }

            if let Some(ct) = ContentType::from_mime(mime_type) {
                types.push((ct, quality));
            }
        }

        // Sort by quality descending
        types.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        AcceptHeader { types, params }
    }

    /// Negotiate the best content type from an Accept header.
    pub fn negotiate(&self, accept_header: &str) -> NegotiationResult {
        let parsed = Self::parse_accept_header(accept_header);
        let supported = self.supported.read().unwrap();

        // Find the best match: highest quality that is also supported
        let mut best_type = None;
        let mut best_quality = -1.0_f32;

        for (ct, q) in &parsed.types {
            if *q < self.min_quality {
                continue;
            }
            if let Some(&server_q) = supported.get(ct) {
                // Combined quality: client q * server q
                let combined = *q * server_q;
                if combined > best_quality {
                    best_quality = combined;
                    best_type = Some(ct.clone());
                }
            }
        }

        // Handle wildcard types
        if best_type.is_none() {
            for (_ct, q) in &parsed.types {
                // Check for */* or application/*
                if *q < self.min_quality {
                    continue;
                }
                // Default to JSON for wildcards
                if supported.contains_key(&ContentType::Json) {
                    let server_q = supported.get(&ContentType::Json).unwrap();
                    let combined = *q * server_q;
                    if combined > best_quality {
                        best_quality = combined;
                        best_type = Some(ContentType::Json);
                    }
                }
            }
        }

        let content_type = best_type.unwrap_or_else(|| self.default_type.clone());

        NegotiationResult {
            content_type,
            charset: "utf-8".to_string(),
            encoding: "identity".to_string(),
            language: "en".to_string(),
            quality: best_quality.max(0.0),
        }
    }

    /// Negotiate content encoding from Accept-Encoding header.
    pub fn negotiate_encoding(&self, accept_encoding: &str) -> String {
        if accept_encoding.trim().is_empty() || accept_encoding == "*" {
            return "identity".to_string();
        }

        let encodings = parse_quality_header(accept_encoding);

        // Priority order: br, gzip, deflate, identity
        let preferred = [
            ("br", 1.1_f32),
            ("gzip", 1.0_f32),
            ("deflate", 0.9_f32),
            ("identity", 0.5_f32),
        ];

        let mut best = "identity".to_string();
        let mut best_q = 0.0_f32;

        for (enc, _default_q) in &preferred {
            let q = encodings.get(*enc).copied().unwrap_or(0.0_f32);
            if q > 0.0 && q > best_q {
                best_q = q;
                best = enc.to_string();
            }
        }

        // Check for wildcard
        if let Some(&star_q) = encodings.get("*") {
            if star_q > best_q {
                best = "gzip".to_string(); // default for wildcard
            }
        }

        best
    }

    /// Negotiate charset from Accept-Charset header.
    pub fn negotiate_charset(&self, accept_charset: &str) -> String {
        if accept_charset.trim().is_empty() {
            return "utf-8".to_string();
        }

        let charsets = parse_quality_header(accept_charset);

        let preferred = [
            ("utf-8", 1.0_f32),
            ("iso-8859-1", 0.8_f32),
        ];

        let mut best = "utf-8".to_string();
        let mut best_q = 0.0_f32;

        for (cs, _default_q) in &preferred {
            let q = charsets.get(*cs).copied().unwrap_or(0.0_f32);
            if q > 0.0 && q > best_q {
                best_q = q;
                best = cs.to_string();
            }
        }

        best
    }

    /// Full negotiation: content type, encoding, and charset.
    pub fn negotiate_all(
        &self,
        accept: &str,
        accept_encoding: &str,
        accept_charset: &str,
    ) -> NegotiationResult {
        let mut result = self.negotiate(accept);
        result.encoding = self.negotiate_encoding(accept_encoding);
        result.charset = self.negotiate_charset(accept_charset);
        result
    }
}

impl Default for ContentNegotiator {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a header with quality values into a map.
fn parse_quality_header(header: &str) -> HashMap<&str, f32> {
    let mut map = HashMap::new();
    for part in header.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let mut segments = part.split(';');
        let value = segments.next().unwrap_or("").trim();
        let mut quality = 1.0_f32;
        for param in segments {
            let param = param.trim();
            if let Some(q_val) = param.strip_prefix("q=") {
                if let Ok(q) = q_val.trim().parse::<f32>() {
                    quality = q.clamp(0.0, 1.0);
                }
            }
        }
        map.insert(value, quality);
    }
    map
}

// ---------------------------------------------------------------------------
// REST API types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct NegotiateRequest {
    pub accept: Option<String>,
    pub accept_encoding: Option<String>,
    pub accept_charset: Option<String>,
}

#[derive(Serialize)]
pub struct SupportedTypeResponse {
    pub content_type: String,
    pub mime: String,
    pub extension: String,
    pub quality: f32,
}

#[derive(Serialize)]
pub struct EncodingResponse {
    pub encoding: String,
}

#[derive(Serialize)]
pub struct CharsetResponse {
    pub charset: String,
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

async fn negotiate_content(
    State(state): State<AppState>,
    Json(body): Json<NegotiateRequest>,
) -> impl IntoResponse {
    let negotiator = &state.content_negotiator;
    let accept = body.accept.unwrap_or_else(|| "*/*".to_string());
    let accept_encoding = body.accept_encoding.unwrap_or_default();
    let accept_charset = body.accept_charset.unwrap_or_default();

    let result = negotiator.negotiate_all(&accept, &accept_encoding, &accept_charset);

    (StatusCode::OK, Json(result))
}

async fn get_supported_types(State(state): State<AppState>) -> impl IntoResponse {
    let negotiator = &state.content_negotiator;
    let types = negotiator.get_supported();
    let response: Vec<SupportedTypeResponse> = types
        .into_iter()
        .map(|(ct, q)| SupportedTypeResponse {
            content_type: format!("{:?}", ct),
            mime: ct.to_mime().to_string(),
            extension: ct.extension().to_string(),
            quality: q,
        })
        .collect();
    (StatusCode::OK, Json(response))
}

async fn negotiate_encoding_handler(
    State(state): State<AppState>,
    Json(body): Json<NegotiateRequest>,
) -> impl IntoResponse {
    let negotiator = &state.content_negotiator;
    let accept_encoding = body.accept_encoding.unwrap_or_default();
    let encoding = negotiator.negotiate_encoding(&accept_encoding);
    (StatusCode::OK, Json(EncodingResponse { encoding }))
}

async fn negotiate_charset_handler(
    State(state): State<AppState>,
    Json(body): Json<NegotiateRequest>,
) -> impl IntoResponse {
    let negotiator = &state.content_negotiator;
    let accept_charset = body.accept_charset.unwrap_or_default();
    let charset = negotiator.negotiate_charset(&accept_charset);
    (StatusCode::OK, Json(CharsetResponse { charset }))
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the content negotiation API router.
pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/v1/negotiate", post(negotiate_content))
        .route("/v1/negotiate/supported", get(get_supported_types))
        .route("/v1/negotiate/encoding", post(negotiate_encoding_handler))
        .route("/v1/negotiate/charset", post(negotiate_charset_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_accept_simple() {
        let header = "application/json";
        let parsed = ContentNegotiator::parse_accept_header(header);
        assert_eq!(parsed.types.len(), 1);
        assert_eq!(parsed.types[0].0, ContentType::Json);
        assert!((parsed.types[0].1 - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_parse_accept_multiple_with_quality() {
        let header = "text/csv;q=0.5, application/json;q=1.0, text/plain;q=0.3";
        let parsed = ContentNegotiator::parse_accept_header(header);
        assert_eq!(parsed.types.len(), 3);
        assert_eq!(parsed.types[0].0, ContentType::Json); // highest q first
        assert_eq!(parsed.types[0].1, 1.0);
        assert_eq!(parsed.types[1].0, ContentType::Csv);
        assert_eq!(parsed.types[1].1, 0.5);
    }

    #[test]
    fn test_negotiate_json_default() {
        let neg = ContentNegotiator::new();
        let result = neg.negotiate("application/json");
        assert_eq!(result.content_type, ContentType::Json);
    }

    #[test]
    fn test_negotiate_wildcard_falls_to_json() {
        let neg = ContentNegotiator::new();
        let result = neg.negotiate("*/*");
        assert_eq!(result.content_type, ContentType::Json);
    }

    #[test]
    fn test_negotiate_unsupported_returns_default() {
        let neg = ContentNegotiator::new();
        let result = neg.negotiate("text/html;q=1.0");
        assert_eq!(result.content_type, ContentType::Json);
    }

    #[test]
    fn test_negotiate_encoding_gzip() {
        let neg = ContentNegotiator::new();
        assert_eq!(neg.negotiate_encoding("gzip"), "gzip");
    }

    #[test]
    fn test_negotiate_encoding_brotli_priority() {
        let neg = ContentNegotiator::new();
        assert_eq!(neg.negotiate_encoding("br, gzip, deflate"), "br");
    }

    #[test]
    fn test_negotiate_encoding_identity_fallback() {
        let neg = ContentNegotiator::new();
        assert_eq!(neg.negotiate_encoding(""), "identity");
    }

    #[test]
    fn test_negotiate_charset_utf8() {
        let neg = ContentNegotiator::new();
        assert_eq!(neg.negotiate_charset("utf-8"), "utf-8");
    }

    #[test]
    fn test_register_and_get_supported() {
        let neg = ContentNegotiator::new();
        neg.register_type(ContentType::Csv, 0.95);
        let supported = neg.get_supported();
        assert!(supported.iter().any(|(ct, _)| *ct == ContentType::Csv));
    }
}
