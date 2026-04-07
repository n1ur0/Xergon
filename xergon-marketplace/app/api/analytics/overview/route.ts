import { NextRequest, NextResponse } from "next/server";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface OverviewSummary {
  totalRequests: number;
  totalTokens: number;
  totalEarningsNanoErg: number;
  totalSpentNanoErg: number;
  averageLatencyMs: number;
  p95LatencyMs: number;
  activeUsers: number;
  activeProviders: number;
}

interface DailyPoint {
  date: string;
  requests: number;
  tokens: number;
  earningsNanoErg: number;
  spentNanoErg: number;
  averageLatencyMs: number;
  uniqueUsers: number;
}

interface OverviewResponse {
  period: { start: string; end: string };
  summary: OverviewSummary;
  daily: DailyPoint[];
}

// ---------------------------------------------------------------------------
// Seed-based deterministic mock data generator
// ---------------------------------------------------------------------------

function seededRandom(seed: number): () => number {
  let s = seed;
  return () => {
    s = (s * 16807 + 0) % 2147483647;
    return (s - 1) / 2147483646;
  };
}

function generateMockData(periodDays: number): OverviewResponse {
  const now = new Date();
  const start = new Date(now);
  start.setDate(start.getDate() - periodDays);

  const rand = seededRandom(periodDays * 7 + 3);
  const daily: DailyPoint[] = [];

  let totalRequests = 0;
  let totalTokens = 0;
  let totalEarnings = 0;
  let totalSpent = 0;
  let latencySum = 0;
  let latencies: number[] = [];
  const userSet = new Set<number>();

  for (let i = 0; i < periodDays; i++) {
    const d = new Date(start);
    d.setDate(d.getDate() + i);
    const dateStr = d.toISOString().split("T")[0];

    const baseReqs = 800 + periodDays * 10;
    const requests = Math.floor(baseReqs + rand() * 600 - 200);
    const tokens = Math.floor(requests * (800 + rand() * 400));
    const earnings = Math.floor(tokens * 0.00015 * 1e9);
    const spent = Math.floor(earnings * (0.9 + rand() * 0.2));
    const avgLatency = Math.floor(120 + rand() * 280);
    const uniqueUsers = Math.floor(20 + rand() * 40);

    daily.push({
      date: dateStr,
      requests,
      tokens,
      earningsNanoErg: earnings,
      spentNanoErg: spent,
      averageLatencyMs: avgLatency,
      uniqueUsers,
    });

    totalRequests += requests;
    totalTokens += tokens;
    totalEarnings += earnings;
    totalSpent += spent;
    latencySum += avgLatency;
    latencies.push(avgLatency);
    for (let u = 0; u < uniqueUsers; u++) userSet.add(Math.floor(rand() * 10000));
  }

  latencies.sort((a, b) => a - b);
  const p95Idx = Math.floor(latencies.length * 0.95);

  return {
    period: {
      start: start.toISOString().split("T")[0],
      end: now.toISOString().split("T")[0],
    },
    summary: {
      totalRequests,
      totalTokens,
      totalEarningsNanoErg: totalEarnings,
      totalSpentNanoErg: totalSpent,
      averageLatencyMs: Math.round(latencySum / periodDays),
      p95LatencyMs: latencies[p95Idx] || 0,
      activeUsers: userSet.size,
      activeProviders: Math.floor(8 + rand() * 12),
    },
    daily,
  };
}

// ---------------------------------------------------------------------------
// GET handler
// ---------------------------------------------------------------------------

export async function GET(request: NextRequest) {
  const { searchParams } = new URL(request.url);
  const period = searchParams.get("period") || "30d";

  const periodMap: Record<string, number> = {
    "7d": 7,
    "30d": 30,
    "90d": 90,
  };
  const days = periodMap[period] || 30;

  const data = generateMockData(days);

  return NextResponse.json(data, {
    headers: {
      "Cache-Control": "public, s-maxage=60, stale-while-revalidate=120",
    },
  });
}
