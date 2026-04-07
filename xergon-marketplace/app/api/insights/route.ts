/**
 * Provider Insights API Routes
 *
 * Handles: GET /api/insights/overview — market overview stats
 *          GET /api/insights/trending — trending models
 *          GET /api/insights/top-providers — top providers by category
 *          GET /api/insights/pricing — pricing trends
 *          GET /api/insights/demand — demand signals
 *          GET /api/insights/geography — geographic distribution
 *          GET /api/insights/recommendations — personalized recommendations
 */

import { NextRequest, NextResponse } from "next/server";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface MarketOverview {
  totalProviders: number;
  activeProviders: number;
  totalModels: number;
  totalRequests24h: number;
  totalTokens24h: number;
  networkHealth: "healthy" | "degraded" | "critical";
  avgLatencyMs: number;
  uptime24h: number;
  totalVolumeNanoerg: number;
  weeklyChange: {
    providers: number;
    models: number;
    requests: number;
    tokens: number;
  };
}

export interface TrendingModel {
  modelId: string;
  modelName: string;
  category: string;
  requestGrowthPct: number;
  totalRequests: number;
  avgLatencyMs: number;
  providerCount: number;
  trend: "up" | "down" | "stable";
}

export interface TopProvider {
  providerPk: string;
  providerName: string;
  region: string;
  category: "latency" | "cost" | "quality" | "reliability";
  score: number;
  metric: string;
  models: number;
  uptime: number;
}

export interface PricingTrend {
  date: string;
  avgPricePer1MInputNanoerg: number;
  avgPricePer1MOutputNanoerg: number;
  medianPricePer1MInputNanoerg: number;
  medianPricePer1MOutputNanoerg: number;
  minPricePer1MInputNanoerg: number;
  maxPricePer1MInputNanoerg: number;
}

export interface DemandSignal {
  modelCategory: string;
  requests24h: number;
  requests7d: number;
  growthPct: number;
  avgLatencyMs: number;
  supplyProviders: number;
  demandSupplyRatio: number;
  status: "undersupplied" | "balanced" | "oversupplied";
}

export interface GeoDistribution {
  region: string;
  requests: number;
  providers: number;
  avgLatencyMs: number;
  marketSharePct: number;
  changeFromLastWeek: number;
}

export interface Recommendation {
  type: "consumer" | "provider";
  category: string;
  title: string;
  description: string;
  impact: "high" | "medium" | "low";
  model?: string;
  provider?: string;
}

export interface WeeklySummary {
  period: { start: string; end: string };
  totalRequests: number;
  totalTokens: number;
  totalCostNanoerg: number;
  avgLatencyMs: number;
  successRate: number;
  topModel: string;
  topProvider: string;
  highlights: string[];
}

// ---------------------------------------------------------------------------
// Mock data
// ---------------------------------------------------------------------------

function seededRandom(seed: number): () => number {
  let s = seed;
  return () => {
    s = (s * 16807 + 0) % 2147483647;
    return s / 2147483647;
  };
}

const rand = seededRandom(99);

// ---------------------------------------------------------------------------
// Route handler
// ---------------------------------------------------------------------------

export async function GET(request: NextRequest) {
  const { searchParams } = new URL(request.url);
  const endpoint = searchParams.get("endpoint") ?? "overview";

  switch (endpoint) {
    case "overview":
      return handleOverview();
    case "trending":
      return handleTrending();
    case "top-providers":
      return handleTopProviders();
    case "pricing":
      return handlePricing(searchParams);
    case "demand":
      return handleDemand();
    case "geography":
      return handleGeography();
    case "recommendations":
      return handleRecommendations();
    case "weekly-summary":
      return handleWeeklySummary();
    default:
      return NextResponse.json({ error: "Unknown endpoint" }, { status: 400 });
  }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

function handleOverview() {
  const overview: MarketOverview = {
    totalProviders: 47,
    activeProviders: 38,
    totalModels: 23,
    totalRequests24h: 142857,
    totalTokens24h: 58340000,
    networkHealth: "healthy",
    avgLatencyMs: 342,
    uptime24h: 99.7,
    totalVolumeNanoerg: 42500000000,
    weeklyChange: {
      providers: 8.3,
      models: 15.0,
      requests: 23.1,
      tokens: 18.7,
    },
  };
  return NextResponse.json(overview);
}

function handleTrending() {
  const trending: TrendingModel[] = [
    { modelId: "deepseek-r1-distill-32b", modelName: "DeepSeek R1 Distill 32B", category: "Reasoning", requestGrowthPct: 342, totalRequests: 28450, avgLatencyMs: 520, providerCount: 5, trend: "up" },
    { modelId: "qwen3.5-4b-f16.gguf", modelName: "Qwen 3.5 4B (F16)", category: "General", requestGrowthPct: 187, totalRequests: 35200, avgLatencyMs: 120, providerCount: 12, trend: "up" },
    { modelId: "phi-4-14b", modelName: "Phi-4 14B", category: "Reasoning", requestGrowthPct: 156, totalRequests: 12800, avgLatencyMs: 280, providerCount: 4, trend: "up" },
    { modelId: "gemma-3-27b", modelName: "Gemma 3 27B", category: "General", requestGrowthPct: 98, totalRequests: 8900, avgLatencyMs: 410, providerCount: 3, trend: "up" },
    { modelId: "llama-3.3-70b", modelName: "Llama 3.3 70B", category: "General", requestGrowthPct: 45, totalRequests: 52000, avgLatencyMs: 380, providerCount: 8, trend: "stable" },
    { modelId: "mistral-small-24b", modelName: "Mistral Small 24B", category: "Code", requestGrowthPct: 23, totalRequests: 31200, avgLatencyMs: 210, providerCount: 7, trend: "stable" },
    { modelId: "llama-3.1-8b", modelName: "Llama 3.1 8B", category: "General", requestGrowthPct: 12, totalRequests: 41500, avgLatencyMs: 85, providerCount: 15, trend: "stable" },
    { modelId: "codestral-22b", modelName: "Codestral 22B", category: "Code", requestGrowthPct: -5, totalRequests: 9800, avgLatencyMs: 340, providerCount: 3, trend: "down" },
  ];
  return NextResponse.json(trending);
}

function handleTopProviders() {
  const providers: TopProvider[] = [
    { providerPk: "3kF8x...a2dE", providerName: "NodeAlpha", region: "North America", category: "latency", score: 98, metric: "72ms avg", models: 6, uptime: 99.95 },
    { providerPk: "7pL2y...b9cF", providerName: "ErgoCompute", region: "Europe", category: "cost", score: 95, metric: "15 ERG/1M tokens", models: 8, uptime: 99.8 },
    { providerPk: "1mN5w...c4gH", providerName: "AsiaNode", region: "Asia", category: "quality", score: 92, metric: "4.8/5 rating", models: 5, uptime: 99.9 },
    { providerPk: "9qR3z...d7jK", providerName: "SouthNode", region: "South America", category: "reliability", score: 97, metric: "99.98% uptime", models: 4, uptime: 99.98 },
    { providerPk: "5vT8b...e1iL", providerName: "OceanCompute", region: "Oceania", category: "latency", score: 91, metric: "85ms avg", models: 3, uptime: 99.7 },
    { providerPk: "2hJ6c...f3mP", providerName: "ErgoNode EU", region: "Europe", category: "cost", score: 88, metric: "18 ERG/1M tokens", models: 7, uptime: 99.5 },
    { providerPk: "8rK1d...g5nQ", providerName: "FastInference", region: "North America", category: "quality", score: 90, metric: "4.7/5 rating", models: 9, uptime: 99.85 },
    { providerPk: "4wX7e...h8sR", providerName: "LatamAI", region: "South America", category: "reliability", score: 86, metric: "99.92% uptime", models: 3, uptime: 99.92 },
  ];
  return NextResponse.json(providers);
}

function handlePricing(searchParams: URLSearchParams) {
  const days = Number(searchParams.get("days") ?? 30);
  const trends: PricingTrend[] = [];
  const baseDate = new Date();
  baseDate.setDate(baseDate.getDate() - days);

  for (let i = 0; i < days; i++) {
    const date = new Date(baseDate);
    date.setDate(date.getDate() + i);
    const dayNoise = rand() * 20 - 10;
    const trendDown = -i * 0.5; // slight downward pricing trend
    trends.push({
      date: date.toISOString().split("T")[0],
      avgPricePer1MInputNanoerg: Math.max(10, 150 + trendDown + dayNoise),
      avgPricePer1MOutputNanoerg: Math.max(15, 200 + trendDown + dayNoise * 1.2),
      medianPricePer1MInputNanoerg: Math.max(8, 120 + trendDown + dayNoise * 0.8),
      medianPricePer1MOutputNanoerg: Math.max(12, 170 + trendDown + dayNoise),
      minPricePer1MInputNanoerg: 0,
      maxPricePer1MInputNanoerg: Math.max(50, 400 + trendDown + dayNoise * 2),
    });
  }

  return NextResponse.json({ days, trends });
}

function handleDemand() {
  const signals: DemandSignal[] = [
    { modelCategory: "Reasoning", requests24h: 41250, requests7d: 278000, growthPct: 342, avgLatencyMs: 420, supplyProviders: 9, demandSupplyRatio: 2.8, status: "undersupplied" },
    { modelCategory: "Code Generation", requests24h: 35800, requests7d: 241000, growthPct: 87, avgLatencyMs: 290, supplyProviders: 10, demandSupplyRatio: 1.5, status: "balanced" },
    { modelCategory: "General Chat", requests24h: 45200, requests7d: 305000, growthPct: 23, avgLatencyMs: 150, supplyProviders: 18, demandSupplyRatio: 0.8, status: "oversupplied" },
    { modelCategory: "Creative Writing", requests24h: 12500, requests7d: 84000, growthPct: 56, avgLatencyMs: 320, supplyProviders: 6, demandSupplyRatio: 1.2, status: "balanced" },
    { modelCategory: "Embeddings", requests24h: 8107, requests7d: 54500, growthPct: 12, avgLatencyMs: 45, supplyProviders: 8, demandSupplyRatio: 0.6, status: "oversupplied" },
    { modelCategory: "Vision/Multimodal", requests24h: 5200, requests7d: 35000, growthPct: 210, avgLatencyMs: 850, supplyProviders: 2, demandSupplyRatio: 3.5, status: "undersupplied" },
  ];
  return NextResponse.json(signals);
}

function handleGeography() {
  const geo: GeoDistribution[] = [
    { region: "North America", requests: 52300, providers: 15, avgLatencyMs: 120, marketSharePct: 36.6, changeFromLastWeek: 5.2 },
    { region: "Europe", requests: 38200, providers: 14, avgLatencyMs: 140, marketSharePct: 26.7, changeFromLastWeek: 3.1 },
    { region: "Asia", requests: 28500, providers: 8, avgLatencyMs: 180, marketSharePct: 20.0, changeFromLastWeek: 8.7 },
    { region: "South America", requests: 14200, providers: 5, avgLatencyMs: 250, marketSharePct: 9.9, changeFromLastWeek: 12.3 },
    { region: "Oceania", requests: 9657, providers: 5, avgLatencyMs: 200, marketSharePct: 6.8, changeFromLastWeek: -2.1 },
  ];
  return NextResponse.json(geo);
}

function handleRecommendations() {
  const recs: Recommendation[] = [
    { type: "consumer", category: "cost", title: "Switch to Llama 3.1 8B for simple tasks", description: "For basic chat and classification tasks, Llama 3.1 8B offers 3x cost savings with acceptable quality. 15 providers available.", impact: "high", model: "llama-3.1-8b" },
    { type: "consumer", category: "performance", title: "Use NodeAlpha for latency-sensitive workloads", description: "NodeAlpha maintains the lowest average latency (72ms) across all regions with 99.95% uptime.", impact: "high", provider: "NodeAlpha" },
    { type: "consumer", category: "quality", title: "DeepSeek R1 for reasoning tasks", description: "DeepSeek R1 Distill 32B shows 342% growth in reasoning tasks with strong benchmark scores.", impact: "medium", model: "deepseek-r1-distill-32b" },
    { type: "provider", category: "pricing", title: "Consider reducing prices by 10-15%", description: "Market-wide pricing has been trending down 5% weekly. Reducing prices can capture higher volume.", impact: "high" },
    { type: "provider", category: "capacity", title: "Add reasoning model support", description: "Reasoning models are undersupplied (2.8x demand/supply ratio). Adding DeepSeek or Phi-4 can fill this gap.", impact: "high" },
    { type: "provider", category: "reliability", title: "Improve uptime to exceed 99.9%", description: "Providers with 99.9%+ uptime receive 30% more requests. Current top providers average 99.9%.", impact: "medium" },
    { type: "provider", category: "region", title: "Consider deploying in South America", description: "South American demand grew 12.3% weekly but has only 5 providers. Low competition opportunity.", impact: "medium" },
    { type: "consumer", category: "cost", title: "Use ErgoCompute for bulk processing", description: "ErgoCompute offers the lowest cost per 1M tokens (15 ERG) with good reliability.", impact: "medium", provider: "ErgoCompute" },
  ];
  return NextResponse.json(recs);
}

function handleWeeklySummary() {
  const now = new Date();
  const weekAgo = new Date(now);
  weekAgo.setDate(weekAgo.getDate() - 7);

  const summary: WeeklySummary = {
    period: { start: weekAgo.toISOString().split("T")[0], end: now.toISOString().split("T")[0] },
    totalRequests: 892340,
    totalTokens: 367000000,
    totalCostNanoerg: 285000000000,
    avgLatencyMs: 328,
    successRate: 96.8,
    topModel: "llama-3.3-70b",
    topProvider: "NodeAlpha",
    highlights: [
      "Network uptime reached 99.7%, up from 99.2% last week",
      "Reasoning model demand surged 342% week-over-week",
      "3 new providers joined the network",
      "Average pricing decreased 5% across all model categories",
      "DeepSeek R1 Distill 32B became the fastest-growing model",
    ],
  };
  return NextResponse.json(summary);
}
