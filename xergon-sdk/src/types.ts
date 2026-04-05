/**
 * All TypeScript interfaces matching the Xergon Relay OpenAPI spec.
 *
 * Wire format uses snake_case; TypeScript interfaces use camelCase
 * for idiomatic JS.  The client layer handles conversion.
 */

// ── Chat Completions (OpenAI-compatible) ──────────────────────────────

export type ChatRole = 'system' | 'user' | 'assistant';

export interface ChatMessage {
  role: ChatRole;
  content: string;
}

export interface ChatCompletionParams {
  model: string;
  messages: ChatMessage[];
  maxTokens?: number;
  temperature?: number;
  topP?: number;
  stream?: boolean;
}

export interface ChatCompletionChoice {
  index: number;
  message: ChatMessage;
  finishReason: 'stop' | 'length' | 'content_filter';
}

export interface ChatCompletionUsage {
  promptTokens: number;
  completionTokens: number;
  totalTokens: number;
}

export interface ChatCompletionResponse {
  id: string;
  object: 'chat.completion';
  created: number;
  model: string;
  choices: ChatCompletionChoice[];
  usage?: ChatCompletionUsage;
}

export interface ChatCompletionDelta {
  role?: ChatRole;
  content?: string;
}

export interface ChatCompletionChunkChoice {
  index: number;
  delta: ChatCompletionDelta;
  finishReason: 'stop' | 'length' | 'content_filter' | null;
}

export interface ChatCompletionChunk {
  id: string;
  object: 'chat.completion.chunk';
  created: number;
  model: string;
  choices: ChatCompletionChunkChoice[];
}

// ── Models ────────────────────────────────────────────────────────────

export interface Model {
  id: string;
  object: string;
  ownedBy: string;
  pricing?: string;
}

export interface ModelsResponse {
  object: string;
  data: Model[];
}

// ── Providers ─────────────────────────────────────────────────────────

export interface Provider {
  publicKey: string;
  endpoint: string;
  models: string[];
  region: string;
  pownScore: number;
  lastHeartbeat?: number;
  pricing?: Record<string, string>;
}

export interface LeaderboardEntry {
  publicKey: string;
  endpoint: string;
  models: string[];
  region: string;
  pownScore: number;
  lastHeartbeat?: number;
  pricing?: Record<string, string>;
  online?: boolean;
  totalRequests?: number;
  totalPromptTokens?: number;
  totalCompletionTokens?: number;
  totalTokens?: number;
}

// ── Balance ───────────────────────────────────────────────────────────

export interface BalanceResponse {
  publicKey: string;
  balanceNanoerg: string;
  balanceErg: string;
  stakingBoxId?: string;
}

// ── GPU Bazar ─────────────────────────────────────────────────────────

export interface GpuListing {
  listingId: string;
  providerPk: string;
  gpuType: string;
  vramGb?: number;
  pricePerHourNanoerg: string;
  region: string;
  available: boolean;
  bandwidthMbps?: number;
}

export interface GpuRental {
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

export interface GpuPricingEntry {
  gpuType: string;
  avgPricePerHourNanoerg: string;
  minPricePerHourNanoerg?: string;
  maxPricePerHourNanoerg?: string;
  listingCount?: number;
}

export interface GpuFilters {
  gpuType?: string;
  minVram?: number;
  maxPrice?: number;
  region?: string;
}

export interface RateGpuParams {
  targetPk: string;
  rentalId: string;
  score: number;
  comment?: string;
}

export interface GpuReputation {
  publicKey: string;
  score: number;
  totalRatings: number;
  average: number;
}

// ── Incentive ─────────────────────────────────────────────────────────

export interface IncentiveStatus {
  active: boolean;
  totalBonusErg: string;
  rareModelsCount: number;
}

export interface RareModel {
  model: string;
  rarityScore: number;
  bonusMultiplier: number;
  providersCount: number;
}

export interface RareModelDetail extends RareModel {
  recentRequests?: number;
  bonusErgAccumulated?: string;
}

// ── Bridge ────────────────────────────────────────────────────────────

export type BridgeChain = 'btc' | 'eth' | 'ada';
export type BridgeInvoiceStatus = 'pending' | 'confirmed' | 'refunded' | 'expired';

export interface BridgeInvoice {
  invoiceId: string;
  amountNanoerg: string;
  chain: BridgeChain;
  status: BridgeInvoiceStatus;
  createdAt: number;
  refundTimeout: number;
}

export interface BridgeStatus {
  status: string;
  supportedChains: string[];
}

// ── Health ────────────────────────────────────────────────────────────

export interface HealthResponse {
  status: string;
  version?: string;
  uptimeSecs?: number;
  ergoNodeConnected?: boolean;
  activeProviders?: number;
  totalProviders?: number;
}

// ── Auth ──────────────────────────────────────────────────────────────

export interface AuthStatus {
  authenticated: boolean;
  publicKey: string;
  tier: string;
}

// ── Client Config ─────────────────────────────────────────────────────

export interface XergonClientConfig {
  baseUrl?: string;
  publicKey?: string;
  privateKey?: string;
}

/** Callback for request/response logging. */
export type LogInterceptor = (event: {
  method: string;
  url: string;
  status?: number;
  durationMs?: number;
  error?: string;
}) => void;
