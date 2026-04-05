"use client";

import { useState, useEffect, useCallback } from "react";
import { fetchHealthSummary, type HealthSummary } from "@/lib/api/health";
import { StatusBanner } from "@/components/health/StatusBanner";
import { ServiceCard } from "@/components/health/ServiceCard";
import { ProviderDistribution } from "@/components/health/ProviderDistribution";
import { UptimeBar } from "@/components/health/UptimeBar";
import { HealthSkeleton } from "@/components/health/HealthSkeleton";
import { SuspenseWrap } from "@/components/ui/SuspenseWrap";
import { ErrorBoundary } from "@/components/ui/ErrorBoundary";

// ---------------------------------------------------------------------------
// Skeleton loaders
// ---------------------------------------------------------------------------

function SkeletonPulse({ className }: { className?: string }) {
  return <div className={`skeleton-shimmer rounded-lg ${className ?? ""}`} />;
}

function BannerSkeleton() {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 px-5 py-4 flex items-center justify-between">
      <div className="flex items-center gap-3">
        <SkeletonPulse className="h-3 w-3 rounded-full" />
        <SkeletonPulse className="h-5 w-48" />
      </div>
      <SkeletonPulse className="h-4 w-20" />
    </div>
  );
}

function CardSkeleton() {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
      <div className="flex items-center justify-between mb-3">
        <SkeletonPulse className="h-4 w-28" />
        <SkeletonPulse className="h-5 w-20 rounded-full" />
      </div>
      <div className="space-y-2">
        <SkeletonPulse className="h-4 w-full" />
        <SkeletonPulse className="h-4 w-3/4" />
        <SkeletonPulse className="h-4 w-1/2" />
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Mock incidents
// ---------------------------------------------------------------------------

interface Incident {
  id: string;
  title: string;
  severity: "resolved" | "monitoring" | "investigating";
  time: string;
  description: string;
}

const MOCK_INCIDENTS: Incident[] = [
  {
    id: "1",
    title: "Relay latency spike",
    severity: "resolved",
    time: "2 hours ago",
    description: "Temporary latency increase due to high request volume. Resolved automatically.",
  },
  {
    id: "2",
    title: "Provider churn increase",
    severity: "monitoring",
    time: "6 hours ago",
    description: "Elevated provider disconnect rate in EU region. Monitoring recovery.",
  },
  {
    id: "3",
    title: "Oracle rate refresh delay",
    severity: "resolved",
    time: "1 day ago",
    description: "Oracle pool rate updates delayed by ~30s. Root cause: Ergo node sync lag.",
  },
];

function incidentSeverityColor(severity: Incident["severity"]): string {
  switch (severity) {
    case "resolved":
      return "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400";
    case "monitoring":
      return "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400";
    case "investigating":
      return "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400";
  }
}

// ---------------------------------------------------------------------------
// Main page
// ---------------------------------------------------------------------------

export default function HealthPage() {
  const [health, setHealth] = useState<HealthSummary | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const loadHealth = useCallback(async () => {
    try {
      setError(null);
      const data = await fetchHealthSummary();
      setHealth(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load health data");
    } finally {
      setIsLoading(false);
    }
  }, []);

  // Initial load + auto-refresh every 30s
  useEffect(() => {
    loadHealth();
    const interval = setInterval(loadHealth, 30_000);
    return () => clearInterval(interval);
  }, [loadHealth]);

  // Generate mock 7-day uptime data for each service
  const getDailyUptime = useCallback(
    (baseUptime: number) => {
      const days: number[] = [];
      for (let i = 0; i < 7; i++) {
        // Slight random variation around base uptime
        const variation = (Math.random() - 0.5) * 2;
        days.push(Math.min(100, Math.max(0, baseUptime + variation)));
      }
      // Today = base uptime
      days[6] = baseUptime;
      return days;
    },
    [],
  );

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-surface-900">
          Network Health
        </h1>
        <p className="text-sm text-surface-800/50 mt-0.5">
          Real-time status of Xergon infrastructure
        </p>
      </div>

      <SuspenseWrap fallback={<HealthSkeleton />}>
      {/* Status banner */}
      {isLoading ? (
        <BannerSkeleton />
      ) : health ? (
        <ErrorBoundary context="Status Banner">
          <StatusBanner
            overall={health.overall}
            lastUpdated={health.services[0]?.lastCheck ?? new Date().toISOString()}
          />
        </ErrorBoundary>
      ) : null}

      {/* Error state */}
      {error && !isLoading && (
        <div className="mt-4 rounded-lg border border-danger-200 bg-danger-50 dark:border-danger-800/40 dark:bg-danger-950/20 px-4 py-3 text-sm text-danger-600 dark:text-danger-400">
          {error}
        </div>
      )}

      {/* Service status cards */}
      <div className="mt-6">
        {isLoading ? (
          <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
            {Array.from({ length: 6 }).map((_, i) => (
              <CardSkeleton key={i} />
            ))}
          </div>
        ) : health ? (
          <ErrorBoundary context="Service Cards">
            <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
              {health.services.map((service) => (
                <ServiceCard key={service.name} service={service} />
              ))}
            </div>
          </ErrorBoundary>
        ) : null}
      </div>

      {/* Chain info strip */}
      {!isLoading && health && (
        <ErrorBoundary context="Chain Info">
          <div className="mt-6 rounded-xl border border-surface-200 bg-surface-0 p-5">
            <h2 className="text-base font-semibold text-surface-900 mb-3">
              Chain Status
            </h2>
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
              <div>
                <span className="text-xs text-surface-800/50 block mb-1">Block Height</span>
                <span className="text-sm font-semibold text-surface-900">
                  {health.chainHeight > 0
                    ? health.chainHeight.toLocaleString()
                    : "--"}
                </span>
              </div>
              <div>
                <span className="text-xs text-surface-800/50 block mb-1">Best Known Height</span>
                <span className="text-sm font-semibold text-surface-900">
                  {health.bestHeight > 0
                    ? health.bestHeight.toLocaleString()
                    : "--"}
                </span>
              </div>
              <div>
                <span className="text-xs text-surface-800/50 block mb-1">Sync Status</span>
                <span className="text-sm font-semibold text-surface-900 flex items-center gap-1.5">
                  <span
                    className={`h-2 w-2 rounded-full ${
                      health.chainSynced ? "bg-emerald-500" : "bg-amber-500"
                    }`}
                  />
                  {health.chainSynced ? "Synced" : "Syncing..."}
                </span>
              </div>
              <div>
                <span className="text-xs text-surface-800/50 block mb-1">Oracle Rate</span>
                <span className="text-sm font-semibold text-surface-900">
                  {health.oracleRate !== null
                    ? `$${health.oracleRate.toFixed(2)}`
                    : "N/A"}
                </span>
              </div>
            </div>
          </div>
        </ErrorBoundary>
      )}

      {/* Provider distribution + Incidents */}
      <div className="mt-6 grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Provider distribution */}
        <div className="lg:col-span-1">
          {isLoading ? (
            <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
              <SkeletonPulse className="h-5 w-36 mb-4 mx-auto" />
              <SkeletonPulse className="h-28 w-28 rounded-full mx-auto" />
              <div className="flex justify-center gap-4 mt-4">
                <SkeletonPulse className="h-4 w-16" />
                <SkeletonPulse className="h-4 w-16" />
                <SkeletonPulse className="h-4 w-16" />
              </div>
            </div>
          ) : health ? (
            <ErrorBoundary context="Provider Distribution">
              <ProviderDistribution
                online={health.providerDistribution.online}
                degraded={health.providerDistribution.degraded}
                offline={health.providerDistribution.offline}
                total={health.providerDistribution.total}
              />
            </ErrorBoundary>
          ) : null}
        </div>

        {/* Recent incidents */}
        <div className="lg:col-span-2">
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-5">
            <h2 className="text-base font-semibold text-surface-900 mb-3">
              Recent Incidents
            </h2>
            <div className="space-y-3">
              {MOCK_INCIDENTS.map((incident) => (
                <div
                  key={incident.id}
                  className="flex items-start gap-3 p-3 rounded-lg border border-surface-100 hover:border-surface-200 transition-colors"
                >
                  <div className="mt-0.5">
                    <span
                      className={`inline-flex items-center px-2 py-0.5 rounded-full text-[10px] font-medium ${incidentSeverityColor(incident.severity)}`}
                    >
                      {incident.severity}
                    </span>
                  </div>
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center justify-between gap-2">
                      <span className="text-sm font-medium text-surface-900">
                        {incident.title}
                      </span>
                      <span className="text-xs text-surface-800/40 whitespace-nowrap">
                        {incident.time}
                      </span>
                    </div>
                    <p className="text-xs text-surface-800/50 mt-0.5">
                      {incident.description}
                    </p>
                  </div>
                </div>
              ))}
            </div>
            <p className="text-[10px] text-surface-800/30 mt-3">
              Incident data is currently simulated. Real incident tracking coming soon.
            </p>
          </div>
        </div>
      </div>

      {/* 7-day uptime bars */}
      {!isLoading && health && (
        <ErrorBoundary context="Uptime Bars">
          <div className="mt-6 rounded-xl border border-surface-200 bg-surface-0 p-5">
            <h2 className="text-base font-semibold text-surface-900 mb-4">
              Uptime — Last 7 Days
            </h2>
            <div className="space-y-3">
              {health.services.map((service) => (
                <UptimeBar
                  key={service.name}
                  serviceName={service.name}
                  dailyUptime={getDailyUptime(service.uptime24h)}
                />
              ))}
            </div>
            <div className="flex items-center gap-4 mt-4 pt-3 border-t border-surface-100">
              <div className="flex items-center gap-1.5">
                <span className="h-2.5 w-2.5 rounded-sm bg-emerald-500" />
                <span className="text-[10px] text-surface-800/50">99%+</span>
              </div>
              <div className="flex items-center gap-1.5">
                <span className="h-2.5 w-2.5 rounded-sm bg-amber-500" />
                <span className="text-[10px] text-surface-800/50">95-99%</span>
              </div>
              <div className="flex items-center gap-1.5">
                <span className="h-2.5 w-2.5 rounded-sm bg-red-500" />
                <span className="text-[10px] text-surface-800/50">&lt;95%</span>
              </div>
            </div>
          </div>
        </ErrorBoundary>
      )}

      {/* Footer */}
      {!isLoading && health && (
        <div className="text-xs text-surface-800/30 text-center mt-6">
          Data refreshes every 30 seconds.
          {health.degraded && (
            <span className="block mt-1 text-amber-500/60">
              Showing degraded data — relay may be unreachable.
            </span>
          )}
        </div>
      )}
      </SuspenseWrap>
    </div>
  );
}
