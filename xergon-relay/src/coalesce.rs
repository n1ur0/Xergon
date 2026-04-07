//! Request Coalescing for chat completions.
//!
//! When multiple identical chat requests arrive within a short time window,
//! they are batched into a single provider request. The response is then
//! fanned out to all waiters via a shared stream buffer.
//!
//! Uses SHA-256 hashing of (model + system_prompt + normalized_user_prompt)
//! for request matching.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for request coalescing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoalesceConfig {
    /// Enable/disable request coalescing (default: true).
    #[serde(default = "default_coalesce_enabled")]
    pub enabled: bool,
    /// Maximum time to wait for more requests in a batch (ms, default: 50).
    #[serde(default = "default_max_wait_ms")]
    pub max_wait_ms: u64,
    /// Maximum number of requests per batch (default: 10).
    #[serde(default = "default_max_batch_size")]
    pub max_batch_size: usize,
    /// Similarity threshold for prompt matching (default: 0.0 = exact match only).
    #[serde(default = "default_similarity_threshold")]
    pub similarity_threshold: f64,
    /// Max prompt length to consider for coalescing (default: 10000).
    #[serde(default = "default_max_prompt_length")]
    pub max_prompt_length: usize,
}

impl Default for CoalesceConfig {
    fn default() -> Self {
        Self {
            enabled: default_coalesce_enabled(),
            max_wait_ms: default_max_wait_ms(),
            max_batch_size: default_max_batch_size(),
            similarity_threshold: default_similarity_threshold(),
            max_prompt_length: default_max_prompt_length(),
        }
    }
}

fn default_coalesce_enabled() -> bool {
    true
}
fn default_max_wait_ms() -> u64 {
    50
}
fn default_max_batch_size() -> usize {
    10
}
fn default_similarity_threshold() -> f64 {
    0.0
}
fn default_max_prompt_length() -> usize {
    10_000
}

// ---------------------------------------------------------------------------
// Coalesce Status
// ---------------------------------------------------------------------------

/// Status of a coalesced request batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoalesceStatus {
    /// Waiting for more requests to join the batch.
    Collecting,
    /// Sent to provider, waiting for response.
    Dispatched,
    /// Receiving response stream from provider.
    Streaming,
    /// Response complete.
    Completed,
    /// Batch was cancelled.
    Cancelled,
}

// ---------------------------------------------------------------------------
// Coalesced Entry
// ---------------------------------------------------------------------------

/// A single entry in a coalesced batch.
#[derive(Clone)]
pub struct CoalescedEntry {
    /// Unique ID for this entry.
    pub id: String,
    /// Sender for streaming SSE chunks to this waiter.
    pub sender: mpsc::Sender<SseChunk>,
    /// Model name.
    pub model: String,
    /// Hash of the prompt for matching.
    pub prompt_hash: String,
    /// When this entry was enqueued.
    pub enqueued_at: Instant,
}

/// An SSE chunk for streaming.
#[derive(Debug, Clone)]
pub struct SseChunk {
    /// The raw data line (without "data: " prefix).
    pub data: String,
    /// Whether this is the final chunk ([DONE]).
    pub is_done: bool,
}

// ---------------------------------------------------------------------------
// Coalesced Request
// ---------------------------------------------------------------------------

/// A batch of coalesced requests waiting to be dispatched.
struct CoalescedRequest {
    /// Hash key for this batch.
    request_hash: String,
    /// When this batch was created.
    created_at: Instant,
    /// Maximum wait time for collection (ms).
    max_wait_ms: u64,
    /// Maximum batch size.
    max_batch_size: usize,
    /// Individual request entries.
    entries: Vec<CoalescedEntry>,
    /// Current status.
    status: CoalesceStatus,
}

// ---------------------------------------------------------------------------
// Coalesce Stats
// ---------------------------------------------------------------------------

/// Statistics for the request coalescer.
#[derive(Debug, Serialize)]
pub struct CoalesceStats {
    /// Number of currently active (collecting/streaming) batches.
    pub active_batches: usize,
    /// Total number of requests that were coalesced (joined existing batch).
    pub total_coalesced: AtomicU64,
    /// Total number of batches created.
    pub total_batches: AtomicU64,
    /// Total number of batches completed.
    pub total_completed: AtomicU64,
    /// Total number of batches cancelled.
    pub total_cancelled: AtomicU64,
    /// Sum of batch sizes (for computing average).
    pub total_entries: AtomicU64,
}

impl CoalesceStats {
    fn new() -> Self {
        Self {
            active_batches: 0,
            total_coalesced: AtomicU64::new(0),
            total_batches: AtomicU64::new(0),
            total_completed: AtomicU64::new(0),
            total_cancelled: AtomicU64::new(0),
            total_entries: AtomicU64::new(0),
        }
    }

    /// Get the average batch size.
    pub fn avg_batch_size(&self) -> f64 {
        let batches = self.total_completed.load(Ordering::Relaxed);
        if batches == 0 {
            0.0
        } else {
            let entries = self.total_entries.load(Ordering::Relaxed);
            entries as f64 / batches as f64
        }
    }
}

// ---------------------------------------------------------------------------
// RequestCoalescer
// ---------------------------------------------------------------------------

/// Thread-safe request coalescer.
pub struct RequestCoalescer {
    /// Pending batches keyed by request hash.
    pending: DashMap<String, CoalescedRequest>,
    /// Configuration.
    config: CoalesceConfig,
    /// Statistics.
    stats: CoalesceStats,
}

/// Result of trying to coalesce a request.
pub enum CoalesceResult {
    /// This request is the first in a new batch.
    /// The caller should start a collection timer and dispatch after it fires.
    NewBatch {
        /// The hash key for this batch.
        hash: String,
        /// The entry to track.
        entry: CoalescedEntry,
        /// Sender for the representative request's response stream.
        /// The coalescer will read from this and fan out to all entries.
        response_receiver: mpsc::Receiver<SseChunk>,
    },
    /// This request was added to an existing batch.
    /// The caller should just return the response stream from the entry's sender.
    Joined {
        /// The entry (already registered in the batch).
        entry: CoalescedEntry,
    },
    /// Coalescing is disabled — proceed normally.
    Disabled,
}

impl RequestCoalescer {
    /// Create a new request coalescer.
    pub fn new(config: CoalesceConfig) -> Self {
        Self {
            pending: DashMap::new(),
            config,
            stats: CoalesceStats::new(),
        }
    }

    /// Generate a coalesce hash from model and messages.
    /// Hash = SHA-256 of (model + system_prompt + normalized_user_prompt).
    pub fn hash_request(model: &str, messages: &[serde_json::Value]) -> String {
        let mut hasher = Sha256::new();

        hasher.update(model.as_bytes());

        // Extract system prompt
        for msg in messages {
            if let Some(role) = msg.get("role").and_then(|v| v.as_str()) {
                if role == "system" {
                    if let Some(content) = msg.get("content").and_then(|v| v.as_str()) {
                        hasher.update(b"|sys|");
                        hasher.update(content.trim().as_bytes());
                    }
                }
            }
        }

        // Extract and normalize user prompts (last user message is most relevant)
        let mut user_prompts: Vec<String> = Vec::new();
        for msg in messages {
            if let Some(role) = msg.get("role").and_then(|v| v.as_str()) {
                if role == "user" {
                    if let Some(content) = msg.get("content").and_then(|v| v.as_str()) {
                        // Normalize: trim whitespace, lowercase first N chars
                        let normalized = content.trim().to_lowercase();
                        user_prompts.push(normalized);
                    }
                }
            }
        }
        hasher.update(b"|usr|");
        for prompt in &user_prompts {
            hasher.update(prompt.as_bytes());
            hasher.update(b"|");
        }

        let hash = hasher.finalize();
        hex::encode(&hash[..16])
    }

    /// Try to coalesce a request.
    ///
    /// If an existing collecting batch matches, adds the entry and returns `Joined`.
    /// Otherwise, creates a new batch and returns `NewBatch`.
    /// The caller is responsible for starting the dispatch timer for new batches.
    pub fn try_coalesce(
        &self,
        model: &str,
        messages: &[serde_json::Value],
        request_id: &str,
        buffer_size: usize,
    ) -> CoalesceResult {
        if !self.config.enabled {
            return CoalesceResult::Disabled;
        }

        // Check prompt length
        let prompt_len: usize = messages
            .iter()
            .filter_map(|m| m.get("content").and_then(|v| v.as_str()).map(|s| s.len()))
            .sum();
        if prompt_len > self.config.max_prompt_length {
            debug!(
                request_id,
                prompt_len,
                max = self.config.max_prompt_length,
                "Prompt too long for coalescing"
            );
            return CoalesceResult::Disabled;
        }

        let hash = Self::hash_request(model, messages);
        let prompt_hash = hash.clone();

        // Try to join an existing batch
        if let Some(mut existing) = self.pending.get_mut(&hash) {
            if (existing.status == CoalesceStatus::Collecting
                || existing.status == CoalesceStatus::Streaming)
                && existing.entries.len() < existing.max_batch_size
            {
                let (tx, _rx) = mpsc::channel(buffer_size);
                let entry = CoalescedEntry {
                    id: request_id.to_string(),
                    sender: tx,
                    model: model.to_string(),
                    prompt_hash: prompt_hash.clone(),
                    enqueued_at: Instant::now(),
                };
                existing.entries.push(entry);
                self.stats.total_coalesced.fetch_add(1, Ordering::Relaxed);
                self.stats.total_entries.fetch_add(1, Ordering::Relaxed);
                debug!(
                    hash = %hash,
                    batch_size = existing.entries.len(),
                    request_id,
                    "Request joined existing coalesce batch"
                );
                return CoalesceResult::Joined {
                    entry: existing.entries.last().unwrap().clone(),
                };
            }
        }

        // Create new batch
        let (response_tx, response_rx) = mpsc::channel(buffer_size);
        let entry = CoalescedEntry {
            id: request_id.to_string(),
            sender: response_tx.clone(),
            model: model.to_string(),
            prompt_hash: prompt_hash.clone(),
            enqueued_at: Instant::now(),
        };

        let batch = CoalescedRequest {
            request_hash: hash.clone(),
            created_at: Instant::now(),
            max_wait_ms: self.config.max_wait_ms,
            max_batch_size: self.config.max_batch_size,
            entries: vec![entry.clone()],
            status: CoalesceStatus::Collecting,
        };

        self.pending.insert(hash.clone(), batch);
        self.stats.total_batches.fetch_add(1, Ordering::Relaxed);
        self.stats.total_entries.fetch_add(1, Ordering::Relaxed);

        debug!(
            hash = %hash,
            request_id,
            "New coalesce batch created"
        );

        CoalesceResult::NewBatch {
            hash,
            entry,
            response_receiver: response_rx,
        }
    }

    /// Mark a batch as dispatched (sent to provider).
    pub fn mark_dispatched(&self, hash: &str) {
        if let Some(mut batch) = self.pending.get_mut(hash) {
            batch.status = CoalesceStatus::Dispatched;
        }
    }

    /// Mark a batch as streaming (receiving response).
    pub fn mark_streaming(&self, hash: &str) {
        if let Some(mut batch) = self.pending.get_mut(hash) {
            batch.status = CoalesceStatus::Streaming;
        }
    }

    /// Mark a batch as completed and remove it.
    pub fn mark_completed(&self, hash: &str) {
        if let Some(mut batch) = self.pending.get_mut(hash) {
            batch.status = CoalesceStatus::Completed;
            let entry_count = batch.entries.len();
            drop(batch);
            self.pending.remove(hash);
            self.stats.total_completed.fetch_add(1, Ordering::Relaxed);
            self.stats.total_entries.fetch_add(entry_count as u64, Ordering::Relaxed);
            debug!(hash = %hash, "Coalesce batch completed");
        }
    }

    /// Cancel a batch (all entries).
    pub fn cancel_batch(&self, hash: &str) -> bool {
        if let Some(mut batch) = self.pending.get_mut(hash) {
            batch.status = CoalesceStatus::Cancelled;
            let count = batch.entries.len();
            drop(batch);
            self.pending.remove(hash);
            self.stats.total_cancelled.fetch_add(1, Ordering::Relaxed);
            debug!(hash = %hash, entries = count, "Coalesce batch cancelled");
            true
        } else {
            false
        }
    }

    /// Cancel a single entry in a batch.
    /// Returns true if the entry was found and removed.
    pub fn cancel_entry(&self, hash: &str, entry_id: &str) -> bool {
        if let Some(mut batch) = self.pending.get_mut(hash) {
            if let Some(pos) = batch.entries.iter().position(|e| e.id == entry_id) {
                batch.entries.remove(pos);
                debug!(
                    hash = %hash,
                    entry_id,
                    remaining = batch.entries.len(),
                    "Entry cancelled from coalesce batch"
                );
                // If no entries left, cancel the batch
                if batch.entries.is_empty() {
                    drop(batch);
                    self.cancel_batch(hash);
                }
                return true;
            }
        }
        false
    }

    /// Get all entries in a batch for dispatching.
    /// Returns None if the batch doesn't exist or is not in Collecting state.
    pub fn get_entries(&self, hash: &str) -> Option<Vec<CoalescedEntry>> {
        let batch = self.pending.get(hash)?;
        if batch.status != CoalesceStatus::Collecting {
            return None;
        }
        Some(batch.entries.clone())
    }

    /// Get the number of entries in a batch.
    pub fn batch_size(&self, hash: &str) -> usize {
        self.pending
            .get(hash)
            .map(|b| b.entries.len())
            .unwrap_or(0)
    }

    /// Remove expired/stale batches (collection timeout exceeded).
    pub fn cleanup(&self) {
        let now = Instant::now();
        let expired: Vec<String> = self
            .pending
            .iter()
            .filter(|entry| {
                let batch = entry.value();
                batch.status == CoalesceStatus::Collecting
                    && now.duration_since(batch.created_at).as_millis()
                        > (batch.max_wait_ms as u128) * 3
            })
            .map(|entry| entry.key().clone())
            .collect();

        for hash in expired {
            warn!(hash = %hash, "Stale coalesce batch cleaned up");
            self.cancel_batch(&hash);
        }
    }

    /// Get current statistics.
    pub fn get_stats(&self) -> CoalesceStats {
        let mut stats = CoalesceStats {
            active_batches: self.pending.len(),
            total_coalesced: AtomicU64::new(
                self.stats.total_coalesced.load(Ordering::Relaxed),
            ),
            total_batches: AtomicU64::new(self.stats.total_batches.load(Ordering::Relaxed)),
            total_completed: AtomicU64::new(
                self.stats.total_completed.load(Ordering::Relaxed),
            ),
            total_cancelled: AtomicU64::new(
                self.stats.total_cancelled.load(Ordering::Relaxed),
            ),
            total_entries: AtomicU64::new(self.stats.total_entries.load(Ordering::Relaxed)),
        };
        stats
    }

    /// Check if coalescing is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the configured max wait time in ms.
    pub fn max_wait_ms(&self) -> u64 {
        self.config.max_wait_ms
    }

    /// Get the configured max batch size.
    pub fn max_batch_size(&self) -> usize {
        self.config.max_batch_size
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> CoalesceConfig {
        CoalesceConfig {
            enabled: true,
            max_wait_ms: 50,
            max_batch_size: 10,
            similarity_threshold: 0.0,
            max_prompt_length: 10_000,
        }
    }

    fn make_messages(content: &str) -> Vec<serde_json::Value> {
        vec![serde_json::json!({
            "role": "user",
            "content": content
        })]
    }

    fn make_messages_with_system(system: &str, user: &str) -> Vec<serde_json::Value> {
        vec![
            serde_json::json!({"role": "system", "content": system}),
            serde_json::json!({"role": "user", "content": user}),
        ]
    }

    #[test]
    fn test_request_hash_same_params_same_hash() {
        let msgs = make_messages("hello world");
        let h1 = RequestCoalescer::hash_request("gpt-4", &msgs);
        let h2 = RequestCoalescer::hash_request("gpt-4", &msgs);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_request_hash_different_model() {
        let msgs = make_messages("hello world");
        let h1 = RequestCoalescer::hash_request("gpt-4", &msgs);
        let h2 = RequestCoalescer::hash_request("gpt-3.5", &msgs);
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_request_hash_normalization() {
        // Trimming and lowercasing should produce the same hash
        let msgs1 = make_messages("  Hello World  ");
        let msgs2 = make_messages("hello world");
        let h1 = RequestCoalescer::hash_request("gpt-4", &msgs1);
        let h2 = RequestCoalescer::hash_request("gpt-4", &msgs2);
        assert_eq!(h1, h2, "Normalized prompts should have same hash");
    }

    #[test]
    fn test_request_hash_different_system_prompt() {
        let msgs1 = make_messages_with_system("You are helpful", "hello");
        let msgs2 = make_messages_with_system("You are terse", "hello");
        let h1 = RequestCoalescer::hash_request("gpt-4", &msgs1);
        let h2 = RequestCoalescer::hash_request("gpt-4", &msgs2);
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_request_hash_same_system_same_user() {
        let msgs1 = make_messages_with_system("You are helpful", "hello");
        let msgs2 = make_messages_with_system("You are helpful", "hello");
        let h1 = RequestCoalescer::hash_request("gpt-4", &msgs1);
        let h2 = RequestCoalescer::hash_request("gpt-4", &msgs2);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_coalescer_creates_new_batch() {
        let coalescer = RequestCoalescer::new(test_config());
        let msgs = make_messages("hello");

        match coalescer.try_coalesce("gpt-4", &msgs, "req-1", 100) {
            CoalesceResult::NewBatch { hash, entry, .. } => {
                assert_eq!(entry.id, "req-1");
                assert!(!hash.is_empty());
            }
            CoalesceResult::Joined { .. } => panic!("Expected NewBatch"),
            CoalesceResult::Disabled => panic!("Expected NewBatch, got Disabled"),
        }
    }

    #[test]
    fn test_coalescer_adds_to_existing_batch() {
        let coalescer = RequestCoalescer::new(test_config());
        let msgs = make_messages("hello");

        // First request creates batch
        let hash = match coalescer.try_coalesce("gpt-4", &msgs, "req-1", 100) {
            CoalesceResult::NewBatch { hash, .. } => hash,
            _ => panic!("Expected NewBatch"),
        };

        // Second request joins batch
        match coalescer.try_coalesce("gpt-4", &msgs, "req-2", 100) {
            CoalesceResult::Joined { entry } => {
                assert_eq!(entry.id, "req-2");
            }
            _ => panic!("Expected Joined"),
        }

        assert_eq!(coalescer.batch_size(&hash), 2);
    }

    #[test]
    fn test_coalescer_max_batch_size() {
        let config = CoalesceConfig {
            max_batch_size: 2,
            ..test_config()
        };
        let coalescer = RequestCoalescer::new(config);
        let msgs = make_messages("hello");

        // First request
        coalescer.try_coalesce("gpt-4", &msgs, "req-1", 100);
        // Second request joins
        coalescer.try_coalesce("gpt-4", &msgs, "req-2", 100);
        // Third request should create new batch (max reached)
        match coalescer.try_coalesce("gpt-4", &msgs, "req-3", 100) {
            CoalesceResult::NewBatch { .. } => {}
            CoalesceResult::Joined { .. } => panic!("Expected NewBatch when max reached"),
            CoalesceResult::Disabled => panic!("Expected NewBatch"),
        }
    }

    #[test]
    fn test_coalescer_disabled() {
        let config = CoalesceConfig {
            enabled: false,
            ..test_config()
        };
        let coalescer = RequestCoalescer::new(config);
        let msgs = make_messages("hello");

        match coalescer.try_coalesce("gpt-4", &msgs, "req-1", 100) {
            CoalesceResult::Disabled => {}
            _ => panic!("Expected Disabled"),
        }
    }

    #[test]
    fn test_coalescer_prompt_too_long() {
        let config = CoalesceConfig {
            max_prompt_length: 10,
            ..test_config()
        };
        let coalescer = RequestCoalescer::new(config);
        let msgs = make_messages("this is a very long prompt that exceeds the limit");

        match coalescer.try_coalesce("gpt-4", &msgs, "req-1", 100) {
            CoalesceResult::Disabled => {}
            _ => panic!("Expected Disabled for long prompt"),
        }
    }

    #[test]
    fn test_coalescer_stats() {
        let coalescer = RequestCoalescer::new(test_config());
        let msgs = make_messages("hello");

        coalescer.try_coalesce("gpt-4", &msgs, "req-1", 100);
        coalescer.try_coalesce("gpt-4", &msgs, "req-2", 100);

        let stats = coalescer.get_stats();
        assert_eq!(stats.active_batches, 1);
        assert_eq!(stats.total_coalesced.load(Ordering::Relaxed), 1);
        assert_eq!(stats.total_batches.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_coalescer_cancel_entry() {
        let coalescer = RequestCoalescer::new(test_config());
        let msgs = make_messages("hello");

        let hash = match coalescer.try_coalesce("gpt-4", &msgs, "req-1", 100) {
            CoalesceResult::NewBatch { hash, .. } => hash,
            _ => panic!("Expected NewBatch"),
        };

        coalescer.try_coalesce("gpt-4", &msgs, "req-2", 100);
        assert_eq!(coalescer.batch_size(&hash), 2);

        // Cancel one entry
        assert!(coalescer.cancel_entry(&hash, "req-1"));
        assert_eq!(coalescer.batch_size(&hash), 1);

        // Cancel the last entry — should remove batch
        assert!(coalescer.cancel_entry(&hash, "req-2"));
        assert_eq!(coalescer.batch_size(&hash), 0);
    }

    #[test]
    fn test_coalescer_cancel_batch() {
        let coalescer = RequestCoalescer::new(test_config());
        let msgs = make_messages("hello");

        let hash = match coalescer.try_coalesce("gpt-4", &msgs, "req-1", 100) {
            CoalesceResult::NewBatch { hash, .. } => hash,
            _ => panic!("Expected NewBatch"),
        };

        assert!(coalescer.cancel_batch(&hash));
        assert_eq!(coalescer.batch_size(&hash), 0);

        let stats = coalescer.get_stats();
        assert_eq!(stats.total_cancelled.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_coalescer_mark_dispatched_and_streaming() {
        let coalescer = RequestCoalescer::new(test_config());
        let msgs = make_messages("hello");

        let hash = match coalescer.try_coalesce("gpt-4", &msgs, "req-1", 100) {
            CoalesceResult::NewBatch { hash, .. } => hash,
            _ => panic!("Expected NewBatch"),
        };

        coalescer.mark_dispatched(&hash);
        // A second request with same params should create a new batch
        // (existing is no longer Collecting)
        match coalescer.try_coalesce("gpt-4", &msgs, "req-2", 100) {
            CoalesceResult::NewBatch { .. } => {}
            CoalesceResult::Joined { .. } => panic!("Should not join dispatched batch"),
            CoalesceResult::Disabled => panic!("Expected NewBatch"),
        }
    }

    #[test]
    fn test_coalescer_mark_completed() {
        let coalescer = RequestCoalescer::new(test_config());
        let msgs = make_messages("hello");

        let hash = match coalescer.try_coalesce("gpt-4", &msgs, "req-1", 100) {
            CoalesceResult::NewBatch { hash, .. } => hash,
            _ => panic!("Expected NewBatch"),
        };

        coalescer.mark_completed(&hash);
        assert_eq!(coalescer.batch_size(&hash), 0);

        let stats = coalescer.get_stats();
        assert_eq!(stats.total_completed.load(Ordering::Relaxed), 1);
    }
}
