"use client";

import { useState, useMemo } from "react";
import { useRouter } from "next/navigation";
import { cn } from "@/lib/utils";
import { FALLBACK_MODELS } from "@/lib/constants";
import {
  useModels,
  type ChainModelInfo,
} from "@/lib/hooks/use-chain-data";
import type { ModelInfo } from "@/lib/api/client";
import { SkeletonCardGrid } from "@/components/ui/SkeletonCard";
import { ErrorBoundary } from "@/components/ui/ErrorBoundary";
import { EmptyState } from "@/components/ui/EmptyState";
import { ModelsSkeleton } from "@/components/models/ModelsSkeleton";
import { SuspenseWrap } from "@/components/ui/SuspenseWrap";

// ── Convert chain model to the display format used by ModelCard ──

interface DisplayModel {
  id: string;
  name: string;
  provider: string;
  tier: string;
  pricePerInputTokenNanoerg: number;
  pricePerOutputTokenNanoerg: number;
  effectivePriceNanoerg?: number;
  providerCount?: number;
  available: boolean;
  description?: string;
  contextWindow?: number;
  speed?: string;
  tags?: string[];
  freeTier?: boolean;
}

function toDisplayModel(m: ChainModelInfo | ModelInfo): DisplayModel {
  // ChainModelInfo uses snake_case, ModelInfo uses camelCase.
  // Use "in" checks to narrow the union type safely.
  const isChain = "price_per_input_token_nanoerg" in m;
  return {
    id: m.id,
    name: m.name,
    provider: m.provider,
    tier: m.tier,
    pricePerInputTokenNanoerg: isChain
      ? m.price_per_input_token_nanoerg
      : (m.pricePerInputTokenNanoerg ?? 0),
    pricePerOutputTokenNanoerg: isChain
      ? m.price_per_output_token_nanoerg
      : (m.pricePerOutputTokenNanoerg ?? 0),
    effectivePriceNanoerg: isChain
      ? m.effective_price_nanoerg
      : m.effectivePriceNanoerg,
    providerCount: isChain
      ? m.provider_count
      : m.providerCount,
    available: m.available,
    description: m.description,
    contextWindow: isChain
      ? m.context_window
      : m.contextWindow,
    speed: m.speed as DisplayModel["speed"],
    tags: m.tags,
    freeTier: isChain
      ? m.free_tier
      : m.freeTier,
  };
}

// ── Tag color map ──
const TAG_STYLES: Record<string, string> = {
  Fast: "bg-amber-100 text-amber-700",
  Smart: "bg-violet-100 text-violet-700",
  Code: "bg-blue-100 text-blue-700",
  Creative: "bg-pink-100 text-pink-700",
  Free: "bg-emerald-100 text-emerald-700",
};

// ── Speed indicator ──
function SpeedBar({ speed }: { speed?: string }) {
  if (!speed) return null;
  const config = {
    fast: { label: "Fast", bars: 3, color: "bg-emerald-500" },
    balanced: { label: "Balanced", bars: 2, color: "bg-amber-500" },
    slow: { label: "Thorough", bars: 1, color: "bg-blue-500" },
  } as const;

  const c = config[speed as keyof typeof config] ?? config.balanced;

  return (
    <div className="flex items-center gap-1.5" title={`Speed: ${c.label}`}>
      <div className="flex gap-0.5">
        {[1, 2, 3].map((i) => (
          <div
            key={i}
            className={cn(
              "h-1.5 w-3 rounded-full",
              i <= c.bars ? c.color : "bg-surface-200",
            )}
          />
        ))}
      </div>
      <span className="text-xs text-surface-800/50">{c.label}</span>
    </div>
  );
}

// ── Context window formatter ──
function formatContextWindow(tokens: number): string {
  if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(tokens % 1_000_000 === 0 ? 0 : 1)}M`;
  if (tokens >= 1_000) return `${(tokens / 1_000).toFixed(0)}K`;
  return String(tokens);
}

// ── nanoERG to ERG formatter ──
function nanoergToErg(nanoerg: number): string {
  if (nanoerg <= 0) return "0";
  const erg = nanoerg / 1e9;
  // Show up to 6 decimal places, trim trailing zeros
  return erg.toFixed(6).replace(/0+$/, "").replace(/\.$/, "");
}

// ── Format price per 1K tokens in ERG ──
function formatPricePer1K(nanoergPerToken: number): string {
  if (nanoergPerToken <= 0) return "Free";
  const nanoergPer1K = nanoergPerToken * 1000;
  return `${nanoergToErg(nanoergPer1K)} ERG`;
}

const ALL_TAGS = ["Fast", "Smart", "Code", "Creative", "Free"] as const;
type TagFilter = (typeof ALL_TAGS)[number];

// ── Model card component ──
function ModelCard({ model, onTry }: { model: DisplayModel; onTry: () => void }) {
  return (
    <div
      className={cn(
        "relative rounded-xl border p-5 transition-all hover:shadow-md",
        model.available
          ? "border-surface-200 bg-surface-0 hover:border-brand-300"
          : "border-surface-200/60 bg-surface-0/50 opacity-60",
      )}
    >
      {/* Free tier badge */}
      {model.freeTier && (
        <div className="absolute -top-2 -right-2 rounded-full bg-emerald-500 px-2.5 py-0.5 text-xs font-semibold text-white shadow-sm">
          Free
        </div>
      )}

      {/* Header: name + provider */}
      <div className="mb-2">
        <h2 className="font-semibold text-surface-900 leading-tight">{model.name}</h2>
        <p className="text-xs text-surface-800/40 mt-0.5">{model.provider}</p>
      </div>

      {/* Description */}
      {model.description && (
        <p className="text-sm text-surface-800/60 mb-3 line-clamp-2">{model.description}</p>
      )}

      {/* Metadata row */}
      <div className="flex items-center gap-4 mb-3">
        {model.contextWindow && (
          <div className="flex items-center gap-1">
            <span className="text-xs text-surface-800/40">Context</span>
            <span className="text-xs font-medium text-surface-800/70">
              {formatContextWindow(model.contextWindow)}
            </span>
          </div>
        )}
        <SpeedBar speed={model.speed} />
      </div>

      {/* Tags */}
      {model.tags && model.tags.length > 0 && (
        <div className="flex flex-wrap gap-1 mb-4">
          {model.tags.map((tag) => (
            <span
              key={tag}
              className={cn(
                "inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium",
                TAG_STYLES[tag] ?? "bg-surface-100 text-surface-800/60",
              )}
            >
              {tag}
            </span>
          ))}
        </div>
      )}

      {/* Footer: pricing + Try It */}
      <div className="flex items-center justify-between pt-3 border-t border-surface-100">
        <div className="flex flex-col gap-1 text-xs text-surface-800/50">
          <div className="flex items-center gap-2">
            <span className="font-medium text-surface-800/70">
              {model.pricePerInputTokenNanoerg <= 0 && model.pricePerOutputTokenNanoerg <= 0
                ? "FREE"
                : formatPricePer1K(model.effectivePriceNanoerg ?? model.pricePerInputTokenNanoerg)}
            </span>
            {model.effectivePriceNanoerg != null && model.effectivePriceNanoerg > 0 && (
              <span className="text-surface-800/30">/1K tokens</span>
            )}
          </div>
          {model.providerCount != null && model.providerCount > 0 && (
            <span className="text-surface-800/30">
              {model.providerCount} provider{model.providerCount !== 1 ? "s" : ""}
            </span>
          )}
        </div>
        <button
          onClick={onTry}
          disabled={!model.available}
          className={cn(
            "rounded-lg px-3 py-1.5 text-sm font-medium transition-colors",
            model.available
              ? "bg-brand-600 text-white hover:bg-brand-700"
              : "bg-surface-100 text-surface-800/30 cursor-not-allowed",
          )}
        >
          Try It
        </button>
      </div>
    </div>
  );
}

// ── Main page ──
export default function ModelsPage() {
  const router = useRouter();
  const { models: chainModels, isLoading, error } = useModels();

  // Convert chain models to display format, falling back to static fallbacks
  const models: DisplayModel[] = useMemo(() => {
    if (chainModels.length > 0) {
      return chainModels.map(toDisplayModel);
    }
    if (!isLoading) {
      return FALLBACK_MODELS.map(toDisplayModel);
    }
    return [];
  }, [chainModels, isLoading]);

  const [activeFilter, setActiveFilter] = useFilterState();

  // Filter models by active tag
  const filteredModels = useMemo(() => {
    if (!activeFilter) return models;
    return models.filter((m) => {
      if (activeFilter === "Free") return m.freeTier;
      return m.tags?.includes(activeFilter);
    });
  }, [models, activeFilter]);

  // Count models per tag
  const tagCounts = useMemo(() => {
    const counts: Record<string, number> = {};
    for (const tag of ALL_TAGS) {
      counts[tag] = models.filter((m) => {
        if (tag === "Free") return m.freeTier;
        return m.tags?.includes(tag);
      }).length;
    }
    return counts;
  }, [models]);

  function handleTryIt(modelId: string) {
    router.push(`/playground?model=${encodeURIComponent(modelId)}`);
  }

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="mb-8">
        <h1 className="text-2xl font-bold mb-2">Models</h1>
        <p className="text-surface-800/60">
          Browse available models from connected providers. Try any model instantly in the Playground.
        </p>
      </div>

      {/* Filter tags */}
      <div className="flex flex-wrap gap-2 mb-6">
        <button
          onClick={() => setActiveFilter(null)}
          className={cn(
            "rounded-full px-3 py-1 text-sm font-medium transition-colors",
            !activeFilter
              ? "bg-surface-900 text-white"
              : "bg-surface-100 text-surface-800/60 hover:bg-surface-200",
          )}
        >
          All ({models.length})
        </button>
        {ALL_TAGS.map((tag) => (
          <button
            key={tag}
            onClick={() => setActiveFilter(activeFilter === tag ? null : tag)}
            className={cn(
              "rounded-full px-3 py-1 text-sm font-medium transition-colors",
              activeFilter === tag
                ? TAG_STYLES[tag]
                : "bg-surface-100 text-surface-800/60 hover:bg-surface-200",
            )}
          >
            {tag} ({tagCounts[tag]})
          </button>
        ))}
      </div>

      <SuspenseWrap fallback={<ModelsSkeleton />}>
      {/* Error state */}
      {error && !isLoading && (
        <div className="text-center py-8">
          <p className="text-sm text-surface-800/50 mb-2">
            Could not load live model data. Showing available models below.
          </p>
        </div>
      )}

      <ErrorBoundary context="Model Listings">
        {/* Loading skeleton */}
        {isLoading && models.length === 0 ? (
          <SkeletonCardGrid count={6} />
        ) : (
          /* Model grid */
          <div className={cn(
            "grid gap-4 sm:grid-cols-2 lg:grid-cols-3",
            !isLoading && "animate-fade-in",
          )}>
            {filteredModels.map((model) => (
              <ModelCard
                key={model.id}
                model={model}
                onTry={() => handleTryIt(model.id)}
              />
            ))}
          </div>
        )}
      </ErrorBoundary>

      {/* Empty state */}
      {!isLoading && filteredModels.length === 0 && activeFilter && (
        <EmptyState
          type="no-search-results"
          action={{
            label: "Clear Filter",
            onClick: () => setActiveFilter(null),
          }}
        />
      )}

      {/* Footer note */}
      <div className="mt-10 text-sm text-surface-800/50 text-center">
        Models with the <span className="text-emerald-600 font-medium">Free</span> badge can be used for free.
        Prices shown per 1K tokens in ERG.
        {chainModels.length > 0 && (
          <span className="block mt-1 text-surface-800/30">
            Live data from {chainModels.length} model(s) across all providers.
          </span>
        )}
      </div>
      </SuspenseWrap>
    </div>
  );
}

// ── Small helper hook for filter state ──
function useFilterState() {
  const [activeFilter, setActiveFilter] = useState<TagFilter | null>(null);
  return [activeFilter, setActiveFilter] as const;
}
