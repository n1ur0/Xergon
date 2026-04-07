"use client";

import { useState, useEffect, useCallback } from "react";
import Link from "next/link";
import {
  fetchModelAnalytics,
  type ModelAnalytics,
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

function nanoergToErg(nanoerg: number): string {
  if (nanoerg <= 0) return "0";
  const erg = nanoerg / 1e9;
  if (erg >= 1_000) return `${(erg / 1_000).toFixed(1)}K ERG`;
  return `${erg.toFixed(2)} ERG`;
}

type SortKey = "modelId" | "requests" | "tokens" | "earningsNanoErg" | "averageLatencyMs" | "p95LatencyMs" | "errorRate" | "totalUsers";

function SortableHeader({
  label,
  sortKey,
  currentSort,
  direction,
  onSort,
}: {
  label: string;
  sortKey: SortKey;
  currentSort: SortKey;
  direction: "asc" | "desc";
  onSort: (key: SortKey) => void;
}) {
  const isActive = currentSort === sortKey;
  return (
    <button
      onClick={() => onSort(sortKey)}
      className={`text-left text-xs font-medium px-3 py-2 whitespace-nowrap transition-colors ${
        isActive
          ? "text-brand-600 dark:text-brand-400"
          : "text-surface-800/50 hover:text-surface-800/80"
      }`}
    >
      {label}
      {isActive && (
        <span className="ml-1">{direction === "asc" ? "▲" : "▼"}</span>
      )}
    </button>
  );
}

function getCellColor(value: number, thresholds: [number, number], invert = false): string {
  const [good, bad] = invert ? [thresholds[0], thresholds[1]] : [thresholds[1], thresholds[0]];
  if (invert) {
    // Lower is better (latency, error rate)
    if (value <= good) return "text-emerald-600 dark:text-emerald-400";
    if (value >= bad) return "text-red-500";
    return "text-amber-500";
  }
  // Higher is better (requests, tokens)
  if (value >= good) return "text-emerald-600 dark:text-emerald-400";
  if (value <= bad) return "text-red-500";
  return "text-amber-500";
}

// ---------------------------------------------------------------------------
// Sparkline (CSS dots)
// ---------------------------------------------------------------------------

function Sparkline({ data, color = "bg-brand-500" }: { data: number[]; color?: string }) {
  const max = Math.max(...data, 1);
  return (
    <div className="flex items-end gap-px h-5">
      {data.map((v, i) => (
        <div
          key={i}
          className={`w-1 rounded-t ${color}`}
          style={{ height: `${Math.max((v / max) * 100, 10)}%` }}
        />
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main page
// ---------------------------------------------------------------------------

export default function ModelAnalyticsPage() {
  const [models, setModels] = useState<ModelAnalytics[]>([]);
  const [selectedModel, setSelectedModel] = useState<string | null>(null);
  const [sortKey, setSortKey] = useState<SortKey>("requests");
  const [sortDir, setSortDir] = useState<"asc" | "desc">("desc");
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadData = useCallback(async () => {
    try {
      setError(null);
      const data = await fetchModelAnalytics();
      setModels(data);
      if (data.length > 0 && !selectedModel) {
        setSelectedModel(data[0].modelId);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load model analytics");
    } finally {
      setIsLoading(false);
    }
  }, [selectedModel]);

  useEffect(() => {
    loadData();
  }, [loadData]);

  const handleSort = useCallback(
    (key: SortKey) => {
      if (sortKey === key) {
        setSortDir((d) => (d === "asc" ? "desc" : "asc"));
      } else {
        setSortKey(key);
        setSortDir("desc");
      }
    },
    [sortKey]
  );

  const sortedModels = [...models].sort((a, b) => {
    const aVal = a[sortKey];
    const bVal = b[sortKey];
    if (typeof aVal === "string" && typeof bVal === "string") {
      return sortDir === "asc" ? aVal.localeCompare(bVal) : bVal.localeCompare(aVal);
    }
    return sortDir === "asc"
      ? (aVal as number) - (bVal as number)
      : (bVal as number) - (aVal as number);
  });

  const activeModel = models.find((m) => m.modelId === selectedModel);

  if (isLoading) {
    return (
      <div className="max-w-6xl mx-auto px-4 py-8">
        <div className="skeleton-shimmer h-8 w-48 mb-6 rounded-lg" />
        <div className="skeleton-shimmer h-64 mb-6 rounded-xl" />
        <div className="skeleton-shimmer h-96 rounded-xl" />
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
            <h1 className="text-2xl font-bold text-surface-900">Model Analytics</h1>
            <p className="text-sm text-surface-800/50 mt-0.5">
              Usage, performance, and revenue breakdown by model
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
        <Link href="/analytics/models" className="px-3 py-1.5 rounded-lg bg-surface-900 text-white dark:bg-surface-100 dark:text-surface-900 font-medium">
          Models
        </Link>
        <Link href="/analytics/providers" className="px-3 py-1.5 rounded-lg bg-surface-100 text-surface-800/60 hover:bg-surface-200 dark:bg-surface-800 dark:hover:bg-surface-700">
          Providers
        </Link>
        <Link href="/analytics/regions" className="px-3 py-1.5 rounded-lg bg-surface-100 text-surface-800/60 hover:bg-surface-200 dark:bg-surface-800 dark:hover:bg-surface-700">
          Regions
        </Link>
      </div>

      {/* Model selector tabs */}
      <div className="flex gap-2 mb-6 overflow-x-auto pb-2">
        {models.map((m) => (
          <button
            key={m.modelId}
            onClick={() => setSelectedModel(m.modelId)}
            className={`px-3 py-1.5 rounded-lg text-xs font-medium whitespace-nowrap transition-colors ${
              selectedModel === m.modelId
                ? "bg-brand-600 text-white"
                : "bg-surface-100 text-surface-800/60 hover:bg-surface-200 dark:bg-surface-800 dark:hover:bg-surface-700"
            }`}
          >
            {m.modelId}
          </button>
        ))}
      </div>

      {/* Selected model detail */}
      {activeModel && (
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 mb-6">
          <h2 className="text-lg font-semibold text-surface-900 mb-4">
            {activeModel.modelId}
          </h2>
          <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-7 gap-4 mb-6">
            {[
              { label: "Requests", value: formatNumber(activeModel.requests) },
              { label: "Tokens", value: formatNumber(activeModel.tokens) },
              { label: "Earnings", value: nanoergToErg(activeModel.earningsNanoErg) },
              { label: "Avg Latency", value: `${activeModel.averageLatencyMs}ms` },
              { label: "P95 Latency", value: `${activeModel.p95LatencyMs}ms` },
              { label: "Error Rate", value: `${activeModel.errorRate}%` },
              { label: "Users", value: String(activeModel.totalUsers) },
            ].map((item) => (
              <div key={item.label}>
                <div className="text-xs text-surface-800/40">{item.label}</div>
                <div className="text-lg font-bold text-surface-900">{item.value}</div>
              </div>
            ))}
          </div>

          {/* Daily usage chart */}
          <h3 className="text-sm font-semibold text-surface-900 mb-3">Daily Usage Trend</h3>
          <div className="flex items-end gap-1 h-32">
            {activeModel.dailyUsage.map((d, i) => {
              const maxReq = Math.max(...activeModel.dailyUsage.map((x) => x.requests), 1);
              const pct = (d.requests / maxReq) * 100;
              return (
                <div key={i} className="flex-1 group relative">
                  <div className="absolute -top-6 left-1/2 -translate-x-1/2 hidden group-hover:block bg-surface-900 text-surface-0 text-[9px] px-1.5 py-0.5 rounded whitespace-nowrap z-10">
                    {formatNumber(d.requests)} req
                  </div>
                  <div
                    className="w-full rounded-t bg-brand-500 min-h-[2px]"
                    style={{ height: `${Math.max(pct, 3)}%` }}
                  />
                </div>
              );
            })}
          </div>

          {/* Top providers table */}
          <h3 className="text-sm font-semibold text-surface-900 mt-6 mb-3">Top Providers</h3>
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-surface-200">
                  <th className="text-left text-xs text-surface-800/50 px-3 py-2 font-medium">Provider</th>
                  <th className="text-left text-xs text-surface-800/50 px-3 py-2 font-medium">Region</th>
                  <th className="text-right text-xs text-surface-800/50 px-3 py-2 font-medium">Requests</th>
                </tr>
              </thead>
              <tbody>
                {activeModel.topProviders.map((p) => (
                  <tr key={p.providerPk} className="border-b border-surface-100">
                    <td className="px-3 py-2 font-mono text-xs text-surface-900">
                      {p.providerPk.slice(0, 8)}...
                    </td>
                    <td className="px-3 py-2 text-xs text-surface-800/60">{p.region}</td>
                    <td className="px-3 py-2 text-xs text-right text-surface-900">
                      {formatNumber(p.requests)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Model comparison table */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
        <div className="px-5 py-4 border-b border-surface-100">
          <h2 className="text-base font-semibold text-surface-900">Model Comparison</h2>
          <p className="text-xs text-surface-800/40 mt-0.5">Click column headers to sort</p>
        </div>
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-surface-200 bg-surface-50 dark:bg-surface-800/50">
                <SortableHeader label="Model" sortKey="modelId" currentSort={sortKey} direction={sortDir} onSort={handleSort} />
                <SortableHeader label="Requests" sortKey="requests" currentSort={sortKey} direction={sortDir} onSort={handleSort} />
                <SortableHeader label="Tokens" sortKey="tokens" currentSort={sortKey} direction={sortDir} onSort={handleSort} />
                <SortableHeader label="Earnings" sortKey="earningsNanoErg" currentSort={sortKey} direction={sortDir} onSort={handleSort} />
                <SortableHeader label="Avg Latency" sortKey="averageLatencyMs" currentSort={sortKey} direction={sortDir} onSort={handleSort} />
                <SortableHeader label="P95" sortKey="p95LatencyMs" currentSort={sortKey} direction={sortDir} onSort={handleSort} />
                <SortableHeader label="Error Rate" sortKey="errorRate" currentSort={sortKey} direction={sortDir} onSort={handleSort} />
                <SortableHeader label="Users" sortKey="totalUsers" currentSort={sortKey} direction={sortDir} onSort={handleSort} />
                <th className="text-xs text-surface-800/50 px-3 py-2 font-medium">Trend</th>
              </tr>
            </thead>
            <tbody>
              {sortedModels.map((m) => (
                <tr
                  key={m.modelId}
                  className="border-b border-surface-100 hover:bg-surface-50 dark:hover:bg-surface-800/30 cursor-pointer transition-colors"
                  onClick={() => setSelectedModel(m.modelId)}
                >
                  <td className="px-3 py-2.5 font-medium text-surface-900 text-xs">{m.modelId}</td>
                  <td className={`px-3 py-2.5 text-xs text-right font-mono ${getCellColor(m.requests, [sortedModels[Math.floor(sortedModels.length / 2)]?.requests || 0, sortedModels[0]?.requests || 0])}`}>
                    {formatNumber(m.requests)}
                  </td>
                  <td className="px-3 py-2.5 text-xs text-right font-mono text-surface-800">
                    {formatNumber(m.tokens)}
                  </td>
                  <td className="px-3 py-2.5 text-xs text-right font-mono text-surface-800">
                    {nanoergToErg(m.earningsNanoErg)}
                  </td>
                  <td className={`px-3 py-2.5 text-xs text-right font-mono ${getCellColor(m.averageLatencyMs, [200, 400], true)}`}>
                    {m.averageLatencyMs}ms
                  </td>
                  <td className={`px-3 py-2.5 text-xs text-right font-mono ${getCellColor(m.p95LatencyMs, [300, 600], true)}`}>
                    {m.p95LatencyMs}ms
                  </td>
                  <td className={`px-3 py-2.5 text-xs text-right font-mono ${getCellColor(m.errorRate, [1, 3], true)}`}>
                    {m.errorRate}%
                  </td>
                  <td className="px-3 py-2.5 text-xs text-right font-mono text-surface-800">
                    {m.totalUsers}
                  </td>
                  <td className="px-3 py-2.5">
                    <Sparkline data={m.dailyUsage.map((d) => d.requests)} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}
