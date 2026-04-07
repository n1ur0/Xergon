"use client";

import { useState, useCallback } from "react";
import Link from "next/link";
import { cn } from "@/lib/utils";
import type { MarketplaceModel } from "@/types/portfolio";

// ── Helpers ──

function formatPricePer1K(nanoergPerToken: number): string {
  if (nanoergPerToken <= 0) return "Free";
  const nanoergPer1K = nanoergPerToken * 1000;
  const erg = nanoergPer1K / 1e9;
  return `${erg.toFixed(6).replace(/0+$/, "").replace(/\.$/, "")} ERG`;
}

function formatContextWindow(tokens: number): string {
  if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(tokens % 1_000_000 === 0 ? 0 : 1)}M`;
  if (tokens >= 1_000) return `${(tokens / 1_000).toFixed(0)}K`;
  return String(tokens);
}

const CATEGORY_COLORS: Record<string, string> = {
  nlp: "bg-blue-100 text-blue-700",
  vision: "bg-purple-100 text-purple-700",
  code: "bg-green-100 text-green-700",
  audio: "bg-orange-100 text-orange-700",
  multimodal: "bg-pink-100 text-pink-700",
  embeddings: "bg-cyan-100 text-cyan-700",
};

const CATEGORY_LABELS: Record<string, string> = {
  nlp: "NLP",
  vision: "Vision",
  code: "Code",
  audio: "Audio",
  multimodal: "Multimodal",
  embeddings: "Embeddings",
};

// ── Mini Benchmark Bars ──

function BenchmarkBars({ benchmarks }: { benchmarks?: Record<string, number> }) {
  if (!benchmarks || Object.keys(benchmarks).length === 0) return null;

  return (
    <div className="flex flex-wrap gap-1.5">
      {Object.entries(benchmarks).slice(0, 3).map(([name, score]) => (
        <div key={name} className="flex items-center gap-1 rounded bg-surface-50 px-1.5 py-0.5">
          <span className="text-[10px] text-surface-800/40">{name}</span>
          <div className="w-10 h-1 bg-surface-200 rounded-full overflow-hidden">
            <div
              className={cn(
                "h-full rounded-full",
                score >= 80 ? "bg-green-500" : score >= 60 ? "bg-amber-500" : "bg-red-400",
              )}
              style={{ width: `${Math.min(100, score)}%` }}
            />
          </div>
          <span className="text-[10px] font-medium text-surface-800/70">{score}</span>
        </div>
      ))}
    </div>
  );
}

// ── Component ──

interface ModelCardV2Props {
  model: MarketplaceModel;
  viewMode?: "grid" | "list";
  onTry?: () => void;
  onCompare?: () => void;
  isComparing?: boolean;
  onFavorite?: () => void;
  isFavorited?: boolean;
  onDetail?: () => void;
}

export function ModelCardV2({
  model,
  viewMode = "grid",
  onTry,
  onCompare,
  isComparing = false,
  onFavorite,
  isFavorited = false,
  onDetail,
}: ModelCardV2Props) {
  const [isHovered, setIsHovered] = useState(false);

  const price = model.effectivePriceNanoerg ?? model.pricePerInputTokenNanoerg;

  // Grid view
  if (viewMode === "grid") {
    return (
      <div
        className={cn(
          "relative group rounded-xl border p-5 transition-all duration-200",
          model.available
            ? "border-surface-200 bg-surface-0 hover:border-brand-300 hover:shadow-md"
            : "border-surface-200/60 bg-surface-0/50 opacity-60",
          isComparing && "ring-2 ring-brand-500 border-brand-500",
        )}
        onMouseEnter={() => setIsHovered(true)}
        onMouseLeave={() => setIsHovered(false)}
      >
        {/* Badges */}
        <div className="absolute -top-2 left-3 flex gap-1.5">
          {model.isFeatured && (
            <span className="rounded-full bg-amber-500 px-2 py-0.5 text-[10px] font-semibold text-white shadow-sm">
              Featured
            </span>
          )}
          {model.isTrending && (
            <span className="rounded-full bg-red-500 px-2 py-0.5 text-[10px] font-semibold text-white shadow-sm">
              Trending
            </span>
          )}
          {model.freeTier && !model.isFeatured && (
            <span className="rounded-full bg-emerald-500 px-2 py-0.5 text-[10px] font-semibold text-white shadow-sm">
              Free
            </span>
          )}
        </div>

        {/* Quick actions (top-right) */}
        <div className="absolute -top-2 right-3 flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
          {onCompare && (
            <button
              onClick={onCompare}
              className={cn(
                "rounded-full p-1.5 text-xs shadow-sm transition-colors",
                isComparing
                  ? "bg-brand-600 text-white"
                  : "bg-surface-0 border border-surface-200 text-surface-800/60 hover:bg-brand-50 hover:text-brand-600",
              )}
              title={isComparing ? "Remove from compare" : "Add to compare"}
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M16 3h5v5" /><path d="M8 3H3v5" /><path d="M12 22v-8.3a4 4 0 0 0-1.172-2.872L3 3" /><path d="m15 9 6-6" />
              </svg>
            </button>
          )}
          {onFavorite && (
            <button
              onClick={onFavorite}
              className={cn(
                "rounded-full p-1.5 text-xs shadow-sm transition-colors",
                isFavorited
                  ? "bg-red-50 text-red-500 border border-red-200"
                  : "bg-surface-0 border border-surface-200 text-surface-800/60 hover:bg-red-50 hover:text-red-500",
              )}
              title={isFavorited ? "Remove from favorites" : "Add to favorites"}
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill={isFavorited ? "currentColor" : "none"} stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z" />
              </svg>
            </button>
          )}
        </div>

        {/* Header */}
        <div className="mb-2 mt-2">
          <div className="flex items-center gap-2 mb-1">
            <span className={cn("rounded-full px-1.5 py-0.5 text-[10px] font-medium", CATEGORY_COLORS[model.category] ?? "bg-surface-100 text-surface-800/60")}>
              {CATEGORY_LABELS[model.category] ?? model.category}
            </span>
            {model.avgRating != null && (
              <span className="flex items-center gap-0.5 text-xs text-surface-800/50">
                <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="currentColor" className="text-amber-400">
                  <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
                </svg>
                {model.avgRating.toFixed(1)}
              </span>
            )}
          </div>
          <button onClick={onDetail} className="text-left w-full">
            <h2 className="font-semibold text-surface-900 leading-tight hover:text-brand-600 transition-colors">
              {model.name}
            </h2>
          </button>
          <p className="text-xs text-surface-800/40 mt-0.5">
            by <span className="text-surface-800/60">{model.provider}</span>
            {model.providerCount != null && model.providerCount > 0 && (
              <span className="text-surface-800/30"> \u00B7 {model.providerCount} provider{model.providerCount !== 1 ? "s" : ""}</span>
            )}
          </p>
        </div>

        {/* Description */}
        {model.description && (
          <p className="text-sm text-surface-800/60 mb-3 line-clamp-2">{model.description}</p>
        )}

        {/* Benchmarks */}
        <BenchmarkBars benchmarks={model.benchmarks} />

        {/* Tags */}
        {model.tags && model.tags.length > 0 && (
          <div className="flex flex-wrap gap-1 mb-3 mt-2">
            {model.tags.slice(0, 4).map(tag => (
              <span key={tag} className="rounded-full bg-surface-100 px-2 py-0.5 text-[10px] text-surface-800/60">
                {tag}
              </span>
            ))}
            {model.tags.length > 4 && (
              <span className="text-[10px] text-surface-800/30">+{model.tags.length - 4}</span>
            )}
          </div>
        )}

        {/* Footer */}
        <div className="flex items-center justify-between pt-3 border-t border-surface-100 mt-auto">
          <div className="flex flex-col gap-0.5">
            <span className="text-xs font-medium text-surface-800/70">
              {price <= 0 ? "FREE" : formatPricePer1K(price)}
            </span>
            {price > 0 && <span className="text-[10px] text-surface-800/30">/1K tokens</span>}
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

  // List view
  return (
    <div
      className={cn(
        "group flex items-center gap-4 rounded-lg border p-4 transition-all duration-200",
        model.available
          ? "border-surface-200 bg-surface-0 hover:border-brand-300 hover:shadow-sm"
          : "border-surface-200/60 bg-surface-0/50 opacity-60",
        isComparing && "ring-2 ring-brand-500 border-brand-500",
      )}
      onMouseEnter={() => setIsHovered(true)}
      onMouseLeave={() => setIsHovered(false)}
    >
      {/* Category + Name */}
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2 mb-0.5">
          <span className={cn("rounded-full px-1.5 py-0.5 text-[10px] font-medium", CATEGORY_COLORS[model.category] ?? "bg-surface-100 text-surface-800/60")}>
            {CATEGORY_LABELS[model.category] ?? model.category}
          </span>
          {model.isFeatured && (
            <span className="rounded-full bg-amber-100 text-amber-700 px-1.5 py-0.5 text-[10px] font-medium">Featured</span>
          )}
          {model.isTrending && (
            <span className="rounded-full bg-red-100 text-red-700 px-1.5 py-0.5 text-[10px] font-medium">Trending</span>
          )}
        </div>
        <button onClick={onDetail} className="text-left">
          <h3 className="font-medium text-surface-900 hover:text-brand-600 transition-colors">{model.name}</h3>
        </button>
        <p className="text-xs text-surface-800/40">by {model.provider}</p>
      </div>

      {/* Context window */}
      <div className="hidden sm:block text-center min-w-[80px]">
        <div className="text-xs text-surface-800/40">Context</div>
        <div className="text-sm font-medium text-surface-800/70">
          {model.contextWindow ? formatContextWindow(model.contextWindow) : "N/A"}
        </div>
      </div>

      {/* Rating */}
      <div className="hidden sm:flex flex-col items-center min-w-[60px]">
        <div className="text-xs text-surface-800/40">Rating</div>
        <div className="flex items-center gap-0.5">
          <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="currentColor" className="text-amber-400">
            <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
          </svg>
          <span className="text-sm font-medium">{model.avgRating?.toFixed(1) ?? "N/A"}</span>
        </div>
        {model.reviewCount != null && <span className="text-[10px] text-surface-800/30">{model.reviewCount} reviews</span>}
      </div>

      {/* Price */}
      <div className="text-right min-w-[100px]">
        <div className="text-xs text-surface-800/40">Price</div>
        <div className="text-sm font-medium text-surface-800/70">
          {price <= 0 ? "Free" : formatPricePer1K(price)}
        </div>
        {price > 0 && <span className="text-[10px] text-surface-800/30">/1K tokens</span>}
      </div>

      {/* Quick actions */}
      <div className="flex items-center gap-1 shrink-0">
        {onCompare && (
          <button
            onClick={onCompare}
            className={cn(
              "rounded-lg p-2 transition-colors",
              isComparing
                ? "bg-brand-100 text-brand-700"
                : "text-surface-800/40 hover:bg-surface-100 hover:text-surface-800/70",
            )}
            title="Compare"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M16 3h5v5" /><path d="M8 3H3v5" /><path d="M12 22v-8.3a4 4 0 0 0-1.172-2.872L3 3" /><path d="m15 9 6-6" />
            </svg>
          </button>
        )}
        {onFavorite && (
          <button
            onClick={onFavorite}
            className={cn(
              "rounded-lg p-2 transition-colors",
              isFavorited ? "text-red-500" : "text-surface-800/40 hover:bg-red-50 hover:text-red-500",
            )}
            title="Favorite"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill={isFavorited ? "currentColor" : "none"} stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M19 14c1.49-1.46 3-3.21 3-5.5A5.5 5.5 0 0 0 16.5 3c-1.76 0-3 .5-4.5 2-1.5-1.5-2.74-2-4.5-2A5.5 5.5 0 0 0 2 8.5c0 2.3 1.5 4.05 3 5.5l7 7Z" />
            </svg>
          </button>
        )}
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
