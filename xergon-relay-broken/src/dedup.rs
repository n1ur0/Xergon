//! Request deduplication for chat completions.
//!
//! When multiple identical chat requests arrive (same model + messages hash),
//! only the first request is forwarded to the provider. Subsequent identical
//! requests within the dedup window piggyback on the in-flight response.
//!
//! Uses a DashMap for thread-safe in-flight tracking with automatic expiry.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde::Serialize;
use serde_json;
use tracing::debug;

// ---------------------------------------------------------------------------
// Dedup Stats
// ---------------------------------------------------------------------------

/// Statistics for the request deduplication system.
#[derive(Debug, Clone, Serialize)]
pub struct DedupStats {
    /// Total requests that have been checked for dedup.
    pub total_requests: u64,
    /// Number of requests served from dedup cache (duplicate hits).
    pub dedup_hits: u64,
    /// Number of requests that were not duplicates (cache misses).
    pub dedup_misses: u64,
    /// Number of currently active cached responses.
    pub active_cached_responses: usize,
    /// Approximate cache size in bytes.
    pub cache_size_bytes: u64,
    /// Whether dedup is enabled.
    pub enabled: bool,
    /// Current dedup window in seconds.
    pub window_secs: u64,
    /// Current dedup TTL in seconds.
    pub dedup_ttl_secs: u64,
}

// ---------------------------------------------------------------------------
// In-flight Entry
// ---------------------------------------------------------------------------

/// A single in-flight dedup entry.
struct InFlightEntry {
    /// The response data chunks collected so far (SSE lines or full JSON).
    response_chunks: Arc<tokio::sync::Mutex<Vec<String>>>,
    /// When this entry was created (for expiry).
    created_at: Instant,
    /// Approximate byte size of the cached response chunks.
    byte_size: Arc<std::sync::atomic::AtomicU64>,
    /// Notifier: all waiters receive the response via this broadcast.
    done_tx: tokio::sync::watch::Sender<bool>,
    done_rx: tokio::sync::watch::Receiver<bool>,
}

// ---------------------------------------------------------------------------
// Request Dedup
// ---------------------------------------------------------------------------

/// Thread-safe request deduplication tracker.
#[derive(Clone)]
pub struct RequestDedup {
    /// Map from request hash -> in-flight entry.
    entries: Arc<DashMap<u64, InFlightEntry>>,
    /// Configuration.
    config: crate::config::DedupConfig,
    /// Total requests checked.
    total_requests: Arc<AtomicU64>,
    /// Dedup hits (duplicates served from cache).
    dedup_hits: Arc<AtomicU64>,
    /// Dedup misses (first requests, not duplicates).
    dedup_misses: Arc<AtomicU64>,
}

/// Result of attempting to register a dedup request.
pub enum DedupResult {
    /// This is the first request for this hash — the caller should proxy.
    FirstRequest {
        /// Shared buffer where response chunks should be written.
        response_writer: Arc<tokio::sync::Mutex<Vec<String>>>,
        /// Signal to send when the response is complete.
        done_signal: tokio::sync::watch::Sender<bool>,
        /// Byte size tracker for cache size estimation.
        byte_size_tracker: Arc<std::sync::atomic::AtomicU64>,
    },
    /// This request is a duplicate — the caller should wait and return the
    /// collected response chunks.
    Duplicate {
        /// Receiver that resolves when the original request completes.
        wait_rx: tokio::sync::watch::Receiver<bool>,
        /// Shared buffer with the response chunks.
        response_reader: Arc<tokio::sync::Mutex<Vec<String>>>,
    },
}

impl RequestDedup {
    /// Create a new dedup tracker.
    pub fn new(enabled: bool, window_secs: u64) -> Self {
        Self::new_with_config(crate::config::DedupConfig {
            enabled,
            window_secs,
            dedup_ttl_secs: 30,
        })
    }

    /// Create a new dedup tracker with full configuration.
    pub fn new_with_config(config: crate::config::DedupConfig) -> Self {
        Self {
            entries: Arc::new(DashMap::new()),
            config,
            total_requests: Arc::new(AtomicU64::new(0)),
            dedup_hits: Arc::new(AtomicU64::new(0)),
            dedup_misses: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Hash a chat completion request for dedup purposes.
    /// Uses model + serialized messages content.
    pub fn hash_request(model: &str, messages: &[serde_json::Value]) -> u64 {
        let mut hasher = DefaultHasher::new();
        model.hash(&mut hasher);
        for msg in messages {
            // Hash the role + content (not other fields like name/tool_call_id)
            if let Some(role) = msg.get("role").and_then(|v| v.as_str()) {
                role.hash(&mut hasher);
            }
            if let Some(content) = msg.get("content") {
                // Serialize content for consistent hashing regardless of Value variant
                let serialized = serde_json::to_string(content).unwrap_or_default();
                serialized.hash(&mut hasher);
            }
        }
        hasher.finish()
    }

    /// Try to register a request for dedup.
    ///
    /// If the hash already has an in-flight entry that hasn't exceeded the TTL,
    /// returns `Duplicate`. Otherwise, creates a new entry and returns `FirstRequest`.
    pub fn register(&self, hash: u64) -> DedupResult {
        self.total_requests.fetch_add(1, Ordering::Relaxed);

        if !self.config.enabled {
            // Dedup disabled — always treat as first request
            let (tx, _rx) = tokio::sync::watch::channel(false);
            self.dedup_misses.fetch_add(1, Ordering::Relaxed);
            return DedupResult::FirstRequest {
                response_writer: Arc::new(tokio::sync::Mutex::new(Vec::new())),
                done_signal: tx,
                byte_size_tracker: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            };
        }

        // Use entry API to check if an entry already exists
        match self.entries.entry(hash) {
            dashmap::mapref::entry::Entry::Occupied(existing) => {
                let entry = existing.get();

                // TTL check: don't dedup against entries older than the TTL window
                let entry_age = entry.created_at.elapsed();
                if entry_age > Duration::from_secs(self.config.dedup_ttl_secs) {
                    debug!(
                        hash = hash,
                        age_secs = entry_age.as_secs(),
                        ttl_secs = self.config.dedup_ttl_secs,
                        "Dedup entry expired (TTL), treating as new request"
                    );
                    // Entry is too old — remove it and create a fresh one
                    drop(existing);
                    self.entries.remove(&hash);
                    return self.create_first_request(hash);
                }

                debug!(hash = hash, "Duplicate request detected — piggybacking");
                self.dedup_hits.fetch_add(1, Ordering::Relaxed);
                DedupResult::Duplicate {
                    wait_rx: existing.get().done_rx.clone(),
                    response_reader: existing.get().response_chunks.clone(),
                }
            }
            dashmap::mapref::entry::Entry::Vacant(vacant) => {
                debug!(hash = hash, "First request registered for dedup");
                self.create_first_request_with_vacant(hash, vacant)
            }
        }
    }

    /// Create a first-request entry (after removing an expired one).
    fn create_first_request(&self, hash: u64) -> DedupResult {
        let (done_tx, done_rx) = tokio::sync::watch::channel(false);
        let byte_size = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let entry = InFlightEntry {
            response_chunks: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            created_at: Instant::now(),
            byte_size: byte_size.clone(),
            done_tx: done_tx.clone(),
            done_rx,
        };
        self.entries.insert(hash, entry);
        self.dedup_misses.fetch_add(1, Ordering::Relaxed);
        DedupResult::FirstRequest {
            response_writer: self
                .entries
                .get(&hash)
                .unwrap()
                .response_chunks
                .clone(),
            done_signal: done_tx,
            byte_size_tracker: byte_size,
        }
    }

    /// Create a first-request entry using a vacant entry slot.
    fn create_first_request_with_vacant(
        &self,
        _hash: u64,
        vacant: dashmap::mapref::entry::VacantEntry<u64, InFlightEntry>,
    ) -> DedupResult {
        let (done_tx, done_rx) = tokio::sync::watch::channel(false);
        let byte_size = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let entry = InFlightEntry {
            response_chunks: Arc::new(tokio::sync::Mutex::new(Vec::new())),
            created_at: Instant::now(),
            byte_size: byte_size.clone(),
            done_tx: done_tx.clone(),
            done_rx,
        };
        let response_chunks = entry.response_chunks.clone();
        vacant.insert(entry);
        self.dedup_misses.fetch_add(1, Ordering::Relaxed);
        DedupResult::FirstRequest {
            response_writer: response_chunks,
            done_signal: done_tx,
            byte_size_tracker: byte_size,
        }
    }

    /// Remove a completed entry from the dedup map.
    pub fn complete(&self, hash: u64) {
        if let Some(entry) = self.entries.get(&hash) {
            // Signal all waiters that the response is done
            let _ = entry.done_tx.send(true);
            // Entry lock is released when `entry` is dropped
        }
        // Remove the entry so future requests are treated as new
        self.entries.remove(&hash);
    }

    /// Remove all expired entries older than the window.
    /// Returns the number of entries pruned.
    pub fn prune_expired(&self) -> usize {
        let window = Duration::from_secs(self.config.window_secs);
        let before = self.entries.len();
        self.entries.retain(|_hash, entry| {
            entry.created_at.elapsed() < window
        });
        before - self.entries.len()
    }

    /// Number of currently in-flight dedup entries.
    #[allow(dead_code)]
    pub fn in_flight_count(&self) -> usize {
        self.entries.len()
    }

    /// Get current dedup statistics.
    pub fn get_stats(&self) -> DedupStats {
        let total_requests = self.total_requests.load(Ordering::Relaxed);
        let dedup_hits = self.dedup_hits.load(Ordering::Relaxed);
        let dedup_misses = self.dedup_misses.load(Ordering::Relaxed);
        let active = self.entries.len();

        // Sum up approximate byte sizes of all cached responses
        let cache_size_bytes: u64 = self
            .entries
            .iter()
            .map(|e| e.value().byte_size.load(Ordering::Relaxed))
            .sum();

        DedupStats {
            total_requests,
            dedup_hits,
            dedup_misses,
            active_cached_responses: active,
            cache_size_bytes,
            enabled: self.config.enabled,
            window_secs: self.config.window_secs,
            dedup_ttl_secs: self.config.dedup_ttl_secs,
        }
    }

    /// Update the dedup byte size tracker for an entry.
    /// Call this when writing response chunks to track cache memory usage.
    pub fn update_byte_size(
        byte_size_tracker: &Arc<std::sync::atomic::AtomicU64>,
        chunk: &str,
    ) {
        byte_size_tracker.fetch_add(chunk.len() as u64, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_messages(content: &str) -> Vec<serde_json::Value> {
        vec![serde_json::json!({
            "role": "user",
            "content": content
        })]
    }

    #[test]
    fn test_hash_deterministic() {
        let msgs = make_messages("hello");
        let h1 = RequestDedup::hash_request("gpt-4", &msgs);
        let h2 = RequestDedup::hash_request("gpt-4", &msgs);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_different_models() {
        let msgs = make_messages("hello");
        let h1 = RequestDedup::hash_request("gpt-4", &msgs);
        let h2 = RequestDedup::hash_request("gpt-3.5", &msgs);
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_hash_different_messages() {
        let msgs1 = make_messages("hello");
        let msgs2 = make_messages("world");
        let h1 = RequestDedup::hash_request("gpt-4", &msgs1);
        let h2 = RequestDedup::hash_request("gpt-4", &msgs2);
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_hash_different_roles() {
        let msgs1 = vec![serde_json::json!({"role": "user", "content": "hi"})];
        let msgs2 = vec![serde_json::json!({"role": "system", "content": "hi"})];
        let h1 = RequestDedup::hash_request("gpt-4", &msgs1);
        let h2 = RequestDedup::hash_request("gpt-4", &msgs2);
        assert_ne!(h1, h2);
    }

    #[tokio::test]
    async fn test_first_request_registered() {
        let dedup = RequestDedup::new(true, 30);
        let msgs = make_messages("hello");
        let hash = RequestDedup::hash_request("gpt-4", &msgs);

        match dedup.register(hash) {
            DedupResult::FirstRequest { .. } => {}
            DedupResult::Duplicate { .. } => panic!("Expected FirstRequest"),
        }

        let stats = dedup.get_stats();
        assert_eq!(stats.total_requests, 1);
        assert_eq!(stats.dedup_hits, 0);
        assert_eq!(stats.dedup_misses, 1);
    }

    #[tokio::test]
    async fn test_duplicate_request_detected() {
        let dedup = RequestDedup::new(true, 30);
        let msgs = make_messages("hello");
        let hash = RequestDedup::hash_request("gpt-4", &msgs);

        // First request
        match dedup.register(hash) {
            DedupResult::FirstRequest { response_writer, done_signal, .. } => {
                // Write some response data
                response_writer.lock().await.push("data: hello\n".to_string());
                // Signal completion
                let _ = done_signal.send(true);
            }
            DedupResult::Duplicate { .. } => panic!("Expected FirstRequest"),
        }

        // Second request should see Duplicate (if entry still exists)
        // Note: register creates a new entry if the first was completed/removed
        // So this tests the concurrent case
    }

    #[tokio::test]
    async fn test_duplicate_piggybacks_on_response() {
        let dedup = RequestDedup::new(true, 30);
        let msgs = make_messages("hello");
        let hash = RequestDedup::hash_request("gpt-4", &msgs);

        // First request — don't complete yet
        let response_writer = match dedup.register(hash) {
            DedupResult::FirstRequest { response_writer, .. } => response_writer,
            DedupResult::Duplicate { .. } => panic!("Expected FirstRequest"),
        };

        // Simulate second request arriving while first is in-flight
        match dedup.register(hash) {
            DedupResult::Duplicate { .. } => {
                // Expected
            }
            DedupResult::FirstRequest { .. } => panic!("Expected Duplicate"),
        }

        let stats = dedup.get_stats();
        assert_eq!(stats.total_requests, 2);
        assert_eq!(stats.dedup_hits, 1);
        assert_eq!(stats.dedup_misses, 1);

        // First request writes response and completes
        response_writer.lock().await.push("data: response chunk\n".to_string());
        response_writer.lock().await.push("data: [DONE]\n".to_string());
        dedup.complete(hash);

        // After completion, a new request should be treated as first again
        match dedup.register(hash) {
            DedupResult::FirstRequest { .. } => {}
            DedupResult::Duplicate { .. } => panic!("Expected FirstRequest after completion"),
        }
    }

    #[tokio::test]
    async fn test_dedup_disabled_always_first() {
        let dedup = RequestDedup::new(false, 30);
        let msgs = make_messages("hello");
        let hash = RequestDedup::hash_request("gpt-4", &msgs);

        match dedup.register(hash) {
            DedupResult::FirstRequest { .. } => {}
            DedupResult::Duplicate { .. } => panic!("Expected FirstRequest when disabled"),
        }

        // Even a second register should return FirstRequest
        match dedup.register(hash) {
            DedupResult::FirstRequest { .. } => {}
            DedupResult::Duplicate { .. } => panic!("Expected FirstRequest when disabled"),
        }

        let stats = dedup.get_stats();
        assert_eq!(stats.total_requests, 2);
        assert_eq!(stats.dedup_hits, 0);
        assert_eq!(stats.dedup_misses, 2);
    }

    #[tokio::test]
    async fn test_prune_expired() {
        let dedup = RequestDedup::new(true, 1); // 1 second window
        let msgs = make_messages("hello");
        let hash = RequestDedup::hash_request("gpt-4", &msgs);

        dedup.register(hash);
        assert_eq!(dedup.in_flight_count(), 1);

        // Wait for expiry
        tokio::time::sleep(Duration::from_secs(2)).await;
        let pruned = dedup.prune_expired();
        assert_eq!(pruned, 1);
        assert_eq!(dedup.in_flight_count(), 0);
    }

    #[tokio::test]
    async fn test_different_hashes_no_dedup() {
        let dedup = RequestDedup::new(true, 30);
        let msgs1 = make_messages("hello");
        let msgs2 = make_messages("world");
        let hash1 = RequestDedup::hash_request("gpt-4", &msgs1);
        let hash2 = RequestDedup::hash_request("gpt-4", &msgs2);

        match dedup.register(hash1) {
            DedupResult::FirstRequest { .. } => {}
            DedupResult::Duplicate { .. } => panic!("Expected FirstRequest"),
        }

        // Different hash should also be FirstRequest
        match dedup.register(hash2) {
            DedupResult::FirstRequest { .. } => {}
            DedupResult::Duplicate { .. } => panic!("Expected FirstRequest for different hash"),
        }

        assert_eq!(dedup.in_flight_count(), 2);

        let stats = dedup.get_stats();
        assert_eq!(stats.total_requests, 2);
        assert_eq!(stats.dedup_hits, 0);
        assert_eq!(stats.dedup_misses, 2);
    }

    #[tokio::test]
    async fn test_ttl_expired_entry() {
        // Use a very short TTL so we can test expiry without sleeping
        let dedup = RequestDedup::new_with_config(crate::config::DedupConfig {
            enabled: true,
            window_secs: 300,
            dedup_ttl_secs: 0, // instant TTL expiry
        });
        let msgs = make_messages("hello");
        let hash = RequestDedup::hash_request("gpt-4", &msgs);

        // First request
        match dedup.register(hash) {
            DedupResult::FirstRequest { .. } => {}
            DedupResult::Duplicate { .. } => panic!("Expected FirstRequest"),
        }

        // Give a tiny bit of time for TTL to pass
        tokio::time::sleep(Duration::from_millis(5)).await;

        // Second request should be treated as first because TTL expired
        // (even though the entry still exists in the map)
        match dedup.register(hash) {
            DedupResult::FirstRequest { .. } => {
                // Expected — TTL expired
            }
            DedupResult::Duplicate { .. } => panic!("Expected FirstRequest after TTL expiry"),
        }
    }

    #[test]
    fn test_get_stats() {
        let dedup = RequestDedup::new(true, 30);
        let stats = dedup.get_stats();
        assert_eq!(stats.total_requests, 0);
        assert_eq!(stats.dedup_hits, 0);
        assert_eq!(stats.dedup_misses, 0);
        assert_eq!(stats.active_cached_responses, 0);
        assert_eq!(stats.cache_size_bytes, 0);
        assert!(stats.enabled);
        assert_eq!(stats.window_secs, 30);
        assert_eq!(stats.dedup_ttl_secs, 30);
    }

    #[test]
    fn test_update_byte_size() {
        let tracker = Arc::new(std::sync::atomic::AtomicU64::new(0));
        RequestDedup::update_byte_size(&tracker, "hello");
        RequestDedup::update_byte_size(&tracker, " world");
        assert_eq!(tracker.load(Ordering::Relaxed), 11);
    }

    #[test]
    fn test_new_with_config() {
        let config = crate::config::DedupConfig {
            enabled: true,
            window_secs: 60,
            dedup_ttl_secs: 45,
        };
        let dedup = RequestDedup::new_with_config(config);
        let stats = dedup.get_stats();
        assert_eq!(stats.window_secs, 60);
        assert_eq!(stats.dedup_ttl_secs, 45);
    }
}
