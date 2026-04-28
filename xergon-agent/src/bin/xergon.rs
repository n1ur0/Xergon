//! `xergon` — User CLI for Xergon Network
//!
//! A separate binary from `xergon-agent` (the provider sidecar).
//! This CLI is what AI users install to interact with the Xergon relay.
//!
//! Commands:
//!   setup    — First-run: generate wallet, configure relay
//!   ask      — Send prompt, stream response
//!   models   — List available models from relay
//!   balance  — Show ERG balance (from relay)
//!   deposit  — Show ERG address to fund
//!   token    — Generate OpenAI-compatible API token
//!   status   — Comprehensive agent/provider status dashboard
//!   update   — Self-update to latest release
//!   gpu      — GPU rental: list, pricing, rent, my-rentals, refund, extend
//!   bridge   — Cross-chain payment bridge

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::io::Read;

use xergon_agent::signing::{self, sign_request};
use xergon_agent::wallet::{self, Wallet};

// ---------------------------------------------------------------------------
// CLI definition
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(
    name = "xergon",
    about = "Xergon Network — Decentralized AI Inference",
    version,
    propagate_version = true,
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Relay URL
    #[arg(long, env = "XERGON_RELAY_URL", global = true, default_value = "https://relay.xergon.gg")]
    relay_url: String,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// First-run: generate wallet and configure relay
    Setup,

    /// Send a prompt and stream the response
    Ask {
        /// The prompt text (omit to read from stdin)
        prompt: Option<String>,

        /// Model to use (e.g., "qwen3.5-4b")
        #[arg(long)]
        model: Option<String>,

        /// Stream responses (default: true)
        #[arg(long, default_value = "true")]
        stream: bool,

        /// System prompt to set context/persona
        #[arg(long)]
        system: Option<String>,

        /// Request structured JSON output
        #[arg(long)]
        json: bool,

        /// Route to a specific provider public key (hex)
        #[arg(long)]
        provider: Option<String>,
    },

    /// List available models from the relay
    Models,

    /// Show ERG balance
    Balance,

    /// Show ERG deposit address
    Deposit,

    /// Generate an OpenAI-compatible API token
    Token {
        /// Token expiry in seconds (default: 86400 = 24h)
        #[arg(long, default_value = "86400")]
        expiry: u64,
    },

    /// Show wallet info, relay connection, and balance
    Status,

    /// Self-update to the latest GitHub release
    Update,

    /// GPU rental commands
    #[command(subcommand)]
    Gpu(GpuCommands),

    /// Cross-chain payment bridge commands
    #[command(subcommand)]
    Bridge(BridgeCommands),
}

#[derive(Subcommand, Debug)]
enum BridgeCommands {
    /// Create a new cross-chain payment invoice
    InvoiceCreate {
        /// Provider public key (hex)
        provider_pk: String,
        /// Amount in ERG
        amount_erg: f64,
        /// Foreign chain: btc, eth, or ada
        foreign_chain: String,
    },
    /// Check invoice status
    InvoiceStatus {
        /// Invoice ID
        invoice_id: String,
    },
    /// Confirm payment (bridge operator only)
    InvoiceConfirm {
        /// Invoice ID
        invoice_id: String,
        /// Foreign chain transaction ID
        foreign_tx_id: String,
    },
    /// Refund an expired invoice (buyer)
    InvoiceRefund {
        /// Invoice ID
        invoice_id: String,
    },
    /// Show bridge status and supported chains
    Status,
}

#[derive(Subcommand, Debug)]
enum GpuCommands {
    /// Browse available GPUs for rent
    List {
        /// Filter by region (e.g. us-east, eu-west)
        #[arg(long)]
        region: Option<String>,
        /// Minimum VRAM in GB
        #[arg(long)]
        min_vram: Option<u32>,
        /// Maximum price per hour in ERG
        #[arg(long)]
        max_price: Option<f64>,
        /// Filter by GPU type (substring match, e.g. "RTX 4090")
        #[arg(long)]
        gpu_type: Option<String>,
    },
    /// Get pricing for all GPU types
    Pricing,
    /// Rent a GPU
    Rent {
        /// Listing ID to rent
        listing_id: String,
        /// Number of hours to rent
        hours: u32,
    },
    /// View your active rentals
    MyRentals,
    /// Refund a rental (before deadline)
    Refund {
        /// Rental box ID to refund
        rental_id: String,
    },
    /// Extend a rental
    Extend {
        /// Rental box ID to extend
        rental_id: String,
        /// Additional hours to add
        hours: u32,
    },
    /// Rate a completed rental (1-5 stars)
    Rate {
        /// Rental box ID that was completed
        rental_id: String,
        /// Rating: 1-5 stars
        rating: u8,
        /// Role of the person being rated: "provider" or "renter"
        #[arg(long)]
        role: String,
        /// Optional comment
        #[arg(long)]
        comment: Option<String>,
    },
    /// View reputation for a provider or renter
    Reputation {
        /// Public key of the provider or renter
        public_key: String,
    },
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Setup => cmd_setup(&cli.relay_url).await,
        Commands::Ask { prompt, model, stream, system, json, provider } => {
            cmd_ask(&cli.relay_url, prompt, model, stream, system, json, provider).await
        }
        Commands::Models => cmd_models(&cli.relay_url).await,
        Commands::Balance => cmd_balance(&cli.relay_url).await,
        Commands::Deposit => cmd_deposit().await,
        Commands::Token { expiry } => cmd_token(expiry),
        Commands::Status => cmd_status(&cli.relay_url).await,
        Commands::Update => cmd_update().await,
        Commands::Gpu(gpu_cmd) => match gpu_cmd {
            GpuCommands::List { region, min_vram, max_price, gpu_type } => {
                cmd_gpu_list(&cli.relay_url, region, min_vram, max_price, gpu_type).await
            }
            GpuCommands::Pricing => cmd_gpu_pricing(&cli.relay_url).await,
            GpuCommands::Rent { listing_id, hours } => {
                cmd_gpu_rent(&cli.relay_url, &listing_id, hours).await
            }
            GpuCommands::MyRentals => cmd_gpu_my_rentals(&cli.relay_url).await,
            GpuCommands::Refund { rental_id } => {
                cmd_gpu_refund(&cli.relay_url, &rental_id).await
            }
            GpuCommands::Extend { rental_id, hours } => {
                cmd_gpu_extend(&cli.relay_url, &rental_id, hours).await
            }
            GpuCommands::Rate { rental_id, rating, role, comment } => {
                cmd_gpu_rate(&cli.relay_url, &rental_id, rating, &role, comment.as_deref()).await
            }
            GpuCommands::Reputation { public_key } => {
                cmd_gpu_reputation(&cli.relay_url, &public_key).await
            }
        },
        Commands::Bridge(bridge_cmd) => match bridge_cmd {
            BridgeCommands::InvoiceCreate { provider_pk, amount_erg, foreign_chain } => {
                cmd_bridge_invoice_create(&cli.relay_url, &provider_pk, amount_erg, &foreign_chain).await
            }
            BridgeCommands::InvoiceStatus { invoice_id } => {
                cmd_bridge_invoice_status(&cli.relay_url, &invoice_id).await
            }
            BridgeCommands::InvoiceConfirm { invoice_id, foreign_tx_id } => {
                cmd_bridge_invoice_confirm(&cli.relay_url, &invoice_id, &foreign_tx_id).await
            }
            BridgeCommands::InvoiceRefund { invoice_id } => {
                cmd_bridge_invoice_refund(&cli.relay_url, &invoice_id).await
            }
            BridgeCommands::Status => cmd_bridge_status(&cli.relay_url).await,
        },
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Load wallet (exits with user-friendly error if not set up).
fn require_wallet() -> Result<Wallet> {
    if !wallet::wallet_exists() {
        anyhow::bail!(
            "No wallet found. Run `xergon setup` first to create one."
        );
    }
    wallet::load_wallet_interactive()
}

/// Build an HTTP client with signing headers.
fn build_signed_request(
    wallet: &Wallet,
    method: &str,
    path: &str,
    relay_url: &str,
    body: &[u8],
) -> Result<(reqwest::RequestBuilder, reqwest::Client)> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    let url = format!("{}{}", relay_url.trim_end_matches('/'), path);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let signature = sign_request(&wallet.secret_key, timestamp, method, path, body);

    let req = client
        .request(method.parse().unwrap(), &url)
        .header("Content-Type", "application/json")
        .header("X-Xergon-Timestamp", timestamp.to_string())
        .header("X-Xergon-Public-Key", &wallet.public_key)
        .header("X-Xergon-Signature", signature);

    Ok((req, client))
}

/// Read prompt from argument or stdin (for pipe support).
fn read_prompt(arg: Option<String>) -> Result<String> {
    if let Some(p) = arg {
        return Ok(p);
    }

    // Check if stdin is a pipe (not a tty)
    let is_pipe = atty_check();

    if is_pipe {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .context("Failed to read from stdin")?;
        Ok(buf.trim().to_string())
    } else {
        // Interactive: prompt the user
        eprint!("  Enter your prompt: ");
        let mut buf = String::new();
        std::io::stdin()
            .read_line(&mut buf)
            .context("Failed to read prompt")?;
        Ok(buf.trim().to_string())
    }
}

/// Check if stdin is piped (not a tty).
fn atty_check() -> bool {
    // Use isatty on unix, or check console mode on windows
    #[cfg(unix)]
    {
        unsafe { libc::isatty(libc::STDIN_FILENO) == 0 }
    }
    #[cfg(windows)]
    {
        // Simplified: assume not a pipe if we can't determine
        false
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

/// Minimal percent-encoding for URL query parameters.
/// Encodes everything except unreserved chars (A-Z, a-z, 0-9, '-', '_', '.', '~').
fn url_encode(input: &str) -> String {
    let mut result = String::with_capacity(input.len() * 2);
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            b' ' => result.push_str("%20"),
            _ => {
                result.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Command implementations
// ---------------------------------------------------------------------------

async fn cmd_setup(relay_url: &str) -> Result<()> {
    println!();
    println!("  ╔══════════════════════════════════════════════╗");
    println!("  ║           XERGON WALLET SETUP                ║");
    println!("  ╚══════════════════════════════════════════════╝");
    println!();

    if wallet::wallet_exists() {
        println!("  A wallet already exists.");
        let confirm = rpassword::prompt_password("  Enter password to verify, or Ctrl+C to cancel: ")
            .context("Failed to read password")?;
        match wallet::load_wallet(&confirm) {
            Ok(w) => {
                println!("  Wallet verified. Public key: {}", w.public_key);
                println!("  Relay URL: {}", relay_url);
                println!();
                println!("  Use `xergon status` to check your connection.");
                return Ok(());
            }
            Err(_) => {
                anyhow::bail!("Wrong password. If you want to create a new wallet, remove ~/.xergon/wallet.json first.");
            }
        }
    }

    let wallet = wallet::generate_wallet_interactive()?;

    // Save relay URL to config
    let config_path = wallet::xergon_dir()?.join("config.json");
    let config = serde_json::json!({
        "relay_url": relay_url,
        "public_key": wallet.public_key,
    });
    let config_str = serde_json::to_string_pretty(&config)?;
    std::fs::write(&config_path, &config_str)
        .with_context(|| format!("Failed to write config to {}", config_path.display()))?;

    println!();
    println!("  Config saved to {}", config_path.display());
    println!("  Relay URL: {}", relay_url);

    // Attempt automatic airdrop
    println!();
    println!("  Requesting free ERG airdrop...");
    let airdrop_url = format!("{}/api/airdrop/request", relay_url.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let airdrop_body = serde_json::json!({
        "public_key": wallet.public_key,
    });
    match client.post(&airdrop_url).json(&airdrop_body).send().await {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<serde_json::Value>().await {
                Ok(body) => {
                    let amount_nanoerg = body.get("amount_nanoerg")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let amount_erg = amount_nanoerg as f64 / 1_000_000_000.0;
                    let tx_id = body.get("tx_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    println!("  Airdrop received! {} ERG deposited (tx: {}).", amount_erg, tx_id);
                }
                Err(_) => {
                    println!("  Airdrop received! Check your balance with `xergon balance`.");
                }
            }
        }
        Ok(resp) => {
            let status = resp.status();
            let warn_text = match resp.text().await {
                Ok(t) if !t.is_empty() => t.chars().take(120).collect::<String>(),
                _ => status.to_string(),
            };
            println!("  ⚠  Airdrop unavailable ({}).", warn_text);
            println!("  To fund your wallet manually, send ERG to your deposit address.");
            println!("  Run `xergon deposit` to see your address.");
        }
        Err(e) => {
            println!("  ⚠  Could not reach relay for airdrop: {}", e);
            println!("  To fund your wallet manually, send ERG to your deposit address.");
            println!("  Run `xergon deposit` to see your address.");
        }
    }

    println!();
    println!("  You're all set! Try these commands:");
    println!("    xergon status    — Check wallet and relay connection");
    println!("    xergon models    — List available AI models");
    println!("    xergon ask \"hello\" — Send your first prompt");
    println!("    xergon deposit   — Get your ERG deposit address");
    println!();

    Ok(())
}

async fn cmd_ask(
    relay_url: &str,
    prompt: Option<String>,
    model: Option<String>,
    stream: bool,
    system: Option<String>,
    json_mode: bool,
    provider: Option<String>,
) -> Result<()> {
    let wallet = require_wallet()?;
    let prompt_text = read_prompt(prompt)?;

    if prompt_text.is_empty() {
        anyhow::bail!("Empty prompt. Provide text as an argument or pipe it in.");
    }

    let model_name = model.unwrap_or_else(|| "auto".to_string());

    // Build messages array: system prompt (optional) + user prompt
    let mut messages: Vec<serde_json::Value> = Vec::new();

    if let Some(ref sys) = system {
        if !sys.is_empty() {
            messages.push(serde_json::json!({
                "role": "system",
                "content": sys
            }));
        }
    }

    messages.push(serde_json::json!({
        "role": "user",
        "content": prompt_text
    }));

    let mut body = serde_json::json!({
        "model": model_name,
        "messages": messages,
        "stream": stream,
    });

    // Add response_format for JSON mode
    if json_mode {
        body["response_format"] = serde_json::json!({ "type": "json_object" });
    }

    // Add provider routing if specified
    if let Some(ref prov) = provider {
        body["provider"] = serde_json::json!(prov);
    }

    let body_bytes = serde_json::to_vec(&body)?;
    let (req_builder, _client) = build_signed_request(
        &wallet,
        "POST",
        "/v1/chat/completions",
        relay_url,
        &body_bytes,
    )?;

    let resp = req_builder
        .body(body_bytes)
        .send()
        .await
        .context("Request to relay failed")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "Relay returned HTTP {}: {}",
            status,
            body_text
        );
    }

    if stream {
        // Parse SSE stream
        let mut stream = resp.bytes_stream();
        use futures_util::StreamExt;

        let mut total_tokens: u64 = 0;
        let mut completion_tokens: u64 = 0;
        let mut prompt_tokens: u64 = 0;
        let mut response_model: String = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Failed to read stream chunk")?;
            let text = String::from_utf8_lossy(&chunk);

            // Parse SSE lines
            for line in text.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    if data.trim() == "[DONE]" {
                        println!();
                        println!();
                        println!("  ─────────────────────────────────────────");
                        if !response_model.is_empty() {
                            println!("  Model: {}", response_model);
                        }
                        if prompt_tokens > 0 || completion_tokens > 0 || total_tokens > 0 {
                            println!(
                                "  Tokens: {} prompt, {} completion, {} total",
                                prompt_tokens, completion_tokens, total_tokens
                            );
                        }
                        return Ok(());
                    }
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                        // Track usage from last chunk if available
                        if let Some(usage) = json.get("usage").or_else(|| json.get("xergon_usage")) {
                            total_tokens = usage.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(total_tokens);
                            completion_tokens = usage.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(completion_tokens);
                            prompt_tokens = usage.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(prompt_tokens);
                        }
                        // Track model name
                        if let Some(m) = json.get("model").and_then(|v| v.as_str()) {
                            if response_model.is_empty() {
                                response_model = m.to_string();
                            }
                        }
                        // Extract delta content from choices[0].delta.content
                        if let Some(content) = json
                            .get("choices")
                            .and_then(|c| c.get(0))
                            .and_then(|c| c.get("delta"))
                            .and_then(|d| d.get("content"))
                            .and_then(|c| c.as_str())
                        {
                            print!("{}", content);
                            use std::io::Write;
                            std::io::stdout().flush()?;
                        }
                    }
                }
            }
        }
        println!();
        println!();
        println!("  ─────────────────────────────────────────");
        if !response_model.is_empty() {
            println!("  Model: {}", response_model);
        }
        if prompt_tokens > 0 || completion_tokens > 0 || total_tokens > 0 {
            println!(
                "  Tokens: {} prompt, {} completion, {} total",
                prompt_tokens, completion_tokens, total_tokens
            );
        }
    } else {
        // Non-streaming: print full response
        let body: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse response")?;

        // Extract the assistant's message content
        if let Some(content) = body
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
        {
            println!("{}", content);
        } else {
            // Fallback: print raw JSON
            println!("{}", serde_json::to_string_pretty(&body)?);
        }

        // Show token usage for non-streaming if available
        if let Some(usage) = body.get("usage") {
            let total = usage.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
            let prompt = usage.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
            let completion = usage.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
            if total > 0 {
                println!();
                println!("  ─────────────────────────────────────────");
                if let Some(model) = body.get("model").and_then(|v| v.as_str()) {
                    println!("  Model: {}", model);
                }
                println!(
                    "  Tokens: {} prompt, {} completion, {} total",
                    prompt, completion, total
                );
            }
        }
    }

    Ok(())
}

async fn cmd_models(relay_url: &str) -> Result<()> {
    let wallet = require_wallet()?;

    let (req_builder, _client) = build_signed_request(
        &wallet,
        "GET",
        "/v1/models",
        relay_url,
        b"",
    )?;

    let resp = req_builder
        .send()
        .await
        .context("Failed to fetch models from relay")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Relay returned HTTP {}: {}", status, body);
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .context("Failed to parse models response")?;

    let models = body
        .get("data")
        .and_then(|d| d.as_array())
        .cloned()
        .unwrap_or_default();

    println!();
    println!("  Available Models ({})", models.len());
    println!("  ─────────────────────────────────────────");

    if models.is_empty() {
        println!("  (no models available)");
    } else {
        for model in &models {
            let id = model.get("id").and_then(|v| v.as_str()).unwrap_or("?");
            let owner = model
                .get("owner")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            println!("  {:30}  owner: {}", id, owner);
        }
    }

    println!();
    println!("  Usage: xergon ask --model <model> \"your prompt\"");
    println!();

    Ok(())
}

async fn cmd_balance(relay_url: &str) -> Result<()> {
    let wallet = require_wallet()?;
    let path = format!("/v1/balance/{}", wallet.public_key);

    let (req_builder, _client) = build_signed_request(
        &wallet,
        "GET",
        &path,
        relay_url,
        b"",
    )?;

    let resp = req_builder
        .send()
        .await
        .context("Failed to fetch balance from relay")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Relay returned HTTP {}: {}", status, body);
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .context("Failed to parse balance response")?;

    let balance = body
        .get("balance_erg")
        .and_then(|v| v.as_str())
        .unwrap_or("0.000000000");

    let nanoerg = body
        .get("balance_nanoerg")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    println!();
    println!("  Balance: {} ERG", balance);
    println!("  Raw: {} nanoERG", nanoerg);
    println!();

    Ok(())
}

async fn cmd_deposit() -> Result<()> {
    let wallet = require_wallet()?;

    println!();
    println!("  ╔══════════════════════════════════════════════╗");
    println!("  ║           ERG DEPOSIT ADDRESS                ║");
    println!("  ╚══════════════════════════════════════════════╝");
    println!();
    println!("  Public Key (hex): {}", wallet.public_key);
    println!();
    println!("  To deposit ERG:");
    println!("    1. Open your Ergo wallet (e.g., Yoroi, Ergo Wallet App)");
    println!("    2. Send ERG to the address associated with this public key");
    println!("    3. The relay will credit your balance once confirmed");
    println!();
    println!("  Check balance: xergon balance");
    println!();

    Ok(())
}

fn cmd_token(expiry_secs: u64) -> Result<()> {
    let wallet = require_wallet()?;

    let (token, timestamp) =
        signing::generate_token(&wallet.secret_key, &wallet.public_key, expiry_secs);

    let created = chrono::DateTime::from_timestamp_millis(timestamp as i64)
        .unwrap_or_default();
    let expires = chrono::DateTime::from_timestamp_millis((timestamp + expiry_secs * 1000) as i64)
        .unwrap_or_default();

    println!();
    println!("  API Token generated (expires: {})", expires.format("%Y-%m-%d %H:%M:%S UTC"));
    println!();
    println!("  export OPENAI_API_KEY={}", token);
    println!("  export OPENAI_BASE_URL={}", std::env::var("XERGON_RELAY_URL").unwrap_or_else(|_| "https://relay.xergon.gg".to_string()));
    println!();
    println!("  Then use with any OpenAI SDK:");
    println!("    openai.chat.completions.create(model=\"qwen3.5-4b\", messages=[...])");
    println!();
    println!("  Created:  {}", created.format("%Y-%m-%d %H:%M:%S UTC"));
    println!("  Expires:  {}", expires.format("%Y-%m-%d %H:%M:%S UTC"));
    println!("  Public Key: {}", wallet.public_key);
    println!();

    Ok(())
}

#[allow(unused_variables)]
async fn cmd_status(relay_url: &str) -> Result<()> {
    let current_version = env!("CARGO_PKG_VERSION");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    // --- Try to load local agent config for provider-side info ---
    let agent_config = xergon_agent::config::AgentConfig::load_from(None).ok();
    let agent_api_url = agent_config
        .as_ref()
        .map(|c| format!("http://{}", c.api.listen_addr));
    let inference_url = agent_config
        .as_ref()
        .map(|c| c.inference.url.clone())
        .unwrap_or_else(|| "http://127.0.0.1:11434".to_string());
    let ergo_node_url = agent_config
        .as_ref()
        .map(|c| c.ergo_node.rest_url.clone())
        .unwrap_or_else(|| "http://127.0.0.1:9053".to_string());
    let relay_config_url = agent_config
        .as_ref()
        .map(|c| c.relay.relay_url.clone())
        .unwrap_or_else(|| relay_url.to_string());

    // --- Fetch agent health (uptime, provider_id) ---
    let mut agent_uptime_secs: u64 = 0;
    let mut agent_provider_id: String = String::new();
    if let Some(ref api) = agent_api_url {
        if let Ok(resp) = client
            .get(format!("{}/xergon/health", api))
            .send()
            .await
        {
            if resp.status().is_success() {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    agent_uptime_secs = body
                        .get("uptime_secs")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    agent_provider_id = body
                        .get("provider_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let _ = body; // consumed, fields extracted above
                }
            }
        }
    }

    // --- Fetch agent dashboard for node + PoNW + settlement ---
    let mut dashboard: Option<serde_json::Value> = None;
    if let Some(ref api) = agent_api_url {
        if let Ok(resp) = client
            .get(format!("{}/xergon/dashboard", api))
            .send()
            .await
        {
            if resp.status().is_success() {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    dashboard = Some(body);
                }
            }
        }
    }

    // --- Fetch inference models from local Ollama/llama-server ---
    let mut local_models: Vec<String> = Vec::new();
    if let Ok(resp) = client.get(format!("{}/v1/models", inference_url)).send().await {
        if resp.status().is_success() {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
                    for m in data {
                        if let Some(id) = m.get("id").and_then(|v| v.as_str()) {
                            local_models.push(id.to_string());
                        }
                    }
                }
            }
        }
    }

    // --- Fetch Ergo node info ---
    let mut node_synced = false;
    let mut node_height: u64 = 0;
    let mut node_peers: usize = 0;
    if let Ok(resp) = client
        .get(format!("{}/info", ergo_node_url))
        .send()
        .await
    {
        if resp.status().is_success() {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                node_synced = body
                    .get("isSynchronized")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                node_height = body
                    .get("fullHeight")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                node_peers = body
                    .get("peersCount")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;
            }
        }
    }

    // --- Fetch wallet / relay balance ---
    let mut wallet_balance = String::from("N/A");
    let mut wallet_pk: Option<String> = None;
    if wallet::wallet_exists() {
        match wallet::load_wallet_interactive() {
            Ok(w) => {
                wallet_pk = Some(w.public_key.clone());
                let path = format!("/v1/balance/{}", w.public_key);
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                let signature = sign_request(&w.secret_key, timestamp, "GET", &path, b"");
                match client
                    .get(format!("{}{}", relay_url.trim_end_matches('/'), path))
                    .header("Content-Type", "application/json")
                    .header("X-Xergon-Timestamp", timestamp.to_string())
                    .header("X-Xergon-Public-Key", &w.public_key)
                    .header("X-Xergon-Signature", signature)
                    .send()
                    .await
                {
                    Ok(resp) if resp.status().is_success() => {
                        if let Ok(body) = resp.json::<serde_json::Value>().await {
                            wallet_balance = body
                                .get("balance_erg")
                                .and_then(|v| v.as_str())
                                .unwrap_or("0")
                                .to_string();
                        }
                    }
                    _ => {}
                }
            }
            Err(_) => {}
        }
    }

    // --- Fetch relay info / PoNW score ---
    let mut pown_total: u64 = 0;
    let mut pown_node: u64 = 0;
    let mut pown_network: u64 = 0;
    let mut pown_ai: u64 = 0;
    if let Some(ref pk) = wallet_pk {
        let path = format!("/v1/leaderboard/self");
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let sk = wallet::load_wallet_interactive()
            .ok()
            .map(|w| w.secret_key);
        if let Some(ref secret_key) = sk {
            let signature = sign_request(secret_key, timestamp, "GET", &path, b"");
            if let Ok(resp) = client
                .get(format!("{}{}", relay_url.trim_end_matches('/'), path))
                .header("Content-Type", "application/json")
                .header("X-Xergon-Timestamp", timestamp.to_string())
                .header("X-Xergon-Public-Key", pk)
                .header("X-Xergon-Signature", signature)
                .send()
                .await
            {
                if resp.status().is_success() {
                    if let Ok(body) = resp.json::<serde_json::Value>().await {
                        pown_total = body.get("total_score").and_then(|v| v.as_u64()).unwrap_or(0);
                        pown_node = body.get("node_score").and_then(|v| v.as_u64()).unwrap_or(0);
                        pown_network = body.get("network_score").and_then(|v| v.as_u64()).unwrap_or(0);
                        pown_ai = body.get("ai_score").and_then(|v| v.as_u64()).unwrap_or(0);
                    }
                }
            }
        }
    }

    // --- Fetch GPU rental info from relay ---
    let mut _gpu_listed: usize = 0;
    let mut gpu_active_rentals: usize = 0;
    if let Some(ref pk) = wallet_pk {
        let path = format!("/v1/gpu/rentals/{}", pk);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let sk = wallet::load_wallet_interactive()
            .ok()
            .map(|w| w.secret_key);
        if let Some(ref secret_key) = sk {
            let signature = sign_request(secret_key, timestamp, "GET", &path, b"");
            if let Ok(resp) = client
                .get(format!("{}{}", relay_url.trim_end_matches('/'), path))
                .header("Content-Type", "application/json")
                .header("X-Xergon-Timestamp", timestamp.to_string())
                .header("X-Xergon-Public-Key", pk)
                .header("X-Xergon-Signature", signature)
                .send()
                .await
            {
                if resp.status().is_success() {
                    if let Ok(body) = resp.json::<serde_json::Value>().await {
                        gpu_active_rentals = body
                            .get("rentals")
                            .and_then(|r| r.as_array())
                            .map(|a| a.len())
                            .unwrap_or(0);
                    }
                }
            }
        }
    }

    // --- Fetch settlement info from local agent ---
    let mut total_earned: f64 = 0.0;
    let mut pending_proofs: usize = 0;
    if let Some(ref api) = agent_api_url {
        if let Ok(resp) = client
            .get(format!("{}/xergon/settlement", api))
            .send()
            .await
        {
            if resp.status().is_success() {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    if let Some(summary) = body.get("summary") {
                        total_earned = summary
                            .get("total_settled_erg")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0);
                    }
                    pending_proofs = body
                        .get("pending_providers")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize;
                }
            }
        }
    }

    // --- Fetch peer info from local agent ---
    let mut known_peers: usize = 0;
    if let Some(ref api) = agent_api_url {
        if let Ok(resp) = client
            .get(format!("{}/xergon/peers", api))
            .send()
            .await
        {
            if resp.status().is_success() {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    known_peers = body
                        .get("unique_xergon_peers_seen")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize;
                }
            }
        }
    }

    // =======================================================================
    // Format output
    // =======================================================================

    // Uptime formatting
    let uptime_str = if agent_uptime_secs > 0 {
        let hours = agent_uptime_secs / 3600;
        let mins = (agent_uptime_secs % 3600) / 60;
        let secs = agent_uptime_secs % 60;
        if hours > 0 {
            format!("{}h {}m {}s", hours, mins, secs)
        } else if mins > 0 {
            format!("{}m {}s", mins, secs)
        } else {
            format!("{}s", secs)
        }
    } else {
        "N/A".to_string()
    };

    // Height formatting with commas
    fn fmt_num(n: u64) -> String {
        let s = n.to_string();
        let mut result = String::new();
        for (i, c) in s.chars().enumerate() {
            if i > 0 && (s.len() - i) % 3 == 0 {
                result.push(',');
            }
            result.push(c);
        }
        result
    }

    // Address truncation
    let ergo_addr = agent_config
        .as_ref()
        .map(|c| c.xergon.ergo_address.clone())
        .unwrap_or_default();
    let ergo_addr_display = if ergo_addr.len() > 8 {
        format!("{}...", &ergo_addr[..8])
    } else if ergo_addr.is_empty() {
        "N/A".to_string()
    } else {
        ergo_addr.clone()
    };

    // Endpoint display
    let endpoint_display = agent_api_url
        .as_ref()
        .map(|s| s.as_str())
        .unwrap_or("N/A");

    // GPU info from dashboard
    let gpu_info_str = if let Some(ref dash) = dashboard {
        if let Some(hw) = dash.get("hardware") {
            let devices = hw
                .get("devices")
                .and_then(|d| d.as_array())
                .cloned()
                .unwrap_or_default();
            let gpu_names: Vec<String> = devices
                .iter()
                .filter_map(|d| {
                    let name = d.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                    let vram = d.get("vram_mb").and_then(|v| v.as_u64()).unwrap_or(0);
                    Some(format!("{} ({}GB)", name, vram / 1024))
                })
                .collect();
            if gpu_names.is_empty() {
                "0".to_string()
            } else {
                format!("{} ({})", devices.len(), gpu_names.join(", "))
            }
        } else {
            "N/A".to_string()
        }
    } else {
        "N/A".to_string()
    };

    // AI points from dashboard
    let ai_points_str = if let Some(ref dash) = dashboard {
        if let Some(ap) = dash.get("ai_points") {
            let total = ap.get("total_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
            let points = ap.get("ai_points").and_then(|v| v.as_u64()).unwrap_or(0);
            format!("{} tokens / {} pts", fmt_num(total), fmt_num(points))
        } else {
            "N/A".to_string()
        }
    } else {
        "N/A".to_string()
    };

    // Provider score from dashboard
    let provider_score_str = if let Some(ref dash) = dashboard {
        if let Some(ps) = dash.get("provider_score") {
            let score = ps
                .get("weighted_composite_score")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            format!("{:.1}", score)
        } else {
            "N/A".to_string()
        }
    } else {
        "N/A".to_string()
    };

    // Settled count from dashboard
    let settled_count = if let Some(ref dash) = dashboard {
        dash.get("settlements")
            .and_then(|s| s.as_array())
            .map(|a| a.len())
            .unwrap_or(0)
    } else {
        0
    };

    // Now print the dashboard
    println!();
    println!("  === Xergon Agent Status ===");
    println!("  Version:     {}", current_version);
    println!("  Uptime:      {}", uptime_str);
    if !agent_provider_id.is_empty() {
        println!("  Provider:    {} ({})", agent_provider_id,
            agent_config.as_ref().map(|c| c.xergon.region.clone()).unwrap_or_default());
    }
    println!();

    // Node section
    println!("  Node:");
    println!("    Status:    {}", if node_synced { "Synced" } else { "Not synced / unreachable" });
    println!("    Height:    {}", if node_height > 0 { fmt_num(node_height) } else { "N/A".to_string() });
    println!("    Peers:     {}", if node_peers > 0 { node_peers.to_string() } else { "N/A".to_string() });
    println!("    ERG Node:  {}", ergo_node_url);
    println!();

    // Agent section
    println!("  Agent:");
    println!("    PoNW Score: {} / 1000", pown_total);
    println!("      Node:    {} ({}%)", pown_node, if pown_total > 0 { pown_node * 100 / pown_total } else { 0 });
    println!("      Network: {} ({}%)", pown_network, if pown_total > 0 { pown_network * 100 / pown_total } else { 0 });
    println!("      AI:      {} ({}%)", pown_ai, if pown_total > 0 { pown_ai * 100 / pown_total } else { 0 });
    println!("    Provider Score: {}", provider_score_str);
    println!("    AI Stats:   {}", ai_points_str);
    println!("    Models:     {}", if local_models.is_empty() { "N/A".to_string() } else { local_models.join(", ") });
    println!("    Inference:  Ollama at {}", inference_url);
    println!("    Endpoint:   {}", endpoint_display);
    println!();

    // Wallet section
    println!("  Wallet:");
    if wallet_pk.is_some() {
        let pk_short = wallet_pk.as_ref().map(|p| {
            if p.len() > 12 { format!("{}...", &p[..12]) } else { p.clone() }
        }).unwrap_or_default();
        println!("    Public Key: {}", pk_short);
        println!("    ERG Balance: {}", wallet_balance);
        println!("    Address:     {}", ergo_addr_display);
    } else {
        println!("    (not configured — run `xergon setup`)");
    }
    println!();

    // GPU Rental section
    println!("  GPU Rental:");
    println!("    GPUs:       {}", gpu_info_str);
    println!("    Active Rentals: {}", gpu_active_rentals);
    println!();

    // Settlement section
    println!("  Settlement:");
    println!("    Total Earned:   {:.4} ERG", total_earned);
    println!("    Settled Batches: {}", settled_count);
    println!("    Pending Proofs: {}", pending_proofs);
    println!();

    // Network section
    println!("  Network:");
    println!("    Known Peers:    {} Xergon agents", known_peers);
    println!("    Relay:          {}", relay_config_url);
    println!();

    println!("  Use `xergon update` to check for updates.");
    println!();

    Ok(())
}

// ---------------------------------------------------------------------------
// Update command implementation
// ---------------------------------------------------------------------------

/// Detect the current OS for the binary download filename.
fn detect_os() -> &'static str {
    if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unknown"
    }
}

/// Detect the current architecture for the binary download filename.
fn detect_arch() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unknown"
    }
}

async fn cmd_update() -> Result<()> {
    let current_version_str = env!("CARGO_PKG_VERSION");
    let current_version: semver::Version = current_version_str
        .parse()
        .context("Failed to parse current version")?;

    // Load update config if available
    let release_url = xergon_agent::config::AgentConfig::load_from(None)
        .ok()
        .map(|c| c.update.release_url)
        .unwrap_or_else(|| {
            "https://api.github.com/repos/n1ur0/Xergon-Network/releases/latest".to_string()
        });

    println!();
    println!("  Current version: v{}", current_version);
    println!("  Checking for updates...");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("xergon-cli")
        .build()?;

    // Query GitHub Releases API
    let resp = client
        .get(&release_url)
        .send()
        .await
        .context("Failed to reach GitHub Releases API")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "GitHub API returned HTTP {}: {}",
            status,
            if body.len() > 120 {
                format!("{}...", &body[..120])
            } else {
                body
            }
        );
    }

    let release: serde_json::Value = resp
        .json()
        .await
        .context("Failed to parse GitHub release response")?;

    let latest_tag = release
        .get("tag_name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    // Strip leading 'v' if present
    let latest_version_str = latest_tag.strip_prefix('v').unwrap_or(latest_tag);
    let latest_version: semver::Version = latest_version_str
        .parse()
        .context(format!("Failed to parse latest version: {}", latest_version_str))?;

    println!("  Latest version:  v{}", latest_version);
    println!();

    if latest_version <= current_version {
        println!("  Already on latest version (v{}).", current_version);
        println!();
        return Ok(());
    }

    // Find the appropriate binary asset
    let os = detect_os();
    let arch = detect_arch();
    let asset_name = format!("xergon-{}-{}", os, arch);

    let assets = release
        .get("assets")
        .and_then(|a| a.as_array())
        .cloned()
        .unwrap_or_default();

    let asset = assets
        .iter()
        .find(|a| {
            a.get("name")
                .and_then(|n| n.as_str())
                .map(|n| n.starts_with(&asset_name))
                .unwrap_or(false)
        });

    let download_url = match asset {
        Some(a) => a
            .get("browser_download_url")
            .and_then(|u| u.as_str())
            .unwrap_or("")
            .to_string(),
        None => {
            println!("  Update available: v{} -> v{}", current_version, latest_version);
            println!("  No prebuilt binary found for {}-{}.", os, arch);
            println!("  Download manually from: {}", release_url);
            println!();
            return Ok(());
        }
    };

    let asset_filename = asset
        .and_then(|a| a.get("name").and_then(|n| n.as_str()).map(String::from))
        .unwrap_or_else(|| asset_name.clone());

    println!("  Downloading {}...", asset_filename);

    // Download the binary
    let download_resp = client.get(&download_url).send().await.context(
        format!("Failed to download binary from {}", download_url),
    )?;

    if !download_resp.status().is_success() {
        anyhow::bail!(
            "Download failed with HTTP {}",
            download_resp.status()
        );
    }

    let bytes = download_resp.bytes().await.context("Failed to read download bytes")?;

    // Get current executable path
    let current_exe = std::env::current_exe().context("Cannot determine current executable path")?;

    // Write to temp file in same directory (required for atomic rename on same filesystem)
    let tmp_path = current_exe.with_extension("tmp-update");

    // Make temp file executable
    {
        let mut file = std::fs::File::create(&tmp_path)
            .with_context(|| format!("Failed to create temp file: {}", tmp_path.display()))?;
        use std::io::Write;
        file.write_all(&bytes)
            .context("Failed to write downloaded binary")?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&tmp_path, perms)
            .context("Failed to set executable permissions")?;
    }

    // Atomic rename
    std::fs::rename(&tmp_path, &current_exe)
        .context(format!(
            "Failed to replace binary. You may need to run with appropriate permissions.\nTemp file saved at: {}",
            tmp_path.display()
        ))?;

    println!("  Updated to v{}.", latest_version);
    println!("  Restart to apply the update.");
    println!();

    Ok(())
}

// ---------------------------------------------------------------------------
// GPU rental command implementations
// ---------------------------------------------------------------------------

async fn cmd_gpu_list(
    relay_url: &str,
    region: Option<String>,
    min_vram: Option<u32>,
    max_price: Option<f64>,
    gpu_type: Option<String>,
) -> Result<()> {
    let wallet = require_wallet()?;

    // Build query parameters as a vec of tuples for reqwest
    let mut query_params: Vec<(&str, String)> = Vec::new();
    if let Some(r) = &region {
        query_params.push(("region", r.clone()));
    }
    if let Some(v) = min_vram {
        query_params.push(("min_vram", v.to_string()));
    }
    if let Some(p) = max_price {
        query_params.push(("max_price_per_hour", p.to_string()));
    }
    if let Some(g) = &gpu_type {
        query_params.push(("gpu_type", g.clone()));
    }

    // Build the URL with query params
    let base_url = format!("{}{}", relay_url.trim_end_matches('/'), "/v1/gpu/listings");
    let url = if query_params.is_empty() {
        base_url
    } else {
        let encoded: Vec<String> = query_params
            .iter()
            .map(|(k, v)| format!("{}={}", k, url_encode(v)))
            .collect();
        format!("{}?{}", base_url, encoded.join("&"))
    };

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let signature = sign_request(&wallet.secret_key, timestamp, "GET", "/v1/gpu/listings", b"");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    let resp = client
        .get(&url)
        .header("Content-Type", "application/json")
        .header("X-Xergon-Timestamp", timestamp.to_string())
        .header("X-Xergon-Public-Key", &wallet.public_key)
        .header("X-Xergon-Signature", signature)
        .send()
        .await
        .context("Failed to fetch GPU listings from relay")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Relay returned HTTP {}: {}", status, body);
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .context("Failed to parse GPU listings response")?;

    let listings = body
        .get("listings")
        .and_then(|d| d.as_array())
        .cloned()
        .unwrap_or_default();

    println!();
    println!("  Available GPUs ({})", listings.len());
    println!("  ──────────────────────────────────────────────────────────────────────────────────────────");
    println!("  {:<12} {:<16} {:>6} {:>10} {:<10} {:<10}", "ID", "GPU Type", "VRAM", "Price/hr", "Region", "Status");
    println!("  ──────────────────────────────────────────────────────────────────────────────────────────");

    if listings.is_empty() {
        println!("  (no GPUs available matching filters)");
    } else {
        for listing in &listings {
            let id = listing.get("listing_id").and_then(|v| v.as_str()).unwrap_or("?");
            let gpu = listing.get("gpu_type").and_then(|v| v.as_str()).unwrap_or("?");
            let vram = listing.get("vram_gb").and_then(|v| v.as_u64()).unwrap_or(0);
            let price = listing.get("price_per_hour").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let region = listing.get("region").and_then(|v| v.as_str()).unwrap_or("?");
            let available = listing.get("available").and_then(|v| v.as_bool()).unwrap_or(false);
            let status = if available { "available" } else { "rented" };

            // Truncate long IDs and GPU types for display
            let display_id = if id.len() > 12 { &id[..12] } else { id };
            let display_gpu = if gpu.len() > 16 { &gpu[..14] } else { gpu };

            println!("  {:<12} {:<16} {:>6} {:>10.4} {:<10} {:<10}", display_id, display_gpu, vram, price, region, status);
        }
    }

    println!();
    println!("  Usage: xergon gpu rent <listing_id> <hours>");
    println!();

    Ok(())
}

async fn cmd_gpu_pricing(relay_url: &str) -> Result<()> {
    let wallet = require_wallet()?;

    let (req_builder, _client) = build_signed_request(
        &wallet,
        "GET",
        "/v1/gpu/pricing",
        relay_url,
        b"",
    )?;

    let resp = req_builder
        .send()
        .await
        .context("Failed to fetch GPU pricing from relay")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Relay returned HTTP {}: {}", status, body);
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .context("Failed to parse GPU pricing response")?;

    let pricing = body
        .get("pricing")
        .and_then(|d| d.as_array())
        .cloned()
        .unwrap_or_else(|| {
            // Try alternative response format: direct array or object with entries
            if body.is_array() {
                body.as_array().cloned().unwrap_or_default()
            } else {
                vec![]
            }
        });

    println!();
    println!("  GPU Pricing");
    println!("  ──────────────────────────────────────────────────────────────");
    println!("  {:<20} {:>10} {:>10} {:>10} {:>10}", "GPU Type", "Avg Price", "Min", "Max", "Listings");
    println!("  ──────────────────────────────────────────────────────────────");

    if pricing.is_empty() {
        println!("  (no pricing data available)");
    } else {
        for item in &pricing {
            let gpu = item.get("gpu_type").and_then(|v| v.as_str()).unwrap_or("?");
            let avg = item.get("avg_price").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let min = item.get("min_price").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let max = item.get("max_price").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let count = item.get("listing_count").and_then(|v| v.as_u64()).unwrap_or(0);

            println!("  {:<20} {:>10.4} {:>10.4} {:>10.4} {:>10}", gpu, avg, min, max, count);
        }
    }

    println!();
    println!("  Prices shown in ERG per hour.");
    println!();

    Ok(())
}

async fn cmd_gpu_rent(relay_url: &str, listing_id: &str, hours: u32) -> Result<()> {
    let wallet = require_wallet()?;

    let body = serde_json::json!({
        "listing_id": listing_id,
        "hours": hours,
        "renter_public_key": wallet.public_key,
    });

    let body_bytes = serde_json::to_vec(&body)?;
    let (req_builder, _client) = build_signed_request(
        &wallet,
        "POST",
        "/v1/gpu/rent",
        relay_url,
        &body_bytes,
    )?;

    let resp = req_builder
        .body(body_bytes)
        .send()
        .await
        .context("Failed to send GPU rent request to relay")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Relay returned HTTP {}: {}", status, body_text);
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .context("Failed to parse GPU rent response")?;

    let rental_id = body
        .get("rental_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let deadline = body
        .get("deadline_block")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cost = match body.get("total_cost_erg") {
        Some(v) if v.is_string() => v.as_str().unwrap_or("?").to_string(),
        Some(v) if v.is_number() => format!("{:.4}", v.as_f64().unwrap_or(0.0)),
        _ => "?".to_string(),
    };

    println!();
    println!("  GPU rented successfully!");
    println!();
    println!("  Listing ID:  {}", listing_id);
    println!("  Rental ID:   {}", rental_id);
    println!("  Hours:       {}", hours);
    println!("  Cost:        {} ERG", cost);
    println!("  Deadline:    block {}", deadline);
    println!();
    println!("  View your rentals: xergon gpu my-rentals");
    println!();

    Ok(())
}

async fn cmd_gpu_my_rentals(relay_url: &str) -> Result<()> {
    let wallet = require_wallet()?;

    let path = format!("/v1/gpu/rentals/{}", wallet.public_key);

    let (req_builder, _client) = build_signed_request(
        &wallet,
        "GET",
        &path,
        relay_url,
        b"",
    )?;

    let resp = req_builder
        .send()
        .await
        .context("Failed to fetch your GPU rentals from relay")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Relay returned HTTP {}: {}", status, body);
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .context("Failed to parse GPU rentals response")?;

    let rentals = body
        .get("rentals")
        .and_then(|d| d.as_array())
        .cloned()
        .unwrap_or_else(|| {
            if body.is_array() {
                body.as_array().cloned().unwrap_or_default()
            } else {
                vec![]
            }
        });

    println!();
    println!("  Your GPU Rentals ({})", rentals.len());
    println!("  ───────────────────────────────────────────────────────────────────────────────────────");
    println!("  {:<14} {:<16} {:<12} {:>10} {:>8} {:<12}", "Rental ID", "GPU Type", "Region", "Price/hr", "Hours", "Status");
    println!("  ───────────────────────────────────────────────────────────────────────────────────────");

    if rentals.is_empty() {
        println!("  (no active rentals)");
    } else {
        for rental in &rentals {
            let id = rental.get("rental_id").and_then(|v| v.as_str()).unwrap_or("?");
            let gpu = rental.get("gpu_type").and_then(|v| v.as_str()).unwrap_or("?");
            let region = rental.get("region").and_then(|v| v.as_str()).unwrap_or("?");
            let price = rental.get("price_per_hour").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let hours = rental.get("hours").and_then(|v| v.as_u64()).unwrap_or(0);
            let status = rental.get("status").and_then(|v| v.as_str()).unwrap_or("active");

            let display_id = if id.len() > 14 { &id[..12] } else { id };
            let display_gpu = if gpu.len() > 16 { &gpu[..14] } else { gpu };

            println!("  {:<14} {:<16} {:<12} {:>10.4} {:>8} {:<12}", display_id, display_gpu, region, price, hours, status);
        }
    }

    println!();
    println!("  Extend:  xergon gpu extend <rental_id> <hours>");
    println!("  Refund:  xergon gpu refund <rental_id>");
    println!();

    Ok(())
}

async fn cmd_gpu_refund(relay_url: &str, rental_id: &str) -> Result<()> {
    let wallet = require_wallet()?;

    let body = serde_json::json!({
        "rental_id": rental_id,
        "renter_public_key": wallet.public_key,
    });

    let body_bytes = serde_json::to_vec(&body)?;
    let (req_builder, _client) = build_signed_request(
        &wallet,
        "POST",
        "/v1/gpu/refund",
        relay_url,
        &body_bytes,
    )?;

    let resp = req_builder
        .body(body_bytes)
        .send()
        .await
        .context("Failed to send GPU refund request to relay")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Relay returned HTTP {}: {}", status, body_text);
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .context("Failed to parse GPU refund response")?;

    let refund_amount = match body.get("refund_amount_erg") {
        Some(v) if v.is_string() => v.as_str().unwrap_or("?").to_string(),
        Some(v) if v.is_number() => format!("{:.4}", v.as_f64().unwrap_or(0.0)),
        _ => "?".to_string(),
    };
    let tx_id = body
        .get("tx_id")
        .and_then(|v| v.as_str())
        .unwrap_or("pending");

    println!();
    println!("  Refund initiated!");
    println!();
    println!("  Rental ID:     {}", rental_id);
    println!("  Refund amount: {} ERG", refund_amount);
    println!("  Transaction:   {}", tx_id);
    println!();

    Ok(())
}

async fn cmd_gpu_extend(relay_url: &str, rental_id: &str, additional_hours: u32) -> Result<()> {
    let wallet = require_wallet()?;

    let body = serde_json::json!({
        "rental_id": rental_id,
        "additional_hours": additional_hours,
        "renter_public_key": wallet.public_key,
    });

    let body_bytes = serde_json::to_vec(&body)?;
    let (req_builder, _client) = build_signed_request(
        &wallet,
        "POST",
        "/v1/gpu/extend",
        relay_url,
        &body_bytes,
    )?;

    let resp = req_builder
        .body(body_bytes)
        .send()
        .await
        .context("Failed to send GPU extend request to relay")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Relay returned HTTP {}: {}", status, body_text);
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .context("Failed to parse GPU extend response")?;

    let new_deadline = body
        .get("new_deadline_block")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let total_hours = body
        .get("total_hours")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let additional_cost = match body.get("additional_cost_erg") {
        Some(v) if v.is_string() => v.as_str().unwrap_or("?").to_string(),
        Some(v) if v.is_number() => format!("{:.4}", v.as_f64().unwrap_or(0.0)),
        _ => "?".to_string(),
    };

    println!();
    println!("  Rental extended!");
    println!();
    println!("  Rental ID:      {}", rental_id);
    println!("  Added hours:    {}", additional_hours);
    println!("  Total hours:    {}", total_hours);
    println!("  Additional cost: {} ERG", additional_cost);
    println!("  New deadline:   block {}", new_deadline);
    println!();

    Ok(())
}

async fn cmd_gpu_rate(
    relay_url: &str,
    rental_id: &str,
    rating: u8,
    role: &str,
    comment: Option<&str>,
) -> Result<()> {
    if !(1..=5).contains(&rating) {
        anyhow::bail!("Rating must be between 1 and 5 stars.");
    }
    if role != "provider" && role != "renter" {
        anyhow::bail!("Role must be \"provider\" or \"renter\".");
    }

    let wallet = require_wallet()?;

    let body = serde_json::json!({
        "rental_id": rental_id,
        "rating": rating,
        "role": role,
        "comment": comment.unwrap_or(""),
        "rater_public_key": wallet.public_key,
    });

    let body_bytes = serde_json::to_vec(&body)?;
    let (req_builder, _client) = build_signed_request(
        &wallet,
        "POST",
        "/v1/gpu/rate",
        relay_url,
        &body_bytes,
    )?;

    let resp = req_builder
        .body(body_bytes)
        .send()
        .await
        .context("Failed to send GPU rating request to relay")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Relay returned HTTP {}: {}", status, body_text);
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .context("Failed to parse GPU rating response")?;

    println!();
    println!("  Rating submitted!");
    println!();
    println!("  Rental ID:  {}", rental_id);
    println!("  Rated:      {} ({}/5 stars)", role, rating);
    if let Some(c) = comment {
        println!("  Comment:    {}", c);
    }
    let rating_id = body
        .get("rating_id")
        .and_then(|v| v.as_str())
        .unwrap_or("recorded");
    println!("  Status:     {}", rating_id);
    println!();

    Ok(())
}

async fn cmd_gpu_reputation(relay_url: &str, public_key: &str) -> Result<()> {
    let wallet = require_wallet()?;

    let path = format!("/v1/gpu/reputation/{}", public_key);
    let (req_builder, _client) = build_signed_request(
        &wallet,
        "GET",
        &path,
        relay_url,
        b"",
    )?;

    let resp = req_builder
        .send()
        .await
        .context("Failed to fetch GPU reputation from relay")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Relay returned HTTP {}: {}", status, body_text);
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .context("Failed to parse GPU reputation response")?;

    let avg_stars = body
        .get("average_rating")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let total_ratings = body
        .get("total_ratings")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    println!();
    println!("  Reputation for {}", public_key);
    println!();
    println!("  Average:       {:.1} / 5.0 stars", avg_stars);
    println!("  Total ratings: {}", total_ratings);

    // Print breakdown by star count if available
    if let Some(breakdown) = body.get("breakdown").and_then(|v| v.as_object()) {
        println!("  Breakdown:");
        for stars in (1..=5u64).rev() {
            let count = breakdown
                .get(&stars.to_string())
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let bar_len = count as usize;
            let bar: String = "*".repeat(bar_len.min(40));
            println!("    {} stars: {:>4}  {}", stars, count, bar);
        }
    }

    println!();

    Ok(())
}

// ---------------------------------------------------------------------------
// Bridge command implementations
// ---------------------------------------------------------------------------

async fn cmd_bridge_status(relay_url: &str) -> Result<()> {
    let wallet = require_wallet()?;

    let (req_builder, _client) = build_signed_request(
        &wallet,
        "GET",
        "/v1/bridge/status",
        relay_url,
        b"",
    )?;

    let resp = req_builder
        .send()
        .await
        .context("Failed to fetch bridge status from relay")?;

    let body: serde_json::Value = resp
        .json()
        .await
        .context("Failed to parse bridge status response")?;

    println!();
    println!("  Cross-Chain Payment Bridge");
    println!("  ─────────────────────────────────────────");
    let enabled = body.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
    println!("  Status:       {}", if enabled { "enabled" } else { "disabled" });
    if let Some(chains) = body.get("supported_chains").and_then(|v| v.as_array()) {
        let chain_str: Vec<String> = chains.iter().filter_map(|c| c.as_str().map(String::from)).collect();
        println!("  Chains:       {}", chain_str.join(", "));
    }
    if let Some(timeout) = body.get("invoice_timeout_blocks").and_then(|v| v.as_u64()) {
        println!("  Timeout:      {} blocks (~{} hours)", timeout, timeout * 2 / 60);
    }
    if let Some(msg) = body.get("message").and_then(|v| v.as_str()) {
        println!("  {}", msg);
    }
    println!();
    println!("  Usage:");
    println!("    xergon bridge invoice-create <provider_pk> <amount_erg> <chain>");
    println!("    xergon bridge invoice-status <invoice_id>");
    println!("    xergon bridge invoice-confirm <invoice_id> <foreign_tx_id>");
    println!("    xergon bridge invoice-refund <invoice_id>");
    println!();

    Ok(())
}

async fn cmd_bridge_invoice_create(
    relay_url: &str,
    provider_pk: &str,
    amount_erg: f64,
    foreign_chain: &str,
) -> Result<()> {
    let wallet = require_wallet()?;

    if amount_erg <= 0.0 {
        anyhow::bail!("Amount must be greater than 0 ERG");
    }

    let body = serde_json::json!({
        "provider_pk": provider_pk,
        "amount_erg": amount_erg,
        "foreign_chain": foreign_chain,
    });

    let body_bytes = serde_json::to_vec(&body)?;
    let (req_builder, _client) = build_signed_request(
        &wallet,
        "POST",
        "/v1/bridge/create-invoice",
        relay_url,
        &body_bytes,
    )?;

    let resp = req_builder
        .body(body_bytes)
        .send()
        .await
        .context("Failed to create bridge invoice via relay")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Relay returned HTTP {}: {}", status, body_text);
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .context("Failed to parse bridge invoice response")?;

    let invoice_id = body.get("invoice_id").and_then(|v| v.as_str()).unwrap_or("?");
    let ergo_tx_id = body.get("ergo_tx_id").and_then(|v| v.as_str()).unwrap_or("?");
    let foreign_addr = body.get("foreign_payment_address").and_then(|v| v.as_str()).unwrap_or("?");
    let chain = body.get("foreign_chain").and_then(|v| v.as_str()).unwrap_or("?");
    let timeout = body.get("timeout_blocks").and_then(|v| v.as_u64()).unwrap_or(720);
    let msg = body.get("message").and_then(|v| v.as_str()).unwrap_or("");

    println!();
    println!("  Bridge Invoice Created");
    println!("  ─────────────────────────────────────────");
    println!("  Invoice ID:       {}", invoice_id);
    println!("  Ergo Tx:          {}", ergo_tx_id);
    println!("  Amount:           {} ERG", amount_erg);
    println!("  Foreign Chain:    {}", chain);
    println!("  Payment Address:  {}", foreign_addr);
    println!("  Timeout:          {} blocks (~{} hours)", timeout, timeout * 2 / 60);
    println!();
    println!("  Next steps:");
    println!("    1. Send {} worth of {} to: {}", amount_erg, chain, foreign_addr);
    println!("    2. Check status: xergon bridge invoice-status {}", invoice_id);
    println!();
    if !msg.is_empty() {
        println!("  {}", msg);
        println!();
    }

    Ok(())
}

async fn cmd_bridge_invoice_status(relay_url: &str, invoice_id: &str) -> Result<()> {
    let wallet = require_wallet()?;

    let path = format!("/v1/bridge/invoice/{}", url_encode(invoice_id));
    let (req_builder, _client) = build_signed_request(
        &wallet,
        "GET",
        &path,
        relay_url,
        b"",
    )?;

    let resp = req_builder
        .send()
        .await
        .context("Failed to fetch invoice status from relay")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Relay returned HTTP {}: {}", status, body_text);
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .context("Failed to parse invoice status response")?;

    let id = body.get("invoice_id").and_then(|v| v.as_str()).unwrap_or(invoice_id);
    let status = body.get("status").and_then(|v| v.as_str()).unwrap_or("unknown");
    let amount = body.get("amount_erg").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let chain = body.get("foreign_chain").and_then(|v| v.as_str()).unwrap_or("?");
    let msg = body.get("message").and_then(|v| v.as_str()).unwrap_or("");

    println!();
    println!("  Invoice Status");
    println!("  ─────────────────────────────────────────");
    println!("  Invoice ID:  {}", id);
    println!("  Status:      {}", status);
    println!("  Amount:      {} ERG", amount);
    println!("  Chain:       {}", chain);
    if !msg.is_empty() {
        println!("  Note:        {}", msg);
    }
    println!();

    Ok(())
}

async fn cmd_bridge_invoice_confirm(relay_url: &str, invoice_id: &str, foreign_tx_id: &str) -> Result<()> {
    let wallet = require_wallet()?;

    let body = serde_json::json!({
        "invoice_id": invoice_id,
        "foreign_tx_id": foreign_tx_id,
        "provider_address": wallet.public_key,
    });

    let body_bytes = serde_json::to_vec(&body)?;
    let (req_builder, _client) = build_signed_request(
        &wallet,
        "POST",
        "/v1/bridge/confirm",
        relay_url,
        &body_bytes,
    )?;

    let resp = req_builder
        .body(body_bytes)
        .send()
        .await
        .context("Failed to confirm bridge payment via relay")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Relay returned HTTP {}: {}", status, body_text);
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .context("Failed to parse bridge confirm response")?;

    let ergo_tx_id = body.get("ergo_tx_id").and_then(|v| v.as_str()).unwrap_or("pending");
    let msg = body.get("message").and_then(|v| v.as_str()).unwrap_or("");

    println!();
    println!("  Payment Confirmed");
    println!("  ─────────────────────────────────────────");
    println!("  Invoice ID:    {}", invoice_id);
    println!("  Foreign Tx:    {}", foreign_tx_id);
    println!("  Ergo Tx:       {}", ergo_tx_id);
    println!();
    if !msg.is_empty() {
        println!("  {}", msg);
        println!();
    }

    Ok(())
}

async fn cmd_bridge_invoice_refund(relay_url: &str, invoice_id: &str) -> Result<()> {
    let wallet = require_wallet()?;

    let body = serde_json::json!({
        "invoice_id": invoice_id,
        "buyer_address": wallet.public_key,
    });

    let body_bytes = serde_json::to_vec(&body)?;
    let (req_builder, _client) = build_signed_request(
        &wallet,
        "POST",
        "/v1/bridge/refund",
        relay_url,
        &body_bytes,
    )?;

    let resp = req_builder
        .body(body_bytes)
        .send()
        .await
        .context("Failed to refund bridge invoice via relay")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Relay returned HTTP {}: {}", status, body_text);
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .context("Failed to parse bridge refund response")?;

    let ergo_tx_id = body.get("ergo_tx_id").and_then(|v| v.as_str()).unwrap_or("pending");
    let msg = body.get("message").and_then(|v| v.as_str()).unwrap_or("");

    println!();
    println!("  Refund Initiated");
    println!("  ─────────────────────────────────────────");
    println!("  Invoice ID:  {}", invoice_id);
    println!("  Ergo Tx:     {}", ergo_tx_id);
    println!();
    if !msg.is_empty() {
        println!("  {}", msg);
        println!();
    }

    Ok(())
}