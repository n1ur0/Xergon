"use client";

import { useState, useMemo } from "react";
import Link from "next/link";

interface Model {
  id: string;
  name: string;
  description: string;
  provider: string;
  contextWindow: number;
  inputPrice: number;
  outputPrice: number;
  features: string[];
  status: "active" | "beta" | "coming_soon";
}

const MODELS: Model[] = [
  {
    id: "llama-3.1-8b",
    name: "Llama 3.1 8B",
    description: "Meta's efficient 8B parameter model. Great for general-purpose tasks, summarization, and code assistance.",
    provider: "Meta",
    contextWindow: 131072,
    inputPrice: 0.0001,
    outputPrice: 0.0002,
    features: ["streaming", "function_calling"],
    status: "active",
  },
  {
    id: "llama-3.1-70b",
    name: "Llama 3.1 70B",
    description: "High-quality 70B parameter model with strong reasoning capabilities. Ideal for complex tasks.",
    provider: "Meta",
    contextWindow: 131072,
    inputPrice: 0.0004,
    outputPrice: 0.0008,
    features: ["streaming", "function_calling", "vision"],
    status: "active",
  },
  {
    id: "llama-3.3-70b",
    name: "Llama 3.3 70B",
    description: "Improved 70B variant with enhanced instruction following and multilingual support.",
    provider: "Meta",
    contextWindow: 131072,
    inputPrice: 0.0004,
    outputPrice: 0.0008,
    features: ["streaming", "function_calling"],
    status: "active",
  },
  {
    id: "mixtral-8x7b",
    name: "Mixtral 8x7B",
    description: "Mistral's mixture-of-experts model. Efficient inference with excellent quality-to-cost ratio.",
    provider: "Mistral AI",
    contextWindow: 32768,
    inputPrice: 0.0003,
    outputPrice: 0.0006,
    features: ["streaming", "function_calling"],
    status: "active",
  },
  {
    id: "mistral-7b",
    name: "Mistral 7B v0.3",
    description: "Fast and efficient 7B model. Excellent for quick responses and real-time applications.",
    provider: "Mistral AI",
    contextWindow: 32768,
    inputPrice: 0.0001,
    outputPrice: 0.0002,
    features: ["streaming"],
    status: "active",
  },
  {
    id: "qwen-2.5-72b",
    name: "Qwen 2.5 72B",
    description: "Alibaba's powerful 72B model with strong multilingual and coding capabilities.",
    provider: "Alibaba",
    contextWindow: 131072,
    inputPrice: 0.0004,
    outputPrice: 0.0008,
    features: ["streaming", "function_calling", "vision"],
    status: "active",
  },
  {
    id: "deepseek-coder-v2",
    name: "DeepSeek Coder V2",
    description: "Specialized code generation model with excellent performance on programming benchmarks.",
    provider: "DeepSeek",
    contextWindow: 131072,
    inputPrice: 0.0003,
    outputPrice: 0.0006,
    features: ["streaming", "function_calling"],
    status: "active",
  },
  {
    id: "phi-4",
    name: "Phi-4",
    description: "Microsoft's small but mighty model. Outstanding quality for its 14B parameter size.",
    provider: "Microsoft",
    contextWindow: 16384,
    inputPrice: 0.0001,
    outputPrice: 0.0002,
    features: ["streaming"],
    status: "active",
  },
  {
    id: "llama-3.1-405b",
    name: "Llama 3.1 405B",
    description: "Meta's frontier-class 405B model. Maximum quality for the most demanding tasks.",
    provider: "Meta",
    contextWindow: 131072,
    inputPrice: 0.001,
    outputPrice: 0.002,
    features: ["streaming", "function_calling", "vision"],
    status: "beta",
  },
  {
    id: "command-r-plus",
    name: "Command R+",
    description: "Cohere's enterprise model optimized for RAG, tool use, and grounded generation.",
    provider: "Cohere",
    contextWindow: 131072,
    inputPrice: 0.0005,
    outputPrice: 0.001,
    features: ["streaming", "function_calling"],
    status: "coming_soon",
  },
];

function formatTokens(n: number): string {
  if (n >= 1000) return `${(n / 1000).toFixed(0)}K`;
  return n.toString();
}

function FeatureBadge({ feature }: { feature: string }) {
  const labels: Record<string, string> = {
    streaming: "Streaming",
    function_calling: "Functions",
    vision: "Vision",
  };
  return (
    <span className="inline-flex items-center px-2 py-0.5 rounded-full text-[11px] font-medium bg-surface-100 text-surface-800/60 dark:bg-surface-200/20 dark:text-surface-800/60">
      {labels[feature] || feature}
    </span>
  );
}

function StatusBadge({ status }: { status: string }) {
  const config: Record<string, { label: string; cls: string }> = {
    active: { label: "Active", cls: "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400" },
    beta: { label: "Beta", cls: "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400" },
    coming_soon: { label: "Coming Soon", cls: "bg-surface-100 text-surface-800/40" },
  };
  const c = config[status] || config.active;
  return (
    <span className={`inline-flex items-center px-2 py-0.5 rounded-full text-[11px] font-medium ${c.cls}`}>
      {c.label}
    </span>
  );
}

export default function ModelsPage() {
  const [search, setSearch] = useState("");
  const [featureFilter, setFeatureFilter] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<"cards" | "table">("cards");

  const allFeatures = useMemo(
    () => Array.from(new Set(MODELS.flatMap((m) => m.features))),
    []
  );

  const filtered = useMemo(() => {
    return MODELS.filter((m) => {
      const matchesSearch =
        !search ||
        m.name.toLowerCase().includes(search.toLowerCase()) ||
        m.id.toLowerCase().includes(search.toLowerCase()) ||
        m.description.toLowerCase().includes(search.toLowerCase());
      const matchesFeature = !featureFilter || m.features.includes(featureFilter);
      return matchesSearch && matchesFeature;
    });
  }, [search, featureFilter]);

  return (
    <div className="space-y-8">
      <section>
        <h1 className="text-3xl font-bold text-surface-900 mb-2">Model Catalog</h1>
        <p className="text-lg text-surface-800/60">
          Browse available models, compare features, and find the right model for
          your application.
        </p>
      </section>

      {/* Filters */}
      <section className="space-y-3">
        <div className="flex flex-col sm:flex-row gap-3">
          <div className="relative flex-1">
            <svg
              className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-surface-800/30"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <circle cx="11" cy="11" r="8" />
              <line x1="21" y1="21" x2="16.65" y2="16.65" />
            </svg>
            <input
              type="text"
              placeholder="Search models..."
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="w-full pl-10 pr-4 py-2 rounded-lg border border-surface-200 text-sm bg-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500/30 focus:border-brand-500"
            />
          </div>
          <div className="flex gap-2">
            {allFeatures.map((f) => (
              <button
                key={f}
                onClick={() =>
                  setFeatureFilter(featureFilter === f ? null : f)
                }
                className={`px-3 py-2 rounded-lg text-xs font-medium transition-colors ${
                  featureFilter === f
                    ? "bg-brand-600 text-white"
                    : "bg-surface-100 text-surface-800/60 hover:text-surface-900"
                }`}
              >
                {f === "streaming" ? "Streaming" : f === "function_calling" ? "Functions" : "Vision"}
              </button>
            ))}
          </div>
        </div>
        <div className="flex items-center justify-between">
          <span className="text-sm text-surface-800/40">
            {filtered.length} model{filtered.length !== 1 ? "s" : ""}
          </span>
          <div className="inline-flex rounded-lg border border-surface-200 overflow-hidden">
            <button
              onClick={() => setViewMode("cards")}
              className={`px-3 py-1.5 text-xs transition-colors ${
                viewMode === "cards"
                  ? "bg-surface-900 text-white dark:bg-surface-200 dark:text-surface-900"
                  : "text-surface-800/50 hover:text-surface-900"
              }`}
            >
              Cards
            </button>
            <button
              onClick={() => setViewMode("table")}
              className={`px-3 py-1.5 text-xs transition-colors ${
                viewMode === "table"
                  ? "bg-surface-900 text-white dark:bg-surface-200 dark:text-surface-900"
                  : "text-surface-800/50 hover:text-surface-900"
              }`}
            >
              Table
            </button>
          </div>
        </div>
      </section>

      {/* Card view */}
      {viewMode === "cards" && (
        <div className="grid sm:grid-cols-2 gap-4">
          {filtered.map((model) => (
            <div
              key={model.id}
              className="rounded-xl border border-surface-200 p-5 bg-surface-0 hover:border-brand-300 hover:shadow-sm transition-all"
            >
              <div className="flex items-start justify-between mb-3">
                <div>
                  <h3 className="font-semibold text-surface-900">{model.name}</h3>
                  <code className="text-xs font-mono text-surface-800/40">
                    {model.id}
                  </code>
                </div>
                <StatusBadge status={model.status} />
              </div>
              <p className="text-sm text-surface-800/60 mb-4 line-clamp-2">
                {model.description}
              </p>
              <div className="flex flex-wrap gap-1.5 mb-4">
                {model.features.map((f) => (
                  <FeatureBadge key={f} feature={f} />
                ))}
              </div>
              <div className="grid grid-cols-3 gap-3 text-center border-t border-surface-200 pt-4">
                <div>
                  <div className="text-xs text-surface-800/40 mb-0.5">Context</div>
                  <div className="text-sm font-semibold text-surface-900">
                    {formatTokens(model.contextWindow)}
                  </div>
                </div>
                <div>
                  <div className="text-xs text-surface-800/40 mb-0.5">Input / 1K</div>
                  <div className="text-sm font-semibold text-surface-900">
                    {model.inputPrice.toFixed(4)}
                  </div>
                </div>
                <div>
                  <div className="text-xs text-surface-800/40 mb-0.5">Output / 1K</div>
                  <div className="text-sm font-semibold text-surface-900">
                    {model.outputPrice.toFixed(4)}
                  </div>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Table view */}
      {viewMode === "table" && (
        <div className="overflow-x-auto rounded-xl border border-surface-200">
          <table className="w-full text-sm">
            <thead>
              <tr className="bg-surface-50 text-left">
                <th className="px-4 py-3 font-medium text-surface-800/60">Model</th>
                <th className="px-4 py-3 font-medium text-surface-800/60">Provider</th>
                <th className="px-4 py-3 font-medium text-surface-800/60">Context</th>
                <th className="px-4 py-3 font-medium text-surface-800/60">Input / 1K</th>
                <th className="px-4 py-3 font-medium text-surface-800/60">Output / 1K</th>
                <th className="px-4 py-3 font-medium text-surface-800/60">Features</th>
                <th className="px-4 py-3 font-medium text-surface-800/60">Status</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-surface-200">
              {filtered.map((model) => (
                <tr key={model.id} className="hover:bg-surface-50 transition-colors">
                  <td className="px-4 py-3">
                    <div className="font-medium text-surface-900">{model.name}</div>
                    <div className="text-xs font-mono text-surface-800/40">{model.id}</div>
                  </td>
                  <td className="px-4 py-3 text-surface-800/60">{model.provider}</td>
                  <td className="px-4 py-3 text-surface-800/60">{formatTokens(model.contextWindow)}</td>
                  <td className="px-4 py-3 font-mono text-surface-800/60">{model.inputPrice.toFixed(4)}</td>
                  <td className="px-4 py-3 font-mono text-surface-800/60">{model.outputPrice.toFixed(4)}</td>
                  <td className="px-4 py-3">
                    <div className="flex flex-wrap gap-1">
                      {model.features.map((f) => (
                        <FeatureBadge key={f} feature={f} />
                      ))}
                    </div>
                  </td>
                  <td className="px-4 py-3">
                    <StatusBadge status={model.status} />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* Pricing note */}
      <div className="rounded-xl bg-brand-50 dark:bg-brand-950/20 border border-brand-200 dark:border-brand-800/30 p-5 text-sm">
        <h3 className="font-medium text-brand-900 dark:text-brand-200 mb-1">
          Pricing in ERG
        </h3>
        <p className="text-brand-800/60 dark:text-brand-300/60">
          All prices are denominated in ERG. Payments are settled on-chain via the
          Ergo blockchain. Actual costs may vary based on provider availability and
          network conditions. Visit the{" "}
          <Link href="/pricing" className="underline font-medium">
            Pricing page
          </Link>{" "}
          for detailed rate information.
        </p>
      </div>

      <section className="flex gap-3">
        <Link
          href="/docs/api-reference"
          className="px-4 py-2 rounded-lg border border-surface-200 text-sm font-medium text-surface-800 hover:border-brand-300 hover:text-brand-700 transition-colors"
        >
          API Reference
        </Link>
        <Link
          href="/playground"
          className="px-4 py-2 rounded-lg bg-brand-600 text-white text-sm font-medium hover:bg-brand-700 transition-colors"
        >
          Try in Playground
        </Link>
      </section>
    </div>
  );
}
