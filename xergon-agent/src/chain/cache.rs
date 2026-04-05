//! In-memory cache for chain state.
//!
//! Uses DashMap for lock-free concurrent reads and tokio::sync::RwLock
//! for the block height. Cached entries have a configurable TTL.

use std::time::{Duration, Instant};

use dashmap::DashMap;
use tokio::sync::RwLock;

use crate::chain::types::*;

/// Time-to-live for cached provider state.
const PROVIDER_CACHE_TTL: Duration = Duration::from_secs(30);
/// Time-to-live for cached user balance.
const USER_CACHE_TTL: Duration = Duration::from_secs(60);

/// Cached provider entry.
struct CachedProvider {
    provider: ProviderBox,
    fetched_at: Instant,
}

/// Cached user balance entry.
struct CachedBalance {
    balance_nanoerg: u64,
    box_id: String,
    fetched_at: Instant,
}

/// In-memory cache for Xergon chain state.
///
/// Thread-safe and async-friendly. All lookups are O(1).
pub struct ChainCache {
    /// Provider boxes keyed by NFT token ID.
    providers: DashMap<String, CachedProvider>,
    /// User balances keyed by public key hex.
    user_balances: DashMap<String, CachedBalance>,
    /// Current best block height.
    current_height: RwLock<i32>,
}

impl ChainCache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            providers: DashMap::new(),
            user_balances: DashMap::new(),
            current_height: RwLock::new(0),
        }
    }

    // -----------------------------------------------------------------------
    // Provider cache
    // -----------------------------------------------------------------------

    /// Get a cached provider by NFT ID. Returns `None` if not present or expired.
    pub fn get_provider(&self, nft_id: &str) -> Option<ProviderBox> {
        self.providers
            .get(nft_id)
            .and_then(|entry| {
                if entry.fetched_at.elapsed() < PROVIDER_CACHE_TTL {
                    Some(entry.provider.clone())
                } else {
                    None
                }
            })
    }

    /// Bulk-set provider boxes (replaces all existing entries).
    pub fn set_providers(&self, providers: Vec<ProviderBox>) {
        // Clear stale entries first
        self.providers.retain(|_, v| v.fetched_at.elapsed() < PROVIDER_CACHE_TTL);

        // Insert fresh entries
        for pb in providers {
            let nft_id = pb.provider_nft_id.clone();
            self.providers.insert(
                nft_id,
                CachedProvider {
                    provider: pb,
                    fetched_at: Instant::now(),
                },
            );
        }
    }

    /// Get all non-expired provider boxes.
    pub fn get_all_providers(&self) -> Vec<ProviderBox> {
        self.providers
            .iter()
            .filter(|entry| entry.fetched_at.elapsed() < PROVIDER_CACHE_TTL)
            .map(|entry| entry.provider.clone())
            .collect()
    }

    /// Get the number of cached (non-expired) providers.
    pub fn provider_count(&self) -> usize {
        self.providers
            .iter()
            .filter(|entry| entry.fetched_at.elapsed() < PROVIDER_CACHE_TTL)
            .count()
    }

    // -----------------------------------------------------------------------
    // User balance cache
    // -----------------------------------------------------------------------

    /// Get a cached user balance by public key hex.
    /// Returns `(balance_nanoerg, box_id)` or `None` if not present/expired.
    pub fn get_user_balance(&self, pk_hex: &str) -> Option<(u64, String)> {
        self.user_balances
            .get(pk_hex)
            .and_then(|entry| {
                if entry.fetched_at.elapsed() < USER_CACHE_TTL {
                    Some((entry.balance_nanoerg, entry.box_id.clone()))
                } else {
                    None
                }
            })
    }

    /// Set a user balance in the cache.
    pub fn set_user_balance(&self, pk_hex: &str, balance: u64, box_id: String) {
        self.user_balances.insert(
            pk_hex.to_string(),
            CachedBalance {
                balance_nanoerg: balance,
                box_id,
                fetched_at: Instant::now(),
            },
        );
    }

    // -----------------------------------------------------------------------
    // Block height
    // -----------------------------------------------------------------------

    /// Set the current best block height.
    pub async fn set_height(&self, height: i32) {
        let mut guard = self.current_height.write().await;
        *guard = height;
    }

    /// Get the cached block height.
    pub async fn get_height(&self) -> i32 {
        let guard = self.current_height.read().await;
        *guard
    }

    // -----------------------------------------------------------------------
    // Maintenance
    // -----------------------------------------------------------------------

    /// Remove all expired entries from the cache.
    pub fn evict_expired(&self) {
        self.providers
            .retain(|_, v| v.fetched_at.elapsed() < PROVIDER_CACHE_TTL);
        self.user_balances
            .retain(|_, v| v.fetched_at.elapsed() < USER_CACHE_TTL);
    }

    /// Clear all cached data.
    pub fn clear(&self) {
        self.providers.clear();
        self.user_balances.clear();
    }
}

impl Default for ChainCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_provider(nft_id: &str) -> ProviderBox {
        ProviderBox {
            box_id: "testbox".to_string(),
            tx_id: "testtx".to_string(),
            provider_nft_id: nft_id.to_string(),
            provider_pk: "02abcdef".to_string(),
            endpoint: "http://localhost:9099".to_string(),
            models: vec!["llama3".to_string()],
            model_pricing: std::collections::HashMap::new(),
            pown_score: 500,
            last_heartbeat: 1000,
            region: "us-east".to_string(),
            value: "1000000000".to_string(),
            creation_height: 900,
            is_active: true,
        }
    }

    #[tokio::test]
    async fn test_provider_cache_basic() {
        let cache = ChainCache::new();

        // Empty cache
        assert!(cache.get_provider("nft1").is_none());
        assert!(cache.get_all_providers().is_empty());

        // Insert and retrieve
        let providers = vec![make_provider("nft1"), make_provider("nft2")];
        cache.set_providers(providers);

        assert!(cache.get_provider("nft1").is_some());
        assert!(cache.get_provider("nft2").is_some());
        assert_eq!(cache.provider_count(), 2);

        let all = cache.get_all_providers();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn test_user_balance_cache() {
        let cache = ChainCache::new();

        assert!(cache.get_user_balance("pk1").is_none());

        cache.set_user_balance("pk1", 1_000_000_000, "box1".to_string());

        let (balance, box_id) = cache.get_user_balance("pk1").unwrap();
        assert_eq!(balance, 1_000_000_000);
        assert_eq!(box_id, "box1");
    }

    #[tokio::test]
    async fn test_height_cache() {
        let cache = ChainCache::new();

        assert_eq!(cache.get_height().await, 0);

        cache.set_height(12345).await;
        assert_eq!(cache.get_height().await, 12345);
    }

    #[test]
    fn test_evict_and_clear() {
        let cache = ChainCache::new();

        cache.set_providers(vec![make_provider("nft1")]);
        cache.set_user_balance("pk1", 100, "box1".to_string());

        assert_eq!(cache.provider_count(), 1);
        assert!(cache.get_user_balance("pk1").is_some());

        cache.clear();

        assert_eq!(cache.provider_count(), 0);
        assert!(cache.get_user_balance("pk1").is_none());
    }
}
