//! OpenTelemetry HTTP tracing middleware for Axum.
//!
//! Creates a span for each HTTP request with standard semantic attributes.
//! Injects trace context into response headers (traceparent) for
//! client-side correlation.

use axum::{
    extract::Request,
    middleware::Next,
    response::Response,
};
use std::time::Instant;
use tracing::{field, info_span, Span};

/// Middleware that creates an OpenTelemetry-compatible span for each HTTP request.
///
/// Span attributes follow OpenTelemetry semantic conventions:
/// - `http.method` — request method (GET, POST, etc.)
/// - `http.url` — full request URL
/// - `http.status_code` — response status
/// - `http.route` — matched route pattern (if available)
/// - `xergon.provider_pk` — provider public key (set by proxy handler if proxied)
/// - `xergon.model` — requested model
/// - `xergon.strategy` — routing strategy used
pub async fn otel_http_middleware(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let path = uri.path();
    let query = uri.query().unwrap_or("");
    let start = Instant::now();

    // Build span name: "{method} {path}"
    let _span_name = format!("{} {}", method, path);

    let span = info_span!(
        "http.request",
        http.method = %method,
        http.url = %uri,
        http.route = field::Empty,
        http.status_code = field::Empty,
        http.flavor = ?uri.scheme(),
        xergon.provider_pk = field::Empty,
        xergon.model = field::Empty,
        xergon.strategy = field::Empty,
        xergon.client_ip = field::Empty,
    );

    // Extract client IP from headers
    let client_ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim())
        .or_else(|| {
            req.headers()
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
        })
        .unwrap_or("unknown");

    span.record("xergon.client_ip", client_ip);

    let _guard = span.enter();

    let response = next.run(req).await;

    let status = response.status();
    let latency = start.elapsed();

    // Record status code on the span
    Span::current().record("http.status_code", status.as_u16());

    // Extract the matched route from response extensions if available
    if let Some(route) = response.extensions().get::<axum::extract::MatchedPath>() {
        Span::current().record("http.route", route.as_str());
    } else {
        Span::current().record("http.route", path);
    }

    // Record request duration as a span event
    Span::current().record(
        "http.request.duration_ms",
        latency.as_millis() as u64,
    );

    tracing::info!(
        status = %status,
        latency_ms = latency.as_millis() as u64,
        method = %method,
        path = path,
        query = query,
        "HTTP request completed"
    );

    // Build response with traceparent header for client-side correlation
    let mut resp = response;
    inject_traceparent_header(&mut resp);

    resp
}

/// Inject W3C traceparent header into response for client-side correlation.
///
/// The traceparent header follows the W3C Trace Context specification:
/// `traceparent: 00-{trace_id}-{span_id}-{flags}`
///
/// When the `telemetry` feature is not compiled, this is a no-op.
fn inject_traceparent_header(_response: &mut Response) {
    #[cfg(feature = "telemetry")]
    {
        use axum::http::HeaderValue;
        use opentelemetry::trace::TraceContextExt as _;
        use tracing_opentelemetry::OpenTelemetrySpanExt as _;

        let context = tracing::Span::current().context();
        let span = context.span();
        let span_context = span.span_context();
        let trace_id = span_context.trace_id();
        let span_id = span_context.span_id();

        // Format: 00-{32-hex-trace-id}-{16-hex-span_id}-01
        let traceparent = format!("00-{}-{}-01", trace_id, span_id);

        if let Ok(val) = traceparent.parse::<HeaderValue>() {
            response.headers_mut().insert("traceparent", val);
        }
    }
}

/// Axum handler for GET /v1/tracing/status.
///
/// Returns current telemetry configuration and status.
#[derive(serde::Serialize)]
pub struct TracingStatus {
    pub enabled: bool,
    pub endpoint: String,
    pub service_name: String,
}

pub async fn tracing_status_handler(
    axum::extract::State(state): axum::extract::State<crate::proxy::AppState>,
) -> axum::Json<TracingStatus> {
    let config = &state.config.telemetry;
    let enabled = crate::telemetry::is_telemetry_enabled(config.enabled);
    let endpoint = crate::telemetry::effective_otlp_endpoint(&config.otlp_endpoint);
    let service_name = crate::telemetry::effective_service_name(&config.service_name);

    axum::Json(TracingStatus {
        enabled,
        endpoint,
        service_name,
    })
}
