//! GET /v1/user/usage/stats and GET /v1/user/usage/history
//!
//! Authenticated endpoints for users to view their usage analytics.

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

use crate::proxy::AppState;

/// Validate a YYYY-MM-DD date string and return NaiveDate.
pub(crate) fn parse_yyyy_mm_dd(s: &str) -> Result<chrono::NaiveDate, String> {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| format!("Invalid date format '{}': expected YYYY-MM-DD", s))
}

#[derive(Debug, Deserialize)]
pub struct StatsQuery {
    pub start: Option<String>,
    pub end: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// GET /v1/user/usage/stats?start=YYYY-MM-DD&end=YYYY-MM-DD
/// Returns aggregated usage stats for the authenticated user.
pub async fn usage_stats_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<StatsQuery>,
) -> axum::response::Response {
    // Authenticate (JWT or API key)
    let identity = match crate::auth::authenticate_request(&headers, &state.config.auth.jwt_secret, &state.db) {
        Ok(Some(id)) => id,
        Ok(None) => {
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Authentication required"})),
            )
                .into_response();
        }
        Err(e) => {
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": format!("{:?}", e)})),
            )
                .into_response();
        }
    };

    // Default date range: last 30 days
    let end_str = params
        .end
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());
    let start_str = params.start.unwrap_or_else(|| {
        (chrono::Utc::now() - chrono::Duration::days(30)).format("%Y-%m-%d").to_string()
    });

    // Validate date formats
    let end_date = match parse_yyyy_mm_dd(&end_str) {
        Ok(d) => d,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e})),
            )
                .into_response();
        }
    };
    let start_date = match parse_yyyy_mm_dd(&start_str) {
        Ok(d) => d,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": e})),
            )
                .into_response();
        }
    };

    // Cap date range to max 1 year
    let max_range = chrono::Duration::days(365);
    if start_date > end_date {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "start date must be before end date"})),
        )
            .into_response();
    }
    if (end_date - start_date) > max_range {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Date range must not exceed 1 year"})),
        )
            .into_response();
    }

    match state.db.get_usage_stats(Some(&identity.sub), &start_str, &end_str) {
        Ok(stats) => Json(stats).into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "Failed to get usage stats");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to retrieve usage stats"})),
            )
                .into_response()
        }
    }
}

/// GET /v1/user/usage/history?limit=50&offset=0
/// Returns paginated usage history for the authenticated user.
pub async fn usage_history_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<HistoryQuery>,
) -> axum::response::Response {
    // Authenticate (JWT or API key)
    let identity = match crate::auth::authenticate_request(&headers, &state.config.auth.jwt_secret, &state.db) {
        Ok(Some(id)) => id,
        Ok(None) => {
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Authentication required"})),
            )
                .into_response();
        }
        Err(e) => {
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": format!("{:?}", e)})),
            )
                .into_response();
        }
    };

    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let offset = params.offset.unwrap_or(0).max(0);

    match state.db.get_usage_history(Some(&identity.sub), limit, offset) {
        Ok(records) => Json(records).into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "Failed to get usage history");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to retrieve usage history"})),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn test_parse_yyyy_mm_dd_valid() {
        let date = parse_yyyy_mm_dd("2024-06-15").unwrap();
        assert_eq!(date.year(), 2024);
        assert_eq!(date.month(), 6);
        assert_eq!(date.day(), 15);
    }

    #[test]
    fn test_parse_yyyy_mm_dd_leap_year() {
        let date = parse_yyyy_mm_dd("2024-02-29").unwrap();
        assert_eq!(date.year(), 2024);
        assert_eq!(date.month(), 2);
        assert_eq!(date.day(), 29);
    }

    #[test]
    fn test_parse_yyyy_mm_dd_invalid_leap_year() {
        let result = parse_yyyy_mm_dd("2023-02-29");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid date format"));
    }

    #[test]
    fn test_parse_yyyy_mm_dd_invalid_format_slashes() {
        let result = parse_yyyy_mm_dd("2024/06/15");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_yyyy_mm_dd_invalid_format_dmy() {
        let result = parse_yyyy_mm_dd("15-06-2024");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_yyyy_mm_dd_garbage() {
        let result = parse_yyyy_mm_dd("not-a-date");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_yyyy_mm_dd_empty() {
        let result = parse_yyyy_mm_dd("");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_yyyy_mm_dd_month_zero() {
        let result = parse_yyyy_mm_dd("2024-00-01");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_yyyy_mm_dd_day_zero() {
        let result = parse_yyyy_mm_dd("2024-01-00");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_yyyy_mm_dd_month_thirteen() {
        let result = parse_yyyy_mm_dd("2024-13-01");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_yyyy_mm_dd_error_contains_input() {
        let result = parse_yyyy_mm_dd("banana");
        let err_msg = result.unwrap_err();
        assert!(err_msg.contains("banana"), "error should echo bad input: {err_msg}");
    }
}
