# Changelog

All notable changes to `@xergon/sdk` will be documented in this file.

## [0.1.0] - 2025-04-05

### Added
- Initial release of the Xergon TypeScript SDK.
- `XergonClient` class with fluent API for all relay endpoints.
- **Chat Completions**: `client.chat.completions.create()` and `client.chat.completions.stream()` with SSE support via `AsyncIterable`.
- **Models**: `client.models.list()` -- list all available models from active providers.
- **Providers**: `client.providers.list()` and `client.leaderboard()` -- provider discovery and PoNW ranking.
- **Balance**: `client.balance.get(userPk)` -- on-chain Staking Box ERG balance.
- **GPU Bazar**: Full marketplace support -- `listings`, `getListing`, `rent`, `myRentals`, `pricing`, `rate`, `reputation`.
- **Incentive System**: `status`, `models`, `modelDetail` -- rare model bonus queries.
- **Cross-Chain Bridge**: `status`, `invoices`, `getInvoice`, `createInvoice`, `confirm`, `refund` -- BTC/ETH/ADA payment bridge.
- **Health Probes**: `client.health.check()` (liveness) and `client.ready.check()` (readiness).
- **Auth**: `client.authStatus()` for verifying HMAC authentication with the relay.
- **HMAC Authentication**: `hmacSign`, `hmacVerify`, `buildHmacPayload` using Web Crypto API (works in browsers and Node.js 20+).
- **Nautilus Wallet Integration** (`@xergon/sdk/wallet`): `connectNautilus`, `disconnectNautilus`, `signMessage`, `getBalance`, `getUtxos`, `signTx`, `submitTx`, `signAndSubmit`.
- **EIP-12 Types**: Full TypeScript definitions for the Ergo dApp connector protocol.
- **Error Handling**: `XergonError` class with typed error types (`XergonErrorType`), convenience predicates (`isUnauthorized`, `isRateLimited`, `isNotFound`, `isServiceUnavailable`).
- **Log Interceptors**: `client.addInterceptor()` / `client.removeInterceptor()` for request/response logging.
- **AbortSignal Support**: All chat completion methods support request cancellation.
- **Zero Dependencies**: The SDK has no runtime dependencies beyond `fetch` (available in Node.js 20+ and all modern browsers).
- TypeScript types for all request/response interfaces matching the relay OpenAPI spec.
- Snake_case to camelCase conversion for idiomatic JavaScript.
