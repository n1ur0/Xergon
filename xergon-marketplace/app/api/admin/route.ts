import { NextResponse } from "next/server";

const RELAY_BASE =
  process.env.XERGON_RELAY_BASE ?? "http://127.0.0.1:9090";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface AdminOverview {
  totalProviders: number;
  activeProviders: number;
  totalUsers: number;
  totalEarningsNanoErg: number;
  totalRequests24h: number;
  totalTokens24h: number;
  averageLatencyMs: number;
  activeRentals: number;
  openDisputes: number;
}

interface SystemHealth {
  relayStatus: "healthy" | "degraded" | "down";
  agentCount: number;
  nodeHeight: number;
  nodeSynced: boolean;
}

interface RecentActivity {
  type: "provider_registered" | "rental_started" | "rental_completed" | "withdrawal" | "dispute_opened";
  description: string;
  timestamp: string;
}

// ---------------------------------------------------------------------------
// Mock data (used when relay is unreachable)
// ---------------------------------------------------------------------------

function mockOverview(): AdminOverview {
  return {
    totalProviders: 12,
    activeProviders: 10,
    totalUsers: 1842,
    totalEarningsNanoErg: 4_560_000_000,
    totalRequests24h: 34_891,
    totalTokens24h: 128_400_000,
    averageLatencyMs: 187,
    activeRentals: 6,
    openDisputes: 2,
  };
}

function mockSystemHealth(): SystemHealth {
  return {
    relayStatus: "healthy",
    agentCount: 3,
    nodeHeight: 892_451,
    nodeSynced: true,
  };
}

function mockRecentActivity(): RecentActivity[] {
  const now = Date.now();
  return [
    {
      type: "provider_registered",
      description: "New provider XergonNode-013 registered in EU region",
      timestamp: new Date(now - 1_800_000).toISOString(),
    },
    {
      type: "rental_started",
      description: "User 9h4k...f2a1 started rental on llama-3.1-70b",
      timestamp: new Date(now - 3_600_000).toISOString(),
    },
    {
      type: "dispute_opened",
      description: "Dispute #DSP-042 opened against provider in US region",
      timestamp: new Date(now - 7_200_000).toISOString(),
    },
    {
      type: "withdrawal",
      description: "Provider 3xk8...m9n2 withdrew 45.6 ERG earnings",
      timestamp: new Date(now - 14_400_000).toISOString(),
    },
    {
      type: "rental_completed",
      description: "Rental #RNT-891 completed — 12,400 tokens processed",
      timestamp: new Date(now - 21_600_000).toISOString(),
    },
    {
      type: "provider_registered",
      description: "New provider XergonNode-012 registered in Asia region",
      timestamp: new Date(now - 43_200_000).toISOString(),
    },
    {
      type: "rental_started",
      description: "User 2j7q...w3e8 started rental on qwen2.5-72b",
      timestamp: new Date(now - 50_400_000).toISOString(),
    },
    {
      type: "dispute_opened",
      description: "Dispute #DSP-041 opened for downtime complaint",
      timestamp: new Date(now - 72_000_000).toISOString(),
    },
  ];
}

// ---------------------------------------------------------------------------
// GET handler
// ---------------------------------------------------------------------------

export async function GET() {
  try {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 5000);

    const res = await fetch(`${RELAY_BASE}/v1/admin/dashboard`, {
      signal: controller.signal,
    });

    clearTimeout(timeout);

    if (!res.ok) {
      // Return mock data when relay is unreachable
      return NextResponse.json({
        overview: mockOverview(),
        systemHealth: mockSystemHealth(),
        recentActivity: mockRecentActivity(),
        degraded: true,
      });
    }

    const data = await res.json();
    return NextResponse.json({
      overview: data.overview ?? mockOverview(),
      systemHealth: data.systemHealth ?? mockSystemHealth(),
      recentActivity: data.recentActivity ?? mockRecentActivity(),
      degraded: false,
    });
  } catch {
    return NextResponse.json({
      overview: mockOverview(),
      systemHealth: mockSystemHealth(),
      recentActivity: mockRecentActivity(),
      degraded: true,
    });
  }
}
