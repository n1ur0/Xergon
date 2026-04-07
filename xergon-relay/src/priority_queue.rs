//! Priority Queue for request scheduling.
//!
//! When providers are at capacity, requests are queued by priority level.
//! Higher priority requests are served first when capacity becomes available.
//!
//! Priority levels:
//!   0 = Critical (paid premium) — highest priority
//!   1 = High (paid standard)
//!   2 = Normal (free, authenticated)
//!   3 = Low (anonymous)

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the priority queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityQueueConfig {
    /// Enable/disable priority queuing (default: true).
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Maximum queue size per priority level (default: 100).
    #[serde(default = "default_max_per_level")]
    pub max_per_level: usize,
    /// Maximum total queue size across all levels (default: 500).
    #[serde(default = "default_max_total")]
    pub max_total: usize,
    /// How long a request can wait in the queue before being rejected (seconds, default: 60).
    #[serde(default = "default_max_wait_secs")]
    pub max_wait_secs: u64,
    /// How often to clean up expired queued requests (seconds, default: 10).
    #[serde(default = "default_cleanup_interval_secs")]
    pub cleanup_interval_secs: u64,
}

impl Default for PriorityQueueConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            max_per_level: default_max_per_level(),
            max_total: default_max_total(),
            max_wait_secs: default_max_wait_secs(),
            cleanup_interval_secs: default_cleanup_interval_secs(),
        }
    }
}

fn default_enabled() -> bool {
    true
}
fn default_max_per_level() -> usize {
    100
}
fn default_max_total() -> usize {
    500
}
fn default_max_wait_secs() -> u64 {
    60
}
fn default_cleanup_interval_secs() -> u64 {
    10
}

// ---------------------------------------------------------------------------
// Priority Level
// ---------------------------------------------------------------------------

/// Priority level for a request. Lower value = higher priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestPriority {
    /// Paid premium users.
    Critical = 0,
    /// Paid standard users.
    High = 1,
    /// Free, authenticated users.
    Normal = 2,
    /// Anonymous users.
    Low = 3,
}

impl RequestPriority {
    /// Convert from a u8 value (used by load_shed::Priority for compatibility).
    pub fn from_u8(val: u8) -> Self {
        match val {
            0 => RequestPriority::Critical,
            1 => RequestPriority::High,
            2 => RequestPriority::Normal,
            _ => RequestPriority::Low,
        }
    }

    /// Convert to a display name.
    pub fn as_str(&self) -> &'static str {
        match self {
            RequestPriority::Critical => "critical",
            RequestPriority::High => "high",
            RequestPriority::Normal => "normal",
            RequestPriority::Low => "low",
        }
    }
}

impl std::fmt::Display for RequestPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Queued Request
// ---------------------------------------------------------------------------

/// A request waiting in the priority queue.
struct QueuedRequest {
    /// Unique identifier for this queued request.
    request_id: String,
    /// Priority level.
    priority: RequestPriority,
    /// When the request was enqueued.
    enqueued_at: Instant,
    /// Optional context (model name, for stats).
    model: Option<String>,
}

// ---------------------------------------------------------------------------
// Queue Stats
// ---------------------------------------------------------------------------

/// Statistics for the priority queue.
#[derive(Debug, Clone, Serialize)]
pub struct PriorityQueueStats {
    /// Number of queued requests per priority level.
    pub queue_depths: serde_json::Value,
    /// Total queued requests across all levels.
    pub total_queued: usize,
    /// Maximum total capacity.
    pub max_total: usize,
    /// Maximum per-level capacity.
    pub max_per_level: usize,
    /// Total requests that have been enqueued since startup.
    pub total_enqueued: u64,
    /// Total requests that have been dequeued since startup.
    pub total_dequeued: u64,
    /// Total requests rejected because the queue was full.
    pub total_rejected: u64,
    /// Total requests that expired while waiting.
    pub total_expired: u64,
    /// Whether priority queuing is enabled.
    pub enabled: bool,
}

// ---------------------------------------------------------------------------
// Priority Queue
// ---------------------------------------------------------------------------

/// Thread-safe priority queue for request scheduling.
///
/// Requests are organized into per-level FIFO queues. When dequeuing,
/// the highest-priority non-empty queue is served first.
pub struct PriorityQueue {
    /// Per-priority-level queues.
    queues: DashMap<RequestPriority, VecDeque<QueuedRequest>>,
    /// Configuration.
    config: PriorityQueueConfig,
    /// Total requests enqueued since startup.
    total_enqueued: AtomicU64,
    /// Total requests dequeued since startup.
    total_dequeued: AtomicU64,
    /// Total requests rejected due to full queue.
    total_rejected: AtomicU64,
    /// Total requests that expired while queued.
    total_expired: AtomicU64,
    /// Notifier for when a slot becomes available (for async waiting).
    slot_available: Notify,
}

/// Result of attempting to enqueue a request.
#[derive(Debug, PartialEq)]
pub enum EnqueueResult {
    /// Successfully enqueued.
    Queued,
    /// Queue is full at this level.
    LevelFull,
    /// Total queue capacity exceeded.
    TotalFull,
    /// Priority queuing is disabled.
    Disabled,
}

/// Result of dequeuing a request.
pub struct DequeuedRequest {
    /// The request ID that was dequeued.
    pub request_id: String,
    /// The priority level of the dequeued request.
    pub priority: RequestPriority,
    /// How long the request waited in the queue.
    pub wait_duration: Duration,
}

impl PriorityQueue {
    /// Create a new priority queue.
    pub fn new(config: PriorityQueueConfig) -> Self {
        let mut queues = DashMap::new();
        queues.insert(RequestPriority::Critical, VecDeque::new());
        queues.insert(RequestPriority::High, VecDeque::new());
        queues.insert(RequestPriority::Normal, VecDeque::new());
        queues.insert(RequestPriority::Low, VecDeque::new());

        Self {
            queues,
            config,
            total_enqueued: AtomicU64::new(0),
            total_dequeued: AtomicU64::new(0),
            total_rejected: AtomicU64::new(0),
            total_expired: AtomicU64::new(0),
            slot_available: Notify::new(),
        }
    }

    /// Check if priority queuing is enabled.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the total number of queued requests across all levels.
    pub fn total_queued(&self) -> usize {
        self.queues.iter().map(|q| q.value().len()).sum()
    }

    /// Get the queue depth for a specific priority level.
    pub fn queue_depth(&self, priority: RequestPriority) -> usize {
        self.queues
            .get(&priority)
            .map(|q| q.value().len())
            .unwrap_or(0)
    }

    /// Enqueue a request at the given priority level.
    ///
    /// Returns `EnqueueResult` indicating success or failure reason.
    pub fn enqueue(
        &self,
        request_id: &str,
        priority: RequestPriority,
        model: Option<&str>,
    ) -> EnqueueResult {
        if !self.config.enabled {
            return EnqueueResult::Disabled;
        }

        let total = self.total_queued();
        let level_depth = self.queue_depth(priority);

        if total >= self.config.max_total {
            self.total_rejected.fetch_add(1, Ordering::Relaxed);
            debug!(
                request_id,
                priority = %priority,
                total,
                max_total = self.config.max_total,
                "Priority queue full (total)"
            );
            return EnqueueResult::TotalFull;
        }

        if level_depth >= self.config.max_per_level {
            self.total_rejected.fetch_add(1, Ordering::Relaxed);
            debug!(
                request_id,
                priority = %priority,
                level_depth,
                max_per_level = self.config.max_per_level,
                "Priority queue full (level)"
            );
            return EnqueueResult::LevelFull;
        }

        if let Some(mut queue) = self.queues.get_mut(&priority) {
            queue.value_mut().push_back(QueuedRequest {
                request_id: request_id.to_string(),
                priority,
                enqueued_at: Instant::now(),
                model: model.map(|s| s.to_string()),
            });
        }

        self.total_enqueued.fetch_add(1, Ordering::Relaxed);
        debug!(
            request_id,
            priority = %priority,
            total = self.total_queued(),
            "Request enqueued in priority queue"
        );

        EnqueueResult::Queued
    }

    /// Dequeue the highest-priority request.
    ///
    /// Iterates through priority levels from Critical to Low
    /// and returns the first available request.
    pub fn dequeue(&self) -> Option<DequeuedRequest> {
        let priorities = [
            RequestPriority::Critical,
            RequestPriority::High,
            RequestPriority::Normal,
            RequestPriority::Low,
        ];

        for priority in &priorities {
            if let Some(mut queue) = self.queues.get_mut(priority) {
                if let Some(req) = queue.value_mut().pop_front() {
                    self.total_dequeued.fetch_add(1, Ordering::Relaxed);
                    let wait = req.enqueued_at.elapsed();
                    self.slot_available.notify_waiters();
                    debug!(
                        request_id = %req.request_id,
                        priority = %priority,
                        wait_ms = wait.as_millis(),
                        "Request dequeued from priority queue"
                    );
                    return Some(DequeuedRequest {
                        request_id: req.request_id,
                        priority: req.priority,
                        wait_duration: wait,
                    });
                }
            }
        }

        None
    }

    /// Remove a specific request from the queue (e.g., if the client disconnects).
    pub fn cancel(&self, request_id: &str) -> bool {
        for mut queue in self.queues.iter_mut() {
            let pos = queue.value().iter().position(|r| r.request_id == request_id);
            if let Some(pos) = pos {
                queue.value_mut().remove(pos);
                self.slot_available.notify_waiters();
                debug!(request_id, "Cancelled queued request");
                return true;
            }
        }
        false
    }

    /// Remove all expired requests (waited longer than max_wait_secs).
    /// Returns the number of expired requests removed.
    pub fn cleanup_expired(&self) -> usize {
        let max_wait = Duration::from_secs(self.config.max_wait_secs);
        let mut expired_count = 0;

        for mut queue in self.queues.iter_mut() {
            let before = queue.value().len();
            queue.value_mut().retain(|r| r.enqueued_at.elapsed() < max_wait);
            let removed = before - queue.value().len();
            expired_count += removed;
        }

        if expired_count > 0 {
            self.total_expired.fetch_add(expired_count as u64, Ordering::Relaxed);
            self.slot_available.notify_waiters();
            debug!(expired_count, "Cleaned up expired queued requests");
        }

        expired_count
    }

    /// Wait until a slot becomes available (or timeout).
    /// Returns true if a slot is available (or queue was modified).
    pub async fn wait_for_slot(&self, timeout: Duration) -> bool {
        // Check if there's room right now
        if self.total_queued() < self.config.max_total {
            return true;
        }

        tokio::select! {
            _ = self.slot_available.notified() => true,
            _ = tokio::time::sleep(timeout) => false,
        }
    }

    /// Get current statistics.
    pub fn get_stats(&self) -> PriorityQueueStats {
        let mut depths = serde_json::Map::new();
        depths.insert(
            "critical".to_string(),
            serde_json::Value::Number(self.queue_depth(RequestPriority::Critical).into()),
        );
        depths.insert(
            "high".to_string(),
            serde_json::Value::Number(self.queue_depth(RequestPriority::High).into()),
        );
        depths.insert(
            "normal".to_string(),
            serde_json::Value::Number(self.queue_depth(RequestPriority::Normal).into()),
        );
        depths.insert(
            "low".to_string(),
            serde_json::Value::Number(self.queue_depth(RequestPriority::Low).into()),
        );

        PriorityQueueStats {
            queue_depths: serde_json::Value::Object(depths),
            total_queued: self.total_queued(),
            max_total: self.config.max_total,
            max_per_level: self.config.max_per_level,
            total_enqueued: self.total_enqueued.load(Ordering::Relaxed),
            total_dequeued: self.total_dequeued.load(Ordering::Relaxed),
            total_rejected: self.total_rejected.load(Ordering::Relaxed),
            total_expired: self.total_expired.load(Ordering::Relaxed),
            enabled: self.config.enabled,
        }
    }

    /// Start a background cleanup task.
    pub fn start_cleanup_task(self: &Arc<Self>) {
        let queue = self.clone();
        let interval_secs = self.config.cleanup_interval_secs;
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(interval_secs)).await;
                queue.cleanup_expired();
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config() -> PriorityQueueConfig {
        PriorityQueueConfig {
            enabled: true,
            max_per_level: 10,
            max_total: 30,
            max_wait_secs: 60,
            cleanup_interval_secs: 10,
        }
    }

    #[test]
    fn test_enqueue_and_dequeue() {
        let pq = PriorityQueue::new(make_config());

        assert_eq!(pq.enqueue("req1", RequestPriority::Normal, Some("llama-3.1")), EnqueueResult::Queued);
        assert_eq!(pq.enqueue("req2", RequestPriority::Critical, Some("llama-3.1")), EnqueueResult::Queued);

        // Critical should be dequeued first despite being enqueued second
        let dequeued = pq.dequeue().unwrap();
        assert_eq!(dequeued.request_id, "req2");
        assert_eq!(dequeued.priority, RequestPriority::Critical);

        let dequeued = pq.dequeue().unwrap();
        assert_eq!(dequeued.request_id, "req1");
        assert_eq!(dequeued.priority, RequestPriority::Normal);
    }

    #[test]
    fn test_priority_ordering() {
        let pq = PriorityQueue::new(make_config());

        pq.enqueue("low", RequestPriority::Low, None);
        pq.enqueue("normal", RequestPriority::Normal, None);
        pq.enqueue("high", RequestPriority::High, None);
        pq.enqueue("critical", RequestPriority::Critical, None);

        // Dequeue order: critical, high, normal, low
        assert_eq!(pq.dequeue().unwrap().request_id, "critical");
        assert_eq!(pq.dequeue().unwrap().request_id, "high");
        assert_eq!(pq.dequeue().unwrap().request_id, "normal");
        assert_eq!(pq.dequeue().unwrap().request_id, "low");
    }

    #[test]
    fn test_level_full() {
        let pq = PriorityQueue::new(PriorityQueueConfig {
            max_per_level: 2,
            max_total: 30,
            ..make_config()
        });

        assert_eq!(pq.enqueue("r1", RequestPriority::Critical, None), EnqueueResult::Queued);
        assert_eq!(pq.enqueue("r2", RequestPriority::Critical, None), EnqueueResult::Queued);
        assert_eq!(pq.enqueue("r3", RequestPriority::Critical, None), EnqueueResult::LevelFull);
    }

    #[test]
    fn test_total_full() {
        let pq = PriorityQueue::new(PriorityQueueConfig {
            max_per_level: 10,
            max_total: 2,
            ..make_config()
        });

        assert_eq!(pq.enqueue("r1", RequestPriority::Critical, None), EnqueueResult::Queued);
        assert_eq!(pq.enqueue("r2", RequestPriority::High, None), EnqueueResult::Queued);
        assert_eq!(pq.enqueue("r3", RequestPriority::Normal, None), EnqueueResult::TotalFull);
    }

    #[test]
    fn test_disabled() {
        let pq = PriorityQueue::new(PriorityQueueConfig {
            enabled: false,
            ..make_config()
        });

        assert_eq!(pq.enqueue("r1", RequestPriority::Normal, None), EnqueueResult::Disabled);
    }

    #[test]
    fn test_cancel() {
        let pq = PriorityQueue::new(make_config());
        pq.enqueue("r1", RequestPriority::Normal, None);
        pq.enqueue("r2", RequestPriority::Normal, None);

        assert!(pq.cancel("r1"));
        assert!(!pq.cancel("r1")); // already removed

        let dequeued = pq.dequeue().unwrap();
        assert_eq!(dequeued.request_id, "r2");
    }

    #[test]
    fn test_fifo_within_priority() {
        let pq = PriorityQueue::new(make_config());

        pq.enqueue("a", RequestPriority::Normal, None);
        pq.enqueue("b", RequestPriority::Normal, None);
        pq.enqueue("c", RequestPriority::Normal, None);

        assert_eq!(pq.dequeue().unwrap().request_id, "a");
        assert_eq!(pq.dequeue().unwrap().request_id, "b");
        assert_eq!(pq.dequeue().unwrap().request_id, "c");
    }

    #[test]
    fn test_stats() {
        let pq = PriorityQueue::new(make_config());
        pq.enqueue("r1", RequestPriority::Critical, None);
        pq.enqueue("r2", RequestPriority::Normal, None);

        let stats = pq.get_stats();
        assert_eq!(stats.total_queued, 2);
        assert_eq!(stats.total_enqueued, 2);
        assert_eq!(stats.total_dequeued, 0);
        assert!(stats.enabled);
    }

    #[test]
    fn test_priority_from_u8() {
        assert_eq!(RequestPriority::from_u8(0), RequestPriority::Critical);
        assert_eq!(RequestPriority::from_u8(1), RequestPriority::High);
        assert_eq!(RequestPriority::from_u8(2), RequestPriority::Normal);
        assert_eq!(RequestPriority::from_u8(3), RequestPriority::Low);
        assert_eq!(RequestPriority::from_u8(99), RequestPriority::Low);
    }

    #[test]
    fn test_cleanup_expired() {
        let pq = PriorityQueue::new(PriorityQueueConfig {
            max_wait_secs: 0, // instant expiry
            ..make_config()
        });

        pq.enqueue("r1", RequestPriority::Normal, None);
        assert_eq!(pq.total_queued(), 1);

        // Give it a tiny bit of time
        std::thread::sleep(std::time::Duration::from_millis(10));

        let expired = pq.cleanup_expired();
        assert_eq!(expired, 1);
        assert_eq!(pq.total_queued(), 0);
    }
}
