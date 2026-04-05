> **DEPRECATED**: This document describes the original Web2 design (JWT auth, Stripe payments, USD credits).
> The current system uses **Ergo-native economics**: wallet-based auth (EIP-12/Nautilus), ERG staking boxes for balances,
> and direct ERG payments. See [ROADMAP.md](../ROADMAP.md) for the current architecture.
> This file is kept for historical reference only.

# XER-7: Marketplace UX — Invisible Blockchain Design Spec

**Goal:** First-time users get value in under 60 seconds. No wallet, no ERG mentioned, no crypto jargon. Feels like a web app, not a dApp.

---

## 1. User Journey: First 60 Seconds

### Landing Page
- Hero: "Run AI models on distributed GPUs. Free tier included."
- Single input box: paste a prompt, select a model from a dropdown, click "Run"
- No sign-up wall. No wallet popup. No "Connect Wallet" button.
- Below: simple model cards showing name, speed tier, and a "free" badge

### Free Tier (Anonymous)
- Rate-limited: 10 requests/day, max 500 tokens per request
- No account needed — fingerprint by IP + browser fingerprint (lightweight)
- After free tier exhausted: inline banner "You've used your free queries. Create an account for more."

### Account Creation
- Email + password (standard web auth)
- Optional: "Sign in with Google" / GitHub
- No crypto wallet at this stage

### Paid Tier
- Pricing shown in **USD** only. ERG is never shown to the user.
- Prepaid credits model: buy $5, $10, $25 packs
- Payment methods: credit card (Stripe), Apple Pay, Google Pay
- Credits auto-replenish option

### When Does Blockchain Appear?
- **Never for 95% of users.** They pay fiat, get inference, done.
- ERG settlement happens server-side between Xergon and providers.
- Advanced users can optionally link an Ergo wallet for provider mode or direct ERG payments (hidden in Settings > Advanced).

---

## 2. Page Structure

### 2.1 Playground (Default Landing)
```
+------------------------------------------+
|  Xergon    [Models]  [Pricing]  [Sign In] |
+------------------------------------------+
|                                          |
|   [Model: Llama 3.1 8B  v]              |
|                                          |
|   +------------------------------------+ |
|   | Type your prompt here...            | |
|   |                                    | |
|   +------------------------------------+ |
|                                          |
|   [Run]                Credits: 50 left  |
|                                          |
|   +------------------------------------+ |
|   | Response appears here...            | |
|   |                                    | |
|   +------------------------------------+ |
|                                          |
+------------------------------------------+
```

Key UX decisions:
- Model selector is a simple dropdown, not a marketplace grid
- "Run" button is the only CTA on the page
- Credits counter in top-right, always visible
- No provider selection — the system picks the best provider automatically
- Response area shows the model name, latency, and token count after completion

### 2.2 Models Page
- Grid of model cards: name, description, context window, speed indicator
- Each card has "Try it" button that jumps to Playground with that model pre-selected
- Tags: "Fast", "Smart", "Code", "Creative", "Free"
- No ERG pricing shown — just "Free tier: 10 requests/day"

### 2.3 Pricing Page
- Three tiers displayed:
  - **Free**: 10 requests/day, 500 token limit
  - **Pro ($10/mo)**: 10,000 requests/mo, full context, priority queue
  - **Enterprise**: Custom limits, API key, dedicated providers
- All pricing in USD. Period.
- "Buy Credits" button leads to Stripe checkout
- FAQ section addresses: "How does this work?", "Where does compute come from?", "Is my data private?"

### 2.4 Settings (Authenticated)
- Account: email, password, 2FA
- API Keys: generate keys for programmatic access (OpenAI-compatible endpoint)
- Usage: daily/monthly request counts, token usage graphs
- Advanced (collapsed): "Link Ergo Wallet" — only visible if user explicitly expands Advanced section

---

## 3. Provider Abstraction Layer

The user never sees providers. The system handles provider selection internally:

### Smart Routing
1. User sends prompt + model choice
2. Backend receives request, checks user's credits/rate limit
3. Backend queries available providers via Xergon agent API (`/api/v1/status`)
4. Selects provider based on: latency, PoNW score, current load, model availability
5. Proxies the inference request to selected provider
6. Streams response back to user
7. Deducts credits, updates usage stats

### Fallback Chain
- If primary provider fails: automatic retry with next-best provider
- User sees "Switching to a faster provider..." toast (no error)
- If all providers busy: queue with position indicator ("#3 in queue")
- Maximum wait: 30 seconds, then "All providers busy. Try again in a moment."

### What the User Sees vs What Happens

| User Sees | System Does |
|-----------|-------------|
| "Running on Llama 3.1 8B..." | Selects best provider for that model |
| Response with latency badge | Routed through Xergon P2P network |
| "Credits: 49 left" | ERG settlement queued for provider |
| "Buy more credits" | Fiat payment via Stripe, ERG bought and distributed server-side |
| Error: "Service unavailable" | No providers online for that model |

---

## 4. Technical Architecture

### Frontend Stack
- Next.js (SSR for SEO, fast initial load)
- Tailwind CSS (rapid UI iteration)
- Zustand (state: auth, credits, model selection)
- Server-Sent Events or WebSocket for streaming responses

### Backend (Marketplace Relay)
- Acts as proxy between users and Xergon providers
- Handles: auth, credit management, provider selection, fiat payments, usage tracking
- API: OpenAI-compatible (`/v1/chat/completions`) for programmatic access
- Stripe integration for fiat payments
- Periodic ERG settlement job: aggregate provider earnings, batch ERG transfers

### ERG Settlement (Server-Side, Invisible)
```
User pays $10 via Stripe
  -> Credits added to user account (USD-denominated)
  -> User makes inference requests
  -> Backend tracks per-provider usage
  -> Nightly job: convert provider earnings to ERG
  -> ERG sent to provider's Ergo address
  -> Provider never needs to interact with the marketplace frontend
```

### Data Model
```
User {
  id, email, password_hash, credits_usd, created_at
}

Usage {
  id, user_id, provider_id, model, tokens_in, tokens_out, cost_usd, created_at
}

ProviderSession {
  id, provider_id, model, requests_handled, tokens_served, earned_erg, settled
}

CreditTransaction {
  id, user_id, amount_usd, type (purchase|usage|refund), reference
}
```

---

## 5. OpenAI-Compatible API

For developers who want programmatic access:

```
POST /v1/chat/completions
Authorization: Bearer xergon_sk_...

{
  "model": "llama-3.1-8b",
  "messages": [{"role": "user", "content": "Hello"}],
  "stream": true
}
```

Same invisible-blockchain experience. API key = authentication. Credits deducted per request. ERG never mentioned in docs or API responses.

---

## 6. Messaging & Copy Guidelines

### SAY:
- "Run AI models on distributed GPUs"
- "Your data stays private — inference runs on independent nodes"
- "Pay as you go with credits"
- "10 free requests every day"

### NEVER SAY:
- "Blockchain" / "Web3" / "dApp"
- "ERG" / "Ergo" / "crypto" (except in Advanced settings)
- "Smart contract" / "decentralized" (say "distributed" instead)
- "Wallet" / "connect wallet" / "sign transaction"
- "Token" (unless referring to AI tokens)

### Privacy Messaging:
- "Your prompts are processed on independent nodes and never stored"
- "No central server sees your data"
- "Providers verify compute via cryptographic proofs" (this is the one blockchain-adjacent thing we mention, because it's a selling point)

---

## 7. Success Metrics

| Metric | Target |
|--------|--------|
| Time to first inference (new user) | < 60 seconds |
| Wallet connection rate | < 2% of users (most never see it) |
| Free tier → paid conversion | > 5% within 7 days |
| Provider selection latency | < 200ms |
| End-to-end inference latency | < 5s for short prompts |

---

## 8. Implementation Priority

1. **P0 — Playground MVP**: Anonymous free tier, single model, no auth, basic streaming
2. **P0 — Smart routing backend**: Provider selection, fallback chain, usage tracking
3. **P1 — Auth + credits**: Email signup, credit purchase via Stripe, rate limiting
4. **P1 — Model catalog**: Multiple models, model cards, model-specific pricing
5. **P2 — API keys**: OpenAI-compatible endpoint for developers
6. **P2 — Dashboard**: Usage analytics, spending history
7. **P3 — Advanced settings**: Ergo wallet linking (for provider operators)
8. **P3 — Provider dashboard**: Separate view for operators to monitor earnings, PoNW score

---

## 9. Relationship to XER-9 (ERG Payment Rail)

XER-9 designed the server-side ERG settlement. This spec builds the user-facing layer on top:

- XER-9: "Credits settle in ERG on-chain periodically" — this spec implements the "credits" part users see
- XER-9: "User never sees ERG unless they choose to" — this spec defines when/how that choice appears
- XER-9: "Research: subscription vs pay-per-request vs prepaid credits" — this spec chooses **prepaid credits + monthly subscription** as the two payment models
