# Xergon Network -- Mainnet Launch

**Decentralized AI Compute on Ergo**

---

Hey everyone! We're excited to announce that **Xergon Network** is live on Ergo mainnet.

## What is Xergon?

Xergon turns every healthy Ergo node into a local, private, censorship-resistant AI compute provider. Instead of sending your prompts to OpenAI or Claude, inference gets routed through a network of independent Ergo node operators running open-weight models like Llama, Qwen, Mistral, and DeepSeek.

No accounts. No cloud APIs. No middlemen. Your Ergo wallet is your identity, ERG is the currency, and the blockchain is the database.

## Quick Start

**For users -- try AI inference in two commands:**

```bash
curl -sSL https://degens.world/xergon | sh
xergon ask "Explain zero-knowledge proofs"
```

That's it. First run auto-generates a local wallet and drops free ERG for the free tier. No signup, no email, no credit card.

**Already have an OpenAI integration?** Just swap the base URL:
```bash
export OPENAI_BASE_URL=https://relay.xergon.gg/v1
```
Everything works unchanged -- same endpoints, same streaming, same response format.

**For providers -- earn ERG with your existing Ergo node:**

```bash
curl -sSL https://degens.world/xergon | sh
xergon-agent setup
```

The setup menu auto-detects your GPU, Ergo node, wallet, and AI backend (Ollama/llama.cpp). Pick your models, set your region, and you're live. ERG payments settle automatically to your node wallet.

## What Makes Xergon Unique on Ergo

- **11 audited ErgoScript contracts** -- provider identity, user balances, usage proofs, treasury, GPU rentals, cross-chain bridge. All on-chain via eUTXO. Two-pass security audit with 26 findings, all patched.
- **Proof-of-Node-Work (PoNW)** -- providers scored on node health (40%), network confirmations (30%), and AI inference work (30%). Hard to fake, easy to verify.
- **Stateless relay** -- the relay owns nothing. All state lives on-chain. Take it down, spin up another, zero data loss.
- **Wallet-connected dApp** -- connect Nautilus or SAFEW wallet, browse models, manage balance, no passwords.
- **One-command install** for both users and providers. Interactive setup menu with auto-detection.
- **Fully open source** (MIT) -- Rust agent + relay, Next.js marketplace. Built by Degens World.

## Links

- GitHub: https://github.com/n1ur0/Xergon-Network
- Lite Paper: https://github.com/n1ur0/Xergon-Network/blob/main/Xergon%20LitePaper.md
- Built by: https://degens.world

Come try it out, run a node, serve a model, and earn some ERG. Questions? Ask away!
