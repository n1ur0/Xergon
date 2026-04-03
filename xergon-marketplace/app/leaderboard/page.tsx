"use client";

import { useState, useEffect } from "react";
import { endpoints, type LeaderboardEntry } from "@/lib/api/client";
import { cn } from "@/lib/utils";

// ── Helpers ──

function formatTokens(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(0)}K`;
  return String(n);
}

function formatRevenue(usd: number): string {
  if (usd >= 1_000) return `$${(usd / 1_000).toFixed(1)}K`;
  return `$${usd.toFixed(2)}`;
}

function formatRequests(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(0)}K`;
  return String(n);
}

// ── Rank badge for top 3 ──

const RANK_STYLES: Record<number, { badge: string; text: string; label: string }> = {
  1: { badge: "bg-amber-400 text-white shadow-amber-200 shadow-md", text: "text-amber-500", label: "1st" },
  2: { badge: "bg-slate-300 text-slate-700 shadow-slate-200 shadow-md", text: "text-slate-400", label: "2nd" },
  3: { badge: "bg-orange-300 text-orange-800 shadow-orange-200 shadow-md", text: "text-orange-400", label: "3rd" },
};

function RankBadge({ rank }: { rank: number }) {
  const style = RANK_STYLES[rank];
  if (style) {
    return (
      <span
        className={cn(
          "inline-flex items-center justify-center h-7 w-7 rounded-full text-xs font-bold",
          style.badge,
        )}
      >
        {rank}
      </span>
    );
  }
  return (
    <span className="inline-flex items-center justify-center h-7 w-7 rounded-full text-xs font-medium bg-surface-100 text-surface-800/50">
      {rank}
    </span>
  );
}

// ── Online status dot ──

function StatusDot({ online }: { online: boolean }) {
  return (
    <span className="inline-flex items-center gap-1.5">
      <span
        className={cn(
          "h-2 w-2 rounded-full",
          online ? "bg-emerald-500 shadow-sm shadow-emerald-300" : "bg-surface-300",
        )}
      />
      <span className="text-xs text-surface-800/50">{online ? "Online" : "Offline"}</span>
    </span>
  );
}

// ── Desktop table row ──

function TableRow({ entry, rank }: { entry: LeaderboardEntry; rank: number }) {
  const isTop3 = rank <= 3;
  const style = RANK_STYLES[rank];

  return (
    <tr
      className={cn(
        "border-b border-surface-100 transition-colors hover:bg-surface-50",
        isTop3 && "bg-surface-50/50",
      )}
    >
      <td className="py-3.5 px-4">
        <RankBadge rank={rank} />
      </td>
      <td className="py-3.5 px-4">
        <div className="flex flex-col">
          <span className={cn("font-semibold text-sm", isTop3 && style?.text)}>
            {entry.provider_name}
          </span>
          <span className="text-xs text-surface-800/40">{entry.region}</span>
        </div>
      </td>
      <td className="py-3.5 px-4">
        <StatusDot online={entry.online} />
      </td>
      <td className="py-3.5 px-4 text-sm text-surface-800/70">
        {entry.unique_models}
      </td>
      <td className="py-3.5 px-4 text-sm font-medium text-surface-900">
        {formatTokens(entry.total_tokens)}
      </td>
      <td className="py-3.5 px-4 text-sm text-surface-800/70">
        {formatRequests(entry.total_requests)}
      </td>
      <td className="py-3.5 px-4 text-sm text-surface-800/70">
        {formatRevenue(entry.total_revenue_usd)}
      </td>
    </tr>
  );
}

// ── Mobile card ──

function MobileCard({ entry, rank }: { entry: LeaderboardEntry; rank: number }) {
  const isTop3 = rank <= 3;
  const style = RANK_STYLES[rank];

  return (
    <div
      className={cn(
        "rounded-xl border p-4 transition-all",
        isTop3
          ? "border-amber-200 bg-amber-50/30 shadow-sm"
          : "border-surface-200 bg-surface-0",
      )}
    >
      {/* Rank + Name + Status */}
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-3">
          <RankBadge rank={rank} />
          <div>
            <h3 className={cn("font-semibold text-sm", isTop3 && style?.text)}>
              {entry.provider_name}
            </h3>
            <p className="text-xs text-surface-800/40">{entry.region}</p>
          </div>
        </div>
        <StatusDot online={entry.online} />
      </div>

      {/* Stats grid */}
      <div className="grid grid-cols-3 gap-3 pt-3 border-t border-surface-100">
        <div>
          <p className="text-xs text-surface-800/40">Models</p>
          <p className="text-sm font-medium text-surface-900">{entry.unique_models}</p>
        </div>
        <div>
          <p className="text-xs text-surface-800/40">Tokens</p>
          <p className="text-sm font-medium text-surface-900">{formatTokens(entry.total_tokens)}</p>
        </div>
        <div>
          <p className="text-xs text-surface-800/40">Requests</p>
          <p className="text-sm font-medium text-surface-900">{formatRequests(entry.total_requests)}</p>
        </div>
      </div>

      {/* Revenue */}
      <div className="mt-2 pt-2 border-t border-surface-100 flex items-center justify-between">
        <span className="text-xs text-surface-800/40">Revenue</span>
        <span className="text-sm font-medium text-emerald-600">{formatRevenue(entry.total_revenue_usd)}</span>
      </div>
    </div>
  );
}

// ── Loading skeleton ──

function LoadingSkeleton() {
  return (
    <div className="space-y-3">
      {Array.from({ length: 6 }).map((_, i) => (
        <div
          key={i}
          className="rounded-xl border border-surface-200 bg-surface-0 p-4 animate-pulse"
        >
          <div className="flex items-center gap-3 mb-3">
            <div className="h-7 w-7 rounded-full bg-surface-200" />
            <div className="flex-1">
              <div className="h-4 w-32 rounded bg-surface-200 mb-1" />
              <div className="h-3 w-16 rounded bg-surface-100" />
            </div>
          </div>
          <div className="grid grid-cols-3 gap-3">
            <div className="h-3 w-12 rounded bg-surface-100" />
            <div className="h-3 w-16 rounded bg-surface-100" />
            <div className="h-3 w-14 rounded bg-surface-100" />
          </div>
        </div>
      ))}
    </div>
  );
}

// ── Main page ──

export default function LeaderboardPage() {
  const [entries, setEntries] = useState<LeaderboardEntry[]>([]);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    endpoints
      .leaderboard()
      .then((data) => setEntries(data))
      .catch(() => {
        // Empty state on error
      })
      .finally(() => setIsLoading(false));
  }, []);

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="mb-8">
        <h1 className="text-2xl font-bold mb-2">Provider Leaderboard</h1>
        <p className="text-surface-800/60">
          Ranked by total tokens processed across all models
        </p>
      </div>

      {/* Loading */}
      {isLoading && <LoadingSkeleton />}

      {/* Empty state */}
      {!isLoading && entries.length === 0 && (
        <div className="text-center py-16">
          <div className="mx-auto mb-4 h-16 w-16 rounded-full bg-surface-100 flex items-center justify-center">
            <svg
              xmlns="http://www.w3.org/2000/svg"
              width="28"
              height="28"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
              strokeLinejoin="round"
              className="text-surface-800/30"
            >
              <path d="M6 9H4.5a2.5 2.5 0 0 1 0-5H6" />
              <path d="M18 9h1.5a2.5 2.5 0 0 0 0-5H18" />
              <path d="M4 22h16" />
              <path d="M10 14.66V17c0 .55-.47.98-.97 1.21C7.85 18.75 7 20.24 7 22" />
              <path d="M14 14.66V17c0 .55.47.98.97 1.21C16.15 18.75 17 20.24 17 22" />
              <path d="M18 2H6v7a6 6 0 0 0 12 0V2Z" />
            </svg>
          </div>
          <p className="text-surface-800/40 text-lg mb-1">No providers yet</p>
          <p className="text-surface-800/30 text-sm">
            Leaderboard will populate once providers start processing requests.
          </p>
        </div>
      )}

      {/* Data loaded */}
      {!isLoading && entries.length > 0 && (
        <>
          {/* Desktop table */}
          <div className="hidden md:block overflow-x-auto rounded-xl border border-surface-200 bg-surface-0">
            <table className="w-full text-left">
              <thead>
                <tr className="border-b border-surface-200 bg-surface-50">
                  <th className="py-3 px-4 text-xs font-medium text-surface-800/40 uppercase tracking-wider w-16">
                    Rank
                  </th>
                  <th className="py-3 px-4 text-xs font-medium text-surface-800/40 uppercase tracking-wider">
                    Provider
                  </th>
                  <th className="py-3 px-4 text-xs font-medium text-surface-800/40 uppercase tracking-wider">
                    Status
                  </th>
                  <th className="py-3 px-4 text-xs font-medium text-surface-800/40 uppercase tracking-wider">
                    Models
                  </th>
                  <th className="py-3 px-4 text-xs font-medium text-surface-800/40 uppercase tracking-wider">
                    Total Tokens
                  </th>
                  <th className="py-3 px-4 text-xs font-medium text-surface-800/40 uppercase tracking-wider">
                    Requests
                  </th>
                  <th className="py-3 px-4 text-xs font-medium text-surface-800/40 uppercase tracking-wider">
                    Revenue
                  </th>
                </tr>
              </thead>
              <tbody>
                {entries.map((entry, index) => (
                  <TableRow key={entry.provider_id} entry={entry} rank={index + 1} />
                ))}
              </tbody>
            </table>
          </div>

          {/* Mobile cards */}
          <div className="md:hidden space-y-3">
            {entries.map((entry, index) => (
              <MobileCard key={entry.provider_id} entry={entry} rank={index + 1} />
            ))}
          </div>
        </>
      )}

      {/* Footer note */}
      {!isLoading && entries.length > 0 && (
        <div className="mt-10 text-sm text-surface-800/50 text-center">
          Data sourced from relay usage records. Updated in real-time as providers process requests.
        </div>
      )}
    </div>
  );
}
