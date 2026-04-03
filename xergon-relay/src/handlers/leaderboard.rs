//! GET /v1/leaderboard — Public provider leaderboard
//!
//! Returns per-provider usage statistics from the DB, enriched with live
//! registration data (provider_name, region, ergo_address, models, online status).
//!
//! This is a PUBLIC endpoint — no authentication required.

use axum::{extract::State, response::Json};
use chrono::Utc;
use serde::Serialize;
use tracing::info;

use crate::proxy::AppState;

/// Enriched leaderboard entry returned to clients.
#[derive(Debug, Clone, Serialize)]
pub struct LeaderboardEntry {
    pub provider_id: String,
    pub provider_name: String,
    pub region: String,
    pub ergo_address: String,
    /// Models currently served by this provider (live from directory)
    pub models: Vec<String>,
    /// Whether the provider is currently online (heartbeat within TTL)
    pub online: bool,
    pub total_requests: i64,
    pub total_prompt_tokens: i64,
    pub total_completion_tokens: i64,
    pub total_tokens: i64,
    pub total_revenue_usd: f64,
    pub unique_models: i64,
    pub first_seen: String,
    pub last_seen: String,
}

/// GET /v1/leaderboard
pub async fn leaderboard_handler(State(state): State<AppState>) -> Json<Vec<LeaderboardEntry>> {
    info!("Leaderboard requested");

    // 1. Fetch aggregated usage stats from DB
    let db_stats = match state.db.get_provider_leaderboard() {
        Ok(stats) => stats,
        Err(e) => {
            tracing::error!(error = %e, "Failed to query provider leaderboard from DB");
            return Json(Vec::new());
        }
    };

    // 2. Fetch all currently registered providers (including offline ones)
    let directory = state.provider_directory.list_providers(false);

    // 3. Build a lookup map from provider_id -> RegisteredProvider
    let mut provider_map: std::collections::HashMap<String, _> = std::collections::HashMap::new();
    let now = Utc::now();

    for rp in &directory.providers {
        let expires_at = rp.last_heartbeat + chrono::Duration::seconds(rp.ttl_secs as i64);
        let online = now <= expires_at;
        provider_map.insert(rp.provider_id.clone(), (rp, online));
    }

    // 4. Merge DB stats with live directory data
    let mut entries: Vec<LeaderboardEntry> = db_stats
        .into_iter()
        .map(|stat| {
            let (name, region, ergo_addr, models, online) =
                match provider_map.get(&stat.provider_id) {
                    Some((rp, on)) => (
                        rp.provider_name.clone(),
                        rp.region.clone(),
                        rp.ergo_address.clone(),
                        rp.models.clone(),
                        *on,
                    ),
                    None => (
                        stat.provider_id.clone(),
                        String::new(),
                        String::new(),
                        Vec::new(),
                        false,
                    ),
                };

            LeaderboardEntry {
                provider_id: stat.provider_id,
                provider_name: name,
                region,
                ergo_address: ergo_addr,
                models,
                online,
                total_requests: stat.total_requests,
                total_prompt_tokens: stat.total_prompt_tokens,
                total_completion_tokens: stat.total_completion_tokens,
                total_tokens: stat.total_tokens,
                total_revenue_usd: stat.total_revenue_usd,
                unique_models: stat.unique_models,
                first_seen: stat.first_seen,
                last_seen: stat.last_seen,
            }
        })
        .collect();

    // Sort by total_tokens descending (should already be from DB, but ensure)
    entries.sort_by(|a, b| b.total_tokens.cmp(&a.total_tokens));

    Json(entries)
}
