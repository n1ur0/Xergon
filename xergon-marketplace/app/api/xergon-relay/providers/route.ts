import { NextResponse } from "next/server";

const RELAY_BASE =
  process.env.XERGON_RELAY_BASE ?? "http://127.0.0.1:9090";

// ---------------------------------------------------------------------------
// Mock data (used when relay is unreachable)
// ---------------------------------------------------------------------------

function mockProviders() {
  const now = Date.now();
  const regions = ["US", "EU", "Asia", "US", "EU", "Asia", "US", "EU", "US", "Asia", "EU", "US"];
  const gpus = [
    "NVIDIA RTX 4090",
    "NVIDIA A100 80GB",
    "NVIDIA H100 80GB",
    "AMD MI300X",
    "NVIDIA RTX 3090",
    "NVIDIA A6000",
  ];
  const modelSets = [
    ["llama-3.1-70b", "llama-3.1-8b", "mistral-7b"],
    ["qwen2.5-72b", "qwen2.5-7b"],
    ["deepseek-coder-33b", "codestral-22b"],
    ["llama-3.1-70b", "gemma-2-27b", "phi-3-medium"],
    ["mistral-7b", "yi-1.5-34b", "command-r-35b"],
    ["llama-3.1-8b", "phi-3-medium", "mistral-7b"],
    ["qwen2.5-72b", "llama-3.1-70b", "deepseek-coder-33b"],
    ["gemma-2-27b", "codestral-22b", "mistral-7b"],
    ["llama-3.1-70b", "llama-3.1-8b", "qwen2.5-72b", "mistral-7b"],
    ["phi-3-medium", "yi-1.5-34b"],
    ["deepseek-coder-33b", "llama-3.1-70b", "command-r-35b"],
    ["mistral-7b", "qwen2.5-7b", "gemma-2-27b"],
  ];

  return Array.from({ length: 12 }, (_, i) => {
    const status: "online" | "degraded" | "offline" =
      i === 11 ? "offline" : i === 7 ? "degraded" : "online";
    const uptime =
      status === "offline" ? 0 : status === "degraded" ? 85 + Math.random() * 10 : 95 + Math.random() * 5;
    const models = modelSets[i % modelSets.length];

    // Generate 7-day uptime history
    const uptimeHistory = Array.from({ length: 7 }, () =>
      status === "offline"
        ? 0
        : status === "degraded"
          ? 80 + Math.random() * 15
          : 93 + Math.random() * 7,
    );

    // Per-model pricing (base + small variance per model)
    const basePrice = 50_000 + Math.floor(Math.random() * 200_000);
    const modelPricing: Record<string, number> = {};
    for (const m of models) {
      modelPricing[m] = basePrice + Math.floor(Math.random() * 50_000);
    }

    return {
      endpoint: `https://node-${String(i + 1).padStart(3, "0")}.xergon.${regions[i].toLowerCase()}.net`,
      name: `XergonNode-${String(i + 1).padStart(3, "0")}`,
      region: regions[i],
      models,
      uptime: Math.round(uptime * 10) / 10,
      totalTokens: Math.floor(500_000 + Math.random() * 10_000_000),
      aiPoints: Math.floor(100 + Math.random() * 5000),
      pricePer1mTokens: basePrice,
      status,
      lastSeen: new Date(now - Math.floor(Math.random() * 300_000)).toISOString(),
      gpuInfo: gpus[i % gpus.length],
      latencyMs: Math.floor(50 + Math.random() * 400),
      ergoAddress: `9${Array.from({ length: 9 }, () =>
        Math.floor(Math.random() * 10),
      ).join("")}`,
      uptimeHistory: uptimeHistory.map((v) => Math.round(v * 10) / 10),
      modelPricing,
    };
  });
}

// ---------------------------------------------------------------------------
// Relay provider response shape
// ---------------------------------------------------------------------------

interface RelayProvider {
  endpoint?: string;
  name?: string;
  region?: string;
  models?: string[];
  status?: string;
  uptime?: number;
  gpu?: string;
  latency_ms?: number;
  tokens_processed?: number;
  ai_points?: number;
  price_per_million?: number;
  last_seen?: string;
  ergo_address?: string;
}

// ---------------------------------------------------------------------------
// Transform relay data into ProviderInfo[]
// ---------------------------------------------------------------------------

function transformProvider(raw: RelayProvider) {
  const now = Date.now();
  const statusMap: Record<string, "online" | "degraded" | "offline"> = {
    online: "online",
    active: "online",
    healthy: "online",
    degraded: "degraded",
    slow: "degraded",
    offline: "offline",
    down: "offline",
  };

  return {
    endpoint: raw.endpoint ?? "",
    name: raw.name ?? raw.endpoint ?? "Unknown",
    region: raw.region ?? "Other",
    models: raw.models ?? [],
    uptime: raw.uptime ?? 0,
    totalTokens: raw.tokens_processed ?? 0,
    aiPoints: raw.ai_points ?? 0,
    pricePer1mTokens: raw.price_per_million ?? 0,
    status: statusMap[raw.status ?? ""] ?? "online",
    lastSeen: raw.last_seen ?? new Date(now).toISOString(),
    gpuInfo: raw.gpu ?? "Unknown GPU",
    latencyMs: raw.latency_ms ?? 0,
    ergoAddress: raw.ergo_address,
    uptimeHistory: undefined,
    modelPricing: undefined,
  };
}

// ---------------------------------------------------------------------------
// GET handler
// ---------------------------------------------------------------------------

export async function GET(request: Request) {
  try {
    const { searchParams } = new URL(request.url);
    const region = searchParams.get("region");
    const status = searchParams.get("status");
    const model = searchParams.get("model");
    const sort = searchParams.get("sort");
    const order = searchParams.get("order");

    // Build relay query params
    const relayParams = new URLSearchParams();
    if (region) relayParams.set("region", region);
    if (status) relayParams.set("status", status);
    if (model) relayParams.set("model", model);

    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 5000);

    const res = await fetch(
      `${RELAY_BASE}/v1/providers${relayParams.toString() ? `?${relayParams.toString()}` : ""}`,
      { signal: controller.signal },
    );

    clearTimeout(timeout);

    if (!res.ok) {
      const mock = mockProviders();
      return NextResponse.json({ providers: mock, degraded: true });
    }

    const data = await res.json();

    // The relay may return { providers: [...] } or a bare array
    const rawProviders: RelayProvider[] =
      data?.providers ?? (Array.isArray(data) ? data : []);

    let providers = rawProviders.map(transformProvider);

    // Server-side filtering (relay may not support all filters)
    if (status && status !== "all") {
      providers = providers.filter((p) => p.status === status);
    }
    if (region && region !== "all") {
      providers = providers.filter((p) => p.region === region);
    }
    if (model && model !== "all") {
      providers = providers.filter((p) => p.models.includes(model));
    }

    // Server-side sorting
    if (sort) {
      const dir = order === "asc" ? 1 : -1;
      providers.sort((a, b) => {
        switch (sort) {
          case "aiPoints":
            return (a.aiPoints - b.aiPoints) * dir;
          case "uptime":
            return (a.uptime - b.uptime) * dir;
          case "tokens":
            return (a.totalTokens - b.totalTokens) * dir;
          case "price":
            return (a.pricePer1mTokens - b.pricePer1mTokens) * dir;
          case "name":
            return a.name.localeCompare(b.name) * dir;
          default:
            return 0;
        }
      });
    }

    return NextResponse.json({ providers, degraded: false });
  } catch {
    // Relay unreachable — return mock data with degraded flag
    return NextResponse.json({ providers: mockProviders(), degraded: true });
  }
}
