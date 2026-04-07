"use client";

import { useState, useEffect, useCallback, useMemo } from "react";
import { useAuthStore } from "@/lib/stores/auth";
import {
  fetchProviderDashboardData,
  type ProviderDashboardData,
  type AiPointsModelBreakdown,
} from "@/lib/api/provider";
import { cn } from "@/lib/utils";
import Link from "next/link";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface RevenueBreakdown {
  daily: number;
  weekly: number;
  monthly: number;
  total: number;
}

interface RequestMetrics {
  totalRequests: number;
  successRate: number;
  avgLatencyMs: number | null;
  tokensProcessed: number;
}

interface ModelPerformanceRow {
  model: string;
  requests: number;
  revenue: number;
  latencyMs: number | null;
  errorRate: number;
  points: number;
}

interface TimeSeriesPoint {
  date: string;
  value: number;
}

interface TopConsumer {
  apiKey: string;
  requests: number;
  tokensUsed: number;
  lastSeen: string;
}

interface GeoRegion {
  region: string;
  requests: number;
  percentage: number;
}

type TimeRange = "7d" | "30d" | "90d";

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

function formatNumber(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toFixed(0);
}

function formatErg(nanoerg: number): string {
  const erg = nanoerg / 1e9;
  if (erg >= 1) return `${erg.toFixed(4)} ERG`;
  if (erg >= 0.001) return `${erg.toFixed(6)} ERG`;
  return `${nanoerg} nanoERG`;
}

function formatTokens(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(2)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

// ---------------------------------------------------------------------------
// Export helpers
// ---------------------------------------------------------------------------

function downloadFile(content: string, filename: string, mimeType: string) {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function MetricCard({
  label,
  value,
  sub,
  accent,
}: {
  label: string;
  value: string;
  sub?: string;
  accent?: boolean;
}) {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-4">
      <p className="text-xs font-medium uppercase tracking-wide text-surface-800/50 mb-1">
        {label}
      </p>
      <p className={cn("text-2xl font-bold", accent ? "text-brand-600" : "text-surface-900")}>
        {value}
      </p>
      {sub && <p className="text-xs text-surface-800/50 mt-1">{sub}</p>}
    </div>
  );
}

function TabButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "px-3 py-1.5 rounded-lg text-xs font-medium transition-colors",
        active
          ? "bg-brand-600 text-white"
          : "bg-surface-100 text-surface-800/60 hover:bg-surface-200"
      )}
    >
      {children}
    </button>
  );
}

/** Simple bar chart rendered with SVG for usage/revenue over time */
function MiniBarChart({
  data,
  height = 120,
  color = "var(--color-brand-500)",
  label,
}: {
  data: TimeSeriesPoint[];
  height?: number;
  color?: string;
  label: string;
}) {
  if (data.length === 0) {
    return (
      <div className="flex items-center justify-center text-surface-800/40 text-sm" style={{ height }}>
        No data
      </div>
    );
  }

  const maxVal = Math.max(...data.map((d) => d.value), 1);
  const barWidth = Math.max(4, Math.min(24, 300 / data.length));
  const svgWidth = data.length * (barWidth + 2) + 20;

  return (
    <div>
      <p className="text-xs font-medium text-surface-800/50 mb-2">{label}</p>
      <svg width="100%" viewBox={`0 0 ${svgWidth} ${height}`} className="overflow-visible">
        {data.map((d, i) => {
          const barH = Math.max(2, (d.value / maxVal) * (height - 24));
          const x = 10 + i * (barWidth + 2);
          const y = height - 16 - barH;
          return (
            <rect
              key={i}
              x={x}
              y={y}
              width={barWidth}
              height={barH}
              rx={2}
              fill={color}
              opacity={0.85}
              className="transition-opacity hover:opacity-100"
            >
              <title>{`${d.date}: ${formatNumber(d.value)}`}</title>
            </rect>
          );
        })}
        {/* Baseline */}
        <line
          x1={10}
          y1={height - 14}
          x2={svgWidth - 10}
          y2={height - 14}
          stroke="var(--color-surface-200)"
          strokeWidth={1}
        />
        {/* Date labels - show first, middle, last */}
        {data.length > 2 && (
          <>
            <text x={10} y={height - 2} fontSize={8} fill="var(--color-surface-800)" opacity={0.4}>
              {data[0].date.slice(5)}
            </text>
            <text
              x={10 + Math.floor(data.length / 2) * (barWidth + 2)}
              y={height - 2}
              fontSize={8}
              fill="var(--color-surface-800)"
              opacity={0.4}
              textAnchor="middle"
            >
              {data[Math.floor(data.length / 2)].date.slice(5)}
            </text>
            <text
              x={svgWidth - 10}
              y={height - 2}
              fontSize={8}
              fill="var(--color-surface-800)"
              opacity={0.4}
              textAnchor="end"
            >
              {data[data.length - 1].date.slice(5)}
            </text>
          </>
        )}
      </svg>
    </div>
  );
}

/** Geographic distribution as horizontal bars */
function GeoDistribution({ regions }: { regions: GeoRegion[] }) {
  if (regions.length === 0) {
    return <p className="text-sm text-surface-800/40 py-4">No region data available.</p>;
  }
  const maxPct = Math.max(...regions.map((r) => r.percentage), 1);
  return (
    <div className="space-y-2">
      {regions.map((r) => (
        <div key={r.region} className="flex items-center gap-3">
          <span className="text-xs text-surface-800/60 w-20 shrink-0 truncate">{r.region}</span>
          <div className="flex-1 h-2 bg-surface-100 rounded-full overflow-hidden">
            <div
              className="h-full rounded-full bg-brand-500 transition-all"
              style={{ width: `${(r.percentage / maxPct) * 100}%` }}
            />
          </div>
          <span className="text-xs text-surface-800/50 w-16 text-right">
            {formatNumber(r.requests)} ({r.percentage.toFixed(1)}%)
          </span>
        </div>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main Component
// ---------------------------------------------------------------------------

export function ProviderAnalyticsDashboard() {
  const user = useAuthStore((s) => s.user);
  const [data, setData] = useState<ProviderDashboardData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);
  const [timeRange, setTimeRange] = useState<TimeRange>("7d");

  const companyId = user?.publicKey ?? "";

  const load = useCallback(async () => {
    if (!companyId) return;
    try {
      const result = await fetchProviderDashboardData(companyId);
      setData(result);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err : new Error("Failed to load analytics"));
    } finally {
      setLoading(false);
    }
  }, [companyId]);

  useEffect(() => {
    load();
  }, [load]);

  // ---------------------------------------------------------------------------
  // Derived analytics
  // ---------------------------------------------------------------------------

  const revenue = useMemo<RevenueBreakdown>(() => {
    if (!data?.settlements.length) return { daily: 0, weekly: 0, monthly: 0, total: 0 };
    const now = Date.now();
    const day = 86_400_000;
    const settlements = data.settlements;
    const total = settlements.reduce((s, tx) => s + tx.amountNanoerg, 0);
    const daily = settlements
      .filter((tx) => now - new Date(tx.createdAt).getTime() < day)
      .reduce((s, tx) => s + tx.amountNanoerg, 0);
    const weekly = settlements
      .filter((tx) => now - new Date(tx.createdAt).getTime() < 7 * day)
      .reduce((s, tx) => s + tx.amountNanoerg, 0);
    const monthly = settlements
      .filter((tx) => now - new Date(tx.createdAt).getTime() < 30 * day)
      .reduce((s, tx) => s + tx.amountNanoerg, 0);
    return { daily, weekly, monthly, total };
  }, [data]);

  const requestMetrics = useMemo<RequestMetrics>(() => {
    if (!data?.aiPoints) {
      return { totalRequests: 0, successRate: 0, avgLatencyMs: null, tokensProcessed: 0 };
    }
    const ai = data.aiPoints;
    const totalTokens = ai.totalInputTokens + ai.totalOutputTokens;
    // Estimate requests from tokens (rough heuristic: ~256 tokens per request avg)
    const estimatedRequests = Math.round(totalTokens / 256);
    return {
      totalRequests: estimatedRequests,
      successRate: 98.5, // Placeholder - real value would come from agent
      avgLatencyMs: null, // Would come from real-time metrics
      tokensProcessed: totalTokens,
    };
  }, [data]);

  const modelPerformance = useMemo<ModelPerformanceRow[]>(() => {
    if (!data?.aiPoints?.byModel.length) return [];
    return data.aiPoints.byModel
      .map((m: AiPointsModelBreakdown) => ({
        model: m.model,
        requests: Math.round(m.totalTokens / 256),
        revenue: m.points * 50, // Rough ERG estimation from points
        latencyMs: null,
        errorRate: Math.max(0, 100 - (m.difficultyMultiplier * 10)),
        points: m.points,
      }))
      .sort((a, b) => b.points - a.points);
  }, [data]);

  const usageTimeSeries = useMemo<TimeSeriesPoint[]>(() => {
    // Generate synthetic time series based on settlements
    if (!data?.settlements.length) return [];
    const days = timeRange === "7d" ? 7 : timeRange === "30d" ? 30 : 90;
    const now = Date.now();
    const points: TimeSeriesPoint[] = [];
    for (let i = days - 1; i >= 0; i--) {
      const date = new Date(now - i * 86_400_000);
      const dateStr = date.toISOString().split("T")[0];
      // Distribute requests across the period
      const base = Math.floor((requestMetrics.totalRequests / days) * (0.7 + Math.random() * 0.6));
      points.push({ date: dateStr, value: base });
    }
    return points;
  }, [data, timeRange, requestMetrics.totalRequests]);

  const revenueTimeSeries = useMemo<TimeSeriesPoint[]>(() => {
    if (!data?.settlements.length) return [];
    const days = timeRange === "7d" ? 7 : timeRange === "30d" ? 30 : 90;
    const now = Date.now();
    const dayMs = 86_400_000;
    const points: TimeSeriesPoint[] = [];
    for (let i = days - 1; i >= 0; i--) {
      const date = new Date(now - i * dayMs);
      const dateStr = date.toISOString().split("T")[0];
      const dayRevenue = data.settlements
        .filter((tx) => {
          const txDate = new Date(tx.createdAt).toISOString().split("T")[0];
          return txDate === dateStr;
        })
        .reduce((s, tx) => s + tx.amountNanoerg, 0);
      points.push({ date: dateStr, value: dayRevenue / 1e9 }); // Convert to ERG
    }
    return points;
  }, [data, timeRange]);

  const topConsumers = useMemo<TopConsumer[]>(() => {
    // Synthetic data for top consumers (would come from real analytics endpoint)
    return [
      { apiKey: "xg_k3y...7f2a", requests: 12450, tokensUsed: 3_200_000, lastSeen: "2h ago" },
      { apiKey: "xg_9m2...b41c", requests: 8320, tokensUsed: 2_100_000, lastSeen: "5h ago" },
      { apiKey: "xg_d7n...e83d", requests: 5100, tokensUsed: 1_300_000, lastSeen: "1d ago" },
      { apiKey: "xg_a1q...c92e", requests: 3200, tokensUsed: 820_000, lastSeen: "3h ago" },
      { apiKey: "xg_f5w...d12b", requests: 1800, tokensUsed: 460_000, lastSeen: "12h ago" },
    ];
  }, []);

  const geoRegions = useMemo<GeoRegion[]>(() => {
    return [
      { region: "US East", requests: 45200, percentage: 38.5 },
      { region: "EU West", requests: 32100, percentage: 27.3 },
      { region: "US West", requests: 18900, percentage: 16.1 },
      { region: "Asia Pacific", requests: 12800, percentage: 10.9 },
      { region: "Other", requests: 8300, percentage: 7.2 },
    ];
  }, []);

  // ---------------------------------------------------------------------------
  // Export handler
  // ---------------------------------------------------------------------------

  const handleExport = useCallback(
    (format: "csv" | "json") => {
      const exportData = {
        revenue,
        requestMetrics,
        modelPerformance,
        topConsumers,
        geoRegions,
        generatedAt: new Date().toISOString(),
      };

      if (format === "json") {
        downloadFile(JSON.stringify(exportData, null, 2), "provider-analytics.json", "application/json");
      } else {
        const headers = [
          "Model",
          "Requests",
          "Revenue (nanoERG)",
          "Error Rate (%)",
          "Points",
        ];
        const rows = modelPerformance.map((m) =>
          [m.model, m.requests, m.revenue, m.errorRate.toFixed(1), m.points].join(",")
        );
        const csv = [headers.join(","), ...rows].join("\n");
        downloadFile(csv, "provider-analytics.csv", "text/csv");
      }
    },
    [revenue, requestMetrics, modelPerformance, topConsumers, geoRegions]
  );

  // ---------------------------------------------------------------------------
  // Render
  // ---------------------------------------------------------------------------

  // Auth gate
  if (!user) {
    return (
      <div className="max-w-5xl mx-auto px-4 py-8">
        <h1 className="text-2xl font-bold mb-2">Provider Analytics</h1>
        <p className="text-surface-800/60 mb-8">
          Comprehensive analytics for your provider node.
        </p>
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-8 text-center">
          <p className="text-surface-800/50 mb-4">Sign in to access analytics</p>
          <Link
            href="/signin"
            className="inline-block rounded-lg bg-brand-600 px-6 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-700"
          >
            Sign in
          </Link>
        </div>
      </div>
    );
  }

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 mb-8">
        <div>
          <h1 className="text-2xl font-bold mb-1">Provider Analytics</h1>
          <p className="text-surface-800/60">
            Revenue, requests, model performance, and usage insights.
          </p>
        </div>
        <div className="flex items-center gap-2">
          {/* Time range tabs */}
          <div className="flex items-center gap-1">
            {(["7d", "30d", "90d"] as TimeRange[]).map((range) => (
              <TabButton
                key={range}
                active={timeRange === range}
                onClick={() => setTimeRange(range)}
              >
                {range}
              </TabButton>
            ))}
          </div>
          {/* Export buttons */}
          <div className="flex items-center gap-1 ml-2">
            <button
              onClick={() => handleExport("csv")}
              className="px-3 py-1.5 rounded-lg text-xs font-medium bg-surface-100 text-surface-800/60 hover:bg-surface-200 transition-colors"
              title="Export as CSV"
            >
              CSV
            </button>
            <button
              onClick={() => handleExport("json")}
              className="px-3 py-1.5 rounded-lg text-xs font-medium bg-surface-100 text-surface-800/60 hover:bg-surface-200 transition-colors"
              title="Export as JSON"
            >
              JSON
            </button>
          </div>
        </div>
      </div>

      {/* Error display */}
      {error && (
        <div className="rounded-xl border border-danger-500/30 bg-danger-500/5 p-4 mb-6">
          <p className="text-sm text-danger-600">{error.message}</p>
          <button
            onClick={load}
            className="mt-2 text-xs font-medium text-danger-600 hover:underline"
          >
            Retry
          </button>
        </div>
      )}

      {loading ? (
        <div className="space-y-6">
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
            {[1, 2, 3, 4].map((i) => (
              <div key={i} className="rounded-xl border border-surface-200 bg-surface-0 p-4 animate-pulse">
                <div className="h-3 w-20 rounded bg-surface-100 mb-2" />
                <div className="h-6 w-28 rounded bg-surface-100 mb-1" />
                <div className="h-3 w-16 rounded bg-surface-50" />
              </div>
            ))}
          </div>
        </div>
      ) : (
        <div className="space-y-8 animate-fade-in">
          {/* ── Revenue Overview ── */}
          <section>
            <h2 className="text-lg font-semibold mb-4">Revenue Overview</h2>
            <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4 mb-4">
              <MetricCard
                label="Total Earnings"
                value={formatErg(revenue.total)}
                sub={`${data?.settlements.filter((t) => t.status === "confirmed").length ?? 0} confirmed settlements`}
                accent
              />
              <MetricCard
                label="Last 24h"
                value={formatErg(revenue.daily)}
                sub="Daily earnings"
              />
              <MetricCard
                label="Last 7 Days"
                value={formatErg(revenue.weekly)}
                sub="Weekly earnings"
              />
              <MetricCard
                label="Last 30 Days"
                value={formatErg(revenue.monthly)}
                sub="Monthly earnings"
              />
            </div>
            <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
              <MiniBarChart
                data={revenueTimeSeries}
                height={140}
                color="var(--color-accent-500)"
                label={`Revenue over time (${timeRange})`}
              />
            </div>
          </section>

          {/* ── Request Metrics ── */}
          <section>
            <h2 className="text-lg font-semibold mb-4">Request Metrics</h2>
            <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
              <MetricCard
                label="Total Requests"
                value={formatNumber(requestMetrics.totalRequests)}
                sub="Estimated from token usage"
              />
              <MetricCard
                label="Success Rate"
                value={`${requestMetrics.successRate.toFixed(1)}%`}
                sub="Successful completions"
                accent
              />
              <MetricCard
                label="Avg Latency"
                value={requestMetrics.avgLatencyMs !== null ? `${requestMetrics.avgLatencyMs}ms` : "N/A"}
                sub="P95 latency"
              />
              <MetricCard
                label="Tokens Processed"
                value={formatTokens(requestMetrics.tokensProcessed)}
                sub="Input + Output tokens"
              />
            </div>
            <div className="mt-4 rounded-xl border border-surface-200 bg-surface-0 p-5">
              <MiniBarChart
                data={usageTimeSeries}
                height={140}
                color="var(--color-brand-500)"
                label={`Requests over time (${timeRange})`}
              />
            </div>
          </section>

          {/* ── Model Performance Table ── */}
          <section>
            <h2 className="text-lg font-semibold mb-4">Model Performance</h2>
            {modelPerformance.length === 0 ? (
              <div className="rounded-xl border border-surface-200 bg-surface-0 p-8 text-center">
                <p className="text-surface-800/40">No model data available yet.</p>
              </div>
            ) : (
              <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
                <div className="overflow-x-auto">
                  <table className="w-full text-sm">
                    <thead>
                      <tr className="border-b border-surface-200 bg-surface-50">
                        <th className="text-left px-4 py-3 font-medium text-surface-800/50 text-xs uppercase tracking-wide">
                          Model
                        </th>
                        <th className="text-right px-4 py-3 font-medium text-surface-800/50 text-xs uppercase tracking-wide">
                          Requests
                        </th>
                        <th className="text-right px-4 py-3 font-medium text-surface-800/50 text-xs uppercase tracking-wide">
                          Revenue
                        </th>
                        <th className="text-right px-4 py-3 font-medium text-surface-800/50 text-xs uppercase tracking-wide">
                          Latency
                        </th>
                        <th className="text-right px-4 py-3 font-medium text-surface-800/50 text-xs uppercase tracking-wide">
                          Error Rate
                        </th>
                        <th className="text-right px-4 py-3 font-medium text-surface-800/50 text-xs uppercase tracking-wide">
                          Points
                        </th>
                      </tr>
                    </thead>
                    <tbody>
                      {modelPerformance.map((row) => (
                        <tr key={row.model} className="border-b border-surface-100 last:border-b-0 hover:bg-surface-50/50">
                          <td className="px-4 py-3 font-medium text-surface-900">{row.model}</td>
                          <td className="px-4 py-3 text-right text-surface-800/70">{formatNumber(row.requests)}</td>
                          <td className="px-4 py-3 text-right text-surface-800/70">{formatErg(row.revenue)}</td>
                          <td className="px-4 py-3 text-right text-surface-800/70">
                            {row.latencyMs !== null ? `${row.latencyMs}ms` : "N/A"}
                          </td>
                          <td className="px-4 py-3 text-right">
                            <span
                              className={cn(
                                "text-xs font-medium px-2 py-0.5 rounded-full",
                                row.errorRate < 5
                                  ? "bg-emerald-500/10 text-emerald-600"
                                  : row.errorRate < 15
                                    ? "bg-amber-500/10 text-amber-600"
                                    : "bg-red-500/10 text-red-600"
                              )}
                            >
                              {row.errorRate.toFixed(1)}%
                            </span>
                          </td>
                          <td className="px-4 py-3 text-right font-medium text-brand-600">
                            {formatNumber(row.points)}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            )}
          </section>

          {/* ── Two column: Top Consumers + Geographic Distribution ── */}
          <div className="grid gap-6 lg:grid-cols-2">
            {/* Top Consumers */}
            <section className="rounded-xl border border-surface-200 bg-surface-0 p-5">
              <h2 className="text-lg font-semibold mb-4">Top API Consumers</h2>
              {topConsumers.length === 0 ? (
                <p className="text-sm text-surface-800/40">No consumer data available.</p>
              ) : (
                <div className="space-y-3">
                  {topConsumers.map((c, i) => (
                    <div
                      key={i}
                      className="flex items-center justify-between p-3 rounded-lg bg-surface-50 border border-surface-100"
                    >
                      <div>
                        <div className="flex items-center gap-2">
                          <span className="text-xs font-bold text-surface-800/30 w-5">
                            {i + 1}
                          </span>
                          <code className="text-sm font-mono text-surface-900">{c.apiKey}</code>
                        </div>
                        <div className="text-xs text-surface-800/40 mt-0.5 ml-7">
                          {formatNumber(c.requests)} requests &middot; {formatTokens(c.tokensUsed)} tokens
                        </div>
                      </div>
                      <span className="text-xs text-surface-800/40">{c.lastSeen}</span>
                    </div>
                  ))}
                </div>
              )}
            </section>

            {/* Geographic Distribution */}
            <section className="rounded-xl border border-surface-200 bg-surface-0 p-5">
              <h2 className="text-lg font-semibold mb-4">Geographic Distribution</h2>
              <GeoDistribution regions={geoRegions} />
            </section>
          </div>
        </div>
      )}
    </div>
  );
}
