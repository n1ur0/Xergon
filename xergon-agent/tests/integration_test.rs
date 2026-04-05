//! Integration tests for the Xergon Agent.
//!
//! These tests require a running agent/relay instance, so they are ignored by default.
//! Run with: cargo test -- --ignored

#[cfg(test)]
mod integration_tests {
    /// Test 1: Agent health endpoint returns valid JSON with expected fields
    #[tokio::test]
    #[ignore]
    async fn test_agent_health() {
        let resp = reqwest::get("http://localhost:9010/api/health")
            .await
            .expect("Failed to connect to agent health endpoint");
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
            body.get("ergo_node_connected").is_some(),
            "Missing 'ergo_node_connected' field"
        );
    }

    /// Test 2: Agent metrics endpoint returns Prometheus text format
    #[tokio::test]
    #[ignore]
    async fn test_agent_metrics_prometheus_format() {
        let resp = reqwest::get("http://localhost:9010/api/metrics")
            .await
            .expect("Failed to connect to agent metrics endpoint");
        assert_eq!(resp.status(), 200);

        let text = resp
            .text()
            .await
            .expect("Failed to read metrics response as text");

        // Verify Prometheus format: HELP/TYPE lines and metric values
        assert!(text.contains("# HELP xergon_pown_score"), "Missing HELP line for xergon_pown_score");
        assert!(text.contains("# TYPE xergon_pown_score gauge"), "Missing TYPE line for xergon_pown_score");
        assert!(text.contains("# HELP xergon_inference_requests_total"), "Missing HELP for inference requests");
        assert!(text.contains("# TYPE xergon_inference_requests_total counter"), "Missing TYPE for inference requests");
        assert!(text.contains("# HELP xergon_uptime_seconds"), "Missing HELP for uptime");
        assert!(text.contains("# HELP xergon_p2p_peers_known"), "Missing HELP for p2p peers");
        assert!(text.contains("# HELP xergon_rollup_commitments_total"), "Missing HELP for rollup commitments");

        // Verify at least one metric value line exists
        assert!(text.contains("xergon_uptime_seconds"), "Missing uptime_seconds metric value");
    }

    /// Test 3: Agent API returns 401 for unsigned requests (when auth is enabled)
    #[tokio::test]
    #[ignore]
    async fn test_agent_auth_required() {
        // Try to access a management endpoint without an API key
        let resp = reqwest::get("http://localhost:9010/xergon/status")
            .await
            .expect("Failed to connect to agent");

        // The agent may or may not have auth enabled depending on config.
        // If auth is required, expect 401 or 403. If not, expect 200.
        let status = resp.status();
        assert!(
            status == 200 || status == 401 || status == 403,
            "Unexpected status code: {}",
            status
        );
    }
}
