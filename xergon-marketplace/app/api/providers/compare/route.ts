import { NextRequest, NextResponse } from "next/server";

// ── Types ──

interface ProviderComparison {
  id: string;
  name: string;
  avatar: string;
  tier: string;
  status: "online" | "offline" | "maintenance";
  rating: number;
  reviewCount: number;
  metrics: {
    latencyAvg: number;
    latencyP50: number;
    latencyP95: number;
    throughput: number;
    reliability: number;
    uptime: number;
    costPer1M: number;
    supportedModels: number;
    regions: string[];
    apiResponseTime: number;
    errorRate: number;
  };
  features: {
    streaming: boolean;
    batching: boolean;
    embeddings: boolean;
    vision: boolean;
    functionCalling: boolean;
    jsonMode: boolean;
  };
}

// ── Mock Data ──

const PROVIDERS: Record<string, ProviderComparison> = {
  "ergo-infra-alpha": {
    id: "ergo-infra-alpha",
    name: "Ergo Infra Alpha",
    avatar: "🟢",
    tier: "Premium",
    status: "online",
    rating: 4.8,
    reviewCount: 342,
    metrics: {
      latencyAvg: 320,
      latencyP50: 280,
      latencyP95: 680,
      throughput: 48,
      reliability: 99.7,
      uptime: 99.95,
      costPer1M: 0.45,
      supportedModels: 18,
      regions: ["US-East", "EU-West", "Asia-Pacific"],
      apiResponseTime: 45,
      errorRate: 0.3,
    },
    features: {
      streaming: true,
      batching: true,
      embeddings: true,
      vision: true,
      functionCalling: true,
      jsonMode: true,
    },
  },
  "neural-node-beta": {
    id: "neural-node-beta",
    name: "Neural Node Beta",
    avatar: "🔵",
    tier: "Standard",
    status: "online",
    rating: 4.5,
    reviewCount: 198,
    metrics: {
      latencyAvg: 410,
      latencyP50: 370,
      latencyP95: 890,
      throughput: 38,
      reliability: 99.2,
      uptime: 99.8,
      costPer1M: 0.32,
      supportedModels: 14,
      regions: ["US-East", "EU-West"],
      apiResponseTime: 62,
      errorRate: 0.8,
    },
    features: {
      streaming: true,
      batching: true,
      embeddings: false,
      vision: true,
      functionCalling: true,
      jsonMode: true,
    },
  },
  "sigma-compute": {
    id: "sigma-compute",
    name: "Sigma Compute",
    avatar: "🟣",
    tier: "Premium",
    status: "online",
    rating: 4.9,
    reviewCount: 521,
    metrics: {
      latencyAvg: 250,
      latencyP50: 220,
      latencyP95: 520,
      throughput: 62,
      reliability: 99.9,
      uptime: 99.99,
      costPer1M: 0.58,
      supportedModels: 24,
      regions: ["US-East", "US-West", "EU-West", "EU-North", "Asia-Pacific"],
      apiResponseTime: 32,
      errorRate: 0.1,
    },
    features: {
      streaming: true,
      batching: true,
      embeddings: true,
      vision: true,
      functionCalling: true,
      jsonMode: true,
    },
  },
  "quantum-edge": {
    id: "quantum-edge",
    name: "Quantum Edge",
    avatar: "🟡",
    tier: "Budget",
    status: "online",
    rating: 4.1,
    reviewCount: 87,
    metrics: {
      latencyAvg: 580,
      latencyP50: 520,
      latencyP95: 1200,
      throughput: 22,
      reliability: 97.8,
      uptime: 98.5,
      costPer1M: 0.18,
      supportedModels: 8,
      regions: ["US-East"],
      apiResponseTime: 95,
      errorRate: 2.1,
    },
    features: {
      streaming: true,
      batching: false,
      embeddings: false,
      vision: false,
      functionCalling: false,
      jsonMode: true,
    },
  },
  "ergo-mesh-pro": {
    id: "ergo-mesh-pro",
    name: "Ergo Mesh Pro",
    avatar: "🔴",
    tier: "Standard",
    status: "online",
    rating: 4.6,
    reviewCount: 276,
    metrics: {
      latencyAvg: 350,
      latencyP50: 310,
      latencyP95: 720,
      throughput: 44,
      reliability: 99.5,
      uptime: 99.9,
      costPer1M: 0.38,
      supportedModels: 16,
      regions: ["US-East", "EU-West", "Asia-Pacific"],
      apiResponseTime: 51,
      errorRate: 0.5,
    },
    features: {
      streaming: true,
      batching: true,
      embeddings: true,
      vision: true,
      functionCalling: true,
      jsonMode: false,
    },
  },
  "phoenix-node": {
    id: "phoenix-node",
    name: "Phoenix Node",
    avatar: "🟠",
    tier: "Budget",
    status: "maintenance",
    rating: 3.9,
    reviewCount: 64,
    metrics: {
      latencyAvg: 650,
      latencyP50: 580,
      latencyP95: 1400,
      throughput: 18,
      reliability: 96.5,
      uptime: 97.2,
      costPer1M: 0.15,
      supportedModels: 6,
      regions: ["EU-West"],
      apiResponseTime: 110,
      errorRate: 3.5,
    },
    features: {
      streaming: true,
      batching: false,
      embeddings: false,
      vision: false,
      functionCalling: false,
      jsonMode: true,
    },
  },
};

// ── Route Handler ──

export async function GET(request: NextRequest) {
  const { searchParams } = new URL(request.url);
  const providerIds = searchParams.get("provider_ids")?.split(",").map((s) => s.trim()).filter(Boolean);

  // If specific IDs requested
  if (providerIds && providerIds.length > 0) {
    const results = providerIds
      .map((id) => PROVIDERS[id])
      .filter(Boolean);

    if (results.length === 0) {
      return NextResponse.json(
        { error: "No providers found for the given IDs" },
        { status: 404 },
      );
    }

    return NextResponse.json({ providers: results });
  }

  // Return all providers
  return NextResponse.json({
    providers: Object.values(PROVIDERS),
    availableIds: Object.keys(PROVIDERS),
  });
}
