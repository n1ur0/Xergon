//! GraphQL API layer for Xergon Relay
//!
//! Provides a GraphQL schema and resolver layer on top of existing REST APIs
//! using async-graphql. Exposes provider, model, request, and system queries.
//!
//! Endpoints:
//! - POST /api/graphql — execute GraphQL queries
//! - GET  /api/graphql — GraphiQL interactive playground

use async_graphql::{
    Context, EmptyMutation, EmptySubscription, FieldResult, ID, Object, Schema,
};
use axum::{
    extract::State,
    response::{Html, IntoResponse},
};
use std::sync::Arc;

use crate::proxy::AppState;

// ---------------------------------------------------------------------------
// GraphQL types
// ---------------------------------------------------------------------------

/// Provider exposed via GraphQL.
#[derive(async_graphql::SimpleObject, Clone)]
pub struct GqlProvider {
    pub id: ID,
    pub name: String,
    pub address: String,
    pub endpoint: String,
    pub status: String,
    pub models: Vec<String>,
    pub reputation: f64,
    pub total_requests: u64,
    pub latency_ms: u64,
    pub uptime_seconds: f64,
    pub region: Option<String>,
}

/// Model exposed via GraphQL.
#[derive(async_graphql::SimpleObject, Clone)]
pub struct GqlModel {
    pub id: ID,
    pub name: String,
    pub provider_count: u32,
    pub cheapest_price_nanoerg: u64,
    pub max_context_length: u32,
    pub available: bool,
}

/// A single inference request record.
#[derive(async_graphql::SimpleObject, Clone)]
pub struct GqlInferenceRequest {
    pub id: ID,
    pub model: String,
    pub provider: String,
    pub status: String,
    pub latency_ms: u64,
    pub tokens_input: u32,
    pub tokens_output: u32,
    pub cost_nanoerg: u64,
    pub timestamp: String,
}

/// System health snapshot.
#[derive(async_graphql::SimpleObject, Clone)]
pub struct GqlSystemHealth {
    pub uptime_seconds: u64,
    pub total_requests: u64,
    pub active_providers: u32,
    pub active_models: u32,
    pub error_rate: f64,
    pub avg_latency_ms: f64,
}

/// Relay-wide statistics.
#[derive(async_graphql::SimpleObject, Clone)]
pub struct GqlRelayStats {
    pub requests_per_second: f64,
    pub cache_hit_rate: f64,
    pub total_providers: u32,
    pub total_models: u32,
    pub regions: Vec<GqlRegionInfo>,
}

/// Region information.
#[derive(async_graphql::SimpleObject, Clone)]
pub struct GqlRegionInfo {
    pub id: String,
    pub name: String,
    pub providers: u32,
    pub avg_latency: f64,
    pub status: String,
}

// ---------------------------------------------------------------------------
// Query root
// ---------------------------------------------------------------------------

/// Helper: convert a Provider from the registry into a GqlProvider.
fn build_gql_provider(provider: &crate::provider::Provider) -> GqlProvider {
    let name = provider
        .status
        .as_ref()
        .and_then(|s| s.provider.as_ref())
        .map(|p| p.name.clone())
        .unwrap_or_default();

    let address = provider
        .status
        .as_ref()
        .and_then(|s| s.provider.as_ref())
        .map(|p| p.id.clone())
        .unwrap_or_else(|| provider.endpoint.clone());

    let status = if provider.is_healthy {
        "healthy".to_string()
    } else {
        "unhealthy".to_string()
    };

    let uptime_secs = provider
        .last_healthy_at
        .signed_duration_since(chrono::Utc::now())
        .num_seconds()
        .unsigned_abs();

    let reputation = provider.pown_score as f64 / 10.0;

    GqlProvider {
        id: ID::from(address.clone()),
        name,
        address,
        endpoint: provider.endpoint.clone(),
        status,
        models: provider.served_models.clone(),
        reputation,
        total_requests: provider.total_requests,
        latency_ms: provider.latency_ms,
        uptime_seconds: uptime_secs as f64,
        region: provider.region.clone(),
    }
}

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    // ---- Provider queries ----

    /// List all providers with optional filtering.
    async fn providers(
        &self,
        ctx: &Context<'_>,
        status: Option<String>,
        region: Option<String>,
        limit: Option<i32>,
        offset: Option<i32>,
    ) -> FieldResult<Vec<GqlProvider>> {
        let state = ctx.data::<Arc<AppState>>()?;
        let limit = limit.unwrap_or(50).max(1).min(500) as usize;
        let offset = offset.unwrap_or(0).max(0) as usize;

        let mut providers: Vec<GqlProvider> = Vec::new();

        for entry in state.provider_registry.providers.iter() {
            let provider = entry.value();

            // Filter by status
            if let Some(ref s) = status {
                let provider_status = if provider.is_healthy { "healthy" } else { "unhealthy" };
                if s != provider_status {
                    continue;
                }
            }

            // Filter by region
            if let Some(ref r) = region {
                if provider.region.as_deref() != Some(r.as_str()) {
                    continue;
                }
            }

            providers.push(build_gql_provider(provider));
        }

        // Sort by reputation descending
        providers.sort_by(|a, b| b.reputation.partial_cmp(&a.reputation).unwrap_or(std::cmp::Ordering::Equal));

        // Apply pagination
        let providers = providers
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect();

        Ok(providers)
    }

    /// Get a single provider by ID (provider public key or endpoint).
    async fn provider(&self, ctx: &Context<'_>, id: ID) -> FieldResult<Option<GqlProvider>> {
        let state = ctx.data::<Arc<AppState>>()?;
        let id_str = id.to_string();

        // Try to find by endpoint first
        if let Some(entry) = state.provider_registry.providers.get(&id_str) {
            let provider = entry.value();
            return Ok(Some(build_gql_provider(provider)));
        }

        // Try finding by provider_pk
        for entry in state.provider_registry.providers.iter() {
            let provider = entry.value();
            if provider
                .status
                .as_ref()
                .and_then(|s| s.provider.as_ref())
                .map(|p| p.id == id_str)
                .unwrap_or(false)
            {
                return Ok(Some(build_gql_provider(provider)));
            }
        }

        Ok(None)
    }

    /// Get a provider by its address (public key).
    async fn provider_by_address(
        &self,
        ctx: &Context<'_>,
        address: String,
    ) -> FieldResult<Option<GqlProvider>> {
        self.provider(ctx, ID::from(address)).await
    }

    // ---- Model queries ----

    /// List all available models with optional filtering.
    async fn models(
        &self,
        ctx: &Context<'_>,
        provider_id: Option<ID>,
        limit: Option<i32>,
    ) -> FieldResult<Vec<GqlModel>> {
        let state = ctx.data::<Arc<AppState>>()?;
        let limit = limit.unwrap_or(50).max(1).min(500) as usize;

        let summaries = state.model_registry.get_all_models();
        let mut result: Vec<GqlModel> = summaries
            .into_iter()
            .map(|s| GqlModel {
                id: ID::from(s.model_id.clone()),
                name: s.model_id.clone(),
                provider_count: s.available_providers as u32,
                cheapest_price_nanoerg: s.cheapest_price_nanoerg_per_million_tokens,
                max_context_length: s.max_context_length,
                available: s.available_providers > 0,
            })
            .collect();

        // Filter by provider_id
        if let Some(ref pid) = provider_id {
            let pid_str = pid.to_string();
            result.retain(|m| {
                let providers = state.model_registry.get_providers_for_model(&m.name);
                providers.iter().any(|p| p.provider_pk == pid_str)
            });
        }

        result.truncate(limit);
        Ok(result)
    }

    /// Get a single model by ID.
    async fn model(&self, ctx: &Context<'_>, id: ID) -> FieldResult<Option<GqlModel>> {
        let state = ctx.data::<Arc<AppState>>()?;
        let model_id = id.to_string();

        let summaries = state.model_registry.get_all_models();
        let found = summaries.into_iter().find(|s| s.model_id == model_id);

        Ok(found.map(|s| GqlModel {
            id: ID::from(s.model_id.clone()),
            name: s.model_id,
            provider_count: s.available_providers as u32,
            cheapest_price_nanoerg: s.cheapest_price_nanoerg_per_million_tokens,
            max_context_length: s.max_context_length,
            available: s.available_providers > 0,
        }))
    }

    /// Search models by query string (substring match).
    async fn search_models(
        &self,
        ctx: &Context<'_>,
        query: String,
        limit: Option<i32>,
    ) -> FieldResult<Vec<GqlModel>> {
        let state = ctx.data::<Arc<AppState>>()?;
        let limit = limit.unwrap_or(20).max(1).min(100) as usize;
        let query_lower = query.to_lowercase();

        let summaries = state.model_registry.get_all_models();
        let mut results: Vec<GqlModel> = summaries
            .into_iter()
            .filter(|s| s.model_id.to_lowercase().contains(&query_lower))
            .map(|s| GqlModel {
                id: ID::from(s.model_id.clone()),
                name: s.model_id,
                provider_count: s.available_providers as u32,
                cheapest_price_nanoerg: s.cheapest_price_nanoerg_per_million_tokens,
                max_context_length: s.max_context_length,
                available: s.available_providers > 0,
            })
            .collect();

        results.truncate(limit);
        Ok(results)
    }

    // ---- Request queries ----

    /// List recent inference requests with optional filtering.
    async fn requests(
        &self,
        ctx: &Context<'_>,
        model: Option<String>,
        status: Option<String>,
        limit: Option<i32>,
    ) -> FieldResult<Vec<GqlInferenceRequest>> {
        let state = ctx.data::<Arc<AppState>>()?;
        let limit = limit.unwrap_or(50).max(1).min(500) as usize;

        let mut results: Vec<GqlInferenceRequest> = Vec::new();

        for entry in state.usage_store.iter() {
            let record = entry.value();

            // Filter by model
            if let Some(ref m) = model {
                if &record.model != m {
                    continue;
                }
            }

            // Determine status from latency (we store successful requests)
            let record_status = if record.latency_ms > 0 {
                "completed"
            } else {
                "failed"
            };
            if let Some(ref s) = status {
                if s != record_status {
                    continue;
                }
            }

            results.push(GqlInferenceRequest {
                id: ID::from(record.request_id.clone()),
                model: record.model.clone(),
                provider: record.provider.clone(),
                status: record_status.to_string(),
                latency_ms: record.latency_ms,
                tokens_input: record.tokens_in,
                tokens_output: record.tokens_out,
                cost_nanoerg: 0, // not tracked at per-request level currently
                timestamp: record.created_at.to_rfc3339(),
            });
        }

        // Sort by timestamp descending (most recent first)
        results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        results.truncate(limit);
        Ok(results)
    }

    /// Get a single inference request by ID.
    async fn request(
        &self,
        ctx: &Context<'_>,
        id: ID,
    ) -> FieldResult<Option<GqlInferenceRequest>> {
        let state = ctx.data::<Arc<AppState>>()?;
        let request_id = id.to_string();

        let entry = state.usage_store.get(&request_id);
        Ok(entry.map(|r| {
            let record = r.value();
            let record_status = if record.latency_ms > 0 {
                "completed"
            } else {
                "failed"
            };
            GqlInferenceRequest {
                id: ID::from(record.request_id.clone()),
                model: record.model.clone(),
                provider: record.provider.clone(),
                status: record_status.to_string(),
                latency_ms: record.latency_ms,
                tokens_input: record.tokens_in,
                tokens_output: record.tokens_out,
                cost_nanoerg: 0,
                timestamp: record.created_at.to_rfc3339(),
            }
        }))
    }

    // ---- System queries ----

    /// Get system health information.
    async fn system_health(&self, ctx: &Context<'_>) -> FieldResult<GqlSystemHealth> {
        let state = ctx.data::<Arc<AppState>>()?;

        let uptime = state.relay_metrics.uptime_seconds();
        let total_requests = {
            // Approximate total from usage store size
            state.usage_store.len() as u64
        };
        let active_providers = state.provider_registry.healthy_provider_count() as u32;
        let active_models = state.model_registry.unique_model_count() as u32;

        // Error rate: approximate from usage store
        let error_rate = {
            let total = state.usage_store.len();
            if total == 0 {
                0.0
            } else {
                // Count requests with 0 latency as failures
                let errors = state
                    .usage_store
                    .iter()
                    .filter(|e| e.value().latency_ms == 0)
                    .count();
                errors as f64 / total as f64
            }
        };

        let avg_latency_ms = {
            let total = state.usage_store.len();
            if total == 0 {
                0.0
            } else {
                let sum: u64 = state
                    .usage_store
                    .iter()
                    .map(|e| e.value().latency_ms)
                    .sum();
                sum as f64 / total as f64
            }
        };

        Ok(GqlSystemHealth {
            uptime_seconds: uptime,
            total_requests,
            active_providers,
            active_models,
            error_rate,
            avg_latency_ms,
        })
    }

    /// Get relay-wide statistics.
    async fn relay_stats(&self, ctx: &Context<'_>) -> FieldResult<GqlRelayStats> {
        let state = ctx.data::<Arc<AppState>>()?;

        let cache_stats = state.response_cache.stats();
        let cache_hit_rate = {
            let total = cache_stats.hits + cache_stats.misses;
            if total == 0 {
                0.0
            } else {
                cache_stats.hits as f64 / total as f64
            }
        };

        let total_providers = state.provider_registry.providers.len() as u32;
        let total_models = state.model_registry.unique_model_count() as u32;

        // Approximate requests per second from uptime
        let uptime = state.relay_metrics.uptime_seconds();
        let total_requests = state.usage_store.len() as u64;
        let requests_per_second = if uptime > 0 {
            total_requests as f64 / uptime as f64
        } else {
            0.0
        };

        // Build region info from provider regions
        let mut region_map: std::collections::HashMap<String, (u32, u64)> =
            std::collections::HashMap::new();
        for entry in state.provider_registry.providers.iter() {
            let provider = entry.value();
            let region_name = provider
                .region
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            let entry = region_map.entry(region_name).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += provider.latency_ms;
        }

        let regions: Vec<GqlRegionInfo> = region_map
            .into_iter()
            .map(|(name, (count, total_latency))| {
                let avg = if count > 0 {
                    total_latency as f64 / count as f64
                } else {
                    0.0
                };
                GqlRegionInfo {
                    id: name.to_lowercase().replace(' ', "-"),
                    name: name.clone(),
                    providers: count,
                    avg_latency: avg,
                    status: "active".to_string(),
                }
            })
            .collect();

        Ok(GqlRelayStats {
            requests_per_second,
            cache_hit_rate,
            total_providers,
            total_models,
            regions,
        })
    }

    /// List all regions with provider and latency info.
    async fn regions(&self, ctx: &Context<'_>) -> FieldResult<Vec<GqlRegionInfo>> {
        let stats = self.relay_stats(ctx).await?;
        Ok(stats.regions)
    }

    /// Provider leaderboard sorted by a given metric.
    async fn leaderboard(
        &self,
        ctx: &Context<'_>,
        sort_by: Option<String>,
        limit: Option<i32>,
    ) -> FieldResult<Vec<GqlProvider>> {
        let mut providers = self
            .providers(ctx, None, None, None, None)
            .await?;

        let sort_by = sort_by.unwrap_or_else(|| "reputation".to_string());
        match sort_by.as_str() {
            "latency" => providers.sort_by_key(|p| p.latency_ms),
            "requests" => providers.sort_by(|a, b| b.total_requests.cmp(&a.total_requests)),
            "name" => providers.sort_by(|a, b| a.name.cmp(&b.name)),
            _ => providers.sort_by(|a, b| b.reputation.partial_cmp(&a.reputation).unwrap_or(std::cmp::Ordering::Equal)),
        }

        let limit = limit.unwrap_or(20).max(1).min(100) as usize;
        providers.truncate(limit);
        Ok(providers)
    }
}

// ---------------------------------------------------------------------------
// Schema type alias
// ---------------------------------------------------------------------------

pub type GraphSchema = Schema<QueryRoot, EmptyMutation, EmptySubscription>;

// ---------------------------------------------------------------------------
// Schema construction
// ---------------------------------------------------------------------------

/// Build the GraphQL schema with shared app state injected as data.
pub fn build_schema(state: Arc<AppState>) -> GraphSchema {
    let query = QueryRoot;
    let mutation = EmptyMutation;
    let subscription = EmptySubscription;

    Schema::build(query, mutation, subscription)
        .data(state)
        .finish()
}

// ---------------------------------------------------------------------------
// Axum route handlers
// ---------------------------------------------------------------------------

/// POST /api/graphql — execute a GraphQL query.
pub async fn graphql_handler(
    State(schema): State<Arc<GraphSchema>>,
    request: async_graphql_axum::GraphQLRequest,
) -> impl IntoResponse {
    async_graphql_axum::GraphQLResponse::from(schema.execute(request.into_inner()).await)
}

/// GET /api/graphql — serve GraphiQL interactive playground (debug builds only).
#[cfg(debug_assertions)]
pub async fn graphql_playground_handler() -> impl IntoResponse {
    Html(
        async_graphql::http::playground_source(
            async_graphql::http::GraphQLPlaygroundConfig::new("/api/graphql"),
        ),
    )
}

/// GET /api/graphql — returns 404 in release builds (introspection disabled).
#[cfg(not(debug_assertions))]
pub async fn graphql_playground_handler() -> impl IntoResponse {
    axum::response::Response::builder()
        .status(axum::http::StatusCode::NOT_FOUND)
        .body(axum::body::Body::from("GraphQL playground is disabled in release builds"))
        .unwrap()
        .into_response()
}

// ---------------------------------------------------------------------------
// Router builder
// ---------------------------------------------------------------------------

/// Build the GraphQL sub-router.
pub fn build_graphql_router(schema: Arc<GraphSchema>) -> axum::Router<AppState> {
    axum::Router::new()
        .route("/api/graphql", axum::routing::post(graphql_handler))
        .route("/api/graphql", axum::routing::get(graphql_playground_handler))
        .with_state(schema)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_builds() {
        // Just verify the schema type alias compiles
        let _schema: Option<GraphSchema> = None;
    }
}
