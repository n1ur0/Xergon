import { NextRequest, NextResponse } from "next/server";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface ModelAnalytics {
  modelId: string;
  requests: number;
  tokens: number;
  earningsNanoErg: number;
  averageLatencyMs: number;
  p95LatencyMs: number;
  errorRate: number;
  totalUsers: number;
  topProviders: Array<{ providerPk: string; region: string; requests: number }>;
  dailyUsage: Array<{ date: string; requests: number; tokens: number }>;
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

const MODELS = [
  "llama-3.1-70b",
  "llama-3.1-8b",
  "mixtral-8x7b",
  "qwen-2.5-72b",
  "deepseek-v3",
  "mistral-7b",
  "codellama-34b",
  "phi-3-medium",
];

const REGIONS = ["US-East", "US-West", "EU-West", "EU-North", "Asia-Pacific"];

function generateModelAnalytics(): ModelAnalytics[] {
  const rand = seededRandom(42);
  const now = new Date();

  return MODELS.map((modelId, mi) => {
    const seed = mi * 1000 + 7;
    const r = seededRandom(seed);

    const requests = Math.floor(2000 + r() * 15000);
    const tokens = Math.floor(requests * (600 + r() * 800));
    const earningsNanoErg = Math.floor(tokens * (0.0001 + r() * 0.0002) * 1e9);
    const averageLatencyMs = Math.floor(100 + r() * 350);
    const p95LatencyMs = Math.floor(averageLatencyMs * (1.4 + r() * 0.6));
    const errorRate = parseFloat((r() * 3).toFixed(2));
    const totalUsers = Math.floor(15 + r() * 60);

    const providerCount = 2 + Math.floor(r() * 4);
    const topProviders = Array.from({ length: providerCount }, (_, pi) => ({
      providerPk: `9x${mi}${pi}${Math.floor(r() * 9999).toString().padStart(4, "0")}`.padEnd(
        12,
        "0"
      ),
      region: REGIONS[Math.floor(r() * REGIONS.length)],
      requests: Math.floor(requests / providerCount * (0.5 + r())),
    }));

    const dailyUsage = Array.from({ length: 30 }, (_, di) => {
      const d = new Date(now);
      d.setDate(d.getDate() - 29 + di);
      return {
        date: d.toISOString().split("T")[0],
        requests: Math.floor(requests / 30 * (0.6 + r() * 0.8)),
        tokens: Math.floor(tokens / 30 * (0.6 + r() * 0.8)),
      };
    });

    return {
      modelId,
      requests,
      tokens,
      earningsNanoErg,
      averageLatencyMs,
      p95LatencyMs,
      errorRate,
      totalUsers,
      topProviders,
      dailyUsage,
    };
  });
}

// ---------------------------------------------------------------------------
// GET handler
// ---------------------------------------------------------------------------

export async function GET(request: NextRequest) {
  const data = generateModelAnalytics();

  return NextResponse.json(data, {
    headers: {
      "Cache-Control": "public, s-maxage=60, stale-while-revalidate=120",
    },
  });
}
