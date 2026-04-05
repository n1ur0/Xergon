//! Integration tests for the Xergon Relay.
//!
//! These tests require a running relay instance, so they are ignored by default.
//! Run with: cargo test -- --ignored

#[cfg(test)]
mod integration_tests {
    /// Test 4: Relay health endpoint returns valid JSON with expected fields
    #[tokio::test]
    #[ignore]
    async fn test_relay_health() {
        let resp = reqwest::get("http://localhost:9090/v1/health")
            .await
            .expect("Failed to connect to relay health endpoint");
        assert_eq!(resp.status(), 200);

        let body: serde_json::Value = resp
            .json()
            .await
            .expect("Failed to parse health response as JSON");

        assert_eq!(body["status"], "ok");
        assert!(body.get("version").is_some(), "Missing 'version' field");
        assert!(
            body.get("uptime_secs").is_some(),
            "Missing 'uptime_secs' field"
        );
        assert!(
            body.get("active_providers").is_some(),
            "Missing 'active_providers' field"
        );
        assert!(
            body.get("total_providers").is_some(),
            "Missing 'total_providers' field"
        );
    }

    /// Test 5: Relay metrics endpoint returns Prometheus text format
    #[tokio::test]
    #[ignore]
    async fn test_relay_metrics_prometheus_format() {
        let resp = reqwest::get("http://localhost:9090/v1/metrics")
            .await
            .expect("Failed to connect to relay metrics endpoint");
        assert_eq!(resp.status(), 200);

        let text = resp
            .text()
            .await
            .expect("Failed to read metrics response as text");

        // Verify Prometheus format: HELP/TYPE lines and metric values
        assert!(text.contains("# HELP xergon_relay_requests_total"), "Missing HELP for relay_requests_total");
        assert!(text.contains("# TYPE xergon_relay_requests_total counter"), "Missing TYPE for relay_requests_total");
        assert!(text.contains("# TYPE xergon_relay_providers_active gauge"), "Missing TYPE for providers_active");
        assert!(text.contains("# TYPE xergon_relay_uptime_seconds gauge"), "Missing TYPE for uptime");

        // Verify labeled metric values
        assert!(text.contains("endpoint=\"chat\""), "Missing endpoint label for chat");
        assert!(text.contains("endpoint=\"models\""), "Missing endpoint label for models");
        assert!(text.contains("xergon_relay_uptime_seconds"), "Missing uptime metric value");
    }

    /// Test: Relay basic /health liveness endpoint
    #[tokio::test]
    #[ignore]
    async fn test_relay_liveness() {
        let resp = reqwest::get("http://localhost:9090/health")
            .await
            .expect("Failed to connect to relay liveness endpoint");
        assert_eq!(resp.status(), 200);
        let body = resp.text().await.expect("Failed to read response");
        assert_eq!(body.trim(), "ok");
    }
}
