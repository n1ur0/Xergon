use chrono::{DateTime, Utc};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const DEFAULT_AGENT_URL: &str = "http://127.0.0.1:9099";

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

/// Body for POST /xergon/lifecycle/register
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RegisterRequest {
    pub provider_pubkey: String,
    pub provider_name: String,
    pub endpoint_url: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub stake_nanoerg: u64,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}

/// Body for POST /xergon/lifecycle/heartbeat
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HeartbeatRequest {
    pub provider_pubkey: String,
    #[serde(default)]
    pub pown_score: f64,
    #[serde(default)]
    pub models_count: u32,
}

/// Body for POST /xergon/lifecycle/rent-protect
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RentProtectRequest {
    pub provider_pubkey: String,
}

/// Body for POST /xergon/lifecycle/deregister
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DeregisterRequest {
    pub provider_pubkey: String,
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Generic API envelope returned by the agent.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ApiResponse {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// On-chain provider status.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProviderStatus {
    pub provider_pubkey: String,
    pub provider_name: String,
    pub endpoint_url: String,
    pub models: Vec<String>,
    pub stake_nanoerg: u64,
    pub registered_at: Option<DateTime<Utc>>,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub pown_score: f64,
    pub models_count: u32,
    pub active: bool,
}

/// Compact info for listing providers.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProviderInfo {
    pub provider_pubkey: String,
    pub provider_name: String,
    pub endpoint_url: String,
    pub active: bool,
    pub models_count: u32,
    pub pown_score: f64,
}

/// Rent-protection urgency record.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RentCheckEntry {
    pub provider_pubkey: String,
    pub provider_name: String,
    pub blocks_remaining: u32,
    pub urgency: String,
}

/// A single history event.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProviderHistoryEvent {
    pub event_type: String,
    pub timestamp: DateTime<Utc>,
    pub details: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_id: Option<String>,
}

/// Aggregate provider-network statistics.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ProviderStats {
    pub total_providers: u32,
    pub active_providers: u32,
    pub total_models: u32,
    pub total_stake_nanoerg: u64,
    pub avg_pown_score: f64,
    pub heartbeat_rate_pct: f64,
}

// ---------------------------------------------------------------------------
// CLI arguments
// ---------------------------------------------------------------------------

/// Top-level arguments for the `provider` command group.
#[derive(Args, Clone, Debug)]
pub struct ProviderArgs {
    #[command(subcommand)]
    pub command: ProviderCommand,

    /// Output raw JSON instead of a human-readable table.
    #[arg(long, global = true)]
    pub json: bool,

    /// Override the agent URL (default: http://127.0.0.1:9099).
    #[arg(long, global = true, env = "XERGON_AGENT_URL")]
    pub agent_url: Option<String>,
}

/// Individual subcommands for provider lifecycle management.
#[derive(Subcommand, Clone, Debug)]
pub enum ProviderCommand {
    /// Register a new provider on-chain.
    Register {
        /// Provider public key (hex-encoded).
        #[arg(long)]
        provider_pubkey: String,
        /// Human-readable provider name.
        #[arg(long)]
        provider_name: String,
        /// Provider endpoint URL.
        #[arg(long)]
        endpoint_url: String,
        /// Comma-separated list of supported model IDs.
        #[arg(long, default_value = "")]
        models: String,
        /// Stake amount in nanoERG.
        #[arg(long, default_value_t = 0)]
        stake_nanoerg: u64,
        /// Optional JSON metadata blob.
        #[arg(long, default_value = "{}")]
        metadata: String,
    },

    /// Query the current status of a registered provider.
    Status {
        /// Provider public key (hex-encoded).
        #[arg(long)]
        provider_pubkey: String,
    },

    /// Submit a heartbeat for an active provider.
    Heartbeat {
        /// Provider public key (hex-encoded).
        #[arg(long)]
        provider_pubkey: String,
        /// Current PoWn score.
        #[arg(long, default_value_t = 0.0)]
        pown_score: f64,
        /// Number of models currently served.
        #[arg(long, default_value_t = 0)]
        models_count: u32,
    },

    /// Trigger rent protection for a provider box.
    RentProtect {
        /// Provider public key (hex-encoded).
        #[arg(long)]
        provider_pubkey: String,
    },

    /// Deregister (unregister) a provider from the network.
    Deregister {
        /// Provider public key (hex-encoded).
        #[arg(long)]
        provider_pubkey: String,
    },

    /// List all registered providers (optionally filter by pubkey).
    Inspect {
        /// Optional: filter to a specific provider pubkey.
        #[arg(long)]
        pubkey: Option<String>,
    },

    /// List all providers that need rent protection.
    RentCheck,

    /// Show the event history for a provider.
    History {
        /// Provider public key (hex-encoded).
        #[arg(long)]
        provider_pubkey: String,
    },

    /// Show aggregate provider-network statistics.
    Stats,
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

/// Main entry point — dispatches to the appropriate subcommand handler.
pub async fn run(args: ProviderArgs) -> Result<(), Box<dyn std::error::Error>> {
    let base_url = args
        .agent_url
        .as_deref()
        .unwrap_or(DEFAULT_AGENT_URL)
        .trim_end_matches('/')
        .to_string();

    let client = reqwest::Client::new();

    match args.command {
        ProviderCommand::Register {
            provider_pubkey,
            provider_name,
            endpoint_url,
            models,
            stake_nanoerg,
            metadata,
        } => {
            let models_vec: Vec<String> = if models.is_empty() {
                Vec::new()
            } else {
                models.split(',').map(|s| s.trim().to_string()).collect()
            };
            let metadata_map: std::collections::HashMap<String, String> =
                serde_json::from_str(&metadata).unwrap_or_default();

            let body = RegisterRequest {
                provider_pubkey,
                provider_name,
                endpoint_url,
                models: models_vec,
                stake_nanoerg,
                metadata: metadata_map,
            };
            let resp = client
                .post(format!("{}/xergon/lifecycle/register", base_url))
                .json(&body)
                .send()
                .await?;
            handle_response::<ProviderStatus>(resp, args.json).await?;
        }

        ProviderCommand::Status { provider_pubkey } => {
            let url = format!(
                "{}/xergon/lifecycle/status/{}",
                base_url, provider_pubkey
            );
            let resp = client.get(&url).send().await?;
            handle_response::<ProviderStatus>(resp, args.json).await?;
        }

        ProviderCommand::Heartbeat {
            provider_pubkey,
            pown_score,
            models_count,
        } => {
            let body = HeartbeatRequest {
                provider_pubkey,
                pown_score,
                models_count,
            };
            let resp = client
                .post(format!("{}/xergon/lifecycle/heartbeat", base_url))
                .json(&body)
                .send()
                .await?;
            handle_response::<serde_json::Value>(resp, args.json).await?;
        }

        ProviderCommand::RentProtect { provider_pubkey } => {
            let body = RentProtectRequest { provider_pubkey };
            let resp = client
                .post(format!("{}/xergon/lifecycle/rent-protect", base_url))
                .json(&body)
                .send()
                .await?;
            handle_response::<serde_json::Value>(resp, args.json).await?;
        }

        ProviderCommand::Deregister { provider_pubkey } => {
            let body = DeregisterRequest { provider_pubkey };
            let resp = client
                .post(format!("{}/xergon/lifecycle/deregister", base_url))
                .json(&body)
                .send()
                .await?;
            handle_response::<serde_json::Value>(resp, args.json).await?;
        }

        ProviderCommand::Inspect { pubkey } => {
            let url = if let Some(pk) = pubkey {
                format!("{}/xergon/lifecycle/providers?pubkey={}", base_url, pk)
            } else {
                format!("{}/xergon/lifecycle/providers", base_url)
            };
            let resp = client.get(&url).send().await?;
            handle_response::<Vec<ProviderInfo>>(resp, args.json).await?;
        }

        ProviderCommand::RentCheck => {
            let url = format!("{}/xergon/lifecycle/rent-check", base_url);
            let resp = client.get(&url).send().await?;
            handle_response::<Vec<RentCheckEntry>>(resp, args.json).await?;
        }

        ProviderCommand::History { provider_pubkey } => {
            let url = format!(
                "{}/xergon/lifecycle/history/{}",
                base_url, provider_pubkey
            );
            let resp = client.get(&url).send().await?;
            handle_response::<Vec<ProviderHistoryEvent>>(resp, args.json).await?;
        }

        ProviderCommand::Stats => {
            let url = format!("{}/xergon/lifecycle/stats", base_url);
            let resp = client.get(&url).send().await?;
            handle_response::<ProviderStats>(resp, args.json).await?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Response handling
// ---------------------------------------------------------------------------

/// Parse an HTTP response and print the result in the requested format.
async fn handle_response<T: Serialize + for<'de> Deserialize<'de> + std::fmt::Debug>(
    resp: reqwest::Response,
    json_mode: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let status = resp.status();
    let body_text = resp.text().await?;

    if !status.is_success() {
        eprintln!("Error (HTTP {}): {}", status, body_text);
        std::process::exit(1);
    }

    if json_mode {
        // Pretty-print the raw JSON body
        if let Ok(pretty) = serde_json::from_str::<serde_json::Value>(&body_text) {
            println!("{}", serde_json::to_string_pretty(&pretty)?);
        } else {
            println!("{}", body_text);
        }
        return Ok(());
    }

    // Try to parse into the typed envelope
    let api: ApiResponse = match serde_json::from_str(&body_text) {
        Ok(a) => a,
        Err(_) => {
            // Fallback: raw body
            println!("{}", body_text);
            return Ok(());
        }
    };

    if !api.success {
        println!("Error: {}", api.message);
        return Ok(());
    }

    if let Some(data) = &api.data {
        // Try to deserialize the inner data into the concrete type
        match serde_json::from_value::<T>(data.clone()) {
            Ok(typed) => print_human_readable(&typed),
            Err(_) => {
                // Fallback: pretty-print the raw data
                if let Ok(pretty) = serde_json::to_string_pretty(data) {
                    println!("{}", pretty);
                }
            }
        }
    } else {
        println!("{}", api.message);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Human-readable formatters
// ---------------------------------------------------------------------------

/// Dispatch to the correct table formatter based on concrete type.
fn print_human_readable<T: Serialize + std::fmt::Debug>(value: &T) {
    let debug_str = format!("{:?}", value);
    // We use the type name to route formatting. Since we cannot downcast,
    // we print the debug representation when no specialised formatter matches.
    // The typed callers below use the concrete overloads.
    #[allow(unused)]
    let _ = debug_str;

    // Defer to concrete formatters via serde_json round-trip for type routing.
    if let Ok(json) = serde_json::to_value(value) {
        format_by_json_type(&json);
    } else {
        println!("{:?}", value);
    }
}

fn format_by_json_type(value: &serde_json::Value) {
    if let Some(arr) = value.as_array() {
        if arr.is_empty() {
            println!("(no entries)");
            return;
        }
        // Use keys from first element as headers
        let first = &arr[0];
        if let Some(obj) = first.as_object() {
            let keys: Vec<&String> = obj.keys().collect();
            let rows: Vec<Vec<String>> = arr
                .iter()
                .map(|item| {
                    if let Some(o) = item.as_object() {
                        keys.iter()
                            .map(|k| {
                                o.get(*k)
                                    .map(|v| truncate(v.to_string(), 60))
                                    .unwrap_or_default()
                            })
                            .collect()
                    } else {
                        vec![truncate(item.to_string(), 80)]
                    }
                })
                .collect();
            print_table(&keys.iter().map(|s| s.as_str()).collect::<Vec<_>>(), &rows);
        } else {
            for item in arr {
                println!("  {}", truncate(item.to_string(), 120));
            }
        }
    } else if let Some(obj) = value.as_object() {
        let keys: Vec<&String> = obj.keys().collect();
        let vals: Vec<String> = keys
            .iter()
            .map(|k| {
                obj.get(*k)
                    .map(|v| truncate(v.to_string(), 80))
                    .unwrap_or_default()
            })
            .collect();
        print_table(
            &keys.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            &[vals],
        );
    } else {
        println!("{}", truncate(value.to_string(), 120));
    }
}

/// Print a simple aligned table.
fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    let col_widths: Vec<usize> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let max_data = rows
                .iter()
                .map(|r| r.get(i).map(|s| s.len()).unwrap_or(0))
                .max()
                .unwrap_or(0);
            h.len().max(max_data).min(80)
        })
        .collect();

    // Header row
    let header_line: String = headers
        .iter()
        .enumerate()
        .map(|(i, h)| format!("{:width$}", h, width = col_widths[i]))
        .collect::<Vec<_>>()
        .join("  ");
    println!("{}", header_line);

    // Separator
    let sep: String = col_widths.iter().map(|w| "-".repeat(*w)).collect::<Vec<_>>().join("--");
    println!("{}", sep);

    // Data rows
    for row in rows {
        let line: String = row
            .iter()
            .enumerate()
            .map(|(i, cell)| format!("{:width$}", truncate(cell.clone(), col_widths[i]), width = col_widths[i]))
            .collect::<Vec<_>>()
            .join("  ");
        println!("{}", line);
    }
}

/// Truncate a string to `max_len` characters with "..." suffix.
fn truncate(s: String, max_len: usize) -> String {
    if s.len() <= max_len {
        s
    } else if max_len > 3 {
        format!("{}...", &s[..max_len - 3])
    } else {
        s.chars().take(max_len).collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- RegisterRequest ----------------------------------------------------

    #[test]
    fn register_request_roundtrip() {
        let req = RegisterRequest {
            provider_pubkey: "abc123".into(),
            provider_name: "test-node".into(),
            endpoint_url: "https://example.com".into(),
            models: vec!["llama-3-8b".into(), "mistral-7b".into()],
            stake_nanoerg: 1_000_000_000,
            metadata: {
                let mut m = std::collections::HashMap::new();
                m.insert("region".into(), "us-east".into());
                m
            },
        };
        let json = serde_json::to_string(&req).unwrap();
        let restored: RegisterRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.provider_pubkey, "abc123");
        assert_eq!(restored.models.len(), 2);
        assert_eq!(restored.stake_nanoerg, 1_000_000_000);
        assert_eq!(restored.metadata.get("region").unwrap(), "us-east");
    }

    #[test]
    fn register_request_defaults() {
        let json = r#"{"provider_pubkey":"pk","provider_name":"n","endpoint_url":"u"}"#;
        let req: RegisterRequest = serde_json::from_str(json).unwrap();
        assert!(req.models.is_empty());
        assert_eq!(req.stake_nanoerg, 0);
        assert!(req.metadata.is_empty());
    }

    // -- HeartbeatRequest ---------------------------------------------------

    #[test]
    fn heartbeat_request_roundtrip() {
        let req = HeartbeatRequest {
            provider_pubkey: "pk1".into(),
            pown_score: 95.5,
            models_count: 3,
        };
        let json = serde_json::to_string(&req).unwrap();
        let restored: HeartbeatRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.pown_score, 95.5);
        assert_eq!(restored.models_count, 3);
    }

    #[test]
    fn heartbeat_request_defaults() {
        let json = r#"{"provider_pubkey":"pk"}"#;
        let req: HeartbeatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.pown_score, 0.0);
        assert_eq!(req.models_count, 0);
    }

    // -- RentProtectRequest / DeregisterRequest ------------------------------

    #[test]
    fn rent_protect_request_roundtrip() {
        let req = RentProtectRequest {
            provider_pubkey: "pk2".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let restored: RentProtectRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.provider_pubkey, "pk2");
    }

    #[test]
    fn deregister_request_roundtrip() {
        let req = DeregisterRequest {
            provider_pubkey: "pk3".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let restored: DeregisterRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.provider_pubkey, "pk3");
    }

    // -- ProviderStatus -----------------------------------------------------

    #[test]
    fn provider_status_roundtrip() {
        let status = ProviderStatus {
            provider_pubkey: "pk".into(),
            provider_name: "node-1".into(),
            endpoint_url: "https://api.example.com".into(),
            models: vec!["llama-3-8b".into()],
            stake_nanoerg: 500_000_000,
            registered_at: Some(Utc::now()),
            last_heartbeat: None,
            pown_score: 88.0,
            models_count: 1,
            active: true,
        };
        let json = serde_json::to_string(&status).unwrap();
        let restored: ProviderStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.provider_name, "node-1");
        assert!(restored.registered_at.is_some());
        assert!(restored.active);
    }

    // -- ProviderInfo -------------------------------------------------------

    #[test]
    fn provider_info_roundtrip() {
        let info = ProviderInfo {
            provider_pubkey: "pk".into(),
            provider_name: "node-2".into(),
            endpoint_url: "https://node2.example.com".into(),
            active: false,
            models_count: 0,
            pown_score: 0.0,
        };
        let json = serde_json::to_string(&info).unwrap();
        let restored: ProviderInfo = serde_json::from_str(&json).unwrap();
        assert!(!restored.active);
    }

    // -- RentCheckEntry -----------------------------------------------------

    #[test]
    fn rent_check_entry_roundtrip() {
        let entry = RentCheckEntry {
            provider_pubkey: "pk".into(),
            provider_name: "node-3".into(),
            blocks_remaining: 42,
            urgency: "critical".into(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let restored: RentCheckEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.blocks_remaining, 42);
        assert_eq!(restored.urgency, "critical");
    }

    // -- ProviderHistoryEvent ------------------------------------------------

    #[test]
    fn provider_history_event_roundtrip() {
        let event = ProviderHistoryEvent {
            event_type: "heartbeat".into(),
            timestamp: Utc::now(),
            details: "score updated".into(),
            tx_id: Some("abcd1234".into()),
        };
        let json = serde_json::to_string(&event).unwrap();
        let restored: ProviderHistoryEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.event_type, "heartbeat");
        assert_eq!(restored.tx_id.as_deref(), Some("abcd1234"));
    }

    #[test]
    fn provider_history_event_skip_tx_id() {
        let json = r#"{"event_type":"register","timestamp":"2025-01-01T00:00:00Z","details":"new provider"}"#;
        let event: ProviderHistoryEvent = serde_json::from_str(json).unwrap();
        assert!(event.tx_id.is_none());
    }

    // -- ProviderStats ------------------------------------------------------

    #[test]
    fn provider_stats_roundtrip() {
        let stats = ProviderStats {
            total_providers: 10,
            active_providers: 8,
            total_models: 25,
            total_stake_nanoerg: 5_000_000_000,
            avg_pown_score: 76.3,
            heartbeat_rate_pct: 99.5,
        };
        let json = serde_json::to_string(&stats).unwrap();
        let restored: ProviderStats = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.total_providers, 10);
        assert_eq!(restored.active_providers, 8);
        assert_eq!(restored.avg_pown_score, 76.3);
    }

    #[test]
    fn provider_stats_default() {
        let stats = ProviderStats::default();
        assert_eq!(stats.total_providers, 0);
        assert_eq!(stats.avg_pown_score, 0.0);
    }

    // -- ApiResponse --------------------------------------------------------

    #[test]
    fn api_response_with_data() {
        let json = r#"{"success":true,"message":"ok","data":{"name":"test"}}"#;
        let resp: ApiResponse = serde_json::from_str(json).unwrap();
        assert!(resp.success);
        assert!(resp.data.is_some());
    }

    #[test]
    fn api_response_without_data() {
        let json = r#"{"success":true,"message":"done"}"#;
        let resp: ApiResponse = serde_json::from_str(json).unwrap();
        assert!(resp.success);
        assert!(resp.data.is_none());
    }

    #[test]
    fn api_response_serialize_skips_none_data() {
        let resp = ApiResponse {
            success: false,
            message: "error".into(),
            data: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(!json.contains("data"));
    }

    // -- truncate helper ----------------------------------------------------

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("hello".into(), 10), "hello");
    }

    #[test]
    fn truncate_long_string() {
        let result = truncate("abcdefghij".into(), 7);
        assert_eq!(result, "abcd...");
        assert!(result.len() <= 7);
    }

    #[test]
    fn truncate_very_short_max() {
        assert_eq!(truncate("abcdef".into(), 3), "abc");
    }
}
