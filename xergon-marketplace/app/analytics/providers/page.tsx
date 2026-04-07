"use client";

import { useState, useEffect, useCallback } from "react";
import Link from "next/link";
import {
  fetchProviderComparison,
  type ProviderData,
  type ProvidersResponse,
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

function truncatePk(pk: string): string {
  if (pk.length <= 16) return pk;
  return `${pk.slice(0, 8)}...${pk.slice(-4)}`;
}

type SortKey = "latency" | "throughput" | "reliability" | "cost" | "reputation";

const REGIONS = ["All", "US-East", "US-West", "EU-West", "EU-North", "Asia-Pacific"];

function getCellColor(value: number, goodThreshold: number, badThreshold: number, lowerIsBetter = false): string {
  if (lowerIsBetter) {
    if (value <= goodThreshold) return "text-emerald-600 dark:text-emerald-400";
    if (value >= badThreshold) return "text-red-500";
    return "text-amber-500";
  }
  if (value >= goodThreshold) return "text-emerald-600 dark:text-emerald-400";
  if (value <= badThreshold) return "text-red-500";
  return "text-amber-500";
}

function RankBadge({ rank }: { rank: number }) {
  if (rank === 1) return <span className="inline-flex items-center justify-center w-6 h-6 rounded-full bg-yellow-100 dark:bg-yellow-900/30 text-yellow-700 dark:text-yellow-400 text-xs font-bold">1</span>;
  if (rank === 2) return <span className="inline-flex items-center justify-center w-6 h-6 rounded-full bg-gray-100 dark:bg-gray-700 text-gray-600 dark:text-gray-300 text-xs font-bold">2</span>;
  if (rank === 3) return <span className="inline-flex items-center justify-center w-6 h-6 rounded-full bg-orange-100 dark:bg-orange-900/30 text-orange-700 dark:text-orange-400 text-xs font-bold">3</span>;
  return <span className="inline-flex items-center justify-center w-6 h-6 rounded-full bg-surface-100 text-surface-800/50 text-xs font-medium">{rank}</span>;
}

// ---------------------------------------------------------------------------
// Sparkline (CSS dots)
// ---------------------------------------------------------------------------

function Sparkline({ values, color }: { values: number[]; color: string }) {
  if (values.length === 0) return null;
  const max = Math.max(...values, 1);
  return (
    <div className="flex items-center gap-px">
      {values.map((v, i) => (
        <div
          key={i}
          className={`w-1.5 h-1.5 rounded-full ${color}`}
          style={{ opacity: 0.3 + (v / max) * 0.7 }}
        />
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Sort button for table
// ---------------------------------------------------------------------------

function SortButton({
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
      {isActive && <span className="ml-1">{direction === "asc" ? "▲" : "▼"}</span>}
    </button>
  );
}

// ---------------------------------------------------------------------------
// Main page
// ---------------------------------------------------------------------------

export default function ProviderComparisonPage() {
  const [data, setData] = useState<ProvidersResponse | null>(null);
  const [sortKey, setSortKey] = useState<SortKey>("reputation");
  const [sortDir, setSortDir] = useState<"asc" | "desc">("desc");
  const [regionFilter, setRegionFilter] = useState<string>("All");
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadData = useCallback(async () => {
    try {
      setError(null);
      const res = await fetchProviderComparison(
        sortKey,
        regionFilter === "All" ? undefined : regionFilter
      );
      setData(res);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load provider data");
    } finally {
      setIsLoading(false);
    }
  }, [sortKey, regionFilter]);

  useEffect(() => {
    setIsLoading(true);
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

  if (isLoading) {
    return (
      <div className="max-w-6xl mx-auto px-4 py-8">
        <div className="skeleton-shimmer h-8 w-48 mb-6 rounded-lg" />
        <div className="skeleton-shimmer h-10 w-64 mb-6 rounded-lg" />
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 mb-6">
          {Array.from({ length: 6 }).map((_, i) => (
            <div key={i} className="skeleton-shimmer h-48 rounded-xl" />
          ))}
        </div>
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

  if (!data) return null;

  const { providers } = data;

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="mb-6">
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-bold text-surface-900">Provider Comparison</h1>
            <p className="text-sm text-surface-800/50 mt-0.5">
              Compare providers by latency, throughput, reliability, cost, and reputation
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
        <Link href="/analytics/providers" className="px-3 py-1.5 rounded-lg bg-surface-900 text-white dark:bg-surface-100 dark:text-surface-900 font-medium">
          Providers
        </Link>
        <Link href="/analytics/regions" className="px-3 py-1.5 rounded-lg bg-surface-100 text-surface-800/60 hover:bg-surface-200 dark:bg-surface-800 dark:hover:bg-surface-700">
          Regions
        </Link>
      </div>

      {/* Filter bar */}
      <div className="flex flex-wrap gap-3 mb-6">
        <div className="flex items-center gap-2">
          <span className="text-xs text-surface-800/50 font-medium">Region:</span>
          <div className="flex gap-1">
            {REGIONS.map((r) => (
              <button
                key={r}
                onClick={() => setRegionFilter(r)}
                className={`px-2.5 py-1 rounded-md text-xs font-medium transition-colors ${
                  regionFilter === r
                    ? "bg-surface-900 text-white dark:bg-surface-100 dark:text-surface-900"
                    : "bg-surface-100 text-surface-800/60 hover:bg-surface-200 dark:bg-surface-800 dark:hover:bg-surface-700"
                }`}
              >
                {r}
              </button>
            ))}
          </div>
        </div>
        <div className="flex items-center gap-2">
          <span className="text-xs text-surface-800/50 font-medium">Sort:</span>
          <div className="flex gap-1">
            {(["reputation", "latency", "throughput", "reliability", "cost"] as SortKey[]).map(
              (s) => (
                <button
                  key={s}
                  onClick={() => handleSort(s)}
                  className={`px-2.5 py-1 rounded-md text-xs font-medium transition-colors capitalize ${
                    sortKey === s
                      ? "bg-brand-600 text-white"
                      : "bg-surface-100 text-surface-800/60 hover:bg-surface-200 dark:bg-surface-800 dark:hover:bg-surface-700"
                  }`}
                >
                  {s}
                </button>
              )
            )}
          </div>
        </div>
      </div>

      {/* Provider comparison cards (top 3) */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4 mb-6">
        {providers.slice(0, 3).map((p) => (
          <div
            key={p.providerPk}
            className="rounded-xl border border-surface-200 bg-surface-0 p-5 transition-all hover:shadow-md"
          >
            <div className="flex items-center justify-between mb-3">
              <div className="flex items-center gap-2">
                <RankBadge rank={p.rank} />
                <span className="font-mono text-xs text-surface-900 font-medium">
                  {truncatePk(p.providerPk)}
                </span>
              </div>
              <span className="text-xs text-surface-800/40 bg-surface-100 px-2 py-0.5 rounded-full">
                {p.region}
              </span>
            </div>
            <div className="flex flex-wrap gap-1 mb-4">
              {p.models.map((m) => (
                <span
                  key={m}
                  className="text-[10px] px-1.5 py-0.5 rounded bg-brand-50 text-brand-700 dark:bg-brand-950/30 dark:text-brand-400"
                >
                  {m}
                </span>
              ))}
            </div>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <div className="text-[10px] text-surface-800/40">Requests</div>
                <div className="text-sm font-bold text-surface-900">{formatNumber(p.requests)}</div>
              </div>
              <div>
                <div className="text-[10px] text-surface-800/40">Tokens</div>
                <div className="text-sm font-bold text-surface-900">{formatNumber(p.tokens)}</div>
              </div>
              <div>
                <div className="text-[10px] text-surface-800/40">Earnings</div>
                <div className="text-sm font-bold text-surface-900">{nanoergToErg(p.earningsNanoErg)}</div>
              </div>
              <div>
                <div className="text-[10px] text-surface-800/40">Avg Latency</div>
                <div className={`text-sm font-bold ${getCellColor(p.averageLatencyMs, 200, 400, true)}`}>
                  {p.averageLatencyMs}ms
                </div>
              </div>
              <div>
                <div className="text-[10px] text-surface-800/40">Uptime</div>
                <div className={`text-sm font-bold ${getCellColor(p.uptime, 99, 96, false)}`}>
                  {p.uptime}%
                </div>
              </div>
              <div>
                <div className="text-[10px] text-surface-800/40">Reputation</div>
                <div className={`text-sm font-bold ${getCellColor(p.reputationScore, 90, 70, false)}`}>
                  {p.reputationScore}
                </div>
              </div>
            </div>
          </div>
        ))}
      </div>

      {/* Full comparison table */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
        <div className="px-5 py-4 border-b border-surface-100">
          <h2 className="text-base font-semibold text-surface-900">All Providers</h2>
          <p className="text-xs text-surface-800/40 mt-0.5">
            {providers.length} providers found. Click headers to sort.
          </p>
        </div>
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-surface-200 bg-surface-50 dark:bg-surface-800/50">
                <th className="text-xs text-surface-800/50 px-3 py-2 font-medium text-left w-8">#</th>
                <th className="text-xs text-surface-800/50 px-3 py-2 font-medium text-left">Provider</th>
                <th className="text-xs text-surface-800/50 px-3 py-2 font-medium text-left">Region</th>
                <th className="text-xs text-surface-800/50 px-3 py-2 font-medium text-left">Models</th>
                <SortButton label="Latency" sortKey="latency" currentSort={sortKey} direction={sortDir} onSort={handleSort} />
                <SortButton label="Throughput" sortKey="throughput" currentSort={sortKey} direction={sortDir} onSort={handleSort} />
                <SortButton label="Reliability" sortKey="reliability" currentSort={sortKey} direction={sortDir} onSort={handleSort} />
                <SortButton label="Cost" sortKey="cost" currentSort={sortKey} direction={sortDir} onSort={handleSort} />
                <SortButton label="Reputation" sortKey="reputation" currentSort={sortKey} direction={sortDir} onSort={handleSort} />
                <th className="text-xs text-surface-800/50 px-3 py-2 font-medium">Trend</th>
              </tr>
            </thead>
            <tbody>
              {providers.map((p) => (
                <tr
                  key={p.providerPk}
                  className="border-b border-surface-100 hover:bg-surface-50 dark:hover:bg-surface-800/30 transition-colors"
                >
                  <td className="px-3 py-2.5">
                    <RankBadge rank={p.rank} />
                  </td>
                  <td className="px-3 py-2.5 font-mono text-xs text-surface-900">
                    {truncatePk(p.providerPk)}
                  </td>
                  <td className="px-3 py-2.5 text-xs text-surface-800/60">{p.region}</td>
                  <td className="px-3 py-2.5">
                    <div className="flex flex-wrap gap-0.5">
                      {p.models.map((m) => (
                        <span
                          key={m}
                          className="text-[9px] px-1 py-0.5 rounded bg-surface-100 text-surface-800/60 dark:bg-surface-700"
                        >
                          {m.split("-")[0]}
                        </span>
                      ))}
                    </div>
                  </td>
                  <td className={`px-3 py-2.5 text-xs text-right font-mono ${getCellColor(p.averageLatencyMs, 200, 400, true)}`}>
                    {p.averageLatencyMs}ms
                  </td>
                  <td className={`px-3 py-2.5 text-xs text-right font-mono ${getCellColor(p.requests, 15000, 5000)}`}>
                    {formatNumber(p.requests)}
                  </td>
                  <td className={`px-3 py-2.5 text-xs text-right font-mono ${getCellColor(p.uptime, 99, 96)}`}>
                    {p.uptime}%
                  </td>
                  <td className="px-3 py-2.5 text-xs text-right font-mono text-surface-800">
                    {nanoergToErg(p.earningsNanoErg)}
                  </td>
                  <td className={`px-3 py-2.5 text-xs text-right font-mono font-bold ${getCellColor(p.reputationScore, 90, 70)}`}>
                    {p.reputationScore}
                  </td>
                  <td className="px-3 py-2.5">
                    <Sparkline
                      values={[p.averageLatencyMs, p.p95LatencyMs, p.errorRate * 100, p.uptime, p.reputationScore]}
                      color={p.reputationScore >= 90 ? "bg-emerald-500" : p.reputationScore >= 70 ? "bg-amber-500" : "bg-red-500"}
                    />
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
