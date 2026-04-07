//! API Versioning Infrastructure
//!
//! Provides version extraction, deprecation headers, and a version metadata endpoint.
//!
//! Features:
//! - ApiVersion enum (V1, V2)
//! - VersionMiddleware that injects X-API-Version, Deprecation, and Sunset headers
//! - Version configuration (release date, deprecation status, sunset date)
//! - GET /api/versions endpoint

use axum::{
    extract::Request,
    http::{HeaderName, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use serde::Serialize;
use std::str::FromStr;

// ---------------------------------------------------------------------------
// Version enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum ApiVersion {
    V1,
    V2,
}

impl ApiVersion {
    /// Parse a version from a path prefix like "/v1" or "/v2".
    pub fn from_path(path: &str) -> Option<Self> {
        let path = path.trim_start_matches('/');
        let segment = path.split('/').next()?;
        match segment.to_lowercase().as_str() {
            "v1" => Some(ApiVersion::V1),
            "v2" => Some(ApiVersion::V2),
            _ => None,
        }
    }

    /// The prefix string, e.g. "/v1".
    pub fn prefix(&self) -> &'static str {
        match self {
            ApiVersion::V1 => "/v1",
            ApiVersion::V2 => "/v2",
        }
    }

    /// The version label, e.g. "v1".
    pub fn label(&self) -> &'static str {
        match self {
            ApiVersion::V1 => "v1",
            ApiVersion::V2 => "v2",
        }
    }
}

impl FromStr for ApiVersion {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "v1" => Ok(ApiVersion::V1),
            "v2" => Ok(ApiVersion::V2),
            _ => Err(format!("Unsupported API version: {s}")),
        }
    }
}

// ---------------------------------------------------------------------------
// Version configuration
// ---------------------------------------------------------------------------

/// Configuration metadata for a single API version.
#[derive(Debug, Clone, Serialize)]
pub struct VersionConfig {
    pub version: String,
    pub status: VersionStatus,
    pub path: String,
    pub released_date: String,
    pub deprecated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sunset_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changelog_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum VersionStatus {
    Current,
    Deprecated,
    Reserved,
    Sunset,
}

/// Returns the full list of supported API version configurations.
pub fn version_configs() -> Vec<VersionConfig> {
    vec![
        VersionConfig {
            version: "v1".into(),
            status: VersionStatus::Current,
            path: "/v1".into(),
            released_date: "2025-01-01".into(),
            deprecated: false,
            sunset_date: None,
            changelog_url: Some("https://docs.xergon.network/api/changelog".into()),
        },
        VersionConfig {
            version: "v2".into(),
            status: VersionStatus::Reserved,
            path: "/v2".into(),
            released_date: String::new(),
            deprecated: false,
            sunset_date: None,
            changelog_url: None,
        },
    ]
}

/// Look up version config by label. Returns None for unknown versions.
pub fn get_version_config(version: &ApiVersion) -> Option<VersionConfig> {
    version_configs()
        .into_iter()
        .find(|c| c.version == version.label())
}

// ---------------------------------------------------------------------------
// Version middleware
// ---------------------------------------------------------------------------

/// Middleware that extracts the API version from the request path and adds
/// version-related headers to the response.
///
/// Headers added:
/// - `X-API-Version: v1` on every request
/// - `Deprecation: true` if the version is deprecated
/// - `Sunset: <date>` if a sunset date is configured
/// - `X-API-Deprecated: true` if the version is deprecated
///
/// For unsupported versions (e.g. /v3), returns 404 with a list of supported versions.
pub async fn version_middleware(req: Request, next: Next) -> Response {
    let path = req.uri().path();
    let version = ApiVersion::from_path(path);

    match version {
        Some(v) => {
            let config = get_version_config(&v);

            // If we have a config entry but it's "reserved", still let it through
            // (the route won't match anyway — 404 from axum). We just set the header.
            let mut response = next.run(req).await;

            let headers = response.headers_mut();
            if let Ok(val) = HeaderValue::from_str(v.label()) {
                headers.insert(
                    HeaderName::from_static("x-api-version"),
                    val,
                );
            }

            if let Some(ref cfg) = config {
                if cfg.deprecated || cfg.status == VersionStatus::Deprecated {
                    if let Ok(val) = HeaderValue::from_str("true") {
                        headers.insert(
                            HeaderName::from_static("deprecation"),
                            val.clone(),
                        );
                        headers.insert(
                            HeaderName::from_static("x-api-deprecated"),
                            val,
                        );
                    }
                    if let Some(ref sunset) = cfg.sunset_date {
                        if let Ok(val) = HeaderValue::from_str(sunset) {
                            headers.insert(
                                HeaderName::from_static("sunset"),
                                val,
                            );
                        }
                    }
                }
            }

            response
        }
        None => {
            // Check if the path looks like it was trying to be a versioned path
            // (starts with /v followed by digits) but is unsupported.
            let path = path.trim_start_matches('/');
            let first_segment = path.split('/').next().unwrap_or("");
            if first_segment.starts_with('v')
                && first_segment.len() >= 2
                && first_segment[1..].chars().all(|c| c.is_ascii_digit())
            {
                // Unsupported version number
                let configs = version_configs();
                let supported: Vec<&str> = configs
                    .iter()
                    .filter(|c| c.status != VersionStatus::Reserved)
                    .map(|c| c.version.as_str())
                    .collect();

                let body = serde_json::json!({
                    "error": {
                        "code": "unsupported_api_version",
                        "message": format!("API version '{}' is not supported. Supported versions: {}", first_segment, supported.join(", "))
                    }
                });

                return (
                    StatusCode::NOT_FOUND,
                    [(HeaderName::from_static("x-api-version"), first_segment)],
                    Json(body),
                )
                    .into_response();
            }

            // Not a versioned path at all (e.g. /health, /ws/status) — pass through
            next.run(req).await
        }
    }
}

// ---------------------------------------------------------------------------
// GET /api/versions handler
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct VersionsResponse {
    pub versions: Vec<VersionConfig>,
}

/// Handler for GET /api/versions
pub async fn list_versions_handler() -> Json<VersionsResponse> {
    Json(VersionsResponse {
        versions: version_configs(),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request as HttpRequest, StatusCode},
        middleware as axum_middleware,
        routing::get,
        Router,
    };
    use tower::ServiceExt;

    fn versioned_app() -> Router {
        Router::new()
            .route("/v1/test", get(|| async { "ok" }))
            .route("/v2/test", get(|| async { "ok" }))
            .route("/health", get(|| async { "ok" }))
            .layer(axum_middleware::from_fn(version_middleware))
    }

    #[tokio::test]
    async fn test_version_extraction_v1() {
        let result = ApiVersion::from_path("/v1/chat/completions");
        assert_eq!(result, Some(ApiVersion::V1));
    }

    #[tokio::test]
    async fn test_version_extraction_v2() {
        let result = ApiVersion::from_path("/v2/something");
        assert_eq!(result, Some(ApiVersion::V2));
    }

    #[tokio::test]
    async fn test_version_extraction_unversioned() {
        let result = ApiVersion::from_path("/health");
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_version_extraction_unknown() {
        let result = ApiVersion::from_path("/v99/something");
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_version_header_added() {
        let app = versioned_app();
        let req = HttpRequest::builder()
            .uri("/v1/test")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("x-api-version").unwrap(),
            "v1"
        );
    }

    #[tokio::test]
    async fn test_unversioned_path_passes_through() {
        let app = versioned_app();
        let req = HttpRequest::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        // No version header expected on unversioned paths
        assert!(resp.headers().get("x-api-version").is_none());
    }

    #[tokio::test]
    async fn test_unsupported_version_returns_404() {
        let app = versioned_app();
        let req = HttpRequest::builder()
            .uri("/v3/test")
            .body(Body::empty())
            .unwrap();

        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_version_configs_v1_is_current() {
        let configs = version_configs();
        let v1 = configs.iter().find(|c| c.version == "v1").unwrap();
        assert_eq!(v1.status, VersionStatus::Current);
        assert!(!v1.deprecated);
    }

    #[tokio::test]
    async fn test_version_configs_v2_is_reserved() {
        let configs = version_configs();
        let v2 = configs.iter().find(|c| c.version == "v2").unwrap();
        assert_eq!(v2.status, VersionStatus::Reserved);
    }

    #[tokio::test]
    async fn test_list_versions_handler() {
        let resp = list_versions_handler().await;
        assert_eq!(resp.versions.len(), 2);
        assert_eq!(resp.versions[0].version, "v1");
    }

    #[test]
    fn test_version_from_str() {
        assert_eq!("v1".parse::<ApiVersion>().unwrap(), ApiVersion::V1);
        assert_eq!("v2".parse::<ApiVersion>().unwrap(), ApiVersion::V2);
        assert!("v3".parse::<ApiVersion>().is_err());
    }
}
