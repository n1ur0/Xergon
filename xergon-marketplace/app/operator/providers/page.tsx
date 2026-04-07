"use client";

import { useState, useEffect, useMemo, useCallback } from "react";
import Link from "next/link";
import {
  fetchProviders,
  filterProviders,
  extractModels,
  type ProviderInfo,
} from "@/lib/api/providers";

// ---------------------------------------------------------------------------
// Status helpers
// ---------------------------------------------------------------------------

const STATUS_COLORS: Record<string, string> = {
  online: "bg-accent-100 text-accent-700",
  degraded: "bg-yellow-100 text-yellow-700",
  offline: "bg-surface-200 text-surface-800/40",
};

const STATUS_DOT: Record<string, string> = {
  online: "bg-accent-500",
  degraded: "bg-yellow-500",
  offline: "bg-surface-400",
};

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function timeAgo(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  if (diff < 60_000) return "just now";
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`;
  return `${Math.floor(diff / 86_400_000)}d ago`;
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export default function ProvidersListPage() {
  const [allProviders, setAllProviders] = useState<ProviderInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [degraded, setDegraded] = useState(false);

  // Filters
  const [search, setSearch] = useState("");
  const [statusFilter, setStatusFilter] = useState("all");
  const [regionFilter, setRegionFilter] = useState("all");
  const [modelFilter, setModelFilter] = useState("all");
  const [sortBy, setSortBy] = useState("aiPoints");
  const [sortOrder, setSortOrder] = useState<"asc" | "desc">("desc");

  const loadData = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await fetchProviders();
      setAllProviders(res.providers);
      setDegraded(res.degraded ?? false);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
    const interval = setInterval(loadData, 30_000);
    return () => clearInterval(interval);
  }, [loadData]);

  const models = useMemo(() => extractModels(allProviders), [allProviders]);

  const regions = useMemo(() => {
    const set = new Set(allProviders.map((p) => p.region));
    return Array.from(set).sort();
  }, [allProviders]);

  const filtered = useMemo(
    () =>
      filterProviders(allProviders, {
        search,
        status: statusFilter,
        region: regionFilter,
        model: modelFilter,
        sortBy,
        sortOrder,
      }),
    [allProviders, search, statusFilter, regionFilter, modelFilter, sortBy, sortOrder],
  );

  const statusCounts = useMemo(() => {
    const counts: Record<string, number> = { all: allProviders.length, online: 0, degraded: 0, offline: 0 };
    for (const p of allProviders) {
      if (p.status in counts) counts[p.status]++;
    }
    return counts;
  }, [allProviders]);

  // ---------------------------------------------------------------------------
  // Render
  // ---------------------------------------------------------------------------

  return (
    <div className="space-y-6">
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold text-surface-900">All Providers</h1>
          <p className="text-sm text-surface-800/50 mt-1">
            Browse and manage all registered providers.
            {degraded && (
              <span className="ml-2 text-yellow-600">
                (Showing fallback data - relay unreachable)
              </span>
            )}
          </p>
        </div>
        <button
          onClick={loadData}
          disabled={loading}
          className="rounded-lg border border-surface-200 px-4 py-2 text-sm font-medium text-surface-800/70 hover:bg-surface-100 transition-colors disabled:opacity-50 inline-flex items-center gap-2"
        >
          <svg className={`w-4 h-4 ${loading ? "animate-spin" : ""}`} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M21 2v6h-6" /><path d="M3 12a9 9 0 0115-6.7L21 8" /><path d="M3 22v-6h6" /><path d="M21 12a9 9 0 01-15 6.7L3 16" />
          </svg>
          Refresh
        </button>
      </div>

      {/* Status summary chips */}
      <div className="flex flex-wrap gap-2">
        {(["all", "online", "degraded", "offline"] as const).map((s) => (
          <button
            key={s}
            onClick={() => setStatusFilter(s)}
            className={`inline-flex items-center gap-1.5 rounded-full px-3 py-1.5 text-xs font-medium transition-colors ${
              statusFilter === s
                ? "bg-brand-600 text-white"
                : "bg-surface-100 text-surface-800/60 hover:bg-surface-200"
            }`}
          >
            {s !== "all" && (
              <span className={`h-1.5 w-1.5 rounded-full ${STATUS_DOT[s]}`} />
            )}
            {s.charAt(0).toUpperCase() + s.slice(1)}
            <span className="opacity-70">({statusCounts[s] ?? 0})</span>
          </button>
        ))}
      </div>

      {/* Filters row */}
      <div className="flex flex-wrap items-center gap-3">
        <div className="relative flex-1 min-w-[200px]">
          <svg className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-surface-800/30" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <circle cx="11" cy="11" r="8" /><line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>
          <input
            type="text"
            placeholder="Search providers, models, GPUs..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="w-full rounded-lg border border-surface-200 bg-surface-0 pl-10 pr-4 py-2 text-sm text-surface-900 placeholder:text-surface-800/30 focus:outline-none focus:ring-2 focus:ring-brand-500/20 focus:border-brand-400"
          />
        </div>

        <select
          value={regionFilter}
          onChange={(e) => setRegionFilter(e.target.value)}
          className="rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm text-surface-800/70 focus:outline-none focus:ring-2 focus:ring-brand-500/20"
        >
          <option value="all">All Regions</option>
          {regions.map((r) => (
            <option key={r} value={r}>{r}</option>
          ))}
        </select>

        <select
          value={modelFilter}
          onChange={(e) => setModelFilter(e.target.value)}
          className="rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm text-surface-800/70 focus:outline-none focus:ring-2 focus:ring-brand-500/20"
        >
          <option value="all">All Models</option>
          {models.map((m) => (
            <option key={m} value={m}>{m}</option>
          ))}
        </select>

        <select
          value={sortBy}
          onChange={(e) => setSortBy(e.target.value)}
          className="rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm text-surface-800/70 focus:outline-none focus:ring-2 focus:ring-brand-500/20"
        >
          <option value="aiPoints">Sort: AI Points</option>
          <option value="uptime">Sort: Uptime</option>
          <option value="tokens">Sort: Tokens</option>
          <option value="price">Sort: Price</option>
          <option value="name">Sort: Name</option>
        </select>

        <button
          onClick={() => setSortOrder((o) => (o === "desc" ? "asc" : "desc"))}
          className="rounded-lg border border-surface-200 px-3 py-2 text-sm font-medium text-surface-800/70 hover:bg-surface-100 transition-colors"
          title={sortOrder === "desc" ? "Descending" : "Ascending"}
        >
          {sortOrder === "desc" ? "Z-A / 9-1" : "A-Z / 1-9"}
        </button>
      </div>

      {/* Error state */}
      {error && !allProviders.length && (
        <div className="rounded-xl border border-danger-200 bg-danger-50/50 p-6 text-center">
          <p className="text-sm text-danger-700">Failed to load providers: {error}</p>
          <button onClick={loadData} className="mt-3 text-sm text-danger-600 hover:underline font-medium">
            Retry
          </button>
        </div>
      )}

      {/* Loading state */}
      {loading && !allProviders.length && (
        <div className="space-y-3 animate-pulse">
          {Array.from({ length: 5 }).map((_, i) => (
            <div key={i} className="h-14 bg-surface-200 rounded-xl" />
          ))}
        </div>
      )}

      {/* Provider table */}
      {filtered.length > 0 && (
        <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-surface-200 text-left bg-surface-50">
                  <th className="px-5 py-3 text-xs font-medium text-surface-800/50">Provider</th>
                  <th className="px-5 py-3 text-xs font-medium text-surface-800/50">Status</th>
                  <th className="px-5 py-3 text-xs font-medium text-surface-800/50 hidden lg:table-cell">Region</th>
                  <th className="px-5 py-3 text-xs font-medium text-surface-800/50 hidden md:table-cell">Models</th>
                  <th className="px-5 py-3 text-xs font-medium text-surface-800/50 hidden lg:table-cell">Uptime</th>
                  <th className="px-5 py-3 text-xs font-medium text-surface-800/50 hidden xl:table-cell">Tokens</th>
                  <th className="px-5 py-3 text-xs font-medium text-surface-800/50 hidden lg:table-cell">AI Points</th>
                  <th className="px-5 py-3 text-xs font-medium text-surface-800/50 hidden md:table-cell">Latency</th>
                  <th className="px-5 py-3 text-xs font-medium text-surface-800/50 hidden xl:table-cell">Last Seen</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((p) => (
                  <tr
                    key={p.endpoint}
                    className="border-b border-surface-100 hover:bg-surface-50 transition-colors"
                  >
                    <td className="px-5 py-3">
                      <div>
                        <Link
                          href={`/operator/providers/${encodeURIComponent(p.endpoint)}`}
                          className="font-medium text-surface-900 hover:text-brand-600 transition-colors"
                        >
                          {p.name}
                        </Link>
                        <p className="text-xs text-surface-800/40 font-mono mt-0.5 truncate max-w-[220px]">
                          {p.endpoint}
                        </p>
                        <p className="text-xs text-surface-800/30 mt-0.5">{p.gpuInfo}</p>
                      </div>
                    </td>
                    <td className="px-5 py-3">
                      <span className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium ${STATUS_COLORS[p.status] ?? STATUS_COLORS.offline}`}>
                        <span className={`h-1.5 w-1.5 rounded-full ${STATUS_DOT[p.status] ?? STATUS_DOT.offline}`} />
                        {p.status}
                      </span>
                    </td>
                    <td className="px-5 py-3 text-surface-800/60 hidden lg:table-cell">{p.region}</td>
                    <td className="px-5 py-3 hidden md:table-cell">
                      <div className="flex flex-wrap gap-1 max-w-[200px]">
                        {p.models.slice(0, 3).map((m) => (
                          <span key={m} className="inline-flex rounded bg-surface-100 px-1.5 py-0.5 text-[10px] font-mono text-surface-800/50">
                            {m}
                          </span>
                        ))}
                        {p.models.length > 3 && (
                          <span className="text-[10px] text-surface-800/30">+{p.models.length - 3}</span>
                        )}
                      </div>
                    </td>
                    <td className="px-5 py-3 hidden lg:table-cell">
                      <div className="flex items-center gap-2">
                        <div className="w-16 h-1.5 rounded-full bg-surface-100 overflow-hidden">
                          <div
                            className={`h-full rounded-full ${p.uptime >= 99 ? "bg-accent-500" : p.uptime >= 90 ? "bg-yellow-400" : "bg-danger-400"}`}
                            style={{ width: `${Math.min(p.uptime, 100)}%` }}
                          />
                        </div>
                        <span className="text-xs text-surface-800/60">{p.uptime}%</span>
                      </div>
                    </td>
                    <td className="px-5 py-3 text-surface-800/60 hidden xl:table-cell">{formatTokens(p.totalTokens)}</td>
                    <td className="px-5 py-3 font-medium hidden lg:table-cell">{p.aiPoints.toLocaleString()}</td>
                    <td className="px-5 py-3 text-surface-800/60 hidden md:table-cell">{p.latencyMs}ms</td>
                    <td className="px-5 py-3 text-xs text-surface-800/40 hidden xl:table-cell">{timeAgo(p.lastSeen)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          <div className="px-5 py-3 border-t border-surface-200 bg-surface-50 text-xs text-surface-800/40">
            Showing {filtered.length} of {allProviders.length} providers
          </div>
        </div>
      )}

      {/* Empty state */}
      {!loading && !error && allProviders.length > 0 && filtered.length === 0 && (
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-8 text-center">
          <p className="text-surface-800/50">No providers match your filters.</p>
          <button
            onClick={() => { setSearch(""); setStatusFilter("all"); setRegionFilter("all"); setModelFilter("all"); }}
            className="mt-2 text-sm text-brand-600 hover:underline font-medium"
          >
            Clear all filters
          </button>
        </div>
      )}
    </div>
  );
}
