# Provider Onboarding Guide

Welcome to the Xergon Network. This guide walks you through setting up your GPU as an AI inference provider and earning ERG for every request you serve.

## Overview

Xergon Network is a decentralized AI inference marketplace built on the Ergo blockchain. As a provider, you run **xergon-agent** alongside your inference backend (Ollama, llama.cpp, vLLM, etc.). The agent:

- Registers your node on-chain via a **Provider Box** guarded by a **Singleton NFT**
- Sends periodic heartbeats to keep your Provider Box alive
- Handles ERG settlement for inference payments
- Registers with relay nodes so users can discover you

You earn ERG on a **pay-per-use** model -- every inference request routed to your node generates on-chain payment.

---

## Prerequisites

| Requirement | Details |
|-------------|---------|
| **GPU** | NVIDIA (CUDA) or AMD (Vulkan) GPU. Minimum 8 GB VRAM recommended for 7B models. |
| **Inference backend** | [Ollama](https://ollama.com) (recommended), [llama.cpp](https://github.com/ggerganov/llama.cpp), or any OpenAI-compatible server |
| **Ergo wallet** | [Nautilus Wallet](https://nautiluswallet.com) (recommended) or any Ergo-compatible wallet |
| **Testnet ERG** | Needed for registration box value + transaction fees (~0.01 ERG to start) |
| **OS** | Linux (recommended), macOS, or Windows with WSL2 |
| **Rust toolchain** | Only if building from source |

---

## Step 1: Get Testnet ERG

Before registering, you need ERG in your wallet for the Provider Box minimum value (0.001 ERG) and transaction fees (0.001 ERG per tx).

### 1a. Install Nautilus Wallet

1. Download from [nautiluswallet.com](https://nautiluswallet.com)
2. Create a new wallet or import an existing one
3. Switch to **Testnet** mode in settings
4. Copy your Ergo address (starts with `9i` or `3W`)

### 1b. Get Testnet ERG from a Faucet

Use any of these Ergo testnet faucets:

- **Ergo Testnet Faucet**: https://testnet.ergo.foundation/faucet
- **Ergo Forum Faucet**: https://www.ergoforum.org/t/ergo-testnet-faucet/

Paste your Nautilus address and wait for the transaction to confirm (usually 2-3 minutes).

### 1c. Verify Balance

In Nautilus, check that your testnet balance shows at least **0.01 ERG**.

---

## Step 2: Install xergon-agent

### Option A: Download Pre-built Binary (Recommended)

```bash
# Linux (x86_64)
curl -sL https://github.com/n1ur0/Xergon-Network/releases/latest/download/xergon-agent-linux-x86_64 \
  -o xergon-agent && chmod +x xergon-agent

# macOS (Apple Silicon)
curl -sL https://github.com/n1ur0/Xergon-Network/releases/latest/download/xergon-agent-macos-aarch64 \
  -o xergon-agent && chmod +x xergon-agent
```

### Option B: Build from Source

```bash
# Prerequisites: Rust 1.75+, protobuf-compiler
git clone https://github.com/n1ur0/Xergon-Network.git
cd Xergon-Network/xergon-agent
cargo build --release
# Binary at: target/release/xergon-agent
```

### Option C: Use the Interactive Setup Wizard

```bash
./xergon-agent setup
```

This walks you through configuration interactively -- it detects your GPU, helps configure Ollama, and generates `config.toml`.

---

## Step 3: Configure

Edit the agent configuration file. If you used the setup wizard, this is already partially done.

```bash
cp config.toml.example config.toml
```

Open `config.toml` and set these fields:

### 3a. Ergo Node Connection

```toml
[ergo_node]
# For testnet, use a public node or run your own:
rest_url = "https://testnet.ergo-node.io:9053"
# Or for local development:
# rest_url = "http://127.0.0.1:9053"
```

> **Tip**: For production, run your own Ergo node. See the [Ergo node docs](https://docs.ergoplatform.com/node/running-a-node/).

### 3b. Provider Identity

```toml
[xergon]
provider_id = "your-provider-name"          # Unique human-readable ID
provider_name = "My GPU Node"              # Display name on marketplace
region = "us-east"                         # Your region for routing
ergo_address = "9fDrtP...your-address"     # Your Ergo P2S address from Nautilus
```

### 3c. Inference Backend

```toml
[llama_server]
url = "http://127.0.0.1:8080"              # Ollama default port
health_check_interval_secs = 60

[inference]
enabled = true
url = "http://127.0.0.1:11434"             # Ollama API
timeout_secs = 120
```

### 3d. Relay Registration

```toml
[relay]
register_on_start = true
relay_url = "https://relay.xergon.gg"     # Public relay (or your own)
token = "your-provider-token"              # Get this from the relay operator
heartbeat_interval_secs = 60              # How often to ping the relay
```

### 3e. Settlement (ERG Payments)

```toml
[settlement]
enabled = true
interval_secs = 86400                      # Settle once per day
dry_run = true                             # Set to false for real ERG transfers
ledger_file = "data/settlement_ledger.json"
min_settlement_usd = 0.10                  # Minimum $0.10 to trigger settlement
```

### 3f. Contract Overrides (Optional)

```toml
[contracts]
# Leave empty to use embedded compiled contracts.
# Override only if deploying custom contract versions.
provider_box_hex = ""
treasury_box_hex = ""
usage_proof_hex = ""
user_staking_hex = ""
```

---

## Step 4: Bootstrap (Register On-Chain)

Registration creates your **Provider NFT** (singleton, supply=1) and **Provider Box** on the Ergo blockchain. This is your permanent on-chain identity.

### 4a. Ensure Your Wallet is Unlocked

If running a local Ergo node, unlock the wallet first:

```bash
# Via Ergo node API
curl -X POST http://127.0.0.1:9053/wallet/unlock \
  -H "Content-Type: application/json" \
  -d '{"pass": "your-wallet-passphrase"}'
```

### 4b. Run Bootstrap

```bash
./xergon-agent bootstrap
```

This command:

1. **Mints a Provider NFT** -- a unique token with `supply=1` that serves as your permanent on-chain identifier
2. **Creates a Provider Box** -- an eUTXO box holding your NFT with registers:
   - R4: Your public key (authorization)
   - R5: Your endpoint URL
   - R6: Models you serve (JSON array)
   - R7: PoNW score (starts at 0)
   - R8: Last heartbeat block height
   - R9: Your region
3. **Stores bootstrap state** locally in `data/bootstrap_state.json` for idempotent re-runs

**Cost**: ~0.002 ERG (0.001 ERG box minimum value + 0.001 ERG fee)

### 4c. Verify Registration

Check your Provider Box on the [Ergo Explorer](https://testnet.ergoplatform.com):

```bash
# The bootstrap output prints your tx_id and provider_box_id
# Search the tx_id on the explorer to confirm
```

---

## Step 5: Start Serving

### 5a. Start Your Inference Backend

```bash
# With Ollama (pull a model first)
ollama pull llama3.1:8b
ollama serve
```

### 5b. Start the Agent

```bash
./xergon-agent run --config config.toml
```

Or simply:

```bash
./xergon-agent run
```

The agent will:

1. **Load contracts** and validate all 11 ErgoScript contracts
2. **Check Ergo node health** (sync status, peer count)
3. **Probe your inference backend** (health check against Ollama)
4. **Register with the relay** (if `relay.register_on_start = true`)
5. **Begin heartbeats** -- on-chain transactions every ~60 seconds to keep your Provider Box alive and update metadata
6. **Start the settlement loop** -- periodically settles ERG earnings

### 5c. What You Should See

```
INFO  Xergon Agent starting...
INFO  Configuration loaded provider_id=my-gpu-node region=us-east
INFO  Loaded compiled contracts total=11 valid=11 invalid=0
INFO  Initial node health check passed synced=true peers=25
INFO  Successfully registered with relay
INFO  Agent is live on http://0.0.0.0:9099
```

---

## Step 6: Monitor

### 6a. Agent Status API

```bash
# Full provider status
curl http://127.0.0.1:9099/xergon/status

# PoNW score
curl http://127.0.0.1:9099/xergon/pown

# Node health
curl http://127.0.0.1:9099/xergon/health
```

### 6b. Check On-Chain State

Use the Ergo Explorer to view your Provider Box:

```
https://testnet.ergoplatform.com/boxes/<your-provider-box-id>
```

Verify:
- The box contains your Provider NFT (supply=1)
- R4-R9 registers are populated
- R8 (heartbeat) updates periodically

### 6c. Relay Provider Listing

```bash
# Check if the relay sees you
curl https://relay.xergon.gg/v1/providers
```

### 6d. Provider Leaderboard

```bash
curl https://relay.xergon.gg/v1/leaderboard
```

Your provider should appear with its PoNW score after processing inference requests.

---

## Smart Contracts Reference

Xergon uses 11 ErgoScript contracts in the `/contracts/` directory:

| Contract | File | Purpose |
|----------|------|---------|
| Provider Box | `provider_box.ergo` | Provider identity and state (NFT-guarded singleton) |
| User Staking | `user_staking.ergo` | User prepaid balance box |
| Usage Proof | `usage_proof.ergo` | Immutable on-chain inference receipt |
| Treasury | `treasury.ergo` | Protocol treasury (NFT + ERG reserve) |
| GPU Rental | `gpu_rental.es` | GPU rental session management |
| GPU Listing | `gpu_rental_listing.es` | GPU availability listings |
| GPU Rating | `gpu_rating.es` | Provider/renter reputation |
| Relay Registry | `relay_registry.es` | Authorized relay nodes |
| Payment Bridge | `payment_bridge.es` | Cross-chain payment support |
| Usage Commitment | `usage_commitment.es` | Batch usage proofs (rollup) |

All contracts follow the **EIP-4** register convention (R4-R9).

---

## Payment Flow

When a user sends an inference request:

```
1. User Staking Box (balance - ERG deducted)
   |
   +--> 2. Provider Box (heartbeat + payment received)
   |
   +--> 3. Usage Proof Box (immutable receipt created)
```

All steps happen in a **single atomic Ergo transaction** -- either everything succeeds or nothing does. Your ERG earnings accumulate in the Provider Box and are settled periodically to your wallet via the settlement engine.

---

## Troubleshooting

### "Ergo node wallet is locked"

Unlock your wallet before bootstrap:
```bash
curl -X POST http://127.0.0.1:9053/wallet/unlock \
  -H "Content-Type: application/json" \
  -d '{"pass": "your-passphrase"}'
```

### "No healthy inference backend"

Ensure Ollama (or your backend) is running and accessible:
```bash
curl http://127.0.0.1:11434/api/tags
# Should return a JSON list of available models
```

### "Relay registration failed"

- Verify `relay_url` is reachable
- Check your `token` is correct
- Ensure your endpoint is publicly accessible (not `127.0.0.1` if the relay is remote). Set `XERGON__RELAY__AGENT_ENDPOINT` env var:
  ```bash
  export XERGON__RELAY__AGENT_ENDPOINT="http://your-public-ip:9099"
  ```

### Provider Box disappeared

Provider Boxes that don't receive heartbeats eventually fall victim to Ergo's storage rent mechanism. Ensure your agent is running continuously. Heartbeats reset the creation height, preventing expiry.

### Low PoNW Score

PoNW (Proof-of-Network-Work) measures your reliability. Improve by:
- Maintaining high uptime
- Serving requests quickly (low latency)
- Running popular/rare models (rarity bonus)
- Keeping your inference backend healthy

### Insufficient ERG for Heartbeats

Heartbeat transactions cost ~0.001 ERG in fees. Keep at least 0.01 ERG in your wallet. The settlement engine can be configured to auto-reserve ERG for operational costs.

---

## Production Checklist

- [ ] Running a local Ergo node (not relying on public nodes)
- [ ] Firewall configured: port 9099 open for relay health checks
- [ ] `dry_run = false` in settlement config for real ERG payments
- [ ] `XERGON__RELAY__AGENT_ENDPOINT` set to your public IP/hostname
- [ ] Ollama (or backend) configured as a systemd service for auto-restart
- [ ] Monitoring setup (agent `/xergon/status` endpoint)
- [ ] ERG wallet funded with enough for fees (at least 0.1 ERG buffer)
- [ ] Contract hex overrides set (if using custom contracts)

---

## Getting Help

- **GitHub Issues**: https://github.com/n1ur0/Xergon-Network/issues
- **Discord**: Join the Xergon community for real-time support
- **Contracts Reference**: See [`contracts/README.md`](../contracts/README.md) for full ErgoScript contract documentation
