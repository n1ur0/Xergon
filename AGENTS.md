# Xergon Network — AI Agent Guide

## Project Structure

```
xergon-agent/        # Rust sidecar — PoNW scoring, P2P peer discovery, ERG settlement
xergon-relay/        # Rust backend — marketplace API, auth, credits, provider proxy
xergon-marketplace/  # Next.js 15 — frontend UI (playground, models, pricing, settings)
```

## Quick Start

### Agent
```bash
cd xergon-agent
cp config.toml.example config.toml  # edit with your node + provider details
cargo run --release
```

### Relay
```bash
cd xergon-relay
cp config.toml.example config.toml  # edit with DB path, JWT secret, Stripe keys
cargo run --release
```

### Marketplace
```bash
cd xergon-marketplace
npm install
npm run dev          # http://localhost:3000
```

## Tech Stack

| Component | Language | Framework |
|-----------|----------|-----------|
| xergon-agent | Rust | Axum + Tokio |
| xergon-relay | Rust | Axum + Tokio + Rusqlite |
| xergon-marketplace | TypeScript | Next.js 15 + React 19 + Tailwind 4 + Zustand |

## Architecture

- **Agent**: Runs alongside an Ergo node. Monitors health, tracks AI inference, computes PoNW scores, handles ERG settlement via Ergo node API.
- **Relay**: Central marketplace backend. Handles user auth (JWT), credit system, Stripe payments, rate limiting, and proxies inference requests to registered providers.
- **Marketplace**: Frontend for users to browse models, purchase credits, and send prompts to providers through the relay.

## Conventions

- Rust: `cargo clippy` + `cargo fmt` before committing
- Marketplace: `npm run typecheck` + `npm run lint` before committing
- All API endpoints return JSON with consistent error format: `{ "error": "...", "code": "..." }`
