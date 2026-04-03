"use client";

import { useState, useEffect, useMemo } from "react";
import { useRouter } from "next/navigation";
import { endpoints, type ModelInfo } from "@/lib/api/client";
import { cn } from "@/lib/utils";
import { FALLBACK_MODELS } from "@/lib/constants";

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

const ALL_TAGS = ["Fast", "Smart", "Code", "Creative", "Free"] as const;
type TagFilter = (typeof ALL_TAGS)[number];

// ── Model card component ──
function ModelCard({ model, onTry }: { model: ModelInfo; onTry: () => void }) {
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
        <div className="flex gap-3 text-xs text-surface-800/50">
          <span>
            {model.pricePerInputToken === 0 ? "Free" : `$${model.pricePerInputToken}`}
            <span className="text-surface-800/30"> /1M in</span>
          </span>
          <span>
            {model.pricePerOutputToken === 0 ? "Free" : `$${model.pricePerOutputToken}`}
            <span className="text-surface-800/30"> /1M out</span>
          </span>
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

  const [models, setModels] = useState<ModelInfo[]>(FALLBACK_MODELS);
  const [isLoading, setIsLoading] = useState(true);
  const [activeFilter, setActiveFilter] = useState<TagFilter | null>(null);

  // Fetch models on mount
  useEffect(() => {
    endpoints
      .listModels()
      .then((ms) => {
        setModels(ms.length > 0 ? ms : FALLBACK_MODELS);
      })
      .catch(() => {
        // Keep fallback models when backend is unavailable
      })
      .finally(() => setIsLoading(false));
  }, []);

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

      {/* Loading skeleton */}
      {isLoading ? (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {Array.from({ length: 6 }).map((_, i) => (
            <div
              key={i}
              className="rounded-xl border border-surface-200 bg-surface-0 p-5 animate-pulse"
            >
              <div className="h-5 w-2/3 rounded bg-surface-200 mb-3" />
              <div className="h-4 w-full rounded bg-surface-100 mb-2" />
              <div className="h-4 w-4/5 rounded bg-surface-100 mb-4" />
              <div className="flex gap-2 mb-4">
                <div className="h-5 w-12 rounded-full bg-surface-100" />
                <div className="h-5 w-12 rounded-full bg-surface-100" />
              </div>
              <div className="flex justify-between pt-3 border-t border-surface-100">
                <div className="h-4 w-24 rounded bg-surface-100" />
                <div className="h-7 w-16 rounded-lg bg-surface-100" />
              </div>
            </div>
          ))}
        </div>
      ) : (
        /* Model grid */
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {filteredModels.map((model) => (
            <ModelCard
              key={model.id}
              model={model}
              onTry={() => handleTryIt(model.id)}
            />
          ))}
        </div>
      )}

      {/* Empty state */}
      {!isLoading && filteredModels.length === 0 && (
        <div className="text-center py-16">
          <p className="text-surface-800/40 text-lg mb-2">No models match this filter</p>
          <button
            onClick={() => setActiveFilter(null)}
            className="text-sm text-brand-600 hover:text-brand-700 font-medium"
          >
            Clear filter
          </button>
        </div>
      )}

      {/* Footer note */}
      <div className="mt-10 text-sm text-surface-800/50 text-center">
        Models with the <span className="text-emerald-600 font-medium">Free</span> badge can be used without credits.
        Prices shown per 1M tokens.
      </div>
    </div>
  );
}
