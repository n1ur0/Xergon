# Xergon Network: Decentralized AI Compute on Ergo -- Mainnet Launch

**Built by [Degens World](https://degens.world) | [GitHub](https://github.com/n1ur0/Xergon-Network) | MIT License**

---

Hey Ergo community,

After 12 phases of development, two-pass security audits, and months of testing on testnet, we're launching **Xergon Network** on mainnet. Xergon turns every healthy Ergo node into a local, private, censorship-resistant AI compute provider -- powered entirely by ERG.

## What is Xergon?

Xergon is a decentralized Proof-of-Node-Work (PoNW) network for AI inference, built directly on the Ergo blockchain. Instead of relying on centralized cloud providers like OpenAI or Anthropic, Xergon routes inference requests through a network of independent Ergo node operators who run open-weight AI models (Llama, Qwen, Mistral, DeepSeek) alongside their nodes.

The core idea is simple: if you're already running an Ergo node, you have the infrastructure to serve AI. Xergon provides the software layer that makes it seamless -- no cloud accounts, no API keys, no middlemen.

**Your Ergo wallet is your identity. ERG is the currency. The blockchain is the database.**

## How It Works

Xergon has three components that run alongside each other:

**1. Xergon Agent (Rust, port 9099)**
A lightweight sidecar daemon that sits next to your Ergo node. It monitors node health (sync status, peer count, tip height), discovers other Xergon peers through the Ergo network, calculates your PoNW score, proxies inference requests to your local AI backend, and handles ERG settlement automatically.

**2. Xergon Relay (Rust, port 9090)**
A stateless routing layer that verifies signatures, reads provider state from the UTXO set, checks user ERG balances on-chain, and routes inference requests to the best available provider based on PoNW score, latency, and current load. The relay owns no state -- take it down, spin up another, zero data loss.

**3. Xergon Marketplace (Next.js 15, port 3000)**
A web frontend where users can browse available models, try the playground, connect their Nautilus wallet (EIP-12), and monitor their ERG balance and usage. Providers can view their dashboard, PoNW ranking, and earnings.

## Proof-of-Node-Work (PoNW)

Every provider earns a composite score based on three weighted categories:

| Category      | Weight | Metrics                                         |
|---------------|--------|-------------------------------------------------|
| Node Work     | 40%    | Uptime, sync status, peer count, tip height     |
| Network Work  | 30%    | Xergon peer confirmations, unique peers seen    |
| AI Work       | 30%    | Requests processed, tokens generated, model difficulty |

This score determines provider ranking on the leaderboard, influences request routing, and is stored on-chain in provider box registers. It's hard to fake, easy for others to verify, and tied to real compute.

## What's New for Mainnet

**11 Audited ErgoScript Contracts**
All network state lives on-chain via the eUTXO model -- no SQLite, no PostgreSQL. The contracts cover the full lifecycle:

- Provider Box -- on-chain identity and state with singleton NFT pattern
- User Staking Box -- ERG prepaid balance (the box value IS the balance)
- Usage Proof Box -- immutable audit trail per inference request
- Treasury Box -- protocol reserve and airdrop funding
- Usage Commitment -- Merkle tree batch proofs for epoch rollups
- GPU Rental Listing -- time-boxed rental contracts with auto-refund
- GPU Rating -- reputation for renters and providers
- Relay Registry -- decentralized relay discovery
- Payment Bridge -- cross-chain invoice-based payments (BTC/ETH/ADA)
- Bootstrap -- genesis deployment script
- Minting -- provider NFT minting

All 11 contracts went through a two-pass security audit (26 findings identified, all patched). A centralized transaction safety module with 7 validators and 25 unit tests guards all transaction builders.

**Wallet-Connected dApp**
The marketplace uses EIP-12 connector for Nautilus and SAFEW wallets. Sign transactions, manage your staking box, browse providers -- all from your Ergo wallet. No email, no passwords, no accounts.

**Production Monitoring**
Prometheus + Grafana + Alertmanager stack for real-time monitoring of agent health, relay metrics, chain scanner state, and settlement status. Pre-built dashboards and alert rules included.

## How to Try It (Users)

If you want to use AI inference without going through centralized providers:

```bash
# One-command install
curl -sSL https://degens.world/xergon | sh

# Interactive prompt
xergon ask "Explain zero-knowledge proofs in simple terms"

# Specify a model
xergon ask --model qwen3.5-32b "Write a Solidity smart contract for an ERC-20 token"

# Pipe input
cat code.py | xergon ask "Review this code for security vulnerabilities"
```

First run generates a local encrypted Ergo wallet automatically. You get a small ERG airdrop for free-tier inference. If you already have an OpenAI integration, just swap the base URL:

```bash
export OPENAI_BASE_URL=https://relay.xergon.gg/v1
# Your existing OpenAI SDK code works unchanged
```

The API is fully OpenAI-compatible at `/v1/chat/completions` with streaming support.

## How to Become a Provider

If you're already running an Ergo node with a GPU, you can start earning ERG for AI inference in minutes:

```bash
# Install
curl -sSL https://degens.world/xergon | sh

# Run interactive setup
xergon-agent setup
```

The setup menu auto-detects your GPU (via nvidia-smi), Ergo node (port 9053), wallet status, and available AI backends (Ollama, llama.cpp). You choose your models, set your region, and pick your GPU mode (mine + serve, or serve only). The agent registers on-chain, mints your provider NFT, and starts serving inference requests.

ERG payments settle automatically to your node wallet. No manual invoicing, no withdrawal requests -- it's all handled by the on-chain contracts.

## Quick Start Checklist

- [ ] Install: `curl -sSL https://degens.world/xergon | sh`
- [ ] Setup: `xergon-agent setup` (providers) or `xergon ask "hello"` (users)
- [ ] Check status: `xergon status` (PoNW score, balance, uptime)
- [ ] Connect wallet at the marketplace (Nautilus/SAFEW)
- [ ] Browse models and providers

## Links

- **GitHub:** https://github.com/n1ur0/Xergon-Network
- **Lite Paper:** [Xergon LitePaper.md](https://github.com/n1ur0/Xergon-Network/blob/main/Xergon%20LitePaper.md)
- **Roadmap:** [ROADMAP.md](https://github.com/n1ur0/Xergon-Network/blob/main/ROADMAP.md)
- **Contracts:** [contracts/](https://github.com/n1ur0/Xergon-Network/tree/main/contracts)
- **Built by:** [Degens World](https://degens.world)

## What Makes Xergon Different

There are plenty of "decentralized AI" projects. Most are centralized services with a blockchain sticker. Xergon is different because:

1. **The blockchain is the database.** Provider registry, balances, usage proofs, reputation -- all on-chain via Ergo boxes and registers. The relay is stateless.
2. **No accounts needed.** Your Ergo wallet is your identity. Signature-based auth, not passwords.
3. **OpenAI-compatible.** Drop-in replacement. Same endpoints, same response format, same streaming. Just change the base URL.
4. **One-command install.** Both users and providers get a single curl command with an interactive setup menu.
5. **Fully open source.** MIT licensed. Rust agent + relay, Next.js marketplace. Build it yourself if you want.

We built Xergon because we believe Ergo's eUTXO model is uniquely suited for decentralized compute markets. Deterministic transactions, no gas surprises, no reverts, and the ability to encode complex state machine logic in ErgoScript registers. This is what on-chain AI compute looks like.

Come try it. Run a node. Serve a model. Earn ERG.

---

*Questions? Feedback? Find us on the Ergo forum or reach out through Degens World.*
