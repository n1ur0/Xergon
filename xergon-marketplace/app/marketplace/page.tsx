"use client";

import { Suspense, useCallback, useEffect, useMemo, useState } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface MarketplaceModel {
  id: string;
  name: string;
  description: string;
  provider: string;
  providerAvatar: string;
  price: number;
  rating: number;
  reviewCount: number;
  category: "chat" | "code" | "image" | "embeddings" | "specialized";
  capabilities: string[];
  featured: boolean;
  requestsPerDay: number;
  tier: "free" | "basic" | "pro" | "enterprise";
}

interface Transaction {
  id: string;
  type: "purchase" | "listing" | "rating";
  model: string;
  user: string;
  amount: string;
  date: string;
}

interface PricingTier {
  name: string;
  price: string;
  requests: string;
  features: string[];
  highlight: boolean;
}

// ---------------------------------------------------------------------------
// Mock data
// ---------------------------------------------------------------------------

const MOCK_MODELS: MarketplaceModel[] = [
  {
    id: "mp1",
    name: "XergonChat-7B",
    description: "General-purpose conversational AI with multi-turn context and tool-use capabilities.",
    provider: "NodeAlpha",
    providerAvatar: "NA",
    price: 0,
    rating: 4.8,
    reviewCount: 1240,
    category: "chat",
    capabilities: ["Chat", "Multi-turn", "Tool-use", "Streaming"],
    featured: true,
    requestsPerDay: 284700,
    tier: "free",
  },
  {
    id: "mp2",
    name: "XergonCode-13B",
    description: "Code generation model fine-tuned on 50B tokens across 40+ programming languages.",
    provider: "ComputeHive",
    providerAvatar: "CH",
    price: 2,
    rating: 4.9,
    reviewCount: 856,
    category: "code",
    capabilities: ["Code-gen", "Debug", "Refactor", "Multi-lang"],
    featured: true,
    requestsPerDay: 156300,
    tier: "basic",
  },
  {
    id: "mp3",
    name: "XergonVision-Pro",
    description: "Multi-modal vision model for image understanding, OCR, and visual QA.",
    provider: "VisionNet",
    providerAvatar: "VN",
    price: 5,
    rating: 4.7,
    reviewCount: 623,
    category: "image",
    capabilities: ["Image QA", "OCR", "Object detection"],
    featured: true,
    requestsPerDay: 89200,
    tier: "pro",
  },
  {
    id: "mp4",
    name: "XergonEmbed-v3",
    description: "High-dimensional embeddings for semantic search, clustering, and RAG.",
    provider: "NodeAlpha",
    providerAvatar: "NA",
    price: 0.2,
    rating: 4.6,
    reviewCount: 2100,
    category: "embeddings",
    capabilities: ["Semantic search", "Clustering", "RAG"],
    featured: true,
    requestsPerDay: 420000,
    tier: "free",
  },
  {
    id: "mp5",
    name: "XergonChat-70B",
    description: "Large-scale model for complex reasoning, analysis, and creative tasks. 128K context.",
    provider: "DeepMesh",
    providerAvatar: "DM",
    price: 8,
    rating: 4.9,
    reviewCount: 432,
    category: "chat",
    capabilities: ["Reasoning", "Creative", "128K context"],
    featured: true,
    requestsPerDay: 62300,
    tier: "enterprise",
  },
  {
    id: "mp6",
    name: "XergonDiffuse-XL",
    description: "Text-to-image diffusion generating high-fidelity images up to 2048x2048.",
    provider: "ArtifAI",
    providerAvatar: "AI",
    price: 5,
    rating: 4.5,
    reviewCount: 789,
    category: "image",
    capabilities: ["Text-to-image", "Style transfer", "Inpainting"],
    featured: false,
    requestsPerDay: 134500,
    tier: "pro",
  },
  {
    id: "mp7",
    name: "XergonCode-7B-Fast",
    description: "Lightweight code completion optimized for low-latency IDE autocomplete.",
    provider: "ComputeHive",
    providerAvatar: "CH",
    price: 0.5,
    rating: 4.7,
    reviewCount: 3200,
    category: "code",
    capabilities: ["Autocomplete", "Fill-in-middle"],
    featured: false,
    requestsPerDay: 890000,
    tier: "basic",
  },
  {
    id: "mp8",
    name: "XergonEmbed-Rerank",
    description: "Cross-encoder reranking model for improving retrieval quality in RAG pipelines.",
    provider: "NodeAlpha",
    providerAvatar: "NA",
    price: 0.8,
    rating: 4.8,
    reviewCount: 540,
    category: "embeddings",
    capabilities: ["Reranking", "RAG", "Multi-lingual"],
    featured: false,
    requestsPerDay: 210000,
    tier: "basic",
  },
  {
    id: "mp9",
    name: "XergonGuard-1B",
    description: "Content safety and moderation for input/output filtering across the network.",
    provider: "SafeNet",
    providerAvatar: "SN",
    price: 0,
    rating: 4.9,
    reviewCount: 1800,
    category: "specialized",
    capabilities: ["Moderation", "Toxicity", "PII detection"],
    featured: false,
    requestsPerDay: 1560000,
    tier: "free",
  },
  {
    id: "mp10",
    name: "XergonSQL-8B",
    description: "Text-to-SQL model supporting PostgreSQL, MySQL, BigQuery, and Snowflake.",
    provider: "DeepMesh",
    providerAvatar: "DM",
    price: 3,
    rating: 4.6,
    reviewCount: 312,
    category: "code",
    capabilities: ["Text-to-SQL", "Multi-dialect"],
    featured: false,
    requestsPerDay: 34500,
    tier: "pro",
  },
  {
    id: "mp11",
    name: "XergonChat-3B-Mini",
    description: "Compact chat model for edge deployment and resource-constrained environments.",
    provider: "EdgeCompute",
    providerAvatar: "EC",
    price: 0,
    rating: 4.4,
    reviewCount: 4500,
    category: "chat",
    capabilities: ["Chat", "Edge deploy", "Low-latency"],
    featured: false,
    requestsPerDay: 1240000,
    tier: "free",
  },
  {
    id: "mp12",
    name: "XergonVision-Mini",
    description: "Lightweight vision model for real-time image classification on edge devices.",
    provider: "EdgeCompute",
    providerAvatar: "EC",
    price: 1,
    rating: 4.3,
    reviewCount: 198,
    category: "image",
    capabilities: ["Classification", "Detection", "Edge"],
    featured: false,
    requestsPerDay: 56000,
    tier: "basic",
  },
];

const MOCK_TRANSACTIONS: Transaction[] = [
  { id: "t1", type: "purchase", model: "XergonChat-7B", user: "0x8f2a...c4d1", amount: "0 ERG", date: "2 min ago" },
  { id: "t2", type: "listing", model: "XergonSQL-8B", user: "DeepMesh", amount: "Pro tier", date: "5 min ago" },
  { id: "t3", type: "purchase", model: "XergonCode-13B", user: "0x3b7e...a12f", amount: "2 ERG", date: "8 min ago" },
  { id: "t4", type: "rating", model: "XergonEmbed-v3", user: "0x9c1d...e5b2", amount: "5 stars", date: "12 min ago" },
  { id: "t5", type: "purchase", model: "XergonVision-Pro", user: "0x5a4f...d8e3", amount: "5 ERG", date: "15 min ago" },
  { id: "t6", type: "listing", model: "XergonDiffuse-XL", user: "ArtifAI", amount: "Pro tier", date: "22 min ago" },
  { id: "t7", type: "purchase", model: "XergonChat-70B", user: "0x7d2c...b1a4", amount: "8 ERG", date: "30 min ago" },
  { id: "t8", type: "rating", model: "XergonCode-7B-Fast", user: "0x2e9a...f6c8", amount: "4 stars", date: "35 min ago" },
];

const PRICING_TIERS: PricingTier[] = [
  {
    name: "Free",
    price: "$0",
    requests: "100 req/day",
    features: ["Access free-tier models", "Community support", "Rate limited", "Standard latency"],
    highlight: false,
  },
  {
    name: "Basic",
    price: "$2",
    requests: "1M tokens",
    features: ["All free features", "Basic tier models", "Priority queue", "Email support"],
    highlight: false,
  },
  {
    name: "Pro",
    price: "$8",
    requests: "1M tokens",
    features: ["All basic features", "Pro tier models", "Instant queue", "Slack support", "Analytics dashboard"],
    highlight: true,
  },
  {
    name: "Enterprise",
    price: "Custom",
    requests: "Unlimited",
    features: ["All pro features", "Enterprise models", "Dedicated nodes", "SLA guarantee", "24/7 support", "Custom fine-tuning"],
    highlight: false,
  },
];

const PROVIDER_STOREFRONTS = [
  { name: "NodeAlpha", avatar: "NA", models: 4, rating: 4.7, description: "Enterprise-grade AI infrastructure" },
  { name: "ComputeHive", avatar: "CH", models: 2, rating: 4.8, description: "High-performance code models" },
  { name: "DeepMesh", avatar: "DM", models: 2, rating: 4.75, description: "Frontier research models" },
  { name: "VisionNet", avatar: "VN", models: 1, rating: 4.7, description: "Computer vision specialists" },
  { name: "ArtifAI", avatar: "AI", models: 1, rating: 4.5, description: "Generative AI creative tools" },
  { name: "SafeNet", avatar: "SN", models: 1, rating: 4.9, description: "AI safety and moderation" },
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toString();
}

function categoryColor(cat: string): string {
  switch (cat) {
    case "chat": return "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300";
    case "code": return "bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-300";
    case "image": return "bg-orange-100 text-orange-700 dark:bg-orange-900/30 dark:text-orange-300";
    case "embeddings": return "bg-teal-100 text-teal-700 dark:bg-teal-900/30 dark:text-teal-300";
    case "specialized": return "bg-pink-100 text-pink-700 dark:bg-pink-900/30 dark:text-pink-300";
    default: return "bg-surface-100 text-surface-800 dark:bg-surface-200 dark:text-surface-900";
  }
}

function StarIcon({ filled }: { filled: boolean }) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill={filled ? "currentColor" : "none"}
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={filled ? "text-yellow-400" : "text-surface-300"}
    >
      <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
    </svg>
  );
}

// ---------------------------------------------------------------------------
// Skeleton
// ---------------------------------------------------------------------------

function MarketplaceCardSkeleton() {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-4 space-y-3 animate-pulse">
      <div className="flex items-center gap-2">
        <div className="h-8 w-8 rounded-full bg-surface-200" />
        <div className="h-4 w-32 rounded bg-surface-200" />
      </div>
      <div className="h-3 w-full rounded bg-surface-200" />
      <div className="h-3 w-2/3 rounded bg-surface-200" />
      <div className="flex gap-1">
        <div className="h-5 w-16 rounded-md bg-surface-200" />
        <div className="h-5 w-20 rounded-md bg-surface-200" />
      </div>
      <div className="flex items-center justify-between">
        <div className="h-4 w-12 rounded bg-surface-200" />
        <div className="h-8 w-20 rounded-lg bg-surface-200" />
      </div>
    </div>
  );
}

function FeaturedCarouselSkeleton() {
  return (
    <div className="flex gap-4 overflow-x-auto scrollbar-none pb-2">
      {Array.from({ length: 5 }, (_, i) => (
        <div key={i} className="shrink-0 w-72 rounded-xl border border-surface-200 bg-surface-0 p-4 space-y-3 animate-pulse">
          <div className="h-5 w-40 rounded bg-surface-200" />
          <div className="h-3 w-full rounded bg-surface-200" />
          <div className="h-3 w-2/3 rounded bg-surface-200" />
          <div className="h-8 w-full rounded-lg bg-surface-200" />
        </div>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Transaction type icon
// ---------------------------------------------------------------------------

function TxIcon({ type }: { type: Transaction["type"] }) {
  switch (type) {
    case "purchase":
      return (
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-accent-500"><circle cx="9" cy="21" r="1"/><circle cx="20" cy="21" r="1"/><path d="M1 1h4l2.68 13.39a2 2 0 0 0 2 1.61h9.72a2 2 0 0 0 2-1.61L23 6H6"/></svg>
      );
    case "listing":
      return (
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-brand-500"><path d="M12 5v14"/><path d="M5 12h14"/></svg>
      );
    case "rating":
      return (
        <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-yellow-500"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/></svg>
      );
  }
}

// ---------------------------------------------------------------------------
// Inner component
// ---------------------------------------------------------------------------

function MarketplaceContent() {
  const [loading, setLoading] = useState(true);
  const [models, setModels] = useState<MarketplaceModel[]>([]);
  const [search, setSearch] = useState("");
  const [sortBy, setSortBy] = useState("popular");
  const [favorites, setFavorites] = useState<Set<string>>(new Set());
  const [activeCategory, setActiveCategory] = useState<string>("all");

  // Load favorites from localStorage
  useEffect(() => {
    try {
      const stored = localStorage.getItem("xergon-favorites");
      if (stored) setFavorites(new Set(JSON.parse(stored)));
    } catch { /* ignore */ }
  }, []);

  // Save favorites
  useEffect(() => {
    localStorage.setItem("xergon-favorites", JSON.stringify([...favorites]));
  }, [favorites]);

  // Simulate fetch
  useEffect(() => {
    const timer = setTimeout(() => {
      setModels(MOCK_MODELS);
      setLoading(false);
    }, 600);
    return () => clearTimeout(timer);
  }, []);

  // Featured models
  const featured = useMemo(() => models.filter((m) => m.featured), [models]);

  // Categories
  const categories = useMemo(
    () => [...new Set(models.map((m) => m.category))],
    [models],
  );

  // Filtered models
  const filtered = useMemo(() => {
    let result = models.filter((m) => {
      if (activeCategory !== "all" && m.category !== activeCategory) return false;
      if (search) {
        const q = search.toLowerCase();
        if (!m.name.toLowerCase().includes(q) && !m.description.toLowerCase().includes(q))
          return false;
      }
      return true;
    });

    result.sort((a, b) => {
      switch (sortBy) {
        case "popular": return b.requestsPerDay - a.requestsPerDay;
        case "price-asc": return a.price - b.price;
        case "price-desc": return b.price - a.price;
        case "rating": return b.rating - a.rating;
        case "newest": return 0;
        default: return 0;
      }
    });

    return result;
  }, [models, activeCategory, search, sortBy]);

  // Toggle favorite
  const toggleFavorite = useCallback((id: string) => {
    setFavorites((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }, []);

  return (
    <main className="mx-auto max-w-6xl px-4 py-6 space-y-8">
      {/* Page header */}
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
        <div className="space-y-1">
          <h1 className="text-2xl font-bold text-surface-900">Model Marketplace</h1>
          <p className="text-sm text-surface-800/60">Buy and sell model inference access on the Xergon network</p>
        </div>
        {/* ERG Balance */}
        <div className="flex items-center gap-2 rounded-xl border border-surface-200 bg-surface-0 px-4 py-2.5">
          <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-brand-500"><circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/></svg>
          <div>
            <p className="text-xs text-surface-800/40">ERG Balance</p>
            <p className="text-sm font-semibold text-surface-900 font-mono">1,247.83 ERG</p>
          </div>
        </div>
      </div>

      {/* Pricing Tiers */}
      <section className="space-y-3">
        <h2 className="text-lg font-semibold text-surface-900">Pricing Tiers</h2>
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
          {PRICING_TIERS.map((tier) => (
            <div
              key={tier.name}
              className={`rounded-xl border p-4 space-y-3 ${
                tier.highlight
                  ? "border-brand-500 bg-brand-50/50 dark:bg-brand-950/20 ring-1 ring-brand-500"
                  : "border-surface-200 bg-surface-0"
              }`}
            >
              <div className="flex items-center justify-between">
                <h3 className="font-semibold text-surface-900">{tier.name}</h3>
                {tier.highlight && (
                  <span className="rounded-full bg-brand-600 px-2 py-0.5 text-xs font-medium text-white">Popular</span>
                )}
              </div>
              <div>
                <span className="text-2xl font-bold text-surface-900">{tier.price}</span>
                {tier.price !== "Custom" && <span className="text-xs text-surface-800/40">/1M tokens</span>}
              </div>
              <p className="text-xs text-surface-800/60">{tier.requests}</p>
              <ul className="space-y-1.5">
                {tier.features.map((f) => (
                  <li key={f} className="flex items-center gap-1.5 text-xs text-surface-800/70">
                    <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-accent-500 shrink-0"><polyline points="20 6 9 17 4 12"/></svg>
                    {f}
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
      </section>

      {/* Featured Carousel */}
      <section className="space-y-3">
        <h2 className="text-lg font-semibold text-surface-900">Featured Models</h2>
        {loading ? (
          <FeaturedCarouselSkeleton />
        ) : (
          <div className="flex gap-4 overflow-x-auto scrollbar-none pb-2">
            {featured.map((model) => (
              <div
                key={model.id}
                className="shrink-0 w-72 rounded-xl border border-brand-200 bg-gradient-to-br from-brand-50/50 to-surface-0 p-4 space-y-3 hover:shadow-md transition-shadow"
              >
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <div className="flex h-8 w-8 items-center justify-center rounded-full bg-brand-600 text-xs font-bold text-white">
                      {model.providerAvatar}
                    </div>
                    <div>
                      <p className="text-sm font-semibold text-surface-900">{model.name}</p>
                      <p className="text-xs text-surface-800/40">{model.provider}</p>
                    </div>
                  </div>
                  <button
                    type="button"
                    onClick={() => toggleFavorite(model.id)}
                    className="shrink-0"
                  >
                    <StarIcon filled={favorites.has(model.id)} />
                  </button>
                </div>
                <p className="text-xs text-surface-800/60 line-clamp-2">{model.description}</p>
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-1">
                    <StarIcon filled={true} />
                    <span className="text-xs font-medium text-surface-900">{model.rating}</span>
                    <span className="text-xs text-surface-800/40">({model.reviewCount})</span>
                  </div>
                  <span className="text-sm font-semibold text-brand-600">
                    {model.price === 0 ? "Free" : `$${model.price}/1M`}
                  </span>
                </div>
              </div>
            ))}
          </div>
        )}
      </section>

      {/* Category tabs + Search + Sort */}
      <section className="space-y-4">
        <div className="flex flex-col sm:flex-row gap-3">
          {/* Search */}
          <div className="relative flex-1">
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="absolute left-3 top-1/2 -translate-y-1/2 text-surface-300"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>
            <input
              type="text"
              placeholder="Search models..."
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="field-input pl-9"
            />
          </div>
          <select
            value={sortBy}
            onChange={(e) => setSortBy(e.target.value)}
            className="field-input w-auto min-w-[140px]"
          >
            <option value="popular">Most Popular</option>
            <option value="rating">Highest Rated</option>
            <option value="price-asc">Price: Low to High</option>
            <option value="price-desc">Price: High to Low</option>
          </select>
        </div>

        {/* Category chips */}
        <div className="flex gap-2 overflow-x-auto scrollbar-none">
          {["all", ...categories].map((cat) => (
            <button
              key={cat}
              type="button"
              onClick={() => setActiveCategory(cat)}
              className={`shrink-0 rounded-full px-4 py-1.5 text-xs font-medium transition-colors ${
                activeCategory === cat
                  ? "bg-brand-600 text-white"
                  : "bg-surface-100 text-surface-800/70 hover:bg-surface-200 dark:bg-surface-200 dark:text-surface-900"
              }`}
            >
              {cat === "all" ? "All" : cat.charAt(0).toUpperCase() + cat.slice(1)}
            </button>
          ))}
        </div>

        {/* Model grid */}
        {loading ? (
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {Array.from({ length: 6 }, (_, i) => (
              <MarketplaceCardSkeleton key={i} />
            ))}
          </div>
        ) : filtered.length === 0 ? (
          <div className="flex flex-col items-center py-12 text-center space-y-2">
            <svg xmlns="http://www.w3.org/2000/svg" width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" className="text-surface-300">
              <circle cx="11" cy="11" r="8" /><line x1="21" y1="21" x2="16.65" y2="16.65" />
            </svg>
            <p className="text-sm text-surface-800/50">No models match your search</p>
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {filtered.map((model) => (
              <div
                key={model.id}
                className="rounded-xl border border-surface-200 bg-surface-0 p-4 space-y-3 hover:border-brand-300 hover:shadow-sm transition-all"
              >
                {/* Header */}
                <div className="flex items-start justify-between">
                  <div className="flex items-center gap-2">
                    <div className="flex h-8 w-8 items-center justify-center rounded-full bg-surface-100 text-xs font-bold text-surface-800 dark:bg-surface-200">
                      {model.providerAvatar}
                    </div>
                    <div>
                      <p className="text-sm font-semibold text-surface-900">{model.name}</p>
                      <p className="text-xs text-surface-800/40">{model.provider}</p>
                    </div>
                  </div>
                  <button
                    type="button"
                    onClick={() => toggleFavorite(model.id)}
                    className="shrink-0 hover:scale-110 transition-transform"
                  >
                    <StarIcon filled={favorites.has(model.id)} />
                  </button>
                </div>

                {/* Description */}
                <p className="text-xs text-surface-800/60 line-clamp-2">{model.description}</p>

                {/* Tags */}
                <div className="flex flex-wrap gap-1">
                  <span className={`inline-flex items-center rounded-md px-2 py-0.5 text-xs font-medium ${categoryColor(model.category)}`}>
                    {model.category}
                  </span>
                  {model.capabilities.slice(0, 2).map((c) => (
                    <span key={c} className="inline-flex items-center rounded-md bg-surface-100 px-2 py-0.5 text-xs text-surface-800/60 dark:bg-surface-200">
                      {c}
                    </span>
                  ))}
                </div>

                {/* Rating + Price */}
                <div className="flex items-center justify-between pt-1">
                  <div className="flex items-center gap-1">
                    <StarIcon filled={true} />
                    <span className="text-xs font-medium text-surface-900">{model.rating}</span>
                    <span className="text-xs text-surface-800/40">({formatNumber(model.reviewCount)})</span>
                  </div>
                  <div className="text-right">
                    <span className="text-sm font-semibold text-surface-900">
                      {model.price === 0 ? "Free" : `$${model.price}/1M`}
                    </span>
                    <p className="text-xs text-surface-800/40">{formatNumber(model.requestsPerDay)} req/day</p>
                  </div>
                </div>

                {/* CTA */}
                <button
                  type="button"
                  className="w-full rounded-lg bg-brand-600 px-4 py-2 text-xs font-medium text-white hover:bg-brand-700 transition-colors"
                >
                  {model.price === 0 ? "Get Started" : "Purchase Access"}
                </button>
              </div>
            ))}
          </div>
        )}
      </section>

      {/* Provider Storefronts */}
      <section className="space-y-3">
        <h2 className="text-lg font-semibold text-surface-900">Provider Storefronts</h2>
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
          {PROVIDER_STOREFRONTS.map((p) => (
            <div
              key={p.name}
              className="rounded-xl border border-surface-200 bg-surface-0 p-4 flex items-center gap-3 hover:border-brand-300 transition-colors"
            >
              <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-brand-100 text-sm font-bold text-brand-700 dark:bg-brand-900/30 dark:text-brand-300">
                {p.avatar}
              </div>
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <p className="text-sm font-semibold text-surface-900">{p.name}</p>
                  <span className="text-xs text-surface-800/40">{p.models} models</span>
                </div>
                <p className="text-xs text-surface-800/50 truncate">{p.description}</p>
                <div className="flex items-center gap-1 mt-0.5">
                  <StarIcon filled={true} />
                  <span className="text-xs font-medium text-surface-800">{p.rating}</span>
                </div>
              </div>
              <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-surface-300 shrink-0"><polyline points="9 18 15 12 9 6"/></svg>
            </div>
          ))}
        </div>
      </section>

      {/* Activity Feed */}
      <section className="space-y-3">
        <h2 className="text-lg font-semibold text-surface-900">Recent Activity</h2>
        <div className="rounded-xl border border-surface-200 bg-surface-0 divide-y divide-surface-200">
          {MOCK_TRANSACTIONS.map((tx) => (
            <div key={tx.id} className="flex items-center gap-3 px-4 py-3">
              <TxIcon type={tx.type} />
              <div className="flex-1 min-w-0">
                <p className="text-xs text-surface-900">
                  <span className="font-medium">{tx.user}</span>
                  <span className="text-surface-800/50">
                    {" "}
                    {tx.type === "purchase" ? "purchased" : tx.type === "listing" ? "listed" : "rated"}{" "}
                  </span>
                  <span className="font-medium">{tx.model}</span>
                </p>
              </div>
              <span className="text-xs text-surface-800/40 whitespace-nowrap">{tx.date}</span>
            </div>
          ))}
        </div>
      </section>
    </main>
  );
}

// ---------------------------------------------------------------------------
// Page component
// ---------------------------------------------------------------------------

export default function MarketplacePage() {
  return (
    <Suspense
      fallback={
        <main className="mx-auto max-w-6xl px-4 py-6 space-y-8">
          <div className="animate-pulse space-y-2">
            <div className="h-7 w-48 rounded bg-surface-200" />
            <div className="h-4 w-80 rounded bg-surface-200" />
          </div>
          <FeaturedCarouselSkeleton />
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {Array.from({ length: 6 }, (_, i) => (
              <MarketplaceCardSkeleton key={i} />
            ))}
          </div>
        </main>
      }
    >
      <MarketplaceContent />
    </Suspense>
  );
}
