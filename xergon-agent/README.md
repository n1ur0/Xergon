# Xergon Agent

![Rust](https://img.shields.io/badge/rust-1.85%2B-orange)
![License](https://img.shields.io/badge/license-MIT-blue)
![Platform](https://img.shields.io/badge/platform-linux%20%7C%20macOS-green)

Rust sidecar for Ergo nodes providing AI inference, PoNW scoring, P2P peer
discovery, on-chain settlement, and GPU rental marketplace integration. Each
agent registers as a provider on the Xergon network and exposes an
OpenAI-compatible inference API proxied to a local LLM backend (Ollama,
llama.cpp, vLLM, etc.).

## Architecture

```
                          +--------------------+
                          |   Xergon Relay     |
                          |   (load balancer)  |
                          +--------+-----------+
                                   |
                    HTTP /v1/chat/completions
                                   |
                          +--------v-----------+
                          |   xergon-agent     |
                          |   (this binary)    |
                          |                    |
  +-------+  +-------+   |  +---------------+ |
  | Ollama|  |llama  |---+->| Inference     | |
  | :11434|  |.cpp   |   |  | Proxy         | |
  +-------+  | :8080 |   |  +-------+-------+ |
              +-------+   |          |         |
                          |  +-------v-------+ |
  +-------+               |  | Settlement    | |
  | Ergo |<------------->|  | Engine        | |
  | Node |  REST + P2P   |  +---------------+ |
  | :9053|               |  +-------+-------+ |
  +-------+               |  | Peer          | |
                          |  | Discovery     | |
                          |  +-------+-------+ |
                          |  +-------+-------+ |
                          |  | PoNW          | |
                          |  | Calculator    | |
                          |  +---------------+ |
                          +--------------------+
```

## Features

- **OpenAI-compatible inference proxy** -- proxied to Ollama, llama.cpp, or any
  OpenAI-compatible backend
- **Proof-of-Neural-Work (PoNW) scoring** -- rewards nodes for running inference
  and maintaining blockchain health
- **P2P peer discovery** -- scans Ergo P2P network to find other Xergon agents
- **ERG settlement engine** -- periodic on-chain or off-chain settlement of
  inference costs in nanoERG
- **Per-model pricing** -- advertise different prices per model on-chain
- **Provider on-chain registration** -- auto-register as a provider with a
  Provider Box on the Ergo blockchain
- **GPU rental marketplace** -- list, rent, extend, and rate GPU resources
- **Airdrop service** -- fund new users with ERG for onboarding
- **P2P provider communication** -- model exchange, load balancing, and
  cross-agent coordination
- **Automatic model pulling** -- fetch missing models from Ollama/HuggingFace
- **Usage proof rollups** -- Merkle tree epoch batching for gas-efficient
  on-chain proofs
- **Cross-chain payment bridge** -- accept BTC, ETH, ADA payments via
  Lock-and-Mint invoices
- **Oracle integration** -- ERG/USD price feed from oracle pool boxes
- **Self-update** -- check for new releases via GitHub Releases API
- **Contracts SDK API** -- TypeScript SDK-facing endpoints for on-chain
  operations (staking, settlement, governance)

## Quick Start

### Prerequisites

- Rust 1.85+
- An Ergo node running locally (port 9053)
- An LLM backend (Ollama recommended)

```bash
# Build
cargo build --release

# Copy and edit configuration
cp config.toml.example config.toml
# Edit config.toml: set provider_id, ergo_address, region

# Interactive first-run setup (detects models, sets up wallet)
./target/release/xergon-agent setup

# Run the agent
./target/release/xergon-agent run -c config.toml
```

### CLI Commands

| Command              | Description                                |
|----------------------|--------------------------------------------|
| `run`                | Start the agent (default)                  |
| `setup`              | Interactive first-run configuration wizard |
| `status`             | Query a running agent's status             |
| `update`             | Check for / apply updates                  |
| `provider set-price` | Set per-model pricing                      |
| `provider list-prices` | List current pricing                     |
| `provider remove-price` | Remove a per-model price override      |

## Configuration

Configuration is loaded from `config.toml` (or `XERGON_CONFIG` env var).
Environment variables with `XERGON__` prefix override file values
(e.g. `XERGON__API__API_KEY=secret`).

### Core Settings

| Section            | Key                             | Default                  | Description                              |
|--------------------|---------------------------------|--------------------------|------------------------------------------|
| `[xergon]`         | `provider_id`                   | (required)               | Unique provider identifier               |
|                    | `provider_name`                 | (required)               | Display name for leaderboards            |
|                    | `region`                        | (required)               | Region code (us-east, eu-west, etc.)     |
|                    | `ergo_address`                  | (required)               | Ergo address for PoNW identity           |
| `[ergo_node]`      | `rest_url`                      | `http://127.0.0.1:9053`  | Ergo node REST API URL                   |
| `[api]`            | `listen_addr`                   | `0.0.0.0:9099`           | Agent REST API bind address              |
|                    | `api_key`                       | `""` (open)              | Bearer token for /xergon/* endpoints     |

### Peer Discovery

| Section                | Key                       | Default | Description                        |
|------------------------|---------------------------|---------|------------------------------------|
| `[peer_discovery]`     | `discovery_interval_secs` | `120`   | Seconds between discovery cycles   |
|                        | `probe_timeout_secs`       | `5`     | Timeout per peer probe             |
|                        | `xergon_agent_port`        | `9099`  | Port agents expose status on       |
|                        | `max_concurrent_probes`    | `10`    | Concurrent probe limit             |
|                        | `peers_file`               | `None`  | Path to persist known peers        |

### Inference

| Section          | Key              | Default                      | Description                    |
|------------------|------------------|------------------------------|--------------------------------|
| `[inference]`    | `enabled`        | `true`                       | Enable inference proxy         |
|                  | `url`            | `http://127.0.0.1:11434`     | LLM backend URL                |
|                  | `timeout_secs`   | `120`                        | Request timeout                |
|                  | `api_key`        | `""`                         | Bearer token for inference     |
|                  | `served_models`  | `[]`                         | Models advertised as available |

### Settlement

| Section          | Key                            | Default         | Description                           |
|------------------|--------------------------------|-----------------|---------------------------------------|
| `[settlement]`   | `enabled`                      | `false`         | Enable settlement engine              |
|                  | `interval_secs`                | `86400`         | Settlement interval (seconds)         |
|                  | `dry_run`                      | `true`          | Log without sending transactions      |
|                  | `cost_per_1k_tokens_nanoerg`   | `1_000_000`     | Cost per 1K tokens (nanoERG)          |
|                  | `min_settlement_nanoerg`       | `1_000_000_000` | Minimum batch settlement (1 ERG)      |
|                  | `chain_enabled`                | `false`         | Use real eUTXO transactions           |

### Pricing

| Section      | Key                         | Default  | Description                         |
|--------------|-----------------------------|----------|-------------------------------------|
| `[pricing]`  | `default_price_per_1m_tokens` | `50000`  | nanoERG per 1M tokens               |
|              | `models`                    | `{}`     | Per-model price overrides (HashMap) |

### GPU Rental

| Section          | Key                            | Default          | Description                        |
|------------------|--------------------------------|------------------|------------------------------------|
| `[gpu_rental]`   | `enabled`                      | `false`          | Enable GPU rental endpoints        |
|                  | `ergo_node_url`                | (ergo_node url)  | Ergo node for GPU operations       |
|                  | `ssh_tunnel_port_range`        | `22000-22100`    | Port range for SSH tunnels         |
|                  | `max_rental_hours`             | `720`            | Max rental duration (30 days)      |
|                  | `ssh_enabled`                  | `true`           | Enable SSH tunnel management       |
|                  | `metering_check_interval_secs` | `60`             | Session expiry check interval      |
|                  | `ssh_username`                 | `xergon`         | SSH username for tunnels           |

### Additional Sections

| Section                | Description                                           |
|------------------------|-------------------------------------------------------|
| `[llama_server]`       | llama.cpp backend URL and health check interval       |
| `[relay]`              | Relay client registration (URL, token, heartbeat)    |
| `[chain]`              | On-chain heartbeat and usage proof transactions      |
| `[airdrop]`            | ERG airdrop service for new users                    |
| `[p2p]`                | Provider-to-provider communication                   |
| `[relay_discovery]`    | Multi-relay discovery via on-chain registry          |
| `[auto_model_pull]`    | Automatic model fetching from registries             |
| `[rollup]`             | Merkle tree usage proof batching                     |
| `[payment_bridge]`     | Cross-chain Lock-and-Mint (BTC, ETH, ADA)           |
| `[update]`             | Self-update via GitHub Releases                      |
| `[contracts]`          | ErgoTree hex overrides for all contracts             |
| `[oracle]`             | ERG/USD price feed from oracle pool box              |
| `[provider_registry]`  | On-chain provider auto-registration                  |
| `[storage_rent]`       | Storage rent monitoring for on-chain boxes           |

## API Endpoints

### Management (require `Authorization: Bearer <api_key>` when configured)

| Method | Path                          | Description                          |
|--------|-------------------------------|--------------------------------------|
| GET    | `/xergon/status`              | Provider identity + PoNW + health    |
| GET    | `/xergon/peers`               | Current peer discovery state         |
| GET    | `/xergon/health`              | Basic health check + uptime          |
| GET    | `/xergon/settlement`          | Settlement engine status/history     |
| POST   | `/api/settlement/execute`     | Trigger settlement execution         |
| GET    | `/api/settlement/boxes`       | List settleable staking boxes        |
| GET    | `/xergon/dashboard`           | Aggregated provider dashboard        |
| POST   | `/xergon/usage`               | Report inference usage               |
| GET    | `/xergon/pricing`             | Get current pricing                  |
| POST   | `/xergon/pricing`             | Update pricing                       |

### Inference (when enabled)

| Method | Path                      | Description                    |
|--------|---------------------------|--------------------------------|
| POST   | `/v1/chat/completions`    | OpenAI-compatible chat         |
| GET    | `/v1/models`              | List available models          |

### Airdrop & P2P

| Method | Path                         | Description                    |
|--------|------------------------------|--------------------------------|
| POST   | `/api/airdrop/request`       | Request ERG airdrop            |
| POST   | `/api/airdrop/eligibility`   | Check airdrop eligibility      |
| GET    | `/api/airdrop/stats`         | Airdrop statistics             |
| GET    | `/api/peer/info`             | Provider info for peers        |
| GET    | `/api/peer/models`           | Available models for peers     |
| POST   | `/api/peer/model-notify`     | Notify peers of new model      |
| POST   | `/api/peer/proxy-request`    | Proxy inference to peer        |

### GPU Bazar

| Method | Path                         | Description                  |
|--------|------------------------------|------------------------------|
| POST   | `/api/gpu/list`              | Create GPU listing           |
| POST   | `/api/gpu/rent`              | Rent a GPU                   |
| POST   | `/api/gpu/claim`             | Claim rental on-chain        |
| POST   | `/api/gpu/refund`            | Refund expired rental        |
| GET    | `/api/gpu/my-rentals`        | List user's active rentals   |
| POST   | `/api/gpu/extend`            | Extend rental duration       |
| GET    | `/api/gpu/sessions`          | Active rental sessions       |
| POST   | `/api/gpu/tunnel`            | Create SSH tunnel            |
| DELETE | `/api/gpu/tunnel/{id}`       | Close SSH tunnel             |
| POST   | `/api/gpu/rate`              | Rate rental experience       |
| GET    | `/api/gpu/reputation/{pk}`   | View provider reputation     |

### Monitoring (public)

| Method | Path                    | Description                  |
|--------|-------------------------|------------------------------|
| GET    | `/api/health`           | Liveness check               |
| GET    | `/api/metrics`          | Prometheus metrics           |
| GET    | `/api/oracle/rate`      | ERG/USD oracle rate          |

### Contracts SDK (`/v1/contracts/*`)

| Method | Path                                       | Description                  |
|--------|--------------------------------------------|------------------------------|
| POST   | `/v1/contracts/provider/register`           | Register provider on-chain   |
| GET    | `/v1/contracts/provider/status`             | Provider registration status |
| GET    | `/v1/contracts/providers`                   | List all providers           |
| POST   | `/v1/contracts/staking/create`              | Create staking box           |
| GET    | `/v1/contracts/staking/balance/{pk}`        | Staking balance              |
| POST   | `/v1/contracts/settlement/build`            | Build settlement transaction |
| GET    | `/v1/contracts/settlement/settleable`       | Settleable boxes             |
| GET    | `/v1/contracts/oracle/rate`                 | Oracle ERG/USD rate          |
| POST   | `/v1/contracts/governance/proposal`         | Create governance proposal   |
| POST   | `/v1/contracts/governance/vote`             | Vote on proposal             |
| GET    | `/v1/contracts/governance/proposals`        | List proposals               |


## Development

```bash
# Build (debug)
cargo build

# Build (release with LTO)
cargo build --release

# Run tests
cargo test

# Run with verbose logging
RUST_LOG=xergon_agent=debug cargo run -- run -c config.toml

# Run with JSON structured logging
RUST_LOG=xergon_agent=info,json cargo run -- run
```

### Feature Flags

| Flag         | Description                              |
|--------------|------------------------------------------|
| `ergo-lib`   | Enable ergo-lib for on-chain tx building |

## Docker

```bash
# Build
docker build -t xergon-agent .

# Run
docker run -d \
  --name xergon-agent \
  -p 9099:9099 \
  -v ./config.toml:/app/config.toml \
  -e XERGON__ERGO_NODE__REST_URL=http://host.docker.internal:9053 \
  xergon-agent run -c /app/config.toml
```

The Dockerfile uses a multi-stage build (Rust 1.85 on Debian Bookworm) with
dependency caching, runs as a non-root `xergon` user, and exposes port 9099.

## Environment Variables

All config values can be overridden via environment variables using the
`XERGON__` prefix with double-underscore separators:

| Variable                            | Equivalent Config Path        |
|-------------------------------------|-------------------------------|
| `XERGON__ERGO_NODE__REST_URL`       | `ergo_node.rest_url`          |
| `XERGON__API__LISTEN_ADDR`          | `api.listen_addr`             |
| `XERGON__API__API_KEY`              | `api.api_key`                 |
| `XERGON__XERGON__PROVIDER_ID`       | `xergon.provider_id`          |
| `XERGON__INFERENCE__ENABLED`        | `inference.enabled`           |
| `XERGON__INFERENCE__URL`            | `inference.url`               |
| `XERGON__SETTLEMENT__ENABLED`       | `settlement.enabled`          |
| `XERGON_CONFIG`                     | Config file path              |
| `RUST_LOG`                          | Log level filter              |

## License

MIT
