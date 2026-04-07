"use client";

import { Suspense, useCallback, useEffect, useMemo, useState } from "react";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface ModelVersion {
  version: string;
  date: string;
  changes: string;
}

interface ModelBenchmark {
  name: string;
  score: number;
  delta: string;
}

interface RegistryModel {
  id: string;
  name: string;
  description: string;
  version: string;
  provider: string;
  category: "chat" | "code" | "image" | "embeddings" | "specialized";
  pricing: number; // nanoERG per 1K tokens
  ponwScore: number;
  requestCount: number;
  latencyP95: number; // ms
  verified: boolean;
  registeredDate: string;
  versions: ModelVersion[];
  benchmarks: ModelBenchmark[];
  providerInfo: {
    endpoint: string;
    region: string;
    uptime: number;
    modelsCount: number;
  };
  capabilities: string[];
}

type SortKey = "popularity" | "price" | "latency" | "ponw" | "newest";
type CategoryFilter = "all" | "chat" | "code" | "image" | "embeddings" | "specialized";
type PriceRange = "all" | "free" | "low" | "mid" | "high";

// ---------------------------------------------------------------------------
// Mock data
// ---------------------------------------------------------------------------

const MOCK_MODELS: RegistryModel[] = [
  {
    id: "m1",
    name: "XergonChat-7B",
    description: "General-purpose chat model optimized for conversational AI with multi-turn context support.",
    version: "v2.3.1",
    provider: "NodeAlpha",
    category: "chat",
    pricing: 0,
    ponwScore: 94,
    requestCount: 2847000,
    latencyP95: 120,
    verified: true,
    registeredDate: "2025-08-15",
    versions: [
      { version: "v2.3.1", date: "2025-11-20", changes: "Improved reasoning, 15% faster" },
      { version: "v2.2.0", date: "2025-09-10", changes: "Added tool-use support" },
      { version: "v2.1.0", date: "2025-08-15", changes: "Initial release" },
    ],
    benchmarks: [
      { name: "MMLU", score: 72.4, delta: "+2.1" },
      { name: "HumanEval", score: 58.2, delta: "+3.0" },
      { name: "MT-Bench", score: 8.1, delta: "+0.3" },
    ],
    providerInfo: { endpoint: "nodealpha.xergon.io", region: "US-East", uptime: 99.8, modelsCount: 5 },
    capabilities: ["Chat", "Multi-turn", "Tool-use"],
  },
  {
    id: "m2",
    name: "XergonCode-13B",
    description: "Code generation model fine-tuned on 50B tokens of high-quality code across 40 languages.",
    version: "v1.8.0",
    provider: "ComputeHive",
    category: "code",
    pricing: 120,
    ponwScore: 97,
    requestCount: 1563000,
    latencyP95: 340,
    verified: true,
    registeredDate: "2025-06-22",
    versions: [
      { version: "v1.8.0", date: "2025-11-01", changes: "Added Rust and Zig support" },
      { version: "v1.7.0", date: "2025-09-15", changes: "Improved test generation" },
    ],
    benchmarks: [
      { name: "HumanEval", score: 78.5, delta: "+4.2" },
      { name: "MBPP", score: 72.1, delta: "+1.8" },
      { name: "CodeContests", score: 34.6, delta: "+2.0" },
    ],
    providerInfo: { endpoint: "computehive.xergon.io", region: "EU-West", uptime: 99.95, modelsCount: 3 },
    capabilities: ["Code-gen", "Debug", "Refactor", "Multi-lang"],
  },
  {
    id: "m3",
    name: "XergonVision-Pro",
    description: "Multi-modal vision model for image understanding, OCR, and visual question answering.",
    version: "v3.0.2",
    provider: "VisionNet",
    category: "image",
    pricing: 250,
    ponwScore: 91,
    requestCount: 892000,
    latencyP95: 580,
    verified: true,
    registeredDate: "2025-07-10",
    versions: [
      { version: "v3.0.2", date: "2025-11-15", changes: "Better OCR accuracy" },
      { version: "v3.0.0", date: "2025-10-01", changes: "Architecture overhaul" },
    ],
    benchmarks: [
      { name: "VQAv2", score: 82.3, delta: "+1.5" },
      { name: "OCR-Bench", score: 91.7, delta: "+3.2" },
      { name: "MMMU", score: 56.4, delta: "+2.0" },
    ],
    providerInfo: { endpoint: "visionnet.xergon.io", region: "US-West", uptime: 99.7, modelsCount: 2 },
    capabilities: ["Image QA", "OCR", "Object detection", "Charts"],
  },
  {
    id: "m4",
    name: "XergonEmbed-v3",
    description: "High-dimensional embedding model for semantic search, clustering, and RAG pipelines.",
    version: "v3.1.0",
    provider: "NodeAlpha",
    category: "embeddings",
    pricing: 10,
    ponwScore: 88,
    requestCount: 4200000,
    latencyP95: 25,
    verified: true,
    registeredDate: "2025-04-01",
    versions: [
      { version: "v3.1.0", date: "2025-10-20", changes: "1024-dim output, faster inference" },
      { version: "v3.0.0", date: "2025-07-01", changes: "New architecture" },
    ],
    benchmarks: [
      { name: "MTEB", score: 68.2, delta: "+3.5" },
      { name: "BEIR", score: 72.1, delta: "+1.0" },
    ],
    providerInfo: { endpoint: "nodealpha.xergon.io", region: "US-East", uptime: 99.8, modelsCount: 5 },
    capabilities: ["Semantic search", "Clustering", "RAG", "Classification"],
  },
  {
    id: "m5",
    name: "XergonChat-70B",
    description: "Large-scale chat model for complex reasoning, analysis, and creative writing tasks.",
    version: "v1.2.0",
    provider: "DeepMesh",
    category: "chat",
    pricing: 450,
    ponwScore: 99,
    requestCount: 623000,
    latencyP95: 1200,
    verified: true,
    registeredDate: "2025-09-01",
    versions: [
      { version: "v1.2.0", date: "2025-11-10", changes: "Context window 128K" },
      { version: "v1.1.0", date: "2025-10-01", changes: "Chain-of-thought improvements" },
    ],
    benchmarks: [
      { name: "MMLU", score: 89.1, delta: "+1.4" },
      { name: "GPQA", score: 52.3, delta: "+3.1" },
      { name: "MT-Bench", score: 9.2, delta: "+0.2" },
    ],
    providerInfo: { endpoint: "deepmesh.xergon.io", region: "EU-Central", uptime: 99.9, modelsCount: 2 },
    capabilities: ["Chat", "Reasoning", "Creative", "128K context"],
  },
  {
    id: "m6",
    name: "XergonDiffuse-XL",
    description: "Text-to-image diffusion model generating high-fidelity images up to 2048x2048.",
    version: "v2.0.1",
    provider: "ArtifAI",
    category: "image",
    pricing: 500,
    ponwScore: 86,
    requestCount: 1345000,
    latencyP95: 3200,
    verified: true,
    registeredDate: "2025-05-20",
    versions: [
      { version: "v2.0.1", date: "2025-11-05", changes: "Style consistency improvements" },
      { version: "v2.0.0", date: "2025-09-01", changes: "New architecture, 4x faster" },
    ],
    benchmarks: [
      { name: "FID (COCO)", score: 12.4, delta: "-1.2" },
      { name: "CLIP Score", score: 31.8, delta: "+0.5" },
    ],
    providerInfo: { endpoint: "artifai.xergon.io", region: "Asia-Pacific", uptime: 99.6, modelsCount: 4 },
    capabilities: ["Text-to-image", "Style transfer", "Inpainting", "Upscale"],
  },
  {
    id: "m7",
    name: "XergonCode-7B-Fast",
    description: "Lightweight code completion model optimized for low-latency IDE autocomplete.",
    version: "v1.5.3",
    provider: "ComputeHive",
    category: "code",
    pricing: 40,
    ponwScore: 82,
    requestCount: 8900000,
    latencyP95: 45,
    verified: true,
    registeredDate: "2025-03-15",
    versions: [
      { version: "v1.5.3", date: "2025-10-28", changes: "Fill-in-middle support" },
      { version: "v1.5.0", date: "2025-08-20", changes: "30% latency reduction" },
    ],
    benchmarks: [
      { name: "HumanEval", score: 62.3, delta: "+1.0" },
      { name: "Pass@1", score: 54.1, delta: "+2.3" },
    ],
    providerInfo: { endpoint: "computehive.xergon.io", region: "EU-West", uptime: 99.95, modelsCount: 3 },
    capabilities: ["Autocomplete", "Fill-in-middle", "Multi-lang"],
  },
  {
    id: "m8",
    name: "XergonEmbed-Lite",
    description: "Ultra-fast lightweight embedding model for real-time applications with minimal latency.",
    version: "v1.4.0",
    provider: "FastNode",
    category: "embeddings",
    pricing: 5,
    ponwScore: 79,
    requestCount: 6700000,
    latencyP95: 12,
    verified: false,
    registeredDate: "2025-07-22",
    versions: [
      { version: "v1.4.0", date: "2025-10-15", changes: "384-dim option" },
    ],
    benchmarks: [
      { name: "MTEB", score: 61.5, delta: "+1.2" },
      { name: "BEIR", score: 65.8, delta: "+0.8" },
    ],
    providerInfo: { endpoint: "fastnode.xergon.io", region: "US-East", uptime: 99.5, modelsCount: 1 },
    capabilities: ["Fast embed", "Semantic search", "Dedup"],
  },
  {
    id: "m9",
    name: "XergonChat-3B-Mini",
    description: "Compact chat model for edge deployment and resource-constrained environments.",
    version: "v1.0.2",
    provider: "EdgeCompute",
    category: "chat",
    pricing: 0,
    ponwScore: 76,
    requestCount: 12400000,
    latencyP95: 65,
    verified: true,
    registeredDate: "2025-09-15",
    versions: [
      { version: "v1.0.2", date: "2025-11-12", changes: "Quantized variant" },
    ],
    benchmarks: [
      { name: "MMLU", score: 52.1, delta: "+0.5" },
      { name: "MT-Bench", score: 6.4, delta: "+0.1" },
    ],
    providerInfo: { endpoint: "edgecompute.xergon.io", region: "Global", uptime: 99.2, modelsCount: 1 },
    capabilities: ["Chat", "Edge deploy", "Low-latency"],
  },
  {
    id: "m10",
    name: "XergonGuard-1B",
    description: "Content safety and moderation model for input/output filtering on the Xergon network.",
    version: "v2.1.0",
    provider: "SafeNet",
    category: "specialized",
    pricing: 0,
    ponwScore: 85,
    requestCount: 15600000,
    latencyP95: 8,
    verified: true,
    registeredDate: "2025-02-01",
    versions: [
      { version: "v2.1.0", date: "2025-10-01", changes: "New toxicity categories" },
      { version: "v2.0.0", date: "2025-06-01", changes: "Multilingual support" },
    ],
    benchmarks: [
      { name: "ToxiGen", score: 94.2, delta: "+1.0" },
      { name: "RealToxicity", score: 0.02, delta: "-0.01" },
    ],
    providerInfo: { endpoint: "safenet.xergon.io", region: "US-East", uptime: 99.99, modelsCount: 1 },
    capabilities: ["Moderation", "Toxicity", "PII detection"],
  },
  {
    id: "m11",
    name: "XergonSQL-8B",
    description: "Text-to-SQL model for natural language database queries across multiple dialects.",
    version: "v1.3.0",
    provider: "DeepMesh",
    category: "code",
    pricing: 180,
    ponwScore: 83,
    requestCount: 345000,
    latencyP95: 420,
    verified: false,
    registeredDate: "2025-10-01",
    versions: [
      { version: "v1.3.0", date: "2025-11-08", changes: "Added BigQuery and Snowflake" },
    ],
    benchmarks: [
      { name: "Spider", score: 78.9, delta: "+2.5" },
      { name: "BIRD", score: 62.1, delta: "+3.0" },
    ],
    providerInfo: { endpoint: "deepmesh.xergon.io", region: "EU-Central", uptime: 99.9, modelsCount: 2 },
    capabilities: ["Text-to-SQL", "Multi-dialect", "Schema-aware"],
  },
  {
    id: "m12",
    name: "XergonEmbed-Rerank",
    description: "Cross-encoder reranking model for improving retrieval quality in RAG pipelines.",
    version: "v1.1.0",
    provider: "NodeAlpha",
    category: "embeddings",
    pricing: 30,
    ponwScore: 90,
    requestCount: 2100000,
    latencyP95: 55,
    verified: true,
    registeredDate: "2025-08-10",
    versions: [
      { version: "v1.1.0", date: "2025-11-01", changes: "Multilingual reranking" },
    ],
    benchmarks: [
      { name: "BEIR (Rerank)", score: 81.4, delta: "+2.8" },
      { name: "TREC DL", score: 0.72, delta: "+0.03" },
    ],
    providerInfo: { endpoint: "nodealpha.xergon.io", region: "US-East", uptime: 99.8, modelsCount: 5 },
    capabilities: ["Reranking", "RAG", "Multi-lingual"],
  },
  {
    id: "m13",
    name: "XergonVision-Mini",
    description: "Lightweight vision model for real-time image classification and object detection on edge devices.",
    version: "v1.0.0",
    provider: "EdgeCompute",
    category: "image",
    pricing: 80,
    ponwScore: 74,
    requestCount: 560000,
    latencyP95: 95,
    verified: false,
    registeredDate: "2025-11-01",
    versions: [
      { version: "v1.0.0", date: "2025-11-01", changes: "Initial release" },
    ],
    benchmarks: [
      { name: "ImageNet", score: 84.5, delta: "-" },
      { name: "COCO mAP", score: 42.1, delta: "-" },
    ],
    providerInfo: { endpoint: "edgecompute.xergon.io", region: "Global", uptime: 99.2, modelsCount: 1 },
    capabilities: ["Classification", "Detection", "Edge deploy"],
  },
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toString();
}

function formatDate(dateStr: string): string {
  return new Date(dateStr).toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

// ---------------------------------------------------------------------------
// Verified badge SVG
// ---------------------------------------------------------------------------

function VerifiedBadge() {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className="text-accent-500"
    >
      <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
      <polyline points="9 12 11 14 15 10" />
    </svg>
  );
}

// ---------------------------------------------------------------------------
// Category badge color
// ---------------------------------------------------------------------------

function categoryColor(cat: string): string {
  switch (cat) {
    case "chat": return "bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300";
    case "code": return "bg-purple-100 text-purple-700 dark:bg-purple-900/30 dark:text-purple-300";
    case "image": return "bg-orange-100 text-orange-700 dark:bg-orange-900/30 dark:text-orange-300";
    case "embeddings": return "bg-teal-100 text-teal-700 dark:bg-teal-900/30 dark:text-teal-300";
    default: return "bg-surface-100 text-surface-800 dark:bg-surface-200 dark:text-surface-900";
  }
}

// ---------------------------------------------------------------------------
// Skeleton loader
// ---------------------------------------------------------------------------

function RegistryCardSkeleton() {
  return (
    <div className="rounded-xl border border-surface-200 bg-surface-0 p-4 space-y-3 animate-pulse">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <div className="h-5 w-5 rounded bg-surface-200" />
          <div className="h-4 w-36 rounded bg-surface-200" />
        </div>
        <div className="h-5 w-20 rounded-full bg-surface-200" />
      </div>
      <div className="h-3 w-full rounded bg-surface-200" />
      <div className="h-3 w-3/4 rounded bg-surface-200" />
      <div className="flex gap-1">
        <div className="h-5 w-16 rounded-md bg-surface-200" />
        <div className="h-5 w-20 rounded-md bg-surface-200" />
        <div className="h-5 w-14 rounded-md bg-surface-200" />
      </div>
      <div className="grid grid-cols-2 gap-2">
        <div className="h-14 rounded-lg bg-surface-200" />
        <div className="h-14 rounded-lg bg-surface-200" />
      </div>
      <div className="h-8 w-full rounded-lg bg-surface-200" />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Stats bar skeleton
// ---------------------------------------------------------------------------

function StatsSkeleton() {
  return (
    <div className="grid grid-cols-2 md:grid-cols-4 gap-4 animate-pulse">
      {Array.from({ length: 4 }, (_, i) => (
        <div key={i} className="rounded-xl border border-surface-200 bg-surface-0 p-4">
          <div className="h-3 w-20 rounded bg-surface-200 mb-2" />
          <div className="h-6 w-12 rounded bg-surface-200" />
        </div>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Inner component
// ---------------------------------------------------------------------------

function RegistryContent() {
  const [loading, setLoading] = useState(true);
  const [models, setModels] = useState<RegistryModel[]>([]);
  const [search, setSearch] = useState("");
  const [category, setCategory] = useState<CategoryFilter>("all");
  const [provider, setProvider] = useState("all");
  const [priceRange, setPriceRange] = useState<PriceRange>("all");
  const [sortBy, setSortBy] = useState<SortKey>("popularity");
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [showDeployTip, setShowDeployTip] = useState<string | null>(null);
  const [page, setPage] = useState(1);
  const perPage = 6;

  // Simulate fetch
  useEffect(() => {
    const timer = setTimeout(() => {
      setModels(MOCK_MODELS);
      setLoading(false);
    }, 800);
    return () => clearTimeout(timer);
  }, []);

  // Unique providers
  const providers = useMemo(
    () => [...new Set(models.map((m) => m.provider))].sort(),
    [models],
  );

  // Price range filter
  const matchesPrice = useCallback(
    (p: number, range: PriceRange) => {
      if (range === "all") return true;
      if (range === "free") return p === 0;
      if (range === "low") return p > 0 && p <= 50;
      if (range === "mid") return p > 50 && p <= 200;
      if (range === "high") return p > 200;
      return true;
    },
    [],
  );

  // Filtered + sorted
  const filtered = useMemo(() => {
    let result = models.filter((m) => {
      if (category !== "all" && m.category !== category) return false;
      if (provider !== "all" && m.provider !== provider) return false;
      if (!matchesPrice(m.pricing, priceRange)) return false;
      if (search) {
        const q = search.toLowerCase();
        if (!m.name.toLowerCase().includes(q) && !m.description.toLowerCase().includes(q))
          return false;
      }
      return true;
    });

    result.sort((a, b) => {
      switch (sortBy) {
        case "popularity": return b.requestCount - a.requestCount;
        case "price": return a.pricing - b.pricing;
        case "latency": return a.latencyP95 - b.latencyP95;
        case "ponw": return b.ponwScore - a.ponwScore;
        case "newest": return new Date(b.registeredDate).getTime() - new Date(a.registeredDate).getTime();
        default: return 0;
      }
    });

    return result;
  }, [models, category, provider, priceRange, search, sortBy, matchesPrice]);

  // Pagination
  const totalPages = Math.max(1, Math.ceil(filtered.length / perPage));
  const paginated = filtered.slice((page - 1) * perPage, page * perPage);

  // Reset page on filter change
  useEffect(() => { setPage(1); }, [category, provider, priceRange, search, sortBy]);

  // Stats
  const stats = useMemo(
    () => ({
      totalModels: models.length,
      totalProviders: providers.length,
      avgPrice: models.length > 0
        ? (models.reduce((s, m) => s + m.pricing, 0) / models.length).toFixed(1)
        : "0",
      throughput: formatNumber(models.reduce((s, m) => s + m.requestCount, 0)),
    }),
    [models, providers],
  );

  return (
    <main className="mx-auto max-w-6xl px-4 py-6 space-y-6">
      {/* Page header */}
      <div className="space-y-1">
        <h1 className="text-2xl font-bold text-surface-900">
          Model Registry
        </h1>
        <p className="text-sm text-surface-800/60">
          Browse all models registered on the Xergon decentralized network
        </p>
      </div>

      {/* Stats bar */}
      {loading ? (
        <StatsSkeleton />
      ) : (
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          {[
            { label: "Total Models", value: stats.totalModels, icon: (
              <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-brand-500"><rect x="2" y="2" width="20" height="8" rx="2"/><rect x="2" y="14" width="20" height="8" rx="2"/><circle cx="6" cy="6" r="1"/><circle cx="6" cy="18" r="1"/></svg>
            )},
            { label: "Providers", value: stats.totalProviders, icon: (
              <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-brand-500"><path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M22 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/></svg>
            )},
            { label: "Avg Price", value: `${stats.avgPrice} nERG`, icon: (
              <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-brand-500"><line x1="12" y1="1" x2="12" y2="23"/><path d="M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6"/></svg>
            )},
            { label: "Total Requests", value: stats.throughput, icon: (
              <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="text-brand-500"><polyline points="22 12 18 12 15 21 9 3 6 12 2 12"/></svg>
            )},
          ].map((s) => (
            <div key={s.label} className="rounded-xl border border-surface-200 bg-surface-0 p-4 flex items-center gap-3">
              <div className="shrink-0">{s.icon}</div>
              <div>
                <p className="text-xs text-surface-800/50">{s.label}</p>
                <p className="text-lg font-semibold text-surface-900">{s.value}</p>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Filters row */}
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
        {/* Category */}
        <select
          value={category}
          onChange={(e) => setCategory(e.target.value as CategoryFilter)}
          className="field-input w-auto min-w-[130px]"
        >
          <option value="all">All Categories</option>
          <option value="chat">Chat</option>
          <option value="code">Code</option>
          <option value="image">Image</option>
          <option value="embeddings">Embeddings</option>
          <option value="specialized">Specialized</option>
        </select>
        {/* Provider */}
        <select
          value={provider}
          onChange={(e) => setProvider(e.target.value)}
          className="field-input w-auto min-w-[130px]"
        >
          <option value="all">All Providers</option>
          {providers.map((p) => (
            <option key={p} value={p}>{p}</option>
          ))}
        </select>
        {/* Price range */}
        <select
          value={priceRange}
          onChange={(e) => setPriceRange(e.target.value as PriceRange)}
          className="field-input w-auto min-w-[110px]"
        >
          <option value="all">Any Price</option>
          <option value="free">Free</option>
          <option value="low">1-50 nERG</option>
          <option value="mid">51-200 nERG</option>
          <option value="high">200+ nERG</option>
        </select>
        {/* Sort */}
        <select
          value={sortBy}
          onChange={(e) => setSortBy(e.target.value as SortKey)}
          className="field-input w-auto min-w-[130px]"
        >
          <option value="popularity">Most Popular</option>
          <option value="price">Lowest Price</option>
          <option value="latency">Fastest</option>
          <option value="ponw">Highest PoNW</option>
          <option value="newest">Newest</option>
        </select>
      </div>

      {/* Results count */}
      <div className="text-xs text-surface-800/50">
        {loading ? "Loading..." : `${filtered.length} model${filtered.length !== 1 ? "s" : ""} found`}
      </div>

      {/* Loading skeletons */}
      {loading && (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {Array.from({ length: 6 }, (_, i) => (
            <RegistryCardSkeleton key={i} />
          ))}
        </div>
      )}

      {/* Empty state */}
      {!loading && filtered.length === 0 && (
        <div className="flex flex-col items-center justify-center py-16 text-center space-y-3">
          <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" className="text-surface-300">
            <circle cx="11" cy="11" r="8" /><line x1="21" y1="21" x2="16.65" y2="16.65" />
          </svg>
          <p className="font-medium text-surface-800/70">No models found</p>
          <p className="text-sm text-surface-800/50">Try adjusting your filters or search terms</p>
          <button
            type="button"
            onClick={() => { setSearch(""); setCategory("all"); setProvider("all"); setPriceRange("all"); }}
            className="text-sm font-medium text-brand-600 hover:text-brand-700 transition-colors"
          >
            Clear all filters
          </button>
        </div>
      )}

      {/* Model grid */}
      {!loading && paginated.length > 0 && (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {paginated.map((model) => {
            const isExpanded = expandedId === model.id;
            return (
              <div key={model.id} className="space-y-0">
                <div
                  className="rounded-xl border border-surface-200 bg-surface-0 p-4 space-y-3 hover:border-brand-300 transition-colors cursor-pointer"
                  onClick={() => setExpandedId(isExpanded ? null : model.id)}
                >
                  {/* Header */}
                  <div className="flex items-start justify-between">
                    <div className="flex items-center gap-2">
                      <h3 className="text-sm font-semibold text-surface-900">{model.name}</h3>
                      {model.verified && <VerifiedBadge />}
                    </div>
                    <span className="text-xs text-surface-800/40">{model.version}</span>
                  </div>

                  {/* Description */}
                  <p className="text-xs text-surface-800/60 line-clamp-2">{model.description}</p>

                  {/* Tags */}
                  <div className="flex flex-wrap gap-1">
                    <span className={`inline-flex items-center rounded-md px-2 py-0.5 text-xs font-medium ${categoryColor(model.category)}`}>
                      {model.category}
                    </span>
                    {model.capabilities.slice(0, 2).map((c) => (
                      <span key={c} className="inline-flex items-center rounded-md bg-surface-100 px-2 py-0.5 text-xs text-surface-800/70 dark:bg-surface-200">
                        {c}
                      </span>
                    ))}
                    {model.capabilities.length > 2 && (
                      <span className="text-xs text-surface-800/40">+{model.capabilities.length - 2}</span>
                    )}
                  </div>

                  {/* Metrics */}
                  <div className="grid grid-cols-2 gap-2 text-xs">
                    <div className="rounded-lg bg-surface-50 p-2 dark:bg-surface-100/50">
                      <span className="text-surface-800/40">Pricing</span>
                      <p className="font-medium text-surface-900">
                        {model.pricing === 0 ? "Free" : `${model.pricing} nERG/1K`}
                      </p>
                    </div>
                    <div className="rounded-lg bg-surface-50 p-2 dark:bg-surface-100/50">
                      <span className="text-surface-800/40">PoNW Score</span>
                      <p className="font-medium text-surface-900">{model.ponwScore}/100</p>
                    </div>
                    <div className="rounded-lg bg-surface-50 p-2 dark:bg-surface-100/50">
                      <span className="text-surface-800/40">Requests</span>
                      <p className="font-medium text-surface-900">{formatNumber(model.requestCount)}</p>
                    </div>
                    <div className="rounded-lg bg-surface-50 p-2 dark:bg-surface-100/50">
                      <span className="text-surface-800/40">Latency P95</span>
                      <p className="font-medium text-surface-900">{model.latencyP95}ms</p>
                    </div>
                  </div>

                  {/* Footer */}
                  <div className="flex items-center justify-between pt-1">
                    <span className="text-xs text-surface-800/40">{model.provider}</span>
                    <div className="relative">
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          setShowDeployTip(showDeployTip === model.id ? null : model.id);
                        }}
                        className="inline-flex items-center gap-1 rounded-lg bg-brand-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-brand-700 transition-colors"
                      >
                        <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>
                        Deploy
                      </button>
                      {showDeployTip === model.id && (
                        <div className="absolute bottom-full right-0 mb-2 w-64 rounded-lg border border-surface-200 bg-surface-0 p-3 shadow-lg z-10">
                          <p className="text-xs font-mono text-surface-800 break-all">
                            xergon deploy {model.id} --provider {model.provider}
                          </p>
                          <p className="text-xs text-surface-800/40 mt-1">Click to copy</p>
                        </div>
                      )}
                    </div>
                  </div>
                </div>

                {/* Expanded detail */}
                {isExpanded && (
                  <div className="rounded-b-xl border border-t-0 border-surface-200 bg-surface-0 p-4 space-y-4 animate-fade-in">
                    {/* Version history */}
                    <div>
                      <h4 className="text-xs font-semibold text-surface-900 mb-2">Version History</h4>
                      <div className="space-y-2">
                        {model.versions.map((v) => (
                          <div key={v.version} className="flex items-start gap-2 text-xs">
                            <span className="font-mono font-medium text-brand-600 whitespace-nowrap">{v.version}</span>
                            <span className="text-surface-800/40 whitespace-nowrap">{formatDate(v.date)}</span>
                            <span className="text-surface-800/60">{v.changes}</span>
                          </div>
                        ))}
                      </div>
                    </div>

                    {/* Provider info */}
                    <div>
                      <h4 className="text-xs font-semibold text-surface-900 mb-2">Provider Info</h4>
                      <div className="grid grid-cols-2 gap-2 text-xs">
                        <div>
                          <span className="text-surface-800/40">Endpoint</span>
                          <p className="font-mono text-surface-800">{model.providerInfo.endpoint}</p>
                        </div>
                        <div>
                          <span className="text-surface-800/40">Region</span>
                          <p className="text-surface-800">{model.providerInfo.region}</p>
                        </div>
                        <div>
                          <span className="text-surface-800/40">Uptime</span>
                          <p className="text-surface-800">{model.providerInfo.uptime}%</p>
                        </div>
                        <div>
                          <span className="text-surface-800/40">Models Hosted</span>
                          <p className="text-surface-800">{model.providerInfo.modelsCount}</p>
                        </div>
                      </div>
                    </div>

                    {/* Benchmarks */}
                    <div>
                      <h4 className="text-xs font-semibold text-surface-900 mb-2">Benchmarks</h4>
                      <div className="flex gap-2 flex-wrap">
                        {model.benchmarks.map((b) => (
                          <div key={b.name} className="rounded-lg border border-surface-200 px-3 py-2 text-center min-w-[90px]">
                            <p className="text-xs text-surface-800/40">{b.name}</p>
                            <p className="text-sm font-semibold text-surface-900">{b.score}</p>
                            <p className={`text-xs ${b.delta.startsWith("+") ? "text-accent-500" : b.delta.startsWith("-") ? "text-danger-500" : "text-surface-800/40"}`}>
                              {b.delta}
                            </p>
                          </div>
                        ))}
                      </div>
                    </div>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {/* Pagination */}
      {!loading && totalPages > 1 && (
        <div className="flex items-center justify-center gap-2 pt-4">
          <button
            type="button"
            disabled={page <= 1}
            onClick={() => setPage((p) => p - 1)}
            className="inline-flex items-center gap-1 rounded-lg border border-surface-200 px-3 py-1.5 text-xs font-medium text-surface-800 hover:bg-surface-50 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><polyline points="15 18 9 12 15 6"/></svg>
            Prev
          </button>
          {Array.from({ length: totalPages }, (_, i) => i + 1).map((p) => (
            <button
              key={p}
              type="button"
              onClick={() => setPage(p)}
              className={`inline-flex items-center justify-center w-8 h-8 rounded-lg text-xs font-medium transition-colors ${
                p === page
                  ? "bg-brand-600 text-white"
                  : "border border-surface-200 text-surface-800 hover:bg-surface-50"
              }`}
            >
              {p}
            </button>
          ))}
          <button
            type="button"
            disabled={page >= totalPages}
            onClick={() => setPage((p) => p + 1)}
            className="inline-flex items-center gap-1 rounded-lg border border-surface-200 px-3 py-1.5 text-xs font-medium text-surface-800 hover:bg-surface-50 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          >
            Next
            <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><polyline points="9 18 15 12 9 6"/></svg>
          </button>
        </div>
      )}
    </main>
  );
}

// ---------------------------------------------------------------------------
// Page component
// ---------------------------------------------------------------------------

export default function RegistryPage() {
  return (
    <Suspense
      fallback={
        <main className="mx-auto max-w-6xl px-4 py-6 space-y-6">
          <div className="animate-pulse space-y-2">
            <div className="h-7 w-40 rounded bg-surface-200" />
            <div className="h-4 w-72 rounded bg-surface-200" />
          </div>
          <StatsSkeleton />
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {Array.from({ length: 6 }, (_, i) => (
              <RegistryCardSkeleton key={i} />
            ))}
          </div>
        </main>
      }
    >
      <RegistryContent />
    </Suspense>
  );
}
