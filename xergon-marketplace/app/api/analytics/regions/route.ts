import { NextResponse } from "next/server";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface RegionData {
  region: string;
  requests: number;
  tokens: number;
  providers: number;
  averageLatencyMs: number;
  marketShare: number;
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

const REGION_FLAGS: Record<string, string> = {
  "US-East": "🇺🇸",
  "US-West": "🇺🇸",
  "EU-West": "🇪🇺",
  "EU-North": "🇸🇪",
  "Asia-Pacific": "🌏",
};

function generateRegions(): RegionData[] {
  const rand = seededRandom(77);
  const regions = ["US-East", "US-West", "EU-West", "EU-North", "Asia-Pacific"];

  const data = regions.map((region, i) => {
    const r = seededRandom(i * 555 + 22);
    const requests = Math.floor(5000 + r() * 25000);
    const tokens = Math.floor(requests * (600 + r() * 600));
    const providers = Math.floor(2 + r() * 5);
    const averageLatencyMs = Math.floor(50 + r() * 350 + i * 30);

    return { region, requests, tokens, providers, averageLatencyMs, marketShare: 0 };
  });

  const totalReqs = data.reduce((s, d) => s + d.requests, 0);
  data.forEach((d) => {
    d.marketShare = parseFloat(((d.requests / totalReqs) * 100).toFixed(1));
  });

  return data;
}

// ---------------------------------------------------------------------------
// GET handler
// ---------------------------------------------------------------------------

export async function GET() {
  const data = generateRegions();

  return NextResponse.json(data, {
    headers: {
      "Cache-Control": "public, s-maxage=60, stale-while-revalidate=120",
    },
  });
}
