/**
 * React hooks for live chain data.
 *
 * These hooks wrap the chain store and API functions with
 * auto-refresh timers and proper loading/error states.
 * They use simple useState+useEffect with the zustand store
 * as a shared cache — no SWR or React Query needed.
 */

import { useEffect, useRef, useCallback, useState } from "react";
import { useChainStore } from "@/lib/stores/chain-store";
import { useAuthStore } from "@/lib/stores/auth";
import {
  fetchProviders,
  fetchModels,
  fetchLeaderboard,
  type ProviderInfo,
  type ChainModelInfo,
  type ChainLeaderboardEntry,
} from "@/lib/api/chain";

// Re-export types so consumers can import from here
export type { ChainModelInfo, ChainLeaderboardEntry, ProviderInfo } from "@/lib/api/chain";

// ── Simple in-memory cache for non-store data ──────────────────────────

interface CacheEntry<T extends { length: number }> {
  data: T;
  fetchedAt: number;
}

const CACHE_TTL = 30_000; // 30 seconds

const modelsCache: CacheEntry<ChainModelInfo[]> = { data: [], fetchedAt: 0 };
const leaderboardCache: CacheEntry<ChainLeaderboardEntry[]> = {
  data: [],
  fetchedAt: 0,
};

function isCacheValid<T extends { length: number }>(entry: CacheEntry<T>): boolean {
  return entry.data.length > 0 && Date.now() - entry.fetchedAt < CACHE_TTL;
}

// ── useProviders ───────────────────────────────────────────────────────

export interface UseProvidersResult {
  providers: ProviderInfo[];
  isLoading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
}

/**
 * Fetch and cache providers. Auto-refreshes every 30s.
 * Uses the chain store as the source of truth.
 */
export function useProviders(): UseProvidersResult {
  const providers = useChainStore((s) => s.providers);
  const providersLoading = useChainStore((s) => s.providersLoading);
  const refreshProviders = useChainStore((s) => s.refreshProviders);
  const errorRef = useRef<string | null>(null);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;

    // Initial fetch if no data yet
    if (providers.length === 0 && !providersLoading) {
      refreshProviders().catch(() => {
        if (mountedRef.current) errorRef.current = "Failed to load providers";
      });
    }

    // Auto-refresh every 30s
    const interval = setInterval(() => {
      refreshProviders().catch(() => {
        if (mountedRef.current) errorRef.current = "Failed to load providers";
      });
    }, 30_000);

    return () => {
      mountedRef.current = false;
      clearInterval(interval);
    };
  }, [providers.length, providersLoading, refreshProviders]);

  return {
    providers,
    isLoading: providersLoading && providers.length === 0,
    error: errorRef.current,
    refresh: refreshProviders,
  };
}

// ── useModels ──────────────────────────────────────────────────────────

export interface UseModelsResult {
  models: ChainModelInfo[];
  isLoading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
}

/**
 * Fetch and cache models. Auto-refreshes every 30s.
 * Uses a simple in-memory cache since models are derived from providers.
 */
export function useModels(): UseModelsResult {
  const cache = useRef(modelsCache);
  const [state, setState] = useState<{
    models: ChainModelInfo[];
    isLoading: boolean;
    error: string | null;
  }>({
    models: isCacheValid(cache.current) ? cache.current.data : [],
    isLoading: !isCacheValid(cache.current),
    error: null,
  });

  const refresh = useCallback(async () => {
    setState((prev) => ({ ...prev, isLoading: true, error: null }));
    try {
      const data = await fetchModels();
      cache.current = { data, fetchedAt: Date.now() };
      setState({ models: data, isLoading: false, error: null });
    } catch {
      setState((prev) => ({
        ...prev,
        isLoading: false,
        error: "Failed to load models",
      }));
    }
  }, []);

  useEffect(() => {
    if (!isCacheValid(cache.current)) {
      refresh();
    }
    const interval = setInterval(refresh, 30_000);
    return () => clearInterval(interval);
  }, [refresh]);

  return { ...state, refresh };
}

// ── useChainBalance ────────────────────────────────────────────────────

export interface UseChainBalanceResult {
  balanceErg: number;
  stakingBoxesCount: number;
  sufficient: boolean;
  isLoading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
}

/**
 * Fetch user's staking balance when wallet is connected.
 * Uses the chain store. Auto-refreshes every 30s.
 * Falls back to the auth store's user.balance if the chain
 * balance hasn't been fetched yet.
 */
export function useChainBalance(): UseChainBalanceResult {
  const user = useAuthStore((s) => s.user);
  const userBalance = useChainStore((s) => s.userBalance);
  const balanceLoading = useChainStore((s) => s.balanceLoading);
  const refreshBalance = useChainStore((s) => s.refreshBalance);

  const publicKey = user?.publicKey ?? null;

  useEffect(() => {
    if (!publicKey) return;

    // Initial fetch
    refreshBalance(publicKey);

    // Auto-refresh every 30s
    const interval = setInterval(() => {
      refreshBalance(publicKey);
    }, 30_000);

    return () => clearInterval(interval);
  }, [publicKey, refreshBalance]);

  return {
    balanceErg: userBalance?.balance_erg ?? user?.balance ?? 0,
    stakingBoxesCount: userBalance?.staking_boxes_count ?? 0,
    sufficient: userBalance?.sufficient ?? true,
    isLoading: balanceLoading && userBalance === null,
    error: null,
    refresh: () => publicKey ? refreshBalance(publicKey) : Promise.resolve(),
  };
}

// ── useNodeStatus ──────────────────────────────────────────────────────

export interface UseNodeStatusResult {
  status: string;
  version: string;
  uptimeSecs: number;
  ergoNodeConnected: boolean;
  activeProviders: number;
  totalProviders: number;
  isLoading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
}

/**
 * Fetch relay/node health status. Auto-refreshes every 60s
 * (less frequent since it rarely changes).
 */
export function useNodeStatus(): UseNodeStatusResult {
  const nodeStatus = useChainStore((s) => s.nodeStatus);
  const nodeStatusLoading = useChainStore((s) => s.nodeStatusLoading);
  const refreshNodeStatus = useChainStore((s) => s.refreshNodeStatus);

  useEffect(() => {
    refreshNodeStatus();
    const interval = setInterval(refreshNodeStatus, 60_000);
    return () => clearInterval(interval);
  }, [refreshNodeStatus]);

  return {
    status: nodeStatus?.status ?? "unknown",
    version: nodeStatus?.version ?? "",
    uptimeSecs: nodeStatus?.uptime_secs ?? 0,
    ergoNodeConnected: nodeStatus?.ergo_node_connected ?? false,
    activeProviders: nodeStatus?.active_providers ?? 0,
    totalProviders: nodeStatus?.total_providers ?? 0,
    isLoading: nodeStatusLoading && nodeStatus === null,
    error: null,
    refresh: refreshNodeStatus,
  };
}

// ── useLeaderboard ─────────────────────────────────────────────────────

export interface UseLeaderboardResult {
  entries: ChainLeaderboardEntry[];
  isLoading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
}

/**
 * Fetch provider leaderboard. Auto-refreshes every 30s.
 */
export function useLeaderboard(): UseLeaderboardResult {
  const cache = useRef(leaderboardCache);
  const [state, setState] = useState<{
    entries: ChainLeaderboardEntry[];
    isLoading: boolean;
    error: string | null;
  }>({
    entries: isCacheValid(cache.current) ? cache.current.data : [],
    isLoading: !isCacheValid(cache.current),
    error: null,
  });

  const refresh = useCallback(async () => {
    setState((prev) => ({ ...prev, isLoading: true, error: null }));
    try {
      const data = await fetchLeaderboard();
      cache.current = { data, fetchedAt: Date.now() };
      setState({ entries: data, isLoading: false, error: null });
    } catch {
      setState((prev) => ({
        ...prev,
        isLoading: false,
        error: "Failed to load leaderboard",
      }));
    }
  }, []);

  useEffect(() => {
    if (!isCacheValid(cache.current)) {
      refresh();
    }
    const interval = setInterval(refresh, 30_000);
    return () => clearInterval(interval);
  }, [refresh]);

  return { ...state, refresh };
}
