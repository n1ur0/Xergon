use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json, Router,
    routing::{delete, get, post, put},
};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{RwLock};
use uuid::Uuid;

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// StreamStatus
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StreamStatus {
    Pending,
    Active,
    Streaming,
    Completed,
    Failed,
    Timeout,
    Cancelled,
}

// ---------------------------------------------------------------------------
// StreamConfig
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StreamConfig {
    pub max_concurrent_streams: u32,
    pub chunk_size: u32,
    pub backpressure_threshold: u32,
    pub timeout_ms: u64,
    pub retry_attempts: u32,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            max_concurrent_streams: 1000,
            chunk_size: 16,
            backpressure_threshold: 100,
            timeout_ms: 30_000,
            retry_attempts: 3,
        }
    }
}

// ---------------------------------------------------------------------------
// TokenChunk
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TokenChunk {
    pub stream_id: String,
    pub tokens: Vec<String>,
    pub chunk_index: u32,
    pub is_final: bool,
    pub latency_ms: u64,
}

// ---------------------------------------------------------------------------
// StreamSession
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StreamSession {
    pub id: String,
    pub request_id: String,
    pub model: String,
    pub client_id: String,
    pub status: StreamStatus,
    pub chunks_received: u32,
    pub total_tokens: u32,
    pub started_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub priority: String,
}

// ---------------------------------------------------------------------------
// StreamMetricsSnapshot
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct StreamMetricsSnapshot {
    pub total_streams: u64,
    pub active_streams: u64,
    pub avg_tokens_per_stream: f64,
    pub total_chunks_delivered: u64,
    pub avg_chunk_latency_ms: f64,
    pub timeout_count: u64,
    pub backpressure_events: u64,
}

// ---------------------------------------------------------------------------
// TokenStreamingMultiplexer
// ---------------------------------------------------------------------------

pub struct TokenStreamingMultiplexer {
    config: RwLock<StreamConfig>,
    sessions: DashMap<String, StreamSession>,
    chunk_buffers: DashMap<String, Vec<TokenChunk>>,
    total_streams: AtomicU64,
    active_streams: AtomicU64,
    total_chunks: AtomicU64,
    total_chunk_latency_ms: AtomicU64,
    timeout_count: AtomicU64,
    backpressure_events: AtomicU64,
}

impl TokenStreamingMultiplexer {
    /// Create a new multiplexer with the given configuration.
    pub fn new(config: StreamConfig) -> Self {
        Self {
            config: RwLock::new(config),
            sessions: DashMap::new(),
            chunk_buffers: DashMap::new(),
            total_streams: AtomicU64::new(0),
            active_streams: AtomicU64::new(0),
            total_chunks: AtomicU64::new(0),
            total_chunk_latency_ms: AtomicU64::new(0),
            timeout_count: AtomicU64::new(0),
            backpressure_events: AtomicU64::new(0),
        }
    }

    /// Create a new streaming session for a request.
    pub fn create_session(
        &self,
        request_id: &str,
        model: &str,
        client_id: &str,
        priority: &str,
    ) -> Result<StreamSession, String> {
        let cfg = self
            .config
            .read()
            .map_err(|e| format!("Failed to read config: {e}"))?;

        let active = self.active_streams.load(Ordering::Relaxed);
        if active >= cfg.max_concurrent_streams as u64 {
            self.backpressure_events
                .fetch_add(1, Ordering::Relaxed);
            return Err(format!(
                "Max concurrent streams reached: {}",
                cfg.max_concurrent_streams
            ));
        }

        let session = StreamSession {
            id: Uuid::new_v4().to_string(),
            request_id: request_id.to_string(),
            model: model.to_string(),
            client_id: client_id.to_string(),
            status: StreamStatus::Pending,
            chunks_received: 0,
            total_tokens: 0,
            started_at: Utc::now(),
            last_activity: Utc::now(),
            priority: priority.to_string(),
        };

        let sid = session.id.clone();
        self.sessions.insert(sid.clone(), session.clone());
        self.chunk_buffers.insert(sid.clone(), Vec::new());
        self.total_streams.fetch_add(1, Ordering::Relaxed);
        self.active_streams.fetch_add(1, Ordering::Relaxed);

        Ok(session)
    }

    /// Retrieve a session by id.
    pub fn get_session(&self, id: &str) -> Option<StreamSession> {
        self.sessions.get(id).map(|r| r.value().clone())
    }

    /// Send a chunk of tokens into a stream.
    pub fn send_chunk(
        &self,
        stream_id: &str,
        tokens: Vec<String>,
        is_final: bool,
    ) -> Result<TokenChunk, String> {
        let mut session = self
            .sessions
            .get_mut(stream_id)
            .ok_or_else(|| format!("Stream not found: {stream_id}"))?;

        if session.status == StreamStatus::Completed
            || session.status == StreamStatus::Cancelled
            || session.status == StreamStatus::Failed
            || session.status == StreamStatus::Timeout
        {
            return Err(format!(
                "Cannot send chunk to stream in state: {:?}",
                session.status
            ));
        }

        // Compute latency from last activity
        let now = Utc::now();
        let latency_ms = now
            .signed_duration_since(session.last_activity)
            .num_milliseconds()
            .unsigned_abs();

        // Determine chunk index from current buffer length
        let chunk_index = session.chunks_received;

        let chunk = TokenChunk {
            stream_id: stream_id.to_string(),
            tokens,
            chunk_index,
            is_final,
            latency_ms,
        };

        // Update session counters
        session.chunks_received += 1;
        session.total_tokens += chunk.tokens.len() as u32;
        session.last_activity = now;

        if is_final {
            session.status = StreamStatus::Completed;
            self.active_streams.fetch_sub(1, Ordering::Relaxed);
        } else {
            session.status = StreamStatus::Streaming;
        }

        // Store the chunk in the buffer
        if let Some(mut buf) = self.chunk_buffers.get_mut(stream_id) {
            buf.push(chunk.clone());
        }

        // Update metrics
        self.total_chunks.fetch_add(1, Ordering::Relaxed);
        self.total_chunk_latency_ms
            .fetch_add(latency_ms, Ordering::Relaxed);

        Ok(chunk)
    }

    /// Get a specific chunk by index from a stream buffer.
    pub fn get_next_chunk(&self, stream_id: &str, chunk_index: u32) -> Option<TokenChunk> {
        let buf = self.chunk_buffers.get(stream_id)?;
        buf.get(chunk_index as usize).cloned()
    }

    /// Cancel a stream by id. Returns true if the stream was found and cancelled.
    pub fn cancel_stream(&self, id: &str) -> bool {
        let mut session = match self.sessions.get_mut(id) {
            Some(s) => s,
            None => return false,
        };

        if session.status == StreamStatus::Completed
            || session.status == StreamStatus::Cancelled
            || session.status == StreamStatus::Failed
            || session.status == StreamStatus::Timeout
        {
            return false;
        }

        session.status = StreamStatus::Cancelled;
        session.last_activity = Utc::now();
        self.active_streams.fetch_sub(1, Ordering::Relaxed);
        true
    }

    /// Mark stale sessions as TimedOut and return the count.
    pub fn timeout_stale_streams(&self) -> u32 {
        let cfg = self
            .config
            .read()
            .map_err(|_| ())
            .expect("config read lock poisoned");
        let timeout_ms = cfg.timeout_ms;
        let now = Utc::now();

        let mut timed_out = 0u32;

        for mut entry in self.sessions.iter_mut() {
            let session = entry.value_mut();
            if session.status == StreamStatus::Completed
                || session.status == StreamStatus::Cancelled
                || session.status == StreamStatus::Failed
                || session.status == StreamStatus::Timeout
            {
                continue;
            }

            let elapsed = now
                .signed_duration_since(session.last_activity)
                .num_milliseconds()
                .unsigned_abs();

            if elapsed > timeout_ms {
                session.status = StreamStatus::Timeout;
                session.last_activity = now;
                self.active_streams.fetch_sub(1, Ordering::Relaxed);
                self.timeout_count.fetch_add(1, Ordering::Relaxed);
                timed_out += 1;
            }
        }

        timed_out
    }

    /// List all streams that are currently active (Pending, Active, or Streaming).
    pub fn list_active_streams(&self) -> Vec<StreamSession> {
        self.sessions
            .iter()
            .filter(|r| {
                matches!(
                    r.status,
                    StreamStatus::Pending | StreamStatus::Active | StreamStatus::Streaming
                )
            })
            .map(|r| r.value().clone())
            .collect()
    }

    /// Check whether backpressure is active.
    pub fn get_backpressure(&self) -> bool {
        let cfg = self
            .config
            .read()
            .map_err(|_| ())
            .expect("config read lock poisoned");
        let active = self.active_streams.load(Ordering::Relaxed);
        active >= cfg.backpressure_threshold as u64
    }

    /// Take a snapshot of current streaming metrics.
    pub fn get_metrics(&self) -> StreamMetricsSnapshot {
        let total_streams = self.total_streams.load(Ordering::Relaxed);
        let active_streams = self.active_streams.load(Ordering::Relaxed);
        let total_chunks_delivered = self.total_chunks.load(Ordering::Relaxed);
        let total_latency = self.total_chunk_latency_ms.load(Ordering::Relaxed);
        let timeout_count = self.timeout_count.load(Ordering::Relaxed);
        let backpressure_events = self.backpressure_events.load(Ordering::Relaxed);

        let avg_chunk_latency_ms = if total_chunks_delivered > 0 {
            total_latency as f64 / total_chunks_delivered as f64
        } else {
            0.0
        };

        // Compute average tokens per stream from sessions
        let total_tokens: u32 = self
            .sessions
            .iter()
            .map(|r| r.value().total_tokens)
            .sum();

        let avg_tokens_per_stream = if total_streams > 0 {
            total_tokens as f64 / total_streams as f64
        } else {
            0.0
        };

        StreamMetricsSnapshot {
            total_streams,
            active_streams,
            avg_tokens_per_stream,
            total_chunks_delivered,
            avg_chunk_latency_ms,
            timeout_count,
            backpressure_events,
        }
    }

    /// Replace the configuration and return the old one.
    pub fn update_config(&self, config: StreamConfig) -> StreamConfig {
        let mut cfg = self
            .config
            .write()
            .expect("config write lock poisoned");
        let old = cfg.clone();
        *cfg = config;
        old
    }

    /// Get a clone of the current configuration.
    pub fn get_config(&self) -> StreamConfig {
        let cfg = self
            .config
            .read()
            .expect("config read lock poisoned");
        cfg.clone()
    }
}

// ---------------------------------------------------------------------------
// HTTP request / response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateStreamRequest {
    pub request_id: String,
    pub model: String,
    pub client_id: String,
    #[serde(default = "default_priority")]
    pub priority: String,
}

fn default_priority() -> String {
    "normal".to_string()
}

#[derive(Deserialize)]
pub struct SendChunkRequest {
    pub tokens: Vec<String>,
    #[serde(default)]
    pub is_final: bool,
}

#[derive(Deserialize)]
pub struct UpdateConfigRequest {
    pub max_concurrent_streams: Option<u32>,
    pub chunk_size: Option<u32>,
    pub backpressure_threshold: Option<u32>,
    pub timeout_ms: Option<u64>,
    pub retry_attempts: Option<u32>,
}

// ---------------------------------------------------------------------------
// Router / handlers
// ---------------------------------------------------------------------------

pub fn build_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/streams", post(create_stream_handler))
        .route("/api/streams/{id}", get(get_stream_handler))
        .route("/api/streams/{id}/chunks", post(send_chunk_handler))
        .route("/api/streams/{id}/chunks/{index}", get(get_chunk_handler))
        .route("/api/streams/{id}", delete(cancel_stream_handler))
        .route("/api/streams/timeout", post(timeout_handler))
        .route("/api/streams/active", get(list_active_handler))
        .route("/api/streams/metrics", get(metrics_handler))
        .route("/api/streams/backpressure", get(backpressure_handler))
        .route("/api/streams/config", put(update_config_handler))
        .route("/api/streams/config", get(get_config_handler))
        .with_state(state)
}

async fn create_stream_handler(
    State(state): State<AppState>,
    Json(body): Json<CreateStreamRequest>,
) -> (StatusCode, Json<StreamSession>) {
    let mux = &state.token_streaming;
    match mux.create_session(&body.request_id, &body.model, &body.client_id, &body.priority) {
        Ok(session) => (StatusCode::CREATED, Json(session)),
        Err(_msg) => (
            StatusCode::CONFLICT,
            Json(StreamSession {
                id: String::new(),
                request_id: String::new(),
                model: String::new(),
                client_id: String::new(),
                status: StreamStatus::Failed,
                chunks_received: 0,
                total_tokens: 0,
                started_at: Utc::now(),
                last_activity: Utc::now(),
                priority: String::new(),
            }),
        ),
    }
}

async fn get_stream_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> (StatusCode, Json<Option<StreamSession>>) {
    let session = state.token_streaming.get_session(&id);
    if session.is_some() {
        (StatusCode::OK, Json(session))
    } else {
        (StatusCode::NOT_FOUND, Json(None))
    }
}

async fn send_chunk_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<SendChunkRequest>,
) -> (StatusCode, Json<Option<TokenChunk>>) {
    match state
        .token_streaming
        .send_chunk(&id, body.tokens, body.is_final)
    {
        Ok(chunk) => (StatusCode::OK, Json(Some(chunk))),
        Err(_) => (StatusCode::NOT_FOUND, Json(None)),
    }
}

async fn get_chunk_handler(
    State(state): State<AppState>,
    Path((id, index)): Path<(String, u32)>,
) -> (StatusCode, Json<Option<TokenChunk>>) {
    let chunk = state.token_streaming.get_next_chunk(&id, index);
    if chunk.is_some() {
        (StatusCode::OK, Json(chunk))
    } else {
        (StatusCode::NOT_FOUND, Json(None))
    }
}

async fn cancel_stream_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> StatusCode {
    if state.token_streaming.cancel_stream(&id) {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn timeout_handler(State(state): State<AppState>) -> Json<u32> {
    let count = state.token_streaming.timeout_stale_streams();
    Json(count)
}

async fn list_active_handler(
    State(state): State<AppState>,
) -> Json<Vec<StreamSession>> {
    Json(state.token_streaming.list_active_streams())
}

async fn metrics_handler(State(state): State<AppState>) -> Json<StreamMetricsSnapshot> {
    Json(state.token_streaming.get_metrics())
}

async fn backpressure_handler(State(state): State<AppState>) -> Json<bool> {
    Json(state.token_streaming.get_backpressure())
}

async fn update_config_handler(
    State(state): State<AppState>,
    Json(body): Json<UpdateConfigRequest>,
) -> Json<StreamConfig> {
    let current = state.token_streaming.get_config();
    let updated = StreamConfig {
        max_concurrent_streams: body.max_concurrent_streams.unwrap_or(current.max_concurrent_streams),
        chunk_size: body.chunk_size.unwrap_or(current.chunk_size),
        backpressure_threshold: body.backpressure_threshold.unwrap_or(current.backpressure_threshold),
        timeout_ms: body.timeout_ms.unwrap_or(current.timeout_ms),
        retry_attempts: body.retry_attempts.unwrap_or(current.retry_attempts),
    };
    let old = state.token_streaming.update_config(updated);
    Json(old)
}

async fn get_config_handler(State(state): State<AppState>) -> Json<StreamConfig> {
    Json(state.token_streaming.get_config())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mux() -> TokenStreamingMultiplexer {
        TokenStreamingMultiplexer::new(StreamConfig::default())
    }

    // -- StreamStatus tests ------------------------------------------------

    #[test]
    fn test_stream_status_equality() {
        assert_eq!(StreamStatus::Pending, StreamStatus::Pending);
        assert_eq!(StreamStatus::Active, StreamStatus::Active);
        assert_eq!(StreamStatus::Streaming, StreamStatus::Streaming);
        assert_eq!(StreamStatus::Completed, StreamStatus::Completed);
        assert_eq!(StreamStatus::Failed, StreamStatus::Failed);
        assert_eq!(StreamStatus::Timeout, StreamStatus::Timeout);
        assert_eq!(StreamStatus::Cancelled, StreamStatus::Cancelled);
        assert_ne!(StreamStatus::Pending, StreamStatus::Completed);
    }

    #[test]
    fn test_stream_status_clone() {
        let s = StreamStatus::Streaming;
        let s2 = s.clone();
        assert_eq!(s, s2);
    }

    #[test]
    fn test_stream_status_serialize_deserialize() {
        let statuses = vec![
            StreamStatus::Pending,
            StreamStatus::Active,
            StreamStatus::Streaming,
            StreamStatus::Completed,
            StreamStatus::Failed,
            StreamStatus::Timeout,
            StreamStatus::Cancelled,
        ];
        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let back: StreamStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, back);
        }
    }

    // -- StreamConfig tests ------------------------------------------------

    #[test]
    fn test_stream_config_default() {
        let cfg = StreamConfig::default();
        assert_eq!(cfg.max_concurrent_streams, 1000);
        assert_eq!(cfg.chunk_size, 16);
        assert_eq!(cfg.backpressure_threshold, 100);
        assert_eq!(cfg.timeout_ms, 30_000);
        assert_eq!(cfg.retry_attempts, 3);
    }

    #[test]
    fn test_stream_config_serialize_deserialize() {
        let cfg = StreamConfig::default();
        let json = serde_json::to_string(&cfg).unwrap();
        let back: StreamConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg.max_concurrent_streams, back.max_concurrent_streams);
        assert_eq!(cfg.chunk_size, back.chunk_size);
        assert_eq!(cfg.backpressure_threshold, back.backpressure_threshold);
        assert_eq!(cfg.timeout_ms, back.timeout_ms);
        assert_eq!(cfg.retry_attempts, back.retry_attempts);
    }

    // -- TokenChunk tests --------------------------------------------------

    #[test]
    fn test_token_chunk_fields() {
        let chunk = TokenChunk {
            stream_id: "s1".to_string(),
            tokens: vec!["hello".to_string(), "world".to_string()],
            chunk_index: 0,
            is_final: false,
            latency_ms: 5,
        };
        assert_eq!(chunk.stream_id, "s1");
        assert_eq!(chunk.tokens.len(), 2);
        assert_eq!(chunk.chunk_index, 0);
        assert!(!chunk.is_final);
    }

    #[test]
    fn test_token_chunk_serialize() {
        let chunk = TokenChunk {
            stream_id: "s1".to_string(),
            tokens: vec!["a".to_string()],
            chunk_index: 0,
            is_final: true,
            latency_ms: 0,
        };
        let json = serde_json::to_string(&chunk).unwrap();
        assert!(json.contains("\"is_final\":true"));
    }

    // -- StreamSession tests -----------------------------------------------

    #[test]
    fn test_stream_session_clone() {
        let session = StreamSession {
            id: "abc".to_string(),
            request_id: "req1".to_string(),
            model: "gpt-4".to_string(),
            client_id: "c1".to_string(),
            status: StreamStatus::Active,
            chunks_received: 5,
            total_tokens: 42,
            started_at: Utc::now(),
            last_activity: Utc::now(),
            priority: "high".to_string(),
        };
        let cloned = session.clone();
        assert_eq!(session.id, cloned.id);
        assert_eq!(session.total_tokens, cloned.total_tokens);
    }

    // -- TokenStreamingMultiplexer tests -----------------------------------

    #[test]
    fn test_new_multiplexer() {
        let mux = make_mux();
        let metrics = mux.get_metrics();
        assert_eq!(metrics.total_streams, 0);
        assert_eq!(metrics.active_streams, 0);
    }

    #[test]
    fn test_create_session() {
        let mux = make_mux();
        let session = mux
            .create_session("req-1", "gpt-4", "client-1", "normal")
            .unwrap();
        assert_eq!(session.request_id, "req-1");
        assert_eq!(session.model, "gpt-4");
        assert_eq!(session.client_id, "client-1");
        assert_eq!(session.status, StreamStatus::Pending);
        assert_eq!(session.chunks_received, 0);
        assert_eq!(session.total_tokens, 0);
    }

    #[test]
    fn test_create_session_increments_counters() {
        let mux = make_mux();
        mux.create_session("req-1", "m", "c", "p").unwrap();
        mux.create_session("req-2", "m", "c", "p").unwrap();
        let metrics = mux.get_metrics();
        assert_eq!(metrics.total_streams, 2);
        assert_eq!(metrics.active_streams, 2);
    }

    #[test]
    fn test_get_session_found() {
        let mux = make_mux();
        let created = mux
            .create_session("req-1", "gpt-4", "c1", "high")
            .unwrap();
        let fetched = mux.get_session(&created.id).unwrap();
        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.request_id, "req-1");
    }

    #[test]
    fn test_get_session_not_found() {
        let mux = make_mux();
        assert!(mux.get_session("nonexistent").is_none());
    }

    #[test]
    fn test_send_chunk_success() {
        let mux = make_mux();
        let session = mux
            .create_session("req-1", "m", "c", "p")
            .unwrap();
        let chunk = mux
            .send_chunk(&session.id, vec!["hello".to_string()], false)
            .unwrap();
        assert_eq!(chunk.chunk_index, 0);
        assert!(!chunk.is_final);
        assert_eq!(chunk.tokens, vec!["hello"]);

        // Verify session was updated
        let s = mux.get_session(&session.id).unwrap();
        assert_eq!(s.chunks_received, 1);
        assert_eq!(s.total_tokens, 1);
        assert_eq!(s.status, StreamStatus::Streaming);
    }

    #[test]
    fn test_send_final_chunk_completes_stream() {
        let mux = make_mux();
        let session = mux
            .create_session("req-1", "m", "c", "p")
            .unwrap();
        let _ = mux.send_chunk(&session.id, vec!["end".to_string()], true).unwrap();

        let s = mux.get_session(&session.id).unwrap();
        assert_eq!(s.status, StreamStatus::Completed);

        let metrics = mux.get_metrics();
        assert_eq!(metrics.active_streams, 0);
    }

    #[test]
    fn test_send_chunk_to_nonexistent_stream() {
        let mux = make_mux();
        let result = mux.send_chunk("nope", vec!["x".to_string()], false);
        assert!(result.is_err());
    }

    #[test]
    fn test_send_chunk_after_completion_fails() {
        let mux = make_mux();
        let session = mux
            .create_session("req-1", "m", "c", "p")
            .unwrap();
        mux.send_chunk(&session.id, vec!["end".to_string()], true)
            .unwrap();
        let result = mux.send_chunk(&session.id, vec!["extra".to_string()], false);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_next_chunk() {
        let mux = make_mux();
        let session = mux
            .create_session("req-1", "m", "c", "p")
            .unwrap();
        let _ = mux
            .send_chunk(&session.id, vec!["a".to_string()], false)
            .unwrap();
        let _ = mux
            .send_chunk(&session.id, vec!["b".to_string()], false)
            .unwrap();

        let c0 = mux.get_next_chunk(&session.id, 0).unwrap();
        assert_eq!(c0.tokens, vec!["a"]);

        let c1 = mux.get_next_chunk(&session.id, 1).unwrap();
        assert_eq!(c1.tokens, vec!["b"]);

        assert!(mux.get_next_chunk(&session.id, 2).is_none());
    }

    #[test]
    fn test_cancel_stream() {
        let mux = make_mux();
        let session = mux
            .create_session("req-1", "m", "c", "p")
            .unwrap();
        assert!(mux.cancel_stream(&session.id));

        let s = mux.get_session(&session.id).unwrap();
        assert_eq!(s.status, StreamStatus::Cancelled);

        let metrics = mux.get_metrics();
        assert_eq!(metrics.active_streams, 0);
    }

    #[test]
    fn test_cancel_nonexistent_stream() {
        let mux = make_mux();
        assert!(!mux.cancel_stream("nope"));
    }

    #[test]
    fn test_cancel_already_completed_stream() {
        let mux = make_mux();
        let session = mux
            .create_session("req-1", "m", "c", "p")
            .unwrap();
        mux.send_chunk(&session.id, vec!["end".to_string()], true)
            .unwrap();
        assert!(!mux.cancel_stream(&session.id));
    }

    #[test]
    fn test_timeout_stale_streams() {
        let mut cfg = StreamConfig::default();
        cfg.timeout_ms = 0; // everything is stale immediately
        let mux = TokenStreamingMultiplexer::new(cfg);

        mux.create_session("req-1", "m", "c", "p").unwrap();
        mux.create_session("req-2", "m", "c", "p").unwrap();

        // Give a tiny bit of time so elapsed > 0
        std::thread::sleep(std::time::Duration::from_millis(2));

        let timed_out = mux.timeout_stale_streams();
        assert_eq!(timed_out, 2);

        let metrics = mux.get_metrics();
        assert_eq!(metrics.timeout_count, 2);
        assert_eq!(metrics.active_streams, 0);
    }

    #[test]
    fn test_timeout_skips_completed_streams() {
        let mut cfg = StreamConfig::default();
        cfg.timeout_ms = 0;
        let mux = TokenStreamingMultiplexer::new(cfg);

        let session = mux
            .create_session("req-1", "m", "c", "p")
            .unwrap();
        mux.send_chunk(&session.id, vec!["done".to_string()], true)
            .unwrap();

        std::thread::sleep(std::time::Duration::from_millis(2));

        let timed_out = mux.timeout_stale_streams();
        assert_eq!(timed_out, 0);
    }

    #[test]
    fn test_list_active_streams() {
        let mux = make_mux();
        let s1 = mux
            .create_session("req-1", "m", "c", "p")
            .unwrap();
        let _s2 = mux
            .create_session("req-2", "m", "c", "p")
            .unwrap();

        // Complete s1
        mux.send_chunk(&s1.id, vec!["done".to_string()], true)
            .unwrap();

        let active = mux.list_active_streams();
        assert_eq!(active.len(), 1);
        // The remaining active one should be s2
        assert_ne!(active[0].id, s1.id);
    }

    #[test]
    fn test_backpressure_false() {
        let mux = make_mux();
        assert!(!mux.get_backpressure());
    }

    #[test]
    fn test_backpressure_true_when_threshold_met() {
        let mut cfg = StreamConfig::default();
        cfg.backpressure_threshold = 2;
        let mux = TokenStreamingMultiplexer::new(cfg);

        mux.create_session("r1", "m", "c", "p").unwrap();
        assert!(!mux.get_backpressure());

        mux.create_session("r2", "m", "c", "p").unwrap();
        assert!(mux.get_backpressure());
    }

    #[test]
    fn test_max_concurrent_streams_limit() {
        let mut cfg = StreamConfig::default();
        cfg.max_concurrent_streams = 1;
        let mux = TokenStreamingMultiplexer::new(cfg);

        mux.create_session("r1", "m", "c", "p").unwrap();
        let result = mux.create_session("r2", "m", "c", "p");
        assert!(result.is_err());

        let metrics = mux.get_metrics();
        assert_eq!(metrics.backpressure_events, 1);
    }

    #[test]
    fn test_update_config() {
        let mux = make_mux();
        let new_cfg = StreamConfig {
            max_concurrent_streams: 500,
            chunk_size: 32,
            backpressure_threshold: 50,
            timeout_ms: 10_000,
            retry_attempts: 5,
        };
        let old = mux.update_config(new_cfg.clone());
        assert_eq!(old.max_concurrent_streams, 1000);

        let current = mux.get_config();
        assert_eq!(current.max_concurrent_streams, 500);
        assert_eq!(current.chunk_size, 32);
        assert_eq!(current.retry_attempts, 5);
    }

    #[test]
    fn test_metrics_snapshot() {
        let mux = make_mux();
        let session = mux
            .create_session("r1", "m", "c", "p")
            .unwrap();
        mux.send_chunk(&session.id, vec!["a".to_string()], false)
            .unwrap();
        mux.send_chunk(&session.id, vec!["b".to_string(), "c".to_string()], true)
            .unwrap();

        let metrics = mux.get_metrics();
        assert_eq!(metrics.total_streams, 1);
        assert_eq!(metrics.active_streams, 0);
        assert_eq!(metrics.total_chunks_delivered, 2);
        // avg_tokens_per_stream = 3/1 = 3.0
        assert!((metrics.avg_tokens_per_stream - 3.0).abs() < 0.01);
        assert!(metrics.avg_chunk_latency_ms >= 0.0);
    }

    // Helper assertion for avg tokens -- we validate the formula indirectly.
    // The above test already checks the fields, but let's add a dedicated
    // test that checks the math more explicitly.

    #[test]
    fn test_avg_tokens_per_stream_calculation() {
        let mux = make_mux();

        let s1 = mux.create_session("r1", "m", "c", "p").unwrap();
        mux.send_chunk(&s1.id, vec!["a".to_string(), "b".to_string()], true)
            .unwrap();

        let s2 = mux.create_session("r2", "m", "c", "p").unwrap();
        mux.send_chunk(&s2.id, vec!["x".to_string()], true)
            .unwrap();

        let metrics = mux.get_metrics();
        // Total tokens = 3, total streams = 2, avg = 1.5
        assert!((metrics.avg_tokens_per_stream - 1.5).abs() < 0.01);
    }

    #[test]
    fn test_avg_chunk_latency_calculation() {
        let mux = make_mux();
        let s1 = mux.create_session("r1", "m", "c", "p").unwrap();
        // Sending two chunks rapidly; latency should be small but >= 0
        let _ = mux.send_chunk(&s1.id, vec!["a".to_string()], false).unwrap();
        let _ = mux.send_chunk(&s1.id, vec!["b".to_string()], true).unwrap();

        let metrics = mux.get_metrics();
        assert_eq!(metrics.total_chunks_delivered, 2);
        assert!(metrics.avg_chunk_latency_ms.is_finite());
    }

    #[test]
    fn test_send_multiple_chunks_sequential_indices() {
        let mux = make_mux();
        let session = mux
            .create_session("r1", "m", "c", "p")
            .unwrap();
        for i in 0..10u32 {
            let chunk = mux
                .send_chunk(
                    &session.id,
                    vec![format!("token-{i}")],
                    i == 9,
                )
                .unwrap();
            assert_eq!(chunk.chunk_index, i);
        }
        let s = mux.get_session(&session.id).unwrap();
        assert_eq!(s.chunks_received, 10);
        assert_eq!(s.total_tokens, 10);
        assert_eq!(s.status, StreamStatus::Completed);
    }

    #[test]
    fn test_priority_field_stored() {
        let mux = make_mux();
        let session = mux
            .create_session("r1", "m", "c", "urgent")
            .unwrap();
        assert_eq!(session.priority, "urgent");

        let fetched = mux.get_session(&session.id).unwrap();
        assert_eq!(fetched.priority, "urgent");
    }

    #[test]
    fn test_get_chunk_from_nonexistent_stream() {
        let mux = make_mux();
        assert!(mux.get_next_chunk("nope", 0).is_none());
    }

    #[test]
    fn test_timeout_handler_empty() {
        let mux = make_mux();
        let count = mux.timeout_stale_streams();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_empty_metrics() {
        let mux = make_mux();
        let m = mux.get_metrics();
        assert_eq!(m.total_streams, 0);
        assert_eq!(m.active_streams, 0);
        assert_eq!(m.total_chunks_delivered, 0);
        assert_eq!(m.avg_tokens_per_stream, 0.0);
        assert_eq!(m.avg_chunk_latency_ms, 0.0);
        assert_eq!(m.timeout_count, 0);
        assert_eq!(m.backpressure_events, 0);
    }

    #[test]
    fn test_cancel_stream_reduces_active_count() {
        let mux = make_mux();
        let s1 = mux.create_session("r1", "m", "c", "p").unwrap();
        let _s2 = mux.create_session("r2", "m", "c", "p").unwrap();

        mux.cancel_stream(&s1.id);
        let metrics = mux.get_metrics();
        assert_eq!(metrics.active_streams, 1);
    }
}
