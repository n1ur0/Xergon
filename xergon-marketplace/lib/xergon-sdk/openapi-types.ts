/**
 * Auto-derived TypeScript types from the Xergon Relay OpenAPI spec (v1.0.0).
 *
 * These types mirror the wire format (snake_case) used by the relay API.
 * The SDK's own types (in @xergon/sdk) convert to camelCase.
 *
 * Generated from: xergon-relay/docs/openapi.yaml
 */

// ── Error Types ─────────────────────────────────────────────────────────

export type ApiErrorType =
  | 'invalid_request'
  | 'unauthorized'
  | 'forbidden'
  | 'not_found'
  | 'rate_limit_error'
  | 'internal_error'
  | 'service_unavailable';

export interface ApiErrorBody {
  type: ApiErrorType;
  message: string;
  code: number;
}

export interface ApiErrorResponse {
  error: ApiErrorBody;
}

// ── Chat Completions ────────────────────────────────────────────────────

export interface ApiChatMessage {
  role: 'system' | 'user' | 'assistant';
  content: string;
}

export interface ApiChatCompletionRequest {
  model: string;
  messages: ApiChatMessage[];
  max_tokens?: number;
  temperature?: number;
  top_p?: number;
  stream?: boolean;
}

export interface ApiChatCompletionChoice {
  index: number;
  message: ApiChatMessage;
  finish_reason: 'stop' | 'length' | 'content_filter';
}

export interface ApiChatCompletionUsage {
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
}

export interface ApiChatCompletionResponse {
  id: string;
  object: 'chat.completion';
  created: number;
  model: string;
  choices: ApiChatCompletionChoice[];
  usage?: ApiChatCompletionUsage;
}

// Streaming chunk (same shape, used with SSE)
export interface ApiChatCompletionChunkChoice {
  index: number;
  delta: Partial<ApiChatMessage>;
  finish_reason: 'stop' | 'length' | 'content_filter' | null;
}

export interface ApiChatCompletionChunk {
  id: string;
  object: 'chat.completion.chunk';
  created: number;
  model: string;
  choices: ApiChatCompletionChunkChoice[];
}

// ── Models ──────────────────────────────────────────────────────────────

export interface ApiModelEntry {
  id: string;
  object: string;
  owned_by: string;
  pricing?: string;
}

export interface ApiModelsResponse {
  object: string;
  data: ApiModelEntry[];
}

// ── Providers ───────────────────────────────────────────────────────────

export interface ApiProviderEntry {
  public_key: string;
  endpoint: string;
  models: string[];
  region: string;
  pown_score: number;
  last_heartbeat?: number;
  pricing?: Record<string, string>;
}

export interface ApiLeaderboardQueryParams {
  limit?: number;
  offset?: number;
}

// ── Balance ─────────────────────────────────────────────────────────────

export interface ApiBalanceResponse {
  public_key: string;
  balance_nanoerg: string;
  balance_erg: string;
  staking_box_id?: string;
}

// ── Auth ────────────────────────────────────────────────────────────────

export interface ApiAuthStatusResponse {
  authenticated: boolean;
  public_key: string;
  tier: string;
}

// ── GPU Bazar ───────────────────────────────────────────────────────────

export interface ApiGpuListing {
  listing_id: string;
  provider_pk: string;
  gpu_type: string;
  vram_gb?: number;
  price_per_hour_nanoerg: string;
  region: string;
  available: boolean;
  bandwidth_mbps?: number;
}

export interface ApiGpuListingsQueryParams {
  gpu_type?: string;
  min_vram?: number;
  max_price?: number;
  region?: string;
}

export interface ApiGpuRentRequest {
  listing_id: string;
  hours: number;
}

export interface ApiGpuRental {
  rental_id: string;
  listing_id: string;
  provider_pk: string;
  renter_pk: string;
  hours: number;
  cost_nanoerg: string;
  started_at: number;
  expires_at: number;
  status: 'active' | 'expired' | 'completed';
}

export interface ApiGpuPricingResponse {
  avg_price_per_hour: string;
  models: Record<string, string>;
}

export interface ApiGpuRateRequest {
  target_pk: string;
  rental_id: string;
  score: number;
  comment?: string;
}

export interface ApiGpuReputationResponse {
  public_key: string;
  score: number;
  total_ratings: number;
  average: number;
}

// ── Incentive ───────────────────────────────────────────────────────────

export interface ApiIncentiveStatusResponse {
  active: boolean;
  total_bonus_erg: string;
  rare_models_count: number;
}

export interface ApiRareModel {
  model: string;
  rarity_score: number;
  bonus_multiplier: number;
  providers_count: number;
}

// ── Bridge ──────────────────────────────────────────────────────────────

export type ApiBridgeChain = 'btc' | 'eth' | 'ada';
export type ApiBridgeInvoiceStatus = 'pending' | 'confirmed' | 'refunded' | 'expired';

export interface ApiBridgeStatusResponse {
  status: string;
  supported_chains: string[];
}

export interface ApiBridgeInvoice {
  invoice_id: string;
  amount_nanoerg: string;
  chain: ApiBridgeChain;
  status: ApiBridgeInvoiceStatus;
  created_at: number;
  refund_timeout: number;
}

export interface ApiCreateInvoiceRequest {
  amount_nanoerg: string;
  chain: ApiBridgeChain;
}

export interface ApiConfirmPaymentRequest {
  invoice_id: string;
  tx_hash: string;
}

export interface ApiRefundRequest {
  invoice_id: string;
}

// ── Health ──────────────────────────────────────────────────────────────

export interface ApiHealthResponse {
  status: string;
  version?: string;
  uptime_secs?: number;
  ergo_node_connected?: boolean;
  active_providers?: number;
  total_providers?: number;
}

// ── Rate Limit Headers ──────────────────────────────────────────────────

export interface ApiRateLimitHeaders {
  'X-RateLimit-Limit': number;
  'X-RateLimit-Remaining': number;
  'X-RateLimit-Reset': number;
  'Retry-After'?: number;
}

// ── Full API Endpoints Map ──────────────────────────────────────────────

/**
 * Maps each API endpoint to its request and response types.
 * Useful for type-safe API client generation.
 */
export interface ApiEndpointMap {
  // Inference
  'POST /v1/chat/completions': {
    request: ApiChatCompletionRequest;
    response: ApiChatCompletionResponse;
    stream: ApiChatCompletionChunk;
  };
  'GET /v1/models': {
    response: ApiModelsResponse;
  };

  // Network
  'GET /v1/leaderboard': {
    params: ApiLeaderboardQueryParams;
    response: ApiProviderEntry[];
  };
  'GET /v1/providers': {
    response: ApiProviderEntry[];
  };
  'GET /v1/balance/{user_pk}': {
    response: ApiBalanceResponse;
  };
  'GET /v1/auth/status': {
    response: ApiAuthStatusResponse;
  };

  // GPU Bazar
  'GET /v1/gpu/listings': {
    params: ApiGpuListingsQueryParams;
    response: ApiGpuListing[];
  };
  'GET /v1/gpu/listings/{listing_id}': {
    response: ApiGpuListing;
  };
  'POST /v1/gpu/rent': {
    request: ApiGpuRentRequest;
    response: ApiGpuRental;
  };
  'GET /v1/gpu/rentals/{renter_pk}': {
    response: ApiGpuRental[];
  };
  'GET /v1/gpu/pricing': {
    response: ApiGpuPricingResponse;
  };
  'POST /v1/gpu/rate': {
    request: ApiGpuRateRequest;
  };
  'GET /v1/gpu/reputation/{public_key}': {
    response: ApiGpuReputationResponse;
  };

  // Incentive
  'GET /v1/incentive/status': {
    response: ApiIncentiveStatusResponse;
  };
  'GET /v1/incentive/models': {
    response: ApiRareModel[];
  };
  'GET /v1/incentive/models/{model}': {
    response: ApiRareModel;
  };

  // Bridge
  'GET /v1/bridge/status': {
    response: ApiBridgeStatusResponse;
  };
  'GET /v1/bridge/invoices': {
    response: ApiBridgeInvoice[];
  };
  'GET /v1/bridge/invoice/{id}': {
    response: ApiBridgeInvoice;
  };
  'POST /v1/bridge/create-invoice': {
    request: ApiCreateInvoiceRequest;
    response: ApiBridgeInvoice;
  };
  'POST /v1/bridge/confirm': {
    request: ApiConfirmPaymentRequest;
  };
  'POST /v1/bridge/refund': {
    request: ApiRefundRequest;
  };

  // Health
  'GET /health': {
    response: string;
  };
  'GET /ready': {
    response: string;
  };
}
