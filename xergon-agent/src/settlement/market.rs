//! ERG/USD market rate fetching
//!
//! Fetches the current ERG/USD exchange rate from CoinGecko with fallback.
//! On startup, loads the last-known-good rate from disk so settlements
//! can proceed even if CoinGecko is temporarily unavailable.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;
use std::path::PathBuf;
use tracing::{info, warn};

/// CoinGecko ERG market data response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct CoinGeckoResponse {
    ergo: CoinGeckoMarketData,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct CoinGeckoMarketData {
    usd: f64,
}

/// Persisted rate cache on disk.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PersistedRate {
    rate: f64,
    fetched_at: i64,
    source: String,
}

/// ERG/USD market rate provider.
pub struct MarketRateProvider {
    http_client: Client,
    /// Cached rate from last successful fetch (in-memory)
    cached_rate: std::sync::Mutex<Option<(f64, i64)>>,
    /// Maximum age of cached rate in seconds (default: 1 hour)
    cache_ttl_secs: i64,
    /// Path to persisted rate file (last-known-good fallback)
    persist_path: PathBuf,
}

#[allow(dead_code)] // TODO: will be used for dynamic pricing
impl MarketRateProvider {
    pub fn new() -> Result<Self> {
        Self::with_persist_path(PathBuf::from("data/last_erg_rate.json"))
    }

    /// Create with a custom persistence path.
    pub fn with_persist_path(persist_path: PathBuf) -> Result<Self> {
        let http_client = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .context("Failed to build HTTP client for market rate provider")?;

        Ok(Self {
            http_client,
            cached_rate: std::sync::Mutex::new(None),
            cache_ttl_secs: 3600,
            persist_path,
        })
    }

    /// Load the persisted last-known-good rate from disk.
    /// Call this on startup to seed the cache.
    pub async fn load_persisted(&self) {
        if !self.persist_path.exists() {
            info!("No persisted rate file found, will fetch from API");
            return;
        }

        match tokio::fs::read_to_string(&self.persist_path).await {
            Ok(data) => match serde_json::from_str::<PersistedRate>(&data) {
                Ok(persisted) => {
                    let age_hours =
                        (chrono::Utc::now().timestamp() - persisted.fetched_at) as f64 / 3600.0;
                    info!(
                        rate = persisted.rate,
                        age_hours = age_hours,
                        source = %persisted.source,
                        "Loaded persisted ERG/USD rate"
                    );
                    let mut guard = self.cached_rate.lock().unwrap();
                    *guard = Some((persisted.rate, persisted.fetched_at));
                }
                Err(e) => {
                    warn!(error = %e, "Failed to parse persisted rate file, ignoring");
                }
            },
            Err(e) => {
                warn!(error = %e, "Failed to read persisted rate file, ignoring");
            }
        }
    }

    /// Get the current ERG/USD rate. Returns cached value if fresh,
    /// otherwise fetches from CoinGecko. Falls back to persisted
    /// rate if CoinGecko is unavailable and a persisted rate exists.
    pub async fn get_rate(&self) -> Result<f64> {
        let now = chrono::Utc::now().timestamp();

        // Check cache first (in-memory, fresh within TTL)
        {
            let guard = self.cached_rate.lock().unwrap();
            if let Some((rate, ts)) = *guard {
                if now - ts < self.cache_ttl_secs {
                    info!(rate = rate, age_secs = now - ts, "Using cached ERG/USD rate");
                    return Ok(rate);
                }
            }
        }

        // Fetch fresh rate
        match self.fetch_coingecko().await {
            Ok(rate) => {
                info!(rate = rate, "Fetched fresh ERG/USD rate from CoinGecko");

                // Update in-memory cache
                {
                    let mut guard = self.cached_rate.lock().unwrap();
                    *guard = Some((rate, now));
                }

                // Persist to disk
                self.persist_rate(rate, now, "coingecko").await;

                Ok(rate)
            }
            Err(e) => {
                warn!(error = %e, "Failed to fetch rate from CoinGecko, checking fallback");

                // Fall back to in-memory cache even if stale
                {
                    let guard = self.cached_rate.lock().unwrap();
                    if let Some((rate, ts)) = *guard {
                        let age_hours = (now - ts) as f64 / 3600.0;
                        warn!(
                            rate = rate,
                            age_hours = age_hours,
                            "Using stale cached rate as fallback"
                        );
                        return Ok(rate);
                    }
                }

                // Fall back to persisted file
                match self.load_persisted_rate().await {
                    Some((rate, ts)) => {
                        let age_hours = (now - ts) as f64 / 3600.0;
                        warn!(
                            rate = rate,
                            age_hours = age_hours,
                            "Using persisted rate from disk as fallback"
                        );
                        // Also update in-memory cache
                        {
                            let mut guard = self.cached_rate.lock().unwrap();
                            *guard = Some((rate, ts));
                        }
                        Ok(rate)
                    }
                    None => {
                        anyhow::bail!(
                            "No ERG/USD rate available: CoinGecko failed, no cache, no persisted fallback"
                        );
                    }
                }
            }
        }
    }

    /// Fetch ERG/USD rate from CoinGecko free API.
    async fn fetch_coingecko(&self) -> Result<f64> {
        let url = "https://api.coingecko.com/api/v3/simple/price?ids=ergo&vs_currencies=usd";

        let resp = self
            .http_client
            .get(url)
            .header("Accept", "application/json")
            .send()
            .await
            .context("Failed to fetch ERG price from CoinGecko")?;

        if !resp.status().is_success() {
            anyhow::bail!("CoinGecko returned status {}", resp.status());
        }

        let data: CoinGeckoResponse = resp
            .json()
            .await
            .context("Failed to parse CoinGecko response")?;

        let rate = data.ergo.usd;

        if rate <= 0.0 || !rate.is_finite() {
            anyhow::bail!("Invalid ERG/USD rate from CoinGecko: {}", rate);
        }

        Ok(rate)
    }

    /// Persist a rate to disk.
    async fn persist_rate(&self, rate: f64, timestamp: i64, source: &str) {
        let persisted = PersistedRate {
            rate,
            fetched_at: timestamp,
            source: source.to_string(),
        };

        if let Some(parent) = self.persist_path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                warn!(error = %e, "Failed to create directory for rate persistence");
                return;
            }
        }

        match serde_json::to_string_pretty(&persisted) {
            Ok(data) => {
                if let Err(e) = tokio::fs::write(&self.persist_path, &data).await {
                    warn!(error = %e, "Failed to persist rate to disk");
                }
            }
            Err(e) => {
                warn!(error = %e, "Failed to serialize rate for persistence");
            }
        }
    }

    /// Load the persisted rate from disk (without side effects).
    async fn load_persisted_rate(&self) -> Option<(f64, i64)> {
        if !self.persist_path.exists() {
            return None;
        }

        let data = tokio::fs::read_to_string(&self.persist_path).await.ok()?;
        let persisted: PersistedRate = serde_json::from_str(&data).ok()?;
        Some((persisted.rate, persisted.fetched_at))
    }

    /// Get the cached rate without fetching (for API display).
    pub fn cached_rate(&self) -> Option<f64> {
        let guard = self.cached_rate.lock().unwrap();
        guard.map(|(rate, _)| rate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_persist_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rate.json");

        let provider = MarketRateProvider::with_persist_path(path.clone()).unwrap();

        // Persist a rate
        provider.persist_rate(0.52, 1700000000, "test").await;

        // Load it back
        let (rate, ts) = provider.load_persisted_rate().await.unwrap();
        assert_eq!(rate, 0.52);
        assert_eq!(ts, 1700000000);
    }
}
