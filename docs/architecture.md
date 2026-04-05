# Xergon Network Architecture

System architecture and component overview for the Xergon decentralized AI compute network.

---

## System Diagram

```
                          ERGO BLOCKCHAIN
                    +-------------------------+
                    |   Provider Boxes (NFT)  |
                    |   Usage Proof Boxes     |
                    |   User Staking Boxes    |
                    |   Treasury Box          |
                    |   GPU Rental Listings   |
                    +----------+--------------+
                               |
                    +----------v--------------+
                    |      Ergo Node          |
                    |   (REST API :9053)      |
                    +----------+--------------+
                               |
          +--------------------+--------------------+
          |                                         |
   +------v------+                          +-------v-------+
   | xergon-agent|                          | xergon-relay  |
   | (Rust)      |                          | (Rust)       |
   |             |                          |               |
   | - PoNW      |<----HTTP/WS------------->| - Request     |
   | - Inference |     heartbeat/status      |   routing     |
   | - Contracts |                          | - Auth        |
   | - P2P       |                          | - Load bal.   |
   | - GPU rent  |                          | - Metrics     |
   | - Settlement|                          | - Health      |
   +------+------+                          +-------+-------+
          |                                         |
          |                                         |
   +------v------+                          +-------v-------+
   | LLM Backend |                          | xergon-       |
   | (Ollama/    |                          | marketplace   |
   |  llama.cpp/ |                          | (Next.js)     |
   |  vLLM)      |                          |               |
   |             |                          | - Browse      |
   | :11434/:8080|                          | - Chat UI     |
   +-------------+                          | - GPU rental  |
                                             | - Payments    |
                                             +---------------+

   User --> Marketplace --> Relay --> Agent --> LLM Backend
              (browser)     (routing)  (compute)
```

---

## Components

### xergon-agent

The provider-side Rust binary. Runs on each GPU node.

**Key responsibilities:**
- Connects to an Ergo node for on-chain operations
- Probes and proxies inference requests to local LLM backends
- Computes and maintains PoNW (Proof-of-Node-Work) score
- Registers with relays and sends periodic heartbeats
- Manages on-chain state via compiled ErgoScript contracts
- Handles GPU rental sessions (SSH tunnels, metering)
- P2P communication with other providers

**Subsystems:**

| Module              | Purpose                                      |
|---------------------|----------------------------------------------|
| `inference`         | OpenAI-compatible proxy to local LLM backends |
| `pown`              | PoNW score calculation and maintenance        |
| `peer_discovery`    | Find other Xergon agents via Ergo peer list   |
| `relay_client`      | Registration and heartbeat with relays        |
| `chain`             | On-chain transactions (usage proofs, heartbeat)|
| `contract_compile`  | Load and validate compiled ErgoScript hex      |
| `gpu_rental`        | GPU rental metering, SSH tunnel management     |
| `payment_bridge`    | Cross-chain invoice-based Lock-and-Mint        |
| `settlement`        | ERG accumulation and settlement ledger         |
| `rollup`            | Merkle tree batching of usage proofs           |
| `p2p`               | Provider-to-provider communication             |
| `auto_model_pull`   | Automatic model downloading from registries   |
| `wallet`            | BIP-39 keypair management                      |
| `signing`           | Transaction signing                           |

### xergon-relay

Central routing service. Matches user requests to available providers.

**Key responsibilities:**
- Maintains provider registry (endpoint, models, PoNW score)
- Routes inference requests to best-available provider
- Load balancing based on PoNW score and region
- API key authentication
- Health monitoring of providers
- Request proxying (streaming support)

**Endpoints:**

| Endpoint                  | Purpose                        |
|---------------------------|--------------------------------|
| `POST /v1/chat/completions`| OpenAI-compatible inference    |
| `GET /v1/models`          | List available models          |
| `POST /register`          | Provider registration          |
| `GET /health`             | Health check                   |
| `GET /providers`          | List registered providers      |

### xergon-marketplace

Next.js web frontend. User-facing interface for browsing and using the network.

**Key features:**
- Browse available providers and models
- Chat interface for inference
- GPU rental marketplace
- Provider dashboard
- Payment management

---

## Contract System

### Compilation Pipeline

ErgoScript contracts are compiled to ErgoTree hex and embedded in the agent binary at build time.

```
contracts/*.es  -->  Ergo Playground/AppKit  -->  contracts/compiled/*.hex
                                                              |
                                                              v
                                                     include_str!() at compile time
                                                              |
                                                              v
                                                     xergon-agent binary
```

**Override mechanism:**
Config `[contracts]` section can override embedded hex per-deployment:

```toml
[contracts]
provider_box_hex = "custom-compiled-hex-here"
```

This allows:
- Testing different contract versions without rebuilding
- Deploying testnet-specific contracts
- Hot-swapping contracts in development

### Contract Registry

| Contract              | File                        | Purpose                          |
|-----------------------|-----------------------------|----------------------------------|
| Provider Box          | `provider_box.es`           | Provider identity and state      |
| Provider Registration | `provider_registration.es`  | Initial provider onboarding      |
| Treasury Box          | `treasury_box.es`           | Protocol treasury and NFT        |
| Usage Proof           | `usage_proof.es`            | Inference receipt (immutable)    |
| User Staking          | `user_staking.es`           | Prepaid ERG balance for users    |

All contracts follow the **EIP-4** register convention and use the **Singleton NFT pattern** for state management.

See `contracts/README.md` for detailed register layouts and spending conditions.

---

## Data Flow: Inference Request

```
1. User sends chat request to marketplace
2. Marketplace forwards to relay (/v1/chat/completions)
3. Relay selects provider (highest PoNW, correct region, model available)
4. Relay proxies request to selected agent
5. Agent forwards to local LLM backend (Ollama/llama.cpp)
6. Response streams back through the chain
7. Agent records usage proof (off-chain, batched to on-chain)
8. Provider earns ERG (settled via on-chain transaction)
```

---

## Data Flow: GPU Rental

```
1. Provider lists GPU (type, VRAM, price, region)
2. Listing stored on-chain (GPU Rental Listing box)
3. User browses listings on marketplace
4. User pays ERG into on-chain escrow (GPU Rental box)
5. Agent creates SSH tunnel to user
6. User connects and uses GPU for rented duration
7. Metering loop checks session expiry
8. On expiry: ERG released to provider, tunnel closed
9. User can rate provider (GPU Rating box)
```

---

## Configuration

All components use TOML configuration files with environment variable overrides.

**Environment variable format:** `XERGON__SECTION__FIELD=value`
(overrides config.toml values)

Example: `XERGON__ERGO_NODE__REST_URL=https://my-node:9053`

See `xergon-agent/config.toml` for the full configuration reference.

---

## Security Model

- **Authentication**: API keys for relay registration; BIP-39 mnemonic for on-chain
- **Authorization**: ErgoScript contracts enforce spending rules on-chain
- **Confidentiality**: TLS for all HTTP connections; SSH for GPU rental tunnels
- **Integrity**: On-chain state is immutable; usage proofs are tamper-evident
- **Availability**: P2P mesh provides redundancy; multiple relays supported

---

## Deployment Topology

**Development:**
```
Local machine: xergon-agent + Ollama + Ergo node (regtest)
```

**Single provider:**
```
Server: xergon-agent + LLM backend + Ergo node
Cloud: xergon-relay + xergon-marketplace
```

**Production network:**
```
Multiple providers (various regions)
Multiple relays (load balanced)
CDN-backed marketplace
Shared Ergo full node or public nodes
```
