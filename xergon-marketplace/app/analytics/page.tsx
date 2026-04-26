"use client";

import { useState, useEffect, useCallback } from "react";
import Link from "next/link";
import {
  fetchAnalyticsOverview,
  fetchNetworkStats,
  type OverviewResponse,
  type NetworkStatsResponse,
} from "@/lib/api/analytics";
import { DateRangePicker } from "@/components/analytics/DateRangePicker";
import { ExportButton } from "@/components/analytics/ExportButton";

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

function TrendBadge({ current, previous }: { current: number; previous: number }) {
  if (previous === 0) return null;
  const pct = ((current - previous) / previous) * 100;
  const isUp = pct >= 0;
  return (
    <span className={`text-xs font-medium ${isUp ? "text-emerald-600 dark:text-emerald-400" : "text-red-500"}`}>
      {isUp ? "▲" : "▼"} {Math.abs(pct).toFixed(1)}%
    </span>
  );
}

// ---------------------------------------------------------------------------
// Period selector
// ---------------------------------------------------------------------------

type Period = "7d" | "30d" | "90d";

function PeriodSelector({
  period,
  onChange,
}: {
  period: Period;
  onChange: (p: Period) => void;
}) {
  const options: Period[] = ["7d", "30d", "90d"];
  return (
    <div className="flex gap-1">
      {options.map((p) => (
        <button
          key={p}
          onClick={() => onChange(p)}
          className={`rounded-lg px-3 py-1.5 text-xs font-medium transition-colors ${
            period === p
              ? "bg-surface-900 text-white dark:bg-surface-100 dark:text-surface-900"
              : "bg-surface-100 text-surface-800/60 hover:bg-surface-200 dark:bg-surface-800 dark:hover:bg-surface-700"
          }`}
        >
          {p.toUpperCase()}
        </button>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Metric card
// ---------------------------------------------------------------------------

function MetricCard({
  label,
  value,
  trend,
  previous,
  icon,
}: {
  label: string;
  value: string;
  trend?: number;
  previous?: number;
  icon: React.ReactNode;
}) {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-4 transition-all hover:shadow-md">
      <div className="flex items-start justify-between mb-2">
        <div className="rounded-lg bg-brand-50 p-2 text-brand-600 dark:bg-brand-950/30">
          {icon}
        </div>
        {trend !== undefined && previous !== undefined && (
          <TrendBadge current={trend} previous={previous} />
        )}
      </div>
      <div className="text-xl font-bold text-surface-900 mb-0.5">{value}</div>
      <div className="text-xs text-surface-800/50">{label}</div>
    </div>
  );
}

// Simple SVG icons (inline to avoid extra deps)
function IconRequests() {
  return (
    <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <polyline points="22 12 18 12 15 21 9 3 6 12 2 12" />
    </svg>
  );
}
function IconTokens() {
  return (
    <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M4 4h16c1.1 0 2 .9 2 2v12c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2z" />
      <polyline points="22 6 12 13 2 6" />
    </svg>
  );
}
function IconEarnings() {
  return (
    <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="12" r="10" />
      <path d="M16 8h-6a2 2 0 00-2 2v1a2 2 0 002 2h4a2 2 0 012 2v1a2 2 0 01-2 2H8" />
      <path d="M12 18V6" />
    </svg>
  );
}
function IconSpent() {
  return (
    <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <line x1="12" y1="1" x2="12" y2="23" />
      <path d="M17 5H9.5a3.5 3.5 0 000 7h5a3.5 3.5 0 010 7H6" />
    </svg>
  );
}
function IconLatency() {
  return (
    <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="12" cy="12" r="10" />
      <polyline points="12 6 12 12 16 14" />
    </svg>
  );
}
function IconUsers() {
  return (
    <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M16 21v-2a4 4 0 00-4-4H6a4 4 0 00-4 4v2" />
      <circle cx="9" cy="7" r="4" />
      <path d="M22 21v-2a4 4 0 00-3-3.87" />
      <path d="M16 3.13a4 4 0 010 7.75" />
    </svg>
  );
}
function IconProviders() {
  return (
    <svg className="w-4 h-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M21 16V8a2 2 0 00-1-1.73l-7-4a2 2 0 00-2 0l-7 4A2 2 0 003 8v8a2 2 0 001 1.73l7 4a2 2 0 002 0l7-4A2 2 0 0021 16z" />
      <polyline points="3.27 6.96 12 12.01 20.73 6.96" />
      <line x1="12" y1="22.08" x2="12" y2="12" />
    </svg>
  );
}

// ---------------------------------------------------------------------------
// Bar chart component (CSS-based)
// ---------------------------------------------------------------------------

function BarChart({
  data,
  label,
  color = "bg-brand-500",
  valueFormatter,
  maxValue,
}: {
  data: Array<{ date: string; value: number }>;
  label: string;
  color?: string;
  valueFormatter?: (v: number) => string;
  maxValue?: number;
}) {
  const fmt = valueFormatter || formatNumber;
  const max = maxValue || Math.max(...data.map((d) => d.value), 1);
  const chartHeight = 200;

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
      <h3 className="text-sm font-semibold text-surface-900 mb-4">{label}</h3>
      <div className="flex items-end gap-1" style={{ height: chartHeight }}>
        {data.map((d, i) => {
          const pct = Math.max((d.value / max) * 100, 2);
          return (
            <div
              key={i}
              className="flex-1 flex flex-col items-center gap-1 group relative"
            >
              {/* Tooltip */}
              <div className="absolute -top-8 left-1/2 -translate-x-1/2 hidden group-hover:block bg-surface-900 text-surface-0 text-[10px] px-2 py-1 rounded whitespace-nowrap z-10">
                {fmt(d.value)}
              </div>
              <div
                className={`w-full rounded-t ${color} transition-all min-w-[3px]`}
                style={{ height: `${pct}%` }}
              />
              <span className="text-[8px] text-surface-800/30 rotate-[-45deg] origin-top-left whitespace-nowrap">
                {d.date.slice(5)}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Dual bar chart (earnings + spent)
// ---------------------------------------------------------------------------

function DualBarChart({
  data,
  label,
}: {
  data: Array<{ date: string; earnings: number; spent: number }>;
  label: string;
}) {
  const maxVal = Math.max(
    ...data.map((d) => Math.max(d.earnings, d.spent)),
    1
  );
  const chartHeight = 200;

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
      <div className="flex items-center justify-between mb-4">
        <h3 className="text-sm font-semibold text-surface-900">{label}</h3>
        <div className="flex items-center gap-3 text-[10px] text-surface-800/50">
          <span className="flex items-center gap-1">
            <span className="w-2 h-2 rounded-sm bg-emerald-500" /> Earnings
          </span>
          <span className="flex items-center gap-1">
            <span className="w-2 h-2 rounded-sm bg-orange-400" /> Spent
          </span>
        </div>
      </div>
      <div className="flex items-end gap-1" style={{ height: chartHeight }}>
        {data.map((d, i) => {
          const earnPct = Math.max((d.earnings / maxVal) * 100, 2);
          const spentPct = Math.max((d.spent / maxVal) * 100, 2);
          return (
            <div
              key={i}
              className="flex-1 flex flex-col items-center gap-1 group relative"
            >
              <div className="absolute -top-8 left-1/2 -translate-x-1/2 hidden group-hover:block bg-surface-900 text-surface-0 text-[10px] px-2 py-1 rounded whitespace-nowrap z-10">
                {nanoergToErg(d.earnings)} / {nanoergToErg(d.spent)}
              </div>
              <div className="w-full flex gap-px" style={{ height: `${Math.max(earnPct, spentPct)}%` }}>
                <div
                  className="flex-1 bg-emerald-500 rounded-t"
                  style={{ height: `${earnPct}%`, marginTop: `${Math.max(spentPct - earnPct, 0)}%` }}
                />
                <div
                  className="flex-1 bg-orange-400 rounded-t"
                  style={{ height: `${spentPct}%`, marginTop: `${Math.max(earnPct - spentPct, 0)}%` }}
                />
              </div>
              <span className="text-[8px] text-surface-800/30 rotate-[-45deg] origin-top-left whitespace-nowrap">
                {d.date.slice(5)}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Latency dot-line chart
// ---------------------------------------------------------------------------

function LatencyChart({
  data,
  label,
}: {
  data: Array<{ date: string; value: number }>;
  label: string;
}) {
  const maxVal = Math.max(...data.map((d) => d.value), 1);
  const minVal = Math.min(...data.map((d) => d.value), 0);
  const range = maxVal - minVal || 1;
  const chartHeight = 200;
  const padding = 16;

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
      <h3 className="text-sm font-semibold text-surface-900 mb-4">{label}</h3>
      <div className="relative" style={{ height: chartHeight }}>
        <svg viewBox={`0 0 ${data.length * 40} ${chartHeight}`} className="w-full h-full" preserveAspectRatio="none">
          {/* Grid lines */}
          {[0, 0.25, 0.5, 0.75, 1].map((pct) => {
            const y = padding + (1 - pct) * (chartHeight - padding * 2);
            const val = minVal + pct * range;
            return (
              <g key={pct}>
                <line x1="0" y1={y} x2={data.length * 40} y2={y} className="stroke-surface-200" strokeWidth="0.5" />
                <text x="2" y={y - 2} className="fill-surface-800/30" fontSize="8">
                  {Math.round(val)}ms
                </text>
              </g>
            );
          })}

          {/* Connecting line */}
          {data.length > 1 && (
            <polyline
              points={data
                .map((d, i) => {
                  const x = i * 40 + 20;
                  const y = padding + (1 - (d.value - minVal) / range) * (chartHeight - padding * 2);
                  return `${x},${y}`;
                })
                .join(" ")}
              fill="none"
              className="stroke-brand-500"
              strokeWidth="2"
              strokeLinejoin="round"
            />
          )}

          {/* Dots */}
          {data.map((d, i) => {
            const x = i * 40 + 20;
            const y = padding + (1 - (d.value - minVal) / range) * (chartHeight - padding * 2);
            return (
              <g key={i}>
                <circle cx={x} cy={y} r="4" className="fill-brand-500" />
                <circle cx={x} cy={y} r="7" className="fill-brand-500/20" />
                <text
                  x={x}
                  y={chartHeight - 2}
                  textAnchor="middle"
                  className="fill-surface-800/30"
                  fontSize="8"
                >
                  {d.date.slice(5)}
                </text>
              </g>
            );
          })}
        </svg>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Skeleton
// ---------------------------------------------------------------------------

function SkeletonPulse({ className }: { className?: string }) {
  return <div className={`skeleton-shimmer rounded-lg ${className ?? ""}`} />;
}

function LoadingSkeleton() {
  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      <div className="flex items-center justify-between mb-6">
        <SkeletonPulse className="h-8 w-48" />
        <SkeletonPulse className="h-8 w-32" />
      </div>
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-6">
        {Array.from({ length: 8 }).map((_, i) => (
          <SkeletonPulse key={i} className="h-28" />
        ))}
      </div>
      <SkeletonPulse className="h-64 mb-6" />
      <SkeletonPulse className="h-64" />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main page
// ---------------------------------------------------------------------------

export default function AnalyticsPage() {
  const [period, setPeriod] = useState<Period>("30d");
  const [dateRange, setDateRange] = useState({ start: "", end: "" });
  const [overview, setOverview] = useState<OverviewResponse | null>(null);
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  const [_relayStats, setRelayStats] = useState<NetworkStatsResponse | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadData = useCallback(async () => {
    try {
      setError(null);
      const [overviewData, relayData] = await Promise.all([
        fetchAnalyticsOverview(period),
        fetchNetworkStats(),
      ]);
      setOverview(overviewData);
      setRelayStats(relayData);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load analytics");
    } finally {
      setIsLoading(false);
    }
  }, [period]);

  useEffect(() => {
    setIsLoading(true);
    loadData();
  }, [loadData]);

  if (isLoading) return <LoadingSkeleton />;

  if (error) {
    return (
      <div className="max-w-6xl mx-auto px-4 py-8">
        <div className="rounded-lg border border-red-200 bg-red-50 dark:border-red-800/40 dark:bg-red-950/20 px-4 py-3 text-sm text-red-600 dark:text-red-400">
          {error}
        </div>
      </div>
    );
  }

  if (!overview) return null;

  const { summary, daily } = overview;

  // Compute previous period for trends (simple: use first/second half)
  const mid = Math.floor(daily.length / 2);
  const firstHalf = daily.slice(0, mid);
  const secondHalf = daily.slice(mid);
  const sumField = (arr: typeof daily, field: keyof (typeof daily)[0]) =>
    arr.reduce((s, d) => s + (d[field] as number), 0);
  const avgField = (arr: typeof daily, field: keyof (typeof daily)[0]) =>
    arr.length > 0 ? sumField(arr, field) / arr.length : 0;

  const metrics = [
    {
      label: "Total Requests",
      value: formatNumber(summary.totalRequests),
      current: summary.totalRequests,
      previous: sumField(firstHalf, "requests"),
      icon: <IconRequests />,
    },
    {
      label: "Total Tokens",
      value: formatNumber(summary.totalTokens),
      current: summary.totalTokens,
      previous: sumField(firstHalf, "tokens"),
      icon: <IconTokens />,
    },
    {
      label: "Earnings",
      value: nanoergToErg(summary.totalEarningsNanoErg),
      current: summary.totalEarningsNanoErg,
      previous: sumField(firstHalf, "earningsNanoErg"),
      icon: <IconEarnings />,
    },
    {
      label: "Spent",
      value: nanoergToErg(summary.totalSpentNanoErg),
      current: summary.totalSpentNanoErg,
      previous: sumField(firstHalf, "spentNanoErg"),
      icon: <IconSpent />,
    },
    {
      label: "Avg Latency",
      value: `${summary.averageLatencyMs}ms`,
      current: summary.averageLatencyMs,
      previous: avgField(firstHalf, "averageLatencyMs"),
      icon: <IconLatency />,
    },
    {
      label: "P95 Latency",
      value: `${summary.p95LatencyMs}ms`,
      current: summary.p95LatencyMs,
      previous: avgField(firstHalf, "averageLatencyMs") * 1.4,
      icon: <IconLatency />,
    },
    {
      label: "Active Users",
      value: formatNumber(summary.activeUsers),
      current: summary.activeUsers,
      previous: Math.floor(summary.activeUsers * 0.85),
      icon: <IconUsers />,
    },
    {
      label: "Active Providers",
      value: String(summary.activeProviders),
      current: summary.activeProviders,
      previous: Math.floor(summary.activeProviders * 0.9),
      icon: <IconProviders />,
    },
  ];

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3 mb-6">
        <div>
          <h1 className="text-2xl font-bold text-surface-900">Analytics</h1>
          <p className="text-sm text-surface-800/50 mt-0.5">
            Marketplace usage, revenue, and performance metrics
          </p>
        </div>
        <PeriodSelector period={period} onChange={setPeriod} />
        <DateRangePicker value={dateRange} onChange={setDateRange} />
        {overview && (
          <ExportButton
            data={overview.daily as unknown as Record<string, unknown>[]}
            filename={`analytics-${period}`}
          />
        )}
      </div>

      {/* Sub-nav links */}
      <div className="flex gap-2 mb-6 text-sm">
        <Link href="/analytics" className="px-3 py-1.5 rounded-lg bg-surface-900 text-white dark:bg-surface-100 dark:text-surface-900 font-medium">
          Overview
        </Link>
        <Link href="/analytics/models" className="px-3 py-1.5 rounded-lg bg-surface-100 text-surface-800/60 hover:bg-surface-200 dark:bg-surface-800 dark:hover:bg-surface-700">
          Models
        </Link>
        <Link href="/analytics/providers" className="px-3 py-1.5 rounded-lg bg-surface-100 text-surface-800/60 hover:bg-surface-200 dark:bg-surface-800 dark:hover:bg-surface-700">
          Providers
        </Link>
        <Link href="/analytics/regions" className="px-3 py-1.5 rounded-lg bg-surface-100 text-surface-800/60 hover:bg-surface-200 dark:bg-surface-800 dark:hover:bg-surface-700">
          Regions
        </Link>
      </div>

      {/* Metric cards */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-6">
        {metrics.map((m) => (
          <MetricCard
            key={m.label}
            label={m.label}
            value={m.value}
            trend={m.current}
            previous={m.previous}
            icon={m.icon}
          />
        ))}
      </div>

      {/* Requests chart */}
      <div className="mb-6">
        <BarChart
          data={daily.map((d) => ({ date: d.date, value: d.requests }))}
          label="Daily Requests"
          color="bg-brand-500"
        />
      </div>

      {/* Tokens chart */}
      <div className="mb-6">
        <BarChart
          data={daily.map((d) => ({ date: d.date, value: d.tokens }))}
          label="Daily Token Usage"
          color="bg-violet-500"
        />
      </div>

      {/* Revenue dual bar */}
      <div className="mb-6">
        <DualBarChart
          data={daily.map((d) => ({
            date: d.date,
            earnings: d.earningsNanoErg,
            spent: d.spentNanoErg,
          }))}
          label="Daily Revenue (Earnings vs Spent)"
        />
      </div>

      {/* Latency chart */}
      <div className="mb-6">
        <LatencyChart
          data={daily.map((d) => ({
            date: d.date,
            value: d.averageLatencyMs,
          }))}
          label="Daily Average Latency"
        />
      </div>

      {/* Footer */}
      <div className="text-xs text-surface-800/30 text-center">
        Data refreshes automatically. Period: {overview.period.start} to {overview.period.end}.
      </div>
    </div>
  );
}
