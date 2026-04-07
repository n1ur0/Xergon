"use client";

import { Suspense, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useSearchParams, useRouter } from "next/navigation";
import {
  fetchProviders,
  filterProviders,
  extractModels,
  type ProviderInfo,
  type ProviderFilters,
} from "@/lib/api/providers";
import { ProviderCard } from "@/components/explorer/ProviderCard";
import { ProviderFiltersBar } from "@/components/explorer/ProviderFilters";
import { ProviderDetail } from "@/components/explorer/ProviderDetail";
import { ExplorerSkeleton } from "@/components/explorer/ExplorerSkeleton";
import { useProviderStatus } from "@/lib/hooks/useProviderStatus";
import { ProviderStatusIndicator } from "@/components/provider/ProviderStatusIndicator";

// ---------------------------------------------------------------------------
// Default filters
// ---------------------------------------------------------------------------

const DEFAULT_FILTERS: ProviderFilters = {
  search: "",
  region: "all",
  status: "all",
  model: "all",
  sortBy: "aiPoints",
  sortOrder: "desc",
};

// ---------------------------------------------------------------------------
// Skeleton loader
// ---------------------------------------------------------------------------

function ProviderCardSkeleton() {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-4 space-y-3 animate-pulse">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <div className="h-2.5 w-2.5 rounded-full bg-surface-200" />
          <div className="h-4 w-28 rounded bg-surface-200" />
        </div>
        <div className="h-5 w-16 rounded-full bg-surface-200" />
      </div>
      <div className="h-3 w-48 rounded bg-surface-200" />
      <div className="flex gap-1">
        <div className="h-5 w-20 rounded-md bg-surface-200" />
        <div className="h-5 w-24 rounded-md bg-surface-200" />
        <div className="h-5 w-16 rounded-md bg-surface-200" />
      </div>
      <div className="space-y-1">
        <div className="h-1.5 w-full rounded-full bg-surface-200" />
      </div>
      <div className="grid grid-cols-3 gap-2">
        <div className="h-12 rounded-lg bg-surface-200" />
        <div className="h-12 rounded-lg bg-surface-200" />
        <div className="h-12 rounded-lg bg-surface-200" />
      </div>
      <div className="h-3 w-full rounded bg-surface-200" />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Live connection indicator
// ---------------------------------------------------------------------------

function LiveIndicator({ isConnected }: { isConnected: boolean }) {
  return (
    <span className="inline-flex items-center gap-1.5 text-xs">
      {isConnected ? (
        <>
          <ProviderStatusIndicator status="online" size="sm" />
          <span className="text-green-600 font-medium">Live</span>
        </>
      ) : (
        <>
          <ProviderStatusIndicator status="offline" size="sm" />
          <span className="text-surface-800/50">Offline</span>
        </>
      )}
    </span>
  );
}

// ---------------------------------------------------------------------------
// Inner component (uses useSearchParams — must be inside Suspense)
// ---------------------------------------------------------------------------

function ExplorerContent() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const [allProviders, setAllProviders] = useState<ProviderInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  const refreshTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // WebSocket real-time status
  const { isConnected, providers: liveProviders } = useProviderStatus();

  // Build a lookup map for live status by provider endpoint/ID
  const liveStatusMap = useMemo(() => {
    const map = new Map<string, "online" | "offline" | "unknown">();
    for (const p of liveProviders) {
      // Match by providerId which could be the provider ID from agent status
      // or the endpoint URL
      map.set(p.providerId, p.status);
    }
    return map;
  }, [liveProviders]);

  // Parse URL params into filters
  const filters: ProviderFilters = useMemo(() => {
    return {
      search: searchParams.get("search") ?? DEFAULT_FILTERS.search,
      region: searchParams.get("region") ?? DEFAULT_FILTERS.region,
      status: searchParams.get("status") ?? DEFAULT_FILTERS.status,
      model: searchParams.get("model") ?? DEFAULT_FILTERS.model,
      sortBy: searchParams.get("sort") ?? DEFAULT_FILTERS.sortBy,
      sortOrder:
        (searchParams.get("order") as "asc" | "desc") ??
        DEFAULT_FILTERS.sortOrder,
    };
  }, [searchParams]);

  // Available models (from all providers)
  const availableModels = useMemo(() => extractModels(allProviders), [allProviders]);

  // Filtered + sorted providers
  const filteredProviders = useMemo(
    () => filterProviders(allProviders, filters),
    [allProviders, filters],
  );

  // Update URL params when filters change
  const updateFilters = useCallback(
    (newFilters: ProviderFilters) => {
      const params = new URLSearchParams();
      if (newFilters.search) params.set("search", newFilters.search);
      if (newFilters.region !== "all") params.set("region", newFilters.region);
      if (newFilters.status !== "all") params.set("status", newFilters.status);
      if (newFilters.model !== "all") params.set("model", newFilters.model);
      if (newFilters.sortBy !== DEFAULT_FILTERS.sortBy)
        params.set("sort", newFilters.sortBy);
      if (newFilters.sortOrder !== DEFAULT_FILTERS.sortOrder)
        params.set("order", newFilters.sortOrder);

      const qs = params.toString();
      router.push(`/explorer${qs ? `?${qs}` : ""}`, { scroll: false });
    },
    [router],
  );

  // Fetch providers
  const loadProviders = useCallback(async () => {
    try {
      setError(null);
      const res = await fetchProviders();
      setAllProviders(res.providers);
      setLastUpdated(new Date());
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Failed to load providers",
      );
    } finally {
      setLoading(false);
    }
  }, []);

  // Initial load + auto-refresh every 30s
  useEffect(() => {
    loadProviders();
    refreshTimerRef.current = setInterval(loadProviders, 30_000);
    return () => {
      if (refreshTimerRef.current) clearInterval(refreshTimerRef.current);
    };
  }, [loadProviders]);

  // Toggle expanded provider
  const handleToggle = useCallback(
    (endpoint: string) => {
      setExpandedId((prev) => (prev === endpoint ? null : endpoint));
    },
    [],
  );

  // Helper to resolve live status for a provider card
  const getProviderLiveStatus = useCallback(
    (provider: ProviderInfo): "online" | "offline" | "unknown" => {
      // Try matching by endpoint (the primary key in the relay registry)
      if (liveStatusMap.has(provider.endpoint)) {
        return liveStatusMap.get(provider.endpoint)!;
      }
      // When WS is connected but provider not in live map, show unknown
      if (isConnected) {
        return "unknown";
      }
      return "unknown";
    },
    [liveStatusMap, isConnected],
  );

  return (
    <main className="mx-auto max-w-6xl px-4 py-6 space-y-6">
      {/* Page header */}
      <div className="space-y-1">
        <div className="flex items-center gap-3">
          <h1 className="text-2xl font-bold text-surface-900">
            Provider Explorer
          </h1>
          <LiveIndicator isConnected={isConnected} />
        </div>
        <p className="text-sm text-surface-800/60">
          Browse all registered providers on the Xergon network
        </p>
      </div>

      {/* Filters */}
      <ProviderFiltersBar
        filters={filters}
        onChange={updateFilters}
        availableModels={availableModels}
      />

      {/* Status bar */}
      <div className="flex items-center justify-between text-xs text-surface-800/50">
        <span>
          {loading
            ? "Loading..."
            : `${filteredProviders.length} of ${allProviders.length} providers`}
        </span>
        {lastUpdated && !loading && (
          <span>
            Updated{" "}
            {lastUpdated.toLocaleTimeString([], {
              hour: "2-digit",
              minute: "2-digit",
            })}
          </span>
        )}
      </div>

      {/* Error state */}
      {error && !loading && (
        <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-sm text-red-700">
          <div className="flex items-center justify-between">
            <span>Error: {error}</span>
            <button
              type="button"
              onClick={loadProviders}
              className="text-xs font-medium underline hover:no-underline"
            >
              Retry
            </button>
          </div>
        </div>
      )}

      {/* Loading skeletons */}
      {loading && (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {Array.from({ length: 6 }, (_, i) => (
            <ProviderCardSkeleton key={i} />
          ))}
        </div>
      )}

      {/* Empty state */}
      {!loading && filteredProviders.length === 0 && !error && (
        <div className="flex flex-col items-center justify-center py-16 text-center space-y-3">
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="48"
            height="48"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
            className="text-surface-300"
          >
            <circle cx="11" cy="11" r="8" />
            <line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>
          <div>
            <p className="font-medium text-surface-800/70">
              No providers found
            </p>
            <p className="text-sm text-surface-800/50">
              Try adjusting your filters or search terms
            </p>
          </div>
          <button
            type="button"
            onClick={() => updateFilters(DEFAULT_FILTERS)}
            className="text-sm font-medium text-brand-600 hover:text-brand-700 transition-colors"
          >
            Clear all filters
          </button>
        </div>
      )}

      {/* Provider grid */}
      {!loading && filteredProviders.length > 0 && (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {filteredProviders.map((provider) => {
            const isExpanded = expandedId === provider.endpoint;
            const liveStatus = getProviderLiveStatus(provider);
            return (
              <div key={provider.endpoint}>
                <div className="relative">
                  <ProviderCard
                    provider={provider}
                    expanded={isExpanded}
                    onToggle={() => handleToggle(provider.endpoint)}
                  />
                  {/* Live status dot overlay */}
                  {isConnected && (
                    <div className="absolute top-4 right-4">
                      <ProviderStatusIndicator status={liveStatus} size="md" />
                    </div>
                  )}
                </div>
                {isExpanded && (
                  <div className="mt-0 rounded-b-xl border border-t-0 border-surface-200 bg-surface-0 p-4">
                    <ProviderDetail provider={provider} />
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </main>
  );
}

// ---------------------------------------------------------------------------
// Page component (wraps ExplorerContent in Suspense for useSearchParams)
// ---------------------------------------------------------------------------

export default function ExplorerPage() {
  return (
    <Suspense fallback={<ExplorerSkeleton />}>
      <ExplorerContent />
    </Suspense>
  );
}
