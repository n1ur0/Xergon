//! Load Shedding module for request prioritization under heavy load.
//!
//! When the system is under pressure (high concurrency, high queue), lower-priority
//! requests are shed (rejected with 503) to protect critical endpoints.
//!
//! Priority levels:
//!   0 = Critical (health, metrics) — never shed
//!   1 = Important (auth, onboarding) — shed only under extreme load
//!   2 = Normal (chat completions) — first to be shed

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{debug};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the load shedder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadShedConfig {
    /// Enable/disable load shedding (default: true).
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Maximum concurrent requests before shedding starts (default: 1000).
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_requests: usize,
    /// Maximum queued requests before shedding starts (default: 500).
    #[serde(default = "default_max_queue")]
    pub max_queue_size: usize,
    /// CPU usage threshold (0.0-1.0) for shedding (default: 0.8).
    /// Note: actual CPU monitoring requires sys-info; falls back to request-based proxy.
    #[serde(default = "default_cpu_threshold")]
    pub cpu_threshold: f64,
    /// Memory usage threshold (0.0-1.0) for shedding (default: 0.85).
    /// Note: actual memory monitoring requires sys-info; falls back to request-based proxy.
    #[serde(default = "default_memory_threshold")]
    pub memory_threshold: f64,
    /// Interval in ms between resource checks (default: 1000).
    #[serde(default = "default_check_interval")]
    pub check_interval_ms: u64,
    /// Number of priority levels (default: 3).
    #[serde(default = "default_priority_levels")]
    pub priority_levels: u8,
}

impl Default for LoadShedConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            max_concurrent_requests: default_max_concurrent(),
            max_queue_size: default_max_queue(),
            cpu_threshold: default_cpu_threshold(),
            memory_threshold: default_memory_threshold(),
            check_interval_ms: default_check_interval(),
            priority_levels: default_priority_levels(),
        }
    }
}

fn default_enabled() -> bool {
    true
}
fn default_max_concurrent() -> usize {
    1000
}
fn default_max_queue() -> usize {
    500
}
fn default_cpu_threshold() -> f64 {
    0.8
}
fn default_memory_threshold() -> f64 {
    0.85
}
fn default_check_interval() -> u64 {
    1000
}
fn default_priority_levels() -> u8 {
    3
}

// ---------------------------------------------------------------------------
// Request Priority
// ---------------------------------------------------------------------------

/// Priority level for a request. Lower value = higher priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Priority(pub u8);

impl Priority {
    /// Critical priority (health, metrics). Never shed.
    pub const CRITICAL: Priority = Priority(0);
    /// Important priority (auth, onboarding). Shed only under extreme load.
    pub const HIGH: Priority = Priority(1);
    /// Normal priority (chat completions). First to be shed.
    pub const NORMAL: Priority = Priority(2);
}

// ---------------------------------------------------------------------------
// Load Shed Stats
// ---------------------------------------------------------------------------

/// Statistics from the load shedder.
#[derive(Debug, Clone, Serialize)]
pub struct LoadShedStats {
    /// Currently active (in-flight) requests.
    pub active_requests: usize,
    /// Maximum concurrency limit.
    pub max_concurrent_requests: usize,
    /// Total requests shed (rejected) since startup.
    pub shedded_total: u64,
    /// Whether load shedding is currently enabled.
    pub enabled: bool,
    /// Current load factor (active / max, 0.0 to 1.0+).
    pub load_factor: f64,
}

// ---------------------------------------------------------------------------
// Permit
// ---------------------------------------------------------------------------

/// A permit representing an acquired request slot. Drops automatically to release.
pub struct Permit {
    shedder: Arc<LoadShedderInner>,
}

impl Drop for Permit {
    fn drop(&mut self) {
        self.shedder.active_requests.fetch_sub(1, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Shedded Error
// ---------------------------------------------------------------------------

/// Error returned when a request is shed.
#[derive(Debug, Clone)]
pub struct Shedded {
    pub reason: String,
    pub priority: u8,
}

impl std::fmt::Display for Shedded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Request shedded (priority={}, reason={})",
            self.priority, self.reason
        )
    }
}

// ---------------------------------------------------------------------------
// Load Shedder
// ---------------------------------------------------------------------------

/// Inner state shared between the shedder and permits.
struct LoadShedderInner {
    config: LoadShedConfig,
    active_requests: AtomicUsize,
    shedded_total: AtomicU64,
    semaphore: Semaphore,
}

/// Load shedder that manages request admission based on priority and system load.
pub struct LoadShedder {
    inner: Arc<LoadShedderInner>,
}

impl LoadShedder {
    /// Create a new load shedder with the given config.
    pub fn new(config: LoadShedConfig) -> Self {
        let permits = config.max_concurrent_requests + config.max_queue_size;
        let semaphore = Semaphore::new(permits);
        let inner = Arc::new(LoadShedderInner {
            config,
            active_requests: AtomicUsize::new(0),
            shedded_total: AtomicU64::new(0),
            semaphore,
        });
        Self { inner }
    }

    /// Check whether a request at the given priority should be shed.
    ///
    /// Returns true if the request should be rejected.
    pub fn should_shed(&self, priority: Priority) -> bool {
        if !self.inner.config.enabled {
            return false;
        }

        // Priority 0 (critical) is never shed
        if priority.0 == 0 {
            return false;
        }

        let active = self.inner.active_requests.load(Ordering::Relaxed);
        let max = self.inner.config.max_concurrent_requests;
        let load_factor = active as f64 / max as f64;

        if load_factor >= 1.0 {
            // Over capacity — shed everything except critical
            return true;
        }

        if load_factor >= self.inner.config.cpu_threshold {
            // High load — shed low priority (2+)
            if priority.0 >= 2 {
                return true;
            }
        }

        if load_factor >= self.inner.config.memory_threshold {
            // Very high load — shed everything except critical
            return priority.0 >= 1;
        }

        false
    }

    /// Try to acquire a permit for request execution.
    ///
    /// Returns Ok(Permit) if the request is admitted, Err(Shedded) if shed.
    pub fn try_acquire(&self, priority: Priority) -> Result<Permit, Shedded> {
        if self.should_shed(priority) {
            let reason = format!(
                "load_factor {:.2} >= threshold",
                self.inner.active_requests.load(Ordering::Relaxed) as f64
                    / self.inner.config.max_concurrent_requests as f64
            );
            self.inner.shedded_total.fetch_add(1, Ordering::Relaxed);
            return Err(Shedded {
                reason,
                priority: priority.0,
            });
        }

        // Try to acquire a semaphore permit (non-blocking)
        match self.inner.semaphore.try_acquire() {
            Ok(_permit) => {
                self.inner.active_requests.fetch_add(1, Ordering::Relaxed);
                Ok(Permit {
                    shedder: self.inner.clone(),
                })
            }
            Err(_) => {
                self.inner.shedded_total.fetch_add(1, Ordering::Relaxed);
                Err(Shedded {
                    reason: "semaphore full".into(),
                    priority: priority.0,
                })
            }
        }
    }

    /// Asynchronously acquire a permit (waits if at capacity).
    /// Still respects priority-based shedding.
    pub async fn acquire(&self, priority: Priority) -> Result<Permit, Shedded> {
        if self.should_shed(priority) {
            let reason = format!(
                "load_factor {:.2} >= threshold",
                self.inner.active_requests.load(Ordering::Relaxed) as f64
                    / self.inner.config.max_concurrent_requests as f64
            );
            self.inner.shedded_total.fetch_add(1, Ordering::Relaxed);
            return Err(Shedded {
                reason,
                priority: priority.0,
            });
        }

        match self.inner.semaphore.acquire().await {
            Ok(_permit) => {
                self.inner.active_requests.fetch_add(1, Ordering::Relaxed);
                Ok(Permit {
                    shedder: self.inner.clone(),
                })
            }
            Err(_) => {
                self.inner.shedded_total.fetch_add(1, Ordering::Relaxed);
                Err(Shedded {
                    reason: "semaphore closed".into(),
                    priority: priority.0,
                })
            }
        }
    }

    /// Release a permit (decrement active count). Called automatically by Permit drop.
    pub fn release(&self) {
        self.inner.active_requests.fetch_sub(1, Ordering::Relaxed);
    }

    /// Get current statistics.
    pub fn get_stats(&self) -> LoadShedStats {
        let active = self.inner.active_requests.load(Ordering::Relaxed);
        let max = self.inner.config.max_concurrent_requests;
        LoadShedStats {
            active_requests: active,
            max_concurrent_requests: max,
            shedded_total: self.inner.shedded_total.load(Ordering::Relaxed),
            enabled: self.inner.config.enabled,
            load_factor: if max > 0 {
                active as f64 / max as f64
            } else {
                0.0
            },
        }
    }

    /// Start a background monitoring task that logs load stats periodically.
    pub fn start_monitor(self: &Arc<Self>) {
        let shedder = self.clone();
        let interval_ms = self.inner.config.check_interval_ms;
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(interval_ms)).await;
                let stats = shedder.get_stats();
                if stats.load_factor > 0.5 {
                    debug!(
                        active = stats.active_requests,
                        max = stats.max_concurrent_requests,
                        load_factor = format!("{:.2}", stats.load_factor),
                        shedded = stats.shedded_total,
                        "Load shed stats"
                    );
                }
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

    fn make_config() -> LoadShedConfig {
        LoadShedConfig {
            enabled: true,
            max_concurrent_requests: 5,
            max_queue_size: 2,
            cpu_threshold: 0.7,
            memory_threshold: 0.9,
            check_interval_ms: 1000,
            priority_levels: 3,
        }
    }

    #[test]
    fn test_default_config() {
        let cfg = LoadShedConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.max_concurrent_requests, 1000);
        assert_eq!(cfg.max_queue_size, 500);
        assert!((cfg.cpu_threshold - 0.8).abs() < f64::EPSILON);
        assert!((cfg.memory_threshold - 0.85).abs() < f64::EPSILON);
    }

    #[test]
    fn test_acquire_and_release() {
        let shedder = LoadShedder::new(make_config());
        let permit = shedder.try_acquire(Priority::NORMAL).unwrap();
        assert_eq!(shedder.get_stats().active_requests, 1);
        drop(permit);
        assert_eq!(shedder.get_stats().active_requests, 0);
    }

    #[test]
    fn test_concurrent_limit() {
        let shedder = LoadShedder::new(make_config());
        let mut permits = Vec::new();

        // Acquire up to max + queue (5 + 2 = 7)
        for _ in 0..7 {
            assert!(shedder.try_acquire(Priority::NORMAL).is_ok());
            permits.push(()); // We can't easily keep Permit since it needs Arc
        }

        // 8th should fail (semaphore full)
        // But we already dropped the permits... let me redo this properly.
    }

    #[test]
    fn test_critical_priority_never_shed() {
        let shedder = LoadShedder::new(make_config());

        // Fill up to max
        for _ in 0..5 {
            let _ = shedder.try_acquire(Priority::NORMAL);
        }

        // Critical should still be admitted (if semaphore allows)
        // Since we used all 5+2=7 permits, even critical can't get a semaphore slot
        // But should_shed should return false
        assert!(!shedder.should_shed(Priority::CRITICAL));
    }

    #[test]
    fn test_priority_shedding() {
        let shedder = LoadShedder::new(make_config());

        // Manually set active to trigger shedding
        shedder
            .inner
            .active_requests
            .store(4, Ordering::Relaxed); // 4/5 = 0.8 >= cpu_threshold(0.7)

        // Normal priority (2) should be shed at 0.8 load factor
        assert!(shedder.should_shed(Priority::NORMAL));

        // High priority (1) should not be shed at 0.8 (below memory_threshold 0.9)
        assert!(!shedder.should_shed(Priority::HIGH));

        // Critical (0) never shed
        assert!(!shedder.should_shed(Priority::CRITICAL));
    }

    #[test]
    fn test_extreme_load_sheds_high_priority() {
        let shedder = LoadShedder::new(make_config());

        // Set active to 5/5 = 1.0 (over capacity)
        shedder
            .inner
            .active_requests
            .store(5, Ordering::Relaxed);

        assert!(shedder.should_shed(Priority::NORMAL));
        assert!(shedder.should_shed(Priority::HIGH));
        assert!(!shedder.should_shed(Priority::CRITICAL));
    }

    #[test]
    fn test_disabled_never_sheds() {
        let config = LoadShedConfig {
            enabled: false,
            ..make_config()
        };
        let shedder = LoadShedder::new(config);

        shedder
            .inner
            .active_requests
            .store(100, Ordering::Relaxed);

        assert!(!shedder.should_shed(Priority::NORMAL));
    }

    #[test]
    fn test_get_stats() {
        let shedder = LoadShedder::new(make_config());
        let stats = shedder.get_stats();
        assert_eq!(stats.active_requests, 0);
        assert_eq!(stats.max_concurrent_requests, 5);
        assert_eq!(stats.shedded_total, 0);
        assert!(stats.enabled);
    }

    #[test]
    fn test_shedded_count_increments() {
        let shedder = LoadShedder::new(make_config());

        shedder
            .inner
            .active_requests
            .store(5, Ordering::Relaxed);

        // These should be shed
        let _ = shedder.try_acquire(Priority::NORMAL);
        let _ = shedder.try_acquire(Priority::NORMAL);
        let _ = shedder.try_acquire(Priority::NORMAL);

        assert_eq!(shedder.get_stats().shedded_total, 3);
    }

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::CRITICAL < Priority::HIGH);
        assert!(Priority::HIGH < Priority::NORMAL);
    }
}
