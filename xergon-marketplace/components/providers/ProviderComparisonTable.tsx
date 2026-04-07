"use client";

import { useState, useEffect, useMemo, useCallback } from "react";
import { cn } from "@/lib/utils";

// ── Types ──

interface ProviderComparison {
  id: string;
  name: string;
  avatar: string;
  tier: string;
  status: "online" | "offline" | "maintenance";
  rating: number;
  reviewCount: number;
  metrics: {
    latencyAvg: number;
    latencyP50: number;
    latencyP95: number;
    throughput: number;
    reliability: number;
    uptime: number;
    costPer1M: number;
    supportedModels: number;
    regions: string[];
    apiResponseTime: number;
    errorRate: number;
  };
  features: {
    streaming: boolean;
    batching: boolean;
    embeddings: boolean;
    vision: boolean;
    functionCalling: boolean;
    jsonMode: boolean;
  };
}

// ── Helpers ──

function formatStars(rating: number): React.ReactNode[] {
  const stars: React.ReactNode[] = [];
  for (let i = 1; i <= 5; i++) {
    stars.push(
      <span
        key={i}
        className={cn(
          "text-sm",
          i <= Math.round(rating) ? "text-amber-400" : "text-surface-200",
        )}
      >
        ★
      </span>,
    );
  }
  return stars;
}

function determineWinner(
  providers: ProviderComparison[],
  metricKey: string,
): "lower" | "higher" | null {
  // For cost and error rate, lower is better; for everything else, higher is better
  const lowerBetter = ["costPer1M", "apiResponseTime", "errorRate", "latencyAvg", "latencyP50", "latencyP95"];
  return lowerBetter.includes(metricKey) ? "lower" : "higher";
}

function getWinnerId(
  providers: ProviderComparison[],
  metricKey: string,
): string | null {
  if (providers.length < 2) return null;
  const direction = determineWinner(providers, metricKey);
  const values = providers.map((p) => {
    const obj = p.metrics as Record<string, unknown>;
    return (obj[metricKey] as number) ?? 0;
  });
  const best = direction === "lower" ? Math.min(...values) : Math.max(...values);
  const idx = values.indexOf(best);
  return providers[idx]?.id ?? null;
}

const FEATURE_LABELS: Record<string, string> = {
  streaming: "Streaming",
  batching: "Batching",
  embeddings: "Embeddings",
  vision: "Vision",
  functionCalling: "Function Calling",
  jsonMode: "JSON Mode",
};

// ── Component ──

export default function ProviderComparisonTable() {
  const [allProviders, setAllProviders] = useState<ProviderComparison[]>([]);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [showPicker, setShowPicker] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [exported, setExported] = useState(false);

  // Fetch all providers
  useEffect(() => {
    fetch("/api/providers/compare")
      .then((r) => r.json())
      .then((data) => {
        setAllProviders(data.providers ?? []);
        // Default selection: first 2
        if (data.providers?.length >= 2) {
          setSelectedIds([data.providers[0].id, data.providers[1].id]);
        }
      })
      .catch(() => {})
      .finally(() => setIsLoading(false));
  }, []);

  const selectedProviders = useMemo(
    () => allProviders.filter((p) => selectedIds.includes(p.id)),
    [allProviders, selectedIds],
  );

  const toggleProvider = useCallback(
    (id: string) => {
      setSelectedIds((prev) => {
        if (prev.includes(id)) {
          return prev.filter((x) => x !== id);
        }
        if (prev.length >= 3) return prev;
        return [...prev, id];
      });
    },
    [],
  );

  const filteredPicker = useMemo(() => {
    const available = allProviders.filter((p) => !selectedIds.includes(p.id));
    if (!searchQuery.trim()) return available;
    const q = searchQuery.toLowerCase();
    return available.filter(
      (p) =>
        p.name.toLowerCase().includes(q) ||
        p.tier.toLowerCase().includes(q),
    );
  }, [allProviders, selectedIds, searchQuery]);

  const handleExport = useCallback(() => {
    if (selectedProviders.length === 0) return;
    const headers = [
      "Metric",
      ...selectedProviders.map((p) => p.name),
    ];
    const rows: string[][] = [
      ["Rating", ...selectedProviders.map((p) => `${p.rating}/5 (${p.reviewCount})`)],
      ["Tier", ...selectedProviders.map((p) => p.tier)],
      ["Status", ...selectedProviders.map((p) => p.status)],
      ["Latency Avg (ms)", ...selectedProviders.map((p) => String(p.metrics.latencyAvg))],
      ["Latency P50 (ms)", ...selectedProviders.map((p) => String(p.metrics.latencyP50))],
      ["Latency P95 (ms)", ...selectedProviders.map((p) => String(p.metrics.latencyP95))],
      ["Throughput (req/s)", ...selectedProviders.map((p) => String(p.metrics.throughput))],
      ["Reliability (%)", ...selectedProviders.map((p) => String(p.metrics.reliability))],
      ["Uptime (%)", ...selectedProviders.map((p) => String(p.metrics.uptime))],
      ["Cost per 1M tokens (ERG)", ...selectedProviders.map((p) => String(p.metrics.costPer1M))],
      ["Supported Models", ...selectedProviders.map((p) => String(p.metrics.supportedModels))],
      ["Regions", ...selectedProviders.map((p) => p.metrics.regions.join(", "))],
      ["API Response Time (ms)", ...selectedProviders.map((p) => String(p.metrics.apiResponseTime))],
      ["Error Rate (%)", ...selectedProviders.map((p) => String(p.metrics.errorRate))],
      ["Streaming", ...selectedProviders.map((p) => p.features.streaming ? "Yes" : "No")],
      ["Batching", ...selectedProviders.map((p) => p.features.batching ? "Yes" : "No")],
      ["Embeddings", ...selectedProviders.map((p) => p.features.embeddings ? "Yes" : "No")],
      ["Vision", ...selectedProviders.map((p) => p.features.vision ? "Yes" : "No")],
      ["Function Calling", ...selectedProviders.map((p) => p.features.functionCalling ? "Yes" : "No")],
      ["JSON Mode", ...selectedProviders.map((p) => p.features.jsonMode ? "Yes" : "No")],
    ];

    const csv = [headers.join(","), ...rows.map((r) => r.join(","))].join("\n");
    const blob = new Blob([csv], { type: "text/csv" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "provider-comparison.csv";
    a.click();
    URL.revokeObjectURL(url);
    setExported(true);
    setTimeout(() => setExported(false), 2000);
  }, [selectedProviders]);

  const handleShare = useCallback(() => {
    if (selectedProviders.length === 0) return;
    const ids = selectedProviders.map((p) => p.id).join(",");
    const url = `${window.location.origin}/compare/providers?providers=${ids}`;
    navigator.clipboard.writeText(url).then(() => {
      setExported(true);
      setTimeout(() => setExported(false), 2000);
    });
  }, [selectedProviders]);

  // ── Loading ──
  if (isLoading) {
    return (
      <div className="max-w-7xl mx-auto px-4 py-8">
        <div className="h-8 w-56 rounded-lg bg-surface-100 animate-pulse mb-2" />
        <div className="h-4 w-80 rounded bg-surface-50 animate-pulse mb-8" />
        <div className="grid grid-cols-3 gap-4 mb-8">
          {[1, 2, 3].map((i) => (
            <div key={i} className="h-24 rounded-xl bg-surface-50 animate-pulse" />
          ))}
        </div>
        <div className="space-y-4">
          {[1, 2, 3, 4, 5, 6].map((i) => (
            <div key={i} className="h-14 rounded-lg bg-surface-50 animate-pulse" />
          ))}
        </div>
      </div>
    );
  }

  const colWidth = selectedProviders.length > 0
    ? `grid-cols-${Math.min(selectedProviders.length + 1, 4)}`
    : "grid-cols-1";

  return (
    <div className="max-w-7xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 mb-8">
        <div>
          <h1 className="text-2xl font-bold text-surface-900 mb-1">
            Provider Comparison
          </h1>
          <p className="text-surface-800/60">
            Compare inference providers side-by-side across performance, pricing, and features.
          </p>
        </div>
        <div className="flex items-center gap-2">
          {selectedProviders.length > 0 && (
            <>
              <button
                onClick={handleExport}
                className="inline-flex items-center gap-1.5 px-4 py-2 rounded-lg border border-surface-200 bg-surface-0 text-sm text-surface-800/60 hover:text-surface-900 hover:bg-surface-50 transition-colors"
              >
                {exported ? "✓ Copied!" : "📥"} Export CSV
              </button>
              <button
                onClick={handleShare}
                className="inline-flex items-center gap-1.5 px-4 py-2 rounded-lg border border-surface-200 bg-surface-0 text-sm text-surface-800/60 hover:text-surface-900 hover:bg-surface-50 transition-colors"
              >
                {exported ? "✓ Copied!" : "🔗"} Share
              </button>
            </>
          )}
        </div>
      </div>

      {/* Provider Picker */}
      <div className="mb-8">
        <div className="flex flex-wrap items-center gap-3 mb-3">
          <span className="text-sm font-medium text-surface-900">
            Select providers (2-3):
          </span>
          {selectedProviders.map((p) => (
            <button
              key={p.id}
              onClick={() => toggleProvider(p.id)}
              className="inline-flex items-center gap-2 px-3 py-1.5 rounded-lg border border-brand-200 bg-brand-50 text-sm font-medium text-brand-700 hover:bg-brand-100 transition-colors"
            >
              <span>{p.avatar}</span>
              {p.name}
              <span className="text-brand-400 ml-0.5">✕</span>
            </button>
          ))}
          {selectedIds.length < 3 && (
            <div className="relative">
              <button
                onClick={() => setShowPicker(!showPicker)}
                className="inline-flex items-center gap-1 px-3 py-1.5 rounded-lg border border-dashed border-surface-300 text-sm text-surface-800/50 hover:border-brand-400 hover:text-brand-600 transition-colors"
              >
                + Add Provider
              </button>
              {showPicker && (
                <div className="absolute top-full left-0 mt-2 w-72 rounded-xl border border-surface-200 bg-surface-0 shadow-lg z-20 overflow-hidden">
                  <div className="p-2 border-b border-surface-100">
                    <input
                      type="text"
                      placeholder="Search providers..."
                      value={searchQuery}
                      onChange={(e) => setSearchQuery(e.target.value)}
                      className="w-full rounded-lg border border-surface-200 bg-surface-50 px-3 py-2 text-sm placeholder:text-surface-800/30 focus:border-brand-500 focus:outline-none"
                      autoFocus
                    />
                  </div>
                  <div className="max-h-60 overflow-y-auto">
                    {filteredPicker.length === 0 ? (
                      <p className="text-sm text-surface-800/40 p-3 text-center">
                        No providers available
                      </p>
                    ) : (
                      filteredPicker.map((p) => (
                        <button
                          key={p.id}
                          onClick={() => {
                            toggleProvider(p.id);
                            setShowPicker(false);
                            setSearchQuery("");
                          }}
                          className="w-full flex items-center gap-3 px-3 py-2.5 text-left hover:bg-surface-50 transition-colors"
                        >
                          <span className="text-lg">{p.avatar}</span>
                          <div>
                            <div className="text-sm font-medium text-surface-900">
                              {p.name}
                            </div>
                            <div className="text-xs text-surface-800/40">
                              {p.tier} · {p.metrics.supportedModels} models
                            </div>
                          </div>
                        </button>
                      ))
                    )}
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
      </div>

      {/* No selection state */}
      {selectedProviders.length < 2 && (
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-12 text-center">
          <p className="text-3xl mb-3">📊</p>
          <p className="text-surface-800/50 font-medium">
            Select at least 2 providers to compare
          </p>
          <p className="text-sm text-surface-800/40 mt-1">
            Click &quot;+ Add Provider&quot; above to get started.
          </p>
        </div>
      )}

      {/* Comparison Table */}
      {selectedProviders.length >= 2 && (
        <div className="space-y-8">
          {/* Provider Header Cards */}
          <div
            className={cn(
              "grid gap-4",
              selectedProviders.length === 2
                ? "grid-cols-2"
                : "grid-cols-3",
            )}
          >
            {selectedProviders.map((p) => (
              <div
                key={p.id}
                className={cn(
                  "rounded-xl border bg-surface-0 p-5 transition-all",
                  p.status === "online"
                    ? "border-emerald-200 bg-emerald-50/30"
                    : p.status === "maintenance"
                      ? "border-amber-200 bg-amber-50/30"
                      : "border-surface-200",
                )}
              >
                <div className="flex items-center gap-3 mb-3">
                  <span className="text-2xl">{p.avatar}</span>
                  <div>
                    <h3 className="font-semibold text-surface-900">{p.name}</h3>
                    <div className="flex items-center gap-2">
                      <span
                        className={cn(
                          "text-xs font-medium px-2 py-0.5 rounded",
                          p.tier === "Premium"
                            ? "bg-purple-50 text-purple-700"
                            : p.tier === "Standard"
                              ? "bg-blue-50 text-blue-700"
                              : "bg-surface-100 text-surface-800/60",
                        )}
                      >
                        {p.tier}
                      </span>
                      <span
                        className={cn(
                          "text-xs font-medium",
                          p.status === "online"
                            ? "text-emerald-600"
                            : p.status === "maintenance"
                              ? "text-amber-600"
                              : "text-red-600",
                        )}
                      >
                        ● {p.status.charAt(0).toUpperCase() + p.status.slice(1)}
                      </span>
                    </div>
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  <div className="flex">{formatStars(p.rating)}</div>
                  <span className="text-sm font-medium text-surface-900">
                    {p.rating}
                  </span>
                  <span className="text-xs text-surface-800/40">
                    ({p.reviewCount} reviews)
                  </span>
                </div>
              </div>
            ))}
          </div>

          {/* Performance Metrics Table */}
          <div>
            <h2 className="text-lg font-semibold text-surface-900 mb-4">
              Performance Metrics
            </h2>
            <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
              <table className="w-full">
                <thead>
                  <tr className="border-b border-surface-100 bg-surface-50/50">
                    <th className="text-left text-xs font-medium text-surface-800/50 px-5 py-3 w-48">
                      Metric
                    </th>
                    {selectedProviders.map((p) => (
                      <th
                        key={p.id}
                        className="text-center text-xs font-medium text-surface-800/50 px-4 py-3"
                      >
                        {p.name}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody className="divide-y divide-surface-100">
                  {/* Latency */}
                  <MetricRow
                    label="Avg Latency"
                    unit="ms"
                    providers={selectedProviders}
                    metricKey="latencyAvg"
                    lowerBetter
                  />
                  <MetricRow
                    label="P50 Latency"
                    unit="ms"
                    providers={selectedProviders}
                    metricKey="latencyP50"
                    lowerBetter
                  />
                  <MetricRow
                    label="P95 Latency"
                    unit="ms"
                    providers={selectedProviders}
                    metricKey="latencyP95"
                    lowerBetter
                  />
                  {/* Throughput */}
                  <MetricRow
                    label="Throughput"
                    unit="req/s"
                    providers={selectedProviders}
                    metricKey="throughput"
                  />
                  {/* Reliability */}
                  <MetricRow
                    label="Reliability"
                    unit="%"
                    providers={selectedProviders}
                    metricKey="reliability"
                  />
                  {/* Uptime */}
                  <MetricRow
                    label="Uptime"
                    unit="%"
                    providers={selectedProviders}
                    metricKey="uptime"
                  />
                  {/* Cost */}
                  <MetricRow
                    label="Cost per 1M tokens"
                    unit="ERG"
                    providers={selectedProviders}
                    metricKey="costPer1M"
                    lowerBetter
                  />
                  {/* Models */}
                  <MetricRow
                    label="Supported Models"
                    unit=""
                    providers={selectedProviders}
                    metricKey="supportedModels"
                    isInt
                  />
                  {/* API Response Time */}
                  <MetricRow
                    label="API Response Time"
                    unit="ms"
                    providers={selectedProviders}
                    metricKey="apiResponseTime"
                    lowerBetter
                  />
                  {/* Error Rate */}
                  <MetricRow
                    label="Error Rate"
                    unit="%"
                    providers={selectedProviders}
                    metricKey="errorRate"
                    lowerBetter
                  />
                  {/* Regions */}
                  <tr className="hover:bg-surface-50/50 transition-colors">
                    <td className="px-5 py-3 text-sm text-surface-800/60">
                      Regions
                    </td>
                    {selectedProviders.map((p) => {
                      const isWinner =
                        selectedProviders.length >= 2 &&
                        p.metrics.regions.length ===
                          Math.max(
                            ...selectedProviders.map(
                              (x) => x.metrics.regions.length,
                            ),
                          );
                      return (
                        <td
                          key={p.id}
                          className={cn(
                            "px-4 py-3 text-center text-sm",
                            isWinner && "bg-emerald-50/50 font-medium",
                          )}
                        >
                          <div className="flex flex-wrap items-center justify-center gap-1">
                            {p.metrics.regions.map((r) => (
                              <span
                                key={r}
                                className="inline-block text-xs bg-surface-100 text-surface-800/60 rounded px-1.5 py-0.5"
                              >
                                {r}
                              </span>
                            ))}
                          </div>
                          {isWinner && (
                            <span className="block text-xs text-emerald-600 mt-1">
                              ★ Most
                            </span>
                          )}
                        </td>
                      );
                    })}
                  </tr>
                </tbody>
              </table>
            </div>
          </div>

          {/* Feature Comparison */}
          <div>
            <h2 className="text-lg font-semibold text-surface-900 mb-4">
              Feature Comparison
            </h2>
            <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
              <table className="w-full">
                <thead>
                  <tr className="border-b border-surface-100 bg-surface-50/50">
                    <th className="text-left text-xs font-medium text-surface-800/50 px-5 py-3 w-48">
                      Feature
                    </th>
                    {selectedProviders.map((p) => (
                      <th
                        key={p.id}
                        className="text-center text-xs font-medium text-surface-800/50 px-4 py-3"
                      >
                        {p.name}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody className="divide-y divide-surface-100">
                  {Object.entries(FEATURE_LABELS).map(([key, label]) => (
                    <FeatureRow
                      key={key}
                      label={label}
                      providers={selectedProviders}
                      featureKey={key}
                    />
                  ))}
                </tbody>
              </table>
            </div>
          </div>

          {/* Footer note */}
          <p className="text-xs text-surface-800/30 text-center">
            Data represents average values over the past 30 days. Individual results may vary.
            Star ratings and review counts are based on user feedback.
          </p>
        </div>
      )}
    </div>
  );
}

// ── Sub-components ──

function MetricRow({
  label,
  unit,
  providers,
  metricKey,
  lowerBetter = false,
  isInt = false,
}: {
  label: string;
  unit: string;
  providers: ProviderComparison[];
  metricKey: string;
  lowerBetter?: boolean;
  isInt?: boolean;
}) {
  const winnerId = getWinnerId(providers, metricKey);

  return (
    <tr className="hover:bg-surface-50/50 transition-colors">
      <td className="px-5 py-3 text-sm text-surface-800/60">{label}</td>
      {providers.map((p) => {
        const value = (p.metrics as Record<string, unknown>)[metricKey] as number;
        const isWinner = p.id === winnerId;
        const display = isInt
          ? value.toString()
          : value % 1 === 0
            ? value.toString()
            : value.toFixed(2);

        return (
          <td
            key={p.id}
            className={cn(
              "px-4 py-3 text-center text-sm transition-colors",
              isWinner && "bg-emerald-50/50 font-medium text-emerald-700",
              !isWinner && "text-surface-900",
            )}
          >
            {display}
            {unit && (
              <span className="text-surface-800/40 ml-0.5">{unit}</span>
            )}
            {isWinner && (
              <span className="block text-xs text-emerald-600 mt-0.5">
                ★ Best
              </span>
            )}
          </td>
        );
      })}
    </tr>
  );
}

function FeatureRow({
  label,
  providers,
  featureKey,
}: {
  label: string;
  providers: ProviderComparison[];
  featureKey: string;
}) {
  const allSupported = providers.every(
    (p) => (p.features as Record<string, unknown>)[featureKey] === true,
  );

  return (
    <tr className="hover:bg-surface-50/50 transition-colors">
      <td className="px-5 py-3 text-sm text-surface-800/60">{label}</td>
      {providers.map((p) => {
        const supported = (p.features as Record<string, unknown>)[featureKey] as boolean;
        return (
          <td key={p.id} className="px-4 py-3 text-center">
            {supported ? (
              <span className="inline-flex items-center gap-1 text-sm font-medium text-emerald-600">
                <span className="text-base">✓</span> Supported
              </span>
            ) : (
              <span className="inline-flex items-center gap-1 text-sm text-surface-800/30">
                <span className="text-base">—</span> Not available
              </span>
            )}
          </td>
        );
      })}
    </tr>
  );
}
