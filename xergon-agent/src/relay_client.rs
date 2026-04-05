//! Relay registration client
//!
//! Connects xergon-agent to the relay's provider registration protocol.
//! Handles:
//! - Initial registration with provider info + models
//! - Periodic heartbeat to maintain registration
//! - On-chain heartbeat transaction (Phase 2, optional)
//! - Graceful deregistration on shutdown
//!
//! Config section: [relay]
//!   relay_url = "http://relay-host:9090"   # Relay base URL
//!   token="shared...oken"            # Must match relay's providers.registration_token
//!   heartbeat_interval_secs = 60             # How often to send heartbeat (default: 60)
//!   register_on_start = true                 # Auto-register on startup (default: true)

use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

/// Relay client configuration
#[derive(Debug, Clone, Deserialize)]
pub struct RelayClientConfig {
    /// Base URL of the relay (e.g., "http://relay-host:9090")
    pub relay_url: String,
    /// Shared secret token for provider registration
    pub token: String,
    /// How often to send heartbeat (seconds, default: 60)
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval_secs: u64,
    /// Auto-register on startup (default: true)
    #[serde(default = "default_register_on_start")]
    pub register_on_start: bool,
}

fn default_heartbeat_interval() -> u64 {
    60
}
fn default_register_on_start() -> bool {
    true
}

impl Default for RelayClientConfig {
    fn default() -> Self {
        Self {
            relay_url: String::new(),
            token: String::new(),
            heartbeat_interval_secs: default_heartbeat_interval(),
            register_on_start: default_register_on_start(),
        }
    }
}

/// Registration request sent to the relay
#[derive(Debug, serde::Serialize)]
struct RegistrationPayload {
    provider_id: String,
    provider_name: String,
    region: String,
    endpoint: String,
    ergo_address: String,
    models: Vec<String>,
    ttl_secs: u64,
}

/// Heartbeat request sent to the relay
#[derive(Debug, serde::Serialize)]
struct HeartbeatPayload {
    models: Vec<String>,
}

/// Response from registration
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // TODO: will be used for provider registration info
struct RegistrationResponse {
    status: String,
    #[allow(dead_code)]
    provider_id: String,
    heartbeat_interval_secs: u64,
    ttl_secs: u64,
    #[allow(dead_code)]
    message: String,
}

/// Response from heartbeat
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // TODO: will be used for heartbeat status monitoring
struct HeartbeatResponse {
    status: String,
    next_heartbeat_before: String,
}

/// The relay client
pub struct RelayClient {
    config: RelayClientConfig,
    http_client: Client,
    provider_id: String,
    provider_name: String,
    region: String,
    ergo_address: String,
    /// The endpoint URL where this agent is reachable
    agent_endpoint: String,
    /// Whether we're currently registered
    registered: AtomicBool,
    /// Suggested TTL from last registration
    ttl_secs: std::sync::Mutex<u64>,
}

impl RelayClient {
    /// Create a new relay client.
    ///
    /// `agent_endpoint` is the URL where the relay can reach this agent
    /// (e.g., "http://192.168.1.100:9099").
    pub fn new(
        config: RelayClientConfig,
        provider_id: String,
        provider_name: String,
        region: String,
        ergo_address: String,
        agent_endpoint: String,
    ) -> Result<Self> {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(10))
            .connect_timeout(Duration::from_secs(5))
            .build()
            .context("Failed to build HTTP client for relay")?;

        Ok(Self {
            config,
            http_client,
            provider_id,
            provider_name,
            region,
            ergo_address,
            agent_endpoint,
            registered: AtomicBool::new(false),
            ttl_secs: std::sync::Mutex::new(180),
        })
    }

    /// Whether relay registration is configured and enabled
    #[allow(dead_code)] // TODO: will be used for relay status endpoint
    pub fn is_enabled(&self) -> bool {
        self.config.register_on_start
            && !self.config.relay_url.is_empty()
            && !self.config.token.is_empty()
    }

    /// Register this provider with the relay.
    ///
    /// Sends provider info and model list. On success, starts sending
    /// periodic heartbeats.
    pub async fn register(&self, models: Vec<String>) -> Result<()> {
        let url = format!(
            "{}/v1/providers/register",
            self.config.relay_url.trim_end_matches('/')
        );

        let ttl = {
            let current = self.ttl_secs.lock().unwrap();
            *current
        };

        let payload = RegistrationPayload {
            provider_id: self.provider_id.clone(),
            provider_name: self.provider_name.clone(),
            region: self.region.clone(),
            endpoint: self.agent_endpoint.clone(),
            ergo_address: self.ergo_address.clone(),
            models: models.clone(),
            ttl_secs: ttl,
        };

        let resp = self
            .http_client
            .post(&url)
            .header("X-Provider-Token", &self.config.token)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .context("Failed to send registration to relay")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Relay registration failed ({}): {}", status, body);
        }

        let reg_resp: RegistrationResponse = resp
            .json()
            .await
            .context("Failed to parse registration response")?;

        // Store the TTL from the relay's response
        *self.ttl_secs.lock().unwrap() = reg_resp.ttl_secs;

        info!(
            relay = %self.config.relay_url,
            status = %reg_resp.status,
            heartbeat_interval = reg_resp.heartbeat_interval_secs,
            ttl = reg_resp.ttl_secs,
            "Registered with relay"
        );

        self.registered.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Send a heartbeat to the relay to keep registration alive.
    pub async fn heartbeat(&self, models: Vec<String>) -> Result<()> {
        if !self.registered.load(Ordering::Relaxed) {
            return Ok(());
        }

        let url = format!(
            "{}/v1/providers/heartbeat",
            self.config.relay_url.trim_end_matches('/')
        );

        let payload = HeartbeatPayload { models };

        let resp = self
            .http_client
            .post(&url)
            .header("X-Provider-Token", &self.config.token)
            .header("X-Provider-Id", &self.provider_id)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await;

        match resp {
            Ok(r) if r.status().is_success() => {
                // Heartbeat OK — no need to parse response
                Ok(())
            }
            Ok(r) if r.status().as_u16() == 404 => {
                // We're no longer registered — need to re-register
                warn!("Heartbeat returned 404 — re-registering with relay");
                self.registered.store(false, Ordering::Relaxed);
                Err(anyhow::anyhow!(
                    "Provider no longer registered, needs re-registration"
                ))
            }
            Ok(r) => {
                let status = r.status();
                let body = r.text().await.unwrap_or_default();
                warn!(
                    relay = %self.config.relay_url,
                    status = %status,
                    body = %body,
                    "Heartbeat failed"
                );
                Err(anyhow::anyhow!("Heartbeat failed: {}", status))
            }
            Err(e) => {
                warn!(relay = %self.config.relay_url, error = %e, "Heartbeat request failed");
                Err(e.into())
            }
        }
    }

    /// Deregister from the relay (call on graceful shutdown).
    pub async fn deregister(&self) {
        if !self.registered.load(Ordering::Relaxed) {
            return;
        }

        let url = format!(
            "{}/v1/providers/register",
            self.config.relay_url.trim_end_matches('/')
        );

        match self
            .http_client
            .delete(&url)
            .header("X-Provider-Token", &self.config.token)
            .header("X-Provider-Id", &self.provider_id)
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                info!(relay = %self.config.relay_url, "Deregistered from relay");
            }
            Ok(resp) => {
                warn!(status = %resp.status(), "Deregistration returned non-success");
            }
            Err(e) => {
                warn!(error = %e, "Failed to send deregistration");
            }
        }

        self.registered.store(false, Ordering::Relaxed);
    }

    /// Spawn the heartbeat loop.
    ///
    /// Periodically sends heartbeats and re-registers if needed.
    /// The closure `get_models` is called each cycle to get the current model list.
    ///
    /// The optional `on_chain_heartbeat` closure is called after a successful
    /// HTTP heartbeat to submit an on-chain heartbeat transaction.
    pub fn spawn_heartbeat_loop<F, FH>(self: Arc<Self>, get_models: F, on_chain_heartbeat: Option<FH>)
    where
        F: Fn() -> Vec<String> + Send + Sync + 'static,
        FH: Fn() + Send + Sync + 'static,
    {
        tokio::spawn(async move {
            let interval = Duration::from_secs(self.config.heartbeat_interval_secs);

            // Wait before first heartbeat (let registration settle)
            tokio::time::sleep(interval).await;

            loop {
                let models = get_models();

                match self.heartbeat(models).await {
                    Ok(()) => {
                        // HTTP heartbeat OK — try on-chain heartbeat if enabled
                        if let Some(ref cb) = on_chain_heartbeat {
                            cb();
                        }
                        // All good, sleep until next heartbeat
                    }
                    Err(_) => {
                        // Heartbeat failed — try to re-register
                        warn!("Attempting to re-register with relay...");
                        let models = get_models();
                        if let Err(e) = self.register(models).await {
                            error!(error = %e, "Re-registration failed");
                        }
                    }
                }

                tokio::time::sleep(interval).await;
            }
        });
    }
}
