"use client";

import { useState, useEffect, useCallback } from "react";
import { cn } from "@/lib/utils";

// ---------------------------------------------------------------------------
// Types (matching API response shapes)
// ---------------------------------------------------------------------------

interface MarketOverview {
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

interface TrendingModel {
  modelId: string;
  modelName: string;
  category: string;
  requestGrowthPct: number;
  totalRequests: number;
  avgLatencyMs: number;
  providerCount: number;
  trend: "up" | "down" | "stable";
}

interface TopProvider {
  providerPk: string;
  providerName: string;
  region: string;
  category: "latency" | "cost" | "quality" | "reliability";
  score: number;
  metric: string;
  models: number;
  uptime: number;
}

interface DemandSignal {
  modelCategory: string;
  requests24h: number;
  requests7d: number;
  growthPct: number;
  avgLatencyMs: number;
  supplyProviders: number;
  demandSupplyRatio: number;
  status: "undersupplied" | "balanced" | "oversupplied";
}

interface Recommendation {
  type: "consumer" | "provider";
  category: string;
  title: string;
  description: string;
  impact: "high" | "medium" | "low";
  model?: string;
  provider?: string;
}

interface WeeklySummary {
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
// Helpers
// ---------------------------------------------------------------------------

function formatNumber(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
}

function nanoergToErg(nanoerg: number): string {
  if (nanoerg <= 0) return "0";
  const erg = nanoerg / 1e9;
  if (erg >= 1_000) return `${(erg / 1_000).toFixed(1)}K ERG`;
  return `${erg.toFixed(2)} ERG`;
}

// ---------------------------------------------------------------------------
// Skeleton
// ---------------------------------------------------------------------------

function SkeletonPulse({ className }: { className?: string }) {
  return <div className={cn("skeleton-shimmer rounded-lg", className)} />;
}

function LoadingSkeleton() {
  return (
    <div className="max-w-7xl mx-auto px-4 py-8 space-y-6">
      <SkeletonPulse className="h-8 w-56" />
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        {Array.from({ length: 4 }).map((_, i) => (
          <SkeletonPulse key={i} className="h-28" />
        ))}
      </div>
      <SkeletonPulse className="h-64" />
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        {Array.from({ length: 4 }).map((_, i) => (
          <SkeletonPulse key={i} className="h-48" />
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Overview card
// ---------------------------------------------------------------------------

function OverviewCard({
  label,
  value,
  change,
  icon,
  color = "brand",
}: {
  label: string;
  value: string;
  change?: number;
  icon: React.ReactNode;
  color?: "brand" | "emerald" | "amber" | "violet";
}) {
  const colorMap = {
    brand: "bg-brand-50 text-brand-600 dark:bg-brand-950/30",
    emerald: "bg-emerald-50 text-emerald-600 dark:bg-emerald-950/30",
    amber: "bg-amber-50 text-amber-600 dark:bg-amber-950/30",
    violet: "bg-violet-50 text-violet-600 dark:bg-violet-950/30",
  };

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-4 transition-all hover:shadow-md">
      <div className="flex items-center justify-between mb-3">
        <div className={cn("rounded-lg p-2", colorMap[color])}>{icon}</div>
        {change !== undefined && (
          <span className={cn("text-xs font-semibold", change >= 0 ? "text-emerald-600 dark:text-emerald-400" : "text-red-500")}>
            {change >= 0 ? "▲" : "▼"} {Math.abs(change).toFixed(1)}%
          </span>
        )}
      </div>
      <div className="text-xl font-bold text-surface-900">{value}</div>
      <div className="text-xs text-surface-800/50 mt-0.5">{label}</div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Trending model card
// ---------------------------------------------------------------------------

function TrendingCard({ model }: { model: TrendingModel }) {
  return (
    <div className="flex items-center gap-3 rounded-lg border border-surface-200 bg-surface-0 p-3 hover:shadow-sm transition-shadow">
      <div className="flex-shrink-0 flex items-center justify-center w-8 h-8 rounded-lg bg-brand-50 dark:bg-brand-950/30">
        {model.trend === "up" ? (
          <svg className="w-4 h-4 text-emerald-600 dark:text-emerald-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="23 6 13.5 15.5 8.5 10.5 1 18" />
            <polyline points="17 6 23 6 23 12" />
          </svg>
        ) : model.trend === "down" ? (
          <svg className="w-4 h-4 text-red-500" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <polyline points="23 18 13.5 8.5 8.5 13.5 1 6" />
            <polyline points="17 18 23 18 23 12" />
          </svg>
        ) : (
          <svg className="w-4 h-4 text-surface-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <line x1="5" y1="12" x2="19" y2="12" />
          </svg>
        )}
      </div>
      <div className="flex-1 min-w-0">
        <div className="text-sm font-semibold text-surface-900 truncate">{model.modelName}</div>
        <div className="text-[11px] text-surface-800/50">{model.category} &middot; {model.providerCount} providers</div>
      </div>
      <div className="text-right flex-shrink-0">
        <div className={cn("text-sm font-bold", model.requestGrowthPct > 0 ? "text-emerald-600 dark:text-emerald-400" : model.requestGrowthPct < 0 ? "text-red-500" : "text-surface-600")}>
          {model.requestGrowthPct > 0 ? "+" : ""}{model.requestGrowthPct}%
        </div>
        <div className="text-[10px] text-surface-800/40">{formatNumber(model.totalRequests)} reqs</div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Leaderboard mini-card
// ---------------------------------------------------------------------------

function LeaderboardCard({
  title,
  providers,
  icon,
}: {
  title: string;
  providers: TopProvider[];
  icon: React.ReactNode;
}) {
  const medals = ["text-yellow-500", "text-surface-400", "text-amber-700"];
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-4">
      <div className="flex items-center gap-2 mb-3">
        <div className="rounded-lg bg-brand-50 p-1.5 text-brand-600 dark:bg-brand-950/30">{icon}</div>
        <h3 className="text-sm font-semibold text-surface-900">{title}</h3>
      </div>
      <div className="space-y-2">
        {providers.slice(0, 3).map((p, i) => (
          <div key={p.providerPk} className="flex items-center gap-2 text-xs">
            <span className={cn("font-bold w-4 text-center", medals[i])}>{i + 1}</span>
            <div className="flex-1 min-w-0">
              <div className="font-semibold text-surface-800 truncate">{p.providerName}</div>
              <div className="text-[10px] text-surface-800/40">{p.region}</div>
            </div>
            <div className="text-right">
              <div className="font-semibold text-surface-700">{p.metric}</div>
              <div className="text-[10px] text-surface-800/40">{p.score} score</div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Demand signal bar
// ---------------------------------------------------------------------------

function DemandBar({ signal, maxRequests }: { signal: DemandSignal; maxRequests: number }) {
  const pct = (signal.requests24h / maxRequests) * 100;
  const statusColors = {
    undersupplied: "bg-red-400",
    balanced: "bg-brand-500",
    oversupplied: "bg-emerald-400",
  };

  return (
    <div className="flex items-center gap-3">
      <div className="w-28 text-xs font-medium text-surface-700 text-right flex-shrink-0">{signal.modelCategory}</div>
      <div className="flex-1 h-6 rounded-full bg-surface-100 dark:bg-surface-800 relative overflow-hidden">
        <div
          className={cn("h-full rounded-full transition-all", statusColors[signal.status])}
          style={{ width: `${pct}%` }}
        />
        <div className="absolute inset-0 flex items-center px-2 text-[10px] font-medium text-surface-700">
          {formatNumber(signal.requests24h)} reqs/24h
        </div>
      </div>
      <div className="w-16 text-right flex-shrink-0">
        <span className={cn(
          "inline-block rounded-full px-1.5 py-0.5 text-[9px] font-semibold",
          signal.status === "undersupplied" ? "bg-red-50 text-red-600 dark:bg-red-950/30 dark:text-red-400" :
          signal.status === "balanced" ? "bg-brand-50 text-brand-600 dark:bg-brand-950/30 dark:text-brand-400" :
          "bg-emerald-50 text-emerald-600 dark:bg-emerald-950/30 dark:text-emerald-400"
        )}>
          {signal.demandSupplyRatio}x
        </span>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Capacity utilization
// ---------------------------------------------------------------------------

function CapacityBar({ utilization }: { utilization: number }) {
  const color = utilization > 90 ? "bg-red-500" : utilization > 70 ? "bg-amber-500" : "bg-emerald-500";
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-4">
      <div className="flex items-center justify-between mb-2">
        <h3 className="text-sm font-semibold text-surface-900">Network Capacity Utilization</h3>
        <span className="text-sm font-bold text-surface-700">{utilization.toFixed(1)}%</span>
      </div>
      <div className="h-3 rounded-full bg-surface-100 dark:bg-surface-800 overflow-hidden">
        <div className={cn("h-full rounded-full transition-all", color)} style={{ width: `${utilization}%` }} />
      </div>
      <div className="flex justify-between mt-1.5 text-[10px] text-surface-800/40">
        <span>0%</span>
        <span className={utilization > 80 ? "text-red-500 font-medium" : ""}>
          {utilization > 90 ? "Critical" : utilization > 70 ? "High" : "Healthy"}
        </span>
        <span>100%</span>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Recommendation card
// ---------------------------------------------------------------------------

function RecommendationCard({
  title,
  recommendations,
  type,
}: {
  title: string;
  recommendations: Recommendation[];
  type: "consumer" | "provider";
}) {
  const impactColors = {
    high: "bg-red-50 text-red-600 dark:bg-red-950/30 dark:text-red-400",
    medium: "bg-amber-50 text-amber-600 dark:bg-amber-950/30 dark:text-amber-400",
    low: "bg-surface-100 text-surface-600 dark:bg-surface-800 dark:text-surface-400",
  };

  const icon = type === "consumer" ? (
    <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2" />
      <circle cx="12" cy="7" r="4" />
    </svg>
  ) : (
    <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M21 16V8a2 2 0 00-1-1.73l-7-4a2 2 0 00-2 0l-7 4A2 2 0 003 8v8a2 2 0 001 1.73l7 4a2 2 0 002 0l7-4A2 2 0 0021 16z" />
    </svg>
  );

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
      <div className="flex items-center gap-2 mb-4">
        <div className="rounded-lg bg-violet-50 p-1.5 text-violet-600 dark:bg-violet-950/30">{icon}</div>
        <h3 className="text-sm font-semibold text-surface-900">{title}</h3>
      </div>
      <div className="space-y-3">
        {recommendations.map((rec, i) => (
          <div key={i} className="rounded-lg border border-surface-100 p-3">
            <div className="flex items-start gap-2">
              <span className={cn("inline-flex items-center rounded-full px-1.5 py-0.5 text-[9px] font-bold mt-0.5", impactColors[rec.impact])}>
                {rec.impact.toUpperCase()}
              </span>
              <div>
                <div className="text-xs font-semibold text-surface-800">{rec.title}</div>
                <div className="text-[11px] text-surface-800/60 mt-0.5 leading-relaxed">{rec.description}</div>
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Weekly summary card
// ---------------------------------------------------------------------------

function WeeklySummaryCard({ summary }: { summary: WeeklySummary }) {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
      <h3 className="text-sm font-semibold text-surface-900 mb-4">Weekly Summary</h3>
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-4">
        <div>
          <div className="text-lg font-bold text-surface-900">{formatNumber(summary.totalRequests)}</div>
          <div className="text-[10px] text-surface-800/50">Total Requests</div>
        </div>
        <div>
          <div className="text-lg font-bold text-surface-900">{formatNumber(summary.totalTokens)}</div>
          <div className="text-[10px] text-surface-800/50">Total Tokens</div>
        </div>
        <div>
          <div className="text-lg font-bold text-surface-900">{nanoergToErg(summary.totalCostNanoerg)}</div>
          <div className="text-[10px] text-surface-800/50">Total Cost</div>
        </div>
        <div>
          <div className="text-lg font-bold text-surface-900">{summary.successRate}%</div>
          <div className="text-[10px] text-surface-800/50">Success Rate</div>
        </div>
      </div>
      <div className="border-t border-surface-100 pt-3">
        <div className="text-[11px] text-surface-800/60 space-y-1">
          <div>Top Model: <span className="font-semibold text-surface-800">{summary.topModel}</span></div>
          <div>Top Provider: <span className="font-semibold text-surface-800">{summary.topProvider}</span></div>
        </div>
        <div className="mt-3 space-y-1">
          {summary.highlights.map((h, i) => (
            <div key={i} className="flex items-start gap-1.5 text-[11px] text-surface-800/60">
              <span className="text-brand-500 mt-0.5">•</span>
              <span>{h}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export default function ProviderInsights() {
  const [overview, setOverview] = useState<MarketOverview | null>(null);
  const [trending, setTrending] = useState<TrendingModel[]>([]);
  const [topProviders, setTopProviders] = useState<TopProvider[]>([]);
  const [demand, setDemand] = useState<DemandSignal[]>([]);
  const [recommendations, setRecommendations] = useState<Recommendation[]>([]);
  const [weeklySummary, setWeeklySummary] = useState<WeeklySummary | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadData = useCallback(async () => {
    try {
      setError(null);
      const [overviewRes, trendingRes, providersRes, demandRes, recsRes, weeklyRes] = await Promise.all([
        fetch("/api/insights?endpoint=overview"),
        fetch("/api/insights?endpoint=trending"),
        fetch("/api/insights?endpoint=top-providers"),
        fetch("/api/insights?endpoint=demand"),
        fetch("/api/insights?endpoint=recommendations"),
        fetch("/api/insights?endpoint=weekly-summary"),
      ]);

      if (!overviewRes.ok || !trendingRes.ok || !providersRes.ok || !demandRes.ok || !recsRes.ok || !weeklyRes.ok) {
        throw new Error("Failed to fetch insights");
      }

      const [o, t, p, d, r, w] = await Promise.all([
        overviewRes.json(),
        trendingRes.json(),
        providersRes.json(),
        demandRes.json(),
        recsRes.json(),
        weeklyRes.json(),
      ]);

      setOverview(o);
      setTrending(t);
      setTopProviders(p);
      setDemand(d);
      setRecommendations(r);
      setWeeklySummary(w);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load insights");
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  // Derived data
  const latencyLeaders = topProviders.filter((p) => p.category === "latency");
  const costLeaders = topProviders.filter((p) => p.category === "cost");
  const qualityLeaders = topProviders.filter((p) => p.category === "quality");
  const reliabilityLeaders = topProviders.filter((p) => p.category === "reliability");
  const maxDemand = Math.max(...demand.map((d) => d.requests24h), 1);
  const consumerRecs = recommendations.filter((r) => r.type === "consumer");
  const providerRecs = recommendations.filter((r) => r.type === "provider");

  const healthScore = overview
    ? overview.networkHealth === "healthy"
      ? 95
      : overview.networkHealth === "degraded"
        ? 65
        : 30
    : 0;

  const capacityUtilization = overview ? ((overview.activeProviders / overview.totalProviders) * 100) : 0;

  // Icons
  const IconProviders = (
    <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M21 16V8a2 2 0 00-1-1.73l-7-4a2 2 0 00-2 0l-7 4A2 2 0 003 8v8a2 2 0 001 1.73l7 4a2 2 0 002 0l7-4A2 2 0 0021 16z" />
    </svg>
  );
  const IconModels = (
    <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2z" />
      <polyline points="22 6 12 13 2 6" />
    </svg>
  );
  const IconRequests = (
    <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <polyline points="22 12 18 12 15 21 9 3 6 12 2 12" />
    </svg>
  );
  const IconHealth = (
    <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M22 11.08V12a10 10 0 11-5.93-9.14" />
      <polyline points="22 4 12 14.01 9 11.01" />
    </svg>
  );
  const IconLatency = (
    <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="12" r="10" />
      <polyline points="12 6 12 12 16 14" />
    </svg>
  );
  const IconCost = (
    <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="12" r="10" />
      <path d="M16 8h-6a2 2 0 000 7h4a2 2 0 010 7H8" />
      <path d="M12 18V6" />
    </svg>
  );
  const IconQuality = (
    <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
    </svg>
  );
  const IconReliability = (
    <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
    </svg>
  );

  if (isLoading) return <LoadingSkeleton />;

  if (error) {
    return (
      <div className="max-w-7xl mx-auto px-4 py-8">
        <div className="rounded-lg border border-red-200 bg-red-50 dark:border-red-800/40 dark:bg-red-950/20 px-4 py-3 text-sm text-red-600 dark:text-red-400">
          {error}
        </div>
      </div>
    );
  }

  return (
    <div className="max-w-7xl mx-auto px-4 py-8 space-y-6">
      {/* Header */}
      <div>
        <h1 className="text-2xl font-bold text-surface-900">Provider Insights</h1>
        <p className="text-sm text-surface-800/50 mt-1">Market intelligence and provider analytics</p>
      </div>

      {/* Market overview cards */}
      {overview && (
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          <OverviewCard
            label="Total Providers"
            value={String(overview.totalProviders)}
            change={overview.weeklyChange.providers}
            icon={IconProviders}
            color="brand"
          />
          <OverviewCard
            label="Total Models"
            value={String(overview.totalModels)}
            change={overview.weeklyChange.models}
            icon={IconModels}
            color="emerald"
          />
          <OverviewCard
            label="Requests (24h)"
            value={formatNumber(overview.totalRequests24h)}
            change={overview.weeklyChange.requests}
            icon={IconRequests}
            color="amber"
          />
          <OverviewCard
            label="Network Health"
            value={`${healthScore}/100`}
            icon={IconHealth}
            color={overview.networkHealth === "healthy" ? "emerald" : overview.networkHealth === "degraded" ? "amber" : "violet"}
          />
        </div>
      )}

      {/* Trending models + Capacity utilization */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <div className="lg:col-span-2 rounded-xl border border-surface-200 bg-surface-0 p-5">
          <div className="flex items-center justify-between mb-4">
            <h2 className="text-sm font-semibold text-surface-900">Trending Models</h2>
            <span className="text-[10px] text-surface-800/40">Weekly growth</span>
          </div>
          <div className="space-y-2">
            {trending.slice(0, 5).map((model) => (
              <TrendingCard key={model.modelId} model={model} />
            ))}
          </div>
        </div>
        <div className="space-y-4">
          <CapacityBar utilization={capacityUtilization} />
          {overview && (
            <div className="rounded-xl border border-surface-200 bg-surface-0 p-4">
              <h3 className="text-sm font-semibold text-surface-900 mb-3">Network Stats</h3>
              <div className="space-y-2 text-xs">
                <div className="flex justify-between">
                  <span className="text-surface-800/50">Avg Latency</span>
                  <span className="font-semibold text-surface-700">{overview.avgLatencyMs}ms</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-surface-800/50">Uptime (24h)</span>
                  <span className="font-semibold text-emerald-600 dark:text-emerald-400">{overview.uptime24h}%</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-surface-800/50">Volume (24h)</span>
                  <span className="font-semibold text-surface-700">{nanoergToErg(overview.totalVolumeNanoerg)}</span>
                </div>
                <div className="flex justify-between">
                  <span className="text-surface-800/50">Tokens (24h)</span>
                  <span className="font-semibold text-surface-700">{formatNumber(overview.totalTokens24h)}</span>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Top providers leaderboards */}
      <div>
        <h2 className="text-sm font-semibold text-surface-900 mb-4">Top Providers by Category</h2>
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
          <LeaderboardCard title="Latency Leader" providers={latencyLeaders} icon={IconLatency} />
          <LeaderboardCard title="Cost Leader" providers={costLeaders} icon={IconCost} />
          <LeaderboardCard title="Quality Leader" providers={qualityLeaders} icon={IconQuality} />
          <LeaderboardCard title="Reliability Leader" providers={reliabilityLeaders} icon={IconReliability} />
        </div>
      </div>

      {/* Demand signals */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-sm font-semibold text-surface-900">Demand Signals by Task Type</h2>
          <div className="flex items-center gap-3 text-[10px] text-surface-800/40">
            <span className="flex items-center gap-1"><span className="w-2 h-2 rounded-full bg-red-400" /> Undersupplied</span>
            <span className="flex items-center gap-1"><span className="w-2 h-2 rounded-full bg-brand-500" /> Balanced</span>
            <span className="flex items-center gap-1"><span className="w-2 h-2 rounded-full bg-emerald-400" /> Oversupplied</span>
          </div>
        </div>
        <div className="space-y-3">
          {demand.map((signal) => (
            <DemandBar key={signal.modelCategory} signal={signal} maxRequests={maxDemand} />
          ))}
        </div>
      </div>

      {/* Recommendations */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <RecommendationCard title="For Consumers" recommendations={consumerRecs} type="consumer" />
        <RecommendationCard title="For Providers" recommendations={providerRecs} type="provider" />
      </div>

      {/* Weekly summary */}
      {weeklySummary && <WeeklySummaryCard summary={weeklySummary} />}
    </div>
  );
}
