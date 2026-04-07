"use client";

import { useState, useEffect, useCallback } from "react";
import Link from "next/link";
import {
  fetchRegionalDistribution,
  type RegionData,
} from "@/lib/api/analytics";
import { DateRangePicker } from "@/components/analytics/DateRangePicker";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatNumber(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
}

const REGION_FLAGS: Record<string, string> = {
  "US-East": "🇺🇸",
  "US-West": "🇺🇸",
  "EU-West": "🇪🇺",
  "EU-North": "🇸🇪",
  "Asia-Pacific": "🌏",
};

const REGION_COLORS: Record<string, string> = {
  "US-East": "bg-blue-500",
  "US-West": "bg-cyan-500",
  "EU-West": "bg-emerald-500",
  "EU-North": "bg-violet-500",
  "Asia-Pacific": "bg-orange-500",
};

const REGION_CHART_COLORS: Record<string, string> = {
  "US-East": "#3b82f6",
  "US-West": "#06b6d4",
  "EU-West": "#10b981",
  "EU-North": "#8b5cf6",
  "Asia-Pacific": "#f97316",
};

// ---------------------------------------------------------------------------
// Horizontal bar chart
// ---------------------------------------------------------------------------

function HorizontalBarChart({ data }: { data: RegionData[] }) {
  const maxRequests = Math.max(...data.map((d) => d.requests), 1);
  const sorted = [...data].sort((a, b) => b.requests - a.requests);

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
      <h3 className="text-sm font-semibold text-surface-900 mb-4">Requests by Region</h3>
      <div className="space-y-3">
        {sorted.map((d) => {
          const pct = (d.requests / maxRequests) * 100;
          return (
            <div key={d.region} className="group">
              <div className="flex items-center justify-between mb-1">
                <span className="text-xs font-medium text-surface-800">
                  {REGION_FLAGS[d.region]} {d.region}
                </span>
                <span className="text-xs font-mono text-surface-800/60">
                  {formatNumber(d.requests)}
                </span>
              </div>
              <div className="w-full h-6 bg-surface-100 dark:bg-surface-800 rounded-md overflow-hidden">
                <div
                  className={`h-full rounded-md ${REGION_COLORS[d.region] || "bg-brand-500"} transition-all`}
                  style={{ width: `${pct}%` }}
                />
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Pie chart equivalent (conic-gradient)
// ---------------------------------------------------------------------------

function PieChart({ data }: { data: RegionData[] }) {
  const total = data.reduce((s, d) => s + d.marketShare, 0);
  let cumulative = 0;

  const gradientParts = data.map((d) => {
    const start = cumulative;
    cumulative += d.marketShare;
    return `${REGION_CHART_COLORS[d.region] || "#888"} ${start}% ${cumulative}%`;
  });

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
      <h3 className="text-sm font-semibold text-surface-900 mb-4">Market Share</h3>
      <div className="flex items-center gap-6">
        {/* Pie circle */}
        <div
          className="w-40 h-40 rounded-full flex-shrink-0"
          style={{
            background: `conic-gradient(${gradientParts.join(", ")})`,
          }}
        />
        {/* Legend */}
        <div className="space-y-2 flex-1">
          {data.map((d) => (
            <div key={d.region} className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <span
                  className="w-3 h-3 rounded-sm flex-shrink-0"
                  style={{ backgroundColor: REGION_CHART_COLORS[d.region] || "#888" }}
                />
                <span className="text-xs text-surface-800">
                  {REGION_FLAGS[d.region]} {d.region}
                </span>
              </div>
              <span className="text-xs font-mono font-medium text-surface-900">
                {d.marketShare}%
              </span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// World map placeholder
// ---------------------------------------------------------------------------

function WorldMapPlaceholder({ data }: { data: RegionData[] }) {
  const positions: Record<string, { x: number; y: number }> = {
    "US-East": { x: 25, y: 35 },
    "US-West": { x: 12, y: 38 },
    "EU-West": { x: 48, y: 30 },
    "EU-North": { x: 52, y: 22 },
    "Asia-Pacific": { x: 78, y: 42 },
  };

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
      <h3 className="text-sm font-semibold text-surface-900 mb-4">Regional Distribution</h3>
      <div className="relative w-full aspect-[2/1] bg-surface-50 dark:bg-surface-800/50 rounded-lg overflow-hidden">
        {/* Grid lines */}
        <div className="absolute inset-0 grid grid-cols-6 grid-rows-3">
          {Array.from({ length: 18 }).map((_, i) => (
            <div key={i} className="border border-surface-200/50 dark:border-surface-700/50" />
          ))}
        </div>

        {/* Region dots */}
        {data.map((d) => {
          const pos = positions[d.region];
          if (!pos) return null;
          const dotSize = Math.max(8, Math.min(24, d.marketShare * 1.5));
          return (
            <div
              key={d.region}
              className="absolute group"
              style={{ left: `${pos.x}%`, top: `${pos.y}%`, transform: "translate(-50%, -50%)" }}
            >
              <div
                className={`rounded-full ${REGION_COLORS[d.region]} opacity-60 animate-pulse`}
                style={{ width: dotSize * 2, height: dotSize * 2 }}
              />
              <div
                className={`rounded-full ${REGION_COLORS[d.region]} absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2`}
                style={{ width: dotSize, height: dotSize }}
              />
              <div className="absolute -top-8 left-1/2 -translate-x-1/2 hidden group-hover:block bg-surface-900 text-surface-0 text-[10px] px-2 py-1 rounded whitespace-nowrap z-10">
                {REGION_FLAGS[d.region]} {d.region}: {d.marketShare}%
              </div>
              <div className="absolute -bottom-5 left-1/2 -translate-x-1/2 text-[9px] text-surface-800/40 whitespace-nowrap">
                {d.region}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main page
// ---------------------------------------------------------------------------

export default function RegionsPage() {
  const [regions, setRegions] = useState<RegionData[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadData = useCallback(async () => {
    try {
      setError(null);
      const data = await fetchRegionalDistribution();
      setRegions(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load regional data");
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  if (isLoading) {
    return (
      <div className="max-w-6xl mx-auto px-4 py-8">
        <div className="skeleton-shimmer h-8 w-48 mb-6 rounded-lg" />
        <div className="skeleton-shimmer h-48 mb-6 rounded-xl" />
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
          <div className="skeleton-shimmer h-64 rounded-xl" />
          <div className="skeleton-shimmer h-64 rounded-xl" />
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="max-w-6xl mx-auto px-4 py-8">
        <div className="rounded-lg border border-red-200 bg-red-50 dark:border-red-800/40 dark:bg-red-950/20 px-4 py-3 text-sm text-red-600 dark:text-red-400">
          {error}
        </div>
      </div>
    );
  }

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="mb-6">
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-bold text-surface-900">Regional Distribution</h1>
            <p className="text-sm text-surface-800/50 mt-0.5">
              Geographic breakdown of providers, requests, and market share
            </p>
          </div>
          <DateRangePicker value={{ start: "", end: "" }} onChange={() => {}} />
        </div>
      </div>

      {/* Sub-nav */}
      <div className="flex gap-2 mb-6 text-sm">
        <Link href="/analytics" className="px-3 py-1.5 rounded-lg bg-surface-100 text-surface-800/60 hover:bg-surface-200 dark:bg-surface-800 dark:hover:bg-surface-700">
          Overview
        </Link>
        <Link href="/analytics/models" className="px-3 py-1.5 rounded-lg bg-surface-100 text-surface-800/60 hover:bg-surface-200 dark:bg-surface-800 dark:hover:bg-surface-700">
          Models
        </Link>
        <Link href="/analytics/providers" className="px-3 py-1.5 rounded-lg bg-surface-100 text-surface-800/60 hover:bg-surface-200 dark:bg-surface-800 dark:hover:bg-surface-700">
          Providers
        </Link>
        <Link href="/analytics/regions" className="px-3 py-1.5 rounded-lg bg-surface-900 text-white dark:bg-surface-100 dark:text-surface-900 font-medium">
          Regions
        </Link>
      </div>

      {/* World map placeholder */}
      <div className="mb-6">
        <WorldMapPlaceholder data={regions} />
      </div>

      {/* Regional cards */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4 mb-6">
        {regions.map((r) => (
          <div
            key={r.region}
            className="rounded-xl border border-surface-200 bg-surface-0 p-5 transition-all hover:shadow-md"
          >
            <div className="flex items-center justify-between mb-3">
              <div className="flex items-center gap-2">
                <span className="text-2xl">{REGION_FLAGS[r.region]}</span>
                <span className="text-base font-semibold text-surface-900">{r.region}</span>
              </div>
              <span className="text-xs font-mono font-medium text-surface-800/60 bg-surface-100 px-2 py-0.5 rounded-full">
                {r.marketShare}%
              </span>
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <div className="text-[10px] text-surface-800/40">Providers</div>
                <div className="text-lg font-bold text-surface-900">{r.providers}</div>
              </div>
              <div>
                <div className="text-[10px] text-surface-800/40">Requests</div>
                <div className="text-lg font-bold text-surface-900">{formatNumber(r.requests)}</div>
              </div>
              <div>
                <div className="text-[10px] text-surface-800/40">Tokens</div>
                <div className="text-sm font-bold text-surface-900">{formatNumber(r.tokens)}</div>
              </div>
              <div>
                <div className="text-[10px] text-surface-800/40">Avg Latency</div>
                <div className="text-sm font-bold text-surface-900">{r.averageLatencyMs}ms</div>
              </div>
            </div>
            {/* Mini bar for market share */}
            <div className="mt-3">
              <div className="w-full h-1.5 bg-surface-100 dark:bg-surface-800 rounded-full overflow-hidden">
                <div
                  className={`h-full rounded-full ${REGION_COLORS[r.region] || "bg-brand-500"}`}
                  style={{ width: `${r.marketShare}%` }}
                />
              </div>
            </div>
          </div>
        ))}
      </div>

      {/* Charts row */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <HorizontalBarChart data={regions} />
        <PieChart data={regions} />
      </div>
    </div>
  );
}
