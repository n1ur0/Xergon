"use client";

import { useState, useEffect, useMemo, useCallback, useRef } from "react";
import { useRouter } from "next/navigation";
import { cn } from "@/lib/utils";
import { ModelCardV2 } from "./ModelCardV2";
import type {
  MarketplaceModel,
  ModelCategoryInfo,
  MarketplaceSortField,
  ViewMode,
} from "@/types/portfolio";

// ── Helpers ──

const SORT_OPTIONS: { value: MarketplaceSortField; label: string }[] = [
  { value: "relevance", label: "Relevance" },
  { value: "price_asc", label: "Price: Low to High" },
  { value: "price_desc", label: "Price: High to Low" },
  { value: "rating", label: "Highest Rated" },
  { value: "popularity", label: "Most Popular" },
  { value: "newest", label: "Newest" },
  { value: "benchmark", label: "Benchmark Score" },
];

const QUANTIZATION_OPTIONS = ["Q4_0", "Q4_K_M", "Q5_K_M", "Q5_K_S", "Q8_0", "FP16", "FP32", "BF16"];

const CATEGORY_ICONS: Record<string, string> = {
  nlp: "\uD83D\uDCDD",
  vision: "\uD83D\uDCF8",
  code: "\uD83D\uDCBB",
  audio: "\uD83C\uDFB5",
  multimodal: "\uD83C\uDFA8",
  embeddings: "\uD83D\uDD0D",
};

const CATEGORY_COLORS: Record<string, string> = {
  nlp: "bg-blue-100 text-blue-700 border-blue-200",
  vision: "bg-purple-100 text-purple-700 border-purple-200",
  code: "bg-green-100 text-green-700 border-green-200",
  audio: "bg-orange-100 text-orange-700 border-orange-200",
  multimodal: "bg-pink-100 text-pink-700 border-pink-200",
  embeddings: "bg-cyan-100 text-cyan-700 border-cyan-200",
};

// ── Model Detail Drawer ──

function ModelDetailDrawer({
  model,
  onClose,
  onTry,
  onCompare,
  isComparing,
}: {
  model: MarketplaceModel;
  onClose: () => void;
  onTry: () => void;
  onCompare: () => void;
  isComparing: boolean;
}) {
  const price = model.effectivePriceNanoerg ?? model.pricePerInputTokenNanoerg;

  return (
    <div className="fixed inset-0 z-50 flex justify-end" onClick={onClose}>
      <div className="absolute inset-0 bg-black/40" />
      <div
        className="relative w-full max-w-lg bg-surface-0 shadow-2xl overflow-y-auto animate-slide-in-right"
        onClick={e => e.stopPropagation()}
        style={{ animation: "slideInRight 0.3s ease-out" }}
      >
        {/* Header */}
        <div className="sticky top-0 bg-surface-0 border-b border-surface-200 px-6 py-4 flex items-center justify-between">
          <h2 className="text-lg font-semibold text-surface-900 truncate">{model.name}</h2>
          <button onClick={onClose} className="rounded-lg p-1.5 hover:bg-surface-100 text-surface-800/60 transition-colors">
            <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M18 6 6 18" /><path d="m6 6 12 12" />
            </svg>
          </button>
        </div>

        <div className="px-6 py-5 space-y-5">
          {/* Provider & Category */}
          <div className="flex items-center gap-2">
            <span className={cn("rounded-full px-2 py-0.5 text-xs font-medium border", CATEGORY_COLORS[model.category] ?? "bg-surface-100 text-surface-800/60 border-surface-200")}>
              {CATEGORY_ICONS[model.category]} {model.category.charAt(0).toUpperCase() + model.category.slice(1)}
            </span>
            {model.avgRating != null && (
              <span className="flex items-center gap-1 text-sm">
                <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="currentColor" className="text-amber-400">
                  <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
                </svg>
                <span className="font-medium">{model.avgRating.toFixed(1)}</span>
                <span className="text-surface-800/40">({model.reviewCount} reviews)</span>
              </span>
            )}
          </div>

          <p className="text-xs text-surface-800/40">
            by <span className="text-surface-800/60">{model.provider}</span>
            {model.providerCount != null && <span className="text-surface-800/30"> \u00B7 Available from {model.providerCount} providers</span>}
          </p>

          {/* Description */}
          {model.description && (
            <p className="text-sm text-surface-800/70 leading-relaxed">{model.description}</p>
          )}

          {/* Tags */}
          {model.tags && model.tags.length > 0 && (
            <div className="flex flex-wrap gap-1.5">
              {model.tags.map(tag => (
                <span key={tag} className="rounded-full bg-surface-100 px-2.5 py-1 text-xs text-surface-800/60">
                  {tag}
                </span>
              ))}
            </div>
          )}

          {/* Specs */}
          <div className="grid grid-cols-2 gap-3">
            {model.contextWindow && (
              <div className="rounded-lg bg-surface-50 p-3">
                <div className="text-xs text-surface-800/40 mb-1">Context Window</div>
                <div className="text-sm font-medium text-surface-900">
                  {model.contextWindow >= 1000 ? `${(model.contextWindow / 1000).toFixed(0)}K` : model.contextWindow} tokens
                </div>
              </div>
            )}
            {model.quantization && (
              <div className="rounded-lg bg-surface-50 p-3">
                <div className="text-xs text-surface-800/40 mb-1">Quantization</div>
                <div className="text-sm font-medium text-surface-900">{model.quantization}</div>
              </div>
            )}
            {model.speed && (
              <div className="rounded-lg bg-surface-50 p-3">
                <div className="text-xs text-surface-800/40 mb-1">Speed</div>
                <div className="text-sm font-medium text-surface-900 capitalize">{model.speed}</div>
              </div>
            )}
            <div className="rounded-lg bg-surface-50 p-3">
              <div className="text-xs text-surface-800/40 mb-1">Price</div>
              <div className="text-sm font-medium text-surface-900">
                {price <= 0 ? "Free" : `${(price * 1000 / 1e9).toFixed(6)} ERG/1K tokens`}
              </div>
            </div>
          </div>

          {/* Benchmarks */}
          {model.benchmarks && Object.keys(model.benchmarks).length > 0 && (
            <div>
              <h3 className="text-sm font-semibold text-surface-900 mb-3">Benchmarks</h3>
              <div className="space-y-2">
                {Object.entries(model.benchmarks).map(([name, score]) => (
                  <div key={name} className="flex items-center gap-3">
                    <span className="text-xs text-surface-800/50 w-20 shrink-0">{name}</span>
                    <div className="flex-1 h-2 bg-surface-100 rounded-full overflow-hidden">
                      <div
                        className={cn(
                          "h-full rounded-full transition-all",
                          score >= 80 ? "bg-green-500" : score >= 60 ? "bg-amber-500" : "bg-red-400",
                        )}
                        style={{ width: `${Math.min(100, score)}%` }}
                      />
                    </div>
                    <span className="text-sm font-medium text-surface-800/70 w-8 text-right">{score}</span>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* Actions */}
          <div className="flex gap-2 pt-2">
            <button
              onClick={onTry}
              disabled={!model.available}
              className={cn(
                "flex-1 rounded-lg py-2.5 text-sm font-medium transition-colors",
                model.available
                  ? "bg-brand-600 text-white hover:bg-brand-700"
                  : "bg-surface-100 text-surface-800/30 cursor-not-allowed",
              )}
            >
              Try in Playground
            </button>
            <button
              onClick={onCompare}
              className={cn(
                "rounded-lg border px-4 py-2.5 text-sm font-medium transition-colors",
                isComparing
                  ? "bg-brand-50 border-brand-200 text-brand-700"
                  : "border-surface-200 text-surface-800/60 hover:bg-surface-50",
              )}
            >
              {isComparing ? "Remove" : "Compare"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Compare Panel ──

function ComparePanel({
  models,
  onClose,
  onRemove,
  onCompare,
}: {
  models: MarketplaceModel[];
  onClose: () => void;
  onRemove: (id: string) => void;
  onCompare: () => void;
}) {
  return (
    <div className="fixed bottom-0 left-0 right-0 z-40 bg-surface-0 border-t border-surface-200 shadow-2xl px-4 py-3">
      <div className="max-w-6xl mx-auto flex items-center justify-between gap-4">
        <div className="flex items-center gap-2 flex-1 min-w-0">
          <span className="text-sm font-medium text-surface-900">Comparing:</span>
          <div className="flex items-center gap-1.5 overflow-x-auto">
            {models.map(m => (
              <span key={m.id} className="inline-flex items-center gap-1 rounded-full bg-brand-50 border border-brand-200 px-2.5 py-1 text-xs font-medium text-brand-700 whitespace-nowrap">
                {m.name}
                <button onClick={() => onRemove(m.id)} className="text-brand-400 hover:text-brand-700 ml-0.5">
                  <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2"><path d="M18 6 6 18" /><path d="m6 6 12 12" /></svg>
                </button>
              </span>
            ))}
            {models.length < 4 && (
              <span className="text-xs text-surface-800/30 whitespace-nowrap">{models.length}/4 models</span>
            )}
          </div>
        </div>
        <div className="flex items-center gap-2 shrink-0">
          <button onClick={onClose} className="rounded-lg border border-surface-200 px-3 py-1.5 text-xs text-surface-800/60 hover:bg-surface-50 transition-colors">
            Clear
          </button>
          <button
            onClick={onCompare}
            disabled={models.length < 2}
            className={cn(
              "rounded-lg px-3 py-1.5 text-xs font-medium transition-colors",
              models.length >= 2
                ? "bg-brand-600 text-white hover:bg-brand-700"
                : "bg-surface-100 text-surface-800/30 cursor-not-allowed",
            )}
          >
            Compare ({models.length})
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Main Component ──

export function ModelMarketplaceV2() {
  const router = useRouter();
  const [models, setModels] = useState<MarketplaceModel[]>([]);
  const [featured, setFeatured] = useState<MarketplaceModel[]>([]);
  const [trending, setTrending] = useState<MarketplaceModel[]>([]);
  const [categories, setCategories] = useState<ModelCategoryInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [viewMode, setViewMode] = useState<ViewMode>("grid");

  // Filters
  const [search, setSearch] = useState("");
  const [selectedCategory, setSelectedCategory] = useState<string | null>(null);
  const [minRating, setMinRating] = useState<number>(0);
  const [quantization, setQuantization] = useState<string | null>(null);
  const [priceRange, setPriceRange] = useState<[number, number]>([0, 1000]);
  const [sortBy, setSortBy] = useState<MarketplaceSortField>("relevance");
  const [showFilters, setShowFilters] = useState(false);
  const [showFreeOnly, setShowFreeOnly] = useState(false);

  // Pagination
  const [page, setPage] = useState(1);
  const [totalPages, setTotalPages] = useState(1);
  const [total, setTotal] = useState(0);

  // Compare
  const [compareIds, setCompareIds] = useState<Set<string>>(new Set());

  // Favorites
  const [favoriteIds, setFavoriteIds] = useState<Set<string>>(new Set());

  // Detail drawer
  const [detailModel, setDetailModel] = useState<MarketplaceModel | null>(null);

  // Infinite scroll
  const sentinelRef = useRef<HTMLDivElement>(null);
  const [isLoadingMore, setIsLoadingMore] = useState(false);

  // Load featured, trending, categories on mount
  useEffect(() => {
    Promise.all([
      fetch("/api/marketplace/models?subpath=featured").then(r => r.json()),
      fetch("/api/marketplace/models?subpath=categories").then(r => r.json()),
    ])
      .then(([featData, catData]) => {
        setFeatured(featData.featured ?? []);
        setTrending(featData.trending ?? []);
        setCategories(catData.categories ?? []);
      })
      .catch(() => {});
  }, []);

  // Load models when filters change
  const fetchModels = useCallback(async (pageNum: number, append = false) => {
    if (pageNum === 1) {
      setLoading(true);
    } else {
      setIsLoadingMore(true);
    }

    const params = new URLSearchParams();
    if (search) params.set("search", search);
    if (selectedCategory) params.set("category", selectedCategory);
    if (minRating > 0) params.set("minRating", String(minRating));
    if (quantization) params.set("quantization", quantization);
    if (priceRange[0] > 0) params.set("priceMin", String(priceRange[0]));
    if (priceRange[1] < 1000) params.set("priceMax", String(priceRange[1]));
    params.set("sort", sortBy);
    params.set("page", String(pageNum));
    params.set("pageSize", "24");

    try {
      const res = await fetch(`/api/marketplace/models?${params.toString()}`);
      const data = await res.json();

      if (append) {
        setModels(prev => [...prev, ...data.models]);
      } else {
        setModels(data.models);
      }
      setTotalPages(data.totalPages);
      setTotal(data.total);
    } catch {
      // Handle error
    } finally {
      setLoading(false);
      setIsLoadingMore(false);
    }
  }, [search, selectedCategory, minRating, quantization, priceRange, sortBy]);

  useEffect(() => {
    setPage(1);
    fetchModels(1);
  }, [fetchModels]);

  // Infinite scroll observer
  useEffect(() => {
    const sentinel = sentinelRef.current;
    if (!sentinel) return;

    const observer = new IntersectionObserver(
      entries => {
        if (entries[0].isIntersecting && page < totalPages && !isLoadingMore) {
          const nextPage = page + 1;
          setPage(nextPage);
          fetchModels(nextPage, true);
        }
      },
      { rootMargin: "200px" },
    );

    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [page, totalPages, isLoadingMore, fetchModels]);

  // Toggle compare
  const toggleCompare = useCallback((id: string) => {
    setCompareIds(prev => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else if (next.size < 4) {
        next.add(id);
      }
      return next;
    });
  }, []);

  // Toggle favorite
  const toggleFavorite = useCallback((id: string) => {
    setFavoriteIds(prev => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  // Get compare models
  const compareModels = useMemo(
    () => models.filter(m => compareIds.has(m.id)),
    [models, compareIds],
  );

  // Show featured/trending when no filters are active
  const showSpotlight = !search && !selectedCategory && minRating === 0 && !quantization && !showFreeOnly;

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Header */}
      <div className="mb-8">
        <h1 className="text-2xl font-bold mb-2">Model Marketplace</h1>
        <p className="text-surface-800/60">
          Browse, compare, and try AI models from providers across the Xergon network.
        </p>
      </div>

      {/* Search + Controls */}
      <div className="flex flex-col sm:flex-row gap-3 mb-6">
        <div className="relative flex-1">
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="absolute left-3 top-1/2 -translate-y-1/2 text-surface-800/30">
            <circle cx="11" cy="11" r="8" /><path d="m21 21-4.3-4.3" />
          </svg>
          <input
            type="text"
            placeholder="Search models by name, provider, or capability..."
            value={search}
            onChange={e => setSearch(e.target.value)}
            className="w-full rounded-lg border border-surface-200 bg-surface-0 pl-9 pr-3 py-2.5 text-sm placeholder:text-surface-800/30 focus:outline-none focus:ring-2 focus:ring-brand-500/40 focus:border-brand-500"
          />
        </div>
        <div className="flex items-center gap-2">
          {/* Filter toggle */}
          <button
            onClick={() => setShowFilters(!showFilters)}
            className={cn(
              "rounded-lg border px-3 py-2 text-sm font-medium transition-colors",
              showFilters
                ? "border-brand-500 bg-brand-50 text-brand-700"
                : "border-surface-200 text-surface-800/60 hover:bg-surface-50",
            )}
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="inline mr-1.5">
              <polygon points="22 3 2 3 10 12.46 10 19 14 21 14 12.46 22 3" />
            </svg>
            Filters
          </button>

          {/* Sort */}
          <select
            value={sortBy}
            onChange={e => setSortBy(e.target.value as MarketplaceSortField)}
            className="rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm text-surface-800/60 focus:outline-none focus:ring-2 focus:ring-brand-500/40"
          >
            {SORT_OPTIONS.map(opt => (
              <option key={opt.value} value={opt.value}>{opt.label}</option>
            ))}
          </select>

          {/* View toggle */}
          <div className="flex rounded-lg border border-surface-200 overflow-hidden">
            <button
              onClick={() => setViewMode("grid")}
              className={cn("p-2 transition-colors", viewMode === "grid" ? "bg-brand-50 text-brand-700" : "text-surface-800/40 hover:bg-surface-50")}
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <rect width="7" height="7" x="3" y="3" rx="1" /><rect width="7" height="7" x="14" y="3" rx="1" /><rect width="7" height="7" x="14" y="14" rx="1" /><rect width="7" height="7" x="3" y="14" rx="1" />
              </svg>
            </button>
            <button
              onClick={() => setViewMode("list")}
              className={cn("p-2 transition-colors", viewMode === "list" ? "bg-brand-50 text-brand-700" : "text-surface-800/40 hover:bg-surface-50")}
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <line x1="8" x2="21" y1="6" y2="6" /><line x1="8" x2="21" y1="12" y2="12" /><line x1="8" x2="21" y1="18" y2="18" /><line x1="3" x2="3.01" y1="6" y2="6" /><line x1="3" x2="3.01" y1="12" y2="12" /><line x1="3" x2="3.01" y1="18" y2="18" />
              </svg>
            </button>
          </div>
        </div>
      </div>

      {/* Filter Panel */}
      {showFilters && (
        <div className="rounded-xl border border-surface-200 bg-surface-0 p-4 mb-6 animate-fade-in">
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
            {/* Min Rating */}
            <div>
              <label className="block text-xs font-medium text-surface-800/60 mb-1.5">Min Rating</label>
              <select
                value={minRating}
                onChange={e => setMinRating(Number(e.target.value))}
                className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-brand-500/40"
              >
                <option value={0}>Any Rating</option>
                <option value={3}>3+ Stars</option>
                <option value={3.5}>3.5+ Stars</option>
                <option value={4}>4+ Stars</option>
                <option value={4.5}>4.5+ Stars</option>
              </select>
            </div>

            {/* Quantization */}
            <div>
              <label className="block text-xs font-medium text-surface-800/60 mb-1.5">Quantization</label>
              <select
                value={quantization ?? ""}
                onChange={e => setQuantization(e.target.value || null)}
                className="w-full rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-brand-500/40"
              >
                <option value="">Any</option>
                {QUANTIZATION_OPTIONS.map(q => (
                  <option key={q} value={q}>{q}</option>
                ))}
              </select>
            </div>

            {/* Price Range */}
            <div>
              <label className="block text-xs font-medium text-surface-800/60 mb-1.5">Max Price (nanoERG/token)</label>
              <input
                type="range"
                min={0}
                max={1000}
                step={10}
                value={priceRange[1]}
                onChange={e => setPriceRange([priceRange[0], Number(e.target.value)])}
                className="w-full accent-brand-600"
              />
              <div className="flex justify-between text-[10px] text-surface-800/30 mt-0.5">
                <span>0</span>
                <span>{priceRange[1]} nanoERG</span>
              </div>
            </div>

            {/* Free Only */}
            <div className="flex items-end">
              <label className="flex items-center gap-2 cursor-pointer">
                <input
                  type="checkbox"
                  checked={showFreeOnly}
                  onChange={e => setShowFreeOnly(e.target.checked)}
                  className="rounded border-surface-300 text-brand-600 focus:ring-brand-500"
                />
                <span className="text-sm text-surface-800/60">Free models only</span>
              </label>
            </div>
          </div>

          {/* Active filter chips */}
          {(selectedCategory || minRating > 0 || quantization || showFreeOnly) && (
            <div className="flex items-center gap-2 mt-4 pt-3 border-t border-surface-100">
              <span className="text-xs text-surface-800/40">Active:</span>
              {selectedCategory && (
                <button onClick={() => setSelectedCategory(null)} className="rounded-full bg-brand-50 border border-brand-200 px-2 py-0.5 text-xs text-brand-700">
                  {selectedCategory} \u00D7
                </button>
              )}
              {minRating > 0 && (
                <button onClick={() => setMinRating(0)} className="rounded-full bg-brand-50 border border-brand-200 px-2 py-0.5 text-xs text-brand-700">
                  {minRating}+ stars \u00D7
                </button>
              )}
              {quantization && (
                <button onClick={() => setQuantization(null)} className="rounded-full bg-brand-50 border border-brand-200 px-2 py-0.5 text-xs text-brand-700">
                  {quantization} \u00D7
                </button>
              )}
              {showFreeOnly && (
                <button onClick={() => setShowFreeOnly(false)} className="rounded-full bg-brand-50 border border-brand-200 px-2 py-0.5 text-xs text-brand-700">
                  Free only \u00D7
                </button>
              )}
            </div>
          )}
        </div>
      )}

      {/* Category Browsing */}
      {showSpotlight && categories.length > 0 && (
        <div className="mb-8">
          <h2 className="text-lg font-semibold text-surface-900 mb-4">Browse by Category</h2>
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {categories.map(cat => (
              <button
                key={cat.id}
                onClick={() => setSelectedCategory(selectedCategory === cat.id ? null : cat.id)}
                className={cn(
                  "rounded-xl border p-4 text-left transition-all",
                  selectedCategory === cat.id
                    ? "border-brand-500 bg-brand-50 shadow-sm"
                    : "border-surface-200 bg-surface-0 hover:border-brand-300 hover:shadow-sm",
                )}
              >
                <div className="flex items-center gap-3">
                  <span className="text-2xl">{cat.icon}</span>
                  <div>
                    <h3 className="font-medium text-surface-900">{cat.label}</h3>
                    <p className="text-xs text-surface-800/50 mt-0.5 line-clamp-1">{cat.description}</p>
                    <span className="text-xs text-surface-800/30">{cat.modelCount} models</span>
                  </div>
                </div>
              </button>
            ))}
          </div>
        </div>
      )}

      {/* Featured Models */}
      {showSpotlight && featured.length > 0 && (
        <div className="mb-8">
          <div className="flex items-center justify-between mb-4">
            <h2 className="text-lg font-semibold text-surface-900">
              <span className="mr-1.5">\u2B50</span>Featured Models
            </h2>
          </div>
          <div className={cn("grid gap-4", viewMode === "grid" ? "sm:grid-cols-2 lg:grid-cols-3" : "flex flex-col")}>
            {featured.slice(0, 3).map(model => (
              <ModelCardV2
                key={model.id}
                model={model}
                viewMode={viewMode}
                onTry={() => router.push(`/playground?model=${encodeURIComponent(model.id)}`)}
                onCompare={() => toggleCompare(model.id)}
                isComparing={compareIds.has(model.id)}
                onFavorite={() => toggleFavorite(model.id)}
                isFavorited={favoriteIds.has(model.id)}
                onDetail={() => setDetailModel(model)}
              />
            ))}
          </div>
        </div>
      )}

      {/* Trending Models */}
      {showSpotlight && trending.length > 0 && (
        <div className="mb-8">
          <div className="flex items-center justify-between mb-4">
            <h2 className="text-lg font-semibold text-surface-900">
              <span className="mr-1.5">\uD83D\uDD25</span>Trending
            </h2>
          </div>
          <div className={cn("grid gap-4", viewMode === "grid" ? "sm:grid-cols-2 lg:grid-cols-3" : "flex flex-col")}>
            {trending.slice(0, 3).map(model => (
              <ModelCardV2
                key={model.id}
                model={model}
                viewMode={viewMode}
                onTry={() => router.push(`/playground?model=${encodeURIComponent(model.id)}`)}
                onCompare={() => toggleCompare(model.id)}
                isComparing={compareIds.has(model.id)}
                onFavorite={() => toggleFavorite(model.id)}
                isFavorited={favoriteIds.has(model.id)}
                onDetail={() => setDetailModel(model)}
              />
            ))}
          </div>
        </div>
      )}

      {/* All Models Section */}
      {!showSpotlight || selectedCategory ? (
        <>
          <div className="flex items-center justify-between mb-4">
            <h2 className="text-lg font-semibold text-surface-900">
              {selectedCategory ? `${selectedCategory.charAt(0).toUpperCase() + selectedCategory.slice(1)} Models` : "All Models"}
              <span className="ml-2 text-sm font-normal text-surface-800/40">({total} results)</span>
            </h2>
          </div>

          {loading ? (
            <div className="animate-pulse space-y-4">
              {Array.from({ length: 6 }).map((_, i) => (
                <div key={i} className="h-48 rounded-xl bg-surface-100" />
              ))}
            </div>
          ) : models.length === 0 ? (
            <div className="text-center py-12">
              <p className="text-surface-800/50 mb-2">No models match your filters.</p>
              <button
                onClick={() => {
                  setSelectedCategory(null);
                  setSearch("");
                  setMinRating(0);
                  setQuantization(null);
                  setShowFreeOnly(false);
                }}
                className="text-sm text-brand-600 hover:text-brand-700"
              >
                Clear all filters
              </button>
            </div>
          ) : (
            <>
              <div className={cn("grid gap-4", viewMode === "grid" ? "sm:grid-cols-2 lg:grid-cols-3" : "flex flex-col")}>
                {models.map(model => (
                  <ModelCardV2
                    key={model.id}
                    model={model}
                    viewMode={viewMode}
                    onTry={() => router.push(`/playground?model=${encodeURIComponent(model.id)}`)}
                    onCompare={() => toggleCompare(model.id)}
                    isComparing={compareIds.has(model.id)}
                    onFavorite={() => toggleFavorite(model.id)}
                    isFavorited={favoriteIds.has(model.id)}
                    onDetail={() => setDetailModel(model)}
                  />
                ))}
              </div>

              {/* Infinite scroll sentinel */}
              <div ref={sentinelRef} className="h-10" />
              {isLoadingMore && (
                <div className="flex items-center justify-center py-6">
                  <div className="animate-spin h-6 w-6 border-2 border-brand-600 border-t-transparent rounded-full" />
                  <span className="ml-2 text-sm text-surface-800/50">Loading more...</span>
                </div>
              )}

              {/* Page info */}
              <div className="text-center text-xs text-surface-800/30 mt-6">
                Page {page} of {totalPages} \u00B7 {total} models total
              </div>
            </>
          )}
        </>
      ) : null}

      {/* Compare Panel */}
      {compareModels.length > 0 && (
        <ComparePanel
          models={compareModels}
          onClose={() => setCompareIds(new Set())}
          onRemove={id => toggleCompare(id)}
          onCompare={() => {
            const ids = compareModels.map(m => m.id).join(",");
            router.push(`/compare?models=${ids}`);
          }}
        />
      )}

      {/* Detail Drawer */}
      {detailModel && (
        <ModelDetailDrawer
          model={detailModel}
          onClose={() => setDetailModel(null)}
          onTry={() => router.push(`/playground?model=${encodeURIComponent(detailModel.id)}`)}
          onCompare={() => toggleCompare(detailModel.id)}
          isComparing={compareIds.has(detailModel.id)}
        />
      )}
    </div>
  );
}
