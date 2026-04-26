/**
 * Server-only HTTP client for Xergon relay.
 * 
 * This module provides server-side API calls using plain fetch.
 * 
 * DO NOT import this in client components!
 */

export const API_BASE = (process.env.NEXT_PUBLIC_API_BASE || 'http://127.0.0.1:9090') + '/v1';
export const RELAY_BASE = API_BASE;

// Re-export types (these are just type definitions, safe to export)
// Note: We're not importing from @xergon/sdk to avoid client component bundling
export type ChatRole = 'user' | 'assistant' | 'system';
export interface ChatMessage { role: ChatRole; content: string; }
export interface ChatCompletionParams { model: string; messages: ChatMessage[]; max_tokens?: number; maxTokens?: number; temperature?: number; }
export interface ChatCompletionResponse { id: string; model: string; choices: Array<{ index: number; message: ChatMessage; }>; usage?: { prompt_tokens: number; completion_tokens: number; total_tokens: number; }; }
export interface Model { id: string; name: string; provider: string; price_per_input_token?: number; price_per_output_token?: number; ownedBy?: string; pricing?: string; }
export interface Provider { provider_id: string; endpoint: string; region: string; models: string[]; pown_score: number; }
export interface LeaderboardEntry { provider_id: string; total_tokens: number; latency_ms: number; }
export interface BalanceResponse { user_pk: string; balance_nanoerg: number; balance_erg: number; staking_boxes_count: number; sufficient: boolean; min_balance_nanoerg: number; }
export interface GpuListing { gpu_id: string; provider_id: string; model: string; price_per_hour: number; }
export interface GpuRental { listing_id: string; user_id: string; started_at: string; expires_at: string; }
export interface GpuPricingEntry { model: string; price_per_hour: number; }
export interface GpuFilters { model?: string; region?: string; min_vram?: number; }
export interface RateGpuParams { listing_id: string; rating: number; comment?: string; }
export interface GpuReputation { provider_id: string; average_rating: number; total_ratings: number; }
export interface IncentiveStatus { type: 'reward' | 'penalty'; amount_nanoerg: number; reason: string; }
export interface RareModel { model_id: string; provider_id: string; rarity: 'common' | 'rare' | 'epic' | 'legendary'; }
export interface BridgeChain { chain: 'btc' | 'eth' | 'ada'; invoice_id: string; amount: number; status: 'pending' | 'paid' | 'expired'; }
export interface BridgeInvoice { invoice_id: string; chain: string; amount: number; status: string; created_at: string; }
export interface BridgeStatus { status: string; message: string; }
export type XergonErrorType = 'auth' | 'rate_limit' | 'not_found' | 'server_error';
export interface XergonErrorBody { error: string; code: string; }

export class XergonError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'XergonError';
  }
}

/**
 * Extended SDK interface with API methods
 */
export interface XergonServerClientWithApi extends XergonServerClient {
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
 * Simple HTTP client for server-side API calls.
 */
export class XergonServerClient {
  private baseUrl: string;
  private apiKey?: string;

  constructor(options: { baseUrl?: string; apiKey?: string } = {}) {
    this.baseUrl = options.baseUrl || API_BASE;
    this.apiKey = options.apiKey;
  }

  getBaseUrl(): string {
    return this.baseUrl;
  }

  async get<T>(path: string, headers: Record<string, string> = {}): Promise<T> {
    const url = `${this.baseUrl}${path}`;
    const response = await fetch(url, {
      method: 'GET',
      headers: {
        'Content-Type': 'application/json',
        ...headers,
      },
    });

    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`);
    }

    return response.json();
  }

  async post<T>(path: string, data: unknown, headers: Record<string, string> = {}): Promise<T> {
    const url = `${this.baseUrl}${path}`;
    const response = await fetch(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...headers,
      },
      body: JSON.stringify(data),
    });

    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`);
    }

    return response.json();
  }
}

// Create a default instance with extended API
export const sdk: XergonServerClientWithApi = Object.assign(
  new XergonServerClient({ baseUrl: API_BASE }),
  {
    models: {
      list: async () => {
        const client = new XergonServerClient({ baseUrl: API_BASE });
        return client.get<Model[]>('/models');
      }
    },
    providers: {
      list: async () => {
        const client = new XergonServerClient({ baseUrl: API_BASE });
        return client.get<Provider[]>('/providers');
      }
    },
    balance: {
      get: async (userPk: string) => {
        const client = new XergonServerClient({ baseUrl: API_BASE });
        return client.get<BalanceResponse>(`/balance/${userPk}`);
      }
    },
    chat: {
      completions: {
        create: async (data: ChatCompletionParams) => {
          const client = new XergonServerClient({ baseUrl: API_BASE });
          return client.post<ChatCompletionResponse>('/chat/completions', data);
        }
      }
    },
    leaderboard: async () => {
      const client = new XergonServerClient({ baseUrl: API_BASE });
      return client.get<LeaderboardEntry[]>('/leaderboard');
    }
  }
);
