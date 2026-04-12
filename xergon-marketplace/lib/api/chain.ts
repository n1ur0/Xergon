/**
 * Chain data API layer.
 *
 * Fetches live data from the Xergon relay's public endpoints using the SDK.
 * All functions return typed responses and gracefully degrade to
 * empty data on error.
 */

import { sdk } from "./config";

// ── Types (matching relay snake_case JSON, kept for marketplace compat) ──

/** Single provider from GET /v1/providers */
export interface ProviderInfo {
  provider_id: string;
  endpoint: string;
  region: string;
  models: string[];
  pown_score: number;
  is_active: boolean;
  value_nanoerg: number;
  box_id: string;
  latency_ms: number | null;
  healthy: boolean;
}

/** Envelope from GET /v1/providers */
export interface ProvidersResponse {
  providers: ProviderInfo[];
}

/** Single enriched model from GET /v1/models */
export interface ChainModelInfo {
  id: string;
  name: string;
  provider: string;
  tier: string;
  price_per_input_token_nanoerg: number;
  price_per_output_token_nanoerg: number;
  min_provider_price_nanoerg?: number;
  effective_price_nanoerg?: number;
  provider_count?: number;
  available: boolean;
  description?: string;
  context_window?: number;
  speed?: string;
  tags?: string[];
  free_tier?: boolean;
}

/** Balance response from GET /v1/balance/:user_pk */
export interface BalanceResponse {
  user_pk: string;
  balance_nanoerg: number;
  balance_erg: number;
  staking_boxes_count: number;
  sufficient: boolean;
  min_balance_nanoerg: number;
}

/** Leaderboard entry from GET /v1/leaderboard */
export interface ChainLeaderboardEntry {
  provider_id: string;
  endpoint: string;
  online: boolean;
  latency_ms: number;
  total_requests: number;
  total_prompt_tokens: number;
  total_completion_tokens: number;
  total_tokens: number;
  pown_score?: number;
  region?: string;
}

/** Health / node status from GET /v1/health */
export interface HealthResponse {
  status: string;
  version: string;
  uptime_secs: number;
  ergo_node_connected: boolean;
  active_providers: number;
  total_providers: number;
}

// ── Helpers ────────────────────────────────────────────────────────────

const NANOERG_PER_ERG = 1_000_000_000;

/** Convert nanoERG to ERG */
export function nanoergToErg(nano: number): number {
  return nano / NANOERG_PER_ERG;
}

/** Generic fetch with error handling and fallback */
async function safeFetch<T>(
  url: string,
  fallback: T,
  label: string,
): Promise<T> {
  try {
    const res = await fetch(url);
    if (!res.ok) {
      console.warn(`[chain] ${label}: HTTP ${res.status}`);
      return fallback;
    }
    return (await res.json()) as T;
  } catch (err) {
    console.warn(`[chain] ${label}:`, err);
    return fallback;
  }
}

// ── API Functions (delegating to SDK where possible) ────────────────────

/** Fetch list of active providers from the relay. */
export async function fetchProviders(): Promise<ProviderInfo[]> {
  const data = await safeFetch<ProvidersResponse>(
    `${sdk.getBaseUrl()}/v1/providers`,
    { providers: [] },
    "fetchProviders",
  );
  return data.providers;
}

/** Fetch available models from the relay. */
export async function fetchModels(): Promise<ChainModelInfo[]> {
  return safeFetch<ChainModelInfo[]>(
    `${sdk.getBaseUrl()}/v1/models`,
    [],
    "fetchModels",
  );
}

/** Fetch user staking balance from the relay. */
export async function fetchBalance(
  userPk: string,
): Promise<BalanceResponse | null> {
  if (!userPk) return null;
  return safeFetch<BalanceResponse | null>(
    `${sdk.getBaseUrl()}/v1/balance/${encodeURIComponent(userPk)}`,
    null,
    "fetchBalance",
  );
}

/** Fetch provider leaderboard from the relay. */
export async function fetchLeaderboard(): Promise<ChainLeaderboardEntry[]> {
  return safeFetch<ChainLeaderboardEntry[]>(
    `${sdk.getBaseUrl()}/v1/leaderboard`,
    [],
    "fetchLeaderboard",
  );
}

/** Fetch relay health / node status. */
export async function fetchNodeStatus(): Promise<HealthResponse | null> {
  return safeFetch<HealthResponse | null>(
    `${sdk.getBaseUrl()}/v1/health`,
    null,
    "fetchNodeStatus",
  );
}
