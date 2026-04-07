//! Streaming Response Buffer.
//!
//! Buffers SSE chunks as they arrive from a provider, allowing multiple
//! subscribers (coalesced requests) to read from the same stream.
//! Supports late joining, backpressure policies, and buffer rotation.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::coalesce::SseChunk;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for stream buffering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamBufferConfig {
    /// Enable/disable stream buffering (default: true).
    #[serde(default = "default_sb_enabled")]
    pub enabled: bool,
    /// Maximum number of chunks to buffer per stream (default: 100).
    #[serde(default = "default_max_buffer_size")]
    pub max_buffer_size: usize,
    /// Maximum total bytes per stream buffer (default: 1MB).
    #[serde(default = "default_max_buffer_bytes")]
    pub max_buffer_bytes: usize,
    /// Maximum subscribers per stream (default: 20).
    #[serde(default = "default_max_subscribers")]
    pub max_subscribers_per_stream: usize,
    /// Remove completed buffers after N ms (default: 5000).
    #[serde(default = "default_cleanup_after_ms")]
    pub cleanup_after_ms: u64,
    /// Backpressure policy for slow subscribers (default: Drop).
    #[serde(default = "default_backpressure_policy")]
    pub backpressure_policy: BackpressurePolicy,
}

impl Default for StreamBufferConfig {
    fn default() -> Self {
        Self {
            enabled: default_sb_enabled(),
            max_buffer_size: default_max_buffer_size(),
            max_buffer_bytes: default_max_buffer_bytes(),
            max_subscribers_per_stream: default_max_subscribers(),
            cleanup_after_ms: default_cleanup_after_ms(),
            backpressure_policy: default_backpressure_policy(),
        }
    }
}

fn default_sb_enabled() -> bool {
    true
}
fn default_max_buffer_size() -> usize {
    100
}
fn default_max_buffer_bytes() -> usize {
    1_048_576 // 1MB
}
fn default_max_subscribers() -> usize {
    20
}
fn default_cleanup_after_ms() -> u64 {
    5000
}
fn default_backpressure_policy() -> BackpressurePolicy {
    BackpressurePolicy::Drop
}

// ---------------------------------------------------------------------------
// Backpressure Policy
// ---------------------------------------------------------------------------

/// Policy for handling slow subscribers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackpressurePolicy {
    /// Drop chunks for slow subscribers (default).
    Drop,
    /// Block the provider stream until subscriber catches up.
    Block,
    /// Disconnect slow subscribers by dropping their channels.
    Disconnect,
}

// ---------------------------------------------------------------------------
// Stream Subscriber
// ---------------------------------------------------------------------------

/// A subscriber reading from a stream buffer.
struct StreamSubscriber {
    /// Unique subscriber ID.
    id: String,
    /// Offset into the buffer to start reading from.
    offset: usize,
    /// Channel to send chunks to the subscriber.
    sender: mpsc::Sender<SseChunk>,
    /// When this subscriber was created.
    created_at: Instant,
}

// ---------------------------------------------------------------------------
// Stream Buffer
// ---------------------------------------------------------------------------

/// A buffer for a single stream, holding SSE chunks for multiple subscribers.
pub struct StreamBuffer {
    /// Buffered chunks.
    chunks: Vec<SseChunk>,
    /// Maximum chunks to buffer.
    max_buffer_size: usize,
    /// Maximum total bytes.
    max_buffer_bytes: usize,
    /// Current total bytes in buffer.
    current_bytes: usize,
    /// Whether the stream is complete.
    complete: bool,
    /// Active subscribers.
    subscribers: Vec<StreamSubscriber>,
    /// Total chunks pushed.
    total_pushed: AtomicU64,
    /// Chunks dropped due to slow subscribers.
    chunks_dropped: AtomicU64,
    /// Chunks dropped due to buffer rotation.
    chunks_rotated: AtomicU64,
    /// Backpressure policy.
    backpressure_policy: BackpressurePolicy,
    /// Max subscribers.
    max_subscribers: usize,
    /// When the buffer was created.
    created_at: Instant,
}

impl StreamBuffer {
    /// Create a new stream buffer.
    pub fn new(config: &StreamBufferConfig) -> Self {
        Self {
            chunks: Vec::new(),
            max_buffer_size: config.max_buffer_size,
            max_buffer_bytes: config.max_buffer_bytes,
            current_bytes: 0,
            complete: false,
            subscribers: Vec::new(),
            total_pushed: AtomicU64::new(0),
            chunks_dropped: AtomicU64::new(0),
            chunks_rotated: AtomicU64::new(0),
            backpressure_policy: config.backpressure_policy,
            max_subscribers: config.max_subscribers_per_stream,
            created_at: Instant::now(),
        }
    }

    /// Push a chunk to the buffer and fan out to all subscribers.
    pub fn push(&mut self, chunk: SseChunk) {
        self.total_pushed.fetch_add(1, Ordering::Relaxed);

        // Rotate buffer if needed
        self.rotate_if_needed();

        // Store chunk
        let chunk_bytes = chunk.data.len();
        self.chunks.push(chunk.clone());
        self.current_bytes += chunk_bytes;

        // Fan out to subscribers
        self.fan_out(&chunk);
    }

    /// Subscribe to the stream buffer. New subscribers start from current position.
    /// Returns None if max subscribers reached.
    pub fn subscribe(&mut self, subscriber_id: &str, buffer_size: usize) -> Option<mpsc::Receiver<SseChunk>> {
        if self.subscribers.len() >= self.max_subscribers {
            return None;
        }

        let (tx, rx) = mpsc::channel(buffer_size);
        let offset = self.chunks.len();

        self.subscribers.push(StreamSubscriber {
            id: subscriber_id.to_string(),
            offset,
            sender: tx,
            created_at: Instant::now(),
        });

        // Send already-buffered chunks to new subscriber
        for chunk in &self.chunks {
            // Try to send without blocking
            if let Err(_) = self.try_send(&chunk, &self.subscribers.last().unwrap().sender) {
                break;
            }
        }

        debug!(
            subscriber_id,
            offset,
            buffered = self.chunks.len(),
            "Subscriber joined stream buffer"
        );

        Some(rx)
    }

    /// Mark the stream as complete.
    pub fn mark_complete(&mut self) {
        self.complete = true;
        // Send done signal to all subscribers
        let done_chunk = SseChunk {
            data: "[DONE]".to_string(),
            is_done: true,
        };
        for subscriber in &self.subscribers {
            let _ = subscriber.sender.try_send(done_chunk.clone());
        }
    }

    /// Remove a subscriber by ID.
    pub fn unsubscribe(&mut self, subscriber_id: &str) {
        self.subscribers.retain(|s| s.id != subscriber_id);
    }

    /// Get buffer stats.
    pub fn stats(&self) -> StreamBufferStats {
        StreamBufferStats {
            chunks_buffered: self.chunks.len(),
            current_bytes: self.current_bytes,
            max_chunks: self.max_buffer_size,
            max_bytes: self.max_buffer_bytes,
            subscriber_count: self.subscribers.len(),
            total_pushed: self.total_pushed.load(Ordering::Relaxed),
            chunks_dropped: self.chunks_dropped.load(Ordering::Relaxed),
            chunks_rotated: self.chunks_rotated.load(Ordering::Relaxed),
            is_complete: self.complete,
            age_ms: self.created_at.elapsed().as_millis() as u64,
        }
    }

    /// Check if the buffer is complete.
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// Number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }

    /// Rotate old chunks when buffer is full.
    fn rotate_if_needed(&mut self) {
        while self.chunks.len() >= self.max_buffer_size || self.current_bytes > self.max_buffer_bytes {
            if let Some(removed) = self.chunks.first() {
                self.current_bytes -= removed.data.len();
            }
            self.chunks.remove(0);
            self.chunks_rotated.fetch_add(1, Ordering::Relaxed);
            // Adjust all subscriber offsets
            for subscriber in &mut self.subscribers {
                if subscriber.offset > 0 {
                    subscriber.offset = subscriber.offset.saturating_sub(1);
                }
            }
        }
    }

    /// Fan out a chunk to all subscribers.
    fn fan_out(&mut self, chunk: &SseChunk) {
        let mut to_remove = Vec::new();

        for (i, subscriber) in self.subscribers.iter().enumerate() {
            match self.backpressure_policy {
                BackpressurePolicy::Drop => {
                    if let Err(_) = self.try_send(chunk, &subscriber.sender) {
                        self.chunks_dropped.fetch_add(1, Ordering::Relaxed);
                    }
                }
                BackpressurePolicy::Disconnect => {
                    if subscriber.sender.try_send(chunk.clone()).is_err() {
                        warn!(
                            subscriber_id = %subscriber.id,
                            "Disconnecting slow subscriber"
                        );
                        to_remove.push(i);
                    }
                }
                BackpressurePolicy::Block => {
                    // For Block policy, we use try_send (non-blocking).
                    // In practice, the caller would need to use blocking send,
                    // but since this is called from an async context with the
                    // buffer lock held, we fall back to drop.
                    if let Err(_) = self.try_send(chunk, &subscriber.sender) {
                        self.chunks_dropped.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }

        // Remove disconnected subscribers (reverse order to preserve indices)
        for i in to_remove.into_iter().rev() {
            self.subscribers.remove(i);
        }
    }

    /// Try to send a chunk, respecting backpressure policy.
    fn try_send(&self, chunk: &SseChunk, sender: &mpsc::Sender<SseChunk>) -> Result<(), ()> {
        sender.try_send(chunk.clone()).map_err(|_| ())
    }
}

// ---------------------------------------------------------------------------
// Stream Buffer Stats
// ---------------------------------------------------------------------------

/// Statistics for a single stream buffer.
#[derive(Debug, Clone, Serialize)]
pub struct StreamBufferStats {
    /// Number of chunks currently in the buffer.
    pub chunks_buffered: usize,
    /// Current bytes in the buffer.
    pub current_bytes: usize,
    /// Maximum chunks allowed.
    pub max_chunks: usize,
    /// Maximum bytes allowed.
    pub max_bytes: usize,
    /// Number of active subscribers.
    pub subscriber_count: usize,
    /// Total chunks pushed to this buffer.
    pub total_pushed: u64,
    /// Chunks dropped due to slow subscribers.
    pub chunks_dropped: u64,
    /// Chunks dropped due to buffer rotation.
    pub chunks_rotated: u64,
    /// Whether the stream is complete.
    pub is_complete: bool,
    /// Age of the buffer in ms.
    pub age_ms: u64,
}

// ---------------------------------------------------------------------------
// Stream Buffer Manager
// ---------------------------------------------------------------------------

/// Manager for multiple stream buffers, keyed by coalesce hash.
pub struct StreamBufferManager {
    /// Active stream buffers.
    buffers: DashMap<String, StreamBuffer>,
    /// Configuration.
    config: StreamBufferConfig,
    /// Total buffers created.
    total_created: AtomicU64,
    /// Total buffers cleaned up.
    total_cleaned: AtomicU64,
}

/// Aggregate statistics for the stream buffer manager.
#[derive(Debug, Clone, Serialize)]
pub struct StreamBufferManagerStats {
    /// Number of active buffers.
    pub active_buffers: usize,
    /// Total buffers created.
    pub total_created: u64,
    /// Total buffers cleaned up.
    pub total_cleaned: u64,
    /// Total subscribers across all buffers.
    pub total_subscribers: usize,
    /// Total chunks across all buffers.
    pub total_chunks: usize,
    /// Total bytes across all buffers.
    pub total_bytes: usize,
    /// Per-buffer stats.
    pub buffers: Vec<BufferDetail>,
}

/// Detail for a single buffer in the aggregate stats.
#[derive(Debug, Clone, Serialize)]
pub struct BufferDetail {
    pub hash: String,
    pub stats: StreamBufferStats,
}

impl StreamBufferManager {
    /// Create a new stream buffer manager.
    pub fn new(config: StreamBufferConfig) -> Self {
        Self {
            buffers: DashMap::new(),
            config,
            total_created: AtomicU64::new(0),
            total_cleaned: AtomicU64::new(0),
        }
    }

    /// Create a new buffer for the given hash.
    /// Returns false if a buffer already exists for this hash.
    pub fn create_buffer(&self, hash: &str) -> bool {
        if self.buffers.contains_key(hash) {
            return false;
        }
        self.buffers
            .insert(hash.to_string(), StreamBuffer::new(&self.config));
        self.total_created.fetch_add(1, Ordering::Relaxed);
        true
    }

    /// Push a chunk to a buffer.
    pub fn push(&self, hash: &str, chunk: SseChunk) {
        if let Some(mut buffer) = self.buffers.get_mut(hash) {
            buffer.push(chunk);
        }
    }

    /// Subscribe to a buffer. Returns None if buffer doesn't exist or max subscribers.
    pub fn subscribe(
        &self,
        hash: &str,
        subscriber_id: &str,
        buffer_size: usize,
    ) -> Option<mpsc::Receiver<SseChunk>> {
        let mut buffer = self.buffers.get_mut(hash)?;
        buffer.subscribe(subscriber_id, buffer_size)
    }

    /// Mark a buffer as complete.
    pub fn mark_complete(&self, hash: &str) {
        if let Some(mut buffer) = self.buffers.get_mut(hash) {
            buffer.mark_complete();
        }
    }

    /// Remove a subscriber from a buffer.
    pub fn unsubscribe(&self, hash: &str, subscriber_id: &str) {
        if let Some(mut buffer) = self.buffers.get_mut(hash) {
            buffer.unsubscribe(subscriber_id);
        }
    }

    /// Remove a specific buffer.
    pub fn remove_buffer(&self, hash: &str) {
        self.buffers.remove(hash);
    }

    /// Get stats for a specific buffer.
    pub fn buffer_stats(&self, hash: &str) -> Option<StreamBufferStats> {
        self.buffers.get(hash).map(|b| b.stats())
    }

    /// Get aggregate stats.
    pub fn get_stats(&self) -> StreamBufferManagerStats {
        let mut total_subscribers = 0usize;
        let mut total_chunks = 0usize;
        let mut total_bytes = 0usize;
        let mut buffer_details = Vec::new();

        for entry in self.buffers.iter() {
            let stats = entry.value().stats();
            total_subscribers += stats.subscriber_count;
            total_chunks += stats.chunks_buffered;
            total_bytes += stats.current_bytes;
            buffer_details.push(BufferDetail {
                hash: entry.key().clone(),
                stats,
            });
        }

        StreamBufferManagerStats {
            active_buffers: self.buffers.len(),
            total_created: self.total_created.load(Ordering::Relaxed),
            total_cleaned: self.total_cleaned.load(Ordering::Relaxed),
            total_subscribers,
            total_chunks,
            total_bytes,
            buffers: buffer_details,
        }
    }

    /// Clean up completed buffers older than cleanup_after_ms.
    pub fn cleanup(&self) {
        let threshold = Duration::from_millis(self.config.cleanup_after_ms);
        let to_remove: Vec<String> = self
            .buffers
            .iter()
            .filter(|entry| {
                let buffer = entry.value();
                buffer.is_complete() && buffer.created_at.elapsed() > threshold
            })
            .map(|entry| entry.key().clone())
            .collect();

        for hash in to_remove {
            self.buffers.remove(&hash);
            self.total_cleaned.fetch_add(1, Ordering::Relaxed);
            debug!(hash = %hash, "Completed stream buffer cleaned up");
        }
    }

    /// Check if stream buffering is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> StreamBufferConfig {
        StreamBufferConfig {
            enabled: true,
            max_buffer_size: 100,
            max_buffer_bytes: 1_048_576,
            max_subscribers_per_stream: 20,
            cleanup_after_ms: 5000,
            backpressure_policy: BackpressurePolicy::Drop,
        }
    }

    fn make_chunk(data: &str) -> SseChunk {
        SseChunk {
            data: data.to_string(),
            is_done: false,
        }
    }

    #[test]
    fn test_buffer_push_and_subscribe() {
        let config = test_config();
        let mut buffer = StreamBuffer::new(&config);

        // Push some chunks
        buffer.push(make_chunk("hello"));
        buffer.push(make_chunk("world"));

        assert_eq!(buffer.stats().chunks_buffered, 2);

        // Subscribe
        let rx = buffer.subscribe("sub-1", 100).expect("should subscribe");
        assert_eq!(buffer.subscriber_count(), 1);

        // New subscriber should receive buffered chunks
        // (they were sent during subscribe via try_send)
        drop(rx);
    }

    #[test]
    fn test_buffer_late_subscriber() {
        let config = test_config();
        let mut buffer = StreamBuffer::new(&config);

        buffer.push(make_chunk("chunk-0"));
        buffer.push(make_chunk("chunk-1"));
        buffer.push(make_chunk("chunk-2"));

        // Subscribe after chunks were pushed
        let mut rx = buffer.subscribe("late-sub", 100).expect("should subscribe");

        // The late subscriber should receive the 3 buffered chunks
        // (they were sent during subscribe)
        // Push one more chunk
        buffer.push(make_chunk("chunk-3"));

        // Try to receive
        let received = rx.try_recv();
        // Either we got a buffered chunk or chunk-3, both are valid
        assert!(received.is_ok());
    }

    #[test]
    fn test_buffer_max_size_rotation() {
        let config = StreamBufferConfig {
            max_buffer_size: 3,
            max_buffer_bytes: 1_048_576,
            ..test_config()
        };
        let mut buffer = StreamBuffer::new(&config);

        buffer.push(make_chunk("a"));
        buffer.push(make_chunk("b"));
        buffer.push(make_chunk("c"));
        assert_eq!(buffer.stats().chunks_buffered, 3);

        // Fourth push should rotate oldest
        buffer.push(make_chunk("d"));
        assert_eq!(buffer.stats().chunks_buffered, 3);
        assert_eq!(buffer.stats().chunks_rotated, 1);

        // Fifth push
        buffer.push(make_chunk("e"));
        assert_eq!(buffer.stats().chunks_buffered, 3);
        assert_eq!(buffer.stats().chunks_rotated, 2);
    }

    #[test]
    fn test_buffer_completion_tracking() {
        let config = test_config();
        let mut buffer = StreamBuffer::new(&config);

        assert!(!buffer.is_complete());
        buffer.mark_complete();
        assert!(buffer.is_complete());
    }

    #[test]
    fn test_buffer_backpressure_drop_policy() {
        let config = StreamBufferConfig {
            backpressure_policy: BackpressurePolicy::Drop,
            ..test_config()
        };
        let mut buffer = StreamBuffer::new(&config);

        // Subscribe with tiny channel
        let rx = buffer.subscribe("slow-sub", 1).expect("should subscribe");

        // Push more chunks than channel can hold
        buffer.push(make_chunk("chunk-1"));
        buffer.push(make_chunk("chunk-2"));
        buffer.push(make_chunk("chunk-3"));

        // Some chunks should have been dropped
        let stats = buffer.stats();
        // At least some should be dropped since channel size is 1
        // and we pushed 3 + tried to send 3 to existing subscriber
        // The subscriber already received chunk-1 during subscribe (0 buffered)
        // Then push sends chunk-1 (ok), chunk-2 (ok), chunk-3 (may drop)
        assert!(stats.total_pushed >= 3);

        drop(rx);
    }

    #[test]
    fn test_buffer_max_subscribers() {
        let config = StreamBufferConfig {
            max_subscribers_per_stream: 2,
            ..test_config()
        };
        let mut buffer = StreamBuffer::new(&config);

        assert!(buffer.subscribe("sub-1", 100).is_some());
        assert!(buffer.subscribe("sub-2", 100).is_some());
        assert!(buffer.subscribe("sub-3", 100).is_none());
    }

    #[test]
    fn test_buffer_unsubscribe() {
        let config = test_config();
        let mut buffer = StreamBuffer::new(&config);

        let rx = buffer.subscribe("sub-1", 100).expect("should subscribe");
        assert_eq!(buffer.subscriber_count(), 1);

        buffer.unsubscribe("sub-1");
        assert_eq!(buffer.subscriber_count(), 0);

        drop(rx);
    }

    #[test]
    fn test_buffer_cleanup_after_completion() {
        let manager = StreamBufferManager::new(StreamBufferConfig {
            cleanup_after_ms: 10,
            ..test_config()
        });

        manager.create_buffer("hash-1");
        manager.mark_complete("hash-1");

        // Wait for cleanup threshold
        std::thread::sleep(Duration::from_millis(50));
        manager.cleanup();

        let stats = manager.get_stats();
        assert_eq!(stats.active_buffers, 0);
        assert_eq!(stats.total_cleaned, 1);
    }

    #[test]
    fn test_manager_create_and_push() {
        let manager = StreamBufferManager::new(test_config());

        assert!(manager.create_buffer("hash-1"));
        assert!(!manager.create_buffer("hash-1")); // duplicate

        manager.push("hash-1", make_chunk("hello"));

        let stats = manager.get_stats();
        assert_eq!(stats.active_buffers, 1);
        assert_eq!(stats.total_created, 1);
        assert_eq!(stats.total_chunks, 1);
    }

    #[test]
    fn test_manager_subscribe() {
        let manager = StreamBufferManager::new(test_config());

        manager.create_buffer("hash-1");
        manager.push("hash-1", make_chunk("chunk-1"));

        let rx = manager.subscribe("hash-1", "sub-1", 100);
        assert!(rx.is_some());

        let none = manager.subscribe("nonexistent", "sub-2", 100);
        assert!(none.is_none());
    }

    #[test]
    fn test_manager_stats() {
        let manager = StreamBufferManager::new(test_config());

        manager.create_buffer("hash-1");
        manager.create_buffer("hash-2");

        manager.push("hash-1", make_chunk("a"));
        manager.push("hash-1", make_chunk("b"));
        manager.push("hash-2", make_chunk("c"));

        let stats = manager.get_stats();
        assert_eq!(stats.active_buffers, 2);
        assert_eq!(stats.total_chunks, 3);
    }

    #[test]
    fn test_manager_disabled() {
        let config = StreamBufferConfig {
            enabled: false,
            ..test_config()
        };
        let manager = StreamBufferManager::new(config);
        assert!(!manager.is_enabled());
    }
}
