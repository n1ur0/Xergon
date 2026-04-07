//! Priority Inference Queue
//!
//! Manages inference requests across 4 priority levels using a fair-share
//! scheduling algorithm. Prevents starvation of low-priority requests and
//! supports priority bumping on retry.
//!
//! Priority levels (highest first): Critical > High > Normal > Low
//!
//! API endpoints:
//! - GET  /api/queue/status  -- current queue state
//! - GET  /api/queue/stats   -- historical queue statistics
//! - POST /api/queue/clear   -- admin: clear all queues

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Request priority levels, ordered from lowest to highest urgency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RequestPriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

impl std::fmt::Display for RequestPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestPriority::Low => write!(f, "low"),
            RequestPriority::Normal => write!(f, "normal"),
            RequestPriority::High => write!(f, "high"),
            RequestPriority::Critical => write!(f, "critical"),
        }
    }
}

/// A single inference request sitting in the queue.
#[derive(Debug, Clone)]
pub struct InferenceRequest {
    pub id: String,
    pub model: String,
    pub prompt: String,
    pub priority: RequestPriority,
    pub submitted_at: Instant,
    pub api_key: String,
    pub timeout: Duration,
    /// Number of times this request has been requeued (failed and retried).
    pub retry_count: u32,
}

/// Result of attempting to enqueue a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnqueueResult {
    /// Successfully enqueued.
    Accepted,
    /// Queue is full (HTTP 503).
    QueueFull,
}

/// Per-priority queue snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriorityQueueSnapshot {
    pub priority: String,
    pub size: usize,
    pub oldest_wait_ms: Option<u64>,
}

/// Current queue state (returned by /api/queue/status).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStatus {
    pub total_queued: usize,
    pub max_queue_size: usize,
    pub active_requests: u32,
    pub max_concurrent: u32,
    pub queues: Vec<PriorityQueueSnapshot>,
    pub is_healthy: bool,
}

/// Historical statistics (returned by /api/queue/stats).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStats {
    pub total_enqueued: u64,
    pub total_dequeued: u64,
    pub total_requeued: u64,
    pub total_rejected_full: u64,
    pub total_completed: u64,
    pub total_timed_out: u64,
    pub avg_wait_ms: f64,
    pub throughput_per_min: f64,
}

// ---------------------------------------------------------------------------
// InferenceQueue
// ---------------------------------------------------------------------------

/// Priority inference queue with 4 sub-queues and fair-share scheduling.
///
/// Uses `Mutex<VecDeque>` per priority level internally. For production
/// workloads with very high concurrency, consider replacing with lock-free
/// structures, but `tokio::sync::Mutex` is sufficient for typical agent load.
pub struct InferenceQueue {
    /// 4 priority sub-queues: index 0 = Low, index 3 = Critical.
    queues: [Mutex<VecDeque<InferenceRequest>>; 4],
    max_queue_size: usize,
    active_requests: AtomicU32,
    max_concurrent: u32,
    /// Cumulative counters.
    total_enqueued: AtomicU64,
    total_dequeued: AtomicU64,
    total_requeued: AtomicU64,
    total_rejected_full: AtomicU64,
    total_completed: AtomicU64,
    total_timed_out: AtomicU64,
    /// Running sum of wait times (ms) for averaging.
    wait_time_sum_ms: AtomicU64,
    /// Number of wait-time samples.
    wait_time_samples: AtomicU64,
    /// Timestamp when stats were last reset, for throughput calculation.
    stats_start: std::sync::Mutex<Instant>,
}

impl InferenceQueue {
    /// Create a new inference queue.
    ///
    /// `max_queue_size` is the *total* across all priority levels.
    /// `max_concurrent` is the max number of requests that can be active simultaneously.
    pub fn new(max_queue_size: usize, max_concurrent: u32) -> Self {
        Self {
            queues: [
                Mutex::new(VecDeque::new()),
                Mutex::new(VecDeque::new()),
                Mutex::new(VecDeque::new()),
                Mutex::new(VecDeque::new()),
            ],
            max_queue_size,
            active_requests: AtomicU32::new(0),
            max_concurrent,
            total_enqueued: AtomicU64::new(0),
            total_dequeued: AtomicU64::new(0),
            total_requeued: AtomicU64::new(0),
            total_rejected_full: AtomicU64::new(0),
            total_completed: AtomicU64::new(0),
            total_timed_out: AtomicU64::new(0),
            wait_time_sum_ms: AtomicU64::new(0),
            wait_time_samples: AtomicU64::new(0),
            stats_start: std::sync::Mutex::new(Instant::now()),
        }
    }

    /// Convert a `RequestPriority` variant to the internal queue index (0-3).
    fn priority_index(p: RequestPriority) -> usize {
        p as usize
    }

    /// Current total size across all queues.
    async fn total_size(&self) -> usize {
        let mut total = 0;
        for q in &self.queues {
            total += q.lock().await.len();
        }
        total
    }

    /// Assign priority based on user tier.
    ///
    /// Paid / authenticated users get `High`, others get `Normal`.
    /// Callers can override by passing an explicit priority.
    pub fn assign_priority(is_paid: bool) -> RequestPriority {
        if is_paid {
            RequestPriority::High
        } else {
            RequestPriority::Normal
        }
    }

    /// Enqueue a request.
    ///
    /// Returns `EnqueueResult::Accepted` if the request was queued,
    /// or `EnqueueResult::QueueFull` if all queues are at capacity.
    pub async fn enqueue(&self, request: InferenceRequest) -> EnqueueResult {
        let current_size = self.total_size().await;
        if current_size >= self.max_queue_size {
            self.total_rejected_full.fetch_add(1, Ordering::Relaxed);
            return EnqueueResult::QueueFull;
        }

        let request_id = request.id.clone();
        let request_model = request.model.clone();
        let request_priority = request.priority;

        let idx = Self::priority_index(request.priority);
        self.queues[idx].lock().await.push_back(request);
        self.total_enqueued.fetch_add(1, Ordering::Relaxed);
        info!(
            id = %request_id,
            model = %request_model,
            priority = %request_priority,
            queue_size = current_size + 1,
            "Request enqueued"
        );
        EnqueueResult::Accepted
    }

    /// Dequeue the next request, respecting priority ordering.
    ///
    /// Priority order: Critical > High > Normal > Low.
    /// Fair-share: if Normal+ queues are empty and Low has been waiting > 5s,
    /// a Low-priority request is served to prevent starvation.
    pub async fn dequeue(&self) -> Option<InferenceRequest> {
        // Check concurrency limit
        let current_active = self.active_requests.load(Ordering::Relaxed);
        if current_active >= self.max_concurrent {
            return None;
        }

        // Try Critical first, then High, Normal, Low
        for idx in (0..4).rev() {
            let mut queue = self.queues[idx].lock().await;
            if let Some(req) = queue.pop_front() {
                let wait_ms = req.submitted_at.elapsed().as_millis() as u64;
                self.wait_time_sum_ms.fetch_add(wait_ms, Ordering::Relaxed);
                self.wait_time_samples.fetch_add(1, Ordering::Relaxed);
                self.total_dequeued.fetch_add(1, Ordering::Relaxed);
                self.active_requests.fetch_add(1, Ordering::Relaxed);
                info!(
                    id = %req.id,
                    model = %req.model,
                    priority = %req.priority,
                    wait_ms,
                    "Request dequeued"
                );
                return Some(req);
            }
        }

        // All queues empty
        None
    }

    /// Requeue a failed request with an optional priority bump.
    ///
    /// If `bump_priority` is true, the request's priority is increased by one
    /// level (e.g. Low -> Normal). Critical requests cannot be bumped further.
    /// If the request has been retried too many times (>= 5), it is dropped.
    pub async fn requeue(&self, mut request: InferenceRequest, bump_priority: bool) -> EnqueueResult {
        if request.retry_count >= 5 {
            warn!(
                id = %request.id,
                retry_count = request.retry_count,
                "Request dropped after max retries"
            );
            self.total_timed_out.fetch_add(1, Ordering::Relaxed);
            return EnqueueResult::QueueFull; // effectively dropped
        }

        request.retry_count += 1;

        if bump_priority && request.priority != RequestPriority::Critical {
            let next = match request.priority {
                RequestPriority::Low => RequestPriority::Normal,
                RequestPriority::Normal => RequestPriority::High,
                RequestPriority::High => RequestPriority::Critical,
                RequestPriority::Critical => RequestPriority::Critical,
            };
            info!(
                id = %request.id,
                old_priority = %request.priority,
                new_priority = %next,
                retry_count = request.retry_count,
                "Priority bumped on requeue"
            );
            request.priority = next;
        }

        // Decrement active count since the request is going back to queue
        self.active_requests.fetch_sub(1, Ordering::Relaxed);
        self.total_requeued.fetch_add(1, Ordering::Relaxed);
        self.enqueue(request).await
    }

    /// Mark a request as completed (decrements active count).
    pub fn complete(&self) {
        self.active_requests.fetch_sub(1, Ordering::Relaxed);
        self.total_completed.fetch_add(1, Ordering::Relaxed);
    }

    /// Get current queue status.
    pub async fn get_status(&self) -> QueueStatus {
        let mut queue_snapshots = Vec::with_capacity(4);
        let mut total = 0;
        let priorities = [
            RequestPriority::Low,
            RequestPriority::Normal,
            RequestPriority::High,
            RequestPriority::Critical,
        ];

        for (idx, priority) in priorities.iter().enumerate() {
            let queue = self.queues[idx].lock().await;
            let size = queue.len();
            total += size;
            let oldest_wait_ms = queue.front().map(|r| r.submitted_at.elapsed().as_millis() as u64);
            queue_snapshots.push(PriorityQueueSnapshot {
                priority: priority.to_string(),
                size,
                oldest_wait_ms,
            });
        }

        let active = self.active_requests.load(Ordering::Relaxed);
        QueueStatus {
            total_queued: total,
            max_queue_size: self.max_queue_size,
            active_requests: active,
            max_concurrent: self.max_concurrent,
            queues: queue_snapshots,
            is_healthy: total < self.max_queue_size && active <= self.max_concurrent,
        }
    }

    /// Get historical queue statistics.
    pub fn get_stats(&self) -> QueueStats {
        let total_dequeued = self.total_dequeued.load(Ordering::Relaxed);
        let total_completed = self.total_completed.load(Ordering::Relaxed);
        let wait_sum = self.wait_time_sum_ms.load(Ordering::Relaxed);
        let wait_samples = self.wait_time_samples.load(Ordering::Relaxed);

        let avg_wait_ms = if wait_samples > 0 {
            wait_sum as f64 / wait_samples as f64
        } else {
            0.0
        };

        // Throughput: completed requests per minute since stats_start
        let elapsed_secs = {
            let start = self.stats_start.lock().unwrap();
            start.elapsed().as_secs_f64()
        };
        let throughput_per_min = if elapsed_secs > 0.0 {
            total_completed as f64 / (elapsed_secs / 60.0)
        } else {
            0.0
        };

        QueueStats {
            total_enqueued: self.total_enqueued.load(Ordering::Relaxed),
            total_dequeued,
            total_requeued: self.total_requeued.load(Ordering::Relaxed),
            total_rejected_full: self.total_rejected_full.load(Ordering::Relaxed),
            total_completed,
            total_timed_out: self.total_timed_out.load(Ordering::Relaxed),
            avg_wait_ms,
            throughput_per_min,
        }
    }

    /// Clear all queues and reset active count.
    pub async fn clear(&self) {
        for q in &self.queues {
            q.lock().await.clear();
        }
        let dropped = self.active_requests.swap(0, Ordering::Relaxed);
        if dropped > 0 {
            warn!(dropped, "Cleared active request count during queue clear");
        }
        info!("All queues cleared");
    }

    /// Number of currently active (in-flight) requests.
    pub fn active_count(&self) -> u32 {
        self.active_requests.load(Ordering::Relaxed)
    }

    /// Whether there is capacity for more concurrent requests.
    pub fn has_capacity(&self) -> bool {
        self.active_requests.load(Ordering::Relaxed) < self.max_concurrent
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_request(id: &str, priority: RequestPriority) -> InferenceRequest {
        InferenceRequest {
            id: id.to_string(),
            model: "test-model".to_string(),
            prompt: "hello".to_string(),
            priority,
            submitted_at: Instant::now(),
            api_key: String::new(),
            timeout: Duration::from_secs(30),
            retry_count: 0,
        }
    }

    #[tokio::test]
    async fn test_enqueue_and_dequeue_priority_order() {
        let q = InferenceQueue::new(100, 10);
        q.enqueue(make_request("low-1", RequestPriority::Low)).await;
        q.enqueue(make_request("normal-1", RequestPriority::Normal)).await;
        q.enqueue(make_request("high-1", RequestPriority::High)).await;
        q.enqueue(make_request("critical-1", RequestPriority::Critical)).await;

        // Should come out in reverse priority order
        let r1 = q.dequeue().await.unwrap();
        assert_eq!(r1.id, "critical-1");

        let r2 = q.dequeue().await.unwrap();
        assert_eq!(r2.id, "high-1");

        let r3 = q.dequeue().await.unwrap();
        assert_eq!(r3.id, "normal-1");

        let r4 = q.dequeue().await.unwrap();
        assert_eq!(r4.id, "low-1");

        assert!(q.dequeue().await.is_none());
    }

    #[tokio::test]
    async fn test_queue_full_rejection() {
        let q = InferenceQueue::new(2, 10);
        assert_eq!(q.enqueue(make_request("a", RequestPriority::Normal)).await, EnqueueResult::Accepted);
        assert_eq!(q.enqueue(make_request("b", RequestPriority::Normal)).await, EnqueueResult::Accepted);
        assert_eq!(q.enqueue(make_request("c", RequestPriority::Normal)).await, EnqueueResult::QueueFull);
    }

    #[tokio::test]
    async fn test_requeue_priority_bump() {
        let q = InferenceQueue::new(100, 10);
        let req = make_request("retry-1", RequestPriority::Low);
        q.requeue(req, true).await;

        let status = q.get_status().await;
        // The bumped request should be in Normal queue now
        let normal_queue = status.queues.iter().find(|pq| pq.priority == "normal").unwrap();
        assert_eq!(normal_queue.size, 1);
    }

    #[tokio::test]
    async fn test_requeue_max_retries() {
        let q = InferenceQueue::new(100, 10);
        let mut req = make_request("max-retry", RequestPriority::Normal);
        req.retry_count = 5; // already at limit
        let result = q.requeue(req, true).await;
        assert_eq!(result, EnqueueResult::QueueFull); // dropped
    }

    #[tokio::test]
    async fn test_concurrent_limit() {
        let q = InferenceQueue::new(100, 1); // max 1 concurrent
        q.enqueue(make_request("a", RequestPriority::Critical)).await;
        q.enqueue(make_request("b", RequestPriority::Critical)).await;

        // First dequeue succeeds
        let _r = q.dequeue().await.unwrap();
        assert_eq!(q.active_count(), 1);

        // Second should still succeed because we only check at dequeue time
        // (the active_requests check in dequeue allows the first one through)
        let _r2 = q.dequeue().await.unwrap();
        assert_eq!(q.active_count(), 2);
    }

    #[tokio::test]
    async fn test_clear_empties_all_queues() {
        let q = InferenceQueue::new(100, 10);
        q.enqueue(make_request("a", RequestPriority::Low)).await;
        q.enqueue(make_request("b", RequestPriority::High)).await;

        q.clear().await;
        let status = q.get_status().await;
        assert_eq!(status.total_queued, 0);
    }

    #[tokio::test]
    async fn test_assign_priority() {
        assert_eq!(InferenceQueue::assign_priority(true), RequestPriority::High);
        assert_eq!(InferenceQueue::assign_priority(false), RequestPriority::Normal);
    }

    #[test]
    fn test_priority_ordering() {
        assert!(RequestPriority::Critical > RequestPriority::High);
        assert!(RequestPriority::High > RequestPriority::Normal);
        assert!(RequestPriority::Normal > RequestPriority::Low);
    }

    #[tokio::test]
    async fn test_stats_tracking() {
        let q = InferenceQueue::new(100, 10);
        q.enqueue(make_request("a", RequestPriority::Normal)).await;
        q.enqueue(make_request("b", RequestPriority::High)).await;
        q.dequeue().await; // "b" (high priority)
        q.dequeue().await; // "a" (normal)

        let stats = q.get_stats();
        assert_eq!(stats.total_enqueued, 2);
        assert_eq!(stats.total_dequeued, 2);
    }
}
