//! Model Registry
//!
//! Maintains an aggregated, thread-safe view of all models available across
//! all providers. Allows clients to discover models without querying each
//! provider individually.
//!
//! Data is synced from two sources:
//! 1. Chain scanner (on-chain provider metadata with pricing)
//! 2. Health polling (live /xergon/status responses)
//!
//! Stale entries (models not seen within a timeout) are pruned periodically.

use dashmap::DashMap;
use serde::Serialize;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Core data types
// ---------------------------------------------------------------------------

/// A single model entry for a specific provider.
#[derive(Debug, Clone)]
pub struct ModelEntry {
    /// Unique key: "{model_id}|{provider_pk}"
    pub key: String,
    pub model_id: String,
    pub provider_pk: String,
    pub provider_endpoint: String,
    pub context_length: u32,
    pub pricing_nanoerg_per_million_tokens: u64,
    pub is_available: bool,
    pub last_seen: Instant,
}

/// Input for syncing model info from a provider.
#[derive(Debug, Clone)]
pub struct SyncModelInfo {
    pub model_id: String,
    pub context_length: u32,
    pub pricing_nanoerg_per_million_tokens: u64,
}

/// Summary of a model across all providers (for GET /v1/models).
#[derive(Debug, Clone, Serialize)]
pub struct ModelSummary {
    pub model_id: String,
    pub available_providers: usize,
    pub cheapest_price_nanoerg_per_million_tokens: u64,
    pub max_context_length: u32,
}

/// Provider entry for a specific model (returned by get_providers_for_model).
#[derive(Debug, Clone, Serialize)]
pub struct ProviderEntry {
    pub provider_pk: String,
    pub provider_endpoint: String,
    pub pricing_nanoerg_per_million_tokens: u64,
    pub context_length: u32,
    pub is_available: bool,
}

/// Detail info for a specific model.
#[derive(Debug, Clone, Serialize)]
pub struct ModelDetail {
    pub model_id: String,
    pub providers: Vec<ProviderEntry>,
    pub available_providers: usize,
    pub cheapest_price_nanoerg_per_million_tokens: u64,
    pub max_context_length: u32,
}

// ---------------------------------------------------------------------------
// ModelRegistry
// ---------------------------------------------------------------------------

/// Thread-safe registry of all models across all providers.
pub struct ModelRegistry {
    /// All model entries keyed by "{model_id}|{provider_pk}" (lowercased).
    pub(crate) entries: DashMap<String, ModelEntry>,
}

impl ModelRegistry {
    /// Create a new empty model registry.
    pub fn new() -> Self {
        Self {
            entries: DashMap::new(),
        }
    }

    /// Sync model entries for a specific provider.
    ///
    /// - Updates existing entries for this provider (refreshes `last_seen`, pricing, etc.)
    /// - Adds new entries for models not yet tracked for this provider
    /// - Does NOT remove entries for models no longer reported by this provider
    ///   (those are cleaned up by `prune_stale_models`)
    pub fn sync_from_provider(
        &self,
        provider_pk: &str,
        provider_endpoint: &str,
        models: Vec<SyncModelInfo>,
    ) {
        let pk_lower = provider_pk.to_lowercase();
        let now = Instant::now();

        // Track which model IDs were seen in this sync
        let mut seen_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

        for model in &models {
            let model_lower = model.model_id.to_lowercase();
            let key = format!("{}|{}", model_lower, pk_lower);
            seen_keys.insert(key.clone());

            let entry = ModelEntry {
                key: key.clone(),
                model_id: model_lower,
                provider_pk: pk_lower.clone(),
                provider_endpoint: provider_endpoint.to_string(),
                context_length: model.context_length,
                pricing_nanoerg_per_million_tokens: model.pricing_nanoerg_per_million_tokens,
                is_available: true,
                last_seen: now,
            };

            self.entries.insert(key, entry);
        }
    }

    /// Get all unique models with aggregated summary info.
    /// Returns one summary per model, sorted by model_id.
    pub fn get_all_models(&self) -> Vec<ModelSummary> {
        let mut model_map: std::collections::HashMap<String, (usize, u64, u32)> =
            std::collections::HashMap::new();

        for entry in self.entries.iter() {
            let e = entry.value();
            let data = model_map.entry(e.model_id.clone()).or_insert((0, u64::MAX, 0));
            if e.is_available {
                data.0 += 1;
            }
            if e.pricing_nanoerg_per_million_tokens < data.1 {
                data.1 = e.pricing_nanoerg_per_million_tokens;
            }
            if e.context_length > data.2 {
                data.2 = e.context_length;
            }
        }

        // Fix up u64::MAX for models with no pricing (they're free)
        let mut summaries: Vec<ModelSummary> = model_map
            .into_iter()
            .map(|(model_id, (count, price, ctx))| ModelSummary {
                model_id,
                available_providers: count,
                cheapest_price_nanoerg_per_million_tokens: if price == u64::MAX {
                    0
                } else {
                    price
                },
                max_context_length: ctx,
            })
            .collect();

        summaries.sort_by(|a, b| a.model_id.cmp(&b.model_id));
        summaries
    }

    /// Get all providers that serve a specific model.
    pub fn get_providers_for_model(&self, model_id: &str) -> Vec<ProviderEntry> {
        let model_lower = model_id.to_lowercase();
        let mut providers = Vec::new();

        for entry in self.entries.iter() {
            let e = entry.value();
            if e.model_id == model_lower {
                providers.push(ProviderEntry {
                    provider_pk: e.provider_pk.clone(),
                    provider_endpoint: e.provider_endpoint.clone(),
                    pricing_nanoerg_per_million_tokens: e.pricing_nanoerg_per_million_tokens,
                    context_length: e.context_length,
                    is_available: e.is_available,
                });
            }
        }

        providers
    }

    /// Get the cheapest provider for a specific model.
    pub fn get_cheapest_provider(&self, model_id: &str) -> Option<ProviderEntry> {
        let providers = self.get_providers_for_model(model_id);
        providers
            .into_iter()
            .filter(|p| p.is_available)
            .min_by_key(|p| p.pricing_nanoerg_per_million_tokens)
    }

    /// Remove all entries for a specific provider (used during deregistration).
    pub fn remove_provider(&self, provider_pk: &str) {
        let pk_lower = provider_pk.to_lowercase();
        let keys_to_remove: Vec<String> = self
            .entries
            .iter()
            .filter(|e| e.value().provider_pk == pk_lower)
            .map(|e| e.key().clone())
            .collect();

        for key in keys_to_remove {
            self.entries.remove(&key);
        }
    }

    /// Remove entries not seen within the given timeout.
    pub fn prune_stale_models(&self, timeout: Duration) {
        let now = Instant::now();
        let keys_to_remove: Vec<String> = self
            .entries
            .iter()
            .filter(|e| now.duration_since(e.value().last_seen) > timeout)
            .map(|e| e.key().clone())
            .collect();

        for key in keys_to_remove {
            self.entries.remove(&key);
        }
    }

    /// Total number of model entries (not unique models).
    pub fn model_count(&self) -> usize {
        self.entries.len()
    }

    /// Number of unique model IDs.
    pub fn unique_model_count(&self) -> usize {
        let mut unique = std::collections::HashSet::new();
        for entry in self.entries.iter() {
            unique.insert(entry.value().model_id.clone());
        }
        unique.len()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_sync(model_id: &str, price: u64, ctx: u32) -> SyncModelInfo {
        SyncModelInfo {
            model_id: model_id.to_string(),
            context_length: ctx,
            pricing_nanoerg_per_million_tokens: price,
        }
    }

    #[test]
    fn test_add_and_sync_models() {
        let reg = ModelRegistry::new();

        reg.sync_from_provider(
            "pk1",
            "http://provider1:9099",
            vec![
                make_sync("llama-3-8b", 100, 4096),
                make_sync("qwen-72b", 500, 32768),
            ],
        );

        assert_eq!(reg.model_count(), 2);

        // Sync again with updated pricing
        reg.sync_from_provider(
            "pk1",
            "http://provider1:9099",
            vec![make_sync("llama-3-8b", 80, 8192)],
        );

        // Should still be 2 (prune not called yet)
        assert_eq!(reg.model_count(), 2);

        // Check updated pricing
        let providers = reg.get_providers_for_model("llama-3-8b");
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].pricing_nanoerg_per_million_tokens, 80);
        assert_eq!(providers[0].context_length, 8192);
    }

    #[test]
    fn test_multiple_providers_same_model() {
        let reg = ModelRegistry::new();

        reg.sync_from_provider(
            "pk1",
            "http://provider1:9099",
            vec![make_sync("llama-3-8b", 100, 4096)],
        );
        reg.sync_from_provider(
            "pk2",
            "http://provider2:9099",
            vec![make_sync("llama-3-8b", 80, 8192)],
        );

        assert_eq!(reg.model_count(), 2);

        let providers = reg.get_providers_for_model("llama-3-8b");
        assert_eq!(providers.len(), 2);

        let summaries = reg.get_all_models();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].available_providers, 2);
        assert_eq!(
            summaries[0].cheapest_price_nanoerg_per_million_tokens,
            80
        );
        assert_eq!(summaries[0].max_context_length, 8192);
    }

    #[test]
    fn test_cheapest_provider_selection() {
        let reg = ModelRegistry::new();

        reg.sync_from_provider(
            "pk1",
            "http://provider1:9099",
            vec![make_sync("llama-3-8b", 200, 4096)],
        );
        reg.sync_from_provider(
            "pk2",
            "http://provider2:9099",
            vec![make_sync("llama-3-8b", 50, 8192)],
        );
        reg.sync_from_provider(
            "pk3",
            "http://provider3:9099",
            vec![make_sync("llama-3-8b", 150, 4096)],
        );

        let cheapest = reg.get_cheapest_provider("llama-3-8b");
        assert!(cheapest.is_some());
        assert_eq!(cheapest.unwrap().provider_pk, "pk2");
    }

    #[test]
    fn test_cheapest_provider_no_results() {
        let reg = ModelRegistry::new();
        assert!(reg.get_cheapest_provider("nonexistent").is_none());
    }

    #[test]
    fn test_stale_model_pruning() {
        let reg = ModelRegistry::new();

        reg.sync_from_provider(
            "pk1",
            "http://provider1:9099",
            vec![make_sync("llama-3-8b", 100, 4096)],
        );
        reg.sync_from_provider(
            "pk2",
            "http://provider2:9099",
            vec![make_sync("qwen-72b", 500, 32768)],
        );

        assert_eq!(reg.model_count(), 2);

        // Prune with 0 timeout removes everything
        reg.prune_stale_models(Duration::from_millis(0));
        assert_eq!(reg.model_count(), 0);
    }

    #[test]
    fn test_prune_respects_timeout() {
        let reg = ModelRegistry::new();

        reg.sync_from_provider(
            "pk1",
            "http://provider1:9099",
            vec![make_sync("llama-3-8b", 100, 4096)],
        );

        // Prune with a very long timeout should not remove anything
        reg.prune_stale_models(Duration::from_secs(3600));
        assert_eq!(reg.model_count(), 1);
    }

    #[test]
    fn test_remove_provider() {
        let reg = ModelRegistry::new();

        reg.sync_from_provider(
            "pk1",
            "http://provider1:9099",
            vec![
                make_sync("llama-3-8b", 100, 4096),
                make_sync("qwen-72b", 500, 32768),
            ],
        );
        reg.sync_from_provider(
            "pk2",
            "http://provider2:9099",
            vec![make_sync("llama-3-8b", 80, 8192)],
        );

        assert_eq!(reg.model_count(), 3);

        reg.remove_provider("pk1");
        assert_eq!(reg.model_count(), 1);

        // Only pk2's llama-3-8b should remain
        let providers = reg.get_providers_for_model("llama-3-8b");
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].provider_pk, "pk2");

        // qwen-72b should be gone
        assert!(reg.get_providers_for_model("qwen-72b").is_empty());
    }

    #[test]
    fn test_model_count_and_unique_count() {
        let reg = ModelRegistry::new();

        reg.sync_from_provider(
            "pk1",
            "http://provider1:9099",
            vec![make_sync("llama-3-8b", 100, 4096)],
        );
        reg.sync_from_provider(
            "pk2",
            "http://provider2:9099",
            vec![make_sync("llama-3-8b", 80, 8192)],
        );

        assert_eq!(reg.model_count(), 2); // 2 entries
        assert_eq!(reg.unique_model_count(), 1); // 1 unique model
    }

    #[test]
    fn test_get_all_models_sorted() {
        let reg = ModelRegistry::new();

        reg.sync_from_provider(
            "pk1",
            "http://provider1:9099",
            vec![
                make_sync("qwen-72b", 500, 32768),
                make_sync("llama-3-8b", 100, 4096),
            ],
        );

        let models = reg.get_all_models();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].model_id, "llama-3-8b");
        assert_eq!(models[1].model_id, "qwen-72b");
    }

    #[test]
    fn test_free_models_have_zero_price() {
        let reg = ModelRegistry::new();

        reg.sync_from_provider(
            "pk1",
            "http://provider1:9099",
            vec![make_sync("free-model", 0, 4096)],
        );

        let summaries = reg.get_all_models();
        assert_eq!(summaries[0].cheapest_price_nanoerg_per_million_tokens, 0);
    }
}
