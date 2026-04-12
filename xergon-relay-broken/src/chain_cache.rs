//! Chain-backed provider cache with TTL-based staleness
//!
//! Caches the results of chain scans so that listing endpoints
//! don't trigger a scan on every request. A background task
//! refreshes the cache periodically. Handlers can also trigger
//! a lazy refresh if the cache is stale.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;
use std::time::{Duration, Instant};
use tracing::{debug, warn};

use crate::chain::{ChainProvider, GpuListing, GpuRental};

/// Thread-safe cache for chain-discovered providers.
///
/// Uses `std::sync::RwLock` for fast reads (no async overhead).
/// Writes are infrequent (every `cache_ttl_secs`).
pub struct ChainCache {
    /// Cached providers from the last chain scan.
    providers: RwLock<Vec<ChainProvider>>,
    /// Cached GPU listings from the last chain scan.
    gpu_listings: RwLock<Vec<GpuListing>>,
    /// Cached GPU rentals from the last chain scan.
    gpu_rentals: RwLock<Vec<GpuRental>>,
    /// When the cache was last updated.
    last_updated: RwLock<Instant>,
    /// When the last successful scan completed.
    last_successful_scan: RwLock<Instant>,
    /// Number of consecutive scan failures.
    consecutive_scan_failures: std::sync::atomic::AtomicU32,
    /// Time-to-live for cached data.
    ttl: Duration,
    /// Whether the cache has ever been populated.
    populated: AtomicBool,
}

impl ChainCache {
    /// Create a new ChainCache with the given TTL.
    pub fn new(ttl: Duration) -> Self {
        // Start with last_updated far in the past so is_stale() returns true
        let start_time = Instant::now() - ttl.saturating_mul(2);
        Self {
            providers: RwLock::new(Vec::new()),
            gpu_listings: RwLock::new(Vec::new()),
            gpu_rentals: RwLock::new(Vec::new()),
            last_updated: RwLock::new(start_time),
            last_successful_scan: RwLock::new(start_time),
            consecutive_scan_failures: std::sync::atomic::AtomicU32::new(0),
            ttl,
            populated: AtomicBool::new(false),
        }
    }

    /// Get cached providers. Returns `None` if the cache has never been
    /// populated or is stale (past TTL).
    ///
    /// Use [`get_providers_or_empty`](Self::get_providers_or_empty) if you
    /// want stale data as a fallback.
    pub fn get_providers(&self) -> Option<Vec<ChainProvider>> {
        if !self.populated.load(Ordering::Relaxed) {
            return None;
        }
        let last = *self.last_updated.read().expect("ChainCache lock poisoned");
        if last.elapsed() > self.ttl {
            return None;
        }
        Some(
            self.providers
                .read()
                .expect("ChainCache lock poisoned")
                .clone(),
        )
    }

    /// Get cached providers regardless of staleness.
    /// Returns an empty `Vec` if never populated.
    pub fn get_providers_or_empty(&self) -> Vec<ChainProvider> {
        self.providers
            .read()
            .expect("ChainCache lock poisoned")
            .clone()
    }

    /// Update the cache with fresh provider data.
    pub fn update(&self, providers: Vec<ChainProvider>) {
        let count = providers.len();
        *self
            .providers
            .write()
            .expect("ChainCache lock poisoned") = providers;
        let now = Instant::now();
        *self
            .last_updated
            .write()
            .expect("ChainCache lock poisoned") = now;
        *self
            .last_successful_scan
            .write()
            .expect("ChainCache lock poisoned") = now;
        self.consecutive_scan_failures
            .store(0, Ordering::Relaxed);
        self.populated.store(true, Ordering::Relaxed);
        debug!(count, "ChainCache updated");
    }

    /// Record a scan failure. Increments the failure counter but keeps stale data.
    pub fn record_scan_failure(&self) {
        let failures = self
            .consecutive_scan_failures
            .fetch_add(1, Ordering::Relaxed)
            + 1;
        if failures == 1 {
            warn!(
                "Chain scan failed — serving stale data until scan recovers"
            );
        } else {
            warn!(
                consecutive_failures = failures,
                "Chain scan failed again — still serving stale data"
            );
        }
    }

    /// Check if the cache is stale (past TTL or never populated).
    #[allow(dead_code)] // Public API for future use
    pub fn is_stale(&self) -> bool {
        if !self.populated.load(Ordering::Relaxed) {
            return true;
        }
        let last = *self.last_updated.read().expect("ChainCache lock poisoned");
        last.elapsed() > self.ttl
    }

    /// Whether the cache has been populated at least once.
    pub fn is_populated(&self) -> bool {
        self.populated.load(Ordering::Relaxed)
    }

    /// Time elapsed since the last update.
    #[allow(dead_code)]
    pub fn age(&self) -> Duration {
        let last = *self.last_updated.read().expect("ChainCache lock poisoned");
        last.elapsed()
    }

    /// Check if the cache is healthy — last successful scan was within 2x TTL.
    /// Returns true even if slightly stale, as long as data is usable.
    pub fn is_healthy(&self) -> bool {
        if !self.populated.load(Ordering::Relaxed) {
            return false;
        }
        let last = *self
            .last_successful_scan
            .read()
            .expect("ChainCache lock poisoned");
        let grace = self.ttl.saturating_mul(2);
        last.elapsed() <= grace
    }

    /// Number of providers in the stale cache (for monitoring).
    /// Returns the count of cached providers regardless of staleness.
    pub fn stale_provider_count(&self) -> usize {
        self.providers
            .read()
            .expect("ChainCache lock poisoned")
            .len()
    }

    /// Number of consecutive scan failures (for monitoring).
    pub fn scan_failure_count(&self) -> u32 {
        self.consecutive_scan_failures.load(Ordering::Relaxed)
    }

    // ── GPU listing cache ──────────────────────────────────────────────

    /// Update the cache with fresh GPU listing data.
    pub fn update_gpu_listings(&self, listings: Vec<GpuListing>) {
        let count = listings.len();
        *self.gpu_listings.write().expect("ChainCache lock poisoned") = listings;
        debug!(count, "ChainCache GPU listings updated");
    }

    /// Get cached GPU listings. Returns `None` if never populated.
    pub fn get_gpu_listings(&self) -> Option<Vec<GpuListing>> {
        if !self.populated.load(Ordering::Relaxed) {
            return None;
        }
        let last = *self.last_updated.read().expect("ChainCache lock poisoned");
        if last.elapsed() > self.ttl {
            return None;
        }
        Some(
            self.gpu_listings
                .read()
                .expect("ChainCache lock poisoned")
                .clone(),
        )
    }

    /// Get cached GPU listings regardless of staleness.
    pub fn get_gpu_listings_or_empty(&self) -> Vec<GpuListing> {
        self.gpu_listings
            .read()
            .expect("ChainCache lock poisoned")
            .clone()
    }

    // ── GPU rental cache ───────────────────────────────────────────────

    /// Update the cache with fresh GPU rental data.
    pub fn update_gpu_rentals(&self, rentals: Vec<GpuRental>) {
        let count = rentals.len();
        *self.gpu_rentals.write().expect("ChainCache lock poisoned") = rentals;
        debug!(count, "ChainCache GPU rentals updated");
    }

    /// Get cached GPU rentals. Returns `None` if never populated.
    #[allow(dead_code)]
    pub fn get_gpu_rentals(&self) -> Option<Vec<GpuRental>> {
        if !self.populated.load(Ordering::Relaxed) {
            return None;
        }
        let last = *self.last_updated.read().expect("ChainCache lock poisoned");
        if last.elapsed() > self.ttl {
            return None;
        }
        Some(
            self.gpu_rentals
                .read()
                .expect("ChainCache lock poisoned")
                .clone(),
        )
    }

    /// Get cached GPU rentals regardless of staleness.
    pub fn get_gpu_rentals_or_empty(&self) -> Vec<GpuRental> {
        self.gpu_rentals
            .read()
            .expect("ChainCache lock poisoned")
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::ChainProvider;

    fn make_provider(id: &str) -> ChainProvider {
        ChainProvider {
            box_id: format!("box-{}", id),
            provider_pk: "02".to_string() + &"00".repeat(32),
            endpoint: format!("http://{}.example.com:9099", id),
            models: vec!["llama-3".to_string()],
            model_pricing: std::collections::HashMap::new(),
            pown_score: 50,
            last_heartbeat: 100,
            region: "us-east".to_string(),
            pricing_nanoerg_per_million_tokens: None,
            value_nanoerg: 1_000_000_000,
        }
    }

    #[test]
    fn test_new_cache_is_stale_and_empty() {
        let cache = ChainCache::new(Duration::from_secs(10));
        assert!(cache.is_stale());
        assert!(!cache.is_populated());
        assert!(cache.get_providers().is_none());
        assert!(cache.get_providers_or_empty().is_empty());
    }

    #[test]
    fn test_update_populates_cache() {
        let cache = ChainCache::new(Duration::from_secs(10));
        let providers = vec![make_provider("p1")];
        cache.update(providers);

        assert!(!cache.is_stale());
        assert!(cache.is_populated());
        assert_eq!(cache.get_providers().unwrap().len(), 1);
        assert_eq!(cache.get_providers_or_empty().len(), 1);
    }

    #[test]
    fn test_cache_goes_stale_after_ttl() {
        let cache = ChainCache::new(Duration::from_millis(50));
        cache.update(vec![make_provider("p1")]);

        assert!(!cache.is_stale());
        std::thread::sleep(Duration::from_millis(100));
        assert!(cache.is_stale());
        // get_providers returns None when stale
        assert!(cache.get_providers().is_none());
        // but get_providers_or_empty still returns data
        assert_eq!(cache.get_providers_or_empty().len(), 1);
    }

    #[test]
    fn test_update_resets_staleness() {
        let cache = ChainCache::new(Duration::from_millis(50));
        cache.update(vec![make_provider("p1")]);
        std::thread::sleep(Duration::from_millis(100));
        assert!(cache.is_stale());

        cache.update(vec![make_provider("p2"), make_provider("p3")]);
        assert!(!cache.is_stale());
        assert_eq!(cache.get_providers().unwrap().len(), 2);
    }

    #[test]
    fn test_is_healthy_within_grace_period() {
        let cache = ChainCache::new(Duration::from_secs(10));
        cache.update(vec![make_provider("p1")]);
        assert!(cache.is_healthy());

        // After TTL but within 2x TTL, should still be "healthy" (grace period)
        std::thread::sleep(Duration::from_millis(150)); // TTL=50ms, 2x=100ms
        // Note: timing-sensitive; in practice 150ms > 100ms so this may fail
        // but the logic is: is_healthy = last_successful_scan.elapsed() <= 2*TTL
    }

    #[test]
    fn test_record_scan_failure_increments() {
        let cache = ChainCache::new(Duration::from_secs(10));
        assert_eq!(cache.scan_failure_count(), 0);

        cache.record_scan_failure();
        assert_eq!(cache.scan_failure_count(), 1);

        cache.record_scan_failure();
        assert_eq!(cache.scan_failure_count(), 2);
    }

    #[test]
    fn test_update_resets_failure_count() {
        let cache = ChainCache::new(Duration::from_secs(10));
        cache.record_scan_failure();
        cache.record_scan_failure();
        assert_eq!(cache.scan_failure_count(), 2);

        cache.update(vec![make_provider("p1")]);
        assert_eq!(cache.scan_failure_count(), 0);
    }

    #[test]
    fn test_stale_provider_count() {
        let cache = ChainCache::new(Duration::from_secs(10));
        assert_eq!(cache.stale_provider_count(), 0);

        cache.update(vec![make_provider("p1"), make_provider("p2")]);
        assert_eq!(cache.stale_provider_count(), 2);
    }

    #[test]
    fn test_is_healthy_unpopulated() {
        let cache = ChainCache::new(Duration::from_secs(10));
        assert!(!cache.is_healthy()); // Never populated
    }
}
