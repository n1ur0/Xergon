# Xergon Provider Guide

A step-by-step guide to setting up and running a Xergon compute provider node.

---

## Prerequisites

- A computer with a GPU (NVIDIA recommended; AMD works with ROCm)
- 8 GB+ VRAM (16+ GB recommended for larger models)
- 16 GB+ system RAM
- An Ergo wallet with ERG for staking (minimum 0.1 ERG)
- Linux or macOS (Windows via WSL2)

---

## Quick Start (5 minutes)

### Step 1: Install Xergon CLI

```bash
curl -sSL https://degens.world/xergon | sh
```

Or build from source:

```bash
git clone https://github.com/n1ur0/Xergon-Network.git
cd Xergon-Network/xergon-agent
cargo build --release
cp target/release/xergon-agent ~/.xergon/bin/
```

### Step 2: Initial Setup

```bash
xergon-agent setup
```

This interactive wizard:
- Generates your keypair (BIP-39 mnemonic)
- Connects to an Ergo node (local or public)
- Creates `~/.xergon/config.toml`
- Probes for installed inference backends

### Step 3: Install an Inference Backend

Choose one:

#### Option A: Ollama (easiest)

```bash
curl -fsSL https://ollama.com/install.sh | sh
ollama pull llama3.1:8b
```

#### Option B: llama.cpp server

```bash
# Download a GGUF model from HuggingFace
wget https://huggingface.co/TheBloke/Llama-3.1-8B-Instruct-GGUF/resolve/main/llama-3.1-8b-instruct.Q4_K_M.gguf -O model.gguf

# Build llama.cpp (or download a release)
git clone https://github.com/ggerganov/llama.cpp && cd llama.cpp && make

# Start server
./llama-server -m model.gguf --port 8080 --ctx-size 4096
```

#### Option C: vLLM (for production, NVIDIA only)

```bash
pip install vllm
vllm serve meta-llama/Llama-3.1-8B-Instruct --port 8080
```

### Step 4: Configure Your Provider

Edit `~/.xergon/config.toml`:

```toml
[ergo_node]
rest_url = "http://127.0.0.1:9053"   # local Ergo node, or use a public node

[xergon]
provider_id = "my-provider-01"
provider_name = "My GPU Node"
region = "us-east"
ergo_address = "YOUR_ERGO_ADDRESS"

[inference]
enabled = true
url = "http://localhost:11434"        # Ollama default
timeout_secs = 120

[relay]
register_on_start = true
relay_url = "https://relay.xergon.network"
token = "YOUR_RELAY_TOKEN"            # get from relay.xergon.network
heartbeat_interval_secs = 60

[chain.tx]
heartbeat_tx_enabled = true
provider_nft_token_id = "YOUR_NFT_TOKEN_ID"

# Contract overrides (optional -- leave empty to use embedded)
[contracts]
provider_box_hex = ""
usage_proof_hex = ""
```

### Step 5: Start Serving

```bash
xergon-agent run
```

Your agent will:
1. Validate and load compiled contracts
2. Connect to the relay
3. Register on-chain (heartbeat transaction)
4. Start accepting inference requests
5. Earn ERG for each completed request

### Step 6: Monitor

```bash
# Check agent status
xergon-agent status

# Check backend health
curl http://localhost:11434/api/tags        # Ollama
curl http://localhost:8080/v1/models         # llama.cpp

# Check relay connection
curl https://relay.xergon.network/health
```

---

## GPU Rental (Optional)

If you want to rent your GPU for arbitrary compute tasks:

### Step 1: Enable GPU Rental

In `config.toml`:

```toml
[gpu_rental]
enabled = true
ergo_node_url = "http://127.0.0.1:9053"
ssh_enabled = true
ssh_username = "xergon"
```

### Step 2: List Your GPUs

```bash
# Via API
curl http://localhost:9099/xergon/gpu/list

# Or check directly
nvidia-smi   # NVIDIA
rocm-smi     # AMD
```

### Step 3: Create a Listing

```bash
curl -X POST http://localhost:9099/xergon/gpu/listing \
  -H "Content-Type: application/json" \
  -d '{
    "gpu_type": "RTX 4090",
    "vram_gb": 24,
    "price_per_hour_erg": 0.10,
    "region": "us-east",
    "max_rental_hours": 720
  }'
```

---

## Earning

### How Earnings Work

You earn ERG for every inference request served:

- **Base rate**: approximately 0.00005 ERG per 1K tokens
- **Rare model bonus**: up to 10x multiplier for exclusive/scarce models
- **PoNW score**: higher score = more requests routed to you by the relay
- **GPU rental**: set your own hourly rate (paid via on-chain escrow)

### PoNW Score

Proof-of-Node-Work (PoNW) measures your reliability and contribution:
- Uptime (continuous operation)
- Peer count (network participation)
- Inference throughput (requests handled)
- Model availability (rare models boost score)

Higher PoNW = more traffic routed to your node.

### Withdrawal

```bash
# Your earned ERG accumulates in your provider box on-chain
# Withdraw to your wallet via the Ergo explorer or your Ergo node
xergon-agent status   # shows current balance
```

---

## Troubleshooting

### "No inference backend found"

Make sure your backend is running:

```bash
curl http://localhost:11434/api/tags   # Ollama
curl http://localhost:8080/v1/models    # llama.cpp
```

Check your config:

```toml
[inference]
url = "http://localhost:11434"   # must match your backend
```

### "Relay connection failed"

```bash
# Check relay status
curl https://relay.xergon.network/health

# Verify your token
# Get a token from relay.xergon.network/register

# Check your config
[relay]
relay_url = "https://relay.xergon.network"
token = "your-valid-token"
register_on_start = true
```

### "Insufficient ERG for heartbeat"

```bash
# Get your address
xergon-agent status | grep "Ergo Address"

# Send ERG to this address from an exchange or another wallet
# Minimum: 0.1 ERG recommended for initial funding
```

### "Cannot connect to Ergo node"

```bash
# Check if your node is running
curl http://127.0.0.1:9053/info

# Or use a public node
[ergo_node]
rest_url = "https://ergo-node.example.com:9053"
```

### Agent not starting

```bash
# Check logs with debug level
RUST_LOG=xergon_agent=debug xergon-agent run

# Common issues:
# - Port 9099 already in use (change [api].listen_addr)
# - Config file syntax error (run: xergon-agent setup to regenerate)
# - Missing dependencies (run: xergon-agent setup)
```

---

## Security Best Practices

1. **Never share your mnemonic** -- stored encrypted in `~/.xergon/wallet.dat`
2. **Use a firewall** -- only expose port 9099 to trusted networks
3. **Set an API key** for management endpoints:
   ```toml
   [api]
   api_key = "your-random-api-key-here"
   ```
4. **Use HTTPS** for the relay URL in production
5. **Keep your agent updated**: `xergon-agent update`

---

## Next Steps

- Join the community: [GitHub Discussions](https://github.com/n1ur0/Xergon-Network/discussions)
- Read the architecture overview: `docs/architecture.md`
- Explore the contract system: `contracts/README.md`
- Check the roadmap: `ROADMAP.md`
