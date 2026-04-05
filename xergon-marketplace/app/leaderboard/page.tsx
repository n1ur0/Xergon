"use client";

import { cn } from "@/lib/utils";
import { useLeaderboard } from "@/lib/hooks/use-chain-data";
import type { ChainLeaderboardEntry } from "@/lib/api/chain";
import { ApiErrorDisplay } from "@/components/ui/ErrorBoundary";
import { EmptyState } from "@/components/ui/EmptyState";
import { ErrorBoundary } from "@/components/ui/ErrorBoundary";
import { LeaderboardSkeleton } from "@/components/leaderboard/LeaderboardSkeleton";
import { SuspenseWrap } from "@/components/ui/SuspenseWrap";

// ── Helpers ──

function formatTokens(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(0)}K`;
  return String(n);
}

function formatRequests(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(0)}K`;
  return String(n);
}

function formatLatency(ms: number): string {
  if (ms >= 1000) return `${(ms / 1000).toFixed(1)}s`;
  return `${ms}ms`;
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

function TableRow({ entry, rank }: { entry: ChainLeaderboardEntry; rank: number }) {
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
            {entry.provider_id}
          </span>
          <span className="text-xs text-surface-800/40">
            {entry.region || "Unknown"}
          </span>
        </div>
      </td>
      <td className="py-3.5 px-4">
        <StatusDot online={entry.online} />
      </td>
      <td className="py-3.5 px-4 text-sm text-surface-800/70">
        {formatLatency(entry.latency_ms)}
      </td>
      <td className="py-3.5 px-4 text-sm font-medium text-surface-900">
        {formatTokens(entry.total_tokens)}
      </td>
      <td className="py-3.5 px-4 text-sm text-surface-800/70">
        {formatRequests(entry.total_requests)}
      </td>
      <td className="py-3.5 px-4">
        {entry.pown_score !== undefined && entry.pown_score > 0 ? (
          <span className="inline-flex items-center rounded-full bg-violet-100 px-2 py-0.5 text-xs font-medium text-violet-700">
            {entry.pown_score.toFixed(1)}
          </span>
        ) : (
          <span className="text-xs text-surface-800/30">--</span>
        )}
      </td>
    </tr>
  );
}

// ── Mobile card ──

function MobileCard({ entry, rank }: { entry: ChainLeaderboardEntry; rank: number }) {
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
              {entry.provider_id}
            </h3>
            <p className="text-xs text-surface-800/40">
              {entry.region || "Unknown"}
            </p>
          </div>
        </div>
        <StatusDot online={entry.online} />
      </div>

      {/* Stats grid */}
      <div className="grid grid-cols-3 gap-3 pt-3 border-t border-surface-100">
        <div>
          <p className="text-xs text-surface-800/40">Latency</p>
          <p className="text-sm font-medium text-surface-900">{formatLatency(entry.latency_ms)}</p>
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

      {/* PoNW Score */}
      {entry.pown_score !== undefined && entry.pown_score > 0 && (
        <div className="mt-2 pt-2 border-t border-surface-100 flex items-center justify-between">
          <span className="text-xs text-surface-800/40">PoNW Score</span>
          <span className="text-sm font-medium text-violet-600">{entry.pown_score.toFixed(1)}</span>
        </div>
      )}
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
          className="rounded-xl border border-surface-200 bg-surface-0 p-4"
        >
          <div className="flex items-center gap-3 mb-3">
            <div className="h-7 w-7 rounded-full skeleton-shimmer" />
            <div className="flex-1">
              <div className="h-4 w-32 rounded skeleton-shimmer mb-1" />
              <div className="h-3 w-16 rounded skeleton-shimmer" />
            </div>
          </div>
          <div className="grid grid-cols-3 gap-3">
            <div className="h-3 w-12 rounded skeleton-shimmer" />
            <div className="h-3 w-16 rounded skeleton-shimmer" />
            <div className="h-3 w-14 rounded skeleton-shimmer" />
          </div>
        </div>
      ))}
    </div>
  );
}

// ── Main page ──

export default function LeaderboardPage() {
  const { entries, isLoading, error, refresh } = useLeaderboard();

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="mb-8">
        <h1 className="text-2xl font-bold mb-2">Provider Leaderboard</h1>
        <p className="text-surface-800/60">
          Ranked by total tokens processed across all models. Live data from the relay.
        </p>
      </div>

      <SuspenseWrap fallback={<LeaderboardSkeleton />}>
      {/* Error state */}
      {error && !isLoading && (
        <ApiErrorDisplay
          error={error}
          onRetry={refresh}
          className="mb-6"
        />
      )}

      {/* Loading */}
      {isLoading && !error && <LoadingSkeleton />}

      {/* Empty state */}
      {!isLoading && !error && entries.length === 0 && (
        <EmptyState type="no-providers" />
      )}

      {/* Data loaded */}
      {!isLoading && entries.length > 0 && (
        <ErrorBoundary context="Leaderboard Data">
          <div className="animate-fade-in">
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
                      Latency
                    </th>
                    <th className="py-3 px-4 text-xs font-medium text-surface-800/40 uppercase tracking-wider">
                      Total Tokens
                    </th>
                    <th className="py-3 px-4 text-xs font-medium text-surface-800/40 uppercase tracking-wider">
                      Requests
                    </th>
                    <th className="py-3 px-4 text-xs font-medium text-surface-800/40 uppercase tracking-wider">
                      PoNW Score
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
          </div>
        </ErrorBoundary>
      )}

      {/* Footer note */}
      {!isLoading && entries.length > 0 && (
        <div className="mt-10 text-sm text-surface-800/50 text-center">
          Live data from the Xergon relay. Auto-refreshes every 30 seconds.
        </div>
      )}
      </SuspenseWrap>
    </div>
  );
}
