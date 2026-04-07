"use client";

import { useState, useEffect, useCallback, useMemo } from "react";
import Link from "next/link";
import { cn } from "@/lib/utils";

/* ─── Types ─── */

interface ModelData {
  slug: string;
  name: string;
  description: string;
  version: string;
  license: string;
  provider: string;
  parameterCount: string;
  contextWindow: number;
  maxOutputTokens: number;
  quantization: string[];
  capabilities: {
    chat: boolean;
    completion: boolean;
    embedding: boolean;
    vision: boolean;
    functionCalling: boolean;
    jsonMode: boolean;
    streaming: boolean;
  };
  pricing: { provider: string; inputPer1M: number; outputPer1M: number }[];
  benchmarks: { label: string; latencyP50: string; latencyP95: string; throughput: string; qualityScore: number }[];
  codeExamples: { curl: string; typescript: string; python: string; rust: string };
  tips: string[];
  relatedModels: { slug: string; name: string }[];
  versionHistory: { version: string; date: string; notes: string }[];
  providers: { name: string; region: string; gpuType: string; status: string }[];
}

interface ModelSummary {
  slug: string;
  name: string;
  description: string;
  provider: string;
  parameterCount: string;
  contextWindow: number;
  status: string;
}

type CodeTab = "curl" | "typescript" | "python" | "rust";

/* ─── Capability label map ─── */
const CAP_LABELS: Record<string, string> = {
  chat: "Chat",
  completion: "Completion",
  embedding: "Embedding",
  vision: "Vision",
  functionCalling: "Function Calling",
  jsonMode: "JSON Mode",
  streaming: "Streaming",
};

/* ─── Code block with copy ─── */
function CodeBlock({ code, lang }: { code: string; lang: string }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    navigator.clipboard.writeText(code);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="relative">
      <div className="absolute top-2 right-2 flex items-center gap-2">
        <span className="text-[10px] font-mono text-surface-800/30 uppercase tracking-wider">
          {lang}
        </span>
        <button
          onClick={handleCopy}
          className="px-2 py-1 text-[10px] font-mono rounded bg-surface-800/20 text-surface-800/50 hover:bg-surface-800/30 transition-colors"
        >
          {copied ? "Copied!" : "Copy"}
        </button>
      </div>
      <pre className="bg-surface-950 text-surface-900 dark:text-surface-200 rounded-xl p-4 overflow-x-auto text-sm font-mono leading-relaxed pr-24">
        <code>{code}</code>
      </pre>
    </div>
  );
}

/* ─── CopyButton ─── */
function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  const handleCopy = () => {
    navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };
  return (
    <button
      onClick={handleCopy}
      className="px-2 py-1 text-[10px] font-mono rounded bg-surface-800/20 text-surface-800/50 hover:bg-surface-800/30 transition-colors"
    >
      {copied ? "Copied!" : "Copy"}
    </button>
  );
}

/* ─── Main component ─── */
export default function ModelDocumentation({ slug }: { slug: string }) {
  const [model, setModel] = useState<ModelData | null>(null);
  const [allModels, setAllModels] = useState<ModelSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [codeTab, setCodeTab] = useState<CodeTab>("curl");
  const [activeSection, setActiveSection] = useState<string>("overview");

  /* Sidebar search/filter state */
  const [search, setSearch] = useState("");
  const [capabilityFilter, setCapabilityFilter] = useState<string | null>(null);
  const [providerFilter, setProviderFilter] = useState<string | null>(null);
  const [sortBy, setSortBy] = useState<"name" | "size" | "popularity">("popularity");

  const fetchData = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const [modelRes, listRes] = await Promise.all([
        fetch(`/api/docs/models?slug=${slug}`),
        fetch("/api/docs/models"),
      ]);
      if (!modelRes.ok) throw new Error("Model not found");
      const modelData: ModelData = await modelRes.json();
      const listData = await listRes.json();
      setModel(modelData);
      setAllModels(listData.data || []);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load model");
    } finally {
      setLoading(false);
    }
  }, [slug]);

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  /* Filtered sidebar models */
  const filteredModels = useMemo(() => {
    let result = [...allModels];
    if (search) {
      const q = search.toLowerCase();
      result = result.filter(
        (m) =>
          m.name.toLowerCase().includes(q) ||
          m.slug.toLowerCase().includes(q) ||
          m.description.toLowerCase().includes(q)
      );
    }
    if (providerFilter) {
      result = result.filter((m) => m.provider.toLowerCase() === providerFilter.toLowerCase());
    }
    if (sortBy === "name") result.sort((a, b) => a.name.localeCompare(b.name));
    else if (sortBy === "size") result.sort((a, b) => (b.contextWindow || 0) - (a.contextWindow || 0));
    return result;
  }, [allModels, search, providerFilter, sortBy]);

  const uniqueProviders = useMemo(
    () => Array.from(new Set(allModels.map((m) => m.provider))),
    [allModels]
  );

  const allCapabilities = Object.keys(CAP_LABELS);

  /* Format context window */
  function formatCtx(n: number): string {
    if (n >= 1000000) return `${(n / 1000000).toFixed(0)}M`;
    if (n >= 1000) return `${(n / 1000).toFixed(0)}K`;
    return n.toString();
  }

  /* Sections */
  const sections = [
    { id: "overview", label: "Overview" },
    { id: "capabilities", label: "Capabilities" },
    { id: "pricing", label: "Pricing" },
    { id: "benchmarks", label: "Benchmarks" },
    { id: "code", label: "Code Examples" },
    { id: "tips", label: "Tips" },
    { id: "providers", label: "Providers" },
    { id: "history", label: "Version History" },
  ];

  if (loading) {
    return (
      <div className="flex items-center justify-center py-20">
        <div className="text-surface-800/40 text-sm">Loading model documentation...</div>
      </div>
    );
  }

  if (error || !model) {
    return (
      <div className="text-center py-20">
        <div className="text-danger-500 font-medium mb-2">Error</div>
        <div className="text-surface-800/60 text-sm">{error || "Model not found"}</div>
        <Link href="/docs/models" className="text-brand-600 text-sm hover:underline mt-2 inline-block">
          &larr; Back to Model Catalog
        </Link>
      </div>
    );
  }

  return (
    <div className="flex gap-8">
      {/* ─── Left sidebar: model search & list ─── */}
      <nav className="hidden xl:block w-64 shrink-0">
        <div className="sticky top-8 space-y-4">
          {/* Search */}
          <div className="relative">
            <svg
              className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-surface-800/30"
              viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"
            >
              <circle cx="11" cy="11" r="8" />
              <line x1="21" y1="21" x2="16.65" y2="16.65" />
            </svg>
            <input
              type="text"
              placeholder="Search models..."
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="w-full pl-10 pr-3 py-2 rounded-lg border border-surface-200 text-sm bg-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500/30 focus:border-brand-500"
            />
          </div>

          {/* Sort */}
          <div>
            <label className="text-[10px] font-semibold uppercase tracking-wider text-surface-800/40 mb-1 block">
              Sort by
            </label>
            <select
              value={sortBy}
              onChange={(e) => setSortBy(e.target.value as "name" | "size" | "popularity")}
              className="w-full px-2 py-1.5 rounded-lg border border-surface-200 text-xs bg-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
            >
              <option value="popularity">Popularity</option>
              <option value="name">Name</option>
              <option value="size">Context Size</option>
            </select>
          </div>

          {/* Provider filter */}
          <div>
            <label className="text-[10px] font-semibold uppercase tracking-wider text-surface-800/40 mb-1 block">
              Provider
            </label>
            <select
              value={providerFilter || ""}
              onChange={(e) => setProviderFilter(e.target.value || null)}
              className="w-full px-2 py-1.5 rounded-lg border border-surface-200 text-xs bg-surface-0 focus:outline-none focus:ring-2 focus:ring-brand-500/30"
            >
              <option value="">All Providers</option>
              {uniqueProviders.map((p) => (
                <option key={p} value={p}>{p}</option>
              ))}
            </select>
          </div>

          {/* Model list */}
          <div>
            <label className="text-[10px] font-semibold uppercase tracking-wider text-surface-800/40 mb-2 block">
              Models ({filteredModels.length})
            </label>
            <ul className="space-y-1 max-h-[50vh] overflow-y-auto">
              {filteredModels.map((m) => (
                <li key={m.slug}>
                  <Link
                    href={`/docs/models/${m.slug}`}
                    className={cn(
                      "block px-2 py-1.5 rounded-lg text-sm transition-colors",
                      m.slug === slug
                        ? "bg-brand-50 text-brand-700 font-medium dark:bg-brand-950/40 dark:text-brand-300"
                        : "text-surface-800/60 hover:text-surface-900 hover:bg-surface-100"
                    )}
                  >
                    <div className="font-medium truncate">{m.name}</div>
                    <div className="text-[11px] text-surface-800/40 font-mono">{m.slug}</div>
                  </Link>
                </li>
              ))}
            </ul>
          </div>
        </div>
      </nav>

      {/* ─── Main content ─── */}
      <div className="flex-1 min-w-0">
        {/* Header */}
        <section className="mb-8">
          <div className="flex items-center gap-3 mb-4">
            <Link
              href="/docs/models"
              className="text-sm text-surface-800/40 hover:text-surface-800/60 transition-colors"
            >
              &larr; Models
            </Link>
            <span className="text-surface-800/20">/</span>
            <span className="text-sm text-surface-800/60">{model.name}</span>
          </div>
          <div className="flex items-start justify-between gap-4">
            <div>
              <h1 className="text-3xl font-bold text-surface-900 mb-2">{model.name}</h1>
              <p className="text-surface-800/60 max-w-2xl">{model.description}</p>
            </div>
            <Link
              href={`/docs/playground?model=${model.slug}`}
              className="shrink-0 px-4 py-2 rounded-lg bg-brand-600 text-white text-sm font-medium hover:bg-brand-700 transition-colors"
            >
              Try it &rarr;
            </Link>
          </div>
        </section>

        {/* Section nav (sticky) */}
        <div className="sticky top-0 z-10 bg-surface-0/80 backdrop-blur-sm py-2 mb-6 -mx-1">
          <div className="flex gap-1 overflow-x-auto">
            {sections.map((s) => (
              <button
                key={s.id}
                onClick={() => {
                  setActiveSection(s.id);
                  document.getElementById(s.id)?.scrollIntoView({ behavior: "smooth", block: "start" });
                }}
                className={cn(
                  "px-3 py-1.5 rounded-lg text-xs font-medium whitespace-nowrap transition-colors",
                  activeSection === s.id
                    ? "bg-brand-50 text-brand-700 dark:bg-brand-950/40 dark:text-brand-300"
                    : "text-surface-800/50 hover:text-surface-900 hover:bg-surface-100"
                )}
              >
                {s.label}
              </button>
            ))}
          </div>
        </div>

        {/* ─── Overview / Model Card ─── */}
        <section id="overview" className="mb-10">
          <h2 className="text-xl font-semibold text-surface-900 mb-4">Model Card</h2>
          <div className="rounded-xl border border-surface-200 p-5 bg-surface-0">
            <div className="grid sm:grid-cols-2 lg:grid-cols-3 gap-4">
              {[
                { label: "Provider", value: model.provider },
                { label: "Version", value: model.version },
                { label: "License", value: model.license },
                { label: "Parameters", value: model.parameterCount },
                { label: "Context Window", value: `${formatCtx(model.contextWindow)} tokens` },
                { label: "Max Output", value: `${formatCtx(model.maxOutputTokens)} tokens` },
              ].map((item) => (
                <div key={item.label}>
                  <div className="text-xs text-surface-800/40 mb-0.5">{item.label}</div>
                  <div className="text-sm font-medium text-surface-900">{item.value}</div>
                </div>
              ))}
            </div>
            <div className="mt-4 pt-4 border-t border-surface-200">
              <div className="text-xs text-surface-800/40 mb-2">Quantization Options</div>
              <div className="flex flex-wrap gap-1.5">
                {model.quantization.map((q) => (
                  <span
                    key={q}
                    className="px-2 py-0.5 rounded-full text-[11px] font-medium bg-surface-100 text-surface-800/60"
                  >
                    {q}
                  </span>
                ))}
              </div>
            </div>
          </div>
        </section>

        {/* ─── Capabilities ─── */}
        <section id="capabilities" className="mb-10">
          <h2 className="text-xl font-semibold text-surface-900 mb-4">Capabilities</h2>
          <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
            {allCapabilities.map((cap) => {
              const enabled = model.capabilities[cap as keyof typeof model.capabilities];
              return (
                <div
                  key={cap}
                  className={cn(
                    "rounded-xl border p-4 text-center transition-colors",
                    enabled
                      ? "border-emerald-200 bg-emerald-50/50 dark:border-emerald-800/30 dark:bg-emerald-950/20"
                      : "border-surface-200 bg-surface-50 opacity-40"
                  )}
                >
                  <div className="text-lg mb-1">{enabled ? "✓" : "—"}</div>
                  <div className="text-xs font-medium text-surface-900">{CAP_LABELS[cap]}</div>
                </div>
              );
            })}
          </div>
        </section>

        {/* ─── Pricing ─── */}
        <section id="pricing" className="mb-10">
          <h2 className="text-xl font-semibold text-surface-900 mb-4">Pricing</h2>
          <div className="overflow-x-auto rounded-xl border border-surface-200">
            <table className="w-full text-sm">
              <thead>
                <tr className="bg-surface-50 text-left">
                  <th className="px-4 py-3 font-medium text-surface-800/60">Provider</th>
                  <th className="px-4 py-3 font-medium text-surface-800/60 text-right">Input / 1M tokens</th>
                  <th className="px-4 py-3 font-medium text-surface-800/60 text-right">Output / 1M tokens</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-surface-200">
                {model.pricing.map((p) => (
                  <tr key={p.provider}>
                    <td className="px-4 py-3 text-surface-900 font-medium">{p.provider}</td>
                    <td className="px-4 py-3 text-right font-mono text-surface-800/60">
                      {p.inputPer1M.toFixed(2)} ERG
                    </td>
                    <td className="px-4 py-3 text-right font-mono text-surface-800/60">
                      {p.outputPer1M.toFixed(2)} ERG
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <p className="text-xs text-surface-800/40 mt-2">
            Prices in ERG. Actual costs may vary by provider and network conditions.
          </p>
        </section>

        {/* ─── Benchmarks ─── */}
        <section id="benchmarks" className="mb-10">
          <h2 className="text-xl font-semibold text-surface-900 mb-4">Performance Benchmarks</h2>
          <div className="overflow-x-auto rounded-xl border border-surface-200">
            <table className="w-full text-sm">
              <thead>
                <tr className="bg-surface-50 text-left">
                  <th className="px-4 py-3 font-medium text-surface-800/60">Task</th>
                  <th className="px-4 py-3 font-medium text-surface-800/60 text-right">Latency P50</th>
                  <th className="px-4 py-3 font-medium text-surface-800/60 text-right">Latency P95</th>
                  <th className="px-4 py-3 font-medium text-surface-800/60 text-right">Throughput</th>
                  <th className="px-4 py-3 font-medium text-surface-800/60 text-right">Quality</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-surface-200">
                {model.benchmarks.map((b) => (
                  <tr key={b.label}>
                    <td className="px-4 py-3 text-surface-900 font-medium">{b.label}</td>
                    <td className="px-4 py-3 text-right font-mono text-surface-800/60">{b.latencyP50}</td>
                    <td className="px-4 py-3 text-right font-mono text-surface-800/60">{b.latencyP95}</td>
                    <td className="px-4 py-3 text-right font-mono text-surface-800/60">{b.throughput}</td>
                    <td className="px-4 py-3 text-right">
                      <span
                        className={cn(
                          "inline-flex items-center px-2 py-0.5 rounded-full text-[11px] font-medium",
                          b.qualityScore >= 85
                            ? "bg-emerald-100 text-emerald-700"
                            : b.qualityScore >= 70
                            ? "bg-amber-100 text-amber-700"
                            : "bg-surface-100 text-surface-800/60"
                        )}
                      >
                        {b.qualityScore}/100
                      </span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>

        {/* ─── Code Examples ─── */}
        <section id="code" className="mb-10">
          <h2 className="text-xl font-semibold text-surface-900 mb-4">Code Examples</h2>
          <div className="mb-3">
            <div className="inline-flex rounded-lg border border-surface-200 overflow-hidden">
              {(["curl", "typescript", "python", "rust"] as CodeTab[]).map((tab) => (
                <button
                  key={tab}
                  onClick={() => setCodeTab(tab)}
                  className={cn(
                    "px-4 py-2 text-sm font-medium transition-colors",
                    codeTab === tab
                      ? "bg-surface-900 text-white dark:bg-surface-200 dark:text-surface-900"
                      : "bg-surface-0 text-surface-800/60 hover:text-surface-900"
                  )}
                >
                  {tab === "typescript" ? "TypeScript" : tab === "python" ? "Python" : tab === "rust" ? "Rust" : "cURL"}
                </button>
              ))}
            </div>
          </div>
          <CodeBlock
            code={model.codeExamples[codeTab]}
            lang={codeTab === "typescript" ? "typescript" : codeTab === "python" ? "python" : codeTab === "rust" ? "rust" : "bash"}
          />
        </section>

        {/* ─── Tips ─── */}
        <section id="tips" className="mb-10">
          <h2 className="text-xl font-semibold text-surface-900 mb-4">Usage Tips</h2>
          <div className="space-y-3">
            {model.tips.map((tip, i) => (
              <div
                key={i}
                className="flex gap-3 items-start rounded-xl border border-surface-200 p-4 bg-surface-0"
              >
                <div className="shrink-0 h-6 w-6 rounded-full bg-brand-50 dark:bg-brand-950/40 flex items-center justify-center text-xs font-bold text-brand-600">
                  {i + 1}
                </div>
                <p className="text-sm text-surface-800/70">{tip}</p>
              </div>
            ))}
          </div>
        </section>

        {/* ─── Providers ─── */}
        <section id="providers" className="mb-10">
          <h2 className="text-xl font-semibold text-surface-900 mb-4">Provider Availability</h2>
          <div className="overflow-x-auto rounded-xl border border-surface-200">
            <table className="w-full text-sm">
              <thead>
                <tr className="bg-surface-50 text-left">
                  <th className="px-4 py-3 font-medium text-surface-800/60">Provider</th>
                  <th className="px-4 py-3 font-medium text-surface-800/60">Region</th>
                  <th className="px-4 py-3 font-medium text-surface-800/60">GPU</th>
                  <th className="px-4 py-3 font-medium text-surface-800/60">Status</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-surface-200">
                {model.providers.map((p) => (
                  <tr key={p.name}>
                    <td className="px-4 py-3 text-surface-900 font-medium">{p.name}</td>
                    <td className="px-4 py-3 font-mono text-xs text-surface-800/60">{p.region}</td>
                    <td className="px-4 py-3 text-surface-800/60">{p.gpuType}</td>
                    <td className="px-4 py-3">
                      <span
                        className={cn(
                          "inline-flex items-center px-2 py-0.5 rounded-full text-[11px] font-medium",
                          p.status === "active"
                            ? "bg-emerald-100 text-emerald-700"
                            : p.status === "idle"
                            ? "bg-amber-100 text-amber-700"
                            : "bg-surface-100 text-surface-800/40"
                        )}
                      >
                        {p.status === "active" ? "● " : "○ "}
                        {p.status}
                      </span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>

        {/* ─── Version History ─── */}
        <section id="history" className="mb-10">
          <h2 className="text-xl font-semibold text-surface-900 mb-4">Version History</h2>
          <div className="relative pl-6 border-l-2 border-surface-200 space-y-6">
            {model.versionHistory.map((v) => (
              <div key={v.version} className="relative">
                <div className="absolute -left-[29px] h-4 w-4 rounded-full bg-brand-500 border-2 border-surface-0" />
                <div className="rounded-xl border border-surface-200 p-4 bg-surface-0">
                  <div className="flex items-center gap-3 mb-1">
                    <span className="font-mono text-sm font-semibold text-surface-900">{v.version}</span>
                    <span className="text-xs text-surface-800/40">{v.date}</span>
                  </div>
                  <p className="text-sm text-surface-800/60">{v.notes}</p>
                </div>
              </div>
            ))}
          </div>
        </section>

        {/* ─── Related Models ─── */}
        <section className="mb-10">
          <h2 className="text-xl font-semibold text-surface-900 mb-4">Related Models</h2>
          <div className="grid sm:grid-cols-2 gap-4">
            {model.relatedModels.map((rm) => (
              <Link
                key={rm.slug}
                href={`/docs/models/${rm.slug}`}
                className="block rounded-xl border border-surface-200 p-4 bg-surface-0 hover:border-brand-300 hover:shadow-sm transition-all group"
              >
                <h3 className="font-medium text-surface-900 group-hover:text-brand-700 transition-colors">
                  {rm.name}
                  <span className="ml-1 text-surface-800/30 group-hover:text-brand-400 transition-colors">&rarr;</span>
                </h3>
                <code className="text-xs font-mono text-surface-800/40">{rm.slug}</code>
              </Link>
            ))}
          </div>
        </section>

        {/* ─── Playground CTA ─── */}
        <section className="rounded-xl bg-brand-50 dark:bg-brand-950/20 border border-brand-200 dark:border-brand-800/30 p-6">
          <h3 className="font-semibold text-brand-900 dark:text-brand-200 mb-2">
            Try {model.name} in the Playground
          </h3>
          <p className="text-sm text-brand-800/60 dark:text-brand-300/60 mb-4">
            Send test requests and see responses in real-time. No API key required for the playground.
          </p>
          <Link
            href={`/docs/playground?model=${model.slug}`}
            className="inline-flex items-center gap-2 px-4 py-2 rounded-lg bg-brand-600 text-white text-sm font-medium hover:bg-brand-700 transition-colors"
          >
            Open Playground
            <span>&rarr;</span>
          </Link>
        </section>
      </div>
    </div>
  );
}
