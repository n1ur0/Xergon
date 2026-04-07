"use client";

import { useState, useEffect, useMemo, useCallback } from "react";
import { useSearchParams, useRouter } from "next/navigation";
import Link from "next/link";
import {
  fetchModels,
  fetchProviders,
  nanoergToErg,
  type ChainModelInfo,
  type ProviderInfo,
} from "@/lib/api/chain";

// ── Helpers ──

function formatErg(nanoerg: number): string {
  if (nanoerg <= 0) return "0";
  const erg = nanoerg / 1e9;
  return erg.toFixed(6).replace(/0+$/, "").replace(/\.$/, "");
}

function formatPricePer1M(nanoergPerToken: number): string {
  if (nanoergPerToken <= 0) return "Free";
  const nanoergPer1M = nanoergPerToken * 1_000_000;
  return `${formatErg(nanoergPer1M)} ERG`;
}

/** Aggregate provider stats for a given model name. */
interface ModelStats {
  name: string;
  id: string;
  tier: string;
  providerCount: number;
  onlineCount: number;
  minPriceNanoerg: number;
  maxPriceNanoerg: number;
  avgLatencyMs: number | null;
  avgThroughput: number | null;
  availabilityPct: number;
  contextWindow: number | null;
  description?: string;
  tags?: string[];
}

function computeStats(
  model: ChainModelInfo,
  providers: ProviderInfo[],
): ModelStats {
  const modelProviders = providers.filter((p) =>
    p.models.some((m) => m === model.id || m === model.name),
  );
  const online = modelProviders.filter((p) => p.healthy || p.is_active);
  const total = modelProviders.length;

  const latencies = modelProviders
    .map((p) => p.latency_ms)
    .filter((l): l is number => l !== null && l > 0);
  const avgLatency =
    latencies.length > 0
      ? Math.round(latencies.reduce((a, b) => a + b, 0) / latencies.length)
      : null;

  // Throughput approximation: rough tokens/sec based on latency
  // This is a heuristic -- real throughput would come from the relay
  const avgThroughput =
    avgLatency !== null && avgLatency > 0
      ? Math.round((1000 / avgLatency) * 32) // rough estimate
      : null;

  const availability =
    total > 0 ? Math.round((online.length / total) * 100) : 0;

  const inputPrice = model.price_per_input_token_nanoerg;
  const outputPrice = model.price_per_output_token_nanoerg;
  const minPrice = Math.min(inputPrice, outputPrice);
  const maxPrice = Math.max(inputPrice, outputPrice);

  return {
    name: model.name,
    id: model.id,
    tier: model.tier,
    providerCount: total,
    onlineCount: online.length,
    minPriceNanoerg: minPrice,
    maxPriceNanoerg: maxPrice,
    avgLatencyMs: avgLatency,
    avgThroughput,
    availabilityPct: availability,
    contextWindow: model.context_window ?? null,
    description: model.description,
    tags: model.tags,
  };
}

// ── Popular models for default selection ──

const POPULAR_MODEL_IDS = [
  "llama-3.1-70b",
  "qwen-2.5-72b",
  "mistral-large",
  "llama-3.3-70b",
  "deepseek-coder-33b",
  "phi-3-medium",
  "gemma-2-27b",
];

// ── Component ──

export default function ComparePage() {
  const searchParams = useSearchParams();
  const router = useRouter();

  const [allModels, setAllModels] = useState<ChainModelInfo[]>([]);
  const [providers, setProviders] = useState<ProviderInfo[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [searchQuery, setSearchQuery] = useState("");
  const [showDropdown, setShowDropdown] = useState(false);

  // Selected model IDs from URL params
  const selectedIds = useMemo(() => {
    const param = searchParams.get("models");
    if (!param) return [];
    return param
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
  }, [searchParams]);

  const updateSelected = useCallback(
    (ids: string[]) => {
      const params = new URLSearchParams(searchParams.toString());
      if (ids.length > 0) {
        params.set("models", ids.join(","));
      } else {
        params.delete("models");
      }
      router.push(`/compare?${params.toString()}`, { scroll: false });
    },
    [searchParams, router],
  );

  const removeModel = useCallback(
    (id: string) => {
      updateSelected(selectedIds.filter((m) => m !== id));
    },
    [selectedIds, updateSelected],
  );

  const addModel = useCallback(
    (id: string) => {
      if (!selectedIds.includes(id) && selectedIds.length < 4) {
        updateSelected([...selectedIds, id]);
        setSearchQuery("");
        setShowDropdown(false);
      }
    },
    [selectedIds, updateSelected],
  );

  // Fetch data
  useEffect(() => {
    Promise.all([fetchModels(), fetchProviders()])
      .then(([models, provs]) => {
        setAllModels(models);
        setProviders(provs);
        setIsLoading(false);
      })
      .catch(() => setIsLoading(false));
  }, []);

  // Compute stats for selected models
  const selectedStats = useMemo(() => {
    return selectedIds
      .map((id) => {
        const model = allModels.find(
          (m) => m.id === id || m.name === id,
        );
        if (!model) return null;
        return computeStats(model, providers);
      })
      .filter((s): s is ModelStats => s !== null);
  }, [selectedIds, allModels, providers]);

  // Filter models for dropdown
  const filteredModels = useMemo(() => {
    if (!searchQuery.trim()) return allModels;
    const q = searchQuery.toLowerCase();
    return allModels.filter(
      (m) =>
        m.name.toLowerCase().includes(q) ||
        m.id.toLowerCase().includes(q) ||
        m.tier.toLowerCase().includes(q),
    );
  }, [allModels, searchQuery]);

  // ── Empty state: show model picker ──
  if (!isLoading && selectedIds.length === 0) {
    return (
      <div className="max-w-4xl mx-auto px-4 py-8">
        <h1 className="text-2xl font-bold mb-2">Compare Models</h1>
        <p className="text-surface-800/60 mb-8">
          Select models to compare side-by-side.
        </p>

        <div className="rounded-xl border border-surface-200 bg-surface-0 p-6">
          <h2 className="text-lg font-semibold mb-4">Choose Models to Compare</h2>

          {allModels.length > 0 ? (
            <div className="grid gap-3 sm:grid-cols-2">
              {allModels.slice(0, 10).map((model) => (
                <button
                  key={model.id}
                  onClick={() => addModel(model.id)}
                  className="flex items-center justify-between rounded-lg border border-surface-200 bg-surface-50 px-4 py-3 text-left hover:border-brand-500 hover:bg-brand-50/50 transition-colors"
                >
                  <div>
                    <div className="font-medium text-surface-900">
                      {model.name}
                    </div>
                    <div className="text-xs text-surface-800/40">
                      {model.tier}
                      {model.provider_count != null
                        ? ` · ${model.provider_count} providers`
                        : ""}
                    </div>
                  </div>
                  <span className="text-brand-600 text-sm font-medium">
                    + Add
                  </span>
                </button>
              ))}
            </div>
          ) : (
            <p className="text-sm text-surface-800/50">
              No models available right now.
            </p>
          )}

          {allModels.length > 10 && (
            <div className="mt-4">
              <p className="text-sm text-surface-800/40 mb-2">
                Or search for a specific model:
              </p>
              <ModelSearchInput
                models={allModels}
                onSelect={addModel}
                searchQuery={searchQuery}
                setSearchQuery={setSearchQuery}
                showDropdown={showDropdown}
                setShowDropdown={setShowDropdown}
              />
            </div>
          )}
        </div>
      </div>
    );
  }

  // ── Loading state ──
  if (isLoading) {
    return (
      <div className="max-w-6xl mx-auto px-4 py-8">
        <h1 className="text-2xl font-bold mb-8">Compare Models</h1>
        <div className="grid gap-6 md:grid-cols-2 lg:grid-cols-3">
          {[1, 2, 3].map((i) => (
            <div
              key={i}
              className="rounded-xl border border-surface-200 bg-surface-0 p-5 animate-pulse"
            >
              <div className="h-5 w-40 rounded bg-surface-100 mb-3" />
              <div className="h-3 w-24 rounded bg-surface-100 mb-6" />
              {[1, 2, 3, 4, 5].map((j) => (
                <div key={j} className="h-4 w-full rounded bg-surface-50 mb-2" />
              ))}
            </div>
          ))}
        </div>
      </div>
    );
  }

  const colCount = Math.min(selectedStats.length, 3);

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 mb-8">
        <div>
          <h1 className="text-2xl font-bold mb-1">Compare Models</h1>
          <p className="text-surface-800/60">
            Side-by-side comparison of {selectedStats.length} model
            {selectedStats.length !== 1 ? "s" : ""} across pricing, performance, and availability.
          </p>
        </div>

        {/* Add model controls */}
        <div className="flex items-center gap-2">
          <ModelSearchInput
            models={filteredModels}
            onSelect={addModel}
            searchQuery={searchQuery}
            setSearchQuery={setSearchQuery}
            showDropdown={showDropdown}
            setShowDropdown={setShowDropdown}
            disabled={selectedIds.length >= 4}
          />
          <Link
            href="/compare"
            className="inline-flex items-center gap-1 rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm text-surface-800/60 hover:text-surface-900 transition-colors"
          >
            Clear
          </Link>
        </div>
      </div>

      {/* Comparison grid */}
      {selectedStats.length > 0 ? (
        <div className={`grid gap-6 ${colCount === 1 ? 'grid-cols-1' : colCount === 2 ? 'md:grid-cols-2' : 'lg:grid-cols-3'}`}>
          {selectedStats.map((stats) => (
            <ModelComparisonCard
              key={stats.id}
              stats={stats}
              onRemove={() => removeModel(stats.id)}
            />
          ))}
        </div>
      ) : (
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-8 text-center">
          <p className="text-surface-800/50 mb-2">
            No matching models found. Try a different search or clear filters.
          </p>
          <Link
            href="/compare"
            className="text-brand-600 text-sm hover:underline"
          >
            Reset comparison
          </Link>
        </div>
      )}

      {/* Legend */}
      <div className="mt-8 text-sm text-surface-800/40">
        Data is live from the Xergon relay. Prices in ERG per 1M tokens.
      </div>
    </div>
  );
}

// ── Sub-components ──

function ModelComparisonCard({
  stats,
  onRemove,
}: {
  stats: ModelStats;
  onRemove: () => void;
}) {
  const priceRange =
    stats.minPriceNanoerg <= 0 && stats.maxPriceNanoerg <= 0
      ? "Free"
      : stats.minPriceNanoerg === stats.maxPriceNanoerg
        ? formatPricePer1M(stats.minPriceNanoerg)
        : `${formatPricePer1M(stats.minPriceNanoerg)} – ${formatPricePer1M(stats.maxPriceNanoerg)}`;

  const availabilityColor =
    stats.availabilityPct >= 80
      ? "text-emerald-600"
      : stats.availabilityPct >= 50
        ? "text-amber-600"
        : "text-red-600";

  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 relative">
      {/* Remove button */}
      <button
        onClick={onRemove}
        className="absolute top-3 right-3 text-surface-800/30 hover:text-red-500 transition-colors text-lg leading-none"
        title="Remove from comparison"
      >
        ×
      </button>

      {/* Model name */}
      <h3 className="font-semibold text-surface-900 text-lg pr-6">
        {stats.name}
      </h3>
      <div className="flex items-center gap-2 mt-1 mb-5">
        <span className="text-xs text-surface-800/40 bg-surface-100 rounded px-1.5 py-0.5">
          {stats.tier}
        </span>
        {stats.tags?.slice(0, 2).map((tag) => (
          <span
            key={tag}
            className="text-xs text-brand-600 bg-brand-50 rounded px-1.5 py-0.5"
          >
            {tag}
          </span>
        ))}
      </div>

      {/* Stats */}
      <div className="space-y-3">
        <StatRow label="Providers" value={`${stats.onlineCount} / ${stats.providerCount} online`} />

        <StatRow label="Price (1M tokens)" value={priceRange} />

        <StatRow
          label="Avg Latency"
          value={stats.avgLatencyMs !== null ? `${stats.avgLatencyMs} ms` : "N/A"}
        />

        <StatRow
          label="Throughput"
          value={stats.avgThroughput !== null ? `~${stats.avgThroughput} tok/s` : "N/A"}
        />

        <StatRow
          label="Availability"
          value={
            <span className={availabilityColor}>
              {stats.availabilityPct}%
            </span>
          }
        />

        <StatRow
          label="Context Window"
          value={
            stats.contextWindow
              ? stats.contextWindow >= 1000
                ? `${(stats.contextWindow / 1000).toFixed(0)}K`
                : String(stats.contextWindow)
              : "N/A"
          }
        />
      </div>

      {/* Availability bar */}
      <div className="mt-4">
        <div className="h-1.5 bg-surface-100 rounded-full overflow-hidden">
          <div
            className={`h-full rounded-full transition-all ${
              stats.availabilityPct >= 80
                ? "bg-emerald-500"
                : stats.availabilityPct >= 50
                  ? "bg-amber-500"
                  : "bg-red-500"
            }`}
            style={{ width: `${stats.availabilityPct}%` }}
          />
        </div>
      </div>
    </div>
  );
}

function StatRow({
  label,
  value,
}: {
  label: string;
  value: React.ReactNode;
}) {
  return (
    <div className="flex items-center justify-between text-sm">
      <span className="text-surface-800/50">{label}</span>
      <span className="font-medium text-surface-900">{value}</span>
    </div>
  );
}

function ModelSearchInput({
  models,
  onSelect,
  searchQuery,
  setSearchQuery,
  showDropdown,
  setShowDropdown,
  disabled,
}: {
  models: ChainModelInfo[];
  onSelect: (id: string) => void;
  searchQuery: string;
  setSearchQuery: (q: string) => void;
  showDropdown: boolean;
  setShowDropdown: (v: boolean) => void;
  disabled?: boolean;
}) {
  const alreadySelected = new Set<string>(); // Could pass selectedIds if needed

  return (
    <div className="relative">
      <input
        type="text"
        placeholder="Search models..."
        value={searchQuery}
        onChange={(e) => {
          setSearchQuery(e.target.value);
          setShowDropdown(true);
        }}
        onFocus={() => setShowDropdown(true)}
        disabled={disabled}
        className="rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm w-48 focus:outline-none focus:border-brand-500 disabled:opacity-50"
      />
      {showDropdown && searchQuery.trim() && (
        <div className="absolute z-10 top-full mt-1 left-0 w-64 max-h-60 overflow-y-auto rounded-lg border border-surface-200 bg-surface-0 shadow-lg">
          {models.slice(0, 8).map((model) => (
            <button
              key={model.id}
              onClick={() => onSelect(model.id)}
              className="w-full text-left px-3 py-2 text-sm hover:bg-brand-50 transition-colors flex items-center justify-between"
            >
              <div>
                <div className="font-medium text-surface-900">
                  {model.name}
                </div>
                <div className="text-xs text-surface-800/40">
                  {model.tier}
                </div>
              </div>
              <span className="text-brand-600 text-xs">+ Add</span>
            </button>
          ))}
          {models.length === 0 && (
            <div className="px-3 py-2 text-sm text-surface-800/40">
              No models found
            </div>
          )}
        </div>
      )}
      {/* Click outside to close */}
      {showDropdown && (
        <div
          className="fixed inset-0 z-[5]"
          onClick={() => setShowDropdown(false)}
        />
      )}
    </div>
  );
}
