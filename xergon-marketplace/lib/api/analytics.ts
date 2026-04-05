/**
 * Analytics API — network stats types and fetch helpers.
 *
 * Data comes from the xergon-relay via /api/xergon-relay/stats proxy route.
 * If the relay is unreachable, the proxy returns mock data with a "degraded" flag.
 */

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface NetworkStats {
  totalProviders: number;
  activeProviders: number;
  totalErgStaked: number;       // nanoERG in staking boxes
  requests24h: number;
  activeModels: number;
  totalTokensProcessed: number;
  avgLatencyMs: number;
  networkUptime: number;        // percentage 0-100
  providersByRegion: Record<string, number>;
  topModels: Array<{ model: string; requests: number; tokens: number }>;
  requestsOverTime: Array<{ timestamp: string; count: number }>;
  ergPriceUsd: number;
  degraded?: boolean;           // true when relay is unreachable (mock data)
}

export interface NetworkStatsResponse extends NetworkStats {
  degraded?: boolean;
}

// ---------------------------------------------------------------------------
// Fetch helper
// ---------------------------------------------------------------------------

/**
 * Fetch live network stats from the relay proxy.
 * Returns stats (potentially degraded mock data if relay is down).
 */
export async function fetchNetworkStats(): Promise<NetworkStatsResponse> {
  try {
    const res = await fetch("/api/xergon-relay/stats", {
      // Cache for 30s, revalidate in background
      next: { revalidate: 30 },
    });
    if (!res.ok) {
      throw new Error(`Stats endpoint returned ${res.status}`);
    }
    return (await res.json()) as NetworkStatsResponse;
  } catch {
    // If even the proxy fails, return empty degraded data
    return {
      totalProviders: 0,
      activeProviders: 0,
      totalErgStaked: 0,
      requests24h: 0,
      activeModels: 0,
      totalTokensProcessed: 0,
      avgLatencyMs: 0,
      networkUptime: 0,
      providersByRegion: {},
      topModels: [],
      requestsOverTime: [],
      ergPriceUsd: 0,
      degraded: true,
    };
  }
}
