# Xergon Network

A decentralized Proof-of-Node-Work (PoNW) network for AI compute on [Ergo](https://ergoplatform.org).
Turns every healthy Ergo node into a local, private, censorship-resistant AI compute provider.

Powered by [Degens World](https://degens.world)

---

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Ergo Node  │     │ Xergon Agent│     │ Xergon Relay│
│  (9053)     │◄────│  (9099)     │────►│  (9090)     │
│             │     │ Rust sidecar│     │ Rust backend│
└─────────────┘     │ PoNW scoring│     │ Auth/Staking│
                    │ Peer discov.│     │ Provider mgmt│
                    │ Settlement  │     │ Rate limiting│
                    └──────┬──────┘     └──────┬──────┘
                           │                   │
                    ┌──────┴──────┐     ┌──────┴──────┐
                    │ Local LLM  │     │  Marketplace │
                    │ Ollama     │     │  Next.js     │
                    │ llama.cpp  │     │  (3000)      │
                    └─────────────┘     └─────────────┘
```

| Component          | Language   | Port | Purpose                                    |
|--------------------|------------|------|--------------------------------------------|
| xergon-agent       | Rust       | 9099 | PoNW scoring, peer discovery, inference proxy, ERG settlement |
| xergon-relay       | Rust       | 9090 | Signature auth, ERG staking, provider routing |
| xergon-marketplace | TypeScript | 3000 | Next.js 15 frontend — playground, models, pricing, provider dashboard |

---

## Quick Start (Local Dev)

### Prerequisites
- [Rust](https://rustup.rs/) 1.85+
- [Node.js](https://nodejs.org/) 20+
- [Ollama](https://ollama.com/) or [llama.cpp server](https://github.com/ggerganov/llama.cpp/tree/master/server) (optional, for AI inference)
- [Ergo node](https://docs.ergoplatform.com/node/) (optional, for PoNW features)

### 1. Agent

```bash
cd xergon-agent
cp config.toml.example config.toml  # edit provider_id, ergo_address, etc.
cargo run --release
```

### 2. Relay

```bash
cd xergon-relay
cp config.toml.example config.toml  # edit registration_token, etc.
cargo run --release
```

### 3. Marketplace

```bash
cd xergon-marketplace
cp .env.example .env.local
npm install
npm run dev
```

Open http://localhost:3000

---

## Docker (All-in-One)

```bash
# Copy config files
cp xergon-agent/config.toml.example xergon-agent/config.toml
cp xergon-relay/config.toml.example xergon-relay/config.toml

# Edit configs — at minimum change registration_token in relay config

# Start everything
docker compose up --build

# Open http://localhost:3000
```

---

## Configuration

### Agent (`xergon-agent/config.toml`)
| Section            | Key                        | Default                  | Description                         |
|--------------------|----------------------------|--------------------------|-------------------------------------|
| `[ergo_node]`      | `rest_url`                 | `http://127.0.0.1:9053`  | Ergo node REST API                  |
| `[xergon]`         | `provider_id`              | —                        | Unique provider identifier          |
| `[xergon]`         | `ergo_address`             | —                        | Ergo address for PoNW identity      |
| `[peer_discovery]` | `discovery_interval_secs`  | `120`                    | Peer scan interval                  |
| `[api]`            | `listen_addr`              | `0.0.0.0:9099`          | Agent API bind address              |
| `[inference]`      | `enabled`                  | `true`                   | Enable OpenAI-compatible proxy      |
| `[inference]`      | `url`                      | `http://127.0.0.1:11434` | LLM backend URL (Ollama default)    |
| `[settlement]`     | `enabled`                  | `false`                  | ERG settlement engine               |
| `[relay]`          | `register_on_start`        | `false`                  | Auto-register with relay            |

### Relay (`xergon-relay/config.toml`)
| Section       | Key                         | Default            | Description                         |
|---------------|-----------------------------|--------------------|-------------------------------------|
| `[relay]`     | `listen_addr`               | `0.0.0.0:9090`    | Relay API bind address              |
| `[providers]` | `registration_token`        | —                  | Provider auth token (CHANGE IN PROD) |
| `[settlement]`   | `cost_per_1k_tokens_nanoerg`  | `200000000`        | nanoERG cost per 1K tokens              |

### Environment Variables
| Variable                      | Component  | Description                          |
|-------------------------------|------------|--------------------------------------|
| `XERGON_CONFIG`              | Agent      | Path to agent config.toml            |
| `XERGON__*`                  | Agent      | Config overrides (e.g. `XERGON__API__LISTEN_ADDR`) |
| `XERGON_RELAY__*`            | Relay      | Config overrides                     |
| `RELAY_URL`                  | Marketplace| Override relay proxy target           |
| `NEXT_PUBLIC_XERGON_AGENT_BASE` | Marketplace | Agent API base URL              |
| `NEXT_PUBLIC_API_BASE`       | Marketplace | API base path                        |

---

## Development

### Agent
```bash
cd xergon-agent
cargo test              # Run 31 tests
cargo clippy            # Lint
cargo fmt               # Format
```

### Relay
```bash
cd xergon-relay
cargo test              # Run 42 tests
cargo clippy
cargo fmt
```

### Marketplace
```bash
cd xergon-marketplace
npm run typecheck        # TypeScript check
npm run lint             # ESLint
npm run dev              # Dev server with Turbopack
```

---

## PoNW Scoring

Proof-of-Node-Work combines three weighted categories:

| Category    | Weight | Metrics                                      |
|-------------|--------|----------------------------------------------|
| Node Work   | 40%    | Uptime, sync status, peer count, tip height   |
| Network Work| 30%    | Xergon peer confirmations, unique peers seen  |
| AI Work     | 30%    | Requests processed, tokens generated, model difficulty |

---

## License

MIT
