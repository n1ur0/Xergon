/**
 * Health API — service status types and fetch helpers.
 *
 * Data comes from the xergon-relay via /api/xergon-relay/health proxy route.
 * If the relay is unreachable, the proxy returns degraded fallback data.
 */

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface ServiceStatus {
  name: string;
  url: string;
  status: "operational" | "degraded" | "down" | "unknown";
  latencyMs: number | null;
  lastCheck: string;
  uptime24h: number; // percentage 0-100
  incidents24h: number;
}

export interface HealthSummary {
  overall: "operational" | "degraded" | "partial" | "major_outage";
  services: ServiceStatus[];
  providerDistribution: {
    online: number;
    degraded: number;
    offline: number;
    total: number;
  };
  chainHeight: number;
  bestHeight: number;
  chainSynced: boolean;
  lastBlockTime: string;
  oracleRate: number | null; // ERG/USD from oracle pool
  degraded?: boolean; // true when relay is unreachable
}

// ---------------------------------------------------------------------------
// Fetch helper
// ---------------------------------------------------------------------------

/**
 * Fetch live health summary from the relay proxy.
 * Returns data (potentially degraded mock data if relay is down).
 */
export async function fetchHealthSummary(): Promise<HealthSummary> {
  try {
    const res = await fetch("/api/xergon-relay/health", {
      cache: "no-store",
    });
    if (!res.ok) {
      throw new Error(`Health endpoint returned ${res.status}`);
    }
    return (await res.json()) as HealthSummary;
  } catch {
    // If even the proxy fails, return empty degraded data
    const now = new Date().toISOString();
    return {
      overall: "major_outage",
      services: [
        {
          name: "Xergon Relay",
          url: "/v1/health",
          status: "unknown",
          latencyMs: null,
          lastCheck: now,
          uptime24h: 0,
          incidents24h: 0,
        },
        {
          name: "Chain Scanner",
          url: "/v1/chain/status",
          status: "unknown",
          latencyMs: null,
          lastCheck: now,
          uptime24h: 0,
          incidents24h: 0,
        },
        {
          name: "Provider Network",
          url: "/v1/providers",
          status: "unknown",
          latencyMs: null,
          lastCheck: now,
          uptime24h: 0,
          incidents24h: 0,
        },
        {
          name: "Oracle Pool",
          url: "/v1/oracle",
          status: "unknown",
          latencyMs: null,
          lastCheck: now,
          uptime24h: 0,
          incidents24h: 0,
        },
        {
          name: "Marketplace API",
          url: "/api/health",
          status: "unknown",
          latencyMs: null,
          lastCheck: now,
          uptime24h: 0,
          incidents24h: 0,
        },
        {
          name: "Ergo Node",
          url: "/v1/node",
          status: "unknown",
          latencyMs: null,
          lastCheck: now,
          uptime24h: 0,
          incidents24h: 0,
        },
      ],
      providerDistribution: { online: 0, degraded: 0, offline: 0, total: 0 },
      chainHeight: 0,
      bestHeight: 0,
      chainSynced: false,
      lastBlockTime: now,
      oracleRate: null,
      degraded: true,
    };
  }
}
