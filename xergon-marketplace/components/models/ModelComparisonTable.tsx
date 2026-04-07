"use client";

import { useState, useMemo, useCallback, useEffect, useRef } from "react";
import { useSearchParams, useRouter } from "next/navigation";
import { cn } from "@/lib/utils";
import Link from "next/link";
import {
  fetchModels,
  fetchProviders,
  nanoergToErg,
  type ChainModelInfo,
  type ProviderInfo,
} from "@/lib/api/chain";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface ModelStats {
  id: string;
  name: string;
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

type SortField =
  | "name"
  | "price"
  | "latency"
  | "throughput"
  | "availability"
  | "contextWindow"
  | "providers";
type SortDir = "asc" | "desc";

const STORAGE_KEY = "xergon-model-comparison-ids";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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

function computeStats(model: ChainModelInfo, providers: ProviderInfo[]): ModelStats {
  const modelProviders = providers.filter((p) =>
    p.models.some((m) => m === model.id || m === model.name),
  );
  const online = modelProviders.filter((p) => p.healthy || p.is_active);
  const total = modelProviders.length;

  const latencies = modelProviders
    .map((p) => p.latency_ms)
    .filter((l): l is number => l !== null && l > 0);
  const avgLatency =
    latencies.length > 0 ? Math.round(latencies.reduce((a, b) => a + b, 0) / latencies.length) : null;

  const avgThroughput =
    avgLatency !== null && avgLatency > 0 ? Math.round((1000 / avgLatency) * 32) : null;

  const availability = total > 0 ? Math.round((online.length / total) * 100) : 0;

  return {
    id: model.id,
    name: model.name,
    tier: model.tier,
    providerCount: total,
    onlineCount: online.length,
    minPriceNanoerg: Math.min(model.price_per_input_token_nanoerg, model.price_per_output_token_nanoerg),
    maxPriceNanoerg: Math.max(model.price_per_input_token_nanoerg, model.price_per_output_token_nanoerg),
    avgLatencyMs: avgLatency,
    avgThroughput,
    availabilityPct: availability,
    contextWindow: model.context_window ?? null,
    description: model.description,
    tags: model.tags,
  };
}

function loadSavedIds(): string[] {
  if (typeof window === "undefined") return [];
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

function saveIds(ids: string[]) {
  if (typeof window === "undefined") return;
  localStorage.setItem(STORAGE_KEY, JSON.stringify(ids));
}

/** Determine the best model for a given field. Returns the model id of the winner. */
function findWinner(models: ModelStats[], field: SortField): string | null {
  if (models.length < 2) return null;
  const valid = models.filter((m) => {
    switch (field) {
      case "price":
        return m.minPriceNanoerg > 0;
      case "latency":
        return m.avgLatencyMs !== null;
      case "throughput":
        return m.avgThroughput !== null;
      case "availability":
        return m.availabilityPct > 0;
      case "contextWindow":
        return m.contextWindow !== null;
      case "providers":
        return m.providerCount > 0;
      default:
        return true;
    }
  });
  if (valid.length < 2) return null;

  const sorted = [...valid].sort((a, b) => {
    switch (field) {
      case "price":
        return a.minPriceNanoerg - b.minPriceNanoerg; // Lower is better
      case "latency":
        return (a.avgLatencyMs ?? Infinity) - (b.avgLatencyMs ?? Infinity);
      case "throughput":
        return (b.avgThroughput ?? 0) - (a.avgThroughput ?? 0); // Higher is better
      case "availability":
        return b.availabilityPct - a.availabilityPct;
      case "contextWindow":
        return (b.contextWindow ?? 0) - (a.contextWindow ?? 0);
      case "providers":
        return b.providerCount - a.providerCount;
      default:
        return 0;
    }
  });
  return sorted[0].id;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export function ModelComparisonTable() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const [allModels, setAllModels] = useState<ChainModelInfo[]>([]);
  const [providers, setProviders] = useState<ProviderInfo[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [searchQuery, setSearchQuery] = useState("");
  const [showDropdown, setShowDropdown] = useState(false);
  const [sortField, setSortField] = useState<SortField>("name");
  const [sortDir, setSortDir] = useState<SortDir>("asc");
  const dropdownRef = useRef<HTMLDivElement>(null);

  // Selected IDs from URL or localStorage
  const selectedIds = useMemo(() => {
    const param = searchParams.get("models");
    if (param) {
      return param.split(",").map((s) => s.trim()).filter(Boolean).slice(0, 4);
    }
    return loadSavedIds().slice(0, 4);
  }, [searchParams]);

  // Persist selections
  useEffect(() => {
    saveIds(selectedIds);
  }, [selectedIds]);

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

  const removeModel = useCallback(
    (id: string) => {
      updateSelected(selectedIds.filter((m) => m !== id));
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

  // Compute stats
  const selectedStats = useMemo(() => {
    return selectedIds
      .map((id) => {
        const model = allModels.find((m) => m.id === id || m.name === id);
        if (!model) return null;
        return computeStats(model, providers);
      })
      .filter((s): s is ModelStats => s !== null);
  }, [selectedIds, allModels, providers]);

  // Filter for dropdown
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

  // Sort stats
  const sortedStats = useMemo(() => {
    return [...selectedStats].sort((a, b) => {
      let cmp = 0;
      switch (sortField) {
        case "name":
          cmp = a.name.localeCompare(b.name);
          break;
        case "price":
          cmp = a.minPriceNanoerg - b.minPriceNanoerg;
          break;
        case "latency":
          cmp = (a.avgLatencyMs ?? Infinity) - (b.avgLatencyMs ?? Infinity);
          break;
        case "throughput":
          cmp = (a.avgThroughput ?? 0) - (b.avgThroughput ?? 0);
          break;
        case "availability":
          cmp = a.availabilityPct - b.availabilityPct;
          break;
        case "contextWindow":
          cmp = (a.contextWindow ?? 0) - (b.contextWindow ?? 0);
          break;
        case "providers":
          cmp = a.providerCount - b.providerCount;
          break;
      }
      return sortDir === "asc" ? cmp : -cmp;
    });
  }, [selectedStats, sortField, sortDir]);

  const handleSort = useCallback((field: SortField) => {
    setSortField((prev) => {
      if (prev === field) {
        setSortDir((d) => (d === "asc" ? "desc" : "asc"));
        return prev;
      }
      setSortDir("asc");
      return field;
    });
  }, []);

  // Close dropdown on outside click
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (dropdownRef.current && !dropdownRef.current.contains(e.target as Node)) {
        setShowDropdown(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  // Generate share URL
  const shareUrl = useMemo(() => {
    if (selectedIds.length === 0) return "";
    const params = new URLSearchParams({ models: selectedIds.join(",") });
    return `${typeof window !== "undefined" ? window.location.origin : ""}/compare?${params.toString()}`;
  }, [selectedIds]);

  const handleShare = useCallback(async () => {
    if (!shareUrl) return;
    try {
      await navigator.clipboard.writeText(shareUrl);
    } catch {
      // Fallback: focus an input
    }
  }, [shareUrl]);

  // Pre-compute winners for each field
  const winners = useMemo(() => {
    const fields: SortField[] = ["price", "latency", "throughput", "availability", "contextWindow", "providers"];
    const map: Record<string, string | null> = {};
    fields.forEach((f) => {
      map[f] = findWinner(selectedStats, f);
    });
    return map;
  }, [selectedStats]);

  // Sort icon
  function SortIcon({ field }: { field: SortField }) {
    if (sortField !== field) {
      return (
        <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="opacity-30 inline ml-1">
          <path d="m7 15 5 5 5-5" />
          <path d="m7 9 5-5 5 5" />
        </svg>
      );
    }
    return (
      <svg
        xmlns="http://www.w3.org/2000/svg"
        width="12"
        height="12"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        className={cn("inline ml-1", sortDir === "asc" ? "rotate-180" : "")}
      >
        <path d="m7 15 5 5 5-5" />
      </svg>
    );
  }

  // ---------------------------------------------------------------------------
  // Render
  // ---------------------------------------------------------------------------

  if (isLoading) {
    return (
      <div className="max-w-6xl mx-auto px-4 py-8">
        <h1 className="text-2xl font-bold mb-8">Model Comparison Table</h1>
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-5 animate-pulse">
          <div className="h-5 w-60 rounded bg-surface-100 mb-4" />
          {[1, 2, 3].map((i) => (
            <div key={i} className="h-12 w-full rounded bg-surface-50 mb-2" />
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 mb-6">
        <div>
          <h1 className="text-2xl font-bold mb-1">Model Comparison Table</h1>
          <p className="text-surface-800/60">
            Compare up to 4 models side-by-side.{" "}
            {selectedStats.length > 0 && `${selectedStats.length} model${selectedStats.length !== 1 ? "s" : ""} selected.`}
          </p>
        </div>
        <div className="flex items-center gap-2 flex-wrap">
          {/* Model search */}
          <div className="relative" ref={dropdownRef}>
            <input
              type="text"
              placeholder={selectedIds.length >= 4 ? "Max 4 models" : "Search models..."}
              value={searchQuery}
              onChange={(e) => {
                setSearchQuery(e.target.value);
                setShowDropdown(true);
              }}
              onFocus={() => setShowDropdown(true)}
              disabled={selectedIds.length >= 4}
              className="w-48 rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm placeholder:text-surface-800/30 focus:outline-none focus:ring-2 focus:ring-brand-500/40 focus:border-brand-500 disabled:opacity-50"
            />
            {showDropdown && filteredModels.length > 0 && (
              <div className="absolute top-full left-0 right-0 mt-1 rounded-lg border border-surface-200 bg-surface-0 shadow-lg z-20 max-h-60 overflow-y-auto">
                {filteredModels.slice(0, 20).map((m) => (
                  <button
                    key={m.id}
                    onClick={() => addModel(m.id)}
                    disabled={selectedIds.includes(m.id)}
                    className="block w-full text-left px-3 py-2 text-sm hover:bg-surface-50 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                  >
                    <span className="font-medium">{m.name}</span>
                    <span className="text-xs text-surface-800/40 ml-2">{m.tier}</span>
                    {selectedIds.includes(m.id) && (
                      <span className="text-xs text-brand-600 ml-2">Selected</span>
                    )}
                  </button>
                ))}
              </div>
            )}
          </div>

          {/* Clear */}
          <Link
            href="/compare"
            className="inline-flex items-center gap-1 rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm text-surface-800/60 hover:text-surface-900 transition-colors"
          >
            Clear
          </Link>

          {/* Share */}
          {shareUrl && (
            <button
              onClick={handleShare}
              className="inline-flex items-center gap-1 rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm text-surface-800/60 hover:text-surface-900 transition-colors"
              title="Copy share link"
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M4 12v8a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8" />
                <polyline points="16 6 12 2 8 6" />
                <line x1="12" y1="2" x2="12" y2="15" />
              </svg>
              Share
            </button>
          )}
        </div>
      </div>

      {/* Comparison Table */}
      {selectedStats.length === 0 ? (
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-8 text-center">
          <p className="text-surface-800/50 mb-4">No models selected. Use the search above to add models.</p>
          <div className="grid gap-3 sm:grid-cols-2 max-w-lg mx-auto">
            {allModels.slice(0, 6).map((m) => (
              <button
                key={m.id}
                onClick={() => addModel(m.id)}
                className="flex items-center justify-between rounded-lg border border-surface-200 bg-surface-50 px-4 py-3 text-left hover:border-brand-500 hover:bg-brand-50/50 transition-colors"
              >
                <div>
                  <div className="font-medium text-surface-900">{m.name}</div>
                  <div className="text-xs text-surface-800/40">{m.tier}</div>
                </div>
                <span className="text-brand-600 text-sm font-medium">+ Add</span>
              </button>
            ))}
          </div>
        </div>
      ) : (
        <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
          {/* Selected model chips */}
          <div className="px-4 py-3 border-b border-surface-200 flex items-center gap-2 flex-wrap">
            {sortedStats.map((m) => (
              <span
                key={m.id}
                className="inline-flex items-center gap-1.5 rounded-full bg-brand-50 border border-brand-200 px-3 py-1 text-sm font-medium text-brand-700"
              >
                {m.name}
                <button
                  onClick={() => removeModel(m.id)}
                  className="text-brand-400 hover:text-brand-700 transition-colors"
                  aria-label={`Remove ${m.name}`}
                >
                  <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <line x1="18" y1="6" x2="6" y2="18" />
                    <line x1="6" y1="6" x2="18" y2="18" />
                  </svg>
                </button>
              </span>
            ))}
          </div>

          {/* Table */}
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-surface-200 bg-surface-50">
                  <th
                    className="text-left px-4 py-3 font-medium text-surface-800/50 text-xs uppercase tracking-wide cursor-pointer hover:text-surface-800/70 transition-colors"
                    onClick={() => handleSort("name")}
                  >
                    Metric <SortIcon field="name" />
                  </th>
                  {sortedStats.map((m) => (
                    <th key={m.id} className="text-center px-4 py-3 font-medium text-surface-900 text-xs">
                      <Link href={`/models/${m.id}`} className="hover:text-brand-600 transition-colors">
                        {m.name}
                      </Link>
                      <div className="text-surface-800/40 font-normal mt-0.5">{m.tier}</div>
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {/* Price row */}
                <ComparisonRow
                  label="Price (1M tokens)"
                  onSort={() => handleSort("price")}
                  sortField={sortField}
                  sortDir={sortDir}
                  field="price"
                >
                  {sortedStats.map((m) => (
                    <td key={m.id} className={cn("px-4 py-3 text-center", winners.price === m.id && "bg-emerald-500/5")}>
                      <span className={cn(
                        "text-sm font-medium",
                        winners.price === m.id ? "text-emerald-600" : "text-surface-900"
                      )}>
                        {m.minPriceNanoerg <= 0 && m.maxPriceNanoerg <= 0
                          ? "Free"
                          : m.minPriceNanoerg === m.maxPriceNanoerg
                            ? formatPricePer1M(m.minPriceNanoerg)
                            : `${formatPricePer1M(m.minPriceNanoerg)} – ${formatPricePer1M(m.maxPriceNanoerg)}`}
                      </span>
                      {winners.price === m.id && (
                        <span className="ml-1 text-[10px] font-bold text-emerald-500">BEST</span>
                      )}
                    </td>
                  ))}
                </ComparisonRow>

                {/* Latency row */}
                <ComparisonRow
                  label="Avg Latency"
                  onSort={() => handleSort("latency")}
                  sortField={sortField}
                  sortDir={sortDir}
                  field="latency"
                >
                  {sortedStats.map((m) => (
                    <td key={m.id} className={cn("px-4 py-3 text-center", winners.latency === m.id && "bg-emerald-500/5")}>
                      <span className={cn(
                        "text-sm",
                        winners.latency === m.id ? "text-emerald-600 font-medium" : "text-surface-800/70"
                      )}>
                        {m.avgLatencyMs !== null ? `${m.avgLatencyMs} ms` : "N/A"}
                      </span>
                      {winners.latency === m.id && (
                        <span className="ml-1 text-[10px] font-bold text-emerald-500">BEST</span>
                      )}
                    </td>
                  ))}
                </ComparisonRow>

                {/* Throughput row */}
                <ComparisonRow
                  label="Throughput"
                  onSort={() => handleSort("throughput")}
                  sortField={sortField}
                  sortDir={sortDir}
                  field="throughput"
                >
                  {sortedStats.map((m) => (
                    <td key={m.id} className={cn("px-4 py-3 text-center", winners.throughput === m.id && "bg-emerald-500/5")}>
                      <span className={cn(
                        "text-sm",
                        winners.throughput === m.id ? "text-emerald-600 font-medium" : "text-surface-800/70"
                      )}>
                        {m.avgThroughput !== null ? `~${m.avgThroughput} tok/s` : "N/A"}
                      </span>
                      {winners.throughput === m.id && (
                        <span className="ml-1 text-[10px] font-bold text-emerald-500">BEST</span>
                      )}
                    </td>
                  ))}
                </ComparisonRow>

                {/* Availability row */}
                <ComparisonRow
                  label="Availability"
                  onSort={() => handleSort("availability")}
                  sortField={sortField}
                  sortDir={sortDir}
                  field="availability"
                >
                  {sortedStats.map((m) => (
                    <td key={m.id} className={cn("px-4 py-3 text-center", winners.availability === m.id && "bg-emerald-500/5")}>
                      <div className="flex flex-col items-center gap-1">
                        <span className={cn(
                          "text-sm",
                          winners.availability === m.id ? "text-emerald-600 font-medium" : "text-surface-800/70"
                        )}>
                          {m.availabilityPct}%
                        </span>
                        <div className="w-16 h-1.5 bg-surface-100 rounded-full overflow-hidden">
                          <div
                            className={cn(
                              "h-full rounded-full",
                              m.availabilityPct >= 80 ? "bg-emerald-500" : m.availabilityPct >= 50 ? "bg-amber-500" : "bg-red-500"
                            )}
                            style={{ width: `${m.availabilityPct}%` }}
                          />
                        </div>
                      </div>
                      {winners.availability === m.id && (
                        <span className="text-[10px] font-bold text-emerald-500">BEST</span>
                      )}
                    </td>
                  ))}
                </ComparisonRow>

                {/* Context Window row */}
                <ComparisonRow
                  label="Context Window"
                  onSort={() => handleSort("contextWindow")}
                  sortField={sortField}
                  sortDir={sortDir}
                  field="contextWindow"
                >
                  {sortedStats.map((m) => (
                    <td key={m.id} className={cn("px-4 py-3 text-center", winners.contextWindow === m.id && "bg-emerald-500/5")}>
                      <span className={cn(
                        "text-sm",
                        winners.contextWindow === m.id ? "text-emerald-600 font-medium" : "text-surface-800/70"
                      )}>
                        {m.contextWindow
                          ? m.contextWindow >= 1000
                            ? `${(m.contextWindow / 1000).toFixed(0)}K`
                            : String(m.contextWindow)
                          : "N/A"}
                      </span>
                      {winners.contextWindow === m.id && (
                        <span className="ml-1 text-[10px] font-bold text-emerald-500">BEST</span>
                      )}
                    </td>
                  ))}
                </ComparisonRow>

                {/* Providers row */}
                <ComparisonRow
                  label="Providers"
                  onSort={() => handleSort("providers")}
                  sortField={sortField}
                  sortDir={sortDir}
                  field="providers"
                >
                  {sortedStats.map((m) => (
                    <td key={m.id} className={cn("px-4 py-3 text-center", winners.providers === m.id && "bg-emerald-500/5")}>
                      <span className={cn(
                        "text-sm",
                        winners.providers === m.id ? "text-emerald-600 font-medium" : "text-surface-800/70"
                      )}>
                        {m.onlineCount} / {m.providerCount} online
                      </span>
                      {winners.providers === m.id && (
                        <span className="ml-1 text-[10px] font-bold text-emerald-500">BEST</span>
                      )}
                    </td>
                  ))}
                </ComparisonRow>

                {/* Tags row */}
                <tr className="border-b border-surface-100 last:border-b-0">
                  <td className="px-4 py-3 text-surface-800/50 text-xs uppercase tracking-wide font-medium">
                    Tags
                  </td>
                  {sortedStats.map((m) => (
                    <td key={m.id} className="px-4 py-3 text-center">
                      <div className="flex items-center justify-center gap-1 flex-wrap">
                        {m.tags && m.tags.length > 0
                          ? m.tags.map((tag) => (
                              <span
                                key={tag}
                                className="text-xs text-brand-600 bg-brand-50 rounded px-1.5 py-0.5"
                              >
                                {tag}
                              </span>
                            ))
                          : <span className="text-surface-800/30 text-xs">N/A</span>}
                      </div>
                    </td>
                  ))}
                </tr>
              </tbody>
            </table>
          </div>
        </div>
      )}

      <div className="mt-6 text-sm text-surface-800/40">
        Data is live from the Xergon relay. Prices in ERG per 1M tokens. Green highlights indicate the best value in each category.
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Sub-component: Comparison row with sort header
// ---------------------------------------------------------------------------

function ComparisonRow({
  label,
  onSort,
  sortField,
  sortDir,
  field,
  children,
}: {
  label: string;
  onSort: () => void;
  sortField: SortField;
  sortDir: SortDir;
  field: SortField;
  children: React.ReactNode;
}) {
  return (
    <tr className="border-b border-surface-100 last:border-b-0 hover:bg-surface-50/50">
      <td
        className="px-4 py-3 text-surface-800/50 text-xs uppercase tracking-wide font-medium cursor-pointer hover:text-surface-800/70 transition-colors select-none"
        onClick={onSort}
      >
        {label}
        {sortField === field && (
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="12"
            height="12"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            className={cn("inline ml-1 opacity-60", sortDir === "asc" ? "rotate-180" : "")}
          >
            <path d="m7 15 5 5 5-5" />
          </svg>
        )}
      </td>
      {children}
    </tr>
  );
}
