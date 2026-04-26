import { NextResponse } from "next/server";

import { RELAY_BASE } from "@/lib/api/server-sdk";

// ---------------------------------------------------------------------------
// Mock portfolio data (used when relay is unreachable or for demo)
// ---------------------------------------------------------------------------

function mockPortfolio(id: string) {
  const now = new Date();
  const daysAgo = (d: number) => new Date(now.getTime() - d * 86400000).toISOString().slice(0, 10);

  const performanceDays = 30;
  const requests = Array.from({ length: performanceDays }, (_, i) => ({
    date: daysAgo(performanceDays - 1 - i),
    value: Math.floor(500 + Math.random() * 2000 + i * 50),
  }));
  const latency = Array.from({ length: performanceDays }, (_, i) => ({
    date: daysAgo(performanceDays - 1 - i),
    value: Math.floor(80 + Math.random() * 120),
  }));
  const availability = Array.from({ length: performanceDays }, () => ({
    date: daysAgo(performanceDays - Math.floor(Math.random() * performanceDays) - 1),
    value: 95 + Math.floor(Math.random() * 5),
  }));

  const modelNames = ["llama-3.3-70b", "mistral-small-24b", "qwen2.5-72b", "deepseek-coder-33b"];
  const models = modelNames.map((name, i) => ({
    id: name,
    name: name.replace(/-/g, " ").replace(/\b\w/g, c => c.toUpperCase()),
    description: `High-quality ${i < 2 ? "general-purpose" : "specialized"} model.`,
    tier: i === 1 ? "free" : "pro",
    pricePerInputTokenNanoerg: i === 1 ? 0 : 100 + i * 50,
    pricePerOutputTokenNanoerg: i === 1 ? 0 : 150 + i * 50,
    contextWindow: [8192, 32768, 32768, 16384][i],
    tags: [["Smart", "Code"], ["Fast", "Creative"], ["Smart", "Code"], ["Code"]][i],
    available: true,
    requestCount: Math.floor(10000 + Math.random() * 90000),
    avgLatencyMs: Math.floor(80 + Math.random() * 200),
    benchmarks: {
      MMLU: 70 + Math.floor(Math.random() * 20),
      HumanEval: 50 + Math.floor(Math.random() * 40),
      "GSM8K": 60 + Math.floor(Math.random() * 30),
    },
  }));

  return {
    providerId: id,
    displayName: id.length > 16 ? `Node ${id.slice(0, 8)}` : id,
    bio: "Experienced compute provider specializing in large language models and code generation.",
    website: "https://xergon.network",
    socialLinks: [{ platform: "twitter", url: "https://x.com/xergon" }],
    stats: {
      totalModels: modelNames.length,
      totalRequests: 487500,
      uptimePct: 99.2,
      avgRating: 4.6,
      totalRevenue: 125000000000,
      repeatCustomers: 342,
      totalUsersServed: 1850,
    },
    skills: [
      { id: "nlp", label: "NLP", category: "nlp" as const },
      { id: "code", label: "Code Generation", category: "code" as const },
      { id: "multimodal", label: "Multimodal", category: "multimodal" as const },
      { id: "embeddings", label: "Embeddings", category: "embeddings" as const },
    ],
    performanceHistory: { requests, latency, availability },
    models,
    reviews: [
      { id: "1", author: "0x3f8a...b2c1", rating: 5, content: "Excellent uptime and fast response times. Highly recommended.", date: daysAgo(5), model: "llama-3.3-70b" },
      { id: "2", author: "0x7d2e...f4a9", rating: 4, content: "Good provider overall. Occasional latency spikes during peak hours.", date: daysAgo(10) },
      { id: "3", author: "0xa1b3...c7d5", rating: 5, content: "Best pricing for code generation models. Great value.", date: daysAgo(15), model: "deepseek-coder-33b" },
      { id: "4", author: "0xe9f2...a4b8", rating: 4, content: "Reliable and fast. Would recommend for production workloads.", date: daysAgo(20) },
    ],
    activity: [
      { id: "1", type: "model_added" as const, description: "Added new model: llama-3.3-70b", date: daysAgo(2) },
      { id: "2", type: "price_change" as const, description: "Reduced pricing for mistral-small-24b by 15%", date: daysAgo(5) },
      { id: "3", type: "milestone" as const, description: "Reached 100,000 total requests served", date: daysAgo(12) },
      { id: "4", type: "certification" as const, description: "Earned SLA Compliant certification", date: daysAgo(18) },
      { id: "5", type: "achievement" as const, description: "Top 10% provider by uptime", date: daysAgo(25) },
      { id: "6", type: "status_change" as const, description: "Provider status: online", date: daysAgo(30) },
    ],
    certifications: [
      { id: "1", label: "Verified Provider", icon: "\u2705", description: "Identity verified on-chain", earnedDate: daysAgo(90) },
      { id: "2", label: "Top Provider", icon: "\uD83C\uDFC6", description: "Top 10% by reputation score", earnedDate: daysAgo(45) },
      { id: "3", label: "SLA Compliant", icon: "\uD83D\uDD04", description: "99%+ uptime for 30 consecutive days", earnedDate: daysAgo(18) },
      { id: "4", label: "Low Latency", icon: "\u26A1", description: "Average latency under 150ms", earnedDate: daysAgo(30) },
    ],
    joinedDate: daysAgo(180),
  };
}

// ---------------------------------------------------------------------------
// GET /api/providers/[id]/portfolio
// ---------------------------------------------------------------------------

export async function GET(
  _request: Request,
  { params }: { params: Promise<{ id: string }> },
) {
  const { id } = await params;

  try {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 5000);

    const res = await fetch(`${RELAY_BASE}/v1/providers/${encodeURIComponent(id)}/portfolio`, {
      signal: controller.signal,
    });
    clearTimeout(timeout);

    if (res.ok) {
      const data = await res.json();
      return NextResponse.json(data);
    }
  } catch {
    // Relay unreachable -- return mock data
  }

  return NextResponse.json(mockPortfolio(id));
}

// ---------------------------------------------------------------------------
// PATCH /api/providers/[id]/portfolio  (owner only)
// ---------------------------------------------------------------------------

export async function PATCH(
  request: Request,
  { params }: { params: Promise<{ id: string }> },
) {
  const { id } = await params;

  try {
    const body = await request.json();

    // In production, verify ownership via wallet auth
    const authHeader = request.headers.get("authorization");
    if (!authHeader) {
      return NextResponse.json(
        { error: "Authentication required" },
        { status: 401 },
      );
    }

    // Attempt relay update
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 5000);

    const res = await fetch(`${RELAY_BASE}/v1/providers/${encodeURIComponent(id)}/portfolio`, {
      method: "PATCH",
      headers: {
        "Content-Type": "application/json",
        Authorization: authHeader,
      },
      body: JSON.stringify(body),
      signal: controller.signal,
    });
    clearTimeout(timeout);

    if (res.ok) {
      const data = await res.json();
      return NextResponse.json(data);
    }

    return NextResponse.json(
      { error: "Failed to update portfolio" },
      { status: res.status },
    );
  } catch {
    return NextResponse.json(
      { error: "Invalid request body" },
      { status: 400 },
    );
  }
}
