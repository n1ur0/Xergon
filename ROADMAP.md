# Xergon Network -- Roadmap

## The Problem We Solve

AI practitioners are trapped:
- **Rate limits** -- GPT-4, Claude, Gemini all throttle you after a few requests
- **Gated models** -- frontier models locked behind waitlists, approvals, corporate vetting
- **High prices** -- $0.03-0.15 per 1K tokens adds up fast for research, fine-tuning, eval runs
- **No transparency** -- you can't see what's running, where your data goes, how pricing works
- **Vendor lock-in** -- switch providers and rewrite your whole integration

Xergon fixes this by turning every GPU that's already mining Ergo into an AI inference node.
When a GPU isn't hashing blocks, it serves models. Providers earn more ERG. Users get
cheap, private, uncensored inference. Nobody goes through a cloud API.

## The Two Audiences

### AI Users (95% of people)
They don't know what Ergo is. They don't care. They want to:
- Run inference without hitting rate limits
- Access open-weight models (Llama, Qwen, Mistral, DeepSeek)
- Pay less than OpenAI
- Keep their prompts private

They install one command, send a prompt, get a response. That's it.

### GPU Owners / Providers
They already run Ergo nodes. Their GPUs sit idle between blocks.
They want to:
- Monetize idle GPU time without changing their setup
- Earn ERG for serving inference (on top of mining rewards)
- Boost their PoNW score and network reputation

They install one command, answer a setup menu, and their GPU starts earning twice.

---

## Architecture Principles

1. **Ergo blockchain is the database** -- no SQLite, no PostgreSQL. Provider registry,
   balances, usage proofs, reputation: all on-chain via Ergo boxes and registers.
2. **Relay is stateless** -- it proxies requests, verifies signatures, reads chain state.
   It owns nothing. Take it down, spin up another, zero data loss.
3. **One-liner install** -- both providers and users get a single curl command that
   installs everything and launches an interactive setup menu.
4. **Invisible blockchain** -- AI users never see ERG, never sign transactions,
   never know crypto is involved. They see an AI API that works.
5. **OpenAI-compatible** -- drop-in replacement for OpenAI/Anthropic APIs. Same
   endpoints, same response format, same streaming. Just change the base URL.

---

## Phases

### Phase 0 -- Clean Slate [DONE]

Remove the web2 crutches that contradict the mission.

**Delete:**
- [x] `db.rs` -- all 7 tables (users, credits, webhooks, api_keys, usage, pricing, password resets)
- [x] `auth.rs` -- JWT tokens, email/password signup, password hashing
- [x] `credits.rs` -- USD credit system, credit packs, Stripe integration
- [x] `handlers/admin.rs` -- admin panel (no central admin in a decentralized network)
- [x] `registration.rs` -- provider registration with shared secret token
- [x] `rate_limit.rs` -- IP-based rate limiting (replace with on-chain stake/ERG balance check)
- [x] Stripe webhook handler
- [x] `handlers/api_keys.rs` -- API key management
- [x] `handlers/usage.rs` -- per-user usage analytics
- [x] `handlers/inference.rs` -- legacy inference API (redundant with chat/completions)
- [x] `usage_report.rs` -- usage reporting loop to provider agents
- [x] xergon-marketplace auth pages (signin, signup, forgot-password, reset-password)
- [x] Marketplace credits/pricing pages (replace with ERG-denominated model)

**Keep (already correct):**
- [x] `proxy.rs` -- inference request proxying and streaming
- [x] `handlers/chat.rs` -- OpenAI-compatible `/v1/chat/completions`
- [x] `handlers/models.rs` -- model listing
- [x] `handlers/leaderboard.rs` -- provider ranking by PoNW score
- [x] `provider.rs` -- provider health checking and selection
- [x] Agent: PoNW scoring engine, inference proxy, peer discovery, settlement
- [x] Agent: `settlement/transactions.rs` -- ERG payment via node wallet API

**Outcome:** Relay becomes a thin stateless router. Agent becomes the full node-side stack.

---

### Phase 1 -- One-Liner Install + Setup Menu [DONE]

The install experience for both providers and users.

```
curl -sSL https://degens.world/xergon | sh
```

**Provider setup menu (first run):**
```
  ╔══════════════════════════════════════════╗
  ║          XERGON NETWORK SETUP            ║
  ╠══════════════════════════════════════════╣
  ║                                          ║
  ║  Provider Name: [Xergon_LT        ]      ║
  ║  Region:        [us-east          ]      ║
  ║                                          ║
  ║  ── Detection ──────────────────────     ║
  ║  GPU:  NVIDIA RTX 4090 (24GB)      ✓     ║
  ║  Ergo Node: http://127.0.0.1:9053  ✓     ║
  ║  Wallet:   UNLOCKED                  ✓     ║
  ║  ERG Balance: 142.5 ERG             ✓     ║
  ║                                          ║
  ║  ── AI Backend ─────────────────────     ║
  ║  [1] Ollama       (detected: :11434)     ║
  ║  [2] llama.cpp    (detected: :8080)      ║
  ║  [3] tinygrad     (not detected)         ║
  ║  [4] Custom URL                         ║
  ║  > 1                                     ║
  ║                                          ║
  ║  ── Models ─────────────────────────     ║
  ║  [✓] qwen3.5-4b-f16.gguf                ║
  ║  [ ] llama-3.1-8b                        ║
  ║  [ ] mistral-7b-v0.3                     ║
  ║                                          ║
  ║  ── GPU Mode ───────────────────────     ║
  ║  [1] Mine + Serve (default)              ║
  ║  [2] Serve only (no mining)              ║
  ║  > 1                                     ║
  ║                                          ║
  ║  Ergo Address: 9fDr...1kM (from node)    ║
  ║  Relay:        https://relay.xergon.gg   ║
  ║                                          ║
  ║        [ Start Xergon ]                  ║
  ╚══════════════════════════════════════════╝
```

**Tasks:**
- [x] Build `xergon-installer` shell script (curl|sh bootstrap)
- [ ] Binary distribution: pre-built musl static binaries for linux/amd64, linux/arm64, darwin/arm64
- [x] Interactive TUI setup menu (clap-based interactive prompts)
- [x] Auto-detect: GPU (nvidia-smi), Ergo node (port 9053), wallet status, LLM backends
- [x] Auto-install: Ollama if no backend detected, pull recommended models
- [x] Config file generation from menu choices
- [x] systemd/launchd service installation
- [ ] `xergon status` -- show PoNW score, ERG balance, models serving, uptime
- [ ] `xergon update` -- self-update to latest binary

**User install (AI practitioner):**
```
curl -sSL https://degens.world/xergon | sh
xergon ask "Write a Python quicksort"
```

First run generates an Ergo wallet automatically (mnemonic shown once, encrypted locally).
Small amount of ERG airdropped for free tier. User never sees this unless they look.

---

### Phase 2 -- On-Chain State [DONE]

Replace all SQLite state with Ergo blockchain state.

**Ergo Contracts (what goes on-chain):**

| Contract | Purpose | Key Registers |
|----------|---------|---------------|
| Provider Registry | Registered providers with metadata | R4=provider_id, R5=endpoint, R6=models (Coll), R7=ergo_address, R8=pown_score |
| Provider Box | Per-provider state box, updated on heartbeat | R4=last_heartbeat, R5=total_tokens_served, R6=total_requests, R7=region |
| Usage Proof | Per-request proof of inference work | R4=user_pk, R5=provider_id, R6=model, R7=token_count, R8=timestamp |
| Staking Box | User ERG deposit for prepaid inference | R4=user_pk, R5=balance_nanoerg, R6=created_at |

**Relay reads chain state:**
- `GET /v1/providers` -- scan UTXO set for Provider Registry boxes
- `GET /v1/models` -- aggregate models from all active provider boxes
- `GET /v1/leaderboard` -- sort providers by PoNW register value
- User balance check -- scan UTXO set for user's Staking Box

**Relay writes chain state:**
- Usage proofs submitted by agents after each inference request
- Provider heartbeat updates (agent submits tx to update its box registers)

**Tasks:**
- [x] Design ErgoScript contracts for Provider Registry box
- [x] Design Provider Box with heartbeat update mechanism
- [x] Design Usage Proof box (lightweight, batch-submitted)
- [x] Design Staking Box for user ERG deposits
- [x] Agent: submit heartbeat as Ergo tx (update provider box registers)
- [x] Agent: submit usage proofs after inference (batched)
- [x] Relay: read providers from UTXO set instead of in-memory registry
- [x] Relay: verify user balance from on-chain Staking Box
- [x] Relay: remove in-memory provider store fallback, chain-only listing with ChainCache
- [x] Ergo node UTXO set scanning with ChainCache (background poll every 10s, lazy refresh)

---

### Phase 3 -- Invisible User Experience [DONE]

The AI user sees a clean, fast API. No crypto, no wallet, no signup.

**CLI:**
```bash
# Install
curl -sSL https://degens.world/xergon | sh

# Interactive
xergon ask "Explain quantum computing"

# Specify model
xergon ask --model qwen3.5-32b "Write Rust code for a web server"

# Pipe input
cat code.py | xergon ask "Review this code for bugs"

# OpenAI-compatible (for existing tools)
export OPENAI_API_KEY=$(xergon token)
export OPENAI_BASE_URL=https://relay.xergon.gg/v1
# Now any OpenAI SDK client just works
```

**What happens under the hood:**
1. First run: generates Ergo keypair, stores encrypted locally
2. `xergon ask` signs request with local key
3. Relay verifies signature, checks on-chain ERG balance
4. Routes to best provider, proxies response
5. Agent submits usage proof on-chain
6. ERG deducted from user's Staking Box, added to provider's earnings

**Free tier:**
- New wallets get a small ERG airdrop (funded from Xergon treasury box)
- Enough for ~100 inference requests
- No signup, no email, no credit card
- Rate limited by ERG balance (when it's zero, requests are rejected)

**Tasks:**
- [x] Build `xergon` CLI binary (Rust)
- [x] Local encrypted wallet (mnemonic -> keypair, stored in ~/.xergon/wallet.json)
- [x] `xergon ask` command with streaming output
- [x] `xergon models` -- list available models from relay
- [x] `xergon balance` -- show ERG balance
- [x] `xergon deposit <amount>` -- fund wallet (shows ERG address to send to)
- [x] `xergon token` -- generate OpenAI-compatible API token (HMAC-signed, short-lived)
- [x] Signature verification in relay (HMAC-SHA256, replay protection, /v1/auth/status)
- [x] On-chain airdrop mechanism for new users (treasury box, auto-airdrop on setup)
- [x] Update marketplace UI to use signature-based auth (wallet popup or CLI)

---

### Phase 4 -- GPU Bazar [DONE]

Beyond inference: rent GPU time directly.

The same hardware that mines ERG and serves AI can also be rented for arbitrary
compute (training, fine-tuning, rendering, etc.).

**Tasks:**
- [x] GPU rental listing on-chain (GPU type, VRAM, bandwidth, price per hour in ERG)
- [x] Time-boxed rental contracts (Ergo box with timeout, auto-refund if unused)
- [x] SSH/Jupyter access to rented GPU nodes (tunneled through agent)
- [x] Marketplace page: browse available GPUs, filter by VRAM/price/region
- [x] Usage metering and automatic ERG deduction per hour
- [x] Reputation system: renters rate providers, providers rate renters

---

### Phase 5 -- Network Effects [DONE]

Make the network self-sustaining and self-healing.

**Tasks:**
- [x] Multi-relay discovery (relays register on-chain, agents auto-discover)
- [x] P2P provider-to-provider communication (model distribution, load balancing)
- [x] Automatic model pulling (when a model is requested but not locally available)
- [x] Incentive layer: providers who serve rare models earn bonus PoNW points
- [x] Lightweight rollups: batch usage proofs into a single commitment tx
- [x] Cross-chain: accept payments from other chains via bridge contracts

---

### Phase 6 -- Production Readiness [DONE]

Hardening, testing, documentation, and UX polish for mainnet deployment.

**Tasks:**
- [x] Binary distribution: pre-built musl static binaries for linux/amd64, linux/arm64, darwin/arm64
- [x] `xergon status` -- show PoNW score, ERG balance, models serving, uptime
- [x] `xergon update` -- self-update to latest binary
- [x] xergon-marketplace auth pages (wallet-based signin/signup replacing JWT)
- [x] Marketplace credits/pricing pages (replace with ERG-denominated model)
- [x] Integration tests: agent + relay + chain interaction end-to-end
- [x] Contract compilation pipeline: ErgoScript -> ErgoTree hex in config
- [x] Provider onboarding docs: one-command setup from zero to serving
- [x] Monitoring: health endpoints, metrics, alerting for relay operators
- [x] Load testing: simulate 100+ concurrent users, measure throughput

---

### Phase 7 -- Testnet Deployment & Wallet Integration [DONE]

Bridge from working code to a live testnet deployment. Real compiled contracts,
on-chain bootstrap, and Nautilus wallet support so anyone can use the marketplace.

**Tasks:**
- [x] Contract compilation: ErgoScript -> real ErgoTree hex (replace placeholders)
- [x] Bootstrap script: mint Xergon Network NFT, create Treasury Box on testnet
- [x] Provider registration: on-chain tx that creates Provider Box + NFT
- [x] User staking: on-chain tx that creates User Staking Box
- [x] EIP-12 Nautilus wallet: real `ergoConnector.nautilus.connect()` in marketplace
- [x] Marketplace reads live chain data: providers, models, balances from node
- [x] On-chain integration tests: create/spend/verify boxes against testnet node
- [x] Testnet deployment guide: step-by-step from `git clone` to live network

### Phase 8 -- Security Audit & Transaction Safety [DONE]

Hardening all transaction builders with safety guards, contract audit fixes, and
input validation to prevent value loss, dust outputs, and invalid on-chain state.

**Tasks:**
- [x] Centralized tx safety module (`protocol/tx_safety.rs`) with 7 validators + 25 unit tests
- [x] `validate_box_value()` -- dynamic minimum based on box size (tokens, registers)
- [x] `validate_fee()` -- bounds check (0.001 ERG min, 0.1 ERG max)
- [x] `validate_address_or_tree()` -- Ergo address prefixes, pk_ format, ErgoTree hex
- [x] `validate_pk_hex()` -- 33-byte compressed secp256k1, 02/03 prefix check
- [x] `validate_token_id()` -- 64-char hex (32 bytes)
- [x] `validate_batch_size()` -- max 50 items per batch (gas exhaustion prevention)
- [x] `validate_payment_request()` -- full JSON validation before node submission
- [x] Integrated guards into all tx builders: chain/transactions, protocol/bootstrap,
      settlement/transactions, payment_bridge, rollup, gpu_rental/transactions

---

## What We Have vs What We Need

### Already Built (keep)

| Component | What Works | Status |
|-----------|-----------|--------|
| Agent: PoNW scoring | Node work (40%) + Network work (30%) + AI work (30%) | DONE |
| Agent: Inference proxy | OpenAI-compatible, streaming SSE, Ollama + llama.cpp | DONE |
| Agent: Peer discovery | Scans Ergo peers for other Xergon agents | DONE |
| Agent: Settlement engine | ERG payment via node wallet API, batch settlements | DONE |
| Agent: Node health | Monitors sync, peers, tip height | DONE |
| Relay: Chat proxy | `/v1/chat/completions` with streaming and fallback chain | DONE |
| Relay: Provider selection | Score-based routing (PoNW + latency + load) | DONE |
| Relay: Leaderboard | Provider ranking by score | DONE |
| Marketplace: Playground | Chat UI with model selector, streaming | DONE |
| Marketplace: Pages | Models, pricing, settings, provider dashboard | DONE |
| Agent: GPU rental | Listing, escrow, metering, SSH tunnel, reputation | DONE |
| Agent: Multi-relay | On-chain registry, auto-discovery, health check failover | DONE |
| Agent: P2P | Provider-to-provider model notify, peer info exchange | DONE |
| Agent: Auto model pull | Ollama/HF download, P2P peer pull, 503 Retry-After | DONE |
| Agent: Usage rollups | Merkle tree commitment boxes, epoch batching | DONE |
| Agent: Payment bridge | Invoice-based cross-chain (BTC/ETH/ADA), refund timeout | DONE |
| Relay: GPU Bazar | Listings, rent, pricing, ratings, reputation endpoints | DONE |
| Relay: Incentive | Rare model rarity scoring, bonus PoNW multiplier | DONE |
| Relay: Bridge | Invoice create/confirm/refund, multi-chain support | DONE |
| Marketplace: GPU Bazar | Browse/filter GPUs, rent modal, my rentals, pricing | DONE |

### Must Build

**Phase 20 in progress. Current: Phase 20.**

| Component | What's Next | Phase |
|-----------|-------------|-------|

### Phase 9 -- Mainnet Readiness [DONE]

Final polish before mainnet launch. Monitoring, alerting, documentation, and any remaining UX fixes.

**Tasks:**
- [x] Production monitoring: Prometheus + Grafana dashboard (agent, relay, chain scanner)
- [x] Alerting: wallet balance low, node desync, provider dropout, settlement failures
- [x] Operator runbook: step-by-step mainnet deployment, key management, recovery
- [x] Marketplace UX pass: loading states, error handling, edge cases on all pages
- [x] Rate limiting: relay endpoints (per-IP + per-API-key), prevent abuse
- [x] Binary release: GitHub Actions CI, cross-compile (linux-amd64, linux-arm64, macos)
- [x] Mainnet contract deployment: fresh bootstrap on mainnet, new NFTs
- [x] Security review: comprehensive audit of all 10 ErgoScript contracts, 26 findings, patches created (docs/SECURITY_AUDIT.md, patches/)
- [x] Load testing: Python async load harness in tests/load-test/, 4 scenarios, CI-compatible JSON output

### Phase 10 -- Post-Audit Hardening & Cleanup [DONE]

Apply security audit patches, implement native transaction builders, recompile contracts, and harden remaining findings.

**Tasks:**
- [x] Apply CRITICAL security patches to contracts (user_staking.ergo auth fix)
- [x] Apply HIGH NFT amount verification patches (7 contracts)
- [x] Implement ergo-lib native transaction builders (4 stub functions)
- [x] Contract recompilation pipeline (patches -> ErgoTree hex -> config)
- [x] Code cleanup: remove dead_code TODOs, stale comments
- [x] Post-audit hardening: remaining MEDIUM findings

### Phase 11 -- Network Launch Prep [DONE]

Test suite refresh, CI contract compilation, on-chain testnet verification, wallet-connected dApp, and launch documentation.

**Tasks:**
- [x] Refresh test suite: ergo-lib tx builder unit tests, patched contract property tests
- [x] CI pipeline: auto-compile all 11 contracts on push, fail on compilation errors
- [x] On-chain integration tests: deploy patched contracts to testnet, verify spend paths (15 tests covering all 11 contracts)
- [x] dApp v2: wallet-connected marketplace using EIP-12 connector (Nautilus/SAFEW) -- signTx/submitTx, tx builders, useWalletTx hook
- [x] Provider onboarding guide: step-by-step setup with real testnet contracts
- [x] User-facing docs: how to get ERG, connect wallet, run first inference
- [x] Genesis bootstrap script: fresh mainnet deployment with all patched contracts

### Phase 12 -- Mainnet Deployment & Launch [DONE]

Production mainnet deployment, monitoring, binary releases, and community onboarding.

**Tasks:**
- [ ] Compile all 11 contracts against mainnet node, verify ErgoTree hex (requires mainnet node)
- [ ] Run deploy-genesis.sh on mainnet (mint NFT, create Treasury, verify on explorer) (requires mainnet node + funded wallet)
- [ ] Update xergon-agent config.toml with mainnet contract hex values (depends on deploy-genesis)
- [x] Build release binaries: GitHub Actions CI/CD + local build-release.sh (4 targets: linux/mac x86_64/aarch64)
- [x] Create GitHub Release with binaries, checksums, and install script (release.yml workflow)
- [x] Mainnet smoke test: smoke-test-mainnet.sh -- 13-step lifecycle verification
- [x] Set up production monitoring: Prometheus + Grafana + Alertmanager stack (docker/monitoring/)
- [x] Install script + config template: scripts/install.sh, config.toml.example (curl | sh)
- [x] Publish docs to degens.world (announcements ready: docs/ANNOUNCEMENT-ERGO-FORUM.md, ANNOUNCEMENT-TWITTER.md, ANNOUNCEMENT-DISCORD.md)
- [x] Community announcement: Ergo forum post, Twitter thread, Discord/Telegram (drafts ready for review)

### Phase 13 -- ERG-Native Economics & Codebase Cleanup [DONE]

Remove all USD/credits/JWT/Stripe references from code and docs. Make the entire stack
ERG-native: pricing, rate limiting, settlement, and user-facing language.

**Tasks:**
- [x] Replace IP-based rate limiting with ERG balance-based throttling (5 tiers by staking balance)
- [x] Refactor settlement module from USD to pure nanoERG denomination (cost_per_1k_nanoerg, min_settlement_nanoerg)
- [x] Delete dead pages: forgot-password, reset-password, signup (redirect stubs and duplicates)
- [x] Delete orphaned CreditsBadge component
- [x] Remove dead API surface: 6 interfaces (UserInfo, CreditBalance, CreditPack, etc.) + 8 endpoints
- [x] Remove JWT token logic from client.ts and config.ts
- [x] Rename creditsCharged -> costNanoerg across playground, store, ProviderDashboard
- [x] Fix stale /signup links in pricing/signin pages
- [x] Clean CLAUDE.md and AGENTS.md (JWT/Stripe/credits references)
- [x] Clean README.md (remove JWT secret, Stripe, credits config rows)
- [x] Add deprecation notices to docs/HOW-IT-WORKS.md and docs/marketplace-ux-design.md

### Phase 14 -- On-Chain Dynamic Pricing & Oracle Integration [DONE]

Replace fixed per-1K-token pricing with dynamic ERG pricing based on model demand,
provider supply, and optional oracle feeds. This makes the marketplace self-regulating.

**Tasks:**
- [x] Design dynamic pricing equation: base_cost * demand_multiplier * model_complexity_factor
- [x] Provider-side: allow providers to set their own per-model nanoERG price in Provider Box registers
- [x] Relay-side: aggregate provider prices, select cheapest valid provider (price-aware routing)
- [x] Demand tracking: count recent requests per model in ChainCache, compute demand multiplier
- [x] Oracle integration: consume ERG/USD oracle pool box as data-input for fiat-equivalent display
- [x] Marketplace UI: show real-time ERG cost per model, cost estimate before sending prompt
- [x] Settlement: use provider's registered price (from chain) instead of global config cost_per_1k_nanoerg
- [x] Provider heartbeat: agent builds R6 with structured pricing JSON (per-model nanoERG prices)
- [x] Free tier: treasury-funded airdrop box tracks per-user free requests, hard limit before ERG required

### Phase 15 -- Provider Tooling & E2E Reliability [DONE]

Polish the provider experience and harden end-to-end reliability for production use.

**Tasks:**
- [x] Provider dashboard: web UI for updating pricing, viewing earnings, health status
- [x] Provider CLI commands: `xergon provider set-price --model llama-3.1-8b --price 50000`
- [x] Settlement reconciliation: periodic check that on-chain payments match relay-recorded usage
- [x] Multi-model serving: provider can serve multiple models simultaneously with different pricing
- [x] Health check SLA: providers that miss health checks get temporarily deprioritized (not removed)
- [x] E2E integration test: full flow from user request -> relay routing -> provider inference -> settlement
- [x] Config validation: agent/relay reject invalid configs at startup with clear error messages
- [x] Graceful degradation: relay continues serving cached models if chain scanner temporarily fails
- [x] Structured logging: JSON-formatted logs with request IDs for tracing across relay -> provider -> settlement

### Phase 16 -- API Quality & Observability [DONE]

Harden the relay and agent API surface with consistent error responses, observability
improvements, security middleware, and API documentation.

**Tasks:**
- [x] SIGTERM graceful shutdown for relay + agent (flush logs, drain connections, cleanup)
- [x] Security headers middleware (X-Content-Type-Options, X-Frame-Options, CSP, Referrer-Policy)
- [x] Rate limit response headers (X-RateLimit-Limit, X-RateLimit-Remaining, X-RateLimit-Reset, Retry-After)
- [x] Latency histogram with percentiles (p50/p95/p99) -- thread-safe, exponential buckets, Prometheus format
- [x] Agent structured JSON error responses: {"error": {"type", "message", "code"}} across all ~25 handlers
- [x] Relay standardized JSON error responses: gpu, bridge, balance handlers match proxy format
- [x] OpenAPI 3.0.2 spec for all relay endpoints (docs/openapi.yaml, 23 endpoints, HMAC auth)
- [x] Wire RelayMetrics into chat handler (inc_chat_requests, observe_request_latency_ms, inc_errors)
- [x] Free tier request tracker: per-user counter (DashMap + AtomicU64), 100 free requests, 24h decay
- [x] CORS middleware (already existed, verified)

### Phase 17 -- Marketplace dApp v3 & SDK [DONE]

Modernize the marketplace frontend with a proper React SDK for Xergon, improved UX,
and real-time chain data integration.

**Tasks:**
- [x] Xergon TypeScript SDK: typed client for relay API (chat, models, balance, GPU bazar, incentive)
- [x] Real-time provider status: WebSocket or SSE feed for provider online/offline events
- [x] Chat playground v2: conversation history, model comparison side-by-side, token counting
- [x] Wallet integration improvements: auto-reconnect, transaction signing feedback, error recovery
- [x] Provider dashboard v2: real-time earnings chart, request heatmap, model performance metrics
- [x] Mobile-responsive layout: full mobile support for marketplace and playground
- [x] Dark/light theme toggle with system preference detection
- [x] Embeddable chat widget: copy-paste snippet for external sites to use Xergon inference
- [x] Rate limit indicator in playground: show remaining requests, tier badge, upgrade prompt
- [x] Internationalization (i18n): support EN, JA, ZH, ES locales

### Phase 18 -- ErgoAuth dApp Protocol & On-Chain Analytics [DONE]

Implement ErgoAuth (EIP-based wallet authentication), real on-chain analytics dashboard
for the marketplace, and ErgoPay support for mobile wallet users.

**Tasks:**
- [x] ErgoAuth integration: implement EIP-12 dApp connector auth flow in marketplace (verify wallet signature server-side)
- [x] ErgoPay support: generate ErgoPay URIs for mobile wallet transactions (QR code signing flow)
- [x] On-chain analytics dashboard: live network stats (total providers, total ERG staked, requests/24h, active models)
- [x] Provider explorer: browse all registered providers on-chain with filters (region, models, PoNW score, uptime)
- [x] Network health page: real-time relay status, chain scanner status, provider distribution map
- [x] Transaction history: user's on-chain inference payment history (scan UTXO set for staking box changes)
- [x] Marketplace i18n rollout: replace all hardcoded strings with t() calls using the i18n infrastructure from Phase 17
- [x] SDK documentation: README, API reference, code examples for xergon-sdk
- [x] OpenAPI spec sync: auto-generate SDK types from relay's OpenAPI spec (docs/openapi.yaml)
- [x] Accessibility audit: ARIA labels, keyboard navigation, screen reader support across all pages

### Phase 19 -- Production Hardening & DevEx [DONE]

Lock down the marketplace for real users: test infrastructure, error handling,
developer experience, and operational tooling.

**Tasks:**
- [x] Marketplace test infrastructure: install vitest + React Testing Library, write component tests for Navbar, ModelSelector, PromptBox, ResponseArea, ThemeToggle, LanguageSwitcher (39 tests)
- [x] Error boundaries: React error boundary wrapper with retry UI, per-page error boundaries, error logging to console
- [x] Loading skeletons: skeleton components for all data-dependent pages (analytics, explorer, health, leaderboard, transactions, models)
- [x] React Suspense: wrap async data fetching in Suspense boundaries with fallback skeletons
- [x] Docker Compose: already existed (marketplace + relay + agent), confirmed working
- [x] PWA support: web app manifest, service worker for API response caching, offline fallback page, installable on mobile
- [x] Landing page: fixed copy to match wallet-based auth (no "Sign Up"), added "Become a Provider" CTA section
- [x] CI pipeline for marketplace: added test step to marketplace CI job, added new SDK test job
- [x] WebSocket real-time provider status: relay /ws/status endpoint with broadcast, marketplace client with reconnect, live dots on explorer
- [x] SDK integration tests: test suite against mock relay responses, coverage for chat completions, model listing, provider queries

### Phase 20 -- Advanced Protocol & Multi-Chain [IN PROGRESS]

Strengthen the on-chain protocol with advanced Ergo patterns and prepare
cross-chain bridge architecture for broader reach.

**Tasks:**
- [ ] Batch settlement: agent accumulates inference fees in memory, submits batch payment transactions (spend N staking boxes, update M provider boxes) instead of per-request settlement
- [ ] Usage proof commitment: batch usage proof boxes into commitment boxes with blake2b256 merkle root (reduces UTXO bloat, follows oracle-core pattern)
- [ ] Provider slashing contract: ErgoScript guard that enables automatic ERG slash from provider stake box when uptime SLA is violated (inspired by Rosen Bridge watcher pattern)
- [ ] Multi-relay health consensus: relays gossip their provider lists via WebSocket, marketplace merges them into a unified view with conflict resolution
- [ ] On-chain governance proposal box: singleton NFT box holding governance state (proposal count, voting threshold), ErgoScript enforces proposal lifecycle
- [ ] Cross-chain price oracle: oracle pool box (EIP-23 pattern) that tracks ERG/USD price for dynamic nanoERG pricing, with epoch-based data posting

### Must Delete

| Component | Why It's Wrong | Phase |
|-----------|---------------|-------|
| `db.rs` (SQLite) | State belongs on-chain, not in a file | 0 |
| `auth.rs` (JWT/email) | Auth = Ergo signature, not passwords | 0 |
| `credits.rs` (USD) | Currency = ERG, not USD credits | 0 |
| Stripe integration | No fiat rails. ERG only. | 0 |
| `rate_limit.rs` (IP) | Rate limit = ERG balance, not IP | 0 |
| Admin panel | No central authority in a P2P network | 0 |
| Signup/login pages | No email auth. Wallet-based only. | 0 |

---

## Timeline

```
Phase 0  [DONE]       Delete SQLite/JWT/Stripe. Relay = stateless router.
Phase 1  [DONE]       One-liner install + setup menu. Binary distribution.
Phase 2  [DONE]       On-chain state. ErgoScript contracts. Chain scanner.
Phase 3  [DONE]       xergon CLI. Signature auth. Invisible UX.
Phase 4  [DONE]       GPU Bazar. Rental contracts. SSH access.
Phase 5  [DONE]       Multi-relay. P2P. Rollups. Cross-chain.
Phase 6  [DONE]       Production: tests, binary dist, docs, monitoring.
Phase 7  [DONE]       Testnet: real contracts, bootstrap, Nautilus wallet, on-chain tests.
Phase 8  [DONE]       Security audit, tx safety guards, contract hardening.
Phase 9  [DONE]       Mainnet readiness: monitoring, docs, security audit, load testing, bootstrap.
Phase 10 [DONE]       Post-audit hardening: security patches, ergo-lib tx builders, contract recompilation, cleanup.
Phase 11 [DONE]       Network launch prep: test suite, CI contracts, on-chain tests, dApp v2, launch docs.
Phase 12 [DONE]       Mainnet deployment: compile contracts, genesis deploy, release binaries, smoke test, monitoring, community.
Phase 13 [DONE]       ERG-native economics: rate limit by balance, nanoERG settlement, delete credits/JWT/Stripe from code and docs.
Phase 14 [DONE]       Dynamic pricing: per-provider on-chain pricing, demand tracking, oracle feeds, price-aware routing.
Phase 15 [DONE]       Provider tooling: CLI pricing commands, settlement reconciliation, health SLA, E2E tests, structured logging, multi-model serving, pricing dashboard.
Phase 16 [DONE]       API quality: structured JSON errors, latency histogram, security headers, rate limit headers, OpenAPI spec, free tier tracker, metrics wiring.
Phase 17 [DONE]       Marketplace dApp v3: TypeScript SDK, SSE events, playground v2, provider dashboard v2, mobile, theme, embed widget, rate limit, i18n.
Phase 18 [DONE]       ErgoAuth (EIP-28 + EIP-12), ErgoPay (EIP-20 QR signing), on-chain analytics dashboard, provider explorer, network health page, transaction history, i18n rollout (170+ keys x 4 locales), SDK README/docs, OpenAPI spec sync, accessibility audit (ARIA, focus traps, keyboard nav).
Phase 19 [DONE]       Production hardening: vitest + RTL tests (39), error boundaries, loading skeletons, Suspense, Docker Compose, PWA (manifest + SW), landing page copy fix + provider CTA, marketplace CI + SDK CI, WebSocket live status, SDK integration tests (34).
Phase 20 [IN PROGRESS] Advanced protocol: batch settlement, usage proof commitment (merkle), provider slashing contract, multi-relay consensus, on-chain governance, cross-chain price oracle (EIP-23).
```

Phases 0-3 are the MVP. A user can install with one command and run inference
through a decentralized provider network, paying ERG, without ever knowing
what Ergo is.

Phases 4-5 are growth. GPU Bazar turns idle miners into a general compute
marketplace. Network effects make it self-sustaining.

---

## Implementation Research: How to Actually Build This on Ergo

This section documents the research into Ergo's eUTXO model, on-chain
patterns, SDK tooling, and ecosystem contracts that inform the actual
implementation of Xergon's on-chain state (Phase 2+).

Sources: Ergo Developer Knowledge Base (ergo-kb), community transcripts
(ergo-transcripts), EIPs, and ecosystem project contract analysis.

### 1. The eUTXO Mental Model

Ergo doesn't have accounts. Everything is a **Box** -- an immutable container
holding ERG, tokens, typed registers (R4-R9), and a guarding script (ErgoTree).

To change state, you **spend** a box and **create** a new box with updated
registers. The old box is destroyed. The new box has a new ID. The guarding
script validates that the transition is legal.

This is fundamentally different from Ethereum:
- No mutable storage. No "call contract to update state."
- You build a transaction that consumes specific boxes and creates specific
  new boxes. Each box's script validates the transition.
- Transactions are deterministic: if valid when you construct them, they
  will be valid when mined (no gas surprises, no reverts).
- Failed transactions simply don't happen -- you validate locally first.

For Xergon, this means:
- Provider state lives in boxes, not database rows
- "Updating a provider" = spending the provider box, creating a new one with
  updated registers (heartbeat, PoNW score, models served)
- "Deducting user balance" = spending the user's staking box, creating a
  new one with lower ERG value
- "Recording usage" = creating a usage proof box with request metadata

### 2. Key Patterns from Ecosystem Projects

**Oracle Pools (EIP-23)** -- the closest architectural match for Xergon:
- A Pool Box holds current state (rate in R4, epoch counter in R5)
- Identified by a **Singleton NFT** (supply=1 token that travels with the box)
- Oracle Boxes each hold a unique oracle token + accumulated reward tokens
- Heartbeat equivalent: each oracle posts a DataPoint box every epoch
- Refresh transaction: spends Pool Box + Refresh Box + all Oracle Boxes,
  recreates them with updated state
- Key insight: oracle boxes are **inputs** (spent and recreated), not data
  inputs. This enables the refresh contract to enforce logic across all
  participants atomically.

**Spectrum DEX (ErgoDEX)** -- for understanding the order box pattern:
- Swap orders are boxes created by traders, sitting in the UTXO set
- Off-chain bots scan the UTXO set, find compatible orders, and build
  transactions that satisfy both orders' ErgoScript conditions
- The on-chain scripts do all enforcement; bots can't steal funds because
  any invalid transaction is rejected by the network
- Key insight: this is exactly the Xergon relay's role. The relay scans
  for active provider boxes and usage proof boxes, building the routing
  and settlement logic off-chain while the contracts enforce correctness.

**Rosen Bridge** -- for cross-chain guard/watcher architecture:
- Guard NFT (Singleton) linearizes the guard state across state transitions
- Watchers stake tokens (X-RWT) to participate; fraud results in automatic
  slashing via ErgoScript
- Commitment boxes store blake2b256(eventData ++ watcherId) -- binding
  identity to data without revealing contents
- Key insight: Xergon can use the same staking pattern for providers.
  Providers stake ERG into a Provider Box. Misbehavior (serving garbage
  results, going offline) reduces their stake. The contract enforces this
  automatically.

### 3. Singleton NFT Pattern -- The Identity Mechanism

Every major Ergo protocol uses this pattern:
1. Mint a token with supply=1 in a bootstrap transaction
2. The token ID = the first input's box ID (Ergo's native token creation rule)
3. This NFT lives inside the protocol's state box, traveling with it across
   state transitions
4. The ErgoScript checks: "output must contain this NFT" -- preventing
   duplication and ensuring exactly one instance exists

For Xergon, we need these singletons:

| NFT | Purpose | Lives In |
|-----|---------|----------|
| Xergon Network NFT | Protocol identity | Treasury/Airdrop Box |
| Relay Registry NFT | Identify registered relays | Relay Registry Box |
| Provider NFT (per provider) | Identify a provider's state box | Provider Box |

The Provider NFT is minted when a provider first registers. It travels
with their Provider Box through every heartbeat update. Anyone can find
a specific provider's box by scanning for UTXOs containing their NFT.

### 4. Box Design for Xergon

**Provider Box (per provider, updated on heartbeat):**
```
Tokens:  Provider NFT (supply=1), earned ERG tokens
R4:      Provider public key (GroupElement) -- for proveDlog
R5:      Endpoint URL (Coll[Byte]) -- encoded UTF-8 string
R6:      Models served (Coll[Byte]) -- encoded JSON array
R7:      PoNW score (Int) -- 0-1000
R8:      Last heartbeat height (Int)
R9:      Region (Coll[Byte]) -- encoded UTF-8 string

Guard script: only the provider (proveDlog of R4) can spend this box.
Output must preserve the Provider NFT.
```

**Usage Proof Box (created after each inference request):**
```
R4:      User public key hash (Coll[Byte])
R5:      Provider NFT ID (Coll[Byte])
R6:      Model name (Coll[Byte])
R7:      Token count (Int)
R8:      Request timestamp (Long)
Guard:   any P2PK (submitted by agent, no spending restriction needed)
```

Note: Usage proofs are **created**, not spent. They accumulate in the UTXO
set and get cleaned up by storage rent after 4 years. For Phase 2 MVP,
this is fine. Phase 5 adds rollups to batch them into commitment boxes.

**User Staking Box (ERG deposit for inference access):**
```
Value:   ERG amount (nanoERGs) -- this IS the balance
R4:      User public key (SigmaProp) -- for proveDlog

Guard script: only the user (proveDlog of R4) can spend.
Path 1: user spends to create a new staking box with less ERG (payment)
Path 2: anyone can spend after 4 years (storage rent cleanup)
```

This is simpler than a timelock. The box just holds ERG. The relay checks
the box's value to determine user balance. Payment = spend the box, create
a new one with (value - fee). No separate "credit" system needed.

**Treasury Box (funds airdrops for new users):**
```
Tokens:  Xergon Network NFT (supply=1)
Value:   ERG reserve for airdrops

Guard script: only the protocol deployer (proveDlog) can spend.
Output must preserve the NFT. R4 tracks total airdropped amount.
```

### 5. Off-Chain Architecture: The Headless dApp Pattern

Every serious Ergo project separates protocol logic into layers:

1. **Box Specifications** -- declarative schemas for what valid boxes look like
2. **Wrapped Boxes** -- typed structs that validate raw ErgoBox data at
   construction time (e.g., ProviderBox, StakingBox, UsageProofBox)
3. **Protocol Equations** -- pure functions for PoNW scoring, fee calculation,
   balance checking. No I/O, no box dependencies.
4. **Action Functions** -- transaction builders that take wrapped boxes and
   user parameters, return unsigned transactions. Never sign, never submit.
5. **Box Finders** -- thin I/O adapters that query the node's UTXO set
   or the Explorer API, returning wrapped boxes.

For Xergon's Rust agent, this maps to:
- `xergon_agent::protocol::specs` -- BoxSpec definitions
- `xergon_agent::protocol::boxes` -- ProviderBox, StakingBox, etc.
- `xergon_agent::protocol::equations` -- pown_score(), fee_for_tokens(), etc.
- `xergon_agent::protocol::actions` -- register_provider(), submit_heartbeat(),
  pay_provider(), airdrop_erg()
- `xergon_agent::chain` -- node API client, box scanning, caching

### 6. Transaction Building with Fleet SDK / ergo-lib

Two SDK options, depending on where the code runs:

**Fleet SDK (TypeScript)** -- for relay, marketplace frontend, browser tools:
- `TransactionBuilder` -- assemble inputs, outputs, fees, change
- `OutputBuilder` -- create output boxes with tokens, registers, values
- `compile()` -- compile ErgoScript strings to ErgoTree
- `estimateMinBoxValue()` -- dynamic minimum value based on box size
- `payMinFee()` -- append miner fee output (0.0011 ERG)
- `sendChangeTo()` -- handle leftover ERG and tokens

**ergo-lib-wasm (Rust/Node.js)** -- for the agent binary:
- Same capabilities, native Rust API
- `Wallet::from_mnemonic()` for headless signing (no browser wallet needed)
- This is what Xergon's agent will use since it runs as a binary alongside
  the Ergo node

Key rules for every transaction:
- Every output must hold >= SAFE_MIN_BOX_VALUE (0.001 ERG)
- Fee output must pay >= RECOMMENDED_MIN_FEE_VALUE (0.001 ERG)
- Tokens in inputs must equal tokens in outputs (unless minting/burning)
- New token IDs must equal the first input's box ID
- Registers must be densely packed (no gaps: can't have R4+R6 without R5)

### 7. Signature-Based Auth (replacing JWT)

Ergo uses **Sigma protocols** for authorization. Every contract returns a
SigmaProp -- a cryptographic statement that must be proven.

For Xergon auth:
- Each user has an Ergo keypair (generated on first `xergon` CLI run)
- Requests to the relay include a Schnorr signature (proveDlog)
- The relay verifies the signature against the user's known public key
- The public key comes from the user's Staking Box in the UTXO set
- No JWT, no session tokens, no passwords

The relay's auth flow:
1. User sends request with: body + timestamp + signature(body + timestamp)
2. Relay extracts the public key from the signature
3. Relay scans UTXO set for a Staking Box with matching R4 (SigmaProp)
4. Relay checks box value >= minimum for the request
5. Relay proxies to provider, returns response
6. Agent submits payment tx (user staking box value decreases)

### 8. Node API: Reading Chain State

The relay doesn't need to run a full node. It reads chain state via:

**Box scanning by token ID:**
```
GET /api/v1/boxes/unspent/byTokenId/{providerNftId}
```
Returns all unspent boxes containing a specific token. Use this to find
provider boxes, staking boxes, etc.

**Box scanning by ErgoTree (registered scans):**
```
POST /wallet/registerscan  { "trackingRule": { "scanName": "providers", ... } }
GET /wallet/boxes/uncertain/{scanId}
```
Register a scan for boxes with a specific ErgoTree. The node tracks them
automatically. This is how oracle-core tracks pool boxes.

**Box registers:**
Each box response includes `additionalRegisters` with R4-R9 as hex-encoded
Sigma type values. Parse them to extract provider metadata, scores, etc.

**Caching strategy:**
Don't query the node on every request. Poll every 10-30 seconds, cache the
results in memory. The relay is stateless but can have an in-memory cache
that refreshes periodically. If the cache is stale, the worst case is
routing to a provider that just went offline -- the agent handles failover.

### 9. Storage Rent Considerations

Ergo charges storage rent on boxes that sit unspent for 4+ years:
- After 4 years (1,051,200 blocks), miners can collect rent from idle boxes
- Rent = boxSizeInBytes * 360 nanoERG/byte per cycle
- A minimal box pays ~0.14 ERG per 4-year cycle
- Tokens CANNOT pay rent -- only ERG can

For Xergon:
- **Provider Boxes** are refreshed every heartbeat (~every 30 blocks = 1 hour).
  They never sit idle for 4 years. No rent concern.
- **Usage Proof Boxes** accumulate and could sit idle. This is fine for MVP --
  they'll be garbage collected eventually. Phase 5 rollups will batch them.
- **User Staking Boxes** -- if a user deposits 1 ERG and disappears, their box
  survives ~28 years before rent fully consumes it. Adequate.
- **Treasury Box** -- fund with enough ERG for 100+ years of rent. 10 ERG
  lasts ~280 years.

### 10. What to Build First (Implementation Order)

Based on the research, the dependency order is:

1. **Bootstrap transaction** -- mint the Xergon Network NFT, create the
   Treasury Box. One-time deployment. Done with ergo-lib-wasm or Fleet SDK.

2. **Provider registration contract** -- ErgoScript guard that allows
   anyone to create a Provider Box by spending ERG into it. The contract
   mints a Provider NFT (supply=1) and stores provider metadata in registers.
   The guard requires proveDlog so only the provider can update their box.

3. **Provider heartbeat action** -- agent spends Provider Box, creates new
   one with updated R8 (last heartbeat height). Same NFT travels with it.
   Simple spend-and-recreate transaction.

4. **User staking contract** -- ErgoScript guard: only the user (proveDlog
   of R4) can spend. Two paths: (a) user spends to pay for inference,
   (b) storage rent cleanup after 4 years. The box value IS the balance.

5. **Chain state reader** -- agent module that polls the Ergo node API
   for boxes by token ID, parses registers, and caches results. This
   replaces the in-memory provider store.

6. **Usage proof submission** -- agent creates a Usage Proof Box after each
   inference request. No spending restriction needed -- it's a receipt that
   sits in the UTXO set. Batch submission in Phase 5.

7. **Payment flow** -- agent spends user's Staking Box, creates new one
   with (value - fee), creates a new Provider Box with (earned + fee).
   Two boxes consumed, two boxes created, one transaction.

### 11. Reference Projects & EIPs

| Resource | What to Study |
|----------|--------------|
| ergoplatform/oracle-core (Rust) | State machine pattern, box scanning, action builders, heartbeat txs |
| spectrum-finance/ergo-dex (Scala) | Order box pattern, off-chain executor bots, Singleton NFT usage |
| rosen-bridge/contract (ErgoScript) | Staking/slashing, guard NFT, watcher permits, cross-chain guard consensus |
| EIP-4 (Assets Standard) | Token minting, NFT metadata in R4-R9, encoding conventions |
| EIP-23 (Oracle Pool v2) | Multi-stage contract design, epoch-based data posting, reward tokens |
| ergo_headless_dapp_framework (Rust) | BoxSpec, WrappedBox, headless dApp pattern |
| Fleet SDK (TypeScript) | Transaction building, register serialization, box selection |
| ergo-lib-wasm (Rust) | Headless signing, Wallet API, transaction construction |
