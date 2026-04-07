use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use bytes::Bytes;
use prost::Message;
use serde_json::Value;
use tracing::{error, info};

use crate::grpc::proto::{self, *};
use crate::AppState;

// ---------------------------------------------------------------------------
// gRPC frame helpers
// ---------------------------------------------------------------------------

/// Encode a proto message into a gRPC wire frame: 1-byte compressed flag (0)
/// + 4-byte big-endian message length + message bytes.
fn encode_grpc_frame(msg: &impl Message) -> Bytes {
    let msg_bytes = msg.encode_to_vec();
    let len = msg_bytes.len() as u32;
    let mut buf = Vec::with_capacity(5 + msg_bytes.len());
    buf.push(0u8); // not compressed
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(&msg_bytes);
    Bytes::from(buf)
}

/// Build a gRPC error response with the given status code and message.
fn grpc_error(code: GrpcStatusCode, message: &str) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert("grpc-status", code.as_str().parse().unwrap());
    headers.insert("grpc-message", message.parse().unwrap());
    headers.insert(header::CONTENT_TYPE, "application/grpc".parse().unwrap());
    (StatusCode::OK, headers, Body::empty()).into_response()
}

/// Build a successful gRPC response carrying an encoded proto message.
fn grpc_response(msg: &impl Message) -> Response {
    let frame = encode_grpc_frame(msg);
    let mut headers = HeaderMap::new();
    headers.insert("grpc-status", "0".parse().unwrap());
    headers.insert(header::CONTENT_TYPE, "application/grpc".parse().unwrap());
    (StatusCode::OK, headers, Body::from(frame)).into_response()
}

// ---------------------------------------------------------------------------
// Inference handler
// ---------------------------------------------------------------------------

pub async fn grpc_inference_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // Decode the gRPC frame (skip 5-byte frame header).
    if body.len() < 5 {
        return grpc_error(
            GrpcStatusCode::InvalidArgument,
            "request body too short",
        );
    }
    let msg_bytes = &body[5..];

    let req = match InferenceRequest::decode(msg_bytes) {
        Ok(r) => r,
        Err(e) => {
            return grpc_error(
                GrpcStatusCode::InvalidArgument,
                &format!("failed to decode InferenceRequest: {e}"),
            );
        }
    };

    let model = req.model.clone();
    let request_id = uuid::Uuid::new_v4().to_string();
    info!(
        grpc_inference = true,
        request_id = %request_id,
        model = %model,
        stream = req.stream,
        "gRPC inference request"
    );

    // Build the JSON body that the internal proxy handler expects (OpenAI-compatible).
    // If the request has structured messages, use those; otherwise wrap prompt as user msg.
    let messages_json: Vec<Value> = if !req.messages.is_empty() {
        req.messages
            .iter()
            .map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content,
                })
            })
            .collect()
    } else {
        vec![serde_json::json!({
            "role": "user",
            "content": req.prompt,
        })]
    };

    let json_body = serde_json::json!({
        "model": req.model,
        "messages": messages_json,
        "max_tokens": req.max_tokens,
        "temperature": req.temperature,
        "stream": false, // gRPC unary doesn't support SSE streaming
    });

    // Forward to the internal provider proxy.
    let client = state.http_client.clone();
    let port = state
        .config
        .relay
        .listen_addr
        .split(':')
        .last()
        .unwrap_or("3000");
    let internal_url = format!("http://127.0.0.1:{port}/v1/chat/completions");

    let mut req_builder = client
        .post(&internal_url)
        .header("Content-Type", "application/json")
        .header("X-Request-ID", &request_id);

    // Propagate auth headers if present.
    if let Some(auth) = headers.get("authorization") {
        req_builder = req_builder.header("authorization", auth);
    }

    let result = req_builder.json(&json_body).send().await;

    let resp = match result {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "gRPC inference: proxy request failed");
            return grpc_error(
                GrpcStatusCode::Unavailable,
                &format!("upstream request failed: {e}"),
            );
        }
    };

    let status = resp.status();
    let body_text = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        return grpc_error(GrpcStatusCode::from_http(status), &body_text);
    }

    // Parse the JSON response and map to InferenceResponse proto.
    let json: Value = match serde_json::from_str(&body_text) {
        Ok(j) => j,
        Err(e) => {
            return grpc_error(
                GrpcStatusCode::Internal,
                &format!("failed to parse upstream response: {e}"),
            );
        }
    };

    let id = json
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or(&request_id)
        .to_string();

    let model_resp = json
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or(&model)
        .to_string();

    let choices: Vec<Choice> = json
        .get("choices")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let index = item.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    // Extract text from message.content or from the top-level
                    let text = item
                        .get("message")
                        .and_then(|m| m.get("content"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let finish_reason = item
                        .get("finish_reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("stop")
                        .to_string();
                    Some(Choice {
                        index,
                        text: text.to_string(),
                        finish_reason,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let usage = json.get("usage").map(|u| Usage {
        prompt_tokens: u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        completion_tokens: u
            .get("completion_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        total_tokens: u.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
    });

    let response = InferenceResponse {
        id,
        model: model_resp,
        choices,
        usage,
    };

    grpc_response(&response)
}

// ---------------------------------------------------------------------------
// Embeddings handler
// ---------------------------------------------------------------------------

pub async fn grpc_embedding_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if body.len() < 5 {
        return grpc_error(
            GrpcStatusCode::InvalidArgument,
            "request body too short",
        );
    }
    let msg_bytes = &body[5..];

    let req = match EmbeddingRequest::decode(msg_bytes) {
        Ok(r) => r,
        Err(e) => {
            return grpc_error(
                GrpcStatusCode::InvalidArgument,
                &format!("failed to decode EmbeddingRequest: {e}"),
            );
        }
    };

    let model = req.model.clone();
    let request_id = uuid::Uuid::new_v4().to_string();
    info!(
        grpc_embedding = true,
        request_id = %request_id,
        model = %model,
        input_count = req.input.len(),
        "gRPC embedding request"
    );

    let json_body = serde_json::json!({
        "model": req.model,
        "input": req.input,
        "dimensions": req.dimensions,
    });

    let client = state.http_client.clone();
    let port = state
        .config
        .relay
        .listen_addr
        .split(':')
        .last()
        .unwrap_or("3000");
    let internal_url = format!("http://127.0.0.1:{port}/v1/embeddings");

    let mut req_builder = client
        .post(&internal_url)
        .header("Content-Type", "application/json")
        .header("X-Request-ID", &request_id);

    if let Some(auth) = headers.get("authorization") {
        req_builder = req_builder.header("authorization", auth);
    }

    let result = req_builder.json(&json_body).send().await;

    let resp = match result {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "gRPC embedding: proxy request failed");
            return grpc_error(
                GrpcStatusCode::Unavailable,
                &format!("upstream request failed: {e}"),
            );
        }
    };

    let status = resp.status();
    let body_text = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        return grpc_error(GrpcStatusCode::from_http(status), &body_text);
    }

    let json: Value = match serde_json::from_str(&body_text) {
        Ok(j) => j,
        Err(e) => {
            return grpc_error(
                GrpcStatusCode::Internal,
                &format!("failed to parse upstream response: {e}"),
            );
        }
    };

    let model_resp = json
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or(&model)
        .to_string();

    let data: Vec<EmbeddingData> = json
        .get("data")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let index = item.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    let embedding: Vec<f32> = item
                        .get("embedding")
                        .and_then(|v| v.as_array())
                        .map(|emb| {
                            emb.iter()
                                .filter_map(|v| v.as_f64())
                                .map(|f| f as f32)
                                .collect()
                        })
                        .unwrap_or_default();
                    Some(EmbeddingData { index, embedding })
                })
                .collect()
        })
        .unwrap_or_default();

    let usage = json.get("usage").map(|u| Usage {
        prompt_tokens: u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        completion_tokens: u
            .get("completion_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        total_tokens: u.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
    });

    let response = EmbeddingResponse {
        data,
        model: model_resp,
        usage,
    };

    grpc_response(&response)
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn build_grpc_router() -> Router<AppState> {
    Router::new()
        .route("/grpc/inference", post(grpc_inference_handler))
        .route("/grpc/embeddings", post(grpc_embedding_handler))
}
