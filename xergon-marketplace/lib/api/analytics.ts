/**
 * Analytics API — network stats types and fetch helpers.
 *
 * Data comes from the xergon-relay via /api/xergon-relay/stats proxy route.
 * If the relay is unreachable, the proxy returns mock data with a "degraded" flag.
 */

// ---------------------------------------------------------------------------
// Types — Network Stats (original)
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
// Types — Analytics Overview
// ---------------------------------------------------------------------------

export interface OverviewSummary {
  totalRequests: number;
  totalTokens: number;
  totalEarningsNanoErg: number;
  totalSpentNanoErg: number;
  averageLatencyMs: number;
  p95LatencyMs: number;
  activeUsers: number;
  activeProviders: number;
}

export interface DailyPoint {
  date: string;
  requests: number;
  tokens: number;
  earningsNanoErg: number;
  spentNanoErg: number;
  averageLatencyMs: number;
  uniqueUsers: number;
}

export interface OverviewResponse {
  period: { start: string; end: string };
  summary: OverviewSummary;
  daily: DailyPoint[];
}

// ---------------------------------------------------------------------------
// Types — Model Analytics
// ---------------------------------------------------------------------------

export interface ModelProvider {
  providerPk: string;
  region: string;
  requests: number;
}

export interface ModelDailyUsage {
  date: string;
  requests: number;
  tokens: number;
}

export interface ModelAnalytics {
  modelId: string;
  requests: number;
  tokens: number;
  earningsNanoErg: number;
  averageLatencyMs: number;
  p95LatencyMs: number;
  errorRate: number;
  totalUsers: number;
  topProviders: ModelProvider[];
  dailyUsage: ModelDailyUsage[];
}

// ---------------------------------------------------------------------------
// Types — Provider Comparison
// ---------------------------------------------------------------------------

export interface ProviderData {
  providerPk: string;
  region: string;
  models: string[];
  requests: number;
  tokens: number;
  earningsNanoErg: number;
  averageLatencyMs: number;
  p95LatencyMs: number;
  errorRate: number;
  uptime: number;
  reputationScore: number;
  rank: number;
}

export interface ProvidersResponse {
  providers: ProviderData[];
  comparisonMetrics: string[];
}

// ---------------------------------------------------------------------------
// Types — Regional Distribution
// ---------------------------------------------------------------------------

export interface RegionData {
  region: string;
  requests: number;
  tokens: number;
  providers: number;
  averageLatencyMs: number;
  marketShare: number;
}

// ---------------------------------------------------------------------------
// Fetch helpers
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

/**
 * Fetch analytics overview data.
 */
export async function fetchAnalyticsOverview(
  period: "7d" | "30d" | "90d" = "30d"
): Promise<OverviewResponse> {
  const res = await fetch(`/api/analytics/overview?period=${period}`);
  if (!res.ok) throw new Error(`Overview endpoint returned ${res.status}`);
  return res.json();
}

/**
 * Fetch model analytics data.
 */
export async function fetchModelAnalytics(): Promise<ModelAnalytics[]> {
  const res = await fetch("/api/analytics/models");
  if (!res.ok) throw new Error(`Models endpoint returned ${res.status}`);
  return res.json();
}

/**
 * Fetch provider comparison data.
 */
export async function fetchProviderComparison(
  sort: string = "reputation",
  region?: string
): Promise<ProvidersResponse> {
  const params = new URLSearchParams({ sort });
  if (region) params.set("region", region);
  const res = await fetch(`/api/analytics/providers?${params}`);
  if (!res.ok) throw new Error(`Providers endpoint returned ${res.status}`);
  return res.json();
}

/**
 * Fetch regional distribution data.
 */
export async function fetchRegionalDistribution(): Promise<RegionData[]> {
  const res = await fetch("/api/analytics/regions");
  if (!res.ok) throw new Error(`Regions endpoint returned ${res.status}`);
  return res.json();
}
