//! Periodic usage reporting from relay to provider agents
//!
//! Every `usage_report_interval_secs` seconds, this task:
//! 1. Drains all entries from the in-memory `usage_store` (DashMap)
//! 2. Aggregates them by provider endpoint
//! 3. Looks up the provider's ID and ergo_address from the provider directory
//! 4. POSTs the aggregated usage to `{provider_endpoint}/xergon/usage`
//! 5. Logs success/failure for each provider

use dashmap::DashMap;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

use crate::config::RelayConfig;
use crate::proxy::UsageRecord;
use crate::registration::ProviderDirectory;

/// Aggregated usage for a single provider endpoint
#[derive(Debug, Default)]
struct AggregatedUsage {
    tokens_in: u64,
    tokens_out: u64,
    request_count: usize,
    request_ids: Vec<String>,
}

/// Run the periodic usage reporting loop.
///
/// Call this in a spawned tokio task. It runs indefinitely, waking up
/// every `interval_secs` to drain and report usage.
pub async fn run_usage_report_loop(
    usage_store: Arc<DashMap<String, UsageRecord>>,
    provider_directory: Arc<ProviderDirectory>,
    http_client: Client,
    config: Arc<RelayConfig>,
) {
    let interval_secs = config.relay.usage_report_interval_secs;
    let registration_token = config.providers.registration_token.clone();

    info!(
        interval_secs = interval_secs,
        "Usage reporting loop started"
    );

    // Wait for the first interval before the first report
    tokio::time::sleep(Duration::from_secs(interval_secs)).await;

    loop {
        // Step 1: Drain the usage_store
        let drained: Vec<UsageRecord> = usage_store
            .iter()
            .map(|r| r.value().clone())
            .collect();

        if drained.is_empty() {
            tokio::time::sleep(Duration::from_secs(interval_secs)).await;
            continue;
        }

        // Step 2: Aggregate by provider endpoint
        let mut by_provider: HashMap<String, AggregatedUsage> = HashMap::new();
        for record in &drained {
            let agg = by_provider
                .entry(record.provider.clone())
                .or_default();
            agg.tokens_in += record.tokens_in as u64;
            agg.tokens_out += record.tokens_out as u64;
            agg.request_count += 1;
            agg.request_ids.push(record.request_id.clone());
        }

        info!(
            providers = by_provider.len(),
            total_records = drained.len(),
            "Reporting aggregated usage to providers"
        );

        // Step 3: For each provider, look up info and POST
        for (endpoint, agg) in &by_provider {
            // Find provider info from the directory by matching endpoint
            let provider_info = provider_directory
                .list_providers(false)
                .providers
                .iter()
                .find(|p| {
                    p.endpoint.trim_end_matches('/') == endpoint.trim_end_matches('/')
                })
                .cloned();

            let (provider_id, ergo_address) = match provider_info {
                Some(info) => (info.provider_id, info.ergo_address),
                None => {
                    warn!(
                        endpoint = %endpoint,
                        "Provider not found in directory, skipping usage report"
                    );
                    continue;
                }
            };

            // Compute cost from config's cost_per_1k_tokens
            let cost_per_1k = config.credits.cost_per_1k_tokens;
            let total_tokens = agg.tokens_in + agg.tokens_out;
            let cost_usd = (total_tokens as f64 / 1000.0) * cost_per_1k;

            // Build the report payload
            let payload = serde_json::json!({
                "provider_id": provider_id,
                "ergo_address": ergo_address,
                "tokens_in": agg.tokens_in,
                "tokens_out": agg.tokens_out,
                "cost_usd": cost_usd,
                "request_id": format!("batch-{}-requests", agg.request_count),
            });

            let url = format!(
                "{}/xergon/usage",
                endpoint.trim_end_matches('/')
            );

            // Build request with Bearer auth using registration token
            let mut req_builder = http_client.post(&url).json(&payload);
            req_builder = req_builder.header("Authorization", format!("Bearer {}", registration_token));
            req_builder = req_builder.header("Content-Type", "application/json");

            match req_builder.send().await {
                Ok(resp) if resp.status().is_success() => {
                    info!(
                        provider_id = %provider_id,
                        endpoint = %endpoint,
                        tokens_in = agg.tokens_in,
                        tokens_out = agg.tokens_out,
                        cost_usd = cost_usd,
                        requests = agg.request_count,
                        "Usage reported to provider agent"
                    );

                    // Remove reported records from the store
                    for record in &drained {
                        if record.provider == *endpoint {
                            usage_store.remove(&record.request_id);
                        }
                    }
                }
                Ok(resp) => {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    warn!(
                        provider_id = %provider_id,
                        endpoint = %endpoint,
                        status = %status,
                        body = %body,
                        "Failed to report usage to provider agent"
                    );
                }
                Err(e) => {
                    warn!(
                        provider_id = %provider_id,
                        endpoint = %endpoint,
                        error = %e,
                        "Error reporting usage to provider agent"
                    );
                }
            }
        }

        tokio::time::sleep(Duration::from_secs(interval_secs)).await;
    }
}
