/**
 * Request Analytics API Routes
 *
 * Handles: GET /api/analytics/requests — list (paginated, filtered)
 *          GET /api/analytics/requests/summary — aggregate stats
 *          GET /api/analytics/requests/timeline — time series data
 *
 * Uses mock/demo data when no real backend is connected.
 */

import { NextRequest, NextResponse } from "next/server";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type RequestStatus = "success" | "error" | "timeout" | "rate_limited";

export interface InferenceRequest {
  id: string;
  timestamp: string;
  model: string;
  provider: string;
  providerPk: string;
  status: RequestStatus;
  latencyMs: number;
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  costNanoerg: number;
  region: string;
  errorMessage?: string;
  requestBody?: string;
  responseBody?: string;
}

export interface RequestSummary {
  totalRequests: number;
  successCount: number;
  errorCount: number;
  timeoutCount: number;
  rateLimitedCount: number;
  successRate: number;
  avgLatencyMs: number;
  p50LatencyMs: number;
  p90LatencyMs: number;
  p99LatencyMs: number;
  totalTokens: number;
  totalCostNanoerg: number;
  avgTokensPerRequest: number;
  uniqueModels: number;
  uniqueProviders: number;
}

export interface TimelinePoint {
  timestamp: string;
  requests: number;
  successes: number;
  errors: number;
  avgLatencyMs: number;
  totalTokens: number;
  totalCostNanoerg: number;
}

// ---------------------------------------------------------------------------
// Mock data generator
// ---------------------------------------------------------------------------

const MODELS = [
  "llama-3.3-70b",
  "qwen3.5-4b-f16.gguf",
  "mistral-small-24b",
  "llama-3.1-8b",
  "deepseek-r1-distill-32b",
  "phi-4-14b",
  "gemma-3-27b",
];

const PROVIDERS = [
  { name: "NodeAlpha", pk: "3kF8x...a2dE", region: "North America" },
  { name: "ErgoCompute", pk: "7pL2y...b9cF", region: "Europe" },
  { name: "AsiaNode", pk: "1mN5w...c4gH", region: "Asia" },
  { name: "SouthNode", pk: "9qR3z...d7jK", region: "South America" },
  { name: "OceanCompute", pk: "5vT8b...e1iL", region: "Oceania" },
];

const STATUSES: RequestStatus[] = ["success", "success", "success", "success", "success", "success", "success", "success", "error", "timeout", "rate_limited"];

function seededRandom(seed: number): () => number {
  let s = seed;
  return () => {
    s = (s * 16807 + 0) % 2147483647;
    return s / 2147483647;
  };
}

function generateMockRequests(count: number): InferenceRequest[] {
  const rand = seededRandom(42);
  const now = Date.now();
  const requests: InferenceRequest[] = [];

  for (let i = 0; i < count; i++) {
    const status = STATUSES[Math.floor(rand() * STATUSES.length)];
    const inputTokens = Math.floor(rand() * 2000) + 50;
    const outputTokens = status === "success" ? Math.floor(rand() * 1500) + 20 : 0;
    const latencyMs = status === "success"
      ? Math.floor(rand() * 3000) + 50
      : status === "timeout"
        ? 30000 + Math.floor(rand() * 10000)
        : Math.floor(rand() * 500) + 10;

    requests.push({
      id: `req_${(now - i * 30000).toString(36)}_${i}`,
      timestamp: new Date(now - i * 30000 - Math.floor(rand() * 25000)).toISOString(),
      model: MODELS[Math.floor(rand() * MODELS.length)],
      provider: PROVIDERS[Math.floor(rand() * PROVIDERS.length)].name,
      providerPk: PROVIDERS[Math.floor(rand() * PROVIDERS.length)].pk,
      status,
      latencyMs,
      inputTokens,
      outputTokens,
      totalTokens: inputTokens + outputTokens,
      costNanoerg: (inputTokens + outputTokens) * (Math.floor(rand() * 300) + 20),
      region: PROVIDERS[Math.floor(rand() * PROVIDERS.length)].region,
      errorMessage: status === "error" ? "Model inference failed: CUDA out of memory" : undefined,
      requestBody: JSON.stringify({ messages: [{ role: "user", content: "Hello world" }], model: MODELS[0], max_tokens: 512 }),
      responseBody: status === "success" ? JSON.stringify({ id: "chatcmpl-xxx", choices: [{ message: { content: "Hello! How can I help you today?" } }] }) : undefined,
    });
  }

  return requests;
}

// Cache mock data in-memory for the lifetime of the serverless function
let _mockRequests: InferenceRequest[] | null = null;

function getMockRequests(): InferenceRequest[] {
  if (!_mockRequests) {
    _mockRequests = generateMockRequests(500);
  }
  return _mockRequests;
}

// ---------------------------------------------------------------------------
// Route handler
// ---------------------------------------------------------------------------

export async function GET(request: NextRequest) {
  const { searchParams } = new URL(request.url);
  const action = searchParams.get("action") ?? "list";

  const allRequests = getMockRequests();

  if (action === "summary") {
    return handleSummary(allRequests);
  }

  if (action === "timeline") {
    return handleTimeline(allRequests, searchParams);
  }

  // Default: paginated list
  return handleList(allRequests, searchParams);
}

// ---------------------------------------------------------------------------
// List handler
// ---------------------------------------------------------------------------

function handleList(allRequests: InferenceRequest[], searchParams: URLSearchParams) {
  let filtered = [...allRequests];

  // Apply filters
  const model = searchParams.get("model");
  if (model) filtered = filtered.filter((r) => r.model === model);

  const provider = searchParams.get("provider");
  if (provider) filtered = filtered.filter((r) => r.provider === provider);

  const status = searchParams.get("status") as RequestStatus | null;
  if (status) filtered = filtered.filter((r) => r.status === status);

  const region = searchParams.get("region");
  if (region) filtered = filtered.filter((r) => r.region === region);

  const startDate = searchParams.get("startDate");
  if (startDate) {
    const start = new Date(startDate).getTime();
    filtered = filtered.filter((r) => new Date(r.timestamp).getTime() >= start);
  }

  const endDate = searchParams.get("endDate");
  if (endDate) {
    const end = new Date(endDate).getTime();
    filtered = filtered.filter((r) => new Date(r.timestamp).getTime() <= end);
  }

  const minLatency = searchParams.get("minLatency");
  if (minLatency) filtered = filtered.filter((r) => r.latencyMs >= Number(minLatency));

  const maxLatency = searchParams.get("maxLatency");
  if (maxLatency) filtered = filtered.filter((r) => r.latencyMs <= Number(maxLatency));

  const minTokens = searchParams.get("minTokens");
  if (minTokens) filtered = filtered.filter((r) => r.totalTokens >= Number(minTokens));

  const maxTokens = searchParams.get("maxTokens");
  if (maxTokens) filtered = filtered.filter((r) => r.totalTokens <= Number(maxTokens));

  // Sort by timestamp descending
  filtered.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime());

  // Pagination
  const page = Math.max(1, Number(searchParams.get("page") ?? 1));
  const pageSize = Math.min(100, Math.max(1, Number(searchParams.get("pageSize") ?? 20)));
  const total = filtered.length;
  const totalPages = Math.ceil(total / pageSize);
  const start = (page - 1) * pageSize;
  const paginated = filtered.slice(start, start + pageSize);

  // Build histogram buckets for latency distribution
  const latencyBuckets = [0, 100, 250, 500, 1000, 2000, 5000, 10000, 30000];
  const latencyHistogram = latencyBuckets.slice(0, -1).map((low, i) => {
    const high = latencyBuckets[i + 1];
    const count = allRequests.filter((r) => r.latencyMs >= low && r.latencyMs < high).length;
    return { range: `${low}-${high}ms`, low, high, count };
  });

  // Error breakdown
  const errorBreakdown = {
    error: allRequests.filter((r) => r.status === "error").length,
    timeout: allRequests.filter((r) => r.status === "timeout").length,
    rate_limited: allRequests.filter((r) => r.status === "rate_limited").length,
  };

  // Model usage breakdown
  const modelUsage: Record<string, { requests: number; tokens: number; cost: number; avgLatency: number }> = {};
  for (const r of allRequests) {
    if (!modelUsage[r.model]) modelUsage[r.model] = { requests: 0, tokens: 0, cost: 0, avgLatency: 0 };
    modelUsage[r.model].requests++;
    modelUsage[r.model].tokens += r.totalTokens;
    modelUsage[r.model].cost += r.costNanoerg;
    modelUsage[r.model].avgLatency += r.latencyMs;
  }
  for (const m of Object.values(modelUsage)) {
    m.avgLatency = Math.round(m.avgLatency / m.requests);
  }

  // Provider performance
  const providerPerf: Record<string, { requests: number; errors: number; avgLatency: number; totalLatency: number }> = {};
  for (const r of allRequests) {
    if (!providerPerf[r.provider]) providerPerf[r.provider] = { requests: 0, errors: 0, avgLatency: 0, totalLatency: 0 };
    providerPerf[r.provider].requests++;
    providerPerf[r.provider].totalLatency += r.latencyMs;
    if (r.status !== "success") providerPerf[r.provider].errors++;
  }
  for (const p of Object.values(providerPerf)) {
    p.avgLatency = Math.round(p.totalLatency / p.requests);
  }

  return NextResponse.json({
    requests: paginated,
    pagination: { page, pageSize, total, totalPages },
    filters: {
      availableModels: [...new Set(allRequests.map((r) => r.model))],
      availableProviders: [...new Set(allRequests.map((r) => r.provider))],
      availableRegions: [...new Set(allRequests.map((r) => r.region))],
      availableStatuses: ["success", "error", "timeout", "rate_limited"],
    },
    latencyHistogram,
    errorBreakdown,
    modelUsage,
    providerPerformance: providerPerf,
  });
}

// ---------------------------------------------------------------------------
// Summary handler
// ---------------------------------------------------------------------------

function handleSummary(allRequests: InferenceRequest[]) {
  const totalRequests = allRequests.length;
  const successes = allRequests.filter((r) => r.status === "success");
  const successCount = successes.length;
  const errorCount = allRequests.filter((r) => r.status === "error").length;
  const timeoutCount = allRequests.filter((r) => r.status === "timeout").length;
  const rateLimitedCount = allRequests.filter((r) => r.status === "rate_limited").length;

  const latencies = successes.map((r) => r.latencyMs).sort((a, b) => a - b);
  const percentile = (arr: number[], p: number) => {
    if (arr.length === 0) return 0;
    const idx = Math.ceil((p / 100) * arr.length) - 1;
    return arr[Math.max(0, idx)];
  };

  const totalTokens = allRequests.reduce((s, r) => s + r.totalTokens, 0);
  const totalCost = allRequests.reduce((s, r) => s + r.costNanoerg, 0);

  const summary: RequestSummary = {
    totalRequests,
    successCount,
    errorCount,
    timeoutCount,
    rateLimitedCount,
    successRate: totalRequests > 0 ? (successCount / totalRequests) * 100 : 0,
    avgLatencyMs: latencies.length > 0 ? Math.round(latencies.reduce((s, l) => s + l, 0) / latencies.length) : 0,
    p50LatencyMs: percentile(latencies, 50),
    p90LatencyMs: percentile(latencies, 90),
    p99LatencyMs: percentile(latencies, 99),
    totalTokens,
    totalCostNanoerg: totalCost,
    avgTokensPerRequest: totalRequests > 0 ? Math.round(totalTokens / totalRequests) : 0,
    uniqueModels: new Set(allRequests.map((r) => r.model)).size,
    uniqueProviders: new Set(allRequests.map((r) => r.provider)).size,
  };

  return NextResponse.json(summary);
}

// ---------------------------------------------------------------------------
// Timeline handler
// ---------------------------------------------------------------------------

function handleTimeline(allRequests: InferenceRequest[], searchParams: URLSearchParams) {
  const interval = searchParams.get("interval") ?? "hour"; // hour, day
  const hours = Number(searchParams.get("hours") ?? 24);

  const now = Date.now();
  const startTime = now - hours * 3600 * 1000;
  const filtered = allRequests.filter((r) => new Date(r.timestamp).getTime() >= startTime);

  // Bucket by interval
  const bucketMs = interval === "day" ? 86400000 : 3600000;
  const buckets: Map<number, { requests: number; successes: number; errors: number; totalLatency: number; totalTokens: number; totalCost: number }> = new Map();

  // Initialize buckets
  const bucketStart = Math.floor(startTime / bucketMs) * bucketMs;
  for (let t = bucketStart; t <= now; t += bucketMs) {
    buckets.set(t, { requests: 0, successes: 0, errors: 0, totalLatency: 0, totalTokens: 0, totalCost: 0 });
  }

  for (const r of filtered) {
    const ts = new Date(r.timestamp).getTime();
    const bucketKey = Math.floor(ts / bucketMs) * bucketMs;
    const bucket = buckets.get(bucketKey);
    if (bucket) {
      bucket.requests++;
      if (r.status === "success") bucket.successes++;
      else bucket.errors++;
      bucket.totalLatency += r.latencyMs;
      bucket.totalTokens += r.totalTokens;
      bucket.totalCost += r.costNanoerg;
    }
  }

  const timeline: TimelinePoint[] = Array.from(buckets.entries())
    .map(([ts, data]) => ({
      timestamp: new Date(ts).toISOString(),
      requests: data.requests,
      successes: data.successes,
      errors: data.errors,
      avgLatencyMs: data.requests > 0 ? Math.round(data.totalLatency / data.requests) : 0,
      totalTokens: data.totalTokens,
      totalCostNanoerg: data.totalCost,
    }))
    .sort((a, b) => new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime());

  return NextResponse.json({ interval, hours, timeline });
}
