//! Protobuf message definitions for the gRPC-like transport layer.
//!
//! These types mirror the protobuf wire format for inference and embedding
//! requests/responses. We use `prost` for zero-copy encoding/decoding without
//! requiring a full tonic runtime.

use prost::Message;

// ---------------------------------------------------------------------------
// InferenceService messages
// ---------------------------------------------------------------------------

/// Protobuf-encoded inference request.
///
/// Wire-compatible with a .proto definition:
/// ```proto
/// message InferenceRequest {
///   string model = 1;
///   string prompt = 2;
///   uint32 max_tokens = 3;
///   float temperature = 4;
///   bool stream = 5;
///   repeated Message messages = 6;
/// }
/// message Message { string role = 1; string content = 2; }
/// ```
#[derive(Clone, PartialEq, Message)]
pub struct InferenceRequest {
    #[prost(string, tag = "1")]
    pub model: String,

    #[prost(string, tag = "2")]
    pub prompt: String,

    #[prost(uint32, tag = "3")]
    pub max_tokens: u32,

    #[prost(float, tag = "4")]
    pub temperature: f32,

    #[prost(bool, tag = "5")]
    pub stream: bool,

    #[prost(message, repeated, tag = "6")]
    pub messages: Vec<ChatMessage>,
}

/// Chat message within an inference request.
#[derive(Clone, PartialEq, Message)]
pub struct ChatMessage {
    #[prost(string, tag = "1")]
    pub role: String,

    #[prost(string, tag = "2")]
    pub content: String,
}

/// Protobuf-encoded inference response.
///
/// Wire-compatible with:
/// ```proto
/// message InferenceResponse {
///   string id = 1;
///   string model = 2;
///   repeated Choice choices = 3;
///   Usage usage = 4;
/// }
/// message Choice { string text = 1; uint32 index = 2; string finish_reason = 3; }
/// message Usage { uint32 prompt_tokens = 1; uint32 completion_tokens = 2; uint32 total_tokens = 3; }
/// ```
#[derive(Clone, PartialEq, Message)]
pub struct InferenceResponse {
    #[prost(string, tag = "1")]
    pub id: String,

    #[prost(string, tag = "2")]
    pub model: String,

    #[prost(message, repeated, tag = "3")]
    pub choices: Vec<Choice>,

    #[prost(message, optional, tag = "4")]
    pub usage: Option<Usage>,
}

/// A single completion choice.
#[derive(Clone, PartialEq, Message)]
pub struct Choice {
    #[prost(string, tag = "1")]
    pub text: String,

    #[prost(uint32, tag = "2")]
    pub index: u32,

    #[prost(string, tag = "3")]
    pub finish_reason: String,
}

/// Token usage statistics.
#[derive(Clone, PartialEq, Message)]
pub struct Usage {
    #[prost(uint32, tag = "1")]
    pub prompt_tokens: u32,

    #[prost(uint32, tag = "2")]
    pub completion_tokens: u32,

    #[prost(uint32, tag = "3")]
    pub total_tokens: u32,
}

// ---------------------------------------------------------------------------
// EmbeddingService messages
// ---------------------------------------------------------------------------

/// Protobuf-encoded embedding request.
///
/// Wire-compatible with:
/// ```proto
/// message EmbeddingRequest {
///   string model = 1;
///   repeated string input = 2;
///   uint32 dimensions = 3;
/// }
/// ```
#[derive(Clone, PartialEq, Message)]
pub struct EmbeddingRequest {
    #[prost(string, tag = "1")]
    pub model: String,

    #[prost(string, repeated, tag = "2")]
    pub input: Vec<String>,

    #[prost(uint32, tag = "3")]
    pub dimensions: u32,
}

/// Protobuf-encoded embedding response.
///
/// Wire-compatible with:
/// ```proto
/// message EmbeddingResponse {
///   repeated EmbeddingData data = 1;
///   string model = 2;
///   Usage usage = 3;
/// }
/// message EmbeddingData { uint32 index = 1; repeated float embedding = 2; }
/// ```
#[derive(Clone, PartialEq, Message)]
pub struct EmbeddingResponse {
    #[prost(message, repeated, tag = "1")]
    pub data: Vec<EmbeddingData>,

    #[prost(string, tag = "2")]
    pub model: String,

    #[prost(message, optional, tag = "3")]
    pub usage: Option<Usage>,
}

/// A single embedding result.
#[derive(Clone, PartialEq, Message)]
pub struct EmbeddingData {
    #[prost(uint32, tag = "1")]
    pub index: u32,

    #[prost(float, repeated, tag = "2")]
    pub embedding: Vec<f32>,
}

// ---------------------------------------------------------------------------
// gRPC error response (application-level)
// ---------------------------------------------------------------------------

/// Standard gRPC-style error message.
#[derive(Clone, PartialEq, Message)]
pub struct GrpcError {
    #[prost(uint32, tag = "1")]
    pub code: u32,

    #[prost(string, tag = "2")]
    pub message: String,
}

// ---------------------------------------------------------------------------
// gRPC wire format helpers
// ---------------------------------------------------------------------------

/// gRPC wire frame: 1 byte compressed flag + 4 byte big-endian length + payload.
///
/// This implements the gRPC Length-Prefixed-Message framing:
/// <Compressed-Flag(1 byte)><Message-Length(4 bytes big-endian)><Message>
pub fn encode_grpc_frame(payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(5 + payload.len());
    frame.push(0); // not compressed
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

/// Decode a gRPC wire frame, returning the payload bytes.
///
/// Returns `None` if the frame is malformed (too short, wrong length).
pub fn decode_grpc_frame(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 5 {
        return None;
    }
    let _compressed = data[0];
    let len = u32::from_be_bytes([data[1], data[2], data[3], data[4]]) as usize;
    if data.len() < 5 + len {
        return None;
    }
    Some(data[5..5 + len].to_vec())
}

/// gRPC status codes we use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrpcStatusCode {
    Ok = 0,
    Cancelled = 1,
    Unknown = 2,
    InvalidArgument = 3,
    DeadlineExceeded = 4,
    NotFound = 5,
    AlreadyExists = 6,
    PermissionDenied = 7,
    ResourceExhausted = 8,
    FailedPrecondition = 9,
    Aborted = 10,
    OutOfRange = 11,
    Unimplemented = 12,
    Internal = 13,
    Unavailable = 14,
    DataLoss = 15,
    Unauthenticated = 16,
}

impl GrpcStatusCode {
    pub fn to_http_status(&self) -> u16 {
        match self {
            Self::Ok => 200,
            Self::InvalidArgument | Self::OutOfRange | Self::FailedPrecondition => 400,
            Self::Unauthenticated => 401,
            Self::PermissionDenied => 403,
            Self::NotFound => 404,
            Self::AlreadyExists | Self::Aborted => 409,
            Self::ResourceExhausted => 429,
            Self::Cancelled | Self::DeadlineExceeded => 499,
            Self::Internal | Self::DataLoss | Self::Unknown => 500,
            Self::Unavailable => 503,
            Self::Unimplemented => 504,
        }
    }

    pub fn from_u32(code: u32) -> Self {
        match code {
            0 => Self::Ok,
            1 => Self::Cancelled,
            2 => Self::Unknown,
            3 => Self::InvalidArgument,
            4 => Self::DeadlineExceeded,
            5 => Self::NotFound,
            6 => Self::AlreadyExists,
            7 => Self::PermissionDenied,
            8 => Self::ResourceExhausted,
            9 => Self::FailedPrecondition,
            10 => Self::Aborted,
            11 => Self::OutOfRange,
            12 => Self::Unimplemented,
            13 => Self::Internal,
            14 => Self::Unavailable,
            15 => Self::DataLoss,
            16 => Self::Unauthenticated,
            _ => Self::Unknown,
        }
    }

    /// Return the numeric code as a string (for grpc-status header).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ok => "0",
            Self::Cancelled => "1",
            Self::Unknown => "2",
            Self::InvalidArgument => "3",
            Self::DeadlineExceeded => "4",
            Self::NotFound => "5",
            Self::AlreadyExists => "6",
            Self::PermissionDenied => "7",
            Self::ResourceExhausted => "8",
            Self::FailedPrecondition => "9",
            Self::Aborted => "10",
            Self::OutOfRange => "11",
            Self::Unimplemented => "12",
            Self::Internal => "13",
            Self::Unavailable => "14",
            Self::DataLoss => "15",
            Self::Unauthenticated => "16",
        }
    }

    /// Map an HTTP status code to the closest gRPC status code.
    pub fn from_http(status: axum::http::StatusCode) -> Self {
        let code = status.as_u16();
        match code {
            200..=299 => Self::Ok,
            400 => Self::InvalidArgument,
            401 => Self::Unauthenticated,
            403 => Self::PermissionDenied,
            404 => Self::NotFound,
            409 => Self::Aborted,
            429 => Self::ResourceExhausted,
            499 => Self::Cancelled,
            500 => Self::Internal,
            503 => Self::Unavailable,
            504 => Self::Unimplemented,
            _ => Self::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_inference_request() {
        let req = InferenceRequest {
            model: "llama-3.1-8b".into(),
            prompt: "Hello world".into(),
            max_tokens: 100,
            temperature: 0.7,
            stream: false,
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "Hello world".into(),
            }],
        };
        let encoded = req.encode_to_vec();
        let decoded = InferenceRequest::decode(encoded.as_slice()).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn test_roundtrip_inference_response() {
        let resp = InferenceResponse {
            id: "req-123".into(),
            model: "llama-3.1-8b".into(),
            choices: vec![Choice {
                text: "Hello!".into(),
                index: 0,
                finish_reason: "stop".into(),
            }],
            usage: Some(Usage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            }),
        };
        let encoded = resp.encode_to_vec();
        let decoded = InferenceResponse::decode(encoded.as_slice()).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn test_roundtrip_embedding_request() {
        let req = EmbeddingRequest {
            model: "text-embedding-3-small".into(),
            input: vec!["Hello world".into(), "Goodbye world".into()],
            dimensions: 1536,
        };
        let encoded = req.encode_to_vec();
        let decoded = EmbeddingRequest::decode(encoded.as_slice()).unwrap();
        assert_eq!(req, decoded);
    }

    #[test]
    fn test_grpc_frame_roundtrip() {
        let payload = b"hello protobuf world";
        let frame = encode_grpc_frame(payload);
        assert_eq!(frame[0], 0); // not compressed
        let decoded = decode_grpc_frame(&frame).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn test_grpc_frame_too_short() {
        assert!(decode_grpc_frame(&[0, 1, 2]).is_none());
    }

    #[test]
    fn test_grpc_status_codes() {
        assert_eq!(GrpcStatusCode::Ok.to_http_status(), 200);
        assert_eq!(GrpcStatusCode::InvalidArgument.to_http_status(), 400);
        assert_eq!(GrpcStatusCode::Unauthenticated.to_http_status(), 401);
        assert_eq!(GrpcStatusCode::PermissionDenied.to_http_status(), 403);
        assert_eq!(GrpcStatusCode::NotFound.to_http_status(), 404);
        assert_eq!(GrpcStatusCode::ResourceExhausted.to_http_status(), 429);
        assert_eq!(GrpcStatusCode::Internal.to_http_status(), 500);
        assert_eq!(GrpcStatusCode::Unavailable.to_http_status(), 503);
    }
}
