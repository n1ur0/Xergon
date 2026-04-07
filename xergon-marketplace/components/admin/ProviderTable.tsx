"use client";

import { useState, useMemo } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface ProviderAdmin {
  providerPk: string;
  endpoint: string;
  region: string;
  models: string[];
  status: "active" | "suspended" | "pending";
  totalEarningsNanoErg: number;
  totalRequests: number;
  averageLatencyMs: number;
  uptime: number;
  registeredAt: string;
  lastHeartbeat: string;
  slashCount: number;
  disputeCount: number;
}

type SortKey = "region" | "status" | "totalEarningsNanoErg" | "totalRequests" | "averageLatencyMs" | "uptime" | "slashCount" | "disputeCount";
type SortDir = "asc" | "desc";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function truncatePk(pk: string): string {
  if (pk.length <= 14) return pk;
  return `${pk.slice(0, 10)}...${pk.slice(-4)}`;
}

function nanoergToErg(nanoerg: number): string {
  if (nanoerg <= 0) return "0";
  return `${(nanoerg / 1e9).toFixed(1)} ERG`;
}

function relativeTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const minutes = Math.floor(diff / 60_000);
  const hours = Math.floor(diff / 3_600_000);
  if (minutes < 1) return "Just now";
  if (minutes < 60) return `${minutes}m ago`;
  if (hours < 24) return `${hours}h ago`;
  return `${Math.floor(hours / 24)}d ago`;
}

// ---------------------------------------------------------------------------
// Status badge
// ---------------------------------------------------------------------------

function StatusBadge({ status }: { status: string }) {
  const colors: Record<string, string> = {
    active: "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400",
    suspended: "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400",
    pending: "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400",
  };

  return (
    <span className={`inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium ${colors[status] ?? "bg-surface-100 text-surface-700"}`}>
      {status}
    </span>
  );
}

// ---------------------------------------------------------------------------
// Sortable header
// ---------------------------------------------------------------------------

function SortHeader({
  label,
  sortKey,
  currentSort,
  currentDir,
  onSort,
}: {
  label: string;
  sortKey: SortKey;
  currentSort: SortKey | null;
  currentDir: SortDir;
  onSort: (key: SortKey) => void;
}) {
  const isActive = currentSort === sortKey;

  return (
    <button
      type="button"
      onClick={() => onSort(sortKey)}
      className={`inline-flex items-center gap-1 text-xs font-medium uppercase tracking-wider px-2 py-1 rounded hover:bg-surface-100 transition-colors ${
        isActive ? "text-brand-600" : "text-surface-800/50"
      }`}
    >
      {label}
      {isActive && (
        <svg className="w-3 h-3" viewBox="0 0 20 20" fill="currentColor">
          {currentDir === "asc" ? (
            <path fillRule="evenodd" d="M14.707 12.707a1 1 0 01-1.414 0L10 9.414l-3.293 3.293a1 1 0 01-1.414-1.414l4-4a1 1 0 011.414 0l4 4a1 1 0 010 1.414z" clipRule="evenodd" />
          ) : (
            <path fillRule="evenodd" d="M5.293 7.293a1 1 0 011.414 0L10 10.586l3.293-3.293a1 1 0 111.414 1.414l-4 4a1 1 0 01-1.414 0l-4-4a1 1 0 010-1.414z" clipRule="evenodd" />
          )}
        </svg>
      )}
    </button>
  );
}

// ---------------------------------------------------------------------------
// ProviderTable
// ---------------------------------------------------------------------------

export function ProviderTable({
  providers,
  onStatusChange,
}: {
  providers: ProviderAdmin[];
  onStatusChange: (pk: string, status: string) => void;
}) {
  const [search, setSearch] = useState("");
  const [sortKey, setSortKey] = useState<SortKey | null>(null);
  const [sortDir, setSortDir] = useState<SortDir>("desc");
  const [changingPk, setChangingPk] = useState<string | null>(null);

  const handleSort = (key: SortKey) => {
    if (sortKey === key) {
      setSortDir((d) => (d === "asc" ? "desc" : "asc"));
    } else {
      setSortKey(key);
      setSortDir("desc");
    }
  };

  const filtered = useMemo(() => {
    let list = [...providers];

    // Search filter
    if (search.trim()) {
      const q = search.toLowerCase();
      list = list.filter(
        (p) =>
          p.providerPk.toLowerCase().includes(q) ||
          p.endpoint.toLowerCase().includes(q) ||
          p.region.toLowerCase().includes(q) ||
          p.models.some((m) => m.toLowerCase().includes(q)),
      );
    }

    // Sort
    if (sortKey) {
      const dir = sortDir === "asc" ? 1 : -1;
      list.sort((a, b) => {
        const aVal = a[sortKey];
        const bVal = b[sortKey];
        if (typeof aVal === "string" && typeof bVal === "string") {
          return aVal.localeCompare(bVal) * dir;
        }
        return ((aVal as number) - (bVal as number)) * dir;
      });
    }

    return list;
  }, [providers, search, sortKey, sortDir]);

  const handleToggle = async (pk: string, currentStatus: string) => {
    const newStatus = currentStatus === "active" ? "suspended" : "active";
    setChangingPk(pk);
    try {
      await fetch(`/api/admin/providers/${encodeURIComponent(pk)}`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ status: newStatus }),
      });
      onStatusChange(pk, newStatus);
    } catch {
      // silently fail for now
    } finally {
      setChangingPk(null);
    }
  };

  return (
    <div className="space-y-4">
      {/* Search */}
      <div className="relative">
        <svg
          className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-surface-800/30"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <circle cx="11" cy="11" r="8" />
          <line x1="21" y1="21" x2="16.65" y2="16.65" />
        </svg>
        <input
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search by PK, endpoint, region, or model..."
          className="w-full pl-10 pr-4 py-2.5 text-sm rounded-lg border border-surface-200 bg-surface-0 placeholder-surface-800/30 focus:outline-none focus:ring-2 focus:ring-brand-500/30 focus:border-brand-500 transition-colors"
        />
      </div>

      {/* Table */}
      <div className="overflow-x-auto rounded-xl border border-surface-200">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-surface-200 bg-surface-50">
              <th className="text-left px-4 py-3">
                <span className="text-xs font-medium uppercase tracking-wider text-surface-800/50 px-2 py-1">
                  Provider
                </span>
              </th>
              <th className="text-left px-4 py-3">
                <SortHeader label="Region" sortKey="region" currentSort={sortKey} currentDir={sortDir} onSort={handleSort} />
              </th>
              <th className="text-left px-4 py-3 hidden lg:table-cell">
                <span className="text-xs font-medium uppercase tracking-wider text-surface-800/50 px-2 py-1">
                  Models
                </span>
              </th>
              <th className="text-left px-4 py-3">
                <SortHeader label="Status" sortKey="status" currentSort={sortKey} currentDir={sortDir} onSort={handleSort} />
              </th>
              <th className="text-right px-4 py-3 hidden md:table-cell">
                <SortHeader label="Earnings" sortKey="totalEarningsNanoErg" currentSort={sortKey} currentDir={sortDir} onSort={handleSort} />
              </th>
              <th className="text-right px-4 py-3 hidden md:table-cell">
                <SortHeader label="Requests" sortKey="totalRequests" currentSort={sortKey} currentDir={sortDir} onSort={handleSort} />
              </th>
              <th className="text-right px-4 py-3 hidden xl:table-cell">
                <SortHeader label="Latency" sortKey="averageLatencyMs" currentSort={sortKey} currentDir={sortDir} onSort={handleSort} />
              </th>
              <th className="text-right px-4 py-3 hidden xl:table-cell">
                <SortHeader label="Uptime" sortKey="uptime" currentSort={sortKey} currentDir={sortDir} onSort={handleSort} />
              </th>
              <th className="text-right px-4 py-3 hidden lg:table-cell">
                <SortHeader label="Slashes" sortKey="slashCount" currentSort={sortKey} currentDir={sortDir} onSort={handleSort} />
              </th>
              <th className="text-right px-4 py-3 hidden lg:table-cell">
                <SortHeader label="Disputes" sortKey="disputeCount" currentSort={sortKey} currentDir={sortDir} onSort={handleSort} />
              </th>
              <th className="text-center px-4 py-3">
                <span className="text-xs font-medium uppercase tracking-wider text-surface-800/50 px-2 py-1">
                  Actions
                </span>
              </th>
            </tr>
          </thead>
          <tbody className="divide-y divide-surface-100">
            {filtered.map((provider) => (
              <tr key={provider.providerPk} className="hover:bg-surface-50 transition-colors">
                <td className="px-4 py-3">
                  <div className="font-mono text-xs text-surface-800/70" title={provider.providerPk}>
                    {truncatePk(provider.providerPk)}
                  </div>
                  <div className="text-xs text-surface-800/30 mt-0.5 truncate max-w-[160px]" title={provider.endpoint}>
                    {provider.endpoint}
                  </div>
                  <div className="text-xs text-surface-800/30">
                    Last seen: {relativeTime(provider.lastHeartbeat)}
                  </div>
                </td>
                <td className="px-4 py-3">
                  <span className="inline-flex items-center px-2 py-0.5 rounded text-xs font-medium bg-surface-100 text-surface-700">
                    {provider.region}
                  </span>
                </td>
                <td className="px-4 py-3 hidden lg:table-cell">
                  <div className="flex flex-wrap gap-1">
                    {provider.models.slice(0, 2).map((m) => (
                      <span key={m} className="inline-block px-1.5 py-0.5 rounded text-xs bg-surface-100 text-surface-800/60 truncate max-w-[120px]" title={m}>
                        {m}
                      </span>
                    ))}
                    {provider.models.length > 2 && (
                      <span className="text-xs text-surface-800/30">+{provider.models.length - 2}</span>
                    )}
                  </div>
                </td>
                <td className="px-4 py-3">
                  <StatusBadge status={provider.status} />
                </td>
                <td className="px-4 py-3 text-right hidden md:table-cell">
                  <span className="text-sm font-medium text-surface-900">
                    {nanoergToErg(provider.totalEarningsNanoErg)}
                  </span>
                </td>
                <td className="px-4 py-3 text-right hidden md:table-cell">
                  <span className="text-sm text-surface-800/70">
                    {provider.totalRequests.toLocaleString()}
                  </span>
                </td>
                <td className="px-4 py-3 text-right hidden xl:table-cell">
                  <span className={`text-sm ${provider.averageLatencyMs > 300 ? "text-amber-600" : "text-surface-800/70"}`}>
                    {provider.averageLatencyMs}ms
                  </span>
                </td>
                <td className="px-4 py-3 text-right hidden xl:table-cell">
                  <span className={`text-sm ${provider.uptime < 95 ? "text-amber-600" : "text-surface-800/70"}`}>
                    {provider.uptime}%
                  </span>
                </td>
                <td className="px-4 py-3 text-right hidden lg:table-cell">
                  <span className={`text-sm ${provider.slashCount > 0 ? "text-danger-500 font-medium" : "text-surface-800/40"}`}>
                    {provider.slashCount}
                  </span>
                </td>
                <td className="px-4 py-3 text-right hidden lg:table-cell">
                  <span className={`text-sm ${provider.disputeCount > 0 ? "text-amber-600 font-medium" : "text-surface-800/40"}`}>
                    {provider.disputeCount}
                  </span>
                </td>
                <td className="px-4 py-3 text-center">
                  {changingPk === provider.providerPk ? (
                    <span className="inline-block w-4 h-4 border-2 border-brand-500 border-t-transparent rounded-full animate-spin" />
                  ) : (
                    <button
                      type="button"
                      onClick={() => handleToggle(provider.providerPk, provider.status)}
                      className={`inline-flex items-center px-3 py-1.5 rounded-lg text-xs font-medium transition-colors ${
                        provider.status === "active"
                          ? "bg-red-50 text-red-600 hover:bg-red-100 dark:bg-red-900/20 dark:hover:bg-red-900/30"
                          : "bg-emerald-50 text-emerald-600 hover:bg-emerald-100 dark:bg-emerald-900/20 dark:hover:bg-emerald-900/30"
                      }`}
                    >
                      {provider.status === "active" ? "Suspend" : "Activate"}
                    </button>
                  )}
                </td>
              </tr>
            ))}
            {filtered.length === 0 && (
              <tr>
                <td colSpan={11} className="px-4 py-8 text-center text-surface-800/40">
                  No providers found matching your search.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      <div className="text-xs text-surface-800/30">
        Showing {filtered.length} of {providers.length} providers
      </div>
    </div>
  );
}
