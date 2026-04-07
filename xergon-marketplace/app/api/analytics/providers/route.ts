import { NextRequest, NextResponse } from "next/server";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface ProviderData {
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

interface ProvidersResponse {
  providers: ProviderData[];
  comparisonMetrics: string[];
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

const REGIONS = ["US-East", "US-West", "EU-West", "EU-North", "Asia-Pacific"];
const ALL_MODELS = [
  "llama-3.1-70b",
  "llama-3.1-8b",
  "mixtral-8x7b",
  "qwen-2.5-72b",
  "deepseek-v3",
  "mistral-7b",
  "codellama-34b",
  "phi-3-medium",
];

function generateProviders(
  regionFilter?: string,
  sortBy?: string
): ProvidersResponse {
  const rand = seededRandom(99);

  const providers: ProviderData[] = Array.from({ length: 12 }, (_, i) => {
    const r = seededRandom(i * 333 + 11);
    const region = REGIONS[i % REGIONS.length];
    const modelCount = 1 + Math.floor(r() * 4);
    const models = ALL_MODELS.slice(0, modelCount);

    const requests = Math.floor(3000 + r() * 20000);
    const tokens = Math.floor(requests * (500 + r() * 1000));
    const earningsNanoErg = Math.floor(tokens * (0.0001 + r() * 0.00025) * 1e9);
    const averageLatencyMs = Math.floor(80 + r() * 400);
    const p95LatencyMs = Math.floor(averageLatencyMs * (1.3 + r() * 0.7));
    const errorRate = parseFloat((r() * 4).toFixed(2));
    const uptime = parseFloat((95 + r() * 5).toFixed(2));
    const reputationScore = parseFloat(
      (Math.min(uptime / 100 * (1 - errorRate / 10) * 100, 100)).toFixed(1)
    );

    return {
      providerPk: `9x${i.toString().padStart(2, "0")}${Math.floor(r() * 99999)
        .toString()
        .padStart(5, "0")}`,
      region,
      models,
      requests,
      tokens,
      earningsNanoErg,
      averageLatencyMs,
      p95LatencyMs,
      errorRate,
      uptime,
      reputationScore,
      rank: 0,
    };
  });

  // Filter by region
  let filtered = regionFilter
    ? providers.filter((p) => p.region === regionFilter)
    : providers;

  // Sort
  const sortFn: Record<string, (a: ProviderData, b: ProviderData) => number> = {
    latency: (a, b) => a.averageLatencyMs - b.averageLatencyMs,
    throughput: (a, b) => b.requests - a.requests,
    reliability: (a, b) => b.uptime - a.uptime,
    cost: (a, b) => a.earningsNanoErg - b.earningsNanoErg,
    reputation: (a, b) => b.reputationScore - a.reputationScore,
  };

  const sortKey = sortBy && sortFn[sortBy] ? sortBy : "reputation";
  filtered.sort(sortFn[sortKey]);

  // Assign ranks
  filtered.forEach((p, i) => {
    p.rank = i + 1;
  });

  return {
    providers: filtered,
    comparisonMetrics: ["latency", "throughput", "reliability", "cost"],
  };
}

// ---------------------------------------------------------------------------
// GET handler
// ---------------------------------------------------------------------------

export async function GET(request: NextRequest) {
  const { searchParams } = new URL(request.url);
  const sort = searchParams.get("sort") || "reputation";
  const region = searchParams.get("region") || undefined;

  const data = generateProviders(region, sort);

  return NextResponse.json(data, {
    headers: {
      "Cache-Control": "public, s-maxage=60, stale-while-revalidate=120",
    },
  });
}
