//! Interactive first-run setup for xergon-agent.
//!
//! Generates a `config.toml` with sensible defaults based on auto-detected
//! hardware and user input. Uses std::io for prompts with ANSI colour codes.

use anyhow::{Context, Result};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// ANSI helpers
// ---------------------------------------------------------------------------

const CYAN: &str = "\x1b[36m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

fn cyan(s: &str) -> String {
    format!("{}{}{}", CYAN, s, RESET)
}
fn green(s: &str) -> String {
    format!("{}{}{}", GREEN, s, RESET)
}
fn yellow(s: &str) -> String {
    format!("{}{}{}", YELLOW, s, RESET)
}
fn bold(s: &str) -> String {
    format!("{}{}{}", BOLD, s, RESET)
}
fn dim(s: &str) -> String {
    format!("{}{}{}", DIM, s, RESET)
}

// ---------------------------------------------------------------------------
// Auto-detection structs
// ---------------------------------------------------------------------------

struct DetectedHardware {
    gpu_name: Option<String>,
    gpu_memory: Option<String>,
    ergo_node_reachable: bool,
    ergo_node_version: Option<String>,
    ollama_available: bool,
    llama_server_available: bool,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub async fn run_interactive_setup() -> Result<()> {
    print_banner();

    let mut hw = detect_hardware().await;
    print_hardware_detection(&hw);

    let config_path = default_config_path();

    // ---- Provider name ----
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "xergon-node".into());

    let provider_name = prompt_string("Provider display name", Some(&hostname))?;

    // ---- Region ----
    println!();
    println!("{}", bold("  Available regions:"));
    let regions = [
        "us-east",
        "us-west",
        "us-central",
        "eu-west",
        "eu-central",
        "eu-north",
        "ap-east",
        "ap-southeast",
        "ap-northeast",
        "sa-east",
    ];
    for (i, r) in regions.iter().enumerate() {
        println!("    {}  {}", cyan(&format!("{:2})", i + 1)), r);
    }
    let region = prompt_choice("Select your region", "1", &regions)?;

    // ---- Ergo node URL ----
    let ergo_url = prompt_string(
        "Ergo node REST URL",
        Some("http://127.0.0.1:9053"),
    )?;
    let ergo_url = ergo_url.trim().to_string();

    // ---- Ergo address ----
    let ergo_address = if hw.ergo_node_reachable {
        match get_ergo_wallet_address(&ergo_url).await {
            Some(addr) => {
                println!("  {} Detected wallet address: {}", green("\u{2713}"), dim(&addr));
                prompt_string("Ergo address for PoNW identity", Some(&addr))?
            }
            None => prompt_string("Ergo address for PoNW identity", None)?,
        }
    } else {
        println!(
            "  {} Ergo node not reachable -- enter address manually",
            yellow("!")
        );
        prompt_string("Ergo address for PoNW identity", None)?
    };

    // ---- AI backend ----
    println!();
    println!("{}", bold("  AI backend selection:"));
    let mut backend_options: Vec<&str> = Vec::new();
    if hw.ollama_available {
        backend_options.push("Ollama (http://127.0.0.1:11434)");
    }
    if hw.llama_server_available {
        backend_options.push("llama.cpp server (http://127.0.0.1:8080)");
    }
    backend_options.push("Install Ollama");
    backend_options.push("Custom URL");
    backend_options.push("None (AI inference disabled)");

    for (i, opt) in backend_options.iter().enumerate() {
        let marker = if (hw.ollama_available && i == 0)
            || (!hw.ollama_available && hw.llama_server_available && i == 0)
        {
            green("\u{2713}")
        } else {
            " ".to_string()
        };
        println!(
            "    {}  {} {}",
            marker,
            cyan(&format!("{:2})", i + 1)),
            opt
        );
    }

    let backend_choice = prompt_choice(
        "Select AI backend",
        "1",
        &backend_options
            .iter()
            .map(|s| s.as_ref())
            .collect::<Vec<_>>(),
    )?;

    let (inference_url, inference_enabled) = match backend_choice.as_str() {
        s if s.contains("Ollama") && !s.contains("Install") => {
            ("http://127.0.0.1:11434".to_string(), true)
        }
        s if s.contains("llama.cpp") => ("http://127.0.0.1:8080".to_string(), true),
        s if s.contains("Install") => {
            // Auto-install Ollama
            let installed = auto_install_ollama().await?;
            if installed {
                hw.ollama_available = true;
                ("http://127.0.0.1:11434".to_string(), true)
            } else {
                println!("  {} Ollama installation skipped or failed.", yellow("!"));
                ("http://127.0.0.1:11434".to_string(), false)
            }
        }
        s if s.contains("Custom") => {
            let url = prompt_string("Custom inference backend URL", None)?;
            (url, true)
        }
        _ => ("http://127.0.0.1:11434".to_string(), false),
    };

    // ---- Auto-install Ollama if backend is Ollama but not detected ----
    if inference_enabled
        && inference_url.contains("11434")
        && !hw.ollama_available
        && !backend_choice.contains("Install")
    {
        println!();
        println!(
            "  {} Ollama selected but not detected on this system.",
            yellow("!")
        );
        let install = prompt_yes_no(
            "Would you like to install Ollama?",
            true,
        )?;
        if install {
            let installed = auto_install_ollama().await?;
            if installed {
                hw.ollama_available = true;
            }
        }
    }

    // ---- Model pull (if Ollama was just installed and has no models) ----
    if hw.ollama_available && inference_enabled && inference_url.contains("11434") {
        let installed_models = get_ollama_models().await.unwrap_or_default();
        if installed_models.is_empty() {
            println!();
            println!("{}", bold("  No Ollama models found. Pull a recommended model:"));
            println!("    {}  qwen3.5-4b  (2.8GB, fast)", cyan("1)"));
            println!("    {}  llama3.1-8b (4.7GB, balanced)", cyan("2)"));
            println!("    {}  mistral-7b   (4.1GB, efficient)", cyan("3)"));
            println!("    {}  Skip model pull", cyan("4)"));

            let model_choice = prompt_string("Select model to pull", Some("1"))?;
            let model_name = match model_choice.trim() {
                "1" => Some("qwen3.5:4b"),
                "2" => Some("llama3.1:8b"),
                "3" => Some("mistral:7b"),
                _ => None,
            };

            if let Some(model) = model_name {
                println!();
                println!("  {} Pulling {} ...", dim("->"), bold(model));
                let status = tokio::process::Command::new("ollama")
                    .args(["pull", model])
                    .status()
                    .await
                    .with_context(|| "Failed to run ollama pull")?;

                if status.success() {
                    println!("  {} Model {} pulled successfully.", green("\u{2713}"), model);
                } else {
                    println!(
                        "  {} Failed to pull model {}. You can pull it later with: ollama pull {}",
                        yellow("!"),
                        model,
                        model
                    );
                }
            }
        }
    }

    // ---- Model selection ----
    let mut served_models: Vec<String> = Vec::new();
    if hw.ollama_available && inference_enabled && inference_url.contains("11434") {
        println!();
        let installed_models = get_ollama_models().await.unwrap_or_default();
        if !installed_models.is_empty() {
            println!("{}", bold("  Available Ollama models:"));
            println!("  ── Models ──────────────────────────");
            for (i, m) in installed_models.iter().enumerate() {
                // First model is selected by default
                let marker = if i == 0 {
                    green("\u{2713}")
                } else {
                    " ".to_string()
                };
                println!("    {} [{}] {}", marker, if i == 0 { "X" } else { " " }, m);
            }
            println!("  ────────────────────────────────────");
            println!(
                "  {} Toggle models by entering numbers (e.g. 1 3). Enter to accept defaults.",
                dim("Tip:")
            );

            let selection_input = prompt_string(
                "Select models to serve",
                Some("1"),
            )?;

            // Parse selection: default is first model
            if selection_input.trim().is_empty() {
                if let Some(first) = installed_models.first() {
                    served_models.push(first.clone());
                }
            } else {
                // Parse space-separated numbers
                let indices: Vec<usize> = selection_input
                    .split_whitespace()
                    .filter_map(|s| s.parse::<usize>().ok())
                    .collect();

                if indices.is_empty() {
                    // Default: first model
                    if let Some(first) = installed_models.first() {
                        served_models.push(first.clone());
                    }
                } else {
                    for idx in indices {
                        if idx >= 1 && idx <= installed_models.len() {
                            let model = &installed_models[idx - 1];
                            if !served_models.contains(model) {
                                served_models.push(model.clone());
                            }
                        }
                    }
                }
            }

            if served_models.is_empty() {
                // Fallback to first model
                if let Some(first) = installed_models.first() {
                    served_models.push(first.clone());
                }
            }

            println!(
                "  {} Serving models: {}",
                green("\u{2713}"),
                bold(&served_models.join(", "))
            );
        }
    }

    // ---- GPU mode ----
    println!();
    println!("{}", bold("  GPU mode:"));
    let gpu_modes = vec![
        "Mine + Serve (GPU mines ERG & serves AI)",
        "Serve only (GPU dedicated to AI)",
    ];
    for (i, m) in gpu_modes.iter().enumerate() {
        println!("    {}  {}", cyan(&format!("{:2})", i + 1)), m);
    }
    let _gpu_mode = prompt_choice(
        "Select GPU mode",
        "1",
        &gpu_modes
            .iter()
            .map(|s| s.as_ref())
            .collect::<Vec<_>>(),
    )?;

    // ---- Generate provider_id ----
    let provider_id = format!(
        "Xergon_{}",
        provider_name
            .to_uppercase()
            .replace(' ', "_")
            .chars()
            .take(12)
            .collect::<String>()
    );

    // ---- Build config TOML ----
    let config_toml = build_config_toml(
        &provider_id,
        &provider_name,
        &region,
        &ergo_url,
        &ergo_address,
        &inference_url,
        inference_enabled,
        &served_models,
    );

    // ---- Write config ----
    println!();
    println!("{}", bold("  Generated configuration:"));
    println!("{}", dim(&config_toml));
    println!();

    let write_path = prompt_string(
        "Config save path",
        Some(&config_path.display().to_string()),
    )?;
    let write_path = PathBuf::from(&write_path);

    // Create parent dirs if needed
    if let Some(parent) = write_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {:?}", parent))?;
    }

    fs::write(&write_path, &config_toml)
        .with_context(|| format!("Failed to write config to {:?}", write_path))?;

    println!();
    println!(
        "  {} Config written to {}",
        green("\u{2713}"),
        bold(&write_path.display().to_string())
    );

    // ---- Service installation ----
    println!();
    let install_service = prompt_yes_no(
        "Would you like to install xergon-agent as a system service?",
        true,
    )?;

    if install_service {
        install_system_service(&write_path).await?;
    }

    println!();
    println!("  {} Next steps:", bold("->"));
    println!(
        "    1. Review the generated config: {}",
        dim(&write_path.display().to_string())
    );
    println!("    2. Start the agent: {}", green("xergon-agent run"));
    println!("    3. Check status:       {}", green("xergon-agent status"));
    println!();

    Ok(())
}

// ---------------------------------------------------------------------------
// Banner
// ---------------------------------------------------------------------------

fn print_banner() {
    println!();
    println!("  \x1b[36m\x1b[1m  XERGON AGENT SETUP\x1b[0m");
    println!("  \x1b[2m  P2P AI Compute for Ergo\x1b[0m");
    println!();
}

// ---------------------------------------------------------------------------
// Hardware detection
// ---------------------------------------------------------------------------

async fn detect_hardware() -> DetectedHardware {
    let mut hw = DetectedHardware {
        gpu_name: None,
        gpu_memory: None,
        ergo_node_reachable: false,
        ergo_node_version: None,
        ollama_available: false,
        llama_server_available: false,
    };

    // GPU detection via nvidia-smi
    if let Ok(output) = tokio::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total",
            "--format=csv,noheader",
        ])
        .output()
        .await
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let line = stdout.lines().next().unwrap_or("");
            let parts: Vec<&str> = line.splitn(2, ',').collect();
            if parts.len() == 2 {
                hw.gpu_name = Some(parts[0].trim().to_string());
                hw.gpu_memory = Some(parts[1].trim().to_string());
            }
        }
    }

    // HTTP client for service detection
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .ok();

    // Ergo node detection
    if let Some(ref client) = client {
        if let Ok(resp) = client.get("http://127.0.0.1:9053/info").send().await {
            if resp.status().is_success() {
                hw.ergo_node_reachable = true;
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    hw.ergo_node_version = body
                        .get("appVersion")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                }
            }
        }
    }

    // Ollama detection
    if let Some(ref client) = client {
        if let Ok(resp) = client.get("http://127.0.0.1:11434/api/version").send().await {
            hw.ollama_available = resp.status().is_success();
        }
    }

    // llama.cpp server detection
    if let Some(ref client) = client {
        if let Ok(resp) = client.get("http://127.0.0.1:8080/v1/models").send().await {
            hw.llama_server_available = resp.status().is_success();
        }
    }

    hw
}

fn print_hardware_detection(hw: &DetectedHardware) {
    println!("  {} Hardware detection results:", bold("->"));
    println!();

    if let (Some(name), Some(mem)) = (&hw.gpu_name, &hw.gpu_memory) {
        println!("    {} GPU: {} ({})", green("\u{2713}"), name, mem);
    } else {
        println!(
            "    {} GPU: None detected (CPU-only mode)",
            yellow("!")
        );
    }

    if hw.ergo_node_reachable {
        let ver = hw
            .ergo_node_version
            .as_deref()
            .unwrap_or("unknown version");
        println!("    {} Ergo node: Connected (v{})", green("\u{2713}"), ver);
    } else {
        println!(
            "    {} Ergo node: Not reachable at http://127.0.0.1:9053",
            yellow("\u{2717}")
        );
    }

    if hw.ollama_available {
        println!("    {} Ollama: Available at :11434", green("\u{2713}"));
    } else {
        println!("    {} Ollama: Not detected at :11434", dim("\u{2717}"));
    }

    if hw.llama_server_available {
        println!("    {} llama.cpp: Available at :8080", green("\u{2713}"));
    } else {
        println!("    {} llama.cpp: Not detected at :8080", dim("\u{2717}"));
    }

    println!();
}

// ---------------------------------------------------------------------------
// Prompt helpers
// ---------------------------------------------------------------------------

fn prompt_string(prompt: &str, default: Option<&str>) -> Result<String> {
    let default_hint = match default {
        Some(d) => format!(" [{}]", dim(d)),
        None => String::new(),
    };
    print!("  {}{}: {} ", bold("?"), prompt, default_hint);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();

    if input.is_empty() {
        Ok(default
            .map(|d| {
                // Strip any "(auto-detected)" suffix from defaults
                d.split("  (auto-detected)")
                    .next()
                    .unwrap_or(d)
                    .trim()
                    .to_string()
            })
            .unwrap_or_default())
    } else {
        Ok(input.to_string())
    }
}

fn prompt_choice(prompt: &str, default: &str, options: &[&str]) -> Result<String> {
    let input = prompt_string(prompt, Some(default))?;

    // Try to parse as number
    if let Ok(num) = input.parse::<usize>() {
        if num >= 1 && num <= options.len() {
            return Ok(options[num - 1].to_string());
        }
    }

    // Otherwise check if it matches an option directly
    for opt in options {
        if opt.eq_ignore_ascii_case(&input) {
            return Ok(opt.to_string());
        }
    }

    // Return default if input doesn't match
    if let Ok(num) = default.parse::<usize>() {
        if num >= 1 && num <= options.len() {
            return Ok(options[num - 1].to_string());
        }
    }

    Ok(input)
}

fn prompt_yes_no(prompt: &str, default: bool) -> Result<bool> {
    let hint = if default { "Y/n" } else { "y/N" };
    print!("  {} {} [{}]: ", bold("?"), prompt, cyan(hint));
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();

    Ok(match input.as_str() {
        "y" | "yes" => true,
        "n" | "no" => false,
        "" => default,
        _ => default,
    })
}

// ---------------------------------------------------------------------------
// Auto-install Ollama
// ---------------------------------------------------------------------------

async fn auto_install_ollama() -> Result<bool> {
    let os = std::env::consts::OS;

    println!();
    println!(
        "  {} Installing Ollama for {}...",
        dim("->"),
        bold(os)
    );

    let success = match os {
        "linux" => {
            println!("  {} Running Ollama install script...", dim("->"));
            let output = tokio::process::Command::new("sh")
                .arg("-c")
                .arg("curl -fsSL https://ollama.com/install.sh | sh")
                .output()
                .await
                .with_context(|| "Failed to execute Ollama install script")?;

            if output.status.success() {
                println!(
                    "  {} Ollama installed via install.sh",
                    green("\u{2713}")
                );
                true
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                println!("  {} Install script failed: {}", yellow("!"), dim(&stderr));
                false
            }
        }
        "macos" => {
            // Check if brew exists
            let brew_check = tokio::process::Command::new("which")
                .arg("brew")
                .output()
                .await;

            if brew_check.is_ok_and(|o| o.status.success()) {
                println!("  {} Running brew install ollama...", dim("->"));
                let output = tokio::process::Command::new("brew")
                    .args(["install", "ollama"])
                    .output()
                    .await
                    .with_context(|| "Failed to run brew install ollama")?;

                if output.status.success() {
                    println!("  {} Ollama installed via Homebrew", green("\u{2713}"));
                    true
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    println!("  {} brew install failed: {}", yellow("!"), dim(&stderr));
                    false
                }
            } else {
                println!(
                    "  {} Homebrew not found. Please install Homebrew first or install Ollama manually:",
                    yellow("!")
                );
                println!("    {} Download from https://ollama.com/download", dim("->"));
                false
            }
        }
        _ => {
            println!(
                "  {} Unsupported OS: {}. Please install Ollama manually from https://ollama.com",
                yellow("!"),
                os
            );
            false
        }
    };

    if success {
        // Wait for Ollama to start up
        println!("  {} Waiting for Ollama to start...", dim("->"));
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        // Verify Ollama is reachable
        if let Ok(client) = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
        {
            if let Ok(resp) = client
                .get("http://127.0.0.1:11434/api/version")
                .send()
                .await
            {
                if resp.status().is_success() {
                    println!("  {} Ollama is running and reachable!", green("\u{2713}"));
                    return Ok(true);
                }
            }
        }
        println!(
            "  {} Ollama installed but not yet reachable. Start it with: ollama serve",
            yellow("!")
        );
        // Still return true - it's installed, just not running
        return Ok(true);
    }

    Ok(false)
}

// ---------------------------------------------------------------------------
// Get Ollama models
// ---------------------------------------------------------------------------

async fn get_ollama_models() -> Result<Vec<String>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    let resp = client
        .get("http://127.0.0.1:11434/api/tags")
        .send()
        .await
        .with_context(|| "Failed to query Ollama models")?;

    if !resp.status().is_success() {
        anyhow::bail!("Ollama API returned status {}", resp.status());
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .with_context(|| "Failed to parse Ollama response")?;

    let models = body
        .get("models")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|model| {
                    model
                        .get("name")
                        .and_then(|n| n.as_str())
                        .map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(models)
}

// ---------------------------------------------------------------------------
// System service installation
// ---------------------------------------------------------------------------

async fn install_system_service(config_path: &std::path::Path) -> Result<()> {
    let os = std::env::consts::OS;
    let current_exe = std::env::current_exe()
        .with_context(|| "Failed to determine current executable path")?;
    let abs_binary = current_exe.to_string_lossy().to_string();
    let abs_config = config_path.to_string_lossy().to_string();

    match os {
        "linux" => install_systemd_service(&abs_binary, &abs_config).await,
        "macos" => install_launchd_service(&abs_binary, &abs_config).await,
        _ => {
            println!(
                "  {} Service installation not supported on {}. Manage the service manually.",
                yellow("!"),
                os
            );
            Ok(())
        }
    }
}

async fn install_systemd_service(binary_path: &str, config_path: &str) -> Result<()> {
    let service_dir = dirs_home()
        .join(".config/systemd/user");
    fs::create_dir_all(&service_dir)
        .with_context(|| format!("Failed to create {:?}", service_dir))?;

    let service_path = service_dir.join("xergon-agent.service");

    let service_content = format!(
        "[Unit]\n\
         Description=Xergon Network Agent\n\
         After=network-online.target\n\
         Wants=network-online.target\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={binary} run --config {config}\n\
         Restart=on-failure\n\
         RestartSec=10\n\
         Environment=RUST_LOG=info\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        binary = binary_path,
        config = config_path,
    );

    fs::write(&service_path, &service_content)
        .with_context(|| format!("Failed to write {:?}", service_path))?;

    println!("  {} systemd service written to {}", green("\u{2713}"), service_path.display());

    // Reload and enable
    println!("  {} Enabling service...", dim("->"));
    let status = tokio::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status()
        .await
        .with_context(|| "Failed to run systemctl --user daemon-reload")?;

    if !status.success() {
        println!("  {} daemon-reload failed. Run manually: systemctl --user daemon-reload", yellow("!"));
    }

    let status = tokio::process::Command::new("systemctl")
        .args(["--user", "enable", "xergon-agent"])
        .status()
        .await
        .with_context(|| "Failed to run systemctl --user enable xergon-agent")?;

    if status.success() {
        println!("  {} Service enabled. Start with: systemctl --user start xergon-agent", green("\u{2713}"));
    } else {
        println!("  {} Failed to enable service. Run manually: systemctl --user enable xergon-agent", yellow("!"));
    }

    Ok(())
}

async fn install_launchd_service(binary_path: &str, config_path: &str) -> Result<()> {
    let plist_dir = dirs_home()
        .join("Library/LaunchAgents");
    fs::create_dir_all(&plist_dir)
        .with_context(|| format!("Failed to create {:?}", plist_dir))?;

    let plist_path = plist_dir.join("ai.xergon.agent.plist");

    let plist_content = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>ai.xergon.agent</string>
    <key>ProgramArguments</key>
    <array>
        <string>{binary}</string>
        <string>run</string>
        <string>--config</string>
        <string>{config}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/tmp/xergon-agent.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/xergon-agent.err</string>
</dict>
</plist>"#,
        binary = binary_path,
        config = config_path,
    );

    fs::write(&plist_path, &plist_content)
        .with_context(|| format!("Failed to write {:?}", plist_path))?;

    println!("  {} launchd plist written to {}", green("\u{2713}"), plist_path.display());

    // Load the service
    println!("  {} Loading service...", dim("->"));
    let status = tokio::process::Command::new("launchctl")
        .args(["load", &plist_path.to_string_lossy()])
        .status()
        .await
        .with_context(|| "Failed to run launchctl load")?;

    if status.success() {
        println!("  {} Service loaded and started!", green("\u{2713}"));
    } else {
        println!(
            "  {} Failed to load service. Run manually: launchctl load {}",
            yellow("!"),
            plist_path.display()
        );
    }

    Ok(())
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/"))
}

// ---------------------------------------------------------------------------
// Ergo wallet address helper
// ---------------------------------------------------------------------------

async fn get_ergo_wallet_address(ergo_url: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .ok()?;

    let resp = client
        .get(format!(
            "{}/wallet/addresses",
            ergo_url.trim_end_matches('/')
        ))
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let body: serde_json::Value = resp.json().await.ok()?;
    body.get(0)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

// ---------------------------------------------------------------------------
// Default config path
// ---------------------------------------------------------------------------

fn default_config_path() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        let p = PathBuf::from(home).join(".xergon").join("config.toml");
        if p.parent().map_or(false, |d| d.exists()) {
            return p;
        }
    }
    PathBuf::from("config.toml")
}

// ---------------------------------------------------------------------------
// Config generation
// ---------------------------------------------------------------------------

fn build_config_toml(
    provider_id: &str,
    provider_name: &str,
    region: &str,
    ergo_url: &str,
    ergo_address: &str,
    inference_url: &str,
    inference_enabled: bool,
    served_models: &[String],
) -> String {
    // Determine llama_server URL based on inference backend
    let llama_url = if inference_url.contains("8080") {
        inference_url
    } else {
        "http://127.0.0.1:8080"
    };

    // Determine backend_type based on the inference URL
    let backend_type = if inference_url.contains("8080") {
        "llama_cpp"
    } else {
        "ollama"
    };

    // Format served_models as TOML array
    let served_models_toml = if served_models.is_empty() {
        String::new()
    } else {
        let models_list: Vec<String> = served_models
            .iter()
            .map(|m| format!("    \"{}\"", m))
            .collect();
        format!(
            "\nserved_models = [\n{}\n]",
            models_list.join(",\n")
        )
    };

    format!(
        r#"# Xergon Agent -- generated by setup
# Modify as needed. See docs for all options.

[ergo_node]
rest_url = "{ergo_url}"

[xergon]
provider_id = "{provider_id}"
provider_name = "{provider_name}"
region = "{region}"
ergo_address = "{ergo_address}"

[peer_discovery]
discovery_interval_secs = 300
probe_timeout_secs = 5
xergon_agent_port = 9099
max_concurrent_probes = 5
max_peers_per_cycle = 20
peers_file = "data/xergon-peers.json"

[api]
listen_addr = "0.0.0.0:9099"

[settlement]
enabled = false
interval_secs = 86400
dry_run = true
ledger_file = "data/settlement_ledger.json"
cost_per_1k_tokens_nanoerg = 1_000_000
min_settlement_nanoerg = 1_000_000_000

[llama_server]
url = "{llama_url}"
health_check_interval_secs = 60
ctx_size = 4096
threads = 0
gpu_layers = 0
n_batch = 512
use_fp16 = false
use_flash_attn = false
lock_gpu = false
model_name = "default"
contiguous_ctx = true

[inference]
enabled = {inference_enabled}
backend_type = "{backend_type}"
url = "{inference_url}"
timeout_secs = 120{served_models_toml}

[relay]
register_on_start = false
relay_url = ""
token=""
heartbeat_interval_secs = 60
"#,
        provider_id = provider_id,
        provider_name = provider_name,
        region = region,
        ergo_url = ergo_url,
        ergo_address = ergo_address,
        inference_url = inference_url,
        inference_enabled = inference_enabled,
        llama_url = llama_url,
        served_models_toml = served_models_toml,
        backend_type = backend_type,
    )
}
