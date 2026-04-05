/**
 * XergonClient -- the main entry point for the Xergon SDK.
 *
 * Provides a fluent API surface covering all relay endpoints:
 * chat completions, models, providers, balance, GPU Bazar,
 * incentive system, bridge, and health probes.
 *
 * @example
 * ```ts
 * import { XergonClient } from '@xergon/sdk';
 *
 * const client = new XergonClient({
 *   baseUrl: 'https://relay.xergon.gg',
 *   publicKey: '0x...',
 *   privateKey: '0x...',
 * });
 *
 * const models = await client.models.list();
 * const completion = await client.chat.completions.create({
 *   model: 'llama-3.3-70b',
 *   messages: [{ role: 'user', content: 'Hello!' }],
 * });
 *
 * for await (const chunk of await client.chat.completions.stream({
 *   model: 'llama-3.3-70b',
 *   messages: [{ role: 'user', content: 'Hello!' }],
 * })) {
 *   process.stdout.write(chunk.choices[0]?.delta?.content ?? '');
 * }
 * ```
 */

import { XergonClientCore } from './client';
import type { XergonClientConfig, LogInterceptor } from './types';

// Re-export all types
export type {
  ChatRole,
  ChatMessage,
  ChatCompletionParams,
  ChatCompletionResponse,
  ChatCompletionChunk,
  ChatCompletionUsage,
  ChatCompletionChoice,
  ChatCompletionDelta,
  ChatCompletionChunkChoice,
  Model,
  ModelsResponse,
  Provider,
  LeaderboardEntry,
  BalanceResponse,
  GpuListing,
  GpuRental,
  GpuPricingEntry,
  GpuFilters,
  RateGpuParams,
  GpuReputation,
  IncentiveStatus,
  RareModel,
  RareModelDetail,
  BridgeChain,
  BridgeInvoiceStatus,
  BridgeInvoice,
  BridgeStatus,
  HealthResponse,
  AuthStatus,
  XergonClientConfig,
  LogInterceptor,
} from './types';

// Re-export errors
export { XergonError } from './errors';
export type { XergonErrorType, XergonErrorBody } from './errors';

// Re-export auth helpers
export { hmacSign, hmacVerify, buildHmacPayload } from './auth';

// Import API modules
import { createChatCompletion, streamChatCompletion } from './chat';
import { listModels } from './models';
import { listProviders, getLeaderboard } from './providers';
import { getBalance } from './balance';
import {
  listGpuListings,
  getGpuListing,
  rentGpu,
  getMyRentals,
  getGpuPricing,
  rateGpu,
  getGpuReputation,
} from './gpu';
import {
  getIncentiveStatus,
  getIncentiveModels,
  getIncentiveModelDetail,
} from './incentive';
import {
  getBridgeStatus,
  getBridgeInvoices,
  getBridgeInvoice,
  createBridgeInvoice,
  confirmBridgePayment,
  refundBridgeInvoice,
} from './bridge';
import { healthCheck, readyCheck } from './health';

export class XergonClient {
  private core: XergonClientCore;

  constructor(config: XergonClientConfig = {}) {
    this.core = new XergonClientCore(config);
  }

  // ── Auth ─────────────────────────────────────────────────────────────

  /**
   * Set full keypair for HMAC authentication.
   */
  authenticate(publicKey: string, privateKey: string): void {
    this.core.authenticate(publicKey, privateKey);
  }

  /**
   * Set only the public key (for Nautilus / wallet-managed signing).
   */
  setPublicKey(pk: string): void {
    this.core.setPublicKey(pk);
  }

  /**
   * Clear all credentials.
   */
  clearAuth(): void {
    this.core.clearAuth();
  }

  getPublicKey(): string | null {
    return this.core.getPublicKey();
  }

  getBaseUrl(): string {
    return this.core.getBaseUrl();
  }

  /**
   * Add a log interceptor for request/response events.
   */
  addInterceptor(fn: LogInterceptor): void {
    this.core.addInterceptor(fn);
  }

  /**
   * Remove a log interceptor.
   */
  removeInterceptor(fn: LogInterceptor): void {
    this.core.removeInterceptor(fn);
  }

  /**
   * Verify authentication with the relay.
   */
  async authStatus(): Promise<import('./types').AuthStatus> {
    return this.core.get<import('./types').AuthStatus>('/v1/auth/status');
  }

  // ── Chat (OpenAI-compatible) ─────────────────────────────────────────

  readonly chat = {
    completions: {
      /**
       * Create a chat completion (non-streaming).
       */
      create: (params: import('./types').ChatCompletionParams, options?: { signal?: AbortSignal }) =>
        createChatCompletion(this.core, params, options),

      /**
       * Stream a chat completion via SSE.
       * Returns an AsyncIterable of ChatCompletionChunk.
       */
      stream: (params: import('./types').ChatCompletionParams, options?: { signal?: AbortSignal }) =>
        streamChatCompletion(this.core, params, options),
    },
  };

  // ── Models ───────────────────────────────────────────────────────────

  readonly models = {
    /**
     * List all available models.
     */
    list: () => listModels(this.core),
  };

  // ── Providers ────────────────────────────────────────────────────────

  readonly providers = {
    /**
     * List all active providers.
     */
    list: () => listProviders(this.core),
  };

  /**
   * Get provider leaderboard ranked by PoNW score.
   */
  leaderboard = (params?: { limit?: number; offset?: number }) =>
    getLeaderboard(this.core, params);

  // ── Balance ──────────────────────────────────────────────────────────

  readonly balance = {
    /**
     * Get user's ERG balance from their on-chain Staking Box.
     */
    get: (userPk: string) => getBalance(this.core, userPk),
  };

  // ── GPU Bazar ────────────────────────────────────────────────────────

  readonly gpu = {
    /**
     * Browse GPU listings with optional filters.
     */
    listings: (filters?: import('./types').GpuFilters) =>
      listGpuListings(this.core, filters),

    /**
     * Get details for a specific GPU listing.
     */
    getListing: (id: string) => getGpuListing(this.core, id),

    /**
     * Rent a GPU for a given number of hours.
     */
    rent: (listingId: string, hours: number) =>
      rentGpu(this.core, listingId, hours),

    /**
     * Get a user's active rentals.
     */
    myRentals: (renterPk: string) => getMyRentals(this.core, renterPk),

    /**
     * Get GPU pricing information.
     */
    pricing: () => getGpuPricing(this.core),

    /**
     * Rate a GPU provider or renter.
     */
    rate: (params: import('./types').RateGpuParams) =>
      rateGpu(this.core, params),

    /**
     * Get reputation score for a public key.
     */
    reputation: (publicKey: string) =>
      getGpuReputation(this.core, publicKey),
  };

  // ── Incentive ────────────────────────────────────────────────────────

  readonly incentive = {
    /**
     * Get incentive system status.
     */
    status: () => getIncentiveStatus(this.core),

    /**
     * Get all rare models with bonus information.
     */
    models: () => getIncentiveModels(this.core),

    /**
     * Get detailed rarity information for a specific model.
     */
    modelDetail: (model: string) => getIncentiveModelDetail(this.core, model),
  };

  // ── Bridge ───────────────────────────────────────────────────────────

  readonly bridge = {
    /**
     * Get bridge operational status.
     */
    status: () => getBridgeStatus(this.core),

    /**
     * List all invoices for the authenticated user.
     */
    invoices: () => getBridgeInvoices(this.core),

    /**
     * Get details for a specific invoice.
     */
    getInvoice: (id: string) => getBridgeInvoice(this.core, id),

    /**
     * Create a new payment invoice.
     */
    createInvoice: (amountNanoerg: string, chain: 'btc' | 'eth' | 'ada') =>
      createBridgeInvoice(this.core, amountNanoerg, chain),

    /**
     * Confirm a payment for an invoice.
     */
    confirm: (invoiceId: string, txHash: string) =>
      confirmBridgePayment(this.core, invoiceId, txHash),

    /**
     * Request a refund for an invoice.
     */
    refund: (invoiceId: string) =>
      refundBridgeInvoice(this.core, invoiceId),
  };

  // ── Health ───────────────────────────────────────────────────────────

  readonly health = {
    /**
     * Liveness probe -- is the relay process running?
     */
    check: () => healthCheck(this.core),
  };

  readonly ready = {
    /**
     * Readiness probe -- can the relay serve requests?
     */
    check: () => readyCheck(this.core),
  };
}
