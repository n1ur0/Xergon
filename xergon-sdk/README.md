# @xergon/sdk

TypeScript SDK for the Xergon Network -- decentralized AI inference relay on Ergo.

Provides a fluent, type-safe client covering all relay endpoints: chat completions (OpenAI-compatible), model discovery, provider leaderboard, balance queries, GPU Bazar marketplace, incentive system, cross-chain bridge, and health probes.

## Installation

```bash
npm install @xergon/sdk
```

### Local development (monorepo)

The marketplace references the SDK as a local package:

```json
{
  "dependencies": {
    "@xergon/sdk": "file:../xergon-sdk"
  }
}
```

## Quick Start

```typescript
import { XergonClient } from '@xergon/sdk';

const client = new XergonClient({
  baseUrl: 'https://relay.xergon.gg',
  publicKey: '0x...',       // Ergo public key (hex)
  privateKey: '0x...',      // Ergo private key (hex) for HMAC signing
});

// List available models
const models = await client.models.list();
console.log(models);

// Non-streaming chat completion
const completion = await client.chat.completions.create({
  model: 'llama-3.3-70b',
  messages: [{ role: 'user', content: 'Hello, Xergon!' }],
});
console.log(completion.choices[0].message.content);

// Streaming chat completion
const stream = await client.chat.completions.stream({
  model: 'llama-3.3-70b',
  messages: [{ role: 'user', content: 'Tell me a story' }],
});

for await (const chunk of stream) {
  const delta = chunk.choices[0]?.delta?.content ?? '';
  process.stdout.write(delta);
}
```

## Configuration

### XergonClientConfig

| Option       | Type     | Default                  | Description                        |
|--------------|----------|--------------------------|------------------------------------|
| `baseUrl`    | `string` | `https://relay.xergon.gg` | Relay base URL                    |
| `publicKey`  | `string` | --                       | Ergo public key (hex)             |
| `privateKey` | `string` | --                       | Ergo private key (hex) for HMAC   |

### Runtime Authentication

You can also set credentials after construction:

```typescript
const client = new XergonClient();

// Full keypair for HMAC auth
client.authenticate(publicKey, privateKey);

// Public key only (for Nautilus / wallet-managed signing)
client.setPublicKey(pk);

// Clear credentials
client.clearAuth();
```

### Log Interceptors

Add request/response logging:

```typescript
client.addInterceptor((event) => {
  console.log(`${event.method} ${event.url} -> ${event.status} (${event.durationMs}ms)`);
  if (event.error) console.error('Error:', event.error);
});

client.removeInterceptor(myInterceptor);
```

## API Reference

### Chat Completions

OpenAI-compatible inference endpoints.

#### `client.chat.completions.create(params, options?)`

Non-streaming chat completion.

```typescript
const response: ChatCompletionResponse = await client.chat.completions.create({
  model: 'llama-3.3-70b',
  messages: [
    { role: 'system', content: 'You are a helpful assistant.' },
    { role: 'user', content: 'Explain proof-of-work.' },
  ],
  maxTokens: 2048,
  temperature: 0.7,
  topP: 1,
}, { signal: abortController.signal });
```

#### `client.chat.completions.stream(params, options?)`

Streaming chat completion via SSE. Returns an `AsyncIterable<ChatCompletionChunk>`.

```typescript
const stream = await client.chat.completions.stream({
  model: 'llama-3.3-70b',
  messages: [{ role: 'user', content: 'Hello!' }],
});

for await (const chunk of stream) {
  const content = chunk.choices[0]?.delta?.content ?? '';
  process.stdout.write(content);
}
```

Supports `AbortSignal` for cancellation.

### Models

#### `client.models.list()`

List all available models from active providers.

```typescript
const models: Model[] = await client.models.list();
// [{ id: 'llama-3.3-70b', object: 'model', ownedBy: 'provider-1', pricing: '50000' }]
```

### Providers

#### `client.providers.list()`

List all active inference providers.

```typescript
const providers: Provider[] = await client.providers.list();
// [{ publicKey: '0x...', endpoint: '...', models: [...], region: 'us-east', pownScore: 95 }]
```

#### `client.leaderboard(params?)`

Get provider leaderboard ranked by PoNW score.

```typescript
const entries: LeaderboardEntry[] = await client.leaderboard({ limit: 10, offset: 0 });
```

### Balance

#### `client.balance.get(userPk)`

Get user's ERG balance from their on-chain Staking Box.

```typescript
const balance: BalanceResponse = await client.balance.get('0x...');
// { publicKey: '0x...', balanceNanoerg: '1000000000', balanceErg: '1.0', stakingBoxId: '...' }
```

### GPU Bazar

#### `client.gpu.listings(filters?)`

Browse GPU listings with optional filters.

```typescript
const listings: GpuListing[] = await client.gpu.listings({
  gpuType: 'A100',
  minVram: 40,
  maxPrice: 500,
  region: 'us-east',
});
```

#### `client.gpu.getListing(id)`

Get details for a specific GPU listing.

```typescript
const listing: GpuListing = await client.gpu.getListing('listing-123');
```

#### `client.gpu.rent(listingId, hours)`

Rent a GPU for a given number of hours.

```typescript
const rental: GpuRental = await client.gpu.rent('listing-123', 24);
```

#### `client.gpu.myRentals(renterPk)`

Get a user's active rentals.

```typescript
const rentals: GpuRental[] = await client.gpu.myRentals('0x...');
```

#### `client.gpu.pricing()`

Get GPU pricing information.

```typescript
const pricing: GpuPricingEntry[] = await client.gpu.pricing();
```

#### `client.gpu.rate(params)`

Rate a GPU provider or renter (score 1-5).

```typescript
await client.gpu.rate({
  targetPk: '0x...',
  rentalId: 'rental-123',
  score: 5,
  comment: 'Great uptime and speed!',
});
```

#### `client.gpu.reputation(publicKey)`

Get reputation score for a public key.

```typescript
const rep: GpuReputation = await client.gpu.reputation('0x...');
```

### Incentive System

#### `client.incentive.status()`

Get incentive system status.

```typescript
const status: IncentiveStatus = await client.incentive.status();
// { active: true, totalBonusErg: '1000.0', rareModelsCount: 5 }
```

#### `client.incentive.models()`

Get all rare models with bonus information.

```typescript
const models: RareModel[] = await client.incentive.models();
```

#### `client.incentive.modelDetail(model)`

Get detailed rarity information for a specific model.

```typescript
const detail: RareModelDetail = await client.incentive.modelDetail('qwen3.5-32b');
```

### Cross-Chain Bridge

#### `client.bridge.status()`

Get bridge operational status.

```typescript
const status: BridgeStatus = await client.bridge.status();
// { status: 'operational', supportedChains: ['btc', 'eth', 'ada'] }
```

#### `client.bridge.invoices()`

List all invoices for the authenticated user.

```typescript
const invoices: BridgeInvoice[] = await client.bridge.invoices();
```

#### `client.bridge.getInvoice(id)`

Get details for a specific invoice.

```typescript
const invoice: BridgeInvoice = await client.bridge.getInvoice('inv-123');
```

#### `client.bridge.createInvoice(amountNanoerg, chain)`

Create a new payment invoice.

```typescript
const invoice: BridgeInvoice = await client.bridge.createInvoice('1000000000', 'btc');
```

#### `client.bridge.confirm(invoiceId, txHash)`

Confirm a payment for an invoice.

```typescript
await client.bridge.confirm('inv-123', '0xtxhash...');
```

#### `client.bridge.refund(invoiceId)`

Request a refund for an invoice.

```typescript
await client.bridge.refund('inv-123');
```

### Health

#### `client.health.check()`

Liveness probe -- is the relay process running?

```typescript
const alive: boolean = await client.health.check();
```

#### `client.ready.check()`

Readiness probe -- can the relay serve requests?

```typescript
const ready: boolean = await client.ready.check();
```

### Contracts (On-Chain Operations)

Agent-mediated contract methods for interacting with Xergon on-chain contracts (provider registration, staking, settlement, governance, oracle).

#### `client.contracts.registerProvider(params)`

Register a new provider on-chain. Creates a Provider Box with NFT + metadata.

```typescript
const result = await client.contracts.registerProvider({
  providerName: 'MyGPU',
  region: 'US',
  endpoint: 'https://gpu.example.com',
  models: ['llama-3.3-70b', 'mistral-small-24b'],
  ergoAddress: '9eZ24...',
  providerPkHex: 'abc123...',  // 32-byte public key hex (64 chars)
});
console.log(`Registered! NFT: ${result.providerNftId}, Box: ${result.providerBoxId}`);
```

#### `client.contracts.queryProviderStatus(providerNftId)`

Query a registered provider's current on-chain status by NFT token ID.

```typescript
const status = await client.contracts.queryProviderStatus(nftId);
console.log(`Provider: ${status.providerName}`);
console.log(`Price: ${status.pricePerToken} nanoERG/token`);
console.log(`Confirmations: ${status.confirmations}`);
```

#### `client.contracts.listOnChainProviders()`

List all on-chain providers by scanning the UTXO set.

```typescript
const providers = await client.contracts.listOnChainProviders();
for (const p of providers) {
  console.log(`${p.providerName} (${p.region}): ${p.models.join(', ')}`);
}
```

#### `client.contracts.createStakingBox(params)`

Create a User Staking Box to lock ERG for inference payments.

```typescript
const result = await client.contracts.createStakingBox({
  userPkHex: 'abc123...',     // 32-byte public key hex
  amountNanoerg: 5_000_000_000n,  // 5 ERG
});
console.log(`Staked in box: ${result.stakingBoxId}`);
```

#### `client.contracts.queryUserBalance(userPkHex)`

Query a user's total ERG balance across all staking boxes.

```typescript
const balance = await client.contracts.queryUserBalance(userPk);
console.log(`Balance: ${Number(balance.totalBalanceNanoerg) / 1e9} ERG`);
console.log(`Boxes: ${balance.stakingBoxCount}`);
```

#### `client.contracts.getUserStakingBoxes(userPkHex)`

Get all staking box details for a user.

```typescript
const boxes = await client.contracts.getUserStakingBoxes(userPk);
for (const box of boxes) {
  console.log(`Box ${box.boxId}: ${Number(box.valueNanoerg) / 1e9} ERG`);
}
```

#### `client.contracts.getSettleableBoxes(maxBoxes?)`

Get staking boxes ready for settlement (accumulated fees exceed threshold).

```typescript
const boxes = await client.contracts.getSettleableBoxes(20);
const totalFees = boxes.reduce((sum, b) => sum + b.feeAmountNanoerg, 0n);
console.log(`${boxes.length} boxes with ${Number(totalFees) / 1e9} ERG in fees`);
```

#### `client.contracts.buildSettlementTx(params)`

Build a settlement transaction for providers to claim accumulated fees.

```typescript
const result = await client.contracts.buildSettlementTx({
  stakingBoxIds: ['box1', 'box2'],
  feeAmounts: [500_000n, 300_000n],
  providerAddress: '9eZ24...',
  maxFeeNanoerg: 1_100_000n,
});
// Sign result.unsignedTx with Nautilus, then broadcast
```

#### `client.contracts.getOracleRate()`

Get the current ERG/USD rate from the oracle pool.

```typescript
const rate = await client.contracts.getOracleRate();
console.log(`ERG/USD: $${rate.ergUsd.toFixed(4)} (epoch ${rate.epoch})`);
```

#### `client.contracts.getOraclePoolStatus()`

Get detailed oracle pool status including epoch, box ID, and update height.

```typescript
const status = await client.contracts.getOraclePoolStatus();
console.log(`Epoch ${status.epoch}, Rate: $${status.ergUsd.toFixed(4)}`);
console.log(`Last update at block ${status.lastUpdateHeight}`);
```

### Auth Status

#### `client.authStatus()`

Verify authentication with the relay.

```typescript
const status: AuthStatus = await client.authStatus();
// { authenticated: true, publicKey: '0x...', tier: 'standard' }
```

## Wallet Integration

The SDK includes a `@xergon/sdk/wallet` subpath export for Nautilus Ergo wallet integration:

```typescript
import {
  isNautilusAvailable,
  connectNautilus,
  disconnectNautilus,
  signMessage,
  getBalance,
  getUtxos,
  signTx,
  submitTx,
  signAndSubmit,
} from '@xergon/sdk/wallet';

// Check if Nautilus is installed
if (isNautilusAvailable()) {
  // Connect and get the change address
  const address = await connectNautilus();

  // Sign a message
  const sig = await signMessage('Hello Xergon');

  // Get ERG balance
  const erg = await getBalance();

  // Disconnect
  await disconnectNautilus();
}
```

### HMAC Helpers

```typescript
import { hmacSign, hmacVerify, buildHmacPayload } from '@xergon/sdk';

const payload = buildHmacPayload(JSON.stringify(body), timestamp);
const signature = await hmacSign(payload, privateKeyHex);
const valid = await hmacVerify(payload, signature, privateKeyHex);
```

## TypeScript Type Reference

### Chat Types

```typescript
type ChatRole = 'system' | 'user' | 'assistant';

interface ChatMessage {
  role: ChatRole;
  content: string;
}

interface ChatCompletionParams {
  model: string;
  messages: ChatMessage[];
  maxTokens?: number;
  temperature?: number;
  topP?: number;
  stream?: boolean;
}

interface ChatCompletionResponse {
  id: string;
  object: 'chat.completion';
  created: number;
  model: string;
  choices: ChatCompletionChoice[];
  usage?: ChatCompletionUsage;
}

interface ChatCompletionChoice {
  index: number;
  message: ChatMessage;
  finishReason: 'stop' | 'length' | 'content_filter';
}

interface ChatCompletionUsage {
  promptTokens: number;
  completionTokens: number;
  totalTokens: number;
}

interface ChatCompletionChunk {
  id: string;
  object: 'chat.completion.chunk';
  created: number;
  model: string;
  choices: ChatCompletionChunkChoice[];
}

interface ChatCompletionChunkChoice {
  index: number;
  delta: ChatCompletionDelta;
  finishReason: 'stop' | 'length' | 'content_filter' | null;
}

interface ChatCompletionDelta {
  role?: ChatRole;
  content?: string;
}
```

### Model Types

```typescript
interface Model {
  id: string;
  object: string;
  ownedBy: string;
  pricing?: string;  // Cost in nanoERG per 1K tokens
}

interface ModelsResponse {
  object: string;
  data: Model[];
}
```

### Provider Types

```typescript
interface Provider {
  publicKey: string;
  endpoint: string;
  models: string[];
  region: string;
  pownScore: number;
  lastHeartbeat?: number;
  pricing?: Record<string, string>;
}

interface LeaderboardEntry extends Provider {
  online?: boolean;
  totalRequests?: number;
  totalPromptTokens?: number;
  totalCompletionTokens?: number;
  totalTokens?: number;
}
```

### Balance Types

```typescript
interface BalanceResponse {
  publicKey: string;
  balanceNanoerg: string;
  balanceErg: string;
  stakingBoxId?: string;
}
```

### GPU Bazar Types

```typescript
interface GpuListing {
  listingId: string;
  providerPk: string;
  gpuType: string;
  vramGb?: number;
  pricePerHourNanoerg: string;
  region: string;
  available: boolean;
  bandwidthMbps?: number;
}

interface GpuRental {
  rentalId: string;
  listingId: string;
  providerPk: string;
  renterPk: string;
  hours: number;
  costNanoerg: string;
  startedAt: number;
  expiresAt: number;
  status: 'active' | 'expired' | 'completed';
}

interface GpuPricingEntry {
  gpuType: string;
  avgPricePerHourNanoerg: string;
  minPricePerHourNanoerg?: string;
  maxPricePerHourNanoerg?: string;
  listingCount?: number;
}

interface GpuFilters {
  gpuType?: string;
  minVram?: number;
  maxPrice?: number;
  region?: string;
}

interface RateGpuParams {
  targetPk: string;
  rentalId: string;
  score: number;     // 1-5
  comment?: string;
}

interface GpuReputation {
  publicKey: string;
  score: number;
  totalRatings: number;
  average: number;
}
```

### Incentive Types

```typescript
interface IncentiveStatus {
  active: boolean;
  totalBonusErg: string;
  rareModelsCount: number;
}

interface RareModel {
  model: string;
  rarityScore: number;
  bonusMultiplier: number;
  providersCount: number;
}

interface RareModelDetail extends RareModel {
  recentRequests?: number;
  bonusErgAccumulated?: string;
}
```

### Bridge Types

```typescript
type BridgeChain = 'btc' | 'eth' | 'ada';
type BridgeInvoiceStatus = 'pending' | 'confirmed' | 'refunded' | 'expired';

interface BridgeInvoice {
  invoiceId: string;
  amountNanoerg: string;
  chain: BridgeChain;
  status: BridgeInvoiceStatus;
  createdAt: number;
  refundTimeout: number;
}

interface BridgeStatus {
  status: string;
  supportedChains: string[];
}
```

### Health & Auth Types

```typescript
interface HealthResponse {
  status: string;
  version?: string;
  uptimeSecs?: number;
  ergoNodeConnected?: boolean;
  activeProviders?: number;
  totalProviders?: number;
}

interface AuthStatus {
  authenticated: boolean;
  publicKey: string;
  tier: string;  // 'trial' | 'basic' | 'standard' | 'premium' | 'provider'
}
```

## Error Handling

All SDK errors are thrown as `XergonError` instances:

```typescript
import { XergonError } from '@xergon/sdk';

try {
  const completion = await client.chat.completions.create({ ... });
} catch (err) {
  if (err instanceof XergonError) {
    console.error(`Error type: ${err.type}`);      // e.g., 'rate_limit_error'
    console.error(`HTTP code: ${err.code}`);        // e.g., 429
    console.error(`Message: ${err.message}`);

    // Convenience predicates
    if (err.isUnauthorized) { /* re-auth */ }
    if (err.isRateLimited) { /* backoff */ }
    if (err.isNotFound) { /* handle missing */ }
    if (err.isServiceUnavailable) { /* retry */ }
  }
}
```

### Error Types

| Type                  | Code | Description                     |
|-----------------------|------|---------------------------------|
| `invalid_request`     | 400  | Malformed request body/params   |
| `unauthorized`        | 401  | Invalid or missing HMAC sig     |
| `forbidden`           | 403  | Insufficient permissions        |
| `not_found`           | 404  | Resource not found              |
| `rate_limit_error`    | 429  | Rate limit exceeded             |
| `internal_error`      | 500  | Relay internal error            |
| `service_unavailable` | 503  | No providers available          |

## Rate Limiting

The relay enforces rate limits per tier:

| Tier     | Requests/min |
|----------|-------------|
| trial    | 5           |
| basic    | 20          |
| standard | 60          |
| premium  | 200         |
| provider | 600         |

Rate-limited responses include headers:
- `X-RateLimit-Limit` -- max requests per window
- `X-RateLimit-Remaining` -- remaining requests
- `X-RateLimit-Reset` -- seconds until reset
- `Retry-After` -- seconds to wait (on 429)

## Authentication

The SDK uses HMAC-SHA256 authentication. Three headers are sent with each request:

1. `X-Xergon-Public-Key` -- User's Ergo public key (hex)
2. `X-Xergon-Timestamp` -- Unix timestamp (seconds)
3. `X-Xergon-Signature` -- HMAC-SHA256(body + timestamp, private_key)

When using Nautilus wallet integration, set only the public key and let the wallet handle signing.

## Examples

### Full Chat Completion with Streaming

```typescript
import { XergonClient, XergonError } from '@xergon/sdk';

const client = new XergonClient({
  baseUrl: 'https://relay.xergon.gg',
  publicKey: process.env.XERGON_PK!,
  privateKey: process.env.XERGON_SK!,
});

const controller = new AbortController();

try {
  const stream = await client.chat.completions.stream(
    {
      model: 'qwen3.5-32b',
      messages: [
        { role: 'system', content: 'You are a concise coding assistant.' },
        { role: 'user', content: 'Write a Fibonacci function in Rust.' },
      ],
      maxTokens: 1024,
      temperature: 0.3,
    },
    { signal: controller.signal },
  );

  let fullResponse = '';
  for await (const chunk of stream) {
    const content = chunk.choices[0]?.delta?.content ?? '';
    fullResponse += content;
    process.stdout.write(content);
  }

  console.log(`\n\nTotal length: ${fullResponse.length}`);
} catch (err) {
  if (err instanceof XergonError && err.isRateLimited) {
    console.error('Rate limited. Retrying in 5s...');
  }
}
```

### Provider Dashboard Data Fetch

```typescript
const client = new XergonClient({ baseUrl: 'https://relay.xergon.gg' });

// Fetch leaderboard
const [topProviders, allProviders] = await Promise.all([
  client.leaderboard({ limit: 10 }),
  client.providers.list(),
]);

console.log(`Top 10 providers by PoNW score:`);
for (const entry of topProviders) {
  console.log(
    `  ${entry.publicKey.slice(0, 12)}... | PoNW: ${entry.pownScore} | ` +
    `Models: ${entry.models.length} | Region: ${entry.region}`,
  );
}

console.log(`\nTotal active providers: ${allProviders.length}`);
```

### Balance Check Before Inference

```typescript
import { XergonClient, XergonError } from '@xergon/sdk';

const client = new XergonClient({
  publicKey: process.env.XERGON_PK!,
  privateKey: process.env.XERGON_SK!,
});

const MIN_BALANCE_ERG = '0.01';

async function inferenceWithBalanceCheck(prompt: string) {
  // 1. Check balance
  const balance = await client.balance.get(client.getPublicKey()!);
  if (parseFloat(balance.balanceErg) < parseFloat(MIN_BALANCE_ERG)) {
    throw new Error(`Insufficient balance: ${balance.balanceErg} ERG`);
  }

  // 2. Check relay readiness
  if (!(await client.ready.check())) {
    throw new Error('Relay is not ready to serve requests');
  }

  // 3. Run inference
  const response = await client.chat.completions.create({
    model: 'llama-3.3-70b',
    messages: [{ role: 'user', content: prompt }],
  });

  return response.choices[0].message.content;
}

try {
  const answer = await inferenceWithBalanceCheck('What is decentralized AI?');
  console.log(answer);
} catch (err) {
  if (err instanceof XergonError && err.isServiceUnavailable) {
    console.error('No providers available. Try again later.');
  }
}
```

## License

MIT
