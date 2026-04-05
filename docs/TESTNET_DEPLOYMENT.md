# Xergon Network -- Testnet Deployment Guide

Deploy the full Xergon Network stack on Ergo testnet: agent, relay, and marketplace.

---

## Architecture

```
  User (Nautilus Wallet)
       |
       v
  xergon-marketplace (Next.js :3000)
       |
       v
  xergon-relay (:9090)  ---GET /v1/providers--->  Ergo Node (:9053)
       |                         |
       v                         v
  xergon-agent (:9099)  ---chain scan--->  UTXO set (provider boxes, staking boxes)
       |
       v
  Ollama / llama.cpp (inference)
```

Three components:
- **xergon-agent** -- provider node: runs inference, registers on-chain, submits heartbeats
- **xergon-relay** -- routing layer: discovers providers, balances requests, checks staking
- **xergon-marketplace** -- web UI: connects Nautilus wallet, proxies inference through relay

---

## Prerequisites

| Tool | Version | Install |
|------|---------|---------|
| Rust | 1.75+ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Node.js | 20+ | `brew install node` or nvm |
| pnpm | 9+ | `npm install -g pnpm` |
| Nautilus Wallet | latest | [Chrome extension](https://nautiluswallet.com/) |
| jq | any | `brew install jq` |
| curl | any | usually pre-installed |

---

## 1. Get Testnet ERG

1. Install **Nautilus Wallet** browser extension
2. Switch Nautilus to **Testnet** mode:
   - Settings > Advanced > Toggle "Use Testnet"
3. Get testnet ERG from a faucet:
   - https://faucet.ergo-platform.com
   - https://t.me/ErgoTestnetFaucetBot (Telegram)
   - Or mine on testnet (any computer can mine testnet blocks)

You need at least 0.1 ERG for deployment (treasury box + provider box + fees).

---

## 2. Run an Ergo Testnet Node

You need a synced Ergo testnet node. Two options:

### Option A: Run your own (recommended for testing)

```bash
# Download latest release
wget https://github.com/ergoplatform/ergo/releases/latest/download/ergo-5.0.12.jar
java -jar ergo-5.0.12.jar --testnet -c /dev/null
```

Wait for "Node is synced" in the log. This takes 1-2 hours.

### Option B: Use a public testnet node

If someone is already running a testnet node, point your config at it:

```toml
[ergo_node]
rest_url = "http://THEIR_IP:9053"
```

Note: You need node wallet access for bootstrap (minting NFTs, creating boxes). If using someone else's node, they must unlock their wallet for you. For full control, run your own.

---

## 3. Clone and Build

```bash
git clone https://github.com/n1ur0/Xergon-Network.git
cd Xergon-Network

# Build everything
make all

# Or build individually:
make agent        # xergon-agent binary
make relay        # xergon-relay binary
make marketplace  # Next.js production build
```

Verify:

```bash
make check  # type-checks agent + relay + marketplace
make test   # runs unit tests
```

---

## 4. Compile Contracts

ErgoScript contracts must be compiled to ErgoTree hex before deployment.

```bash
# Requires a running Ergo testnet node
export ERGO_NODE_URL=http://127.0.0.1:9053
make compile-contracts
```

This compiles all `.es` files in `xergon-agent/contracts/` and writes hex to `contracts/compiled/`:

```
contracts/compiled/treasury_box.hex           # Treasury Box guard script
contracts/compiled/provider_box.hex           # Provider Box guard script
contracts/compiled/provider_registration.hex   # Provider registration guard
contracts/compiled/user_staking.hex           # User Staking Box guard script
contracts/compiled/usage_proof.hex            # Usage Proof Box guard script
```

Verify:

```bash
make validate-contracts
```

If compilation fails, check:
- Node is running and synced (`curl $ERGO_NODE_URL/info`)
- Node wallet is unlocked (`curl $ERGO_NODE_URL/wallet/status`)
- Contracts have valid ErgoScript syntax

---

## 5. Bootstrap the Network

The bootstrap mints the **Xergon Network NFT** (singleton token, supply=1) and creates the **Treasury Box** that holds the protocol's ERG reserve.

```bash
cd xergon-agent
./scripts/bootstrap.sh --node-url http://127.0.0.1:9053 --amount-erg 0.05
```

This will:
1. Compile `treasury_box.es` via the node
2. Create a transaction that mints the NFT (token ID = first input box ID) and creates the Treasury Box
3. Submit to the network
4. Print the **NFT token ID** and **Treasury Box ID**

**Save these values** -- you need them for all subsequent steps:

```
NFT Token ID:     abc123...  (32-byte hex)
Treasury Box ID:  def456...  (32-byte hex)
Treasury TX ID:   ghi789...  (32-byte hex)
```

The bootstrap is idempotent -- running it again detects the existing Treasury Box and skips.

---

## 6. Start a Provider (Agent)

### 6a. Generate a Provider Key

The agent needs a secp256k1 key pair for on-chain registration. Generate one:

```bash
# Option 1: Let the agent generate one on first run
# The agent creates keys in ~/.xergon/keys/ on first start

# Option 2: Use an existing key from Nautilus
# Export your public key from Nautilus (33-byte compressed hex)
```

### 6b. Register On-Chain

Create a Provider Box on-chain with your public key, endpoint, models, and region:

```bash
cd xergon-agent
./scripts/register_provider.sh \
  --node-url http://127.0.0.1:9053 \
  --provider-pk-hex 02abcdef0123456789... \
  --endpoint http://YOUR_PUBLIC_IP:9099 \
  --models '["llama-3.1-8b","mistral-7b"]' \
  --region us-east
```

This mints a per-provider NFT (unique to you) and creates a Provider Box with registers:
- R4: your public key (GroupElement)
- R5: endpoint URL
- R6: models served (JSON)
- R7: PoNW score (starts at 0)
- R8: last heartbeat height
- R9: region

Save the output:
```
Provider NFT ID:  xyz789...  (your unique provider token)
Provider Box ID:  uvw012...
Registration TX:  rst345...
```

### 6c. Configure the Agent

```bash
cp xergon-agent/config.toml.example xergon-agent/config.toml
```

Edit `xergon-agent/config.toml`:

```toml
[ergo_node]
rest_url = "http://127.0.0.1:9053"

[xergon]
provider_id = "my-provider"
provider_name = "My Xergon Node"
region = "us-east"
ergo_address = "9fDrt...your-ergo-address"

[api]
listen_addr = "0.0.0.0:9099"

[inference]
enabled = true
url = "http://127.0.0.1:11434"  # Ollama

[relay]
register_on_start = true
relay_url = "http://127.0.0.1:9090"
token = "your-registration-token"
heartbeat_interval_secs = 60
```

### 6d. Install a Model

```bash
# Pull a model with Ollama
ollama pull llama3.1:8b

# Verify it works
ollama run llama3.1:8b "Hello, world!"
```

### 6e. Start the Agent

```bash
cd xergon-agent
cargo run -- serve
# or use the release binary:
# ./target/release/xergon-agent serve
```

Verify:
```bash
curl http://127.0.0.1:9099/xergon/status
curl http://127.0.0.1:9099/v1/models
```

---

## 7. Start the Relay

### 7a. Configure

```bash
cp xergon-relay/config.toml.example xergon-relay/config.toml
```

Edit `xergon-relay/config.toml`:

```toml
[relay]
listen_addr = "0.0.0.0:9090"
cors_origins = "*"
health_poll_interval_secs = 30
provider_timeout_secs = 30
max_fallback_attempts = 3

[providers]
known_endpoints = [
    "http://127.0.0.1:9099",
]

# Enable chain scanning to discover providers from on-chain boxes
[chain]
enabled = true
ergo_node_url = "http://127.0.0.1:9053"
scan_interval_secs = 60
# Compiled provider_box contract hex (from Step 4)
provider_tree_bytes = "PLACEHOLDER"

# Balance checking (verify users have ERG staking boxes)
[balance]
enabled = true
ergo_node_url = "http://127.0.0.1:9053"
staking_tree_bytes = "PLACEHOLDER"
min_balance_nanoerg = 1000000  # 0.001 ERG minimum
```

### 7b. Start

```bash
cd xergon-relay
cargo run
# or: ./target/release/xergon-relay
```

Verify:
```bash
curl http://127.0.0.1:9090/v1/providers
curl http://127.0.0.1:9090/v1/models
curl http://127.0.0.1:9090/health
```

You should see your provider in the list.

---

## 8. Start the Marketplace

### 8a. Configure

```bash
cd xergon-marketplace
cp .env.example .env.local
```

Edit `.env.local`:

```bash
NEXT_PUBLIC_API_BASE=/api/v1
# RELAY_URL=http://127.0.0.1:9090   # optional, uses proxy by default
NEXT_PUBLIC_XERGON_AGENT_BASE=http://127.0.0.1:9099
```

### 8b. Start

```bash
pnpm install
pnpm dev
```

Open http://localhost:3000.

### 8c. Connect Wallet

1. Open Nautilus wallet (ensure testnet mode)
2. Click "Connect Wallet" on the marketplace
3. Approve the connection in Nautilus
4. Your ERG balance should appear in the navbar

---

## 9. Create a User Staking Box

To use inference through the relay, a user needs a staking box with ERG locked.

### Option A: Via Script

```bash
cd xergon-agent
./scripts/create_staking.sh \
  --node-url http://127.0.0.1:9053 \
  --user-pk-hex 02YOUR_PUBLIC_KEY_HEX_33_BYTES \
  --amount-erg 0.1
```

This creates a box guarded by `user_staking.es` with:
- R4: your public key (GroupElement)
- Value: 0.1 ERG (your inference balance)
- Rent period: 4 years (after which anyone can sweep the box)

### Option B: Via Agent API

```bash
curl -X POST http://127.0.0.1:9099/xergon/staking \
  -H "Content-Type: application/json" \
  -d '{
    "user_pk_hex": "02...",
    "amount_nanoerg": 100000000
  }'
```

### Verify

```bash
# Check balance via relay
curl http://127.0.0.1:9090/v1/balance/YOUR_PUBLIC_KEY_HEX

# Expected response:
# {
#   "user_pk": "02...",
#   "balance_nanoerg": 100000000,
#   "balance_erg": 0.1,
#   "staking_boxes_count": 1,
#   "sufficient": true
# }
```

---

## 10. Test the Full Flow

End-to-end test: marketplace -> relay -> agent -> model.

```bash
# 1. Verify providers are visible
curl http://127.0.0.1:9090/v1/providers

# 2. Run inference through relay
curl -X POST http://127.0.0.1:9090/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3.1:8b",
    "messages": [{"role": "user", "content": "Say hello in 5 words"}]
  }'

# 3. Or use the marketplace playground at http://localhost:3000
```

---

## Troubleshooting

### Node not syncing
```bash
# Check node status
curl http://127.0.0.1:9053/info | jq '.fullHeight, .headersHeight, .peers'

# If peers=0, add some:
curl -X POST http://127.0.0.1:9053/peers/add \
  -H "Content-Type: application/json" \
  -d '{"address": "213.239.193.138:9030"}'  # public testnet peer
```

### No testnet ERG
- Visit the faucet (see Step 1)
- Check Nautilus is in testnet mode (Settings > Advanced > "Use Testnet")
- Testnet ERG has no value -- you can request more freely

### Contract compilation fails
```bash
# Verify node is reachable
curl http://127.0.0.1:9053/info

# Verify node has the /script endpoint
curl http://127.0.0.1:9053/script/p2sAddress -X POST \
  -H "Content-Type: application/json" \
  -d '{"source": "{ sigmaProp(true) }"}'

# Check compiled hex files exist
ls xergon-agent/contracts/compiled/
```

### Provider not appearing in relay
```bash
# Check relay logs for chain scanning errors
# Check relay config has chain.enabled = true
# Verify provider box exists on chain:
curl http://127.0.0.1:9053/utxo/withTokenId/YOUR_PROVIDER_NFT_ID
```

### Wallet connection fails
- Ensure Nautilus is installed and unlocked
- Ensure Nautilus is in testnet mode
- Check browser console for EIP-12 errors
- Try a different browser or incognito mode

### Insufficient balance error on inference
```bash
# Create a staking box (Step 9)
# Check your balance:
curl http://127.0.0.1:9090/v1/balance/YOUR_PK_HEX
```

---

## Port Reference

| Service | Port | Purpose |
|---------|------|---------|
| xergon-relay | 9090 | API server (marketplace connects here) |
| xergon-agent | 9099 | Agent API (relay proxies here) |
| Ergo node | 9053 | REST API (chain queries, wallet ops) |
| Ollama | 11434 | Inference backend |
| llama-server | 8080 | Alternative inference backend |
| Marketplace | 3000 | Next.js dev server |

---

## Cleaning Up

### Stop all services
```bash
# Ctrl+C on each terminal, or:
pkill -f xergon-agent
pkill -f xergon-relay
```

### Reset state
```bash
# Remove agent data
rm -rf ~/.xergon/

# Remove config
rm xergon-agent/config.toml
rm xergon-relay/config.toml

# Re-compile contracts
make compile-contracts

# Re-bootstrap (creates new NFT and Treasury Box)
./xergon-agent/scripts/bootstrap.sh
```

### Full reset (nuke everything)
```bash
cd Xergon-Network
git clean -fdx
git checkout .
# Start over from Step 3
```

---

## Next Steps

After testnet deployment works:

1. **Add more providers** -- have friends run agents and register on-chain
2. **Monitor** -- check `xergon status` for PoNW scores, `/v1/leaderboard` for rankings
3. **Test inference flow** -- send requests through the marketplace playground
4. **GPU Bazar** -- list GPUs for rent via `xergon gpu list` and `xergon gpu rent`
5. **Cross-chain** -- set up payment bridge for BTC/ETH/ADA users

For questions, see the [ROADMAP.md](../ROADMAP.md) or the [Ergo Developer Knowledge Base](https://ergoplatform.org/docs).
