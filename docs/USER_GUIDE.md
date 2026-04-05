# User Guide -- AI Inference on Xergon

## What is Xergon?

Xergon Network is a **decentralized AI inference marketplace** built on the Ergo blockchain. Instead of paying a monthly subscription to a cloud provider, you pay per inference request with **ERG** (Ergo's native token).

**Key benefits:**

- **Pay-per-use** -- no subscriptions, no upfront commitments
- **Decentralized** -- your requests are routed to the best available GPU provider
- **Trustless** -- all payments are on-chain. Providers can't fake usage
- **Private** -- no API keys or accounts required. Your wallet is your identity
- **Open** -- OpenAI-compatible API, works with existing tools

---

## Getting Started

### Option A: Via the `xergon` CLI (Recommended)

The `xergon` CLI handles wallet creation, signing, and relay communication automatically.

#### Install

```bash
# Linux (x86_64)
curl -sL https://github.com/n1ur0/Xergon-Network/releases/latest/download/xergon-linux-x86_64 \
  -o xergon && chmod +x xergon

# macOS (Apple Silicon)
curl -sL https://github.com/n1ur0/Xergon-Network/releases/latest/download/xergon-macos-aarch64 \
  -o xergon && chmod +x xergon
```

#### First-Time Setup

```bash
xergon setup
```

This will:

1. Generate a local wallet (encrypted, stored at `~/.xergon/wallet.json`)
2. Save your relay configuration
3. Attempt to claim a free ERG airdrop to get you started

```
  ╔══════════════════════════════════════════════╗
  ║           XERGON WALLET SETUP                ║
  ╚══════════════════════════════════════════════╝

  Create a password for your wallet: ********
  Confirm password: ********

  Wallet created!
  Public key: 02a1b2c3d4...
  Config saved to ~/.xergon/config.json

  Requesting free ERG airdrop...
  Airdrop received! 0.05 ERG deposited.

  You're all set! Try these commands:
    xergon status    -- Check wallet and relay connection
    xergon models    -- List available AI models
    xergon ask "hello" -- Send your first prompt
```

#### Check Status

```bash
xergon status
```

#### Fund Your Wallet (if airdrop unavailable)

```bash
# Show your deposit address
xergon deposit

# Send ERG to that address via Nautilus, an exchange, or a friend
# Then check balance:
xergon balance
```

### Option B: Via the dApp (Wallet Connected)

Visit the Xergon Marketplace dApp:

1. Open the marketplace URL in your browser
2. Click "Connect Wallet" and select Nautilus (or compatible Ergo wallet)
3. Browse available models and providers
4. Send inference requests directly through the UI

The dApp handles signing and payment automatically.

### Option C: Direct API (No Wallet)

If you just want to try it out, you can hit the relay's OpenAI-compatible API directly:

```bash
curl -X POST https://relay.xergon.gg/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3.1:8b",
    "messages": [{"role": "user", "content": "Hello, Xergon!"}],
    "stream": false
  }'
```

> **Note**: Without a wallet, you're limited to free-tier models and rate-limited requests. For full access, use the CLI or dApp.

---

## Running Your First Inference

### Step 1: Pick a Model

List available models:

```bash
xergon models
```

Or via API:

```bash
curl https://relay.xergon.gg/v1/models
```

Example output:

```json
[
  {
    "id": "llama3.1:8b",
    "name": "llama3.1:8b",
    "provider": "3 providers",
    "tier": "free",
    "price_per_input_token": 0.0,
    "price_per_output_token": 0.0,
    "available": true,
    "speed": "fast",
    "tags": ["Fast", "Free"]
  },
  {
    "id": "qwen2.5:32b",
    "name": "qwen2.5:32b",
    "provider": "1 providers",
    "tier": "pro",
    "price_per_input_token": 0.000002,
    "price_per_output_token": 0.000002,
    "available": true,
    "speed": "balanced",
    "tags": ["Smart", "Code", "Creative"]
  }
]
```

**Tiers:**
- **Free** -- no ERG cost, rate-limited
- **Pro** -- paid per token, higher quality

### Step 2: Check the Leaderboard

See which providers are most reliable:

```bash
curl https://relay.xergon.gg/v1/leaderboard
```

Providers are ranked by **PoNW score** (Proof-of-Network-Work), which measures uptime, latency, and reliability.

### Step 3: Send a Request

#### Via CLI

```bash
# Simple prompt
xergon ask "Explain quantum computing in one paragraph"

# Specify a model
xergon ask "Write a Python hello world" --model qwen2.5:32b

# Non-streaming (wait for full response)
xergon ask "What is Ergo?" --stream false

# Pipe input
echo "Summarize this article about AI" | xergon ask
```

#### Via curl (OpenAI-Compatible)

```bash
# Non-streaming
curl -X POST https://relay.xergon.gg/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "X-Xergon-Timestamp: $(date +%s%3N)" \
  -H "X-Xergon-Public-Key: YOUR_PUBLIC_KEY" \
  -H "X-Xergon-Signature: YOUR_SIGNATURE" \
  -d '{
    "model": "llama3.1:8b",
    "messages": [
      {"role": "system", "content": "You are a helpful assistant."},
      {"role": "user", "content": "What is decentralized AI?"}
    ],
    "temperature": 0.7,
    "max_tokens": 500,
    "stream": false
  }'
```

#### Via Python

```python
import requests

relay_url = "https://relay.xergon.gg"

# List models first
models = requests.get(f"{relay_url}/v1/models").json()
print("Available models:", [m["id"] for m in models])

# Send a chat completion (streaming)
response = requests.post(
    f"{relay_url}/v1/chat/completions",
    json={
        "model": "llama3.1:8b",
        "messages": [
            {"role": "user", "content": "Hello from Python!"}
        ],
        "stream": True,
    },
    stream=True,
)

for line in response.iter_lines():
    if line:
        print(line.decode("utf-8"))
```

#### Via JavaScript / TypeScript

```javascript
const RELAY_URL = "https://relay.xergon.gg";

// Non-streaming
const response = await fetch(`${RELAY_URL}/v1/chat/completions`, {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({
    model: "llama3.1:8b",
    messages: [{ role: "user", content: "Hello from JavaScript!" }],
    stream: false,
  }),
});

const data = await response.json();
console.log(data.choices[0].message.content);

// Streaming (SSE)
const stream = await fetch(`${RELAY_URL}/v1/chat/completions`, {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({
    model: "llama3.1:8b",
    messages: [{ role: "user", content: "Tell me a story" }],
    stream: true,
  }),
});

const reader = stream.body.getReader();
const decoder = new TextDecoder();

while (true) {
  const { done, value } = await reader.read();
  if (done) break;
  const text = decoder.decode(value);
  // Parse SSE data lines
  for (const line of text.split("\n")) {
    if (line.startsWith("data: ") && line !== "data: [DONE]") {
      const chunk = JSON.parse(line.slice(6));
      const content = chunk.choices?.[0]?.delta?.content || "";
      process.stdout.write(content);
    }
  }
}
```

### Step 4: Pay for Usage

#### How ERG Payments Work

When you use the CLI or dApp, payment is handled **automatically and invisibly**:

1. Your ERG balance lives in an on-chain **User Staking Box**
2. Each inference request deducts a small amount of ERG
3. Payment goes directly to the provider in an **atomic on-chain transaction**
4. A **Usage Proof Box** is created as an immutable receipt

You never need to manually send transactions -- the relay and agent handle everything.

#### Balance Tracking

```bash
# Check your ERG balance
xergon balance

# View your deposit address (to add more ERG)
xergon deposit
```

#### Getting ERG

- **Airdrop**: The `xergon setup` command tries to claim a free airdrop
- **Faucet**: Use the [Ergo testnet faucet](https://testnet.ergo.foundation/faucet)
- **Exchange**: Buy ERG on exchanges like [CoinEx](https://www.coinex.com), [GorillaPool](https://gorillapool.io)
- **Nautilus**: Receive from another Ergo user

---

## Advanced: Direct Provider Connection

You can bypass the relay and connect directly to a provider for lower latency. This requires managing on-chain payments yourself.

### 1. Browse Providers

```bash
curl https://relay.xergon.gg/v1/providers
```

Find a provider's `endpoint` (e.g., `http://203.0.113.42:9099`).

### 2. Send Request Directly

```bash
curl -X POST http://203.0.113.42:9099/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "llama3.1:8b",
    "messages": [{"role": "user", "content": "Hello directly!"}],
    "stream": false
  }'
```

### 3. Pay ERG Directly

For direct provider connections, you need to create an on-chain payment transaction that:

1. Spends your **User Staking Box** (deducts ERG)
2. Creates a **Usage Proof Box** (receipt)
3. Sends ERG to the **Provider Box**

This is handled automatically by `xergon-agent` on the provider side. The relay abstracts this entirely for normal users.

---

## GPU Rental

Xergon also lets you **rent GPUs** directly from providers for dedicated compute.

### Browse Available GPUs

```bash
xergon gpu list --region us-east --min-vram 16
```

### Check GPU Pricing

```bash
xergon gpu pricing
```

### Rent a GPU

```bash
xergon gpu rent <listing_id> 24    # Rent for 24 hours
```

### Manage Rentals

```bash
xergon gpu my-rentals               # View active rentals
xergon gpu extend <rental_id> 12    # Extend by 12 hours
xergon gpu refund <rental_id>       # Refund before deadline
```

### Rate Providers

```bash
xergon gpu rate <rental_id> 5 --role provider --comment "Great uptime!"
```

---

## Cross-Chain Payments

Don't have ERG? You can pay with BTC, ETH, or ADA via the cross-chain payment bridge.

```bash
# Create a payment invoice
xergon bridge invoice-create <provider_pk> 0.1 eth

# Check invoice status
xergon bridge invoice-status <invoice_id>

# View bridge status and supported chains
xergon bridge status
```

---

## API Reference

The relay exposes an OpenAI-compatible API:

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/v1/chat/completions` | POST | Send a chat completion request |
| `/v1/models` | GET | List available models |
| `/v1/providers` | GET | List registered providers |
| `/v1/leaderboard` | GET | Provider leaderboard by PoNW score |
| `/v1/balance/{user_pk}` | GET | Check ERG balance |
| `/v1/auth/status` | GET | Check auth system status |
| `/v1/gpu/listings` | GET | Browse GPUs for rent |
| `/v1/gpu/pricing` | GET | GPU pricing info |
| `/v1/gpu/rent` | POST | Rent a GPU |
| `/v1/gpu/rate` | POST | Rate a rental |
| `/v1/bridge/status` | GET | Cross-chain bridge status |
| `/health` | GET | Relay liveness check |
| `/ready` | GET | Readiness check (at least 1 healthy provider) |

### Authentication

For paid requests, include signed headers:

| Header | Description |
|--------|-------------|
| `X-Xergon-Public-Key` | Your wallet's public key (hex) |
| `X-Xergon-Timestamp` | Unix timestamp in milliseconds |
| `X-Xergon-Signature` | HMAC-SHA256 signature of `timestamp + method + path + body` |

The `xergon` CLI handles signing automatically.

---

## Security

### No API Keys Needed

Xergon uses **cryptographic signatures** instead of API keys. Your wallet's private key signs every request. This is more secure than traditional API key-based auth because:

- Signatures are **per-request** (can't be replayed)
- No centralized key server to compromise
- Your private key never leaves your machine

### On-Chain Trustlessness

- **Payments are atomic** -- ERG only moves when inference is delivered
- **Usage proofs are immutable** -- every request creates a permanent on-chain receipt
- **Provider reputation is transparent** -- PoNW scores are publicly verifiable
- **No middleman** -- payments go directly from user to provider

### Privacy

- The relay does **not** store your prompts or responses
- Inference happens on the provider's machine, not on any central server
- Your wallet address is the only identifier -- no email, no phone number
- Usage Proof Boxes store a hash of your public key, not your address

---

## Incentive System

Xergon rewards users and providers through an incentive system:

### Rarity Bonus

Providers running **rare models** (models with few providers) earn bonus PoNW points. This encourages a diverse model ecosystem.

```bash
# Check incentive status
curl https://relay.xergon.gg/v1/incentive/status

# View models with rarity bonuses
curl https://relay.xergon.gg/v1/incentive/models
```

### Free Tier

Many smaller models (7B parameters and below) are offered on the **free tier** -- no ERG required. This lets you try Xergon before committing.

---

## FAQ

### How do I get ERG?

- Run `xergon setup` to claim a free airdrop (if available)
- Use the [Ergo testnet faucet](https://testnet.ergo.foundation/faucet) for testnet ERG
- Buy mainnet ERG on exchanges like CoinEx or GorillaPool
- Receive ERG from another Ergo wallet user

### How much does inference cost?

- **Free tier models**: $0.00 (rate-limited)
- **Pro tier models**: ~$0.002 per 1K tokens (input + output)
- Pricing varies by model. Use `xergon models` to see current prices.

### What models are available?

Models depend on what providers are running. Common models include:

- `llama3.1:8b` -- Fast general-purpose (free tier)
- `llama3.1:70b` -- Larger, more capable (pro tier)
- `qwen2.5:32b` -- Strong reasoning (pro tier)
- `mistral:7b` -- Efficient coding (free tier)

Run `xergon models` to see the current list.

### Is my data private?

Your prompts are sent to a single provider's GPU and processed in-memory. They are not stored by the relay. However, the provider's node does process your request, so avoid sending highly sensitive data to unknown providers. For maximum privacy, run your own provider node.

### What happens if a provider goes offline?

The relay has **automatic fallback** -- if your selected provider fails, the request is retried with the next best provider (up to 3 attempts by default). This is transparent to you.

### Can I use Xergon with my existing tools?

Yes. The relay API is **OpenAI-compatible**. You can point any tool that supports the OpenAI API format at the relay URL:

```bash
export OPENAI_BASE_URL="https://relay.xergon.gg/v1"
# Now use with any OpenAI-compatible tool
```

### How do providers earn ERG?

Providers earn ERG per inference request. Payment is settled on-chain via atomic transactions. Providers can configure automatic settlement (daily, weekly, etc.) to their Ergo wallet.

### What is PoNW?

**Proof-of-Network-Work** (PoNW) is Xergon's provider reputation score (0-1000). It measures:
- Uptime and availability
- Inference latency
- Model diversity (rarity bonus)
- Successful request completion rate

Higher PoNW scores mean more requests routed to that provider.

---

## Getting Help

- **GitHub Issues**: https://github.com/n1ur0/Xergon-Network/issues
- **Discord**: Join the Xergon community for real-time support
- **Provider Guide**: See [`PROVIDER_ONBOARDING.md`](./PROVIDER_ONBOARDING.md) if you want to become a provider
- **Contracts**: See [`contracts/README.md`](../contracts/README.md) for technical blockchain details
