# Xergon Relay

Central marketplace backend for the Xergon Network - handles provider registration, authentication, rate limiting, and inference routing.

## Overview

The Xergon Relay is a Rust-based Axum server that acts as the central hub for the Xergon Network marketplace. It manages:

- **Provider Registration**: Dynamic registration of GPU providers with PoNW scores
- **Authentication**: HMAC-SHA256 signature verification for API security
- **Rate Limiting**: Per-API-key rate limiting to prevent abuse
- **Inference Routing**: Routes chat completion requests to available providers
- **Settlement Tracking**: Tracks usage for on-chain ERG settlement

## Tech Stack

- **Runtime**: Tokio (async runtime)
- **Web Framework**: Axum 0.7
- **Database**: Rusqlite (SQLite with bundled libsqlite3)
- **HTTP Client**: Reqwest
- **Configuration**: Config 0.14
- **Cryptography**: HMAC 0.12, SHA2 0.10

## Quick Start

### 1. Install Dependencies

```bash
# Ensure Rust is installed (1.70+)
rustc --version

# Install clippy (optional but recommended)
sudo apt-get install rust-clippy
```

### 2. Configure the Relay

```bash
cd xergon-relay
cp config.toml.example config.toml
```

Edit `config.toml`:

```toml
[server]
host = "0.0.0.0"
port = 9090

# Pre-registered providers (optional - dynamic registration is also supported)
[[providers]]
id = "provider-1"
name = "Test GPU Provider"
base_url = "http://localhost:8080"
api_key = "${PROVIDER_API_KEY}"
```

### 3. Run the Relay

```bash
# Development
cargo run --release

# With custom config path
CONFIG_PATH=/path/to/config.toml cargo run --release

# With custom settlement database
SETTLEMENT_DB_PATH=/path/to/settlement.db cargo run --release
```

The relay will start on `http://0.0.0.0:9090` by default.

## API Endpoints

### Provider Management

#### POST `/register`
Register a new provider with the network.

**Request:**
```json
{
  "provider_id": "gpu-provider-1",
  "ergo_address": "9fDrtPahmtQDAPbq9AccibtZVmyPD8xmNJkrNXBbFDkejkez1kM",
  "region": "us-east",
  "models": ["qwen-3.5-122b", "llama-3-70b"],
  "capacity_gpus": 8,
  "max_concurrent_requests": 10
}
```

**Response:**
```json
{
  "success": true,
  "provider_id": "gpu-provider-1",
  "message": "Provider registered successfully",
  "registered_at": 1713072000,
  "endpoint": "/heartbeat",
  "heartbeat_interval": "30s"
}
```

#### POST `/heartbeat`
Send heartbeat from a registered provider.

**Request:**
```json
{
  "provider_id": "gpu-provider-1",
  "pown_score": 85.5
}
```

**Response:**
```json
{
  "status": "ok",
  "received_at": 1713072000,
  "provider_status": {
    "provider_id": "gpu-provider-1",
    "health_status": "healthy",
    "last_seen": 1713072000,
    "pown_score": 85.5
  }
}
```

#### GET `/providers`
List all registered providers.

**Response:**
```json
[
  {
    "provider_id": "gpu-provider-1",
    "ergo_address": "9fDrtPahmtQDAPbq9AccibtZVmyPD8xmNJkrNXBbFDkejkez1kM",
    "region": "us-east",
    "models": ["qwen-3.5-122b"],
    "capacity_gpus": 8,
    "max_concurrent_requests": 10,
    "registered_at": 1713072000,
    "last_heartbeat": 1713072000,
    "health_status": "healthy",
    "pown_score": 85.5
  }
]
```

### Inference

#### POST `/v1/chat/completions`
Chat completion endpoint (requires API key).

**Headers:**
```
X-API-Key: your-api-key
Content-Type: application/json
```

**Request:**
```json
{
  "model": "qwen-3.5-122b",
  "messages": [
    {"role": "user", "content": "Hello, how can you help me?"}
  ],
  "temperature": 0.7,
  "max_tokens": 1024
}
```

**Response:**
```json
{
  "id": "chatcmpl-123",
  "object": "chat.completion",
  "created": 1713072000,
  "model": "qwen-3.5-122b",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "I can help you with..."
      },
      "finish_reason": "stop"
    }
  ]
}
```

### Settlement

#### POST `/settlement/batch`
Submit a batch of usage proofs for settlement.

**Headers:**
```
X-API-Key: your-api-key
Content-Type: application/json
```

**Request:**
```json
{
  "proofs": [
    {
      "provider_id": "gpu-provider-1",
      "user_id": "user-123",
      "tokens_input": 100,
      "tokens_output": 500,
      "model": "qwen-3.5-122b",
      "timestamp": 1713072000,
      "signature": "abc123..."
    }
  ],
  "provider_signature": "def456..."
}
```

**Response:**
```json
{
  "success": true,
  "transaction_id": "batch-processed",
  "message": "Successfully processed 1 usage proofs",
  "batch_size": 1
}
```

#### GET `/settlement/summary`
Get settlement summary for an API key.

**Headers:**
```
X-API-Key: your-api-key
```

**Response:**
```json
{
  "success": true,
  "api_key": "your-api-key",
  "total_records": 10,
  "pending_records": 3,
  "settled_records": 7,
  "total_tokens_input": 1000,
  "total_tokens_output": 5000
}
```

### Health

#### GET `/health`
Health check endpoint.

**Response:**
```json
{
  "status": "healthy",
  "service": "xergon-relay",
  "version": "0.1.0",
  "features": ["registration", "heartbeat", "authentication", "rate-limiting", "settlement"]
}
```

## Security Features

### HMAC-SHA256 Signature Verification

All authenticated endpoints support signature verification:

1. Client creates payload (JSON request body)
2. Client computes HMAC-SHA256(payload, api_secret)
3. Client sends signature in `X-Signature` header
4. Server verifies signature before processing

### Rate Limiting

- Per-API-key rate limiting (default: 60 requests per minute)
- Sliding window implementation
- Returns `429 Too Many Requests` when exceeded

### API Key Management

- API keys are stored in memory (can be extended to database)
- Each key has an associated rate limit
- Keys can be added programmatically via `AuthManager::add_key()`

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `CONFIG_PATH` | Path to config.toml | `config.toml` |
| `SETTLEMENT_DB_PATH` | Path to settlement database | `data/settlement.db` |

### Config File Options

```toml
[server]
host = "0.0.0.0"
port = 9090

[[providers]]
id = "provider-id"
name = "Provider Name"
base_url = "http://provider:8080"
api_key = "${ENV_VAR}"  # Can use environment variables
```

## Database Schema

The settlement database uses SQLite with the following tables:

```sql
CREATE TABLE user_balances (
    user_id TEXT PRIMARY KEY,
    erg_balance INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL
);

CREATE TABLE usage_records (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    tokens_input INTEGER NOT NULL,
    tokens_output INTEGER NOT NULL,
    model TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    FOREIGN KEY (user_id) REFERENCES user_balances(user_id)
);
```

## Development

### Build

```bash
cargo build --release
```

### Run Tests

```bash
cargo test
```

### Code Quality

```bash
# Run clippy
cargo clippy -- -D warnings

# Format code
cargo fmt
```

### Docker

```bash
# Build image
docker build -t xergon-relay .

# Run container
docker run -p 9090:9090 -v $(pwd)/config.toml:/app/config.toml xergon-relay
```

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Client    │────▶│   Relay     │────▶│  Provider   │
│  (Market)   │     │  (Axum)     │     │ (GPU Node)  │
└─────────────┘     └─────────────┘     └─────────────┘
                           │
                           ▼
                    ┌─────────────┐
                    │   SQLite    │
                    │  (Settlement)
                    └─────────────┘
```

### Request Flow

1. **Client** sends request to Relay with API key
2. **Relay** validates API key and rate limit
3. **Relay** selects available provider (round-robin or score-based)
4. **Relay** forwards request to Provider
5. **Provider** processes inference and returns response
6. **Relay** records usage for settlement
7. **Client** receives response

## Integration with Xergon Network

The Relay integrates with:

- **xergon-agent**: Receives PoNW scores from agents
- **xergon-marketplace**: Frontend UI for users
- **Ergo Blockchain**: On-chain settlement for ERG payments

## Troubleshooting

### Common Issues

**"Failed to load config"**
- Ensure `config.toml` exists and is valid TOML
- Check environment variable substitutions (e.g., `${PROVIDER_API_KEY}`)

**"Provider not found"**
- Provider must register via POST `/register` first
- Check heartbeat endpoint is being called regularly

**"Invalid API key"**
- Ensure API key is included in `X-API-Key` header
- Check that the key is registered in the auth manager

### Logging

Enable verbose logging:

```bash
RUST_LOG=debug cargo run --release
```

## License

MIT License - See LICENSE file for details.

## Contributing

1. Fork the repository
2. Create a feature branch
3. Run `cargo clippy` and `cargo fmt`
4. Submit a pull request

## Support

- GitHub Issues: https://github.com/n1ur0/Xergon-Network/issues
- Documentation: https://github.com/n1ur0/Xergon-Network/wiki
