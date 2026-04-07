import { NextResponse } from "next/server";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface EarningsDaily {
  date: string;
  earningsNanoErg: number;
  requests: number;
  tokensServed: number;
  uniqueUsers: number;
}

interface EarningsByModel {
  modelId: string;
  earningsNanoErg: number;
  requests: number;
  tokensServed: number;
}

interface EarningsResponse {
  provider: { address: string; region: string; models: string[] };
  summary: {
    totalEarningsNanoErg: number;
    totalRequests: number;
    totalTokensServed: number;
    averageLatencyMs: number;
    uptime: number;
    period: { start: string; end: string };
  };
  daily: EarningsDaily[];
  byModel: EarningsByModel[];
}

// ---------------------------------------------------------------------------
// Deterministic mock data generator
// ---------------------------------------------------------------------------

function generateMockEarnings(address: string): EarningsResponse {
  const now = new Date();
  const days = 30;

  // Seed from address for deterministic data
  let seed = 0;
  for (let i = 0; i < address.length; i++)
    seed = (seed * 31 + address.charCodeAt(i)) | 0;

  function rand(): number {
    seed = (seed * 16807 + 12345) & 0x7fffffff;
    return seed / 0x7fffffff;
  }

  const models = [
    "llama-3.1-70b",
    "qwen2.5-72b",
    "mistral-7b",
    "deepseek-coder-33b",
    "phi-3-medium",
  ];

  const regions = [
    "North America",
    "Europe",
    "Asia",
    "South America",
    "Oceania",
  ];
  const region = regions[Math.floor(rand() * regions.length)];

  // Generate daily data for 30 days
  const daily: EarningsDaily[] = Array.from({ length: days }, (_, i) => {
    const date = new Date(now);
    date.setDate(date.getDate() - (days - 1 - i));
    const dayOfWeek = date.getDay();
    // Lower earnings on weekends
    const weekendFactor = dayOfWeek === 0 || dayOfWeek === 6 ? 0.6 : 1;
    const baseEarnings = 15_000_000_000 + rand() * 35_000_000_000; // 15-50 ERG range in nanoERG
    const earnings = Math.floor(baseEarnings * weekendFactor);
    const requests = Math.floor((earnings / 1e9) * (80 + rand() * 120));
    const tokensServed = Math.floor(requests * (200 + rand() * 800));
    const uniqueUsers = Math.floor(requests * (0.1 + rand() * 0.3));

    return {
      date: date.toISOString().split("T")[0],
      earningsNanoErg: earnings,
      requests,
      tokensServed,
      uniqueUsers,
    };
  });

  // Generate per-model data
  const byModel: EarningsByModel[] = models.map((modelId) => {
    const modelWeight = 0.1 + rand() * 0.35;
    const modelEarnings = Math.floor(
      daily.reduce((s, d) => s + d.earningsNanoErg, 0) * modelWeight,
    );
    return {
      modelId,
      earningsNanoErg: modelEarnings,
      requests: Math.floor(modelEarnings / 1e9 * (80 + rand() * 120)),
      tokensServed: Math.floor(
        (modelEarnings / 1e9) * (200 + rand() * 800),
      ),
    };
  });

  // Normalize byModel so totals roughly match daily totals
  const totalByModelEarnings = byModel.reduce(
    (s, m) => s + m.earningsNanoErg,
    0,
  );
  const totalDailyEarnings = daily.reduce(
    (s, d) => s + d.earningsNanoErg,
    0,
  );
  const scale = totalByModelEarnings > 0 ? totalDailyEarnings / totalByModelEarnings : 1;
  for (const m of byModel) {
    m.earningsNanoErg = Math.floor(m.earningsNanoErg * scale);
    m.requests = Math.floor(m.requests * scale);
    m.tokensServed = Math.floor(m.tokensServed * scale);
  }

  const startDate = new Date(now);
  startDate.setDate(startDate.getDate() - days + 1);

  const totalEarningsNanoErg = daily.reduce(
    (s, d) => s + d.earningsNanoErg,
    0,
  );
  const totalRequests = daily.reduce((s, d) => s + d.requests, 0);
  const totalTokensServed = daily.reduce(
    (s, d) => s + d.tokensServed,
    0,
  );

  return {
    provider: {
      address: address || "3WxTQSY6VxEL3UdRp2Nxv5Tf1T9K7mMNcVv3",
      region,
      models,
    },
    summary: {
      totalEarningsNanoErg,
      totalRequests,
      totalTokensServed,
      averageLatencyMs: Math.floor(180 + rand() * 320),
      uptime: Math.floor((96 + rand() * 4) * 10) / 10,
      period: {
        start: startDate.toISOString().split("T")[0],
        end: now.toISOString().split("T")[0],
      },
    },
    daily,
    byModel: byModel.sort(
      (a, b) => b.earningsNanoErg - a.earningsNanoErg,
    ),
  };
}

// ---------------------------------------------------------------------------
// GET handler
// ---------------------------------------------------------------------------

export async function GET(request: Request) {
  try {
    const { searchParams } = new URL(request.url);
    const address =
      searchParams.get("address") || "3WxTQSY6VxEL3UdRp2Nxv5Tf1T9K7mMNcVv3";

    const data = generateMockEarnings(address);

    return NextResponse.json({ ...data, degraded: true });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : "Internal server error" },
      { status: 500 },
    );
  }
}
