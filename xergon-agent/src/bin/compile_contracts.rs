//! compile_contracts -- Compile ErgoScript (.es) files to ErgoTree hex via an Ergo node.
//!
//! Reads .es/.ergo source files from the contracts directory, sends each to the Ergo node's
//! REST API at POST /script/p2sAddress, extracts the ErgoTree hex from the returned
//! P2S address (base58 decoded), and writes the hex to contracts/compiled/*.hex.
//!
//! Usage:
//!   cargo run --bin compile_contracts -- [OPTIONS]
//!
//! Options:
//!   --node-url <URL>    Ergo node REST API URL (default: http://127.0.0.1:9053)
//!   --contracts-dir <DIR>   Directory containing .es source files
//!   --output-dir <DIR>      Directory to write .hex output files
//!   --dry-run             Print what would be compiled without writing
//!   --verify              Check that output hex differs from known placeholders

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use blake2::Digest as _;
use clap::Parser;
use serde::Deserialize;
use tokio::io::AsyncWriteExt;

// ---------------------------------------------------------------------------
// Known placeholder hex prefixes -- if the compiled output starts with these,
// it means the source contains placeholder tokens and the result is not real.
// ---------------------------------------------------------------------------

/// Placeholder detection: these are patterns found in the fake hex files.
/// We detect placeholders by checking if the compiled hex matches the old file.
const PLACEHOLDER_MARKER: &str = "100804020e36100204a00b08cd";

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(
    name = "compile_contracts",
    about = "Compile ErgoScript contracts to ErgoTree hex via an Ergo node",
    version
)]
struct Cli {
    /// Ergo node REST API URL
    #[arg(long, default_value = "http://127.0.0.1:9053", env = "ERGO_NODE_URL")]
    node_url: String,

    /// Directory containing .es source files
    #[arg(long, default_value = "contracts")]
    contracts_dir: PathBuf,

    /// Directory to write .hex output files
    #[arg(long, default_value = "contracts/compiled")]
    output_dir: PathBuf,

    /// Print what would be compiled without actually compiling or writing
    #[arg(long)]
    dry_run: bool,

    /// Verify that output hex differs from known placeholders
    #[arg(long)]
    verify: bool,
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Response from POST /script/p2sAddress
#[derive(Debug, Deserialize)]
struct P2SAddressResponse {
    address: String,
}

// ---------------------------------------------------------------------------
// Contract compilation
// ---------------------------------------------------------------------------

/// Known contract file names (without extension)
const CONTRACT_NAMES: &[&str] = &[
    "provider_box",
    "provider_registration",
    "treasury_box",
    "usage_proof",
    "user_staking",
    "gpu_rental",
    "usage_commitment",
    "relay_registry",
    "gpu_rating",
    "gpu_rental_listing",
    "payment_bridge",
];

/// Read a contract source file, trying .es first then .ergo extension.
fn read_contract_source(contracts_dir: &Path, name: &str) -> Result<String> {
    // Try .es extension first
    let es_path = contracts_dir.join(format!("{name}.es"));
    if es_path.exists() {
        let source = std::fs::read_to_string(&es_path)
            .with_context(|| format!("Failed to read contract source: {}", es_path.display()))?;
        return Ok(source);
    }
    // Fall back to .ergo extension
    let ergo_path = contracts_dir.join(format!("{name}.ergo"));
    if ergo_path.exists() {
        let source = std::fs::read_to_string(&ergo_path)
            .with_context(|| format!("Failed to read contract source: {}", ergo_path.display()))?;
        return Ok(source);
    }
    // Neither found -- report both paths tried
    anyhow::bail!(
        "Contract source not found: tried {} and {}",
        es_path.display(),
        ergo_path.display()
    );
}

/// Call the Ergo node to compile ErgoScript source into a P2S address.
async fn compile_via_node(
    client: &reqwest::Client,
    node_url: &str,
    source: &str,
) -> Result<P2SAddressResponse> {
    let url = format!("{node_url}/script/p2sAddress");
    let body = serde_json::json!({ "source": source });

    let resp = client
        .post(&url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .with_context(|| format!("Failed to connect to Ergo node at {url}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        bail!("Node returned error {status}: {text}");
    }

    resp.json::<P2SAddressResponse>()
        .await
        .context("Failed to parse node response as JSON")
}

/// Decode a P2S address from base58 and extract the ErgoTree hex.
///
/// P2S address format:
///   byte 0: network prefix (0x01 mainnet, 0x02 testnet)
///   bytes 1..N-4: ErgoTree serialized bytes
///   bytes N-4..N: 4-byte checksum (first 4 bytes of BLAKE2b256 of prefix+content)
///
/// We return the hex of bytes 1..N-4 (the ErgoTree proper).
fn extract_ergotree_hex(address: &str) -> Result<String> {
    let decoded = bs58::decode(address).into_vec()
        .context("Failed to decode P2S address from base58")?;

    if decoded.len() < 6 {
        bail!(
            "Decoded address too short ({} bytes), expected at least 6",
            decoded.len()
        );
    }

    // Verify checksum: BLAKE2b256(prefix_byte || content_bytes), take first 4 bytes
    let content_len = decoded.len() - 4;
    let checksum_stored = &decoded[content_len..];
    let mut hasher = blake2::Blake2b::<blake2::digest::typenum::U32>::new();
    blake2::Digest::update(&mut hasher, &decoded[..content_len]);
    let hasher = blake2::Digest::finalize(hasher);
    let checksum_expected = &hasher[..4];

    if checksum_stored != checksum_expected {
        bail!(
            "Address checksum mismatch: expected {expected:02x?}, got {stored:02x?}",
            expected = checksum_expected,
            stored = checksum_stored,
        );
    }

    // ErgoTree = everything after the first byte (network prefix)
    let ergotree_bytes = &decoded[1..content_len];
    let hex = hex::encode(ergotree_bytes);
    Ok(hex)
}

/// Write the ErgoTree hex to the output .hex file.
async fn write_hex_file(output_dir: &Path, name: &str, hex: &str) -> Result<()> {
    tokio::fs::create_dir_all(output_dir)
        .await
        .context("Failed to create output directory")?;

    let hex_path = output_dir.join(format!("{name}.hex"));
    let mut file = tokio::fs::File::create(&hex_path)
        .await
        .with_context(|| format!("Failed to create {}", hex_path.display()))?;

    file.write_all(hex.as_bytes())
        .await
        .context("Failed to write hex content")?;

    Ok(())
}

/// Read an existing .hex file (for placeholder comparison).
fn read_existing_hex(output_dir: &Path, name: &str) -> Option<String> {
    let hex_path = output_dir.join(format!("{name}.hex"));
    std::fs::read_to_string(&hex_path).ok().map(|s| s.trim().to_string())
}

/// Check if a hex string looks like a known placeholder.
fn is_placeholder(hex: &str) -> bool {
    hex.starts_with(PLACEHOLDER_MARKER)
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    println!("Xergon Contract Compiler");
    println!("  Node URL:     {}", cli.node_url);
    println!("  Contracts:    {}", cli.contracts_dir.display());
    println!("  Output:       {}", cli.output_dir.display());
    if cli.dry_run {
        println!("  Mode:         DRY RUN (no compilation or writes)");
    }
    if cli.verify {
        println!("  Verify:       checking output vs placeholders");
    }
    println!();

    // Build HTTP client
    let client = Arc::new(
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?,
    );

    if cli.dry_run {
        println!("Contracts to compile:");
        for name in CONTRACT_NAMES {
            let source = read_contract_source(&cli.contracts_dir, name)?;
            let line_count = source.lines().count();
            let existing = read_existing_hex(&cli.output_dir, name);
            let status = match &existing {
                Some(hex) if is_placeholder(hex) => "PLACEHOLDER",
                Some(_) => "exists",
                None => "no output file",
            };
            println!("  {name:25} ({line_count:3} lines)  [{status}]");
        }
        println!("\nDry run complete. No files were modified.");
        return Ok(());
    }

    // Health check: can we reach the node?
    println!("Checking Ergo node connectivity...");
    let info_url = format!("{}/info", cli.node_url);
    match client.get(&info_url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let info: serde_json::Value = resp.json().await.unwrap_or_default();
            let version = info["nodeVersion"].as_str().unwrap_or("unknown");
            let network = info["network"].as_str().unwrap_or("unknown");
            println!("  Node version: {version}");
            println!("  Network:      {network}");
        }
        Ok(resp) => {
            eprintln!("  Warning: Node returned status {}", resp.status());
        }
        Err(e) => {
            bail!("Cannot reach Ergo node at {}: {e}", cli.node_url);
        }
    }
    println!();

    // Compile each contract
    let mut compiled = 0usize;
    let mut failed = 0usize;

    for name in CONTRACT_NAMES {
        print!("Compiling {name:25} ... ");
        std::io::Write::flush(&mut std::io::stdout()).ok();

        // Read source
        let source = match read_contract_source(&cli.contracts_dir, name) {
            Ok(s) => s,
            Err(e) => {
                println!("SKIP (source not found: {e})");
                failed += 1;
                continue;
            }
        };

        // Compile via node
        let response = match compile_via_node(&client, &cli.node_url, &source).await {
            Ok(r) => r,
            Err(e) => {
                println!("FAILED ({e})");
                failed += 1;
                continue;
            }
        };

        // Extract ErgoTree hex from P2S address
        let ergotree_hex = match extract_ergotree_hex(&response.address) {
            Ok(h) => h,
            Err(e) => {
                println!("FAILED (hex extraction: {e})");
                failed += 1;
                continue;
            }
        };

        let hex_len = ergotree_hex.len();

        // Verify mode: check against placeholder
        if cli.verify {
            if let Some(old_hex) = read_existing_hex(&cli.output_dir, name) {
                if old_hex == ergotree_hex {
                    println!("WARNING (output matches existing -- may be placeholder)");
                    compiled += 1;
                    continue;
                }
                if is_placeholder(&old_hex) && !is_placeholder(&ergotree_hex) {
                    println!("OK ({hex_len} chars, replaced placeholder)");
                }
            }
        }

        // Write output
        match write_hex_file(&cli.output_dir, name, &ergotree_hex).await {
            Ok(()) => {
                println!("OK ({hex_len} chars)");
                compiled += 1;
            }
            Err(e) => {
                println!("FAILED (write: {e})");
                failed += 1;
            }
        }
    }

    println!();
    println!("Results: {compiled} compiled, {failed} failed out of {} contracts", CONTRACT_NAMES.len());

    if failed > 0 {
        bail!("{failed} contract(s) failed to compile");
    }

    Ok(())
}
