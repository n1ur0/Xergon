"use client";

import { useState, useEffect, useMemo } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface ModelInfo {
  id: string;
  name?: string;
  ownedBy?: string;
  pricing?: string;
  available?: boolean;
  description?: string;
  contextWindow?: number;
}

interface ModelProvider {
  endpoint: string;
  name: string;
  status: string;
  region: string;
  pricePer1mTokens: number;
  uptime: number;
  totalTokens?: number;
}

interface EnrichedModel {
  id: string;
  name: string;
  providers: ModelProvider[];
  minPrice: number;
  maxPrice: number;
  avgPrice: number;
  providerCount: number;
  onlineCount: number;
  totalTokens: number;
  tags: string[];
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function inferTags(modelId: string): string[] {
  const id = modelId.toLowerCase();
  const tags: string[] = [];
  if (id.includes("llama") || id.includes("mistral") || id.includes("qwen") || id.includes("yi") || id.includes("command")) tags.push("Chat");
  if (id.includes("coder") || id.includes("codestral") || id.includes("code")) tags.push("Code");
  if (id.includes("sd") || id.includes("flux") || id.includes("stable-diffusion")) tags.push("Image");
  if (id.includes("whisper") || id.includes("audio")) tags.push("Audio");
  if (id.includes("vision") || id.includes("vl")) tags.push("Vision");
  if (id.includes("8b") || id.includes("7b") || id.includes("phi") || id.includes("mini")) tags.push("Fast");
  if (id.includes("70b") || id.includes("72b") || id.includes("34b") || id.includes("405b") || id.includes("mixtral")) tags.push("Smart");
  if (tags.length === 0) tags.push("Chat");
  return tags;
}

function nanoErgToErg(nano: number): string {
  return (nano / 1_000_000_000).toFixed(4);
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export default function ModelsPage() {
  const [models, setModels] = useState<EnrichedModel[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [degraded, setDegraded] = useState(false);
  const [search, setSearch] = useState("");
  const [tagFilter, setTagFilter] = useState("all");

  useEffect(() => {
    async function load() {
      setLoading(true);
      setError(null);
      try {
        // Fetch models from relay
        const modelsRes = await fetch("/api/operator/models");
        if (!modelsRes.ok) throw new Error(`Models endpoint returned ${modelsRes.status}`);
        const modelsData = await modelsRes.json();

        // Fetch providers for enrichment
        const providersRes = await fetch("/api/operator/providers");
        if (!providersRes.ok) throw new Error(`Providers endpoint returned ${providersRes.status}`);
        const providersData = await providersRes.json();
        const providers = providersData?.providers ?? providersData ?? [];
        setDegraded(providersData?.degraded ?? false);

        // Enrich models with provider info
        const modelMap = new Map<string, EnrichedModel>();

        // Initialize from models endpoint
        const rawModels: ModelInfo[] = modelsData?.data ?? modelsData?.models ?? (Array.isArray(modelsData) ? modelsData : []);
        for (const m of rawModels) {
          const id = m.id ?? m.name ?? "unknown";
          modelMap.set(id, {
            id,
            name: m.name ?? id,
            providers: [],
            minPrice: 0,
            maxPrice: 0,
            avgPrice: 0,
            providerCount: 0,
            onlineCount: 0,
            totalTokens: 0,
            tags: inferTags(id),
          });
        }

        // Enrich from providers data
        for (const p of providers) {
          for (const modelName of (p.models ?? [])) {
            const existing = modelMap.get(modelName);
            if (!existing) {
              modelMap.set(modelName, {
                id: modelName,
                name: modelName,
                providers: [],
                minPrice: 0,
                maxPrice: 0,
                avgPrice: 0,
                providerCount: 0,
                onlineCount: 0,
                totalTokens: 0,
                tags: inferTags(modelName),
              });
            }
            const model = modelMap.get(modelName)!;
            model.providers.push({
              endpoint: p.endpoint ?? "",
              name: p.name ?? p.endpoint ?? "Unknown",
              status: p.status ?? "online",
              region: p.region ?? "Unknown",
              pricePer1mTokens: p.price_per_million ?? p.pricePer1mTokens ?? 0,
              uptime: p.uptime ?? 0,
              totalTokens: p.tokens_processed ?? p.totalTokens ?? 0,
            });
            model.providerCount = model.providers.length;
            model.onlineCount = model.providers.filter((pr) => pr.status === "online" || pr.status === "active" || pr.status === "healthy").length;
          }
        }

        // Calculate pricing
        for (const model of modelMap.values()) {
          if (model.providers.length > 0) {
            const prices = model.providers.map((pr) => pr.pricePer1mTokens).filter((p) => p > 0);
            model.minPrice = prices.length > 0 ? Math.min(...prices) : 0;
            model.maxPrice = prices.length > 0 ? Math.max(...prices) : 0;
            model.avgPrice = prices.length > 0 ? Math.round(prices.reduce((a, b) => a + b, 0) / prices.length) : 0;
            model.totalTokens = model.providers.reduce((s, pr) => s + (pr.totalTokens ?? 0), 0);
          }
        }

        const sorted = Array.from(modelMap.values()).sort((a, b) => b.providerCount - a.providerCount);
        setModels(sorted);
      } catch (err) {
        setError((err as Error).message);
      } finally {
        setLoading(false);
      }
    }

    load();
  }, []);

  const allTags = useMemo(() => {
    const set = new Set<string>();
    for (const m of models) for (const t of m.tags) set.add(t);
    return Array.from(set).sort();
  }, [models]);

  const filtered = useMemo(() => {
    let result = models;
    if (search) {
      const q = search.toLowerCase();
      result = result.filter(
        (m) =>
          m.name.toLowerCase().includes(q) ||
          m.id.toLowerCase().includes(q) ||
          m.tags.some((t) => t.toLowerCase().includes(q)),
      );
    }
    if (tagFilter !== "all") {
      result = result.filter((m) => m.tags.includes(tagFilter));
    }
    return result;
  }, [models, search, tagFilter]);

  // ---------------------------------------------------------------------------
  // Render
  // ---------------------------------------------------------------------------

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold text-surface-900">Models</h1>
        <p className="text-sm text-surface-800/50 mt-1">
          View model distribution and availability across the network.
          {degraded && (
            <span className="ml-2 text-yellow-600">(Fallback data - relay unreachable)</span>
          )}
        </p>
      </div>

      {/* Filters */}
      <div className="flex flex-wrap items-center gap-3">
        <div className="relative flex-1 min-w-[200px]">
          <svg className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-surface-800/30" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <circle cx="11" cy="11" r="8" /><line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>
          <input
            type="text"
            placeholder="Search models..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="w-full rounded-lg border border-surface-200 bg-surface-0 pl-10 pr-4 py-2 text-sm text-surface-900 placeholder:text-surface-800/30 focus:outline-none focus:ring-2 focus:ring-brand-500/20 focus:border-brand-400"
          />
        </div>

        <div className="flex flex-wrap gap-1.5">
          <button
            onClick={() => setTagFilter("all")}
            className={`rounded-full px-3 py-1.5 text-xs font-medium transition-colors ${tagFilter === "all" ? "bg-brand-600 text-white" : "bg-surface-100 text-surface-800/60 hover:bg-surface-200"}`}
          >
            All ({models.length})
          </button>
          {allTags.map((tag) => {
            const count = models.filter((m) => m.tags.includes(tag)).length;
            return (
              <button
                key={tag}
                onClick={() => setTagFilter(tag)}
                className={`rounded-full px-3 py-1.5 text-xs font-medium transition-colors ${tagFilter === tag ? "bg-brand-600 text-white" : "bg-surface-100 text-surface-800/60 hover:bg-surface-200"}`}
              >
                {tag} ({count})
              </button>
            );
          })}
        </div>
      </div>

      {/* Error */}
      {error && !models.length && (
        <div className="rounded-xl border border-danger-200 bg-danger-50/50 p-6 text-center">
          <p className="text-sm text-danger-700">Failed to load models: {error}</p>
        </div>
      )}

      {/* Loading */}
      {loading && !models.length && (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 animate-pulse">
          {Array.from({ length: 6 }).map((_, i) => (
            <div key={i} className="h-48 bg-surface-200 rounded-xl" />
          ))}
        </div>
      )}

      {/* Model cards */}
      {filtered.length > 0 && (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {filtered.map((model) => (
            <div
              key={model.id}
              className="rounded-xl border border-surface-200 bg-surface-0 p-5 hover:border-brand-200 hover:shadow-sm transition-all"
            >
              {/* Header */}
              <div className="flex items-start justify-between gap-2 mb-3">
                <div className="min-w-0">
                  <h3 className="font-semibold text-surface-900 truncate">{model.name}</h3>
                  <p className="text-xs text-surface-800/40 font-mono mt-0.5">{model.id}</p>
                </div>
                <span className={`inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium flex-shrink-0 ${
                  model.onlineCount > 0
                    ? "bg-accent-100 text-accent-700"
                    : "bg-surface-200 text-surface-800/40"
                }`}>
                  <span className={`h-1.5 w-1.5 rounded-full ${model.onlineCount > 0 ? "bg-accent-500" : "bg-surface-400"}`} />
                  {model.onlineCount}/{model.providerCount}
                </span>
              </div>

              {/* Tags */}
              <div className="flex flex-wrap gap-1.5 mb-4">
                {model.tags.map((tag) => (
                  <span key={tag} className="inline-flex rounded-full bg-brand-50 px-2 py-0.5 text-[10px] font-medium text-brand-700">
                    {tag}
                  </span>
                ))}
              </div>

              {/* Pricing */}
              <div className="space-y-2 text-sm">
                <div className="flex items-center justify-between">
                  <span className="text-surface-800/50">Min Price</span>
                  <span className="font-medium text-surface-900">
                    {model.minPrice > 0 ? `${nanoErgToErg(model.minPrice)} ERG/1M tok` : "N/A"}
                  </span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-surface-800/50">Avg Price</span>
                  <span className="font-medium text-surface-900">
                    {model.avgPrice > 0 ? `${nanoErgToErg(model.avgPrice)} ERG/1M tok` : "N/A"}
                  </span>
                </div>
                <div className="flex items-center justify-between">
                  <span className="text-surface-800/50">Providers</span>
                  <span className="font-medium text-surface-900">{model.providerCount}</span>
                </div>
              </div>

              {/* Top providers */}
              {model.providers.length > 0 && (
                <div className="mt-3 pt-3 border-t border-surface-100 space-y-1.5">
                  <p className="text-xs text-surface-800/40 mb-1">Top providers</p>
                  {model.providers.slice(0, 3).map((p) => (
                    <div key={p.endpoint} className="flex items-center justify-between text-xs">
                      <span className="text-surface-800/60 truncate max-w-[150px]">{p.name}</span>
                      <span className={`inline-flex items-center gap-1 ${p.status === "online" || p.status === "active" || p.status === "healthy" ? "text-accent-600" : "text-surface-800/30"}`}>
                        <span className={`h-1 w-1 rounded-full ${p.status === "online" || p.status === "active" || p.status === "healthy" ? "bg-accent-500" : "bg-surface-400"}`} />
                        {p.region}
                      </span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      {/* Empty */}
      {!loading && !error && models.length > 0 && filtered.length === 0 && (
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-8 text-center">
          <p className="text-surface-800/50">No models match your search.</p>
          <button
            onClick={() => { setSearch(""); setTagFilter("all"); }}
            className="mt-2 text-sm text-brand-600 hover:underline font-medium"
          >
            Clear filters
          </button>
        </div>
      )}
    </div>
  );
}
