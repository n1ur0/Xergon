//! Caching middleware for GET responses with ETag/conditional request support.
//!
//! For cacheable GET routes:
//! - Checks If-None-Match header; returns 304 if ETag matches
//! - Caches successful responses with ETag and Cache-Control headers
//! - Adds X-Cache header ("HIT" or "MISS")
//!
//! For mutating routes (POST/PUT/DELETE):
//! - Invalidates relevant cache entries
//! - Forwards request through

use axum::body::Body;
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use bytes::Bytes;
use std::sync::Arc;
use std::time::Duration;
use tracing::debug;

use crate::cache::{etag_matches, CacheConfig, ResponseCache};

// ---------------------------------------------------------------------------
// Cacheable route configuration
// ---------------------------------------------------------------------------

/// Configuration for a single cacheable route.
#[derive(Clone)]
struct CacheableRoute {
    method: Method,
    path_pattern: String,
    ttl: Duration,
    vary_headers: Vec<String>,
}

/// Set of cacheable routes with their TTLs.
#[derive(Clone)]
struct CacheRoutes {
    routes: Vec<CacheableRoute>,
}

impl CacheRoutes {
    fn new(config: &CacheConfig) -> Self {
        Self {
            routes: vec![
                CacheableRoute {
                    method: Method::GET,
                    path_pattern: "/v1/models".to_string(),
                    ttl: Duration::from_secs(config.model_list_ttl_secs),
                    vary_headers: vec![],
                },
                CacheableRoute {
                    method: Method::GET,
                    path_pattern: "/v1/providers".to_string(),
                    ttl: Duration::from_secs(config.provider_list_ttl_secs),
                    vary_headers: vec![],
                },
                CacheableRoute {
                    method: Method::GET,
                    path_pattern: "/v1/health".to_string(),
                    ttl: Duration::from_secs(config.health_ttl_secs),
                    vary_headers: vec![],
                },
                CacheableRoute {
                    method: Method::GET,
                    path_pattern: "/v1/leaderboard".to_string(),
                    ttl: Duration::from_secs(10),
                    vary_headers: vec![],
                },
                CacheableRoute {
                    method: Method::GET,
                    path_pattern: "/v1/openapi.json".to_string(),
                    ttl: Duration::from_secs(300),
                    vary_headers: vec![],
                },
                CacheableRoute {
                    method: Method::GET,
                    path_pattern: "/api/oracle/rate".to_string(),
                    ttl: Duration::from_secs(60),
                    vary_headers: vec![],
                },
                CacheableRoute {
                    method: Method::GET,
                    path_pattern: "/v1/docs".to_string(),
                    ttl: Duration::from_secs(300),
                    vary_headers: vec![],
                },
                CacheableRoute {
                    method: Method::GET,
                    path_pattern: "/api/versions".to_string(),
                    ttl: Duration::from_secs(300),
                    vary_headers: vec![],
                },
            ],
        }
    }

    /// Find a matching cacheable route for the given method and path.
    fn find(&self, method: &Method, path: &str) -> Option<&CacheableRoute> {
        self.routes
            .iter()
            .find(|r| r.method == *method && r.path_pattern == path)
    }
}

// ---------------------------------------------------------------------------
// State shared via request extensions (set by the middleware layer builder)
// ---------------------------------------------------------------------------

/// Extension state inserted into every request by the cache middleware layer.
/// Handlers and other middleware can access this to invalidate cache entries.
#[derive(Clone)]
pub struct CacheLayerState {
    pub cache: Arc<ResponseCache>,
    cache_routes: CacheRoutes,
}

impl CacheLayerState {
    pub fn new(cache: Arc<ResponseCache>, config: &CacheConfig) -> Self {
        Self {
            cache,
            cache_routes: CacheRoutes::new(config),
        }
    }
}

// ---------------------------------------------------------------------------
// Cache middleware
// ---------------------------------------------------------------------------

/// The main caching middleware function.
///
/// This is used as `axum::middleware::from_fn_with_state(cache_state, cache_middleware)`.
pub async fn cache_middleware(
    axum::extract::State(state): axum::extract::State<CacheLayerState>,
    mut req: axum::extract::Request,
    next: Next,
) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let query = req.uri().query().unwrap_or("").to_string();

    // Only cache GET requests
    if method != Method::GET {
        // For mutating methods, invalidate relevant cache entries
        if matches!(method, Method::POST | Method::PUT | Method::DELETE | Method::PATCH) {
            invalidate_on_mutation(&state.cache, &path);
        }
        return next.run(req).await;
    }

    // Check if this route is cacheable
    let route_config = match state.cache_routes.find(&method, &path) {
        Some(r) => r,
        None => return next.run(req).await,
    };

    let cache_key = ResponseCache::cache_key(method.as_str(), &path, &query);

    // Check If-None-Match header for conditional request
    if let Some(if_none_match) = req.headers().get(header::IF_NONE_MATCH) {
        if let Ok(if_none_match_str) = if_none_match.to_str() {
            if let Some(cached) = state.cache.get(&cache_key) {
                if etag_matches(if_none_match_str, &cached.etag) {
                    debug!(
                        path = %path,
                        etag = %cached.etag,
                        "Returning 304 Not Modified (cache hit)"
                    );
                    return build_304_response(&cached.etag, route_config.ttl);
                }
            }
        }
    }

    // Forward the request
    let mut response = next.run(req).await;

    // Only cache successful responses with body
    let status = response.status();
    if !status.is_success() && status != StatusCode::NOT_MODIFIED {
        return response;
    }

    // Try to collect the response body
    let (body_parts, body) = response.into_parts();
    match axum::body::to_bytes(body, state.cache.max_entry_size_bytes() + 1).await {
        Ok(bytes) if bytes.len() <= state.cache.max_entry_size_bytes() => {
            let content_type = body_parts
                .headers
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/octet-stream")
                .to_string();

            // Store in cache
            let ttl = route_config.ttl;
            if state.cache.put(
                &cache_key,
                bytes.clone(),
                content_type.clone(),
                status.as_u16(),
                ttl,
            ) {
                let etag = {
                    // Re-get to get the generated ETag
                    state.cache.get(&cache_key).map(|c| c.etag)
                };

                if let Some(ref etag_val) = etag {
                    debug!(
                        path = %path,
                        etag = %etag_val,
                        ttl_secs = ttl.as_secs(),
                        "Response cached"
                    );

                    // Rebuild response with cache headers
                    let mut resp = Response::builder()
                        .status(status)
                        .header("X-Cache", "MISS")
                        .header(header::ETAG, etag_val)
                        .header(
                            header::CACHE_CONTROL,
                            format!("public, max-age={}", ttl.as_secs()),
                        );

                    // Copy original headers
                    for (name, value) in body_parts.headers.iter() {
                        if name != header::CACHE_CONTROL && name != header::ETAG {
                            resp = resp.header(name, value);
                        }
                    }

                    return resp.body(Body::from(bytes)).unwrap();
                }
            }

            // Couldn't cache — return original response with X-Cache: BYPASS
            let mut resp = Response::builder()
                .status(status)
                .header("X-Cache", "BYPASS");

            for (name, value) in body_parts.headers.iter() {
                resp = resp.header(name, value);
            }

            resp.body(Body::from(bytes)).unwrap()
        }
        Ok(_) => {
            // Body too large to cache, return as-is
            let mut resp = Response::builder()
                .status(status)
                .header("X-Cache", "BYPASS");

            for (name, value) in body_parts.headers.iter() {
                resp = resp.header(name, value);
            }

            resp.body(Body::from(Bytes::new())).unwrap()
        }
        Err(_) => {
            // Stream body or error — can't cache, return empty body
            Response::builder()
                .status(status)
                .header("X-Cache", "BYPASS")
                .body(Body::empty())
                .unwrap()
        }
    }
}

// ---------------------------------------------------------------------------
// Cache invalidation on mutations
// ---------------------------------------------------------------------------

/// Invalidate cache entries relevant to a mutation at the given path.
fn invalidate_on_mutation(cache: &ResponseCache, path: &str) {
    match path {
        // Provider onboarding/update/removal -> invalidate providers + models
        p if p.starts_with("/v1/providers/onboard")
            || p.starts_with("/v1/providers/")
            && !p.contains("/onboard/") =>
        {
            cache.invalidate_prefix(&ResponseCache::cache_prefix("GET", "/v1/providers"));
            cache.invalidate_prefix(&ResponseCache::cache_prefix("GET", "/v1/models"));
            debug!(path = %path, "Invalidated providers and models cache");
        }
        // Chat completions don't affect read-only endpoints
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Response builders
// ---------------------------------------------------------------------------

/// Build a 304 Not Modified response with ETag and Cache-Control headers.
fn build_304_response(etag: &str, ttl: Duration) -> Response {
    Response::builder()
        .status(StatusCode::NOT_MODIFIED)
        .header(header::ETAG, etag)
        .header(
            header::CACHE_CONTROL,
            format!("public, max-age={}", ttl.as_secs()),
        )
        .header("X-Cache", "HIT")
        .body(Body::empty())
        .unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;
    use tower::ServiceExt;

    fn test_cache_config() -> CacheConfig {
        CacheConfig {
            enabled: true,
            max_entries: 100,
            default_ttl_secs: 60,
            model_list_ttl_secs: 30,
            provider_list_ttl_secs: 15,
            health_ttl_secs: 5,
            max_entry_size_bytes: 10240,
        }
    }

    fn make_cache_state() -> CacheLayerState {
        let cache = Arc::new(ResponseCache::new(test_cache_config()));
        CacheLayerState::new(cache, &test_cache_config())
    }

    #[tokio::test]
    async fn test_cache_miss_first_request() {
        let state = make_cache_state();
        let app = axum::Router::new()
            .route("/v1/models", axum::routing::get(|| async { "[]" }))
            .layer(axum::middleware::from_fn_with_state(
                state,
                cache_middleware,
            ));

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/models")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers().get("X-Cache").unwrap(), "MISS");
        assert!(resp.headers().get(header::ETAG).is_some());
        assert!(resp.headers().get(header::CACHE_CONTROL).is_some());
    }

    #[tokio::test]
    async fn test_non_cacheable_route_bypass() {
        let state = make_cache_state();
        let app = axum::Router::new()
            .route("/v1/chat/completions", axum::routing::post(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                state,
                cache_middleware,
            ));

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/chat/completions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        // No X-Cache header for non-cacheable routes
        assert!(resp.headers().get("X-Cache").is_none());
    }

    #[tokio::test]
    async fn test_cache_hit_returns_304() {
        let state = make_cache_state();

        // Pre-populate the cache
        let key = ResponseCache::cache_key("GET", "/v1/health", "");
        let body = Bytes::from_static(b"ok");
        state.cache.put(&key, body, "text/plain".into(), 200, Duration::from_secs(60));

        let app = axum::Router::new()
            .route("/v1/health", axum::routing::get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                cache_middleware,
            ));

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/health")
                    .header(
                        header::IF_NONE_MATCH,
                        state.cache.get(&key).unwrap().etag,
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(resp.headers().get("X-Cache").unwrap(), "HIT");
        assert!(resp.headers().get(header::ETAG).is_some());
        assert!(resp.headers().get(header::CACHE_CONTROL).is_some());
    }

    #[tokio::test]
    async fn test_mutation_invalidates_cache() {
        let state = make_cache_state();

        // Pre-populate cache
        let models_key = ResponseCache::cache_key("GET", "/v1/models", "");
        state.cache.put(
            &models_key,
            Bytes::from_static(b"[]"),
            "application/json".into(),
            200,
            Duration::from_secs(60),
        );

        let app = axum::Router::new()
            .route(
                "/v1/providers/onboard",
                axum::routing::post(|| async { "registered" }),
            )
            .layer(axum::middleware::from_fn_with_state(
                state.clone(),
                cache_middleware,
            ));

        // POST to onboarding endpoint
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/providers/onboard")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        // Models cache should be invalidated
        assert!(state.cache.get(&models_key).is_none());
    }

    #[tokio::test]
    async fn test_cache_control_headers_present() {
        let state = make_cache_state();
        let app = axum::Router::new()
            .route("/v1/models", axum::routing::get(|| async { "[]" }))
            .layer(axum::middleware::from_fn_with_state(
                state,
                cache_middleware,
            ));

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/models")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let cache_control = resp.headers().get(header::CACHE_CONTROL).unwrap().to_str().unwrap();
        assert!(cache_control.starts_with("public, max-age="));
        assert!(cache_control.contains("30")); // model_list_ttl_secs = 30
    }

    #[tokio::test]
    async fn test_if_none_match_no_match_forward() {
        let state = make_cache_state();

        let app = axum::Router::new()
            .route("/v1/health", axum::routing::get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                state,
                cache_middleware,
            ));

        // Send a non-matching If-None-Match
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/health")
                    .header(header::IF_NONE_MATCH, "\"sha256:deadbeef00000000\"")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Should get 200, not 304
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
