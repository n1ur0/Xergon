//! Rate limiting — supports both IP-based (anonymous) and user-based (authenticated) limits.
//!
//! Tracks requests per key per time window using a sliding window.
//! In-memory only — suitable for single-instance deployment.
//! For multi-instance, use Redis or similar.
//!
//! Tiers:
//!   anonymous: 10 requests/day per IP
//!   free:      10 requests/day per user
//!   pro:       10,000 requests/30 days per user

use dashmap::DashMap;
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// Rate limit entry for a single key
#[derive(Debug)]
struct RateLimitEntry {
    /// Number of requests in current window
    count: u32,
    /// When this entry was created (start of window)
    window_start: Instant,
    /// Window duration for this entry
    window_duration: Duration,
}

/// In-memory rate limiter supporting both anonymous and authenticated users
pub struct RateLimiter {
    /// key -> rate limit entry (key = IP for anon, "user:{id}" for auth)
    entries: DashMap<String, RateLimitEntry>,
    /// Max requests per day for anonymous users
    max_anon_per_day: u32,
    /// Default window duration (1 day) for anonymous requests
    default_window_duration: Duration,
}

impl RateLimiter {
    pub fn new(max_per_day: u32) -> Self {
        Self {
            entries: DashMap::new(),
            max_anon_per_day: max_per_day,
            default_window_duration: Duration::from_secs(24 * 60 * 60),
        }
    }

    /// Check if an IP is allowed to make a request (anonymous).
    /// Returns (allowed, remaining_count).
    pub fn check(&self, ip: &str) -> (bool, u32) {
        self.check_with_window(ip, self.max_anon_per_day, self.default_window_duration.as_secs())
    }

    /// Check if a key is allowed to make a request with custom window.
    /// Used for tier-based rate limiting (e.g. pro users get 10K/30days).
    /// Returns (allowed, remaining_count).
    pub fn check_with_window(&self, key: &str, max_requests: u32, window_secs: u64) -> (bool, u32) {
        let now = Instant::now();
        let window_duration = Duration::from_secs(window_secs);

        let mut entry = self.entries.entry(key.to_string()).or_insert_with(|| {
            info!(key, "New rate limit entry created");
            RateLimitEntry {
                count: 0,
                window_start: now,
                window_duration,
            }
        });

        // Check if window has expired
        if now.duration_since(entry.window_start) >= entry.window_duration {
            info!(key, old_count = entry.count, "Rate limit window expired, resetting");
            entry.count = 0;
            entry.window_start = now;
            entry.window_duration = window_duration;
        }

        if entry.count >= max_requests {
            let remaining = 0;
            warn!(key, count = entry.count, max = max_requests, "Rate limit exceeded");
            (false, remaining)
        } else {
            entry.count += 1;
            let remaining = max_requests - entry.count;
            (true, remaining)
        }
    }

    /// Get remaining count for a key without consuming a request (anonymous)
    #[allow(dead_code)] // TODO: will be used for rate limit info endpoint
    pub fn remaining(&self, ip: &str) -> u32 {
        let now = Instant::now();
        if let Some(mut entry) = self.entries.get_mut(ip) {
            if now.duration_since(entry.window_start) >= entry.window_duration {
                entry.count = 0;
                entry.window_start = now;
            }
            self.max_anon_per_day.saturating_sub(entry.count)
        } else {
            self.max_anon_per_day
        }
    }

    /// Get remaining count for a key with custom window (authenticated)
    #[allow(dead_code)] // TODO: will be used for rate limit info endpoint
    pub fn remaining_with_window(&self, key: &str, max_requests: u32, window_secs: u64) -> u32 {
        let now = Instant::now();
        if let Some(mut entry) = self.entries.get_mut(key) {
            if now.duration_since(entry.window_start) >= entry.window_duration {
                entry.count = 0;
                entry.window_start = now;
                entry.window_duration = Duration::from_secs(window_secs);
            }
            max_requests.saturating_sub(entry.count)
        } else {
            max_requests
        }
    }

    /// Periodic cleanup of expired entries
    pub fn cleanup(&self) {
        let now = Instant::now();
        let before = self.entries.len();
        self.entries.retain(|key, entry| {
            if now.duration_since(entry.window_start) >= entry.window_duration {
                info!(key, "Cleaning up expired rate limit entry");
                false
            } else {
                true
            }
        });
        let after = self.entries.len();
        if before != after {
            info!(removed = before - after, remaining = after, "Rate limit cleanup completed");
        }
    }
}
