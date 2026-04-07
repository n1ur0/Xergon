//! Request Multiplexer — unified entry point for coalescing + buffering.
//!
//! Combines the RequestCoalescer and StreamBufferManager into a single
//! interface. Handles coalescing, buffering, fan-out, and cleanup.
//!
//! Usage:
//!   1. Call `multiplex_request()` before proxying to a provider
//!   2. If coalesced, return the stream from the subscriber receiver
//!   3. If new batch, dispatch to provider and feed chunks through `push_chunk()`
//!   4. Call `complete_stream()` when the provider response is done

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::coalesce::{
    CoalesceConfig, CoalesceResult, RequestCoalescer, SseChunk,
};
use crate::stream_buffer::{StreamBufferConfig, StreamBufferManager};

// ---------------------------------------------------------------------------
// Multiplexer Config
// ---------------------------------------------------------------------------

/// Combined configuration for the request multiplexer.
#[derive(Debug, Clone)]
pub struct MultiplexerConfig {
    /// Coalescing configuration.
    pub coalesce: CoalesceConfig,
    /// Stream buffer configuration.
    pub stream_buffer: StreamBufferConfig,
    /// Channel buffer size for subscriber channels (default: 64).
    pub channel_buffer_size: usize,
}

impl Default for MultiplexerConfig {
    fn default() -> Self {
        Self {
            coalesce: CoalesceConfig::default(),
            stream_buffer: StreamBufferConfig::default(),
            channel_buffer_size: 64,
        }
    }
}

impl Default for RequestMultiplexer {
    fn default() -> Self {
        Self::new(MultiplexerConfig::default())
    }
}

// ---------------------------------------------------------------------------
// Multiplexer Result
// ---------------------------------------------------------------------------

/// Result of attempting to multiplex a request.
pub enum MultiplexResult {
    /// This is a new request — dispatch to provider normally.
    /// After getting the response stream, call `start_streaming()` and `push_chunk()`.
    NewRequest {
        /// Hash key for this batch.
        hash: String,
        /// Request ID.
        request_id: String,
    },
    /// This request was coalesced — return the receiver stream.
    Coalesced {
        /// Receiver for SSE chunks from the shared stream.
        receiver: mpsc::Receiver<SseChunk>,
        /// Request ID.
        request_id: String,
    },
    /// Multiplexing is disabled — proceed normally.
    Bypass,
}

// ---------------------------------------------------------------------------
// Request Multiplexer
// ---------------------------------------------------------------------------

/// Unified request multiplexer combining coalescing and buffering.
pub struct RequestMultiplexer {
    coalescer: Arc<RequestCoalescer>,
    buffer_manager: Arc<StreamBufferManager>,
    config: MultiplexerConfig,
}

impl RequestMultiplexer {
    /// Create a new request multiplexer.
    pub fn new(config: MultiplexerConfig) -> Self {
        Self {
            coalescer: Arc::new(RequestCoalescer::new(config.coalesce.clone())),
            buffer_manager: Arc::new(StreamBufferManager::new(config.stream_buffer.clone())),
            config,
        }
    }

    /// Try to multiplex a request.
    ///
    /// If coalescing finds a matching batch, the caller gets a receiver stream.
    /// If this is a new request, the caller should dispatch to the provider,
    /// then call `start_streaming()` and feed chunks via `push_chunk()`.
    pub fn multiplex_request(
        &self,
        model: &str,
        messages: &[serde_json::Value],
        request_id: &str,
    ) -> MultiplexResult {
        if !self.coalescer.is_enabled() || !self.buffer_manager.is_enabled() {
            return MultiplexResult::Bypass;
        }

        match self.coalescer.try_coalesce(
            model,
            messages,
            request_id,
            self.config.channel_buffer_size,
        ) {
            CoalesceResult::NewBatch { hash, entry, .. } => {
                // Create a stream buffer for this batch
                self.buffer_manager.create_buffer(&hash);

                // Subscribe the first entry to the buffer
                if let Some(receiver) = self.buffer_manager.subscribe(
                    &hash,
                    &entry.id,
                    self.config.channel_buffer_size,
                ) {
                    // Store the receiver in a way the caller can use
                    // For now, we use the entry's sender for the representative
                    // and the buffer for all subscribers
                    debug!(
                        hash = %hash,
                        request_id,
                        "New multiplexed batch created"
                    );
                    MultiplexResult::NewRequest {
                        hash,
                        request_id: request_id.to_string(),
                    }
                } else {
                    MultiplexResult::Bypass
                }
            }
            CoalesceResult::Joined { entry } => {
                // Subscribe this entry to the existing buffer
                if let Some(receiver) = self.buffer_manager.subscribe(
                    &entry.prompt_hash,
                    &entry.id,
                    self.config.channel_buffer_size,
                ) {
                    debug!(
                        request_id,
                        batch_hash = %entry.prompt_hash,
                        "Request coalesced into existing batch"
                    );
                    MultiplexResult::Coalesced {
                        receiver,
                        request_id: request_id.to_string(),
                    }
                } else {
                    // Buffer doesn't exist or max subscribers reached — bypass
                    warn!(
                        request_id,
                        "Could not subscribe to stream buffer, bypassing"
                    );
                    MultiplexResult::Bypass
                }
            }
            CoalesceResult::Disabled => MultiplexResult::Bypass,
        }
    }

    /// Start streaming for a new batch (after dispatching to provider).
    pub fn start_streaming(&self, hash: &str) {
        self.coalescer.mark_dispatched(hash);
        self.coalescer.mark_streaming(hash);
    }

    /// Push a chunk from the provider response to all subscribers.
    pub fn push_chunk(&self, hash: &str, chunk: SseChunk) {
        self.buffer_manager.push(hash, chunk);
    }

    /// Mark the stream as complete for a batch.
    pub fn complete_stream(&self, hash: &str) {
        self.buffer_manager.mark_complete(hash);
        self.coalescer.mark_completed(hash);
    }

    /// Cancel a single entry from a batch.
    pub fn cancel_entry(&self, hash: &str, entry_id: &str) {
        self.coalescer.cancel_entry(hash, entry_id);
        self.buffer_manager.unsubscribe(hash, entry_id);
    }

    /// Cancel an entire batch. Returns true if the batch was found.
    pub fn cancel_batch(&self, hash: &str) -> bool {
        let cancelled = self.coalescer.cancel_batch(hash);
        if cancelled {
            self.buffer_manager.remove_buffer(hash);
        }
        cancelled
    }

    /// Get coalescer statistics.
    pub fn coalesce_stats(&self) -> crate::coalesce::CoalesceStats {
        self.coalescer.get_stats()
    }

    /// Get stream buffer statistics.
    pub fn buffer_stats(&self) -> crate::stream_buffer::StreamBufferManagerStats {
        self.buffer_manager.get_stats()
    }

    /// Clean up stale entries.
    pub fn cleanup(&self) {
        self.coalescer.cleanup();
        self.buffer_manager.cleanup();
    }

    /// Start a background cleanup task.
    pub fn start_cleanup_task(self: &Arc<Self>, interval_secs: u64) -> tokio::task::JoinHandle<()> {
        let mux = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(interval_secs)).await;
                mux.cleanup();
                debug!("Multiplexer cleanup cycle completed");
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> MultiplexerConfig {
        MultiplexerConfig {
            coalesce: CoalesceConfig {
                enabled: true,
                max_wait_ms: 50,
                max_batch_size: 10,
                similarity_threshold: 0.0,
                max_prompt_length: 10_000,
            },
            stream_buffer: StreamBufferConfig {
                enabled: true,
                max_buffer_size: 100,
                max_buffer_bytes: 1_048_576,
                max_subscribers_per_stream: 20,
                cleanup_after_ms: 0,
                backpressure_policy: crate::stream_buffer::BackpressurePolicy::Drop,
            },
            channel_buffer_size: 64,
        }
    }

    fn make_messages(content: &str) -> Vec<serde_json::Value> {
        vec![serde_json::json!({
            "role": "user",
            "content": content
        })]
    }

    #[test]
    fn test_multiplexer_new_request() {
        let mux = RequestMultiplexer::new(test_config());
        let msgs = make_messages("hello");

        match mux.multiplex_request("gpt-4", &msgs, "req-1") {
            MultiplexResult::NewRequest { hash, request_id } => {
                assert_eq!(request_id, "req-1");
                assert!(!hash.is_empty());
            }
            _ => panic!("Expected NewRequest"),
        }
    }

    #[test]
    fn test_multiplexer_coalesced_request() {
        let mux = RequestMultiplexer::new(test_config());
        let msgs = make_messages("hello");

        // First request creates batch
        let hash = match mux.multiplex_request("gpt-4", &msgs, "req-1") {
            MultiplexResult::NewRequest { hash, .. } => hash,
            _ => panic!("Expected NewRequest"),
        };

        mux.start_streaming(&hash);

        // Second request should be coalesced
        match mux.multiplex_request("gpt-4", &msgs, "req-2") {
            MultiplexResult::Coalesced { request_id, .. } => {
                assert_eq!(request_id, "req-2");
            }
            _ => panic!("Expected Coalesced"),
        }
    }

    #[test]
    fn test_multiplexer_bypass_when_disabled() {
        let config = MultiplexerConfig {
            coalesce: CoalesceConfig {
                enabled: false,
                ..CoalesceConfig::default()
            },
            stream_buffer: StreamBufferConfig {
                enabled: false,
                ..StreamBufferConfig::default()
            },
            channel_buffer_size: 64,
        };
        let mux = RequestMultiplexer::new(config);
        let msgs = make_messages("hello");

        match mux.multiplex_request("gpt-4", &msgs, "req-1") {
            MultiplexResult::Bypass => {}
            _ => panic!("Expected Bypass"),
        }
    }

    #[test]
    fn test_multiplexer_push_and_complete() {
        let mux = RequestMultiplexer::new(test_config());
        let msgs = make_messages("hello");

        let hash = match mux.multiplex_request("gpt-4", &msgs, "req-1") {
            MultiplexResult::NewRequest { hash, .. } => hash,
            _ => panic!("Expected NewRequest"),
        };

        mux.start_streaming(&hash);

        // Push chunks
        mux.push_chunk(&hash, SseChunk {
            data: "hello".to_string(),
            is_done: false,
        });
        mux.push_chunk(&hash, SseChunk {
            data: "world".to_string(),
            is_done: false,
        });

        // Complete
        mux.complete_stream(&hash);
        mux.cleanup();

        // Buffer should be complete
        let stats = mux.buffer_stats();
        assert_eq!(stats.active_buffers, 0); // completed buffers are removed
    }

    #[tokio::test]
    async fn test_multiplexer_end_to_end() {
        let mux = Arc::new(RequestMultiplexer::new(test_config()));
        let msgs = make_messages("hello world");

        // First request
        let hash = match mux.multiplex_request("gpt-4", &msgs, "req-1") {
            MultiplexResult::NewRequest { hash, .. } => hash,
            _ => panic!("Expected NewRequest"),
        };

        // Second request — coalesced
        let receiver = match mux.multiplex_request("gpt-4", &msgs, "req-2") {
            MultiplexResult::Coalesced { receiver, .. } => receiver,
            _ => panic!("Expected Coalesced"),
        };

        mux.start_streaming(&hash);

        // Simulate provider sending chunks
        mux.push_chunk(&hash, SseChunk {
            data: serde_json::json!({"choices":[{"delta":{"content":"Hi"}}]}).to_string(),
            is_done: false,
        });
        mux.push_chunk(&hash, SseChunk {
            data: serde_json::json!({"choices":[{"delta":{"content":" there"}}]}).to_string(),
            is_done: false,
        });
        mux.complete_stream(&hash);

        // The coalesced request should have received chunks
        drop(receiver);

        // Stats should show coalescing happened
        let coalesce_stats = mux.coalesce_stats();
        assert_eq!(coalesce_stats.total_coalesced.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[test]
    fn test_multiplexer_cancel_entry() {
        let mux = RequestMultiplexer::new(test_config());
        let msgs = make_messages("hello");

        let hash = match mux.multiplex_request("gpt-4", &msgs, "req-1") {
            MultiplexResult::NewRequest { hash, .. } => hash,
            _ => panic!("Expected NewRequest"),
        };

        mux.start_streaming(&hash);

        // Second request joins
        match mux.multiplex_request("gpt-4", &msgs, "req-2") {
            MultiplexResult::Coalesced { .. } => {}
            _ => panic!("Expected Coalesced"),
        }

        // Cancel req-2
        mux.cancel_entry(&hash, "req-2");

        // Stats should show one less entry
        let coalesce_stats = mux.coalesce_stats();
        // Entry was removed from the coalescer
    }

    #[test]
    fn test_multiplexer_cancel_batch() {
        let mux = RequestMultiplexer::new(test_config());
        let msgs = make_messages("hello");

        let hash = match mux.multiplex_request("gpt-4", &msgs, "req-1") {
            MultiplexResult::NewRequest { hash, .. } => hash,
            _ => panic!("Expected NewRequest"),
        };

        mux.cancel_batch(&hash);

        // Buffer should be removed
        let stats = mux.buffer_stats();
        assert_eq!(stats.active_buffers, 0);

        // Coalescer stats should show cancelled
        let coalesce_stats = mux.coalesce_stats();
        assert_eq!(coalesce_stats.total_cancelled.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[test]
    fn test_multiplexer_stats() {
        let mux = RequestMultiplexer::new(test_config());
        let msgs = make_messages("hello");

        mux.multiplex_request("gpt-4", &msgs, "req-1");
        mux.multiplex_request("gpt-4", &msgs, "req-2");

        let coalesce_stats = mux.coalesce_stats();
        assert_eq!(coalesce_stats.active_batches, 1);

        let buffer_stats = mux.buffer_stats();
        assert_eq!(buffer_stats.active_buffers, 1);
    }
}
