//! Free tier request tracker for the Xergon relay.
//!
//! Tracks per-user free request counts so that new users who received an
//! airdrop get a limited number of free inference requests before needing
//! to deposit ERG.
//!
//! Thread-safe via `DashMap` + `AtomicU64` for the counter.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use dashmap::DashMap;
use tracing::debug;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Result of a free tier check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FreeTierCheck {
    /// User is within the free tier limit — request is allowed.
    Free,
    /// User has exhausted their free tier quota.
    Exhausted { used: u64, limit: u64 },
}

/// Current usage information for a user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreeTierUsage {
    /// Number of requests used in the current window.
    pub requests_used: u64,
    /// Maximum allowed requests per window.
    pub max_requests: u64,
    /// Whether the user is currently exhausted.
    pub exhausted: bool,
    /// Seconds remaining until the counter resets (0 if not tracked).
    pub reset_in_secs: u64,
}

// ---------------------------------------------------------------------------
// FreeTierState (per-user)
// ---------------------------------------------------------------------------

struct FreeTierState {
    requests_used: AtomicU64,
    first_request_at: Instant,
}

// ---------------------------------------------------------------------------
// FreeTierTracker
// ---------------------------------------------------------------------------

/// Thread-safe tracker for per-user free request quotas.
///
/// Each user is identified by their public key (hex string). The counter
/// resets after `decay_hours` have elapsed since the first request in the
/// current window, giving the user a fresh quota.
pub struct FreeTierTracker {
    users: DashMap<String, FreeTierState>,
    max_free_requests: u64,
    decay_secs: u64,
}

impl FreeTierTracker {
    /// Create a new free tier tracker.
    ///
    /// # Arguments
    /// * `max_free_requests` — maximum free requests per window (default: 100)
    /// * `decay_hours` — hours after which the counter resets (default: 24)
    pub fn new(max_free_requests: u64, decay_hours: u64) -> Self {
        Self {
            users: DashMap::with_capacity(10_000),
            max_free_requests,
            decay_secs: decay_hours * 3600,
        }
    }

    /// Check whether a user is within their free tier quota and increment
    /// the counter if so.
    ///
    /// Returns [`FreeTierCheck::Free`] if the user is under the limit
    /// (the counter is atomically incremented).  Returns
    /// [`FreeTierCheck::Exhausted`] if the user has already used all their
    /// free requests in the current window.
    ///
    /// If the user is not yet tracked or the previous window has expired
    /// (decay_hours passed), a fresh counter is started.
    pub fn check_and_increment(&self, pk: &str) -> FreeTierCheck {
        // Fast path: existing, non-expired entry
        if let Some(mut entry) = self.users.get_mut(pk) {
            let state = entry.value_mut();
            if state.first_request_at.elapsed().as_secs() < self.decay_secs {
                // Window still active — check quota
                let used = state.requests_used.fetch_add(1, Ordering::Relaxed);
                if used < self.max_free_requests {
                    return FreeTierCheck::Free;
                }
                // Already exhausted — decrement the add we just did
                // (or leave it, since they're already over)
                return FreeTierCheck::Exhausted {
                    used: used + 1,
                    limit: self.max_free_requests,
                };
            }
            // Window expired — fall through to create a fresh entry
            // Drop the mutable ref before removing/re-inserting
        }

        // Slow path: expired or new user — create a fresh state.
        // Use entry API to avoid TOCTOU races.
        let mut entry = self.users.entry(pk.to_string()).or_insert_with(|| {
            FreeTierState {
                requests_used: AtomicU64::new(1), // count this request
                first_request_at: Instant::now(),
            }
        });

        // If the entry already existed (expired window), reset it
        let used = entry.requests_used.load(Ordering::Relaxed);
        if entry.first_request_at.elapsed().as_secs() >= self.decay_secs {
            // Expired — reset
            entry.requests_used.store(1, Ordering::Relaxed);
            entry.first_request_at = Instant::now();
            return FreeTierCheck::Free;
        }

        // New entry or non-expired entry reached via or_insert
        if used < self.max_free_requests {
            FreeTierCheck::Free
        } else {
            FreeTierCheck::Exhausted {
                used,
                limit: self.max_free_requests,
            }
        }
    }

    /// Return the current usage for a user, or `None` if not tracked.
    pub fn get_usage(&self, pk: &str) -> Option<FreeTierUsage> {
        let entry = self.users.get(pk)?;
        let state = entry.value();
        let used = state.requests_used.load(Ordering::Relaxed);
        let elapsed_secs = state.first_request_at.elapsed().as_secs();
        let expired = elapsed_secs >= self.decay_secs;

        Some(FreeTierUsage {
            requests_used: if expired { 0 } else { used },
            max_requests: self.max_free_requests,
            exhausted: !expired && used >= self.max_free_requests,
            reset_in_secs: if expired {
                0
            } else {
                self.decay_secs.saturating_sub(elapsed_secs)
            },
        })
    }

    /// Remove expired entries from the map.
    ///
    /// Call this periodically (e.g. every 5 minutes) to prevent unbounded
    /// memory growth.
    pub fn cleanup(&self) {
        let before = self.users.len();
        self.users.retain(|_, state| {
            state.first_request_at.elapsed().as_secs() < self.decay_secs
        });
        let removed = before - self.users.len();
        if removed > 0 {
            debug!(removed, "Cleaned up expired free tier entries");
        }
    }

    /// Return the number of tracked users.
    pub fn len(&self) -> usize {
        self.users.len()
    }

    /// Return `true` if no users are tracked.
    pub fn is_empty(&self) -> bool {
        self.users.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_user_gets_free() {
        let tracker = FreeTierTracker::new(100, 24);
        let result = tracker.check_and_increment("user1");
        assert_eq!(result, FreeTierCheck::Free);
    }

    #[test]
    fn test_user_under_limit() {
        let tracker = FreeTierTracker::new(5, 24);
        for _ in 0..5 {
            assert_eq!(tracker.check_and_increment("user1"), FreeTierCheck::Free);
        }
    }

    #[test]
    fn test_user_at_limit_exhausted() {
        let tracker = FreeTierTracker::new(3, 24);
        // Use all 3
        for _ in 0..3 {
            assert_eq!(tracker.check_and_increment("user1"), FreeTierCheck::Free);
        }
        // 4th should be exhausted
        match tracker.check_and_increment("user1") {
            FreeTierCheck::Exhausted { used, limit } => {
                assert_eq!(used, 4);
                assert_eq!(limit, 3);
            }
            FreeTierCheck::Free => panic!("Expected Exhausted, got Free"),
        }
    }

    #[test]
    fn test_different_users_independent() {
        let tracker = FreeTierTracker::new(2, 24);
        // User A exhausts
        assert_eq!(tracker.check_and_increment("a"), FreeTierCheck::Free);
        assert_eq!(tracker.check_and_increment("a"), FreeTierCheck::Free);
        assert!(matches!(tracker.check_and_increment("a"), FreeTierCheck::Exhausted { .. }));

        // User B still has quota
        assert_eq!(tracker.check_and_increment("b"), FreeTierCheck::Free);
        assert_eq!(tracker.check_and_increment("b"), FreeTierCheck::Free);
    }

    #[test]
    fn test_get_usage_for_tracked_user() {
        let tracker = FreeTierTracker::new(10, 24);
        tracker.check_and_increment("user1");
        tracker.check_and_increment("user1");

        let usage = tracker.get_usage("user1").unwrap();
        assert_eq!(usage.requests_used, 2);
        assert_eq!(usage.max_requests, 10);
        assert!(!usage.exhausted);
        assert!(usage.reset_in_secs > 0);
    }

    #[test]
    fn test_get_usage_for_unknown_user() {
        let tracker = FreeTierTracker::new(10, 24);
        assert!(tracker.get_usage("unknown").is_none());
    }

    #[test]
    fn test_get_usage_exhausted() {
        let tracker = FreeTierTracker::new(2, 24);
        tracker.check_and_increment("user1");
        tracker.check_and_increment("user1");
        // Exhaust
        let _ = tracker.check_and_increment("user1");

        let usage = tracker.get_usage("user1").unwrap();
        assert!(usage.exhausted);
        assert!(usage.requests_used >= 2);
    }

    #[test]
    fn test_len_tracks_users() {
        let tracker = FreeTierTracker::new(10, 24);
        assert_eq!(tracker.len(), 0);
        tracker.check_and_increment("a");
        assert_eq!(tracker.len(), 1);
        tracker.check_and_increment("b");
        assert_eq!(tracker.len(), 2);
    }

    #[test]
    fn test_cleanup_removes_expired() {
        // Use a very short decay (1 second) so entries expire immediately
        let tracker = FreeTierTracker::new(10, 0); // 0 hours = instant expiry
        tracker.check_and_increment("user1");
        assert_eq!(tracker.len(), 1);

        // Wait a moment for the instant to elapse
        std::thread::sleep(std::time::Duration::from_millis(10));
        tracker.cleanup();
        assert_eq!(tracker.len(), 0);
    }

    #[test]
    fn test_cleanup_keeps_active() {
        let tracker = FreeTierTracker::new(10, 24); // 24 hours
        tracker.check_and_increment("user1");
        tracker.cleanup();
        assert_eq!(tracker.len(), 1);
    }

    #[test]
    fn test_is_empty() {
        let tracker = FreeTierTracker::new(10, 24);
        assert!(tracker.is_empty());
        tracker.check_and_increment("user1");
        assert!(!tracker.is_empty());
    }

    #[test]
    fn test_concurrent_increments() {
        use std::sync::Arc;
        use std::thread;

        let tracker = Arc::new(FreeTierTracker::new(200, 24));
        let mut handles = vec![];

        for _ in 0..10 {
            let t = tracker.clone();
            handles.push(thread::spawn(move || {
                let mut allowed = 0u64;
                for _ in 0..20 {
                    if matches!(t.check_and_increment("concurrent_user"), FreeTierCheck::Free) {
                        allowed += 1;
                    }
                }
                allowed
            }));
        }

        let mut total_allowed: u64 = 0;
        for h in handles {
            total_allowed += h.join().unwrap();
        }

        // Should have allowed exactly 200 requests (some threads may have
        // seen Exhausted when the counter hit the limit concurrently)
        assert!(
            total_allowed <= 200,
            "Expected at most 200 allowed, got {}",
            total_allowed
        );
        // At least 190 should succeed (some threads may see the counter
        // increment past the limit due to concurrent fetch_add)
        assert!(
            total_allowed >= 190,
            "Expected at least 190 allowed, got {}",
            total_allowed
        );

        // After all threads, usage should be at or over 200
        let usage = tracker.get_usage("concurrent_user").unwrap();
        assert!(usage.requests_used >= 200);
    }
}
