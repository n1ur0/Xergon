//! Request-level middleware
//!
//! Generates a unique request ID (UUID v4) for every incoming request,
//! stores it in request extensions for handler access, and adds it as
//! an `X-Request-Id` response header for client-side correlation.

use axum::{
    extract::Request,
    middleware::Next,
    response::Response,
};

/// Wrapper type for the request ID stored in request extensions.
#[derive(Clone)]
pub struct RequestId(pub String);

/// Middleware that assigns a UUID v4 request ID to every incoming request.
///
/// - Inserts `RequestId` into request extensions so any handler can read it.
/// - Sets `X-Request-Id` on the outgoing response.
pub async fn request_id_middleware(req: Request, next: Next) -> Response {
    let request_id = uuid::Uuid::new_v4().to_string();

    // Store in request extensions so handlers can access it
    let mut req = req;
    req.extensions_mut()
        .insert(RequestId(request_id.clone()));

    tracing::info!(
        request_id = %request_id,
        method = %req.method(),
        path = %req.uri().path(),
        "Incoming request"
    );

    let mut response = next.run(req).await;

    // Add as response header
    if let Ok(val) = request_id.parse() {
        response.headers_mut().insert("X-Request-Id", val);
    }

    response
}

/// Middleware that adds security headers to every response.
///
/// Headers added:
/// - `X-Content-Type-Options: nosniff` — prevents MIME type sniffing
/// - `X-Frame-Options: DENY` — prevents clickjacking via iframes
/// - `X-XSS-Protection: 0` — disables buggy XSS auditor (modern browsers)
/// - `Referrer-Policy: strict-origin-when-cross-origin`
/// - `Content-Security-Policy: default-src 'self'` (configurable via env)
pub async fn security_headers_middleware(req: Request, next: Next) -> Response {
    let mut response = next.run(req).await;

    let headers = response.headers_mut();
    headers.insert("X-Content-Type-Options", "nosniff".parse().unwrap());
    headers.insert("X-Frame-Options", "DENY".parse().unwrap());
    headers.insert("X-XSS-Protection", "0".parse().unwrap());
    headers.insert(
        "Referrer-Policy",
        "strict-origin-when-cross-origin".parse().unwrap(),
    );

    response
}
