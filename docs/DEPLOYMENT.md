# Xergon Network -- Mainnet Deployment Guide

Production deployment guide for Xergon Network relay, agent, and monitoring stack.

---

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Architecture Overview](#architecture-overview)
3. [Deployment Methods](#deployment-methods)
   - [Binary Deployment](#31-binary-deployment)
   - [Docker Deployment](#32-docker-deployment)
   - [Docker Compose (Full Stack)](#33-docker-compose-full-stack)
4. [Configuration Reference](#configuration-reference)
   - [Relay Configuration](#41-relay-configuration)
   - [Agent Configuration](#42-agent-configuration)
5. [Security Hardening Checklist](#security-hardening-checklist)
6. [Monitoring Setup](#monitoring-setup)
7. [Database and State](#database-and-state)
8. [Scaling Guidance](#scaling-guidance)
9. [Health Check Endpoints](#health-check-endpoints)
10. [Systemd Service Files](#systemd-service-files)
11. [TLS Termination](#tls-termination)
    - [Nginx](#111-nginx)
    - [Caddy](#112-caddy)
12. [Port Reference](#port-reference)

---

## Prerequisites

| Requirement | Version | Notes |
|---|---|---|
| Rust toolchain | 1.83+ | Only needed for building from source |
| Ergo node | 5.0+ | Fully synced, wallet unlocked for agent |
| Docker + Compose | 20.10+ / v2+ | For monitoring stack and containerized deploys |
| OS | Ubuntu 22.04+ / Debian 12+ | Recommended; any Linux with glibc 2.31+ works |
| RAM | 4 GB minimum | Agent needs 2 GB+ for inference workloads |
| Disk | 20 GB minimum | Ergo node requires ~15 GB; binaries ~100 MB |
| Network | Public IP, ports open | See firewall rules in security section |

### Quick dependency install

```bash
# Rust (if building from source)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Docker (for monitoring)
curl -fsSL https://get.docker.com | sh
sudo usermod -aG docker $USER
newgrp docker

# Docker Compose plugin (usually included, verify)
docker compose version
```

---

## Architecture Overview

```
                        +-------------------+
                        |   Ergo Mainnet    |
                        |   Node (:9053)    |
                        +--------+----------+
                                 |
                +----------------+----------------+
                |                                 |
        +-------v--------+               +--------v-------+
        |  xergon-relay  |               |  xergon-agent  |
        |  (:8080/9090)  |---proxy------>|  (:9099)       |
        |  [stateless]   |               |  [on-chain     |
        +---+---+--------+               |   state]       |
            |   |                        +---+----+-------+
            |   |                            |    |
    +-------+   +--------+           +-------+    +-------+
    |                      |           |                   |
+---v---+            +-----v----+ +---v----+         +----v------+
| User  |            | Load     | | Ollama |         | llama.cpp |
| (Naut-|            | Balancer | | (:11434|         | (:8080)   |
| ilus  |            | (nginx)  | +--------+         +-----------+
| Wallet|            +----------+
+-------+
```

**Data flow:**

1. User connects via Nautilus wallet to the marketplace or relay API
2. Relay receives OpenAI-compatible requests (`POST /v1/chat/completions`)
3. Relay selects a healthy provider using latency + PoNW + load scoring
4. Request is proxied to the agent's inference endpoint
5. Agent proxies to local LLM backend (Ollama or llama.cpp)
6. Agent submits on-chain heartbeats and usage proofs to Ergo node

**Key design points:**

- **Relay is stateless** -- no database, no disk writes, safe to horizontally scale
- **Agent uses on-chain state** -- provider registration, staking, heartbeats are Ergo UTXOs
- **Ergo node is the source of truth** -- provider discovery, balance checks, settlement all read from chain

---

## Deployment Methods

### 3.1 Binary Deployment

Download pre-built binaries from GitHub Releases (recommended for production).

```bash
# Determine your architecture
ARCH=$(uname -m)
case $ARCH in
    x86_64)  PLATFORM="linux-amd64"  ;;
    aarch64) PLATFORM="linux-arm64"  ;;
    *)       echo "Unsupported: $ARCH"; exit 1 ;;
esac

# Download latest release
VERSION=$(curl -s https://api.github.com/repos/n1ur0/Xergon-Network/releases/latest | jq -r .tag_name)
cd /tmp
wget "https://github.com/n1ur0/Xergon-Network/releases/download/${VERSION}/xergon-${PLATFORM}.tar.gz"
tar xzf "xergon-${PLATFORM}.tar.gz"

# Install system-wide
sudo mkdir -p /opt/xergon/bin /opt/xergon/config /opt/xergon/data
sudo cp xergon-relay-${PLATFORM} /opt/xergon/bin/xergon-relay
sudo cp xergon-agent-${PLATFORM}  /opt/xergon/bin/xergon-agent
sudo chmod +x /opt/xergon/bin/xergon-*

# Verify
/opt/xergon/bin/xergon-relay --version
/opt/xergon/bin/xergon-agent --version
```

### 3.2 Docker Deployment

Images are published to GitHub Container Registry on every release.

```bash
# Pull images
docker pull ghcr.io/n1ur0/xergon-network/xergon-relay:latest
docker pull ghcr.io/n1ur0/xergon-network/xergon-agent:latest

# Or pin to a specific version
docker pull ghcr.io/n1ur0/xergon-network/xergon-relay:v0.1.0
docker pull ghcr.io/n1ur0/xergon-network/xergon-agent:v0.1.0

# Run relay
docker run -d \
  --name xergon-relay \
  --restart unless-stopped \
  -p 9090:9090 \
  -v /opt/xergon/config/relay.toml:/app/config.toml:ro \
  -e RUST_LOG=xergon_relay=info \
  -e XERGON_ENV=production \
  ghcr.io/n1ur0/xergon-network/xergon-relay:latest

# Run agent
docker run -d \
  --name xergon-agent \
  --restart unless-stopped \
  -p 9099:9099 \
  -v /opt/xergon/config/agent.toml:/app/config.toml:ro \
  -v /opt/xergon/data:/app/data \
  -e RUST_LOG=xergon_agent=info \
  -e XERGON_ENV=production \
  --gpus all \
  ghcr.io/n1ur0/xergon-network/xergon-agent:latest
```

### 3.3 Docker Compose (Full Stack)

The repository includes a root `docker-compose.yml` for the full application stack.

```bash
# Clone the repository
git clone https://github.com/n1ur0/Xergon-Network.git
cd Xergon-Network

# Copy and edit configs
cp xergon-relay/config.toml xergon-relay/config.toml.prod
cp xergon-agent/config.toml  xergon-agent/config.toml.prod
# Edit the .prod files with your mainnet settings

# Update docker-compose.yml to use prod configs
# Change volume mounts from config.toml to config.toml.prod

# Start everything
docker compose up -d

# Check status
docker compose ps
docker compose logs -f --tail=100
```

For monitoring, see [Monitoring Setup](#monitoring-setup).

---

## Configuration Reference

### 4.1 Relay Configuration

The relay loads config from `config.toml` (or path set by `XERGON_RELAY_CONFIG` env var).
Environment variables with `XERGON_RELAY__` prefix override file values.

**Example production `config.toml`:**

```toml
[relay]
listen_addr = "0.0.0.0:9090"
cors_origins = "https://xergon.ai,https://app.xergon.ai"
health_poll_interval_secs = 15
provider_timeout_secs = 30
max_fallback_attempts = 3

[providers]
known_endpoints = [
    "http://agent1.internal:9099",
    "http://agent2.internal:9099",
]

[chain]
enabled = true
ergo_node_url = "http://127.0.0.1:9053"
scan_interval_secs = 60
cache_ttl_secs = 30
provider_tree_bytes = "1008040004...your-compiled-provider-box-hex"
gpu_listing_tree_bytes = "1008040004...your-compiled-gpu-listing-hex"
gpu_rental_tree_bytes = ""
gpu_rating_tree_bytes = ""
agent_gpu_endpoint = "http://127.0.0.1:9099"

[balance]
enabled = true
ergo_node_url = "http://127.0.0.1:9053"
min_balance_nanoerg = 1000000    # 0.001 ERG
cache_ttl_secs = 30
free_tier_enabled = true
free_tier_requests = 10
staking_tree_bytes = "1008040004...your-compiled-staking-box-hex"

[auth]
enabled = true
max_age_secs = 300               # 5 minutes
replay_cache_size = 10000
require_staking_box = false

[incentive]
rarity_bonus_enabled = true
rarity_max_multiplier = 10.0
rarity_min_providers = 1

[bridge]
enabled = false
bridge_public_key = ""
supported_chains = ["btc", "eth", "ada"]
invoice_timeout_blocks = 720
invoice_tree_hex = ""

[rate_limit]
enabled = true
ip_rpm = 30
ip_burst = 10
key_rpm = 120
key_burst = 30
```

**Environment variable reference (prefix: `XERGON_RELAY__`):**

| Variable | Section | Default | Description |
|---|---|---|---|
| `XERGON_RELAY__RELAY__LISTEN_ADDR` | relay | `0.0.0.0:8080` | Bind address |
| `XERGON_RELAY__RELAY__CORS_ORIGINS` | relay | `*` | Comma-separated allowed origins |
| `XERGON_RELAY__RELAY__HEALTH_POLL_INTERVAL_SECS` | relay | `30` | Health check poll interval |
| `XERGON_RELAY__RELAY__PROVIDER_TIMEOUT_SECS` | relay | `30` | Provider request timeout |
| `XERGON_RELAY__RELAY__MAX_FALLBACK_ATTEMPTS` | relay | `3` | Max fallback on provider failure |
| `XERGON_RELAY__CHAIN__ENABLED` | chain | `true` | Enable chain scanning |
| `XERGON_RELAY__CHAIN__ERGO_NODE_URL` | chain | `http://127.0.0.1:9053` | Ergo node REST URL |
| `XERGON_RELAY__CHAIN__SCAN_INTERVAL_SECS` | chain | `30` | Chain scan interval |
| `XERGON_RELAY__CHAIN__CACHE_TTL_SECS` | chain | `10` | Cache freshness window |
| `XERGON_RELAY__CHAIN__PROVIDER_TREE_BYTES` | chain | `""` | Provider Box ErgoTree hex |
| `XERGON_RELAY__CHAIN__GPU_LISTING_TREE_BYTES` | chain | `""` | GPU Listing Box ErgoTree hex |
| `XERGON_RELAY__CHAIN__GPU_RENTAL_TREE_BYTES` | chain | `""` | GPU Rental Box ErgoTree hex |
| `XERGON_RELAY__CHAIN__AGENT_GPU_ENDPOINT` | chain | `""` | Agent GPU API base URL |
| `XERGON_RELAY__BALANCE__ENABLED` | balance | `true` | Enable balance checking |
| `XERGON_RELAY__BALANCE__MIN_BALANCE_NANOERG` | balance | `1000000` | Min balance (nanoERG) |
| `XERGON_RELAY__BALANCE__FREE_TIER_ENABLED` | balance | `true` | Allow free tier requests |
| `XERGON_RELAY__BALANCE__FREE_TIER_REQUESTS` | balance | `10` | Free requests per user |
| `XERGON_RELAY__BALANCE__STAKING_TREE_BYTES` | balance | `""` | Staking Box ErgoTree hex |
| `XERGON_RELAY__AUTH__ENABLED` | auth | `true` | Enable signature auth |
| `XERGON_RELAY__AUTH__MAX_AGE_SECS` | auth | `300` | Max request age (seconds) |
| `XERGON_RELAY__AUTH__REPLAY_CACHE_SIZE` | auth | `10000` | Replay protection cache size |
| `XERGON_RELAY__RATE_LIMIT__ENABLED` | rate_limit | `true` | Enable rate limiting |
| `XERGON_RELAY__RATE_LIMIT__IP_RPM` | rate_limit | `30` | Requests/min per IP |
| `XERGON_RELAY__RATE_LIMIT__IP_BURST` | rate_limit | `10` | Burst per IP |
| `XERGON_RELAY__RATE_LIMIT__KEY_RPM` | rate_limit | `120` | Requests/min per API key |
| `XERGON_RELAY__RATE_LIMIT__KEY_BURST` | rate_limit | `30` | Burst per API key |
| `XERGON_ENV` | global | (none) | Set `production` for security warnings |

### 4.2 Agent Configuration

The agent loads config from `config.toml` (or path set by `XERGON_CONFIG` env var).
Environment variables with `XERGON__` prefix override file values.

**Example production `config.toml`:**

```toml
[ergo_node]
rest_url = "http://127.0.0.1:9053"

[xergon]
provider_id = "Xergon_Mainnet_01"
provider_name = "Xergon Mainnet Node 1"
region = "us-east"
ergo_address = "9fDrtPahmtQDAPbq9AccibtZVmyPD8xmNJkrNXBbFDkejkez1kM"

[peer_discovery]
discovery_interval_secs = 300
probe_timeout_secs = 5
xergon_agent_port = 9099
max_concurrent_probes = 10
max_peers_per_cycle = 50
peers_file = "/opt/xergon/data/xergon-peers.json"

[api]
listen_addr = "0.0.0.0:9099"
api_key = "change-me-to-a-secure-random-string"

[settlement]
enabled = true
interval_secs = 86400
dry_run = false
ledger_file = "/opt/xergon/data/settlement_ledger.json"
min_settlement_usd = 0.10

[llama_server]
url = "http://127.0.0.1:8080"
health_check_interval_secs = 60

[inference]
enabled = true
url = "http://127.0.0.1:11434"
timeout_secs = 120
api_key = ""

[relay]
register_on_start = true
relay_url = "http://127.0.0.1:9090"
token = "your-relay-registration-token"
heartbeat_interval_secs = 60

[chain]
heartbeat_tx_enabled = true
usage_proof_tx_enabled = true
usage_proof_batch_interval_secs = 30
provider_nft_token_id = "abc123...your-provider-nft-token-id"
usage_proof_tree_hex = "1008040004...your-compiled-usage-proof-hex"

[gpu_rental]
enabled = false
ergo_node_url = "http://127.0.0.1:9053"
ssh_tunnel_port_range = "22000-22100"
max_rental_hours = 720
ssh_enabled = true
metering_check_interval_secs = 60
ssh_username = "xergon"

[auto_model_pull]
enabled = true
pull_timeout_secs = 600
max_concurrent_pulls = 2
pre_pull_models = ["llama3.1:8b"]

[payment_bridge]
enabled = false

[update]
release_url = "https://api.github.com/repos/n1ur0/Xergon-Network/releases/latest"
auto_check = false
check_interval_hours = 24

[contracts]
provider_box_hex = ""
provider_registration_hex = ""
treasury_box_hex = ""
usage_proof_hex = ""
user_staking_hex = ""
```

**Environment variable reference (prefix: `XERGON__`):**

| Variable | Section | Default | Description |
|---|---|---|---|
| `XERGON__ERGO_NODE__REST_URL` | ergo_node | `http://127.0.0.1:9053` | Ergo node REST URL |
| `XERGON__XERGON__PROVIDER_ID` | xergon | (required) | Unique provider identifier |
| `XERGON__XERGON__PROVIDER_NAME` | xergon | (required) | Display name |
| `XERGON__XERGON__REGION` | xergon | (required) | Provider region |
| `XERGON__XERGON__ERGO_ADDRESS` | xergon | (required) | Ergo P2S address |
| `XERGON__API__LISTEN_ADDR` | api | `0.0.0.0:9099` | Agent API bind address |
| `XERGON__API__API_KEY` | api | `""` | Management API key |
| `XERGON__INFERENCE__ENABLED` | inference | `true` | Enable inference proxy |
| `XERGON__INFERENCE__URL` | inference | `http://127.0.0.1:11434` | LLM backend URL |
| `XERGON__INFERENCE__TIMEOUT_SECS` | inference | `120` | Request timeout |
| `XERGON__INFERENCE__API_KEY` | inference | `""` | Inference auth key |
| `XERGON__SETTLEMENT__ENABLED` | settlement | `false` | Enable ERG settlement |
| `XERGON__SETTLEMENT__DRY_RUN` | settlement | `true` | Dry run mode |
| `XERGON__RELAY__REGISTER_ON_START` | relay | `true` | Auto-register with relay |
| `XERGON__RELAY__RELAY_URL` | relay | (none) | Relay URL for registration |
| `XERGON__RELAY__TOKEN` | relay | (none) | Registration auth token |
| `XERGON__CHAIN__HEARTBEAT_TX_ENABLED` | chain | `false` | On-chain heartbeat txs |
| `XERGON__CHAIN__USAGE_PROOF_TX_ENABLED` | chain | `false` | On-chain usage proof txs |
| `XERGON__GPU_RENTAL__ENABLED` | gpu_rental | `false` | Enable GPU rental endpoints |
| `XERGON__GPU_RENTAL__ERGO_NODE_URL` | gpu_rental | `http://127.0.0.1:9053` | Node for GPU rental ops |
| `XERGON__AUTO_MODEL_PULL__ENABLED` | auto_model_pull | `false` | Auto-pull missing models |
| `XERGON_CONFIG` | global | `config.toml` | Config file path |

---

## Security Hardening Checklist

Run through this checklist before exposing any service to the public internet.

### 1. Restrict CORS origins

```toml
# WRONG (development only)
cors_origins = "*"

# CORRECT (production)
cors_origins = "https://xergon.ai,https://app.xergon.ai"
```

### 2. Enable signature-based authentication

```toml
[auth]
enabled = true
max_age_secs = 300
replay_cache_size = 10000
require_staking_box = false  # set true for stricter enforcement
```

### 3. Set production environment

```bash
# Required for production -- enables security warnings
export XERGON_ENV=production
```

Without this, the relay will not warn about insecure defaults like wildcard CORS.

### 4. Configure rate limiting

```toml
[rate_limit]
enabled = true
ip_rpm = 30        # 30 requests/min per IP
ip_burst = 10      # allow burst of 10
key_rpm = 120      # 120 requests/min per API key
key_burst = 30     # allow burst of 30
```

### 5. TLS termination

Never expose relay or agent ports directly to the internet. Use a reverse proxy with TLS.

See [TLS Termination](#tls-termination) for nginx and Caddy examples.

### 6. Firewall rules

```bash
# Only allow these ports from the internet
sudo ufw allow 443/tcp    # HTTPS (reverse proxy)
sudo ufw allow 80/tcp     # HTTP (redirect to HTTPS)

# Ergo node P2P (only from known peers if possible)
sudo ufw allow from <TRUSTED_PEERS> to any port 9030/tcp

# Internal only -- block from external
# Relay: 9090
# Agent:  9099
# Ergo REST: 9053
# Ollama: 11434

sudo ufw enable
```

### 7. Agent API key

```toml
[api]
api_key = "$(openssl rand -hex 32)"
```

### 8. File permissions

```bash
# Config files should be readable only by the xergon user
sudo chmod 640 /opt/xergon/config/*.toml
sudo chown xergon:xergon /opt/xergon/config/*.toml

# Data directory
sudo chmod 750 /opt/xergon/data
sudo chown xergon:xergon /opt/xergon/data
```

### 9. Disable free tier (optional)

```toml
[balance]
free_tier_enabled = false
```

---

## Monitoring Setup

The monitoring stack runs Prometheus and Grafana via Docker Compose.

```bash
cd /opt/xergon/monitoring

# Copy monitoring config from the repository
cp -r /path/to/Xergon-Network/monitoring/* .

# Edit prometheus.yml to point to your actual hosts
# The defaults use host.docker.internal for Docker-on-host scraping

# IMPORTANT: If relay/agent run on the same host, adjust ports:
#   relay metrics:    host.docker.internal:9090  (or your actual port)
#   agent metrics:    host.docker.internal:9099  (or your actual port)
#
# If they run on different hosts, replace host.docker.internal with their IPs

# Start monitoring stack
docker compose up -d

# Verify
curl http://localhost:9090/api/v1/targets  | jq '.data.activeTargets[].health'
# Should show "up" for xergon-agent and xergon-relay

# Grafana
# Open http://localhost:3000
# Login: admin / xergon_admin  (CHANGE THIS IN PRODUCTION)
```

**Production Grafana password:**

```yaml
# monitoring/docker-compose.yml
environment:
  GF_SECURITY_ADMIN_PASSWORD: your-secure-grafana-password
```

**Prometheus targets configuration** (monitoring/prometheus.yml):

```yaml
scrape_configs:
  - job_name: "xergon-agent"
    metrics_path: "/api/metrics"
    static_configs:
      - targets: ["host.docker.internal:9099"]
    scrape_interval: 10s

  - job_name: "xergon-relay"
    metrics_path: "/v1/metrics"
    static_configs:
      - targets: ["host.docker.internal:9090"]
    scrape_interval: 10s
```

### Alert rules

Alert rules are defined in `monitoring/alerts.yml`. See `docs/RUNBOOK.md` for response playbooks.

| Alert | Severity | Threshold |
|---|---|---|
| AgentDown | critical | Unreachable for 1 min |
| RelayDown | critical | Unreachable for 1 min |
| NoActiveProviders | critical | 0 healthy providers for 5 min |
| WalletBalanceLow | warning | < 0.1 ERG for 10 min |
| NodeDesync | warning | Chain height = 0 for 5 min |
| HighErrorRate | warning | > 50% inference errors for 5 min |
| RelayHighErrorRate | warning | > 30% relay errors for 5 min |
| NoNodePeers | warning | < 3 Ergo node peers for 10 min |
| HighLatency | warning | > 30s inference latency for 5 min |

---

## Database and State

### Relay (stateless)

The relay maintains **no persistent state**. All data is in-memory:

- **Provider registry**: Populated from chain scans and health polls. Rebuilt on restart.
- **Chain cache**: Provider boxes, GPU listings. Refreshed every `scan_interval_secs`.
- **Rate limiter**: In-memory token bucket. Reset on restart (acceptable).
- **Replay cache**: In-memory nonce tracking for auth. Reset on restart.

This means relay instances are interchangeable. You can restart, scale, or replace them at any time.

### Agent (on-chain state)

The agent's canonical state lives on the Ergo blockchain:

- **Provider Box**: Your registration (public key, endpoint, models, region, PoNW score)
- **Staking Boxes**: User balances locked in `user_staking` contract boxes
- **Usage Proofs**: On-chain proof of inference work (optional, controlled by `chain.usage_proof_tx_enabled`)
- **Heartbeat**: Last-seen height updated via on-chain transaction (optional)

Local files used by the agent:

| Path | Purpose |
|---|---|
| `~/.xergon/keys/` | Provider key pair (secp256k1) |
| `/opt/xergon/data/settlement_ledger.json` | Settlement tracking |
| `/opt/xergon/data/xergon-peers.json` | Discovered Xergon peer list |

---

## Scaling Guidance

### Horizontal relay scaling

Since the relay is stateless, you can run multiple instances behind a load balancer.

```
                  +------------------+
                  |  Load Balancer   |
                  |  (nginx/HAProxy) |
                  +--------+---------+
                           |
              +------------+------------+
              |            |            |
        +-----v---+  +----v-----+ +---v------+
        | relay-1 |  | relay-2  | | relay-3  |
        | :9090   |  | :9090    | | :9090    |
        +---------+  +----------+ +----------+
              \            |            /
               \           |           /
                +----------v----------+
                |   Ergo Node :9053  |
                +-------------------+
```

**Steps:**

1. Deploy 2+ relay instances on separate hosts (or separate ports on the same host)
2. Point all instances at the same Ergo node
3. Place a load balancer (nginx, HAProxy, or cloud LB) in front
4. Use `/ready` endpoint for health checks (returns 200 only if >= 1 healthy provider)
5. Use sticky sessions if you need SSE streaming affinity

**Nginx load balancer config:**

```nginx
upstream xergon_relays {
    least_conn;
    server 10.0.1.10:9090 max_fails=3 fail_timeout=30s;
    server 10.0.1.11:9090 max_fails=3 fail_timeout=30s;
    server 10.0.1.12:9090 max_fails=3 fail_timeout=30s;
}

server {
    listen 443 ssl http2;
    server_name relay.xergon.ai;

    # TLS config (see TLS section below)
    ssl_certificate     /etc/ssl/xergon/fullchain.pem;
    ssl_certificate_key /etc/ssl/xergon/privkey.pem;

    location / {
        proxy_pass http://xergon_relays;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # SSE streaming support
        proxy_buffering off;
        proxy_cache off;
        proxy_read_timeout 300s;
    }

    location /health {
        proxy_pass http://xergon_relays;
    }
    location /ready {
        proxy_pass http://xergon_relays;
    }
}
```

### Agent scaling

Agents are stateful (bound to a specific Ergo identity and GPU). To scale:

- Run multiple agents with different provider identities
- Each agent registers its own Provider Box on-chain
- The relay automatically discovers new providers via chain scanning
- No load balancer needed; the relay handles routing

---

## Health Check Endpoints

### Relay endpoints

| Endpoint | Method | Purpose | Response |
|---|---|---|---|
| `/health` | GET | Liveness -- process is running | `"ok"` (plain text) |
| `/ready` | GET | Readiness -- at least 1 healthy provider | `200 OK` or `503` |
| `/v1/health` | GET | Detailed health (JSON) | Status, version, uptime, providers |
| `/v1/metrics` | GET | Prometheus metrics | Text exposition format |
| `/v1/providers` | GET | List all known providers | JSON |
| `/v1/models` | GET | List available models | JSON |

**Example `/v1/health` response:**

```json
{
  "status": "ok",
  "version": "0.1.0",
  "uptime_secs": 92520,
  "ergo_node_connected": true,
  "active_providers": 15,
  "total_providers": 23
}
```

### Agent endpoints

| Endpoint | Method | Purpose | Response |
|---|---|---|---|
| `/xergon/health` | GET | Basic liveness | JSON with provider_id |
| `/api/health` | GET | Enhanced health for monitoring | JSON with version, uptime, models |
| `/api/metrics` | GET | Prometheus metrics | Text exposition format |
| `/xergon/status` | GET | Full status (PoNW, peers, node) | JSON |
| `/xergon/peers` | GET | Peer discovery state | JSON |

**Example `/api/health` response:**

```json
{
  "status": "ok",
  "version": "0.1.0",
  "uptime_secs": 92520,
  "ergo_node_connected": true,
  "inference_backend": "ollama",
  "models_loaded": ["llama3.1:8b", "mistral:7b"]
}
```

---

## Systemd Service Files

### Relay service

Create `/etc/systemd/system/xergon-relay.service`:

```ini
[Unit]
Description=Xergon Relay - Stateless inference router
After=network.target
Wants=ergo-node.service

[Service]
Type=simple
User=xergon
Group=xergon
WorkingDirectory=/opt/xergon
ExecStart=/opt/xergon/bin/xergon-relay
Environment=XERGON_ENV=production
Environment=RUST_LOG=xergon_relay=info,tower_http=debug
Environment=XERGON_RELAY_CONFIG=/opt/xergon/config/relay.toml
Restart=always
RestartSec=5
LimitNOFILE=65535
StandardOutput=journal
StandardError=journal
SyslogIdentifier=xergon-relay

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/opt/xergon/data
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

### Agent service

Create `/etc/systemd/system/xergon-agent.service`:

```ini
[Unit]
Description=Xergon Agent - Inference provider node
After=network.target ergo-node.service
Wants=ergo-node.service

[Service]
Type=simple
User=xergon
Group=xergon
WorkingDirectory=/opt/xergon
ExecStart=/opt/xergon/bin/xergon-agent serve
Environment=XERGON_ENV=production
Environment=RUST_LOG=xergon_agent=info
Environment=XERGON_CONFIG=/opt/xergon/config/agent.toml
Restart=always
RestartSec=5
LimitNOFILE=65535
StandardOutput=journal
StandardError=journal
SyslogIdentifier=xergon-agent

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/opt/xergon/data
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

### Enable and manage services

```bash
# Create xergon user
sudo useradd -r -s /bin/false -d /opt/xergon xergon

# Reload systemd
sudo systemctl daemon-reload

# Enable services (start on boot)
sudo systemctl enable xergon-relay xergon-agent

# Start services
sudo systemctl start xergon-relay
sudo systemctl start xergon-agent

# Check status
sudo systemctl status xergon-relay
sudo systemctl status xergon-agent

# View logs
sudo journalctl -u xergon-relay -f
sudo journalctl -u xergon-agent -f

# Restart
sudo systemctl restart xergon-relay
sudo systemctl restart xergon-agent
```

---

## TLS Termination

### 11.1 Nginx

Create `/etc/nginx/sites-available/xergon-relay`:

```nginx
# HTTP -> HTTPS redirect
server {
    listen 80;
    listen [::]:80;
    server_name relay.xergon.ai;
    return 301 https://$host$request_uri;
}

# HTTPS relay proxy
server {
    listen 443 ssl http2;
    listen [::]:443 ssl http2;
    server_name relay.xergon.ai;

    # TLS certificates (use Let's Encrypt / certbot)
    ssl_certificate     /etc/letsencrypt/live/relay.xergon.ai/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/relay.xergon.ai/privkey.pem;

    # TLS hardening
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384;
    ssl_prefer_server_ciphers off;
    ssl_session_cache shared:SSL:10m;
    ssl_session_timeout 1d;
    ssl_stapling on;
    ssl_stapling_verify on;

    # Security headers
    add_header Strict-Transport-Security "max-age=63072000; includeSubDomains" always;
    add_header X-Content-Type-Options nosniff always;
    add_header X-Frame-Options DENY always;
    add_header X-XSS-Protection "1; mode=block" always;

    # Proxy to relay
    location / {
        proxy_pass http://127.0.0.1:9090;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # SSE streaming
        proxy_buffering off;
        proxy_cache off;
        proxy_read_timeout 300s;
        proxy_set_header Connection "";
    }
}

# Obtain certificate
sudo certbot --nginx -d relay.xergon.ai
```

### 11.2 Caddy

Create a `Caddyfile`:

```
relay.xergon.ai {
    reverse_proxy localhost:9090

    # SSE streaming support
    flush_interval -1

    # Security headers
    header {
        Strict-Transport-Security "max-age=63072000; includeSubDomains"
        X-Content-Type-Options "nosniff"
        X-Frame-Options "DENY"
        -Server
    }

    # Log access
    log {
        output file /var/log/caddy/xergon-relay.log
        format json
    }
}
```

Start Caddy:

```bash
sudo caddy run --config /etc/caddy/Caddyfile
# or with systemd:
sudo systemctl start caddy
```

Caddy automatically provisions and renews TLS certificates via Let's Encrypt.

---

## Port Reference

| Service | Default Port | Purpose | Public? |
|---|---|---|---|
| xergon-relay | 9090 | API server (marketplace / users connect here) | No (behind proxy) |
| xergon-agent | 9099 | Agent API (relay proxies here) | No (internal only) |
| Ergo node REST | 9053 | Chain queries, wallet operations | No (internal only) |
| Ergo node P2P | 9030 | Peer-to-peer network | Maybe (firewall restrict) |
| Ollama | 11434 | Inference backend | No (internal only) |
| llama.cpp server | 8080 | Alternative inference backend | No (internal only) |
| Prometheus | 9090 | Metrics scraper | No (internal only) |
| Grafana | 3000 | Dashboard UI | Maybe (behind auth) |
| Nginx / Caddy | 80, 443 | TLS termination | Yes |
