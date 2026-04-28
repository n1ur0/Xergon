/**
 * Server-only SDK client for Xergon relay.
 * 
 * Uses the real @xergon/sdk XergonClient with full HMAC auth,
 * retry logic, and interceptors.
 * 
 * DO NOT import this in client components!
 */

import { XergonClient } from '@xergon/sdk';
import type {
  ChatMessage,
  ChatCompletionParams,
  ChatCompletionResponse,
  Model,
  Provider,
  LeaderboardEntry,
  BalanceResponse,
} from '@xergon/sdk';

export const API_BASE = (process.env.NEXT_PUBLIC_API_BASE || 'http://127.0.0.1:9090') + '/v1';
export const RELAY_BASE = API_BASE;

// Re-export types from SDK (safe for use across the codebase)
export type { ChatMessage, ChatCompletionParams, ChatCompletionResponse, Model, Provider, LeaderboardEntry, BalanceResponse };

// Re-export SDK error class
export { XergonError } from '@xergon/sdk';
export type { XergonErrorType, XergonErrorBody } from '@xergon/sdk';

/**
 * Extended SDK interface with API methods used by marketplace
 */
export interface XergonServerClientWithApi extends XergonClient {
  models: {
    list: () => Promise<Model[]>;
  };
  providers: {
    list: () => Promise<Provider[]>;
  };
  balance: {
    get: (userPk: string) => Promise<BalanceResponse>;
  };
  chat: {
    completions: {
      create: (data: ChatCompletionParams) => Promise<ChatCompletionResponse>;
    };
  };
  leaderboard: () => Promise<LeaderboardEntry[]>;
}

/**
 * Create a relay SDK client instance.
 * 
 * For server-side use, pass the wallet public key for auth:
 *   const client = createRelayClient({ publicKey: walletPk });
 */
export function createRelayClient(options: { publicKey?: string; baseUrl?: string } = {}): XergonClient {
  return new XergonClient({
    baseUrl: options.baseUrl || API_BASE,
    publicKey: options.publicKey,
  });
}

// Default client instance (no auth - for public endpoints)
const defaultClient = createRelayClient();

/**
 * Default SDK instance with extended API methods.
 * 
 * Uses the real XergonClient under the hood with:
 * - HMAC-SHA256 request signing (when publicKey is set)
 * - Automatic retry with backoff
 * - Request/response interceptors
 * - Proper error handling
 */
export const sdk: XergonServerClientWithApi = Object.assign(
  defaultClient,
  {
    models: {
      list: () => defaultClient.models.list(),
    },
    providers: {
      list: () => defaultClient.providers.list(),
    },
    balance: {
      get: (userPk: string) => defaultClient.balance.get(userPk),
    },
    chat: {
      completions: {
        create: (data: ChatCompletionParams) => 
          defaultClient.chat.completions.create(data as Parameters<typeof defaultClient.chat.completions.create>[0]),
      },
    },
    leaderboard: () => defaultClient.leaderboard(),
  }
) as XergonServerClientWithApi;
