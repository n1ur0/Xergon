# Xergon Relay

![Rust](https://img.shields.io/badge/rust-1.85%2B-orange)
![License](https://img.shields.io/badge/license-MIT-blue)
![Platform](https://img.shields.io/badge/platform-linux%20%7C%20macOS-green)

Thin, stateless router that sits between frontend clients and Xergon agent
providers. Exposes an OpenAI-compatible API (`/v1/chat/completions`,
`/v1/models`), smart-routes requests to the best available provider based on
latency, PoNW score, and current load, and provides fallback chains, circuit
breakers, sticky sessions, and real-time provider health monitoring.

## Architecture

```
  Clients                     Relay                         Agents
  +------+                 +-------------+             +-----------+
  | Web  |--HTTP/SSE------>|             |---status--->| Agent A   |
  | App  |  /v1/chat/      |   xergon    |             | :9099     |
  | CLI  |  completions    |   relay     |<--proxy---->| (Ollama)  |
  +------+                 |             |             +-----------+
                            |  +-------+  |
                            |  |Smart  |  |             +-----------+
                            |  |Router |  |---status--->| Agent B   |
  +------+                 |  +-------+  |             | :9099     |
  | SDK  |--HTTP---------->|  |Circuit|  |<--proxy---->| (llama)   |
  +------+                 |  |Breaker|  |             +-----------+
                            |  +-------+  |
                            |  |Sticky |  |             +-----------+
                            |  |Session|  |---status--->| Agent C   |
                            |  +-------+  |<--fallback-->| :9099     |
                            |             |             +-----------+
                            |  +-------+  |
                            |  |Gossip |  |             +-----------+
                            |  |Consens|  |<--gossip--->| Relay B   |
                            |  +-------+  |             +-----------+
                            |  |Chain  |  |
                            |  |Scanner|  |---> Ergo Node
                            |  +-------+  |     :9053
                            +-------------+
```

## Features

- **OpenAI-compatible API** -- drop-in replacement for OpenAI endpoints
- **Smart routing** -- selects providers by latency + PoNW score + current load
- **Circuit breaker** -- Closed/Open/HalfOpen states prevent cascading failures
- **Sticky sessions** -- session affinity with configurable TTL (default 30 min)
- **Fallback chain** -- automatically retries on the next-best provider on failure
- **SSE streaming** -- server-sent events passthrough with token counting
- **Provider health polling** -- periodic `/xergon/status` checks with parallel polling
- **Ergo chain-state discovery** -- auto-discovers providers from on-chain Provider Boxes
- **Balance-based rate limiting** -- ERG staking balance determines rate tier
- **Signature-based auth** -- Ergo wallet signature verification with replay protection
- **Free tier** -- limited requests for new users before deposit required
- **Gossip consensus** -- multi-relay agreement on provider health (solo mode default)
- **Real-time events** -- SSE `/v1/events` and WebSocket `/ws/status` endpoints
- **Demand tracking** -- sliding-window per-model request counting
- **Oracle ERG/USD rate** -- price feed from on-chain oracle pool boxes
- **GPU Bazar** -- browse, rent, rate GPU listings from on-chain data
- **Cross-chain bridge** -- BTC/ETH/ADA Lock-and-Mint payment invoices
- **Incentive system** -- rarity bonuses for providers of scarce models

## Quick Start

### Prerequisites

- Rust 1.85+
- At least one xergon-agent instance running

```bash
# Build
cargo build --release

# Copy and edit configuration
cp config.toml.example config.toml
# Edit: set known_endpoints, chain config

# Run
./target/release/xergon-relay
```

## Configuration

Configuration is loaded from `config.toml` (or `XERGON_RELAY_CONFIG` env var).
Environment variables with `XERGON_RELAY__` prefix override file values.

### Core Settings

| Section        | Key                            | Default               | Description                        |
|----------------|--------------------------------|-----------------------|------------------------------------|
| `[relay]`      | `listen_addr`                  | `0.0.0.0:8080`        | Server bind address                |
|                | `cors_origins`                 | `*`                   | CORS origins (comma-separated)     |
|                | `health_poll_interval_secs`    | `30`                  | Provider health poll interval      |
|                | `provider_timeout_secs`        | `30`                  | Per-request timeout to providers   |
|                | `max_fallback_attempts`        | `3`                   | Fallback chain depth               |
|                | `circuit_failure_threshold`    | `5`                   | Failures before circuit opens      |
|                | `circuit_recovery_timeout_secs`| `30`                  | Open -> HalfOpen transition time   |
|                | `circuit_half_open_max_probes` | `2`                   | Probes allowed in HalfOpen state   |
|                | `sticky_session_ttl_secs`      | `1800` (30 min)       | Session affinity TTL               |
| `[providers]`  | `known_endpoints`              | `["http://127.0.0.1:9099"]` | Static agent endpoints     |

### Chain Discovery

| Section   | Key                      | Default                 | Description                          |
|-----------|--------------------------|-------------------------|--------------------------------------|
| `[chain]` | `enabled`                | `true`                  | Enable chain scanning                |
|           | `ergo_node_url`          | `http://127.0.0.1:9053` | Ergo node REST API URL               |
|           | `scan_interval_secs`     | `30`                    | Chain scan interval                  |
|           | `cache_ttl_secs`         | `10`                    | Cache freshness window               |
|           | `provider_tree_bytes`    | `""`                    | Provider Box ErgoTree hex            |
|           | `gpu_listing_tree_bytes` | `""`                    | GPU listing ErgoTree hex             |
|           | `gpu_rental_tree_bytes`  | `""`                    | GPU rental ErgoTree hex              |

### Balance & Auth

| Section      | Key                   | Default       | Description                         |
|--------------|-----------------------|---------------|-------------------------------------|
| `[balance]`  | `enabled`             | `true`        | Enable balance checking             |
|              | `min_balance_nanoerg` | `1_000_000`   | Min balance (0.001 ERG)             |
|              | `cache_ttl_secs`      | `30`          | Balance cache TTL                   |
|              | `free_tier_enabled`   | `true`        | Allow limited free requests         |
|              | `free_tier_requests`  | `10`          | Free requests allowed               |
|              | `staking_tree_bytes`  | `""`          | Staking box ErgoTree hex            |
| `[auth]`     | `enabled`             | `true`        | Enable signature auth               |
|              | `max_age_secs`        | `300` (5 min) | Max request age                     |
|              | `replay_cache_size`   | `10_000`      | Replay protection cache size        |
|              | `require_staking_box` | `false`       | Require on-chain staking for auth   |

### Rate Limiting

| Section          | Key        | Default | Description                   |
|------------------|------------|---------|-------------------------------|
| `[rate_limit]`   | `enabled`  | `true`  | Enable rate limiting          |
|                  | `ip_rpm`   | `30`    | Requests per minute per IP    |
|                  | `ip_burst` | `10`    | IP burst capacity             |
|                  | `key_rpm`  | `120`   | Requests per minute per key   |
|                  | `key_burst`| `30`    | Key burst capacity            |

### Oracle, Events, Gossip

| Section     | Key               | Default     | Description                     |
|-------------|-------------------|-------------|---------------------------------|
| `[oracle]`  | `pool_nft_token_id` | `None`    | Oracle pool NFT for ERG/USD     |
|             | `refresh_secs`    | `300` (5 min)| Oracle rate refresh interval |
| `[events]`  | `enabled`         | `true`      | Enable SSE events endpoint      |
|             | `max_subscribers` | `1000`      | Max concurrent SSE subscribers  |
| `[gossip]`  | `enabled`         | `false`     | Enable gossip protocol          |
|             | `peers`           | `[]`        | Peer relay URLs for gossip      |
|             | `interval_secs`   | `30`        | Gossip round interval           |
|             | `relay_id`        | (auto UUID) | Unique relay instance ID        |

### Free Tier & Incentive

| Section       | Key                    | Default | Description                    |
|---------------|------------------------|---------|--------------------------------|
| `[free_tier]` | `enabled`              | `true`  | Enable free tier tracking      |
|               | `max_requests`         | `100`   | Free requests per window       |
|               | `decay_hours`          | `24`    | Hours until counter resets     |
| `[incentive]` | `rarity_bonus_enabled` | `true`  | Enable rarity PoNW multiplier  |
|               | `rarity_max_multiplier`| `10.0`  | Cap for rarity bonus           |
|               | `rarity_min_providers` | `1`     | Providers for max multiplier   |

### Cross-chain Bridge

| Section    | Key                    | Default           | Description                  |
|------------|------------------------|-------------------|------------------------------|
| `[bridge]` | `enabled`              | `false`           | Enable payment bridge        |
|            | `bridge_public_key`    | `""`              | Operator public key (hex)    |
|            | `supported_chains`     | `[btc,eth,ada]`   | Supported foreign chains     |
|            | `invoice_timeout_blocks`| `720` (~24h)     | Invoice expiry (blocks)      |
|            | `invoice_tree_hex`     | `""`              | Bridge contract ErgoTree hex |

## API Endpoints

### OpenAI-Compatible

| Method | Path                    | Description                         |
|--------|-------------------------|-------------------------------------|
| POST   | `/v1/chat/completions`  | Chat completion (streaming supported)|
| GET    | `/v1/models`            | List available models across agents |

### Discovery & Leaderboard

| Method | Path                    | Description                          |
|--------|-------------------------|--------------------------------------|
| GET    | `/v1/leaderboard`       | Provider leaderboard ranked by PoNW  |
| GET    | `/v1/providers`         | Chain + in-memory merged provider list|
| GET    | `/v1/balance/{user_pk}` | User ERG staking balance             |

### GPU Bazar

| Method | Path                         | Description                    |
|--------|------------------------------|--------------------------------|
| GET    | `/v1/gpu/listings`           | Browse GPU listings             |
| GET    | `/v1/gpu/listings/{id}`      | Get specific listing            |
| POST   | `/v1/gpu/rent`               | Rent a GPU                      |
| GET    | `/v1/gpu/rentals/{pk}`       | User's active rentals           |
| GET    | `/v1/gpu/pricing`            | GPU pricing information         |
| POST   | `/v1/gpu/rate`               | Rate rental experience          |
| GET    | `/v1/gpu/reputation/{pk}`    | Provider reputation             |

### Auth & Incentive

| Method | Path                       | Description                    |
|--------|----------------------------|--------------------------------|
| GET    | `/v1/auth/status`          | Auth system status             |
| GET    | `/v1/incentive/status`     | Incentive system status        |
| GET    | `/v1/incentive/models`     | Models with rarity bonuses     |
| GET    | `/v1/incentive/models/{m}` | Detail for a model's rarity    |

### Cross-chain Bridge

| Method | Path                       | Description                    |
|--------|----------------------------|--------------------------------|
| GET    | `/v1/bridge/status`        | Bridge status                  |
| GET    | `/v1/bridge/invoices`      | List invoices                  |
| GET    | `/v1/bridge/invoice/{id}`  | Invoice status                 |
| POST   | `/v1/bridge/create-invoice`| Create payment invoice         |
| POST   | `/v1/bridge/confirm`       | Confirm cross-chain payment    |
| POST   | `/v1/bridge/refund`        | Refund expired invoice         |

### Real-time

| Method | Path          | Description                            |
|--------|---------------|----------------------------------------|
| GET    | `/v1/events`  | SSE stream of provider events          |
| GET    | `/ws/status`  | WebSocket real-time provider status    |

### Health & Gossip

| Method | Path                       | Description                    |
|--------|----------------------------|--------------------------------|
| GET    | `/health`                  | Liveness probe                 |
| GET    | `/ready`                   | Readiness (1+ healthy provider)|
| GET    | `/v1/health/detailed`      | Detailed system health         |
| POST   | `/gossip/ping`             | Gossip heartbeat               |
| POST   | `/gossip/push`             | Push provider health to peer   |
| GET    | `/gossip/status`           | Gossip consensus status        |

## Provider Routing Algorithm

When a request arrives, the relay selects the best provider in this order:

1. **Sticky session check** -- if the client has a `X-Session-Id` header with
   a valid cached session, route to the same provider (within TTL).

2. **Filter** -- remove providers that are:
   - Circuit breaker in `Open` state
   - Currently unhealthy (last health poll failed)
   - Not serving the requested model

3. **Score** each remaining provider:
   ```
   score = normalized_latency * 0.5
         + normalized_pown   * 0.3
         + normalized_load   * 0.2
   ```
   - Latency: measured during health polls (lower is better)
   - PoNW: work points from provider's `/xergon/status` (higher is better)
   - Load: current in-flight requests (lower is better)

4. **Select** the provider with the highest composite score.

5. **Fallback** -- on provider failure (timeout, 5xx, circuit open), try the
   next-best provider up to `max_fallback_attempts`.

6. **Rarity bonus** -- when incentive system is enabled, providers serving
   rare models (few providers) get a multiplier up to `rarity_max_multiplier`.

## Development

```bash
# Build (debug)
cargo build

# Build (release with LTO)
cargo build --release

# Run tests
cargo test

# Run with verbose logging
RUST_LOG=xergon_relay=debug,tower_http=debug cargo run

# Run with JSON structured logging
RUST_LOG=xergon_relay=info,json cargo run
```

## Docker

```bash
# Build
docker build -t xergon-relay .

# Run
docker run -d \
  --name xergon-relay \
  -p 9090:9090 \
  -v ./config.toml:/app/config.toml \
  -e XERGON_RELAY__CHAIN__ERGO_NODE_URL=http://host.docker.internal:9053 \
  xergon-relay
```

The Dockerfile uses a multi-stage build (Rust 1.85 on Debian Bookworm) with
dependency caching, runs as a non-root `xergon` user, and exposes port 9090.

## Environment Variables

| Variable                                  | Description                        |
|-------------------------------------------|------------------------------------|
| `XERGON_RELAY_CONFIG`                     | Config file path                   |
| `XERGON_RELAY__RELAY__LISTEN_ADDR`        | Override listen address            |
| `XERGON_RELAY__CHAIN__ERGO_NODE_URL`      | Override Ergo node URL             |
| `XERGON_RELAY__PROVIDERS__KNOWN_ENDPOINTS`| Override provider endpoints (JSON) |
| `XERGON_RELAY__RELAY__CORS_ORIGINS`       | Override CORS origins              |
| `XERGON_ENV`                              | Set to `development` to suppress CORS warnings |
| `RUST_LOG`                                | Log level filter                   |

## License

MIT
