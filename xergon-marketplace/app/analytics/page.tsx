"use client";

import { useState, useEffect, useCallback } from "react";
import { fetchNetworkStats, type NetworkStatsResponse } from "@/lib/api/analytics";
import { StatsHero } from "@/components/analytics/StatsHero";
import { RequestsChart } from "@/components/analytics/RequestsChart";
import { TopModelsTable } from "@/components/analytics/TopModelsTable";
import { RegionDistribution } from "@/components/analytics/RegionDistribution";
import { NetworkUptime } from "@/components/analytics/NetworkUptime";
import { AnalyticsSkeleton } from "@/components/analytics/AnalyticsSkeleton";
import { ErrorBoundary } from "@/components/ui/ErrorBoundary";
import { SuspenseWrap } from "@/components/ui/SuspenseWrap";

// ---------------------------------------------------------------------------
// Skeleton loaders
// ---------------------------------------------------------------------------

function SkeletonPulse({ className }: { className?: string }) {
  return <div className={`skeleton-shimmer rounded-lg ${className ?? ""}`} />;
}

function StatsSkeleton() {
  return (
    <div className="grid grid-cols-2 md:grid-cols-3 xl:grid-cols-6 gap-4">
      {Array.from({ length: 6 }).map((_, i) => (
        <div
          key={i}
          className="rounded-xl border border-surface-200 bg-surface-0 p-5"
        >
          <SkeletonPulse className="h-8 w-8 rounded-lg mb-3" />
          <SkeletonPulse className="h-7 w-24 mb-1.5" />
          <SkeletonPulse className="h-4 w-16 mb-1" />
          <SkeletonPulse className="h-3 w-12" />
        </div>
      ))}
    </div>
  );
}

function ChartSkeleton() {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
      <div className="flex items-center justify-between mb-4">
        <SkeletonPulse className="h-5 w-32" />
        <SkeletonPulse className="h-7 w-20 rounded-lg" />
      </div>
      <SkeletonPulse className="h-[260px] w-full" />
    </div>
  );
}

function TableSkeleton() {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
      <div className="px-5 py-4 border-b border-surface-100">
        <SkeletonPulse className="h-5 w-24 mb-1" />
        <SkeletonPulse className="h-3 w-40" />
      </div>
      <div className="space-y-0">
        {Array.from({ length: 5 }).map((_, i) => (
          <div key={i} className="flex items-center gap-4 px-5 py-3 border-b border-surface-50">
            <SkeletonPulse className="h-4 w-4" />
            <SkeletonPulse className="h-4 w-32" />
            <div className="flex-1" />
            <SkeletonPulse className="h-4 w-16" />
            <SkeletonPulse className="h-4 w-16" />
            <SkeletonPulse className="h-1.5 w-24 rounded-full" />
          </div>
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// ERG price ticker
// ---------------------------------------------------------------------------

function ErgPriceTicker({ price, degraded }: { price: number; degraded?: boolean }) {
  if (price <= 0 && !degraded) return null;

  return (
    <div className="inline-flex items-center gap-2 rounded-full border border-surface-200 bg-surface-0 px-4 py-1.5 text-sm">
      <span className="text-surface-800/40">ERG</span>
      <span className="font-semibold text-surface-900">
        {price > 0 ? `$${price.toFixed(2)}` : "--"}
      </span>
      {degraded && (
        <span className="inline-block h-1.5 w-1.5 rounded-full bg-amber-500 animate-pulse" title="Degraded mode" />
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Degraded banner
// ---------------------------------------------------------------------------

function DegradedBanner() {
  return (
    <div className="rounded-lg border border-amber-200 bg-amber-50 dark:border-amber-800/40 dark:bg-amber-950/20 px-4 py-3 text-sm text-amber-800 dark:text-amber-300">
      The relay is currently unreachable. Showing cached or estimated data.
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main page
// ---------------------------------------------------------------------------

export default function AnalyticsPage() {
  const [stats, setStats] = useState<NetworkStatsResponse | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadStats = useCallback(async () => {
    try {
      setError(null);
      const data = await fetchNetworkStats();
      setStats(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load stats");
    } finally {
      setIsLoading(false);
    }
  }, []);

  // Initial load + auto-refresh every 60s
  useEffect(() => {
    loadStats();
    const interval = setInterval(loadStats, 60_000);
    return () => clearInterval(interval);
  }, [loadStats]);

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3 mb-6">
        <div>
          <h1 className="text-2xl font-bold text-surface-900">
            Network Analytics
          </h1>
          <p className="text-sm text-surface-800/50 mt-0.5">
            Live on-chain metrics from the Xergon network
          </p>
        </div>
        {stats && (
          <ErgPriceTicker price={stats.ergPriceUsd} degraded={stats.degraded} />
        )}
      </div>

      <SuspenseWrap fallback={<AnalyticsSkeleton />}>
      {/* Degraded banner */}
      {stats?.degraded && <DegradedBanner />}

      {/* Error state */}
      {error && !isLoading && (
        <div className="mb-6 rounded-lg border border-danger-200 bg-danger-50 dark:border-danger-800/40 dark:bg-danger-950/20 px-4 py-3 text-sm text-danger-600 dark:text-danger-400">
          {error}
        </div>
      )}

      {/* Stats hero */}
      <div className="mb-6">
        {isLoading ? (
          <StatsSkeleton />
        ) : stats ? (
          <ErrorBoundary context="Stats Hero">
            <StatsHero stats={stats} />
          </ErrorBoundary>
        ) : null}
      </div>

      {/* Main content: chart + sidebar */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6 mb-6">
        {/* Requests chart (takes 2/3 width on desktop) */}
        <div className="lg:col-span-2">
          {isLoading ? (
            <ChartSkeleton />
          ) : stats ? (
            <ErrorBoundary context="Requests Chart">
              <RequestsChart data={stats.requestsOverTime} />
            </ErrorBoundary>
          ) : null}
        </div>

        {/* Sidebar: uptime + regions */}
        <div className="space-y-6">
          {isLoading ? (
            <>
              <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
                <SkeletonPulse className="h-5 w-28 mb-3 mx-auto" />
                <SkeletonPulse className="h-24 w-24 rounded-full mx-auto" />
              </div>
              <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
                <SkeletonPulse className="h-5 w-36 mb-3" />
                <div className="space-y-3">
                  {Array.from({ length: 4 }).map((_, i) => (
                    <div key={i}>
                      <SkeletonPulse className="h-4 w-20 mb-1" />
                      <SkeletonPulse className="h-2 w-full rounded-full" />
                    </div>
                  ))}
                </div>
              </div>
            </>
          ) : stats ? (
            <>
              <ErrorBoundary context="Network Uptime">
                <NetworkUptime uptime={stats.networkUptime} />
              </ErrorBoundary>
              <ErrorBoundary context="Region Distribution">
                <RegionDistribution regions={stats.providersByRegion} />
              </ErrorBoundary>
            </>
          ) : null}
        </div>
      </div>

      {/* Top models table */}
      <div className="mb-6">
        {isLoading ? (
          <TableSkeleton />
        ) : stats ? (
          <ErrorBoundary context="Top Models Table">
            <TopModelsTable models={stats.topModels} />
          </ErrorBoundary>
        ) : null}
      </div>

      {/* Footer */}
      {!isLoading && stats && (
        <div className="text-xs text-surface-800/30 text-center">
          Data refreshes every 60 seconds.
          {stats.degraded && (
            <span className="block mt-1 text-amber-500/60">
              Showing estimated data — relay is offline.
            </span>
          )}
        </div>
      )}
      </SuspenseWrap>
    </div>
  );
}
