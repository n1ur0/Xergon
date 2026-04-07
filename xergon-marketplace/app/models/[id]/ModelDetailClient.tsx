"use client";

import { useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { cn } from "@/lib/utils";

// ── Types ──

interface ModelData {
  id: string;
  name: string;
  provider: string;
  tier: string;
  description?: string;
  contextWindow?: number;
  speed?: string;
  tags: string[];
  freeTier: boolean;
  pricePerInputTokenNanoerg: number;
  pricePerOutputTokenNanoerg: number;
  effectivePriceNanoerg?: number;
  providerCount: number;
  available: boolean;
}

interface ModelDetailClientProps {
  model: ModelData;
}

// ── Helpers ──

function formatContextWindow(tokens: number): string {
  if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(tokens % 1_000_000 === 0 ? 0 : 1)}M`;
  if (tokens >= 1_000) return `${(tokens / 1_000).toFixed(0)}K`;
  return String(tokens);
}

function nanoergToErg(nanoerg: number): string {
  if (nanoerg <= 0) return "0";
  const erg = nanoerg / 1e9;
  return erg.toFixed(6).replace(/0+$/, "").replace(/\.$/, "");
}

function formatPricePer1K(nanoergPerToken: number): string {
  if (nanoergPerToken <= 0) return "Free";
  const nanoergPer1K = nanoergPerToken * 1000;
  return `${nanoergToErg(nanoergPer1K)} ERG`;
}

const TAG_STYLES: Record<string, string> = {
  Fast: "bg-amber-100 text-amber-700",
  Smart: "bg-violet-100 text-violet-700",
  Code: "bg-blue-100 text-blue-700",
  Creative: "bg-pink-100 text-pink-700",
  Free: "bg-emerald-100 text-emerald-700",
};

const SPEED_CONFIG = {
  fast: { label: "Fast", color: "text-green-600 bg-green-50" },
  balanced: { label: "Balanced", color: "text-amber-600 bg-amber-50" },
  slow: { label: "Thorough", color: "text-blue-600 bg-blue-50" },
} as const;

// Mock related models
const RELATED_MODELS = [
  { id: "llama-3.1-8b", name: "Llama 3.1 8B", provider: "Meta", price: "Free" },
  { id: "mistral-small-24b", name: "Mistral Small 24B", provider: "Mistral AI", price: "Free" },
  { id: "qwen3.5-4b-f16.gguf", name: "Qwen 3.5 4B (F16)", provider: "Alibaba", price: "0.05 ERG/1K" },
];

// Mock reviews
const MOCK_REVIEWS = [
  {
    id: "1",
    author: "0x3f8a...b2c1",
    rating: 5,
    content: "Excellent model for general-purpose tasks. Fast response times and good quality output.",
    date: "2025-12-18",
  },
  {
    id: "2",
    author: "0x7d2e...f4a9",
    rating: 4,
    content: "Solid performance for coding tasks. Slightly slower than some alternatives but quality is good.",
    date: "2025-12-15",
  },
];

// Mock benchmarks
const MOCK_BENCHMARKS = [
  { name: "MMLU", score: 82 },
  { name: "HumanEval", score: 76 },
  { name: "GSM8K", score: 91 },
  { name: "ARC-C", score: 85 },
];

// ── Star Rating ──

function StarRating({ rating }: { rating: number }) {
  return (
    <div className="flex items-center gap-0.5">
      {[1, 2, 3, 4, 5].map((star) => (
        <svg
          key={star}
          xmlns="http://www.w3.org/2000/svg"
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill={star <= rating ? "currentColor" : "none"}
          stroke="currentColor"
          strokeWidth="2"
          className={star <= rating ? "text-amber-400" : "text-surface-300"}
        >
          <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
        </svg>
      ))}
    </div>
  );
}

// ── Component ──

export function ModelDetailClient({ model }: ModelDetailClientProps) {
  const router = useRouter();
  const [prompt, setPrompt] = useState("");
  const [response, setResponse] = useState("");
  const [isGenerating, setIsGenerating] = useState(false);
  const [activeTab, setActiveTab] = useState<"playground" | "benchmarks">("playground");

  const speedConfig = SPEED_CONFIG[model.speed as keyof typeof SPEED_CONFIG] ?? SPEED_CONFIG.balanced;

  const handleTryIt = () => {
    router.push(`/playground?model=${encodeURIComponent(model.id)}`);
  };

  const handleInlinePlayground = async () => {
    if (!prompt.trim() || isGenerating) return;
    setIsGenerating(true);
    setResponse("");

    try {
      const res = await fetch("/api/xergon-relay/chat/completions", {
        method: "POST",
        headers: { "Content-Type": "application/json", Accept: "text/event-stream" },
        body: JSON.stringify({
          model: model.id,
          messages: [{ role: "user", content: prompt.trim() }],
          stream: true,
        }),
      });

      if (!res.ok) {
        setResponse(`Error: Model returned status ${res.status}. Try the full playground.`);
        return;
      }

      const reader = res.body?.getReader();
      if (!reader) {
        setResponse("Error: No response stream.");
        return;
      }

      const decoder = new TextDecoder();
      let accumulated = "";
      let buffer = "";

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split("\n");
        buffer = lines.pop() || "";

        for (const line of lines) {
          const trimmed = line.trim();
          if (!trimmed || trimmed === "data: [DONE]" || !trimmed.startsWith("data: ")) continue;
          try {
            const json = JSON.parse(trimmed.slice(6));
            const content = json.choices?.[0]?.delta?.content;
            if (content) {
              accumulated += content;
              setResponse(accumulated);
            }
          } catch {
            // skip
          }
        }
      }

      if (!accumulated) setResponse("(No response content received)");
    } catch {
      setResponse("Error: Failed to get response. Try the full playground.");
    } finally {
      setIsGenerating(false);
    }
  };

  return (
    <div className="max-w-6xl mx-auto px-4 py-8">
      {/* Back link */}
      <Link
        href="/models"
        className="inline-flex items-center gap-1.5 text-sm text-surface-800/50 hover:text-surface-800/80 mb-6 transition-colors"
      >
        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          <path d="m12 19-7-7 7-7" />
          <path d="M19 12H5" />
        </svg>
        Back to Models
      </Link>

      {/* Model Header */}
      <div className="rounded-xl border border-surface-200 bg-surface-0 p-6 mb-6">
        <div className="flex flex-col sm:flex-row sm:items-start gap-4">
          <div className="flex items-center justify-center h-16 w-16 rounded-xl bg-brand-100 text-brand-700 text-xl font-bold shrink-0">
            {model.name.slice(0, 2).toUpperCase()}
          </div>

          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2 flex-wrap mb-1">
              <h1 className="text-xl font-bold text-surface-900">{model.name}</h1>
              {model.freeTier && (
                <span className="rounded-full bg-emerald-500 px-2.5 py-0.5 text-xs font-semibold text-white">
                  Free
                </span>
              )}
              {model.speed && (
                <span className={cn("rounded-full px-2 py-0.5 text-xs font-medium", speedConfig.color)}>
                  {speedConfig.label}
                </span>
              )}
            </div>

            <div className="flex items-center gap-2 text-xs text-surface-800/40 mb-2">
              <span>by {model.provider}</span>
              <span className="text-surface-200">|</span>
              <span className="capitalize">{model.tier} tier</span>
            </div>

            {model.description && (
              <p className="text-sm text-surface-800/60">{model.description}</p>
            )}

            {/* Tags */}
            {model.tags.length > 0 && (
              <div className="flex flex-wrap gap-1 mt-3">
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
          </div>

          {/* Try It button */}
          <button
            onClick={handleTryIt}
            className="rounded-lg bg-brand-600 px-4 py-2.5 text-sm font-medium text-white hover:bg-brand-700 transition-colors shrink-0"
          >
            Open in Playground
          </button>
        </div>
      </div>

      {/* Stats */}
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-4 mb-6">
        <StatCard
          label="Price / 1K tokens"
          value={formatPricePer1K(model.effectivePriceNanoerg ?? model.pricePerInputTokenNanoerg)}
        />
        <StatCard
          label="Context Window"
          value={model.contextWindow ? `${formatContextWindow(model.contextWindow)} tokens` : "N/A"}
        />
        <StatCard
          label="Providers"
          value={`${model.providerCount} available`}
        />
        <StatCard
          label="Status"
          value={model.available ? "Available" : "Unavailable"}
          valueColor={model.available ? "text-green-600" : "text-red-600"}
        />
      </div>

      {/* Tabs: Playground / Benchmarks */}
      <div className="mb-6">
        <div className="flex items-center border-b border-surface-200 mb-4">
          {(["playground", "benchmarks"] as const).map((tab) => (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              className={cn(
                "px-4 py-2.5 text-sm font-medium transition-colors relative capitalize",
                activeTab === tab
                  ? "text-brand-600"
                  : "text-surface-800/40 hover:text-surface-800/70",
              )}
            >
              {tab}
              {activeTab === tab && (
                <span className="absolute bottom-0 left-0 right-0 h-0.5 bg-brand-600" />
              )}
            </button>
          ))}
        </div>

        {activeTab === "playground" ? (
          <div className="rounded-xl border border-surface-200 bg-surface-0 overflow-hidden">
            <div className="p-4 border-b border-surface-100">
              <textarea
                value={prompt}
                onChange={(e) => setPrompt(e.target.value)}
                placeholder={`Try ${model.name} with a prompt...`}
                rows={3}
                className="w-full resize-none rounded-lg border border-surface-200 bg-surface-0 px-3 py-2 text-sm placeholder:text-surface-800/30 focus:outline-none focus:ring-2 focus:ring-brand-500/40 focus:border-brand-500"
              />
              <div className="flex justify-end mt-2">
                <button
                  onClick={handleInlinePlayground}
                  disabled={!prompt.trim() || isGenerating}
                  className="rounded-lg bg-brand-600 px-4 py-2 text-sm font-medium text-white hover:bg-brand-700 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
                >
                  {isGenerating ? "Generating..." : "Generate"}
                </button>
              </div>
            </div>

            {response && (
              <div className="p-4 min-h-[120px] max-h-[400px] overflow-y-auto">
                <div className="text-sm text-surface-800/70 whitespace-pre-wrap">{response}</div>
              </div>
            )}

            {isGenerating && !response && (
              <div className="p-4 flex items-center gap-1.5 text-surface-800/40">
                <span className="inline-block h-1.5 w-1.5 rounded-full bg-brand-500 animate-bounce [animation-delay:0ms]" />
                <span className="inline-block h-1.5 w-1.5 rounded-full bg-brand-500 animate-bounce [animation-delay:150ms]" />
                <span className="inline-block h-1.5 w-1.5 rounded-full bg-brand-500 animate-bounce [animation-delay:300ms]" />
                <span className="ml-1 text-xs">Thinking...</span>
              </div>
            )}

            {!response && !isGenerating && (
              <div className="p-8 text-center text-sm text-surface-800/30">
                Enter a prompt above to try this model, or{" "}
                <button
                  onClick={handleTryIt}
                  className="text-brand-600 hover:text-brand-700 font-medium"
                >
                  open the full playground
                </button>
                .
              </div>
            )}
          </div>
        ) : (
          <div className="rounded-xl border border-surface-200 bg-surface-0 p-6">
            <h3 className="text-sm font-medium text-surface-900 mb-4">Performance Benchmarks</h3>
            <div className="space-y-3">
              {MOCK_BENCHMARKS.map((bench) => (
                <div key={bench.name} className="flex items-center gap-3">
                  <span className="text-xs text-surface-800/50 w-20">{bench.name}</span>
                  <div className="flex-1 h-2 rounded-full bg-surface-200 overflow-hidden">
                    <div
                      className="h-full rounded-full bg-brand-500 transition-all duration-500"
                      style={{ width: `${bench.score}%` }}
                    />
                  </div>
                  <span className="text-xs font-medium text-surface-800/70 w-10 text-right">
                    {bench.score}%
                  </span>
                </div>
              ))}
            </div>
            <p className="text-xs text-surface-800/30 mt-4">
              Benchmark scores are illustrative. Real-world performance may vary by provider.
            </p>
          </div>
        )}
      </div>

      {/* Reviews + Related Models */}
      <div className="grid gap-6 lg:grid-cols-2">
        {/* Reviews */}
        <section>
          <h2 className="text-lg font-semibold text-surface-900 mb-4">Reviews</h2>
          <div className="space-y-4">
            {MOCK_REVIEWS.map((review) => (
              <div
                key={review.id}
                className="rounded-lg border border-surface-200 bg-surface-0 p-4"
              >
                <div className="flex items-center justify-between mb-2">
                  <div className="flex items-center gap-2">
                    <span className="text-xs font-mono text-surface-800/60">
                      {review.author}
                    </span>
                    <StarRating rating={review.rating} />
                  </div>
                  <span className="text-xs text-surface-800/30">{review.date}</span>
                </div>
                <p className="text-sm text-surface-800/70">{review.content}</p>
              </div>
            ))}
          </div>
        </section>

        {/* Related Models */}
        <section>
          <h2 className="text-lg font-semibold text-surface-900 mb-4">Related Models</h2>
          <div className="space-y-3">
            {RELATED_MODELS.filter((m) => m.id !== model.id).map((related) => (
              <Link
                key={related.id}
                href={`/models/${encodeURIComponent(related.id)}`}
                className="group flex items-center justify-between rounded-lg border border-surface-200 bg-surface-0 p-4 hover:border-brand-300 hover:shadow-sm transition-all"
              >
                <div>
                  <h3 className="font-medium text-surface-900 group-hover:text-brand-600 transition-colors">
                    {related.name}
                  </h3>
                  <p className="text-xs text-surface-800/40">by {related.provider}</p>
                </div>
                <span className="text-xs font-medium text-surface-800/50">{related.price}</span>
              </Link>
            ))}
          </div>
        </section>
      </div>
    </div>
  );
}

// ── Stat Card ──

function StatCard({
  label,
  value,
  valueColor,
}: {
  label: string;
  value: string;
  valueColor?: string;
}) {
  return (
    <div className="rounded-lg border border-surface-200 bg-surface-0 p-4">
      <div className="text-xs text-surface-800/40 mb-1">{label}</div>
      <div className={cn("text-lg font-semibold", valueColor ?? "text-surface-900")}>
        {value}
      </div>
    </div>
  );
}
